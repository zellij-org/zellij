//! The `[cli_client]` is used to attach to a running server session
//! and dispatch actions, that are specified through the command line.

mod control_message;

use std::{
    collections::HashMap,
    env, fs,
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use crate::keyboard_parser::KittyKeyboardParser;
use crate::{
    input_handler::from_termwiz,
    os_input_output::{get_client_os_input, ClientOsApi},
    report_changes_in_config_file, spawn_server,
};
use axum::{
    extract::Request,
    extract::{
        ws::{Message, WebSocket},
        Path as AxumPath, Query, State, WebSocketUpgrade,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{any, get, post},
    Json, Router,
};

use axum_extra::extract::cookie::{Cookie, SameSite};

use std::io::{prelude::*, BufRead, BufReader};

use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;

use interprocess::unnamed_pipe::pipe;
use tower_http::cors::CorsLayer;

use daemonize::{self, Outcome};
use nix::sys::stat::{umask, Mode};

use control_message::{
    SetConfigPayload, WebClientToWebServerControlMessage,
    WebClientToWebServerControlMessagePayload, WebServerToWebClientControlMessage,
};
use zellij_utils::{
    cli::CliArgs,
    consts::VERSION,
    data::{ConnectToSession, LayoutInfo, Style, WebSharing},
    envs,
    errors::prelude::*,
    input::{
        actions::Action,
        cast_termwiz_key,
        config::{Config, ConfigError},
        layout::Layout,
        mouse::MouseEvent,
        options::Options,
    },
    ipc::{ClientAttributes, ClientToServerMsg, ExitReason, ServerToClientMsg},
    sessions::{generate_unique_session_name, resurrection_layout, session_exists},
    setup::{find_default_config_dir, get_layout_dir},
    web_authentication_tokens::validate_token,
};

use futures::{prelude::stream::SplitSink, SinkExt, StreamExt};
use include_dir;
use log::info;
use serde::{Deserialize, Serialize};
use serde_json;
use termwiz::input::{InputEvent, InputParser};
use uuid::Uuid;

use tokio::{
    runtime::Runtime,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

const BRACKETED_PASTE_START: [u8; 6] = [27, 91, 50, 48, 48, 126]; // \u{1b}[200~
const BRACKETED_PASTE_END: [u8; 6] = [27, 91, 50, 48, 49, 126]; // \u{1b}[201~

#[derive(Debug, Default, Clone)]
struct ConnectionTable {
    // client_id_to_os_api: HashMap<String, Box<dyn ClientOsApi>>
    client_id_to_os_api: HashMap<String, ClientChannels>, // TODO: rename
}

#[derive(Debug, Clone)]
struct ClientChannels {
    os_api: Box<dyn ClientOsApi>,
    control_channel_tx: Option<tokio::sync::mpsc::UnboundedSender<Message>>,
    terminal_channel_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>, // STDOUT
}

impl ClientChannels {
    pub fn new(os_api: Box<dyn ClientOsApi>) -> Self {
        ClientChannels {
            os_api,
            control_channel_tx: None,
            terminal_channel_tx: None,
        }
    }
    pub fn add_control_tx(
        &mut self,
        control_channel_tx: tokio::sync::mpsc::UnboundedSender<Message>,
    ) {
        self.control_channel_tx = Some(control_channel_tx);
    }
    pub fn add_terminal_tx(
        &mut self,
        terminal_channel_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) {
        self.terminal_channel_tx = Some(terminal_channel_tx);
    }
}

impl ConnectionTable {
    pub fn add_new_client(&mut self, client_id: String, client_os_api: Box<dyn ClientOsApi>) {
        self.client_id_to_os_api
            .insert(client_id, ClientChannels::new(client_os_api));
    }
    pub fn add_client_control_tx(
        &mut self,
        client_id: &str,
        control_channel_tx: tokio::sync::mpsc::UnboundedSender<Message>,
    ) {
        self.client_id_to_os_api
            .get_mut(client_id)
            .map(|c| c.add_control_tx(control_channel_tx));
    }
    pub fn add_client_terminal_tx(
        &mut self,
        client_id: &str,
        terminal_channel_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) {
        self.client_id_to_os_api
            .get_mut(client_id)
            .map(|c| c.add_terminal_tx(terminal_channel_tx));
    }
    pub fn get_client_os_api(&self, client_id: &str) -> Option<&Box<dyn ClientOsApi>> {
        self.client_id_to_os_api.get(client_id).map(|c| &c.os_api)
    }
    pub fn get_client_terminal_tx(
        &self,
        client_id: &str,
    ) -> Option<tokio::sync::mpsc::UnboundedSender<String>> {
        self.client_id_to_os_api
            .get(client_id)
            .and_then(|c| c.terminal_channel_tx.clone())
    }
    pub fn get_client_control_tx(
        &self,
        client_id: &str,
    ) -> Option<tokio::sync::mpsc::UnboundedSender<Message>> {
        self.client_id_to_os_api
            .get(client_id)
            .and_then(|c| c.control_channel_tx.clone())
    }
    pub fn remove_client(&mut self, client_id: &str) {
        self.client_id_to_os_api.remove(client_id);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StdinMessage {
    web_client_id: String,
    stdin: String,
}

fn daemonize_web_server(
    web_server_ip: IpAddr,
    web_server_port: u16,
    web_server_cert: &Option<PathBuf>,
    web_server_key: &Option<PathBuf>,
) -> (Runtime, std::net::TcpListener, Option<RustlsConfig>) {
    // TODO: test this on mac
    let (mut exit_message_tx, exit_message_rx) = pipe().unwrap();
    let (mut exit_status_tx, mut exit_status_rx) = pipe().unwrap();
    let current_umask = umask(Mode::all());
    umask(current_umask);
    let web_server_key = web_server_key.clone();
    let web_server_cert = web_server_cert.clone();
    let daemonization_outcome = daemonize::Daemonize::new()
        .working_directory(std::env::current_dir().unwrap())
        .umask(current_umask.bits() as u32)
        .privileged_action(
            move || -> Result<(Runtime, std::net::TcpListener, Option<RustlsConfig>), String> {
                let runtime = Runtime::new().map_err(|e| e.to_string())?;
                let tls_config = match (web_server_cert, web_server_key) {
                    (Some(web_server_cert), Some(web_server_key)) => {
                        let tls_config = runtime.block_on(async move {
                            RustlsConfig::from_pem_file(
                                PathBuf::from(web_server_cert),
                                PathBuf::from(web_server_key),
                            )
                            .await
                        });
                        let tls_config = match tls_config {
                            Ok(tls_config) => tls_config,
                            Err(e) => {
                                return Err(e.to_string());
                            },
                        };
                        Some(tls_config)
                    },
                    (None, None) => None,
                    _ => {
                        return Err(
                            "Must specify both web_server_cert and web_server_key".to_owned()
                        )
                    },
                };

                let listener = runtime.block_on(async move {
                    std::net::TcpListener::bind(format!("{}:{}", web_server_ip, web_server_port))
                });
                listener
                    .map(|listener| (runtime, listener, tls_config))
                    .map_err(|e| e.to_string())
            },
        )
        .execute();
    match daemonization_outcome {
        Outcome::Parent(Ok(parent)) => {
            if parent.first_child_exit_code == 0 {
                let mut buf = [0; 1];
                // here we wait for the child to send us an exit status, indicating whether the web
                // server was successfully started or not
                match exit_status_rx.read_exact(&mut buf) {
                    Ok(_) => {
                        let exit_status = buf.iter().next().copied().unwrap_or(0) as i32;
                        let mut message = String::new();
                        let mut reader = BufReader::new(exit_message_rx);
                        let _ = reader.read_line(&mut message);
                        if exit_status == 0 {
                            println!("{}", message.trim());
                        } else {
                            eprintln!("{}", message.trim());
                        }
                        std::process::exit(exit_status);
                    },
                    Err(e) => {
                        eprintln!("{}", e);
                        std::process::exit(2);
                    },
                }
            } else {
                std::process::exit(parent.first_child_exit_code);
            }
        },
        Outcome::Child(Ok(child)) => match child.privileged_action_result {
            Ok(listener_and_runtime) => {
                let _ = writeln!(
                    exit_message_tx,
                    "Web Server started on {} port {}",
                    web_server_ip, web_server_port
                );
                let _ = exit_status_tx.write_all(&[0]);
                listener_and_runtime
            },
            Err(e) => {
                let _ = exit_status_tx.write_all(&[2]);
                let _ = writeln!(exit_message_tx, "{}", e);
                std::process::exit(2);
            },
        },
        _ => {
            eprintln!("Failed to start server");
            std::process::exit(2);
        },
    }
}

pub fn start_web_client(config: Config, config_options: Options, run_daemonized: bool) {
    std::panic::set_hook({
        Box::new(move |info| {
            let thread = thread::current();
            let thread = thread.name().unwrap_or("unnamed");
            let msg = match info.payload().downcast_ref::<&'static str>() {
                Some(s) => Some(*s),
                None => info.payload().downcast_ref::<String>().map(|s| &**s),
            }
            .unwrap_or("An unexpected error occurred!");
            log::error!(
                "Thread {} panicked: {}, location {:?}",
                thread,
                msg,
                info.location()
            );
            eprintln!("{}", msg);
            std::process::exit(2);
        })
    });
    let web_server_ip = config_options
        .web_server_ip
        .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    let web_server_port = config_options.web_server_port.unwrap_or_else(|| 8082);
    let web_server_cert = &config.options.web_server_cert;
    let web_server_key = &config.options.web_server_key;
    let has_https_certificate = web_server_cert.is_some() && web_server_key.is_some();

    if let Err(e) = should_use_https(
        web_server_ip,
        has_https_certificate,
        config.options.enforce_https_for_localhost.unwrap_or(false),
    ) {
        eprintln!("{}", e);
        std::process::exit(2);
    };
    let (runtime, listener, tls_config) = if run_daemonized {
        daemonize_web_server(
            web_server_ip,
            web_server_port,
            web_server_cert,
            web_server_key,
        )
    } else {
        let runtime = Runtime::new().unwrap();
        let listener = runtime.block_on(async move {
            std::net::TcpListener::bind(format!("{}:{}", web_server_ip, web_server_port))
        });
        let tls_config = match (web_server_cert, web_server_key) {
            (Some(web_server_cert), Some(web_server_key)) => {
                let tls_config = runtime.block_on(async move {
                    RustlsConfig::from_pem_file(
                        PathBuf::from(web_server_cert),
                        PathBuf::from(web_server_key),
                    )
                    .await
                });
                let tls_config = match tls_config {
                    Ok(tls_config) => tls_config,
                    Err(e) => {
                        eprintln!("{}", e);
                        std::process::exit(2);
                    },
                };
                Some(tls_config)
            },
            (None, None) => None,
            _ => {
                eprintln!("Must specify both web_server_cert and web_server_key");
                std::process::exit(2);
            },
        };

        match listener {
            Ok(listener) => {
                println!(
                    "Web Server started on {} port {}",
                    web_server_ip, web_server_port
                );
                (runtime, listener, tls_config)
            },
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(2);
            },
        }
    };

    runtime.block_on(serve_web_client(
        config,
        config_options,
        listener,
        tls_config,
    ));
}

const WEB_CLIENT_PAGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/",
    "assets/index.html"
));

const ASSETS_DIR: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/assets");

#[derive(Clone)]
struct AppState {
    connection_table: Arc<Mutex<ConnectionTable>>,
    config: Config,
    config_options: Options,
    server_handle: Handle,
}

async fn serve_html(request: Request) -> Html<String> {
    let cookies = parse_cookies(&request);
    let is_authenticated = cookies.get("auth_token").is_some();
    let auth_value = if is_authenticated { "true" } else { "false" };
    let html = Html(WEB_CLIENT_PAGE.replace("IS_AUTHENTICATED", &format!("{}", auth_value)));
    html
}

async fn serve_web_client(
    config: Config,
    config_options: Options,
    listener: std::net::TcpListener,
    rustls_config: Option<RustlsConfig>,
) {
    let connection_table = Arc::new(Mutex::new(ConnectionTable::default()));
    let server_handle = Handle::new();
    let state = AppState {
        connection_table,
        config,
        config_options,
        server_handle: server_handle.clone(),
    };
    let app = Router::new()
        .route("/ws/control", any(ws_handler_control))
        .route("/ws/terminal", any(ws_handler_terminal))
        .route("/ws/terminal/{session}", any(ws_handler_terminal))
        .route("/session", post(create_new_client))
        .route_layer(middleware::from_fn(auth_middleware))
        .route("/", get(serve_html))
        .route("/{session}", get(serve_html))
        .route("/assets/{*path}", get(get_static_asset))
        // TODO: do we want to restrict these somehow?
        .route("/info/version", get(VERSION))
        .route("/command/shutdown", post(send_shutdown_signal))
        .layer(CorsLayer::permissive()) // TODO: configure correctly
        .with_state(state);

    match rustls_config {
        Some(rustls_config) => {
            axum_server::from_tcp_rustls(listener, rustls_config)
                .handle(server_handle)
                .serve(app.into_make_service())
                .await
                .unwrap();
        },
        None => {
            axum_server::from_tcp(listener)
                .handle(server_handle)
                .serve(app.into_make_service())
                .await
                .unwrap();
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShutdownSignal {
    Shutdown,
}

#[derive(Serialize, Debug)]
pub struct SendShutdownSignalResponse {
    status: String,
}

async fn send_shutdown_signal(State(state): State<AppState>) -> Json<SendShutdownSignalResponse> {
    tokio::spawn(async move {
        // wait so that we have time to send a response to this request
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        state.server_handle.shutdown();
    });
    Json(SendShutdownSignalResponse {
        status: "Ok".to_owned(),
    })
}

async fn get_static_asset(AxumPath(path): AxumPath<String>) -> impl IntoResponse {
    let path = path.trim_start_matches('/');

    match ASSETS_DIR.get_file(path) {
        None => (
            [(header::CONTENT_TYPE, "text/html")],
            "Not Found".as_bytes(),
        ),
        Some(file) => {
            let ext = file.path().extension().and_then(|ext| ext.to_str());
            let mime_type = get_mime_type(ext);
            ([(header::CONTENT_TYPE, mime_type)], file.contents())
        },
    }
}

fn get_mime_type(ext: Option<&str>) -> &str {
    match ext {
        None => "text/plain",
        Some(ext) => match ext {
            "html" => "text/html",
            "css" => "text/css",
            "js" => "application/javascript",
            "wasm" => "application/wasm",
            "png" => "image/png",
            "ico" => "image/x-icon",
            "svg" => "image/svg+xml",
            _ => "text/plain",
        },
    }
}

#[derive(Serialize)]
struct CreateClientIdResponse {
    web_client_id: String,
}

/// Create os_input for new client and return the client id
async fn create_new_client(
    State(state): State<AppState>,
) -> Result<Json<CreateClientIdResponse>, (StatusCode, impl IntoResponse)> {
    let web_client_id = String::from(Uuid::new_v4());
    let os_input = get_client_os_input()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(e.to_string())))?;

    state
        .connection_table
        .lock()
        .unwrap()
        .add_new_client(web_client_id.to_owned(), Box::new(os_input));

    Ok(Json(CreateClientIdResponse { web_client_id }))
}

async fn ws_handler_control(
    ws: WebSocketUpgrade,
    path: Option<AxumPath<String>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    log::info!(
        "Control WebSocket connection established with path: {:?}",
        path
    );
    ws.on_upgrade(move |socket| handle_ws_control(socket, state))
}

#[derive(Deserialize)]
struct TerminalParams {
    web_client_id: String,
}

async fn ws_handler_terminal(
    ws: WebSocketUpgrade,
    session_name: Option<AxumPath<String>>,
    Query(params): Query<TerminalParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    log::info!(
        "Terminal WebSocket connection established with session_name: {:?}",
        session_name
    );

    ws.on_upgrade(move |socket| handle_ws_terminal(socket, session_name, params, state))
}

async fn handle_ws_control(socket: WebSocket, state: AppState) {
    info!("New Control WebSocket connection established");

    let config = SetConfigPayload::from((&state.config, &state.config_options));
    let set_config_msg = WebServerToWebClientControlMessage::SetConfig(config);
    info!("Sending initial config to client: {:?}", set_config_msg);

    let (control_socket_tx, mut control_socket_rx) = socket.split();

    let (control_channel_tx, control_channel_rx) = tokio::sync::mpsc::unbounded_channel();
    send_control_messages_to_client(control_channel_rx, control_socket_tx);

    let _ = control_channel_tx.send(Message::Text(
        serde_json::to_string(&set_config_msg).unwrap().into(),
    ));

    let send_message_to_server = |deserialized_msg: WebClientToWebServerControlMessage| {
        let Some(client_connection) = state
            .connection_table
            .lock()
            .unwrap()
            .get_client_os_api(&deserialized_msg.web_client_id)
            .cloned()
        else {
            log::error!("Unknown web_client_id: {}", deserialized_msg.web_client_id);
            return;
        };
        let client_msg = match deserialized_msg.payload {
            WebClientToWebServerControlMessagePayload::TerminalResize(size) => {
                ClientToServerMsg::TerminalResize(size)
            },
        };

        let _ = client_connection.send_to_server(client_msg);
    };

    let mut set_client_control_channel = false;

    // Handle incoming messages
    // while let Some(Ok(msg)) = socket.next().await {
    while let Some(Ok(msg)) = control_socket_rx.next().await {
        match msg {
            Message::Text(msg) => {
                let deserialized_msg: Result<WebClientToWebServerControlMessage, _> =
                    serde_json::from_str(&msg);
                match deserialized_msg {
                    Ok(deserialized_msg) => {
                        if !set_client_control_channel {
                            // on first message, we set the control channel so that
                            // zellij_server_listener has access to it too
                            set_client_control_channel = true;
                            state
                                .connection_table
                                .lock()
                                .unwrap()
                                .add_client_control_tx(
                                    &deserialized_msg.web_client_id,
                                    control_channel_tx.clone(),
                                );
                        }
                        send_message_to_server(deserialized_msg);
                    },
                    Err(e) => {
                        log::error!("Failed to deserialize client msg: {:?}", e);
                    },
                }
            },
            Message::Close(_) => {
                log::info!("Control WebSocket connection closed, exiting");
                return;
            },
            _ => {
                log::error!("Unsupported messagetype : {:?}", msg);
            },
        }
    }
}

async fn handle_ws_terminal(
    socket: WebSocket,
    session_name: Option<AxumPath<String>>,
    params: TerminalParams,
    state: AppState,
) {
    let web_client_id = params.web_client_id;
    let Some(os_input) = state
        .connection_table
        .lock()
        .unwrap()
        .get_client_os_api(&web_client_id)
        .cloned()
    else {
        log::error!("Unknown web_client_id: {}", web_client_id);
        return;
    };

    let (client_channel_tx, mut client_channel_rx) = socket.split();
    info!(
        "New Terminal WebSocket connection established {:?}",
        session_name
    );
    let (stdout_channel_tx, stdout_channel_rx) = tokio::sync::mpsc::unbounded_channel();
    state
        .connection_table
        .lock()
        .unwrap()
        .add_client_terminal_tx(&web_client_id, stdout_channel_tx);

    zellij_server_listener(
        os_input.clone(),
        state.connection_table.clone(),
        session_name.map(|p| p.0),
        state.config.clone(),
        state.config_options.clone(),
        web_client_id.clone(),
    );
    render_to_client(stdout_channel_rx, client_channel_tx);

    // Handle incoming messages (STDIN)

    let explicitly_disable_kitty_keyboard_protocol = state
        .config
        .options
        .support_kitty_keyboard_protocol
        .map(|e| !e)
        .unwrap_or(false);
    let mut mouse_old_event = MouseEvent::new();
    while let Some(Ok(msg)) = client_channel_rx.next().await {
        match msg {
            Message::Text(msg) => {
                let Some(client_connection) = state
                    .connection_table
                    .lock()
                    .unwrap()
                    .get_client_os_api(&web_client_id)
                    .cloned()
                else {
                    log::error!("Unknown web_client_id: {}", web_client_id);
                    continue;
                };
                parse_stdin(
                    msg.as_bytes(),
                    client_connection.clone(),
                    &mut mouse_old_event,
                    explicitly_disable_kitty_keyboard_protocol,
                );
            },
            Message::Close(_) => {
                log::info!("Client WebSocket connection closed, exiting");
                state
                    .connection_table
                    .lock()
                    .unwrap()
                    .remove_client(&web_client_id);
                break;
            },
            _ => {
                log::error!("Unsupported websocket msg type");
            },
        }
    }
    os_input.send_to_server(ClientToServerMsg::ClientExited);
}

fn build_initial_connection(
    session_name: Option<String>,
) -> Result<Option<ConnectToSession>, &'static str> {
    let should_start_with_welcome_screen = session_name.is_none();
    if should_start_with_welcome_screen {
        // if the client connects without a session path in the url, we always open the
        // welcome screen
        let Some(initial_session_name) = session_name.clone().or_else(generate_unique_session_name)
        else {
            return Err("Failed to generate unique session name, bailing.");
        };
        Ok(Some(ConnectToSession {
            name: Some(initial_session_name.clone()),
            layout: Some(LayoutInfo::BuiltIn("welcome".to_owned())),
            ..Default::default()
        }))
    } else if let Some(session_name) = session_name {
        Ok(Some(ConnectToSession {
            name: Some(session_name.clone()),
            ..Default::default()
        }))
    } else {
        Ok(None)
    }
}

// TODO: move elsewhere
#[derive(Debug)]
struct ClientConnectionBus {
    connection_table: Arc<Mutex<ConnectionTable>>,
    stdout_channel_tx: Option<UnboundedSender<String>>,
    control_channel_tx: Option<UnboundedSender<Message>>,
    web_client_id: String,
}

impl ClientConnectionBus {
    pub fn new(web_client_id: &str, connection_table: &Arc<Mutex<ConnectionTable>>) -> Self {
        let connection_table = connection_table.clone();
        let web_client_id = web_client_id.to_owned();
        let (stdout_channel_tx, control_channel_tx) = {
            let connection_table = connection_table.lock().unwrap();
            (
                connection_table.get_client_terminal_tx(&web_client_id),
                connection_table.get_client_control_tx(&web_client_id),
            )
        };
        ClientConnectionBus {
            connection_table,
            stdout_channel_tx,
            control_channel_tx,
            web_client_id,
        }
    }
    pub fn send_stdout(&mut self, stdout: String) {
        match self.stdout_channel_tx.as_ref() {
            Some(stdout_channel_tx) => {
                let _ = stdout_channel_tx.send(stdout);
            },
            None => {
                self.get_stdout_channel_tx(); // retry to circumvent races
                if let Some(stdout_channel_tx) = self.stdout_channel_tx.as_ref() {
                    let _ = stdout_channel_tx.send(stdout);
                } else {
                    // if at this point we still don't have an STDOUT channel
                    // likely the client disconnected and/or the state is corrupt
                    log::error!("Failed to send STDOUT message to client");
                }
            },
        }
    }
    pub fn send_control(&mut self, message: WebServerToWebClientControlMessage) {
        let message = Message::Text(serde_json::to_string(&message).unwrap().into());
        match self.control_channel_tx.as_ref() {
            Some(control_channel_tx) => {
                let _ = control_channel_tx.send(message);
            },
            None => {
                self.get_control_channel_tx(); // retry to circumvent races
                if let Some(control_channel_tx) = self.control_channel_tx.as_ref() {
                    let _ = control_channel_tx.send(message);
                } else {
                    // if at this point we still don't have a control channel
                    // likely the client disconnected and/or the state is corrupt
                    log::error!("Failed to send control message to client");
                }
            },
        }
    }
    fn get_control_channel_tx(&mut self) {
        if let Some(control_channel_tx) = self
            .connection_table
            .lock()
            .unwrap()
            .get_client_control_tx(&self.web_client_id)
        {
            self.control_channel_tx = Some(control_channel_tx);
        }
    }
    fn get_stdout_channel_tx(&mut self) {
        if let Some(stdout_channel_tx) = self
            .connection_table
            .lock()
            .unwrap()
            .get_client_terminal_tx(&self.web_client_id)
        {
            self.stdout_channel_tx = Some(stdout_channel_tx);
        }
    }
}

fn zellij_server_listener(
    os_input: Box<dyn ClientOsApi>,
    connection_table: Arc<Mutex<ConnectionTable>>,
    session_name: Option<String>,
    config: Config,
    config_options: Options,
    web_client_id: String,
) {
    let _server_listener_thread = std::thread::Builder::new()
        .name("server_listener".to_string())
        .spawn({
            move || {
                let mut client_connection_bus = ClientConnectionBus::new(&web_client_id, &connection_table);
                let mut reconnect_to_session = match build_initial_connection(session_name) {
                    Ok(initial_session_connection) => initial_session_connection,
                    Err(e) => {
                        log::error!("{}", e);
                        return;
                    },
                };
                'reconnect_loop: loop {
                    let reconnect_info = reconnect_to_session.take();
                    let path = {
                        let Some(session_name) = reconnect_info
                            .as_ref()
                            .and_then(|r| r.name.clone())
                            .or_else(generate_unique_session_name)
                        else {
                            log::error!("Failed to generate unique session name, bailing.");
                            return;
                        };
                        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
                        sock_dir.push(session_name.clone());
                        sock_dir.to_str().unwrap().to_owned()
                    };

                    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
                    let mut sent_init_messages = false;

                    let palette = config
                        .theme_config(config_options.theme.as_ref())
                        .unwrap_or_else(|| os_input.load_palette().into());
                    let client_attributes = ClientAttributes {
                        size: full_screen_ws,
                        style: Style {
                            colors: palette,
                            rounded_corners: config.ui.pane_frames.rounded_corners,
                            hide_session_name: config.ui.pane_frames.hide_session_name,
                        },
                    };

                    let session_name = PathBuf::from(path.clone())
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned();

                    let is_web_client = true;
                    let (first_message, zellij_ipc_pipe) = spawn_session_if_needed(
                        &session_name,
                        path,
                        client_attributes,
                        &config,
                        &config_options,
                        is_web_client,
                        os_input.clone(),
                        reconnect_info.as_ref().and_then(|r| r.layout.clone()),
                    );

                    os_input.connect_to_server(&zellij_ipc_pipe);
                    os_input.send_to_server(first_message);

                    // we keep the _config_file_watcher here so that it's dropped on the next round
                    // of the reconnect loop
                    let _config_file_watcher =
                        // we send an empty CliArgs because it's not possible to configure the web
                        // server through the cli
                        report_changes_in_config_file(&CliArgs::default(), &os_input);

                    // we do this so that the browser's URL will change to the new session name
                    client_connection_bus.send_control(WebServerToWebClientControlMessage::SwitchedSession { new_session_name: session_name.clone() });

                    loop {
                        match os_input.recv_from_server() {
                            Some((ServerToClientMsg::UnblockInputThread, _)) => {
                                // no-op - no longer relevant
                            },
                            Some((ServerToClientMsg::Exit(exit_reason), _)) => {
                                match exit_reason {
                                    ExitReason::WebClientsForbidden => {
                                        client_connection_bus.send_stdout(format!("\u{1b}[2J\n Web Clients are not allowed to attach to this session."));
                                    }
                                    ExitReason::Error(e) => {
                                        // TODO: since at this point copy/paste won't work as usual
                                        // (since there is no zellij session) we need to count on
                                        // xterm.js's copy/paste... for linux it's ctrl-insert -
                                        // what is it for mac?
                                        //
                                        // once we know, we should probably display this info here
                                        // too
                                        let goto_start_of_last_line = format!("\u{1b}[{};{}H", 1, 1);
                                        let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
                                        let disable_mouse = "\u{1b}[?1006l\u{1b}[?1015l\u{1b}[?1003l\u{1b}[?1002l\u{1b}[?1000l";
                                        let error = format!(
                                            "{}{}\n{}{}\n",
                                            disable_mouse, clear_client_terminal_attributes, goto_start_of_last_line, e.to_string().replace("\n", "\n\r")
                                        );
                                        client_connection_bus.send_stdout(format!("\u{1b}[2J\n{}", error));
                                    },
                                    _ => {},
                                }
                                os_input.send_to_server(ClientToServerMsg::ClientExited);
                                break;
                            },
                            Some((ServerToClientMsg::Render(bytes), _)) => {
                                if !sent_init_messages {
                                    // we only send these once we've rendered the first byte to
                                    // make sure the server is ready before the client receives any
                                    // messages on the terminal channel
                                    for message in terminal_init_messages() {
                                        client_connection_bus.send_stdout(message.to_owned())
                                    }
                                    sent_init_messages = true;
                                }
                                client_connection_bus.send_stdout(bytes);
                            },
                            Some((ServerToClientMsg::SwitchSession(connect_to_session), _)) => {
                                reconnect_to_session = Some(connect_to_session);
                                continue 'reconnect_loop;
                            },
                            Some((ServerToClientMsg::WriteConfigToDisk { config }, _)) => {
                                // TODO: get config path from actual CLI args and differentiate
                                // between sessions (this is also a bug in the CLI client)
                                match Config::write_config_to_disk(config, &CliArgs::default()) {
                                    Ok(written_config) => {
                                        let _ = os_input.send_to_server(
                                            ClientToServerMsg::ConfigWrittenToDisk(written_config),
                                        );
                                    },
                                    Err(e) => {
                                        let error_path = e
                                            .as_ref()
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_else(String::new);
                                        log::error!(
                                            "Failed to write config to disk: {}",
                                            error_path
                                        );
                                        let _ = os_input.send_to_server(
                                            ClientToServerMsg::FailedToWriteConfigToDisk(e),
                                        );
                                    },
                                }
                            },
                            Some((ServerToClientMsg::QueryTerminalSize, _)) => {
                                client_connection_bus.send_control(WebServerToWebClientControlMessage::QueryTerminalSize);
                            }
                            Some((ServerToClientMsg::Log(lines), _)) => {
                                client_connection_bus.send_control(WebServerToWebClientControlMessage::Log{lines});
                            }
                            Some((ServerToClientMsg::LogError(lines), _)) => {
                                client_connection_bus.send_control(WebServerToWebClientControlMessage::LogError{lines});
                            }
                            _ => {},
                        }
                    }
                    if reconnect_to_session.is_none() {
                        break;
                    }
                }
            }
        });
}

fn render_to_client(
    mut stdout_channel_rx: UnboundedReceiver<String>,
    mut client_channel_tx: SplitSink<WebSocket, Message>,
) {
    tokio::spawn(async move {
        while let Some(rendered_bytes) = stdout_channel_rx.recv().await {
            if client_channel_tx
                .send(Message::Text(rendered_bytes.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });
}

// TODO: make sure this and the above function exit when the client is disconnected
fn send_control_messages_to_client(
    mut control_channel_rx: UnboundedReceiver<Message>,
    mut socket_channel_tx: SplitSink<WebSocket, Message>,
) {
    tokio::spawn(async move {
        while let Some(message) = control_channel_rx.recv().await {
            if socket_channel_tx.send(message).await.is_err() {
                break;
            }
        }
    });
}

fn terminal_init_messages() -> Vec<&'static str> {
    let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
    let enter_alternate_screen = "\u{1b}[?1049h";
    let bracketed_paste = "\u{1b}[?2004h";
    let enter_kitty_keyboard_mode = "\u{1b}[>1u";
    let enable_mouse_mode = "\u{1b}[?1000h\u{1b}[?1002h\u{1b}[?1015h\u{1b}[?1006h";
    vec![
        clear_client_terminal_attributes,
        enter_alternate_screen,
        bracketed_paste,
        enter_kitty_keyboard_mode,
        enable_mouse_mode,
    ]
}

fn parse_stdin(
    buf: &[u8],
    os_input: Box<dyn ClientOsApi>,
    mouse_old_event: &mut MouseEvent,
    explicitly_disable_kitty_keyboard_protocol: bool,
) {
    if !explicitly_disable_kitty_keyboard_protocol {
        // first we try to parse with the KittyKeyboardParser
        // if we fail, we try to parse normally
        match KittyKeyboardParser::new().parse(&buf) {
            Some(key_with_modifier) => {
                os_input.send_to_server(ClientToServerMsg::Key(
                    key_with_modifier.clone(),
                    buf.to_vec(),
                    true,
                ));
                return;
            },
            None => {},
        }
    }

    let mut input_parser = InputParser::new();
    let maybe_more = false; // read_from_stdin should (hopefully) always empty the STDIN buffer completely
    let mut events = vec![];
    input_parser.parse(
        &buf,
        |input_event: InputEvent| {
            events.push(input_event);
        },
        maybe_more,
    );

    for (i, input_event) in events.into_iter().enumerate() {
        match input_event {
            InputEvent::Key(key_event) => {
                let key = cast_termwiz_key(key_event.clone(), &buf, None);
                os_input.send_to_server(ClientToServerMsg::Key(key.clone(), buf.to_vec(), false));
            },
            InputEvent::Mouse(mouse_event) => {
                let mouse_event = from_termwiz(mouse_old_event, mouse_event);
                let action = Action::MouseEvent(mouse_event);
                os_input.send_to_server(ClientToServerMsg::Action(action, None, None));
            },
            InputEvent::Paste(pasted_text) => {
                os_input.send_to_server(ClientToServerMsg::Action(
                    Action::Write(None, BRACKETED_PASTE_START.to_vec(), false),
                    None,
                    None,
                ));
                os_input.send_to_server(ClientToServerMsg::Action(
                    Action::Write(None, pasted_text.as_bytes().to_vec(), false),
                    None,
                    None,
                ));
                os_input.send_to_server(ClientToServerMsg::Action(
                    Action::Write(None, BRACKETED_PASTE_END.to_vec(), false),
                    None,
                    None,
                ));
            },
            _ => {
                log::error!("Unsupported event: {:#?}", input_event);
            },
        }
    }
}

fn layout_for_new_session(
    config: &Config,
    requested_layout: Option<LayoutInfo>,
) -> Result<(Layout, Config), ConfigError> {
    let layout_dir = config
        .options
        .layout_dir
        .clone()
        .or_else(|| get_layout_dir(find_default_config_dir()));
    match requested_layout {
        Some(LayoutInfo::BuiltIn(layout_name)) => Layout::from_default_assets(
            &PathBuf::from(layout_name),
            layout_dir.clone(),
            config.clone(),
        ),
        Some(LayoutInfo::File(layout_name)) => Layout::from_path_or_default(
            Some(&PathBuf::from(layout_name)),
            layout_dir.clone(),
            config.clone(),
        ),
        Some(LayoutInfo::Url(url)) => Layout::from_url(&url, config.clone()),
        Some(LayoutInfo::Stringified(stringified_layout)) => {
            Layout::from_stringified_layout(&stringified_layout, config.clone())
        },
        None => Layout::from_default_assets(
            &PathBuf::from("default"),
            layout_dir.clone(),
            config.clone(),
        ),
    }
}

fn spawn_session_if_needed(
    session_name: &str,
    path: String,
    client_attributes: ClientAttributes,
    config: &Config,
    config_options: &Options,
    is_web_client: bool,
    os_input: Box<dyn ClientOsApi>,
    requested_layout: Option<LayoutInfo>,
) -> (ClientToServerMsg, PathBuf) {
    if session_exists(&session_name).unwrap_or(false) {
        ipc_pipe_and_first_message_for_existing_session(
            path,
            client_attributes,
            &config,
            &config_options,
            is_web_client,
        )
    } else {
        let force_run_commands = false; // this can only be true through the CLI, so not relevant
                                        // here
        let resurrection_layout =
            resurrection_layout(&session_name).map(|mut resurrection_layout| {
                if force_run_commands {
                    resurrection_layout.recursively_add_start_suspended(Some(false));
                }
                resurrection_layout
            });

        match resurrection_layout {
            Some(resurrection_layout) => spawn_new_session(
                &session_name,
                is_web_client,
                os_input.clone(),
                config.clone(),
                config_options.clone(),
                Some(resurrection_layout),
                client_attributes,
            ),
            None => {
                let new_session_layout = layout_for_new_session(&config, requested_layout);

                spawn_new_session(
                    &session_name,
                    is_web_client,
                    os_input.clone(),
                    config.clone(),
                    config_options.clone(),
                    new_session_layout.ok().map(|(l, c)| l), // TODO: handle config
                    client_attributes,
                )
            },
        }
    }
}

fn spawn_new_session(
    name: &str,
    is_web_client: bool,
    mut os_input: Box<dyn ClientOsApi>,
    mut config: Config,
    config_opts: Options,
    layout: Option<Layout>,
    client_attributes: ClientAttributes,
) -> (ClientToServerMsg, PathBuf) {
    let debug = false;
    envs::set_session_name(name.to_owned());
    os_input.update_session_name(name.to_owned());

    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(name);
        sock_dir
    };

    spawn_server(&*zellij_ipc_pipe, debug).unwrap();

    let successfully_written_config = Config::write_config_to_disk_if_it_does_not_exist(
        config.to_string(true),
        &Default::default(),
    );
    // if we successfully wrote the config to disk, it means two things:
    // 1. It did not exist beforehand
    // 2. The config folder is writeable
    //
    // If these two are true, we should launch the setup wizard, if even one of them is
    // false, we should never launch it.
    let should_launch_setup_wizard = successfully_written_config;
    let cli_args = CliArgs::default(); // TODO: what do we do about this and the above setup
                                       // wizard?
    config.options.web_server = Some(true);
    config.options.web_sharing = Some(WebSharing::On);
    let is_web_client = true;

    (
        ClientToServerMsg::NewClient(
            client_attributes,
            Box::new(cli_args),
            Box::new(config.clone()),
            Box::new(config_opts.clone()),
            Box::new(layout.unwrap()),
            Box::new(config.plugins.clone()),
            should_launch_setup_wizard,
            is_web_client,
        ),
        zellij_ipc_pipe,
    )
}

fn ipc_pipe_and_first_message_for_existing_session(
    session_name: String,
    client_attributes: ClientAttributes,
    config: &Config,
    config_options: &Options,
    is_web_client: bool,
) -> (ClientToServerMsg, PathBuf) {
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };
    let first_message = ClientToServerMsg::AttachClient(
        client_attributes,
        config.clone(),
        config_options.clone(),
        None,
        None,
        is_web_client,
    );
    (first_message, zellij_ipc_pipe)
}

fn should_use_https(
    ip: IpAddr,
    has_certificate: bool,
    enforce_https_for_localhost: bool,
) -> Result<bool, String> {
    let is_loopback = match ip {
        IpAddr::V4(ipv4) => ipv4.is_loopback(),
        IpAddr::V6(ipv6) => ipv6.is_loopback(),
    };

    if is_loopback && !enforce_https_for_localhost {
        // if we have a certificate -> https, otherwise -> http
        Ok(has_certificate)
    } else if is_loopback {
        Err(format!("Cannot bind without an SSL certificate."))
    } else if has_certificate {
        // if this is not the loopback and we have a vertificate -> https
        Ok(true)
    } else {
        Err(format!(
            "Cannot bind to non-loopback IP: {} without an SSL certificate.",
            ip
        ))
    }
}

fn parse_cookies(request: &Request) -> HashMap<String, String> {
    let mut cookies = HashMap::new();

    if let Some(cookie_header) = request.headers().get("cookie") {
        if let Ok(cookie_str) = cookie_header.to_str() {
            for cookie_part in cookie_str.split(';') {
                if let Ok(cookie) = Cookie::parse(cookie_part.trim()) {
                    cookies.insert(cookie.name().to_string(), cookie.value().to_string());
                }
            }
        }
    }

    cookies
}

async fn auth_middleware(
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    log::info!("auth_middleware");
    let cookies = parse_cookies(&request);
    let header_token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .or_else(|| params.get("token").map(|s| s.as_str()));
    let cookie_token = cookies.get("auth_token").cloned();
    let (token, should_set_cookie) = match (header_token, &cookie_token) {
        (Some(header_tok), _) => {
            // New login with header token - check for remember_me preference
            let remember_me = headers
                .get("x-remember-me")
                .and_then(|h| h.to_str().ok())
                .map(|s| s == "true")
                .unwrap_or(false);
            (header_tok.to_owned(), remember_me)
        },
        (None, Some(cookie_tok)) => {
            // Existing session with cookie
            (cookie_tok.to_owned(), false)
        },
        (None, None) => return Err(StatusCode::UNAUTHORIZED),
    };
    if !token_is_valid(&token) {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let mut response = next.run(request).await;

    if should_set_cookie {
        let cookie = Cookie::build(("auth_token", token))
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Strict)
            .max_age(time::Duration::hours(24 * 30)) // 30 days = 720 hours
            .path("/")
            .build();
        if let Ok(cookie_header) = HeaderValue::from_str(&cookie.to_string()) {
            response.headers_mut().insert("set-cookie", cookie_header);
        }
    }

    Ok(response)
}

fn token_is_valid(token: &str) -> bool {
    match validate_token(token) {
        Ok(is_valid) => is_valid,
        Err(e) => {
            log::error!("Failed to validate token: {}", e);
            false
        },
    }
}

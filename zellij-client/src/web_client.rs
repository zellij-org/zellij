//! The `[cli_client]` is used to attach to a running server session
//! and dispatch actions, that are specified through the command line.
use std::collections::{BTreeMap, HashMap};
use std::io::BufRead;
use std::path::Path;
use std::process;
use std::{fs, path::PathBuf};

use crate::os_input_output::get_client_os_input;
use crate::os_input_output::ClientOsApi;
use axum::extract::Path as AxumPath;
use axum::http::header;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use zellij_utils::{
    data::Style,
    errors::prelude::*,
    include_dir,
    input::actions::Action,
    input::cast_termwiz_key,
    input::config::{Config, ConfigError},
    input::options::Options,
    ipc::{
        ClientAttributes, ClientToServerMsg, ExitReason, IpcSenderWithContext, ServerToClientMsg,
    },
    pane_size::{Size, SizeInPixels},
    serde::{Deserialize, Serialize},
    serde_json,
    uuid::Uuid,
};

use std::sync::{Arc, Mutex};

use futures::{future, prelude::stream::SplitSink, SinkExt};
use futures::{join, StreamExt};
use log::info;
use std::env;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::{task, time};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{
    accept_async, accept_hdr_async,
    tungstenite::http::{Request, Response},
    WebSocketStream,
};

// DEV INSTRUCTIONS:
// * to run this:
//      - cargo x run --singlepass
//      - (inside the session): target/dev-opt/zellij --web $ZELLIJ_SESSION_NAME

// TODO:
// - handle switching sessions
// - place control and terminal channels on different endpoints rather than different ports
// - use http headers to communicate client_id rather than the payload so that we can get rid of
// one serialization level
// - look into flow control

type ConnectionTable = Arc<Mutex<HashMap<String, Arc<Mutex<Box<dyn ClientOsApi>>>>>>; // TODO: no

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RenderedBytes {
    web_client_id: String,
    bytes: String,
}

impl RenderedBytes {
    pub fn new(bytes: String, web_client_id: &str) -> Self {
        RenderedBytes {
            web_client_id: web_client_id.to_owned(),
            bytes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ControlMessage {
    web_client_id: String,
    message: ClientToServerMsg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StdinMessage {
    web_client_id: String,
    stdin: String,
}

pub fn start_web_client(session_name: &str, config: Config, config_options: Options) {
    log::info!("WebSocket server started and listening on port 8080 and 8081");

    let connection_table: HashMap<String, Arc<Mutex<Box<dyn ClientOsApi>>>> = HashMap::new();
    let connection_table = Arc::new(Mutex::new(connection_table));

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        join!(
            terminal_server(
                session_name,
                config.clone(),
                config_options.clone(),
                connection_table.clone(),
            ),
            handle_server_control(session_name, config, config_options, connection_table),
            serve_web_client(),
        )
    });
}

async fn terminal_server(
    session_name: &str,
    config: Config,
    config_options: Options,
    connection_table: ConnectionTable,
) {
    let addr = "127.0.0.1:8080";
    let listener = TcpListener::bind(addr).await.unwrap();
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(start_terminal_connection(
            stream,
            session_name.to_owned(),
            config.clone(),
            config_options.clone(),
            connection_table.clone(),
        ));
    }
}

async fn handle_server_control(
    session_name: &str,
    config: Config,
    config_options: Options,
    connection_table: ConnectionTable,
) {
    let addr = "127.0.0.1:8081";
    let listener = TcpListener::bind(addr).await.unwrap();
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(handle_client_control(
            stream,
            session_name.to_owned(),
            config.clone(),
            config_options.clone(),
            connection_table.clone(),
        ));
    }
}

const WEB_CLIENT_PAGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/",
    "assets/index.html"
));

const ASSETS_DIR: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/assets");

async fn serve_web_client() {
    let addr = "127.0.0.1:8082";

    async fn page_html() -> Html<&'static str> {
        Html(WEB_CLIENT_PAGE)
    }

    let app = Router::new()
        .route("/", get(page_html))
        .route("/assets/*path", get(get_static_asset));

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    log::info!("Started listener on 8082");

    axum::serve(listener, app).await.unwrap();
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

async fn start_terminal_connection(
    stream: tokio::net::TcpStream,
    mut session_name: String,
    config: Config,
    config_options: Options,
    connection_table: ConnectionTable,
) {
    let web_client_id = String::from(Uuid::new_v4());
    let os_input = get_client_os_input().unwrap(); // TODO: log error and quit

    connection_table.lock().unwrap().insert(
        web_client_id.to_owned(),
        Arc::new(Mutex::new(Box::new(os_input.clone()))),
    );

    let callback = |req: &Request<_>, response: Response<_>| {
        let mut request_uri = req.uri().to_string();
        if request_uri.starts_with('/') {
            request_uri.remove(0);
            if !request_uri.is_empty() {
                session_name = request_uri;
            }
        }
        Ok(response)
    };

    let ws_stream = accept_hdr_async(stream, callback).await.unwrap();
    let (client_channel_tx, mut client_channel_rx) = ws_stream.split();
    info!("New WebSocket connection established");
    let (stdout_channel_tx, stdout_channel_rx) = tokio::sync::mpsc::unbounded_channel();

    zellij_server_listener(
        Box::new(os_input.clone()),
        stdout_channel_tx,
        &session_name,
        config.clone(),
        config_options.clone(),
    );
    render_to_client(stdout_channel_rx, web_client_id, client_channel_tx);

    // Handle incoming messages (STDIN)
    while let Some(Ok(msg)) = client_channel_rx.next().await {
        match msg {
            Message::Text(msg) => {
                let deserialized_msg: Result<StdinMessage, _> = serde_json::from_str(&msg);
                match deserialized_msg {
                    Ok(deserialized_msg) => {
                        let Some(client_connection) = connection_table
                            .lock()
                            .unwrap()
                            .get(&deserialized_msg.web_client_id)
                            .cloned()
                        else {
                            log::error!(
                                "Unknown web_client_id: {}",
                                deserialized_msg.web_client_id
                            );
                            continue;
                        };
                        parse_stdin(
                            deserialized_msg.stdin.as_bytes(),
                            client_connection.lock().unwrap().clone(),
                        );
                    },
                    Err(e) => {
                        log::error!("Failed to deserialize stdin: {}", e);
                    },
                }
            },
            _ => {
                log::error!("Unsupported websocket msg type");
            },
        }
    }
    os_input.send_to_server(ClientToServerMsg::ClientExited);
}

async fn handle_client_control(
    stream: tokio::net::TcpStream,
    session_name: String,
    config: Config,
    config_options: Options,
    connection_table: ConnectionTable,
) {
    let os_input = get_client_os_input().unwrap(); // TODO: log error and quit
    let ws_stream = accept_async(stream).await.unwrap();
    let (mut write, mut read) = ws_stream.split();
    info!("New Control WebSocket connection established");

    // Handle incoming messages
    while let Some(Ok(msg)) = read.next().await {
        match msg {
            Message::Text(msg) => {
                let deserialized_msg: Result<ControlMessage, _> = serde_json::from_str(&msg);
                match deserialized_msg {
                    Ok(deserialized_msg) => {
                        let Some(client_connection) = connection_table
                            .lock()
                            .unwrap()
                            .get(&deserialized_msg.web_client_id)
                            .cloned()
                        else {
                            log::error!(
                                "Unknown web_client_id: {}",
                                deserialized_msg.web_client_id
                            );
                            continue;
                        };
                        client_connection
                            .lock()
                            .unwrap()
                            .send_to_server(deserialized_msg.message);
                    },
                    Err(e) => {
                        log::error!("Failed to deserialize client msg: {:?}", e);
                    },
                }
            },
            _ => {
                log::error!("Unsupported messagetype : {:?}", msg);
            },
        }
    }
}

fn zellij_server_listener(
    os_input: Box<dyn ClientOsApi>,
    stdout_channel_tx: tokio::sync::mpsc::UnboundedSender<String>,
    session_name: &str,
    config: Config,
    config_options: Options,
) {
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);

    let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
    let enter_alternate_screen = "\u{1b}[?1049h";
    let bracketed_paste = "\u{1b}[?2004h";
    let enter_kitty_keyboard_mode = "\u{1b}[>1u";
    let _ = stdout_channel_tx.send(clear_client_terminal_attributes.to_owned());
    let _ = stdout_channel_tx.send(enter_alternate_screen.to_owned());
    let _ = stdout_channel_tx.send(bracketed_paste.to_owned());
    let _ = stdout_channel_tx.send(enter_kitty_keyboard_mode.to_owned());

    let palette = config
        .theme_config(config_options.theme.as_ref())
        .unwrap_or_else(|| os_input.load_palette());
    let client_attributes = ClientAttributes {
        size: full_screen_ws,
        style: Style {
            colors: palette,
            rounded_corners: config.ui.pane_frames.rounded_corners,
            hide_session_name: config.ui.pane_frames.hide_session_name,
        },
    };

    let is_web_client = true;
    let first_message = ClientToServerMsg::AttachClient(
        client_attributes,
        config.clone(),
        config_options.clone(),
        None,
        None,
        is_web_client,
    );

    os_input.connect_to_server(&*zellij_ipc_pipe);
    os_input.send_to_server(first_message);

    let _server_listener_thread = std::thread::Builder::new()
        .name("server_listener".to_string())
        .spawn({
            move || {
                loop {
                    match os_input.recv_from_server() {
                        //             Some((ServerToClientMsg::UnblockInputThread, _)) => {
                        //                 break;
                        //             },
                        //             Some((ServerToClientMsg::Log(log_lines), _)) => {
                        //                 log_lines.iter().for_each(|line| println!("{line}"));
                        //                 break;
                        //             },
                        //             Some((ServerToClientMsg::LogError(log_lines), _)) => {
                        //                 log_lines.iter().for_each(|line| eprintln!("{line}"));
                        //                 process::exit(2);
                        //             },
                        Some((ServerToClientMsg::Exit(exit_reason), _)) => {
                            match exit_reason {
                                ExitReason::Error(e) => {
                                    eprintln!("{}", e);
                                    // process::exit(2);
                                },
                                _ => {},
                            }
                            os_input.send_to_server(ClientToServerMsg::ClientExited);
                            break;
                        },
                        Some((ServerToClientMsg::Render(bytes), _)) => {
                            let _ = stdout_channel_tx.send(bytes);
                        },
                        _ => {},
                    }
                }
            }
        });
}

fn render_to_client(
    mut stdout_channel_rx: UnboundedReceiver<String>,
    web_client_id: String,
    mut client_channel_tx: SplitSink<WebSocketStream<TcpStream>, Message>,
) {
    tokio::spawn(async move {
        loop {
            if let Some(rendered_bytes) = stdout_channel_rx.recv().await {
                match serde_json::to_string(&RenderedBytes::new(rendered_bytes, &web_client_id)) {
                    Ok(rendered_bytes) => {
                        if client_channel_tx
                            .send(Message::Text(rendered_bytes))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    },
                    Err(e) => {
                        log::error!("Failed to serialize rendered bytes: {:?}", e);
                    },
                }
            }
        }
    });
}

use zellij_utils::termwiz::input::{InputEvent, InputParser, MouseButtons};
fn is_mouse_press_or_hold(input_event: &InputEvent) -> bool {
    if let InputEvent::Mouse(mouse_event) = input_event {
        if mouse_event.mouse_buttons.contains(MouseButtons::LEFT)
            || mouse_event.mouse_buttons.contains(MouseButtons::RIGHT)
        {
            return true;
        }
    }
    false
}
fn parse_stdin(buf: &[u8], os_input: Box<dyn ClientOsApi>) {
    let mut holding_mouse = false;
    let mut input_parser = InputParser::new();
    // let mut current_buffer = vec![];
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
        match &input_event {
            InputEvent::Key(key_event) => {
                let key = cast_termwiz_key(
                    key_event.clone(),
                    &buf,
                    None, // TODO: config, for ctrl-j etc.
                );
                os_input.send_to_server(ClientToServerMsg::Key(
                    key.clone(),
                    buf.to_vec(),
                    false, // TODO: kitty keyboard support?
                ));
            },
            _ => {
                log::error!("Unsupported event: {:#?}", input_event);
            },
        }
    }
}

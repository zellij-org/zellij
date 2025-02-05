//! The `[cli_client]` is used to attach to a running server session
//! and dispatch actions, that are specified through the command line.
use std::{
    collections::HashMap,
    env, fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::keyboard_parser::KittyKeyboardParser;
use crate::{
    report_changes_in_config_file,
    input_handler::from_termwiz,
    os_input_output::{get_client_os_input, ClientOsApi},
    spawn_server,
};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path as AxumPath, State, WebSocketUpgrade,
    },
    http::header,
    response::{Html, IntoResponse},
    routing::{any, get},
    Router,
};
use zellij_utils::{
    cli::CliArgs,
    data::{ConnectToSession, LayoutInfo, Style},
    envs,
    errors::prelude::*,
    include_dir,
    input::layout::Layout,
    input::{
        actions::Action, cast_termwiz_key, config::Config, mouse::MouseEvent, options::Options,
    },
    ipc::{ClientAttributes, ClientToServerMsg, ExitReason, ServerToClientMsg},
    serde::{Deserialize, Serialize},
    serde_json,
    sessions::{resurrection_layout, session_exists},
    setup::{find_default_config_dir, get_layout_dir},
    termwiz::input::{InputEvent, InputParser},
    uuid::Uuid,
};

use futures::{prelude::stream::SplitSink, SinkExt, StreamExt};
use log::info;

use tokio::{runtime::Runtime, sync::mpsc::UnboundedReceiver};

const BRACKETED_PASTE_START: [u8; 6] = [27, 91, 50, 48, 48, 126]; // \u{1b}[200~
const BRACKETED_PASTE_END: [u8; 6] = [27, 91, 50, 48, 49, 126]; // \u{1b}[201~

// DEV INSTRUCTIONS:
// * to run this:
//      - ps ax | grep web | grep zellij | grep target | awk \'{print $1}\' | xargs kill -9 # this
//      kills the webserver from previous runs
//      - cargo x run --singlepass -- options --enable-web-server true
//      - point the browser at localhost:8082

// TODO:
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

pub fn start_web_client(ipc_path: &str, config: Config, config_options: Options) {
    log::info!(
        "WebSocket server started and listening on port 8082, with ipc_path {}",
        ipc_path
    );

    let connection_table: ConnectionTable = Arc::new(Mutex::new(HashMap::new()));

    let rt = Runtime::new().unwrap();
    rt.block_on(serve_web_client(
        ipc_path,
        config,
        config_options,
        connection_table,
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
    connection_table: ConnectionTable,
    ipc_path: String,
    config: Config,
    config_options: Options,
}

async fn serve_web_client(
    ipc_path: &str,
    config: Config,
    config_options: Options,
    connection_table: ConnectionTable,
) {
    let addr = "127.0.0.1:8082";

    let state = AppState {
        connection_table,
        ipc_path: ipc_path.to_owned(),
        config,
        config_options,
    };

    async fn page_html(path: Option<AxumPath<String>>) -> Html<&'static str> {
        log::info!("Serving web client html with path: {:?}", path);
        Html(WEB_CLIENT_PAGE)
    }

    let app = Router::new()
        .route("/", get(page_html))
        .route("/{session}", get(page_html))
        .route("/assets/{*path}", get(get_static_asset))
        .route("/ws/control", any(ws_handler_control))
        .route("/ws/terminal", any(ws_handler_terminal))
        .route("/ws/terminal/{session}", any(ws_handler_terminal))
        .with_state(state);

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

async fn ws_handler_terminal(
    ws: WebSocketUpgrade,
    session_name: Option<AxumPath<String>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    log::info!(
        "Terminal WebSocket connection established with session_name: {:?}",
        session_name
    );

    ws.on_upgrade(move |socket| handle_ws_terminal(socket, session_name, state))
}

async fn handle_ws_control(mut socket: WebSocket, state: AppState) {
    info!("New Control WebSocket connection established");

    // Handle incoming messages
    while let Some(Ok(msg)) = socket.next().await {
        match msg {
            Message::Text(msg) => {
                let deserialized_msg: Result<ControlMessage, _> = serde_json::from_str(&msg);
                match deserialized_msg {
                    Ok(deserialized_msg) => {
                        let Some(client_connection) = state
                            .connection_table
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

async fn handle_ws_terminal(
    socket: WebSocket,
    session_name: Option<AxumPath<String>>,
    state: AppState,
) {
    let ipc_path = session_name
        .map(|p| {
            let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
            sock_dir.push(p.0);
            sock_dir.to_str().unwrap().to_owned()
        })
        .unwrap_or(state.ipc_path.clone());

    let web_client_id = String::from(Uuid::new_v4());
    let os_input = get_client_os_input().unwrap(); // TODO: log error and quit

    state.connection_table.lock().unwrap().insert(
        web_client_id.to_owned(),
        Arc::new(Mutex::new(Box::new(os_input.clone()))),
    );

    let (client_channel_tx, mut client_channel_rx) = socket.split();
    info!("New Terminal WebSocket connection established {}", ipc_path);
    let (stdout_channel_tx, stdout_channel_rx) = tokio::sync::mpsc::unbounded_channel();

    zellij_server_listener(
        Box::new(os_input.clone()),
        stdout_channel_tx,
        &ipc_path,
        state.config.clone(),
        state.config_options.clone(),
    );
    render_to_client(stdout_channel_rx, web_client_id, client_channel_tx);

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
                let deserialized_msg: Result<StdinMessage, _> = serde_json::from_str(&msg);
                match deserialized_msg {
                    Ok(deserialized_msg) => {
                        let Some(client_connection) = state
                            .connection_table
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
                            &mut mouse_old_event,
                            explicitly_disable_kitty_keyboard_protocol,
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

fn zellij_server_listener(
    os_input: Box<dyn ClientOsApi>,
    stdout_channel_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ipc_path: &str,
    config: Config,
    config_options: Options,
) {
    let _server_listener_thread = std::thread::Builder::new()
        .name("server_listener".to_string())
        .spawn({
            let path = ipc_path.to_owned();
            move || {
                let mut reconnect_to_session: Option<ConnectToSession> = None;
                'reconnect_loop: loop {
                    let reconnect_info = reconnect_to_session.take();
                    let path = reconnect_info.as_ref().and_then(|r| r.name.to_owned()).map(|name| {
                        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
                        sock_dir.push(name);
                        sock_dir.to_str().unwrap().to_owned()
                    }).unwrap_or_else(|| path.clone());

                    let full_screen_ws = os_input.get_terminal_size_using_fd(0);

                    let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
                    let enter_alternate_screen = "\u{1b}[?1049h";
                    let bracketed_paste = "\u{1b}[?2004h";
                    let enter_kitty_keyboard_mode = "\u{1b}[>1u";
                    let enable_mouse_mode = "\u{1b}[?1000h\u{1b}[?1002h\u{1b}[?1015h\u{1b}[?1006h";
                    let _ = stdout_channel_tx.send(clear_client_terminal_attributes.to_owned());
                    let _ = stdout_channel_tx.send(enter_alternate_screen.to_owned());
                    let _ = stdout_channel_tx.send(bracketed_paste.to_owned());
                    let _ = stdout_channel_tx.send(enable_mouse_mode.to_owned());
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

                    let session_name = PathBuf::from(path.clone()).file_name().unwrap().to_str().unwrap().to_owned();

                    let is_web_client = true;
                    let (first_message, zellij_ipc_pipe) = if session_exists(&session_name).unwrap_or(false) { // TODO: handle error
                        let zellij_ipc_pipe: PathBuf = {
                            let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
                            fs::create_dir_all(&sock_dir).unwrap();
                            zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
                            sock_dir.push(path);
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
                    } else {
                        let force_run_commands = false; // TODO: from config for resurrection
                                                        // layout
                        let resurrection_layout =
                            resurrection_layout(&session_name).map(|mut resurrection_layout| {
                                if force_run_commands {
                                    resurrection_layout.recursively_add_start_suspended(Some(false));
                                }
                                resurrection_layout
                            });

                        match resurrection_layout {
                            Some(resurrection_layout) => {
                                spawn_new_session(
                                    &path,
                                    is_web_client,
                                    os_input.clone(),
                                    config.clone(),
                                    config_options.clone(),
                                    Some(resurrection_layout),
                                    client_attributes
                                )
                            },
                            None => {
                                let layout_dir = config.options.layout_dir.clone().or_else(|| {
                                    get_layout_dir(find_default_config_dir())
                                });
                                let new_session_layout = match reconnect_info.as_ref().and_then(|r| r.layout.clone()) {
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
                                    Some(LayoutInfo::Stringified(stringified_layout)) => Layout::from_stringified_layout(
                                        &stringified_layout,
                                        config.clone(),
                                    ),
                                    None => Ok(Default::default())
                                };

                                spawn_new_session(
                                    &path,
                                    is_web_client,
                                    os_input.clone(),
                                    config.clone(),
                                    config_options.clone(),
                                    new_session_layout.ok().map(|(l, c)| l), // TODO: handle config
                                    client_attributes
                                )

                            }
                        }
                    };

                    os_input.connect_to_server(&zellij_ipc_pipe);
                    os_input.send_to_server(first_message);

                    // we keep the _config_file_watcher here so that it's dropped on the next round
                    // of the reconnect loop
                    // TODO: get actual CliArgs
                    let _config_file_watcher = report_changes_in_config_file(&CliArgs::default(), &os_input);
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
                                    },
                                    _ => {},
                                }
                                os_input.send_to_server(ClientToServerMsg::ClientExited);
                                break;
                            },
                            Some((ServerToClientMsg::Render(bytes), _)) => {
                                let _ = stdout_channel_tx.send(bytes);
                            },
                            Some((ServerToClientMsg::SwitchSession(connect_to_session), _)) => {
                                reconnect_to_session = Some(connect_to_session);
                                continue 'reconnect_loop;
                            },
                            Some((ServerToClientMsg::WriteConfigToDisk{ config }, _)) => {
                                // TODO: get config path from actual CLI args and differentiate
                                // between sessions (this is also a bug in the CLI client)
                                match Config::write_config_to_disk(config, &CliArgs::default()) {
                                    Ok(written_config) => {
                                        let _ = os_input
                                            .send_to_server(ClientToServerMsg::ConfigWrittenToDisk(written_config));
                                    },
                                    Err(e) => {
                                        let error_path = e
                                            .as_ref()
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_else(String::new);
                                        log::error!("Failed to write config to disk: {}", error_path);
                                        let _ = os_input
                                            .send_to_server(ClientToServerMsg::FailedToWriteConfigToDisk(e));
                                    },
                                }

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
    web_client_id: String,
    mut client_channel_tx: SplitSink<WebSocket, Message>,
) {
    tokio::spawn(async move {
        loop {
            if let Some(rendered_bytes) = stdout_channel_rx.recv().await {
                match serde_json::to_string(&RenderedBytes::new(rendered_bytes, &web_client_id)) {
                    Ok(rendered_bytes) => {
                        if client_channel_tx
                            .send(Message::Text(rendered_bytes.into()))
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
                let key = cast_termwiz_key(
                    key_event.clone(),
                    &buf,
                    None, // TODO: config, for ctrl-j etc.
                );
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

fn spawn_new_session(
    name: &str,
    is_web_client: bool,
    mut os_input: Box<dyn ClientOsApi>,
    config: Config,
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

    // TODO: make this happen
    //     let successfully_written_config =
    //         Config::write_config_to_disk_if_it_does_not_exist(config.to_string(true), &config_opts);
    // if we successfully wrote the config to disk, it means two things:
    // 1. It did not exist beforehand
    // 2. The config folder is writeable
    //
    // If these two are true, we should launch the setup wizard, if even one of them is
    // false, we should never launch it.
    // let should_launch_setup_wizard = successfully_written_config;
    let should_launch_setup_wizard = false;
    let cli_args = CliArgs::default(); // TODO: what do we do about this and the above setup
                                       // wizard?

    (
        ClientToServerMsg::NewClient(
            client_attributes,
            Box::new(cli_args),
            Box::new(config.clone()),
            // Box::new(config_options.clone()),
            Box::new(config_opts.clone()), // TODO: what is the difference?
            Box::new(layout.unwrap()),
            Box::new(config.plugins.clone()),
            should_launch_setup_wizard,
        ),
        zellij_ipc_pipe,
    )
}

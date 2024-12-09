//! The `[cli_client]` is used to attach to a running server session
//! and dispatch actions, that are specified through the command line.
use std::collections::{BTreeMap, HashMap};
use std::io::BufRead;
use std::process;
use std::{fs, path::PathBuf};

use crate::os_input_output::ClientOsApi;
use crate::os_input_output::get_client_os_input;
use zellij_utils::{
    errors::prelude::*,
    input::actions::Action,
    input::config::{Config, ConfigError},
    input::options::Options,
    data::Style,
    ipc::{ClientToServerMsg, ExitReason, ServerToClientMsg, ClientAttributes, IpcSenderWithContext},
    uuid::Uuid,
    pane_size::{Size, SizeInPixels},
    serde::{Serialize, Deserialize},
    serde_json
};

use std::sync::{Arc, Mutex};


use std::env;
use log::info;
use std::time::Duration;
use tokio::{task, time};
use tokio::sync::mpsc::{UnboundedSender, UnboundedReceiver};
use futures::{future, SinkExt, prelude::stream::SplitSink};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, WebSocketStream};
use futures::StreamExt;
use tokio_tungstenite::tungstenite::Message;
use tokio::runtime::Runtime;



// DEV INSTRUCTIONS:
// * to run this:
//      - cargo x run --singlepass
//      - (inside the session): target/dev-opt/zellij --web $ZELLIJ_SESSION_NAME


type ConnectionTable = Arc<Mutex<HashMap<String, Arc<Mutex<Box< dyn ClientOsApi>>>>>>; // TODO: no

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

pub fn start_web_client(
    session_name: &str,
    config: Config,
    config_options: Options,
) {
    log::info!("WebSocket server started and listening on port 8080 and 8081");


    let connection_table: HashMap<String, Arc<Mutex<Box<dyn ClientOsApi>>>> = HashMap::new();
    let connection_table = Arc::new(Mutex::new(connection_table));

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        future::join(
            terminal_server(session_name, config.clone(), config_options.clone(), connection_table.clone()),
            handle_server_control(session_name, config, config_options, connection_table),
        ).await;
    });
    log::info!("Server closed");
}

async fn terminal_server(session_name: &str, config: Config, config_options: Options, connection_table: ConnectionTable) {
    let addr = "127.0.0.1:8080";
    let listener = TcpListener::bind(addr).await.unwrap();
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(start_terminal_connection(stream, session_name.to_owned(), config.clone(), config_options.clone(), connection_table.clone()));
    }
}

async fn handle_server_control(session_name: &str, config: Config, config_options: Options, connection_table: ConnectionTable) {
    let addr = "127.0.0.1:8081";
    let listener = TcpListener::bind(addr).await.unwrap();
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(handle_client_control(stream, session_name.to_owned(), config.clone(), config_options.clone(), connection_table.clone()));
    }
}

async fn start_terminal_connection(
    stream: tokio::net::TcpStream,
    session_name: String,
    config: Config,
    config_options: Options,
    connection_table: ConnectionTable,
) {
    let web_client_id = String::from(Uuid::new_v4());
    let os_input = get_client_os_input().unwrap(); // TODO: log error and quit

    connection_table.lock().unwrap().insert(web_client_id.to_owned(), Arc::new(Mutex::new(Box::new(os_input.clone()))));

    let ws_stream = accept_async(stream).await.unwrap();
    let (client_channel_tx, mut client_channel_rx) = ws_stream.split();
    info!("New WebSocket connection established");
    let (stdout_channel_tx, stdout_channel_rx) = tokio::sync::mpsc::unbounded_channel();

    zellij_server_listener(Box::new(os_input.clone()), stdout_channel_tx, &session_name, config.clone(), config_options.clone());
    render_to_client(stdout_channel_rx, web_client_id, client_channel_tx);

    // Handle incoming messages (STDIN)
    while let Some(Ok(_msg)) = client_channel_rx.next().await {
        // TODO
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
                        log::info!("can has!: {:?}", deserialized_msg);
                        let Some(client_connection) = connection_table
                            .lock()
                            .unwrap()
                            .get(&deserialized_msg.web_client_id)
                            .cloned() else {
                                log::error!("Unknown web_client_id: {}", deserialized_msg.web_client_id);
                                continue;
                            };
                        client_connection.lock().unwrap()
                            .send_to_server(deserialized_msg.message);
                    },
                    Err(e) => {
                        log::error!("Failed to deserialize client msg: {:?}", e);
                    }
                }
            },
            _ => {
                log::error!("Unsupported messagetype : {:?}", msg);
            }
        }
    }
    os_input.send_to_server(ClientToServerMsg::ClientExited);
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


    let first_message = ClientToServerMsg::AttachClient(
        client_attributes,
        config.clone(),
        Default::default(),
        Default::default(),
        Default::default(),
//         client_attributes,
//         config.clone(),
//         config_options.clone(),
//         tab_position_to_focus,
//         pane_id_to_focus,
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
                        }
                        _ => {},
                    }
                }
            }
        });

}

fn render_to_client(mut stdout_channel_rx: UnboundedReceiver<String>, web_client_id: String, mut client_channel_tx: SplitSink<WebSocketStream<TcpStream>, Message>) {
    tokio::spawn(async move {
        loop {
            if let Some(rendered_bytes) = stdout_channel_rx.recv().await {
                match serde_json::to_string(&RenderedBytes::new(rendered_bytes, &web_client_id)) {
                    Ok(rendered_bytes) => {
                        if client_channel_tx.send(Message::Text(rendered_bytes)).await.is_err() {
                            break;
                        }
                    },
                    Err(e) => {
                        log::error!("Failed to serialize rendered bytes: {:?}", e);
                    }
                }
            }
        }
    });
}

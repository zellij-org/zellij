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
    serde_json
};

use std::sync::{Arc, Mutex};


use std::env;
use log::info;
use std::time::Duration;
use tokio::{task, time};
use futures::{future, SinkExt};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use futures::StreamExt;
use tokio_tungstenite::tungstenite::Message;
use tokio::runtime::Runtime;



// DEV INSTRUCTIONS:
// * to run this:
//      - cargo x run --singlepass
//      - (inside the session): target/dev-opt/zellij --web $ZELLIJ_SESSION_NAME

type ConnectionTable = Arc<Mutex<HashMap<String, Arc<Mutex<Option<Box< dyn ClientOsApi>>>>>>>; // TODO: no

pub fn start_web_client(
    session_name: &str,
    config: Config,
    config_options: Options,
) {
    log::info!("WebSocket server started and listening on port 8080 and 8081");


    let connection_table: HashMap<String, Arc<Mutex<Option<Box<dyn ClientOsApi>>>>> = HashMap::new();
    let connection_table = Arc::new(Mutex::new(connection_table));

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        future::join(
            handle_server_terminal(session_name, config.clone(), config_options.clone(), connection_table.clone()),
            handle_server_control(session_name, config, config_options, connection_table),
        ).await;
    });
    log::info!("Server closed");
}

async fn handle_server_terminal(session_name: &str, config: Config, config_options: Options, connection_table: ConnectionTable) {
    let addr = "127.0.0.1:8080";
    let listener = TcpListener::bind(addr).await.unwrap();
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(handle_client_terminal(stream, session_name.to_owned(), config.clone(), config_options.clone(), connection_table.clone()));
    }
}

async fn handle_server_control(session_name: &str, config: Config, config_options: Options, connection_table: ConnectionTable) {
    let addr = "127.0.0.1:8081";
    let listener = TcpListener::bind(addr).await.unwrap();
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(handle_client_control(stream, session_name.to_owned(), config.clone(), config_options.clone(), connection_table.clone()));
    }
}

async fn handle_client_terminal(
    stream: tokio::net::TcpStream,
    session_name: String,
    config: Config,
    config_options: Options,
    connection_table: ConnectionTable,
) {
    let os_input = get_client_os_input().unwrap(); // TODO: log error and quit
    let ws_stream = accept_async(stream).await.unwrap();
    let (mut write, mut read) = ws_stream.split();
    info!("New WebSocket connection established");
    let (websocket_channel_tx, mut websocket_channel_rc) = tokio::sync::mpsc::unbounded_channel();
    let _server_listener_thread = std::thread::Builder::new()
        .name("server_listener".to_string())
        .spawn({
            let session_name = session_name.clone();
            let os_input = os_input.clone();
            move || {
                server_listener(Box::new(os_input), websocket_channel_tx, &session_name, config.clone(), config_options.clone(), connection_table);
            }
        });

    tokio::spawn(async move {
        loop {
            if let Some(rendered_bytes) = websocket_channel_rc.recv().await {
                if write.send(Message::Text(rendered_bytes)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Handle incoming messages (STDIN)
    while let Some(Ok(_msg)) = read.next().await {
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
    let client_connection = connection_table.lock().unwrap().entry("my_client_id".to_owned()).or_insert_with(Default::default).clone();
// player_stats.entry("health").or_insert(100);
    let os_input = get_client_os_input().unwrap(); // TODO: log error and quit
    let ws_stream = accept_async(stream).await.unwrap();
    let (mut write, mut read) = ws_stream.split();
    info!("New Control WebSocket connection established");
    

    // let (websocket_channel_tx, mut websocket_channel_rc) = tokio::sync::mpsc::unbounded_channel();
//     let _server_listener_thread = std::thread::Builder::new()
//         .name("server_listener".to_string())
//         .spawn({
//             let session_name = session_name.clone();
//             let os_input = os_input.clone();
//             move || {
//                 server_listener(Box::new(os_input), websocket_channel_tx, &session_name, config.clone(), config_options.clone());
//             }
//         });




//     tokio::spawn(async move {
//         loop {
//             if let Some(rendered_bytes) = websocket_channel_rc.recv().await {
//                 if write.send(Message::Text(rendered_bytes)).await.is_err() {
//                     break;
//                 }
//             }
//         }
//     });

    // Handle incoming messages
    while let Some(Ok(msg)) = read.next().await {
        match msg {
            Message::Text(msg) => {
                let deserialized_msg: Result<ClientToServerMsg, _> = serde_json::from_str(&msg);
                match deserialized_msg {
                    Ok(deserialized_msg) => {
                        log::info!("can has!: {:?}", deserialized_msg);
                        match client_connection.lock().unwrap().as_ref() {
                            Some(os_input) => {
                                log::info!("sending to server");
                                os_input.send_to_server(deserialized_msg);
                            },
                            None => {
                                // TODO: retry?
                                log::error!("Connection not ready yet");
                            }
                        }
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


fn server_listener(
    os_input: Box<dyn ClientOsApi>,
    web_sender: tokio::sync::mpsc::UnboundedSender<String>,
    session_name: &str,
    config: Config,
    config_options: Options,
    connection_table: ConnectionTable
) {
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
//     let client_attributes = ClientAttributes {
//         size: full_screen_ws,
//         style: Default::default(),
//     };

    let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
    let enter_alternate_screen = "\u{1b}[?1049h";
    let bracketed_paste = "\u{1b}[?2004h";
    let enter_kitty_keyboard_mode = "\u{1b}[>1u";
    let _ = web_sender.send(clear_client_terminal_attributes.to_owned());
    let _ = web_sender.send(enter_alternate_screen.to_owned());
    let _ = web_sender.send(bracketed_paste.to_owned());
    let _ = web_sender.send(enter_kitty_keyboard_mode.to_owned());

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


    // TODO: CONTINUE HERE (04/12)
    // - send the initialization string stuffs to the client - DONE
    // - generate a client id, send it on the control channel and expect it on the terminal (and
    // control) channels (instead of hte my_client_id stuffs)
    // - clean up and commit so that Thomas can work with this
    // - fill these up (copy/paste from lib.rs in zellij client hopefully) and
    // see if this does the trick
    // - receive keyboard input from client
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
    // let send_messages_to_server = os_input.clone_server_sender();
    // TODO: CONTINUE HERE (04/12 evening)- change this to get_or_insert, then use the other one
    // when sending control messages to the server
    connection_table.lock().unwrap().entry("my_client_id".to_owned()).or_insert_with(Default::default).lock().unwrap().replace(os_input.clone());

        // .insert(send_messages_to_server);



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
                let _ = web_sender.send(bytes);
            }
            _ => {},
        }
    }
}

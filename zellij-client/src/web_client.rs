//! The `[cli_client]` is used to attach to a running server session
//! and dispatch actions, that are specified through the command line.
use std::collections::BTreeMap;
use std::io::BufRead;
use std::process;
use std::{fs, path::PathBuf};

use crate::os_input_output::ClientOsApi;
use crate::os_input_output::get_client_os_input;
use zellij_utils::{
    errors::prelude::*,
    input::actions::Action,
    ipc::{ClientToServerMsg, ExitReason, ServerToClientMsg, ClientAttributes},
    uuid::Uuid,
};


use std::env;
use log::info;
use std::time::Duration;
use tokio::{task, time};
use futures::SinkExt;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use futures::StreamExt;
use tokio_tungstenite::tungstenite::Message;
use tokio::runtime::Runtime;



pub fn start_web_client(
    session_name: &str,
) {
    log::info!("WebSocket server started and listening on port 8080");
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        handle_server(session_name).await;
    });
    log::info!("Server closed?");
}

async fn handle_server(session_name: &str) {
    let addr = "127.0.0.1:8080";
    let listener = TcpListener::bind(addr).await.unwrap();
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(handle_client(stream, session_name.to_owned()));
    }
}

async fn handle_client(
    stream: tokio::net::TcpStream,
    session_name: String,
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
                server_listener(Box::new(os_input), websocket_channel_tx, &session_name);
                log::info!("done server listening");
            }
        });

    // Create a task to periodically send updates
    tokio::spawn(async move {
        
        loop {
            if let Some(rendered_bytes) = websocket_channel_rc.recv().await {
                if write.send(Message::Text(rendered_bytes)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Handle incoming messages (if necessary)
    while let Some(Ok(_msg)) = read.next().await {
        // In this example, we don't need to handle incoming messages
    }
    log::info!("client dead?");
    os_input.send_to_server(ClientToServerMsg::ClientExited);
    log::info!("sent exited to server");
}


fn server_listener(
    os_input: Box<dyn ClientOsApi>,
    web_sender: tokio::sync::mpsc::UnboundedSender<String>,
    session_name: &str,
) {
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    let client_attributes = ClientAttributes {
        size: full_screen_ws,
        style: Default::default(),
    };

    // TODO: CONTINUE HERE (28/11)
    // - send the initialization string stuffs to the client
    // - fill these up (copy/paste from lib.rs in zellij client hopefully) and
    // see if this does the trick
    // - receive keyboard input from client
    let first_message = ClientToServerMsg::AttachClient(
        client_attributes,
        Default::default(),
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



    loop {
        log::info!("server_listener listening to msg");
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
                log::info!("can has render");
                let _ = web_sender.send(bytes);
            }
            _ => {},
        }
    }
}

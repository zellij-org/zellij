//! The `[cli_client]` is used to attach to a running server session
//! and dispatch actions, that are specified through the command line.
use std::process;
use std::{fs, path::PathBuf};

use crate::os_input_output::ClientOsApi;
use zellij_utils::{
    input::actions::Action,
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

pub fn start_cli_client(os_input: Box<dyn ClientOsApi>, session_name: &str, actions: Vec<Action>) {
    log::info!("Starting cli client");
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };
    os_input.connect_to_server(&*zellij_ipc_pipe);
    let pane_id = os_input
        .env_variable("ZELLIJ_PANE_ID")
        .and_then(|e| e.trim().parse().ok());
    for action in actions {
        let msg = ClientToServerMsg::Action(action, pane_id, None);
        os_input.send_to_server(msg);
    }
    loop {
        match os_input.recv_from_server() {
            Ok((ServerToClientMsg::UnblockInputThread, _)) => {
                os_input.send_to_server(ClientToServerMsg::ClientExited);
                process::exit(0);
            },
            Ok((ServerToClientMsg::Log(log_lines), _)) => {
                log_lines.iter().for_each(|line| println!("{line}"));
                process::exit(0);
            },
            Ok((ServerToClientMsg::LogError(log_lines), _)) => {
                log_lines.iter().for_each(|line| eprintln!("{line}"));
                process::exit(2);
            },
            _ => {},
        }
    }
}

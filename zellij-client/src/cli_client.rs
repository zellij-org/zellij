//! The `[cli_client]` is used to attach to a running server session
//! and dispatch actions, that are specified through the command line.
use std::process;
use std::{fs, path::PathBuf};
use std::collections::BTreeMap;

use crate::os_input_output::ClientOsApi;
use zellij_utils::{
    uuid::Uuid,
    errors::prelude::*,
    input::actions::Action,
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

pub fn start_cli_client(mut os_input: Box<dyn ClientOsApi>, session_name: &str, actions: Vec<Action>) {
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
        match action {
            Action::CliMessage { name, payload, plugin, args } if payload.is_none() => {
                pipe_client(&mut os_input, name, plugin, args, pane_id);
            },
            action => {
                single_message_client(&mut os_input, action, pane_id);
            }
        }
    }
}

fn pipe_client(os_input: &mut Box<dyn ClientOsApi>, mut pipe_name: Option<String>, plugin: Option<String>, args: Option<BTreeMap<String, String>>, pane_id: Option<u32>) {
    use std::io::BufRead;
    let stdin = std::io::stdin(); // TODO: from os_input
    let mut handle = stdin.lock();
    let name = pipe_name.take().or_else(|| Some(Uuid::new_v4().to_string()));
    loop {
        let mut buffer = String::new();
        handle.read_line(&mut buffer).unwrap(); // TODO: no unwrap etc.
        if buffer.is_empty() {
            let msg = ClientToServerMsg::Action(Action::CliMessage{ name: name.clone(), payload: None, args: args.clone(), plugin: plugin.clone() }, pane_id, None);
            os_input.send_to_server(msg);
            break;
        } else {
            let msg = ClientToServerMsg::Action(Action::CliMessage{ name: name.clone(), payload: Some(buffer), args: args.clone(), plugin: plugin.clone() }, pane_id, None);
            os_input.send_to_server(msg);
        }
        loop {
            match os_input.recv_from_server() {
                Some((ServerToClientMsg::UnblockCliPipeInput(pipe_name), _)) => {
                    if Some(pipe_name) == name {
                        break;
                    }
                },
                Some((ServerToClientMsg::CliPipeOutput(pipe_name, output), _)) => {
                    let err_context = "Failed to write to stdout";
                    if Some(pipe_name) == name {
                        let mut stdout = os_input.get_stdout_writer();
                        stdout
                            .write_all(output.as_bytes())
                            .context(err_context)
                            .non_fatal();
                        stdout.flush()
                            .context(err_context)
                            .non_fatal();
                    }
                },
                _ => {},
            }
        }
    }
}

fn single_message_client(os_input: &mut Box<dyn ClientOsApi>, action: Action, pane_id: Option<u32>) {
    let msg = ClientToServerMsg::Action(action, pane_id, None);
    os_input.send_to_server(msg);
    loop {
        match os_input.recv_from_server() {
            Some((ServerToClientMsg::UnblockInputThread, _)) => {
                os_input.send_to_server(ClientToServerMsg::ClientExited);
                process::exit(0);
            },
            Some((ServerToClientMsg::Log(log_lines), _)) => {
                log_lines.iter().for_each(|line| println!("{line}"));
                process::exit(0);
            },
            Some((ServerToClientMsg::LogError(log_lines), _)) => {
                log_lines.iter().for_each(|line| eprintln!("{line}"));
                process::exit(2);
            },
            _ => {},
        }
    }
}

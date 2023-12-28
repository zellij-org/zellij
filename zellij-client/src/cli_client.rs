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
    ipc::{ClientToServerMsg, ServerToClientMsg, ExitReason},
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
            Action::CliMessage { input_pipe_id, name, payload, plugin, args, configuration, launch_new, skip_cache, floating, in_place, cwd, pane_title } if payload.is_none() => {
                pipe_client(&mut os_input, input_pipe_id, name, plugin, args, configuration, launch_new, skip_cache, floating, in_place, pane_id, cwd, pane_title);
            },
            action => {
                single_message_client(&mut os_input, action, pane_id);
            }
        }
    }
}

fn pipe_client(
    os_input: &mut Box<dyn ClientOsApi>,
    input_pipe_id: String,
    mut name: Option<String>,
    plugin: Option<String>,
    args: Option<BTreeMap<String, String>>,
    mut configuration: Option<BTreeMap<String, String>>,
    launch_new: bool,
    skip_cache: bool,
    floating: Option<bool>,
    in_place: Option<bool>,
    pane_id: Option<u32>,
    cwd: Option<PathBuf>,
    pane_title: Option<String>,
) {
    use std::io::BufRead;
    let stdin = std::io::stdin(); // TODO: from os_input
    let mut handle = stdin.lock();
    let name = name.take().or_else(|| Some(Uuid::new_v4().to_string()));
    if launch_new {
        configuration.get_or_insert_with(BTreeMap::new).insert("_zellij_id".to_owned(), Uuid::new_v4().to_string());
    }
    loop {
        let mut buffer = String::new();
        handle.read_line(&mut buffer).unwrap(); // TODO: no unwrap etc.
        if buffer.is_empty() {
            let msg = ClientToServerMsg::Action(Action::CliMessage{
                input_pipe_id: input_pipe_id.clone(),
                name: name.clone(),
                payload: None,
                args: args.clone(),
                plugin: plugin.clone(),
                configuration: configuration.clone(),
                floating,
                in_place,
                launch_new,
                skip_cache,
                cwd: cwd.clone(),
                pane_title: pane_title.clone()
            }, pane_id, None);
            os_input.send_to_server(msg);
            break;
        } else {
            let msg = ClientToServerMsg::Action(Action::CliMessage{
                input_pipe_id: input_pipe_id.clone(),
                name: name.clone(),
                payload: Some(buffer),
                args: args.clone(),
                plugin: plugin.clone(),
                configuration: configuration.clone(),
                floating,
                in_place,
                launch_new,
                skip_cache,
                cwd: cwd.clone(),
                pane_title: pane_title.clone()
            }, pane_id, None);
            os_input.send_to_server(msg);
            // launch_new = false; // if we don't do this a plugin will be launched for each pipe
                                // message and we definitely don't want that
        }
        loop {
            match os_input.recv_from_server() {
                Some((ServerToClientMsg::UnblockCliPipeInput(pipe_name), _)) => {
                    if pipe_name == input_pipe_id {
                        break;
                    }
                },
                Some((ServerToClientMsg::CliPipeOutput(pipe_name, output), _)) => {
                    let err_context = "Failed to write to stdout";
                    if pipe_name == input_pipe_id {
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
                Some((ServerToClientMsg::Exit(exit_reason), _)) => {
                    match exit_reason {
                        ExitReason::Error(e) => {
                            eprintln!("{}", e);
                            process::exit(2);
                        },
                        _ => {
                            process::exit(0);
                        }
                    }
                }
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
            Some((ServerToClientMsg::Exit(exit_reason), _)) => {
                match exit_reason {
                    ExitReason::Error(e) => {
                        eprintln!("{}", e);
                        process::exit(2);
                    },
                    _ => {
                        process::exit(0);
                    }
                }
            }
            _ => {},
        }
    }
}

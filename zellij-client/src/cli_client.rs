//! The `[cli_client]` is used to attach to a running server session
//! and dispatch actions, that are specified through the command line.
use std::collections::BTreeMap;
use std::io::BufRead;
use std::process;
use std::{fs, path::PathBuf};

use crate::os_input_output::ClientOsApi;
use zellij_utils::{
    errors::prelude::*,
    input::actions::Action,
    ipc::{ClientToServerMsg, ExitReason, ServerToClientMsg},
    uuid::Uuid,
};

pub fn start_cli_client(
    mut os_input: Box<dyn ClientOsApi>,
    session_name: &str,
    actions: Vec<Action>,
) {
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
            Action::CliPipe {
                pipe_id,
                name,
                payload,
                plugin,
                args,
                configuration,
                launch_new,
                skip_cache,
                floating,
                in_place,
                cwd,
                pane_title,
            } => {
                pipe_client(
                    &mut os_input,
                    pipe_id,
                    name,
                    payload,
                    plugin,
                    args,
                    configuration,
                    launch_new,
                    skip_cache,
                    floating,
                    in_place,
                    pane_id,
                    cwd,
                    pane_title,
                );
            },
            action => {
                individual_messages_client(&mut os_input, action, pane_id);
            },
        }
    }
    os_input.send_to_server(ClientToServerMsg::ClientExited);
}

fn pipe_client(
    os_input: &mut Box<dyn ClientOsApi>,
    pipe_id: String,
    mut name: Option<String>,
    mut payload: Option<String>,
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
    let mut stdin = os_input.get_stdin_reader();
    let name = name
        // first we try to take the explicitly supplied message name
        .take()
        // then we use the plugin, to facilitate using aliases
        .or_else(|| plugin.clone())
        // then we use a uuid to at least have some sort of identifier for this message
        .or_else(|| Some(Uuid::new_v4().to_string()));
    if launch_new {
        // we do this to make sure the plugin is unique (has a unique configuration parameter) so
        // that a new one would be launched, but we'll still send it to the same instance rather
        // than launching a new one in every iteration of the loop
        configuration
            .get_or_insert_with(BTreeMap::new)
            .insert("_zellij_id".to_owned(), Uuid::new_v4().to_string());
    }
    let create_msg = |payload: Option<String>| -> ClientToServerMsg {
        ClientToServerMsg::Action(
            Action::CliPipe {
                pipe_id: pipe_id.clone(),
                name: name.clone(),
                payload,
                args: args.clone(),
                plugin: plugin.clone(),
                configuration: configuration.clone(),
                floating,
                in_place,
                launch_new,
                skip_cache,
                cwd: cwd.clone(),
                pane_title: pane_title.clone(),
            },
            pane_id,
            None,
        )
    };
    let is_piped = !os_input.stdin_is_terminal();
    loop {
        if let Some(payload) = payload.take() {
            let msg = create_msg(Some(payload));
            os_input.send_to_server(msg);
        } else if !is_piped {
            // here we send an empty message to trigger the plugin, because we don't have any more
            // data
            let msg = create_msg(None);
            os_input.send_to_server(msg);
        } else {
            // we didn't get payload from the command line, meaning we listen on STDIN because this
            // signifies the user is about to pipe more (eg. cat my-large-file | zellij pipe ...)
            let mut buffer = String::new();
            let _ = stdin.read_line(&mut buffer);
            if buffer.is_empty() {
                let msg = create_msg(None);
                os_input.send_to_server(msg);
                break;
            } else {
                // we've got data! send it down the pipe (most common)
                let msg = create_msg(Some(buffer));
                os_input.send_to_server(msg);
            }
        }
        loop {
            // wait for a response and act accordingly
            match os_input.recv_from_server() {
                Some((ServerToClientMsg::UnblockCliPipeInput(pipe_name), _)) => {
                    // unblock this pipe, meaning we need to stop waiting for a response and read
                    // once more from STDIN
                    if pipe_name == pipe_id {
                        if !is_piped {
                            // if this client is not piped, we need to exit the process completely
                            // rather than wait for more data
                            process::exit(0);
                        } else {
                            break;
                        }
                    }
                },
                Some((ServerToClientMsg::CliPipeOutput(pipe_name, output), _)) => {
                    // send data to STDOUT, this *does not* mean we need to unblock the input
                    let err_context = "Failed to write to stdout";
                    if pipe_name == pipe_id {
                        let mut stdout = os_input.get_stdout_writer();
                        stdout
                            .write_all(output.as_bytes())
                            .context(err_context)
                            .non_fatal();
                        stdout.flush().context(err_context).non_fatal();
                    }
                },
                Some((ServerToClientMsg::Log(log_lines), _)) => {
                    log_lines.iter().for_each(|line| println!("{line}"));
                    process::exit(0);
                },
                Some((ServerToClientMsg::LogError(log_lines), _)) => {
                    log_lines.iter().for_each(|line| eprintln!("{line}"));
                    process::exit(2);
                },
                Some((ServerToClientMsg::Exit(exit_reason), _)) => match exit_reason {
                    ExitReason::Error(e) => {
                        eprintln!("{}", e);
                        process::exit(2);
                    },
                    _ => {
                        process::exit(0);
                    },
                },
                _ => {},
            }
        }
    }
}

fn individual_messages_client(
    os_input: &mut Box<dyn ClientOsApi>,
    action: Action,
    pane_id: Option<u32>,
) {
    let msg = ClientToServerMsg::Action(action, pane_id, None);
    os_input.send_to_server(msg);
    loop {
        match os_input.recv_from_server() {
            Some((ServerToClientMsg::UnblockInputThread, _)) => {
                break;
            },
            Some((ServerToClientMsg::Log(log_lines), _)) => {
                log_lines.iter().for_each(|line| println!("{line}"));
                break;
            },
            Some((ServerToClientMsg::LogError(log_lines), _)) => {
                log_lines.iter().for_each(|line| eprintln!("{line}"));
                process::exit(2);
            },
            Some((ServerToClientMsg::Exit(exit_reason), _)) => match exit_reason {
                ExitReason::Error(e) => {
                    eprintln!("{}", e);
                    process::exit(2);
                },
                _ => {
                    break;
                },
            },
            _ => {},
        }
    }
}

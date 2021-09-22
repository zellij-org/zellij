pub mod os_input_output;

mod command_is_executing;
mod input_handler;

use log::info;
use std::env::current_exe;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::thread;

use crate::{
    command_is_executing::CommandIsExecuting, input_handler::input_loop,
    os_input_output::ClientOsApi,
};
use zellij_utils::{
    channels::{self, ChannelWithContext, SenderWithContext},
    consts::{SESSION_NAME, ZELLIJ_IPC_PIPE},
    errors::{ClientContext, ContextType, ErrorInstruction},
    input::{actions::Action, config::Config, options::Options},
    ipc::{ClientAttributes, ClientToServerMsg, ExitReason, ServerToClientMsg},
};
use zellij_utils::{cli::CliArgs, input::layout::LayoutFromYaml};

/// Instructions related to the client-side application
#[derive(Debug, Clone)]
pub(crate) enum ClientInstruction {
    Error(String),
    Render(String),
    UnblockInputThread,
    Exit(ExitReason),
}

impl From<ServerToClientMsg> for ClientInstruction {
    fn from(instruction: ServerToClientMsg) -> Self {
        match instruction {
            ServerToClientMsg::Exit(e) => ClientInstruction::Exit(e),
            ServerToClientMsg::Render(buffer) => ClientInstruction::Render(buffer),
            ServerToClientMsg::UnblockInputThread => ClientInstruction::UnblockInputThread,
        }
    }
}

impl From<&ClientInstruction> for ClientContext {
    fn from(client_instruction: &ClientInstruction) -> Self {
        match *client_instruction {
            ClientInstruction::Exit(_) => ClientContext::Exit,
            ClientInstruction::Error(_) => ClientContext::Error,
            ClientInstruction::Render(_) => ClientContext::Render,
            ClientInstruction::UnblockInputThread => ClientContext::UnblockInputThread,
        }
    }
}

impl ErrorInstruction for ClientInstruction {
    fn error(err: String) -> Self {
        ClientInstruction::Error(err)
    }
}

fn spawn_server(socket_path: &Path) -> io::Result<()> {
    let status = Command::new(current_exe()?)
        .arg("--server")
        .arg(socket_path)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        let msg = "Process returned non-zero exit code";
        let err_msg = match status.code() {
            Some(c) => format!("{}: {}", msg, c),
            None => msg.to_string(),
        };
        Err(io::Error::new(io::ErrorKind::Other, err_msg))
    }
}

#[derive(Debug, Clone)]
pub enum ClientInfo {
    Attach(String, bool, Options),
    New(String),
}

pub fn start_client(
    mut os_input: Box<dyn ClientOsApi>,
    opts: CliArgs,
    config: Config,
    config_options: Options,
    info: ClientInfo,
    layout: Option<LayoutFromYaml>,
) {
    info!("Starting Zellij client!");
    let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}12l\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
    let take_snapshot = "\u{1b}[?1049h";
    let bracketed_paste = "\u{1b}[?2004h";
    os_input.unset_raw_mode(0);

    let _ = os_input
        .get_stdout_writer()
        .write(take_snapshot.as_bytes())
        .unwrap();
    let _ = os_input
        .get_stdout_writer()
        .write(clear_client_terminal_attributes.as_bytes())
        .unwrap();
    std::env::set_var(&"ZELLIJ", "0");

    let palette = config.themes.clone().map_or_else(
        || os_input.load_palette(),
        |t| {
            t.theme_config(&config_options)
                .unwrap_or_else(|| os_input.load_palette())
        },
    );

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    let client_attributes = ClientAttributes {
        size: full_screen_ws,
        palette,
    };

    let first_msg = match info {
        ClientInfo::Attach(name, force, config_options) => {
            SESSION_NAME.set(name).unwrap();
            std::env::set_var(&"ZELLIJ_SESSION_NAME", SESSION_NAME.get().unwrap());

            ClientToServerMsg::AttachClient(client_attributes, force, config_options)
        }
        ClientInfo::New(name) => {
            SESSION_NAME.set(name).unwrap();
            std::env::set_var(&"ZELLIJ_SESSION_NAME", SESSION_NAME.get().unwrap());

            spawn_server(&*ZELLIJ_IPC_PIPE).unwrap();

            ClientToServerMsg::NewClient(
                client_attributes,
                Box::new(opts),
                Box::new(config_options.clone()),
                layout.unwrap(),
                Some(config.plugins.clone()),
            )
        }
    };

    os_input.connect_to_server(&*ZELLIJ_IPC_PIPE);
    os_input.send_to_server(first_msg);

    let mut command_is_executing = CommandIsExecuting::new();

    os_input.set_raw_mode(0);
    let _ = os_input
        .get_stdout_writer()
        .write(bracketed_paste.as_bytes())
        .unwrap();

    let (send_client_instructions, receive_client_instructions): ChannelWithContext<
        ClientInstruction,
    > = channels::bounded(50);
    let send_client_instructions = SenderWithContext::new(send_client_instructions);

    std::panic::set_hook({
        use zellij_utils::errors::handle_panic;
        let send_client_instructions = send_client_instructions.clone();
        Box::new(move |info| {
            handle_panic(info, &send_client_instructions);
        })
    });

    let on_force_close = config_options.on_force_close.unwrap_or_default();

    let _stdin_thread = thread::Builder::new()
        .name("stdin_handler".to_string())
        .spawn({
            let send_client_instructions = send_client_instructions.clone();
            let command_is_executing = command_is_executing.clone();
            let os_input = os_input.clone();
            let default_mode = config_options.default_mode.unwrap_or_default();
            move || {
                input_loop(
                    os_input,
                    config,
                    config_options,
                    command_is_executing,
                    send_client_instructions,
                    default_mode,
                )
            }
        });

    let _signal_thread = thread::Builder::new()
        .name("signal_listener".to_string())
        .spawn({
            let os_input = os_input.clone();
            move || {
                os_input.handle_signals(
                    Box::new({
                        let os_api = os_input.clone();
                        move || {
                            os_api.send_to_server(ClientToServerMsg::TerminalResize(
                                os_api.get_terminal_size_using_fd(0),
                            ));
                        }
                    }),
                    Box::new({
                        let os_api = os_input.clone();
                        move || {
                            os_api.send_to_server(ClientToServerMsg::Action(on_force_close.into()));
                        }
                    }),
                );
            }
        })
        .unwrap();

    let router_thread = thread::Builder::new()
        .name("router".to_string())
        .spawn({
            let os_input = os_input.clone();
            let mut should_break = false;
            move || loop {
                let (instruction, err_ctx) = os_input.recv_from_server();
                err_ctx.update_thread_ctx();
                if let ServerToClientMsg::Exit(_) = instruction {
                    should_break = true;
                }
                send_client_instructions.send(instruction.into()).unwrap();
                if should_break {
                    break;
                }
            }
        })
        .unwrap();

    let handle_error = |backtrace: String| {
        os_input.unset_raw_mode(0);
        let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
        let restore_snapshot = "\u{1b}[?1049l";
        os_input.disable_mouse();
        let error = format!(
            "{}\n{}{}",
            goto_start_of_last_line, restore_snapshot, backtrace
        );
        let _ = os_input
            .get_stdout_writer()
            .write(error.as_bytes())
            .unwrap();
        std::process::exit(1);
    };

    let exit_msg: String;

    loop {
        let (client_instruction, mut err_ctx) = receive_client_instructions
            .recv()
            .expect("failed to receive app instruction on channel");

        err_ctx.add_call(ContextType::Client((&client_instruction).into()));
        match client_instruction {
            ClientInstruction::Exit(reason) => {
                os_input.send_to_server(ClientToServerMsg::ClientExited);

                if let ExitReason::Error(_) = reason {
                    handle_error(format!("{}", reason));
                }
                exit_msg = format!("{}", reason);
                break;
            }
            ClientInstruction::Error(backtrace) => {
                let _ = os_input.send_to_server(ClientToServerMsg::Action(Action::Quit));
                handle_error(backtrace);
            }
            ClientInstruction::Render(output) => {
                let mut stdout = os_input.get_stdout_writer();
                stdout
                    .write_all(output.as_bytes())
                    .expect("cannot write to stdout");
                stdout.flush().expect("could not flush");
            }
            ClientInstruction::UnblockInputThread => {
                command_is_executing.unblock_input_thread();
            }
        }
    }

    router_thread.join().unwrap();

    // cleanup();
    let reset_style = "\u{1b}[m";
    let show_cursor = "\u{1b}[?25h";
    let restore_snapshot = "\u{1b}[?1049l";
    let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
    let goodbye_message = format!(
        "{}\n{}{}{}{}\n",
        goto_start_of_last_line, restore_snapshot, reset_style, show_cursor, exit_msg
    );

    os_input.disable_mouse();
    info!("{}", exit_msg);
    os_input.unset_raw_mode(0);
    let mut stdout = os_input.get_stdout_writer();
    let _ = stdout.write(goodbye_message.as_bytes()).unwrap();
    stdout.flush().unwrap();
}

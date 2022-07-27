pub mod os_input_output;

mod command_is_executing;
pub mod fake_client;
mod input_handler;
mod sessions;
mod stdin_ansi_parser;
mod stdin_handler;

use log::info;
use std::env::current_exe;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::stdin_ansi_parser::{AnsiStdinInstruction, StdinAnsiParser};
use crate::{
    command_is_executing::CommandIsExecuting, input_handler::input_loop,
    os_input_output::ClientOsApi, stdin_handler::stdin_loop,
};
use zellij_utils::{
    channels::{self, ChannelWithContext, SenderWithContext},
    consts::ZELLIJ_IPC_PIPE,
    data::{ClientId, InputMode, Style},
    envs,
    errors::{ClientContext, ContextType, ErrorInstruction},
    input::{actions::Action, config::Config, options::Options},
    ipc::{ClientAttributes, ClientToServerMsg, ExitReason, ServerToClientMsg},
    termwiz::input::InputEvent,
};
use zellij_utils::{cli::CliArgs, input::layout::LayoutFromYaml};

/// Instructions related to the client-side application
#[derive(Debug, Clone)]
pub(crate) enum ClientInstruction {
    Error(String),
    Render(String),
    UnblockInputThread,
    Exit(ExitReason),
    SwitchToMode(InputMode),
    Connected,
    ActiveClients(Vec<ClientId>),
}

impl From<ServerToClientMsg> for ClientInstruction {
    fn from(instruction: ServerToClientMsg) -> Self {
        match instruction {
            ServerToClientMsg::Exit(e) => ClientInstruction::Exit(e),
            ServerToClientMsg::Render(buffer) => ClientInstruction::Render(buffer),
            ServerToClientMsg::UnblockInputThread => ClientInstruction::UnblockInputThread,
            ServerToClientMsg::SwitchToMode(input_mode) => {
                ClientInstruction::SwitchToMode(input_mode)
            },
            ServerToClientMsg::Connected => ClientInstruction::Connected,
            ServerToClientMsg::ActiveClients(clients) => ClientInstruction::ActiveClients(clients),
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
            ClientInstruction::SwitchToMode(_) => ClientContext::SwitchToMode,
            ClientInstruction::Connected => ClientContext::Connected,
            ClientInstruction::ActiveClients(_) => ClientContext::ActiveClients,
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
    Attach(String, Options),
    New(String),
}

impl ClientInfo {
    pub fn get_session_name(&self) -> &str {
        match self {
            Self::Attach(ref name, _) => name,
            Self::New(ref name) => name,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum InputInstruction {
    KeyEvent(InputEvent, Vec<u8>),
    SwitchToMode(InputMode),
    AnsiStdinInstructions(Vec<AnsiStdinInstruction>),
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
    let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
    let take_snapshot = "\u{1b}[?1049h";
    let bracketed_paste = "\u{1b}[?2004h";
    os_input.unset_raw_mode(0).unwrap();

    let _ = os_input
        .get_stdout_writer()
        .write(take_snapshot.as_bytes())
        .unwrap();
    let _ = os_input
        .get_stdout_writer()
        .write(clear_client_terminal_attributes.as_bytes())
        .unwrap();
    envs::set_zellij("0".to_string());
    config.env.set_vars();

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
        style: Style {
            colors: palette,
            rounded_corners: config.ui.unwrap_or_default().pane_frames.rounded_corners,
        },
        keybinds: config.keybinds.clone(),
    };

    let first_msg = match info {
        ClientInfo::Attach(name, config_options) => {
            envs::set_session_name(name);
            envs::set_initial_environment_vars();

            ClientToServerMsg::AttachClient(client_attributes, config_options)
        },
        ClientInfo::New(name) => {
            envs::set_session_name(name);
            envs::set_initial_environment_vars();

            spawn_server(&*ZELLIJ_IPC_PIPE).unwrap();

            ClientToServerMsg::NewClient(
                client_attributes,
                Box::new(opts),
                Box::new(config_options.clone()),
                Box::new(layout.unwrap()),
                Some(config.plugins.clone()),
            )
        },
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

    let (send_input_instructions, receive_input_instructions): ChannelWithContext<
        InputInstruction,
    > = channels::bounded(50);
    let send_input_instructions = SenderWithContext::new(send_input_instructions);

    std::panic::set_hook({
        use zellij_utils::errors::handle_panic;
        let send_client_instructions = send_client_instructions.clone();
        let os_input = os_input.clone();
        Box::new(move |info| {
            if let Ok(()) = os_input.unset_raw_mode(0) {
                handle_panic(info, &send_client_instructions);
            }
        })
    });

    let on_force_close = config_options.on_force_close.unwrap_or_default();
    let stdin_ansi_parser = Arc::new(Mutex::new(StdinAnsiParser::new()));

    let _stdin_thread = thread::Builder::new()
        .name("stdin_handler".to_string())
        .spawn({
            let os_input = os_input.clone();
            let send_input_instructions = send_input_instructions.clone();
            let stdin_ansi_parser = stdin_ansi_parser.clone();
            move || stdin_loop(os_input, send_input_instructions, stdin_ansi_parser)
        });

    let _input_thread = thread::Builder::new()
        .name("input_handler".to_string())
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
                    receive_input_instructions,
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
                            // send a query to the terminal emulator in case the font size changed
                            // as well - we'll parse the response through STDIN
                            let terminal_emulator_query_string = stdin_ansi_parser
                                .lock()
                                .unwrap()
                                .window_size_change_query_string();
                            let _ = os_api
                                .get_stdout_writer()
                                .write(terminal_emulator_query_string.as_bytes())
                                .unwrap();
                        }
                    }),
                    Box::new({
                        let os_api = os_input.clone();
                        move || {
                            os_api.send_to_server(ClientToServerMsg::Action(
                                on_force_close.into(),
                                None,
                            ));
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
                match os_input.recv_from_server() {
                    Some((instruction, err_ctx)) => {
                        err_ctx.update_thread_ctx();
                        if let ServerToClientMsg::Exit(_) = instruction {
                            should_break = true;
                        }
                        send_client_instructions.send(instruction.into()).unwrap();
                        if should_break {
                            break;
                        }
                    },
                    None => {
                        send_client_instructions
                            .send(ClientInstruction::UnblockInputThread)
                            .unwrap();
                        log::error!("Received empty message from server");
                        send_client_instructions
                            .send(ClientInstruction::Error(
                                "Received empty message from server".to_string(),
                            ))
                            .unwrap();
                        break;
                    },
                }
            }
        })
        .unwrap();

    let handle_error = |backtrace: String| {
        os_input.unset_raw_mode(0).unwrap();
        let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
        let restore_snapshot = "\u{1b}[?1049l";
        os_input.disable_mouse();
        let error = format!(
            "{}\n{}{}\n",
            restore_snapshot, goto_start_of_last_line, backtrace
        );
        let _ = os_input
            .get_stdout_writer()
            .write(error.as_bytes())
            .unwrap();
        let _ = os_input.get_stdout_writer().flush().unwrap();
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
                    handle_error(reason.to_string());
                }
                exit_msg = reason.to_string();
                break;
            },
            ClientInstruction::Error(backtrace) => {
                let _ = os_input.send_to_server(ClientToServerMsg::Action(Action::Quit, None));
                handle_error(backtrace);
            },
            ClientInstruction::Render(output) => {
                let mut stdout = os_input.get_stdout_writer();
                stdout
                    .write_all(output.as_bytes())
                    .expect("cannot write to stdout");
                stdout.flush().expect("could not flush");
            },
            ClientInstruction::UnblockInputThread => {
                command_is_executing.unblock_input_thread();
            },
            ClientInstruction::SwitchToMode(input_mode) => {
                send_input_instructions
                    .send(InputInstruction::SwitchToMode(input_mode))
                    .unwrap();
            },
            _ => {},
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
    os_input.unset_raw_mode(0).unwrap();
    let mut stdout = os_input.get_stdout_writer();
    let _ = stdout.write(goodbye_message.as_bytes()).unwrap();
    stdout.flush().unwrap();
}

#[cfg(test)]
#[path = "./unit/stdin_tests.rs"]
mod stdin_tests;

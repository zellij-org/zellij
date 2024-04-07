pub mod os_input_output;

pub mod cli_client;
mod command_is_executing;
mod input_handler;
pub mod old_config_converter;
mod stdin_ansi_parser;
mod stdin_handler;

use log::info;
use std::env::current_exe;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use zellij_utils::errors::FatalError;
use zellij_utils::ipc::ReceiveError;

use crate::stdin_ansi_parser::{AnsiStdinInstruction, StdinAnsiParser, SyncOutput};
use crate::{
    command_is_executing::CommandIsExecuting, input_handler::input_loop,
    os_input_output::ClientOsApi, stdin_handler::stdin_loop,
};
#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    DISABLE_NEWLINE_AUTO_RETURN, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
};
#[cfg(unix)]
use zellij_utils::consts::set_permissions;
use zellij_utils::pane_size::Size;
use zellij_utils::{
    channels::{self, ChannelWithContext, SenderWithContext},
    consts::ZELLIJ_SOCK_DIR,
    data::{ClientId, ConnectToSession, InputMode, Style},
    envs,
    errors::{ClientContext, ContextType, ErrorInstruction},
    input::{config::Config, options::Options},
    ipc::{ClientAttributes, ClientToServerMsg, ExitReason, ServerToClientMsg},
    termwiz::input::InputEvent,
};
use zellij_utils::{cli::CliArgs, input::layout::Layout};

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
    StartedParsingStdinQuery,
    DoneParsingStdinQuery,
    Log(Vec<String>),
    LogError(Vec<String>),
    SwitchSession(ConnectToSession),
    SetSynchronizedOutput(Option<SyncOutput>),
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
            ServerToClientMsg::Log(log_lines) => ClientInstruction::Log(log_lines),
            ServerToClientMsg::LogError(log_lines) => ClientInstruction::LogError(log_lines),
            ServerToClientMsg::SwitchSession(connect_to_session) => {
                ClientInstruction::SwitchSession(connect_to_session)
            },
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
            ClientInstruction::Log(_) => ClientContext::Log,
            ClientInstruction::LogError(_) => ClientContext::LogError,
            ClientInstruction::StartedParsingStdinQuery => ClientContext::StartedParsingStdinQuery,
            ClientInstruction::DoneParsingStdinQuery => ClientContext::DoneParsingStdinQuery,
            ClientInstruction::SwitchSession(..) => ClientContext::SwitchSession,
            ClientInstruction::SetSynchronizedOutput(..) => ClientContext::SetSynchronisedOutput,
        }
    }
}

impl ErrorInstruction for ClientInstruction {
    fn error(err: String) -> Self {
        ClientInstruction::Error(err)
    }
}

fn spawn_server(socket_path: &Path, debug: bool) -> io::Result<()> {
    let mut cmd = Command::new(current_exe()?);
    cmd.arg("--server");
    cmd.arg(socket_path);
    if debug {
        cmd.arg("--debug");
    }
    #[cfg(unix)]
    {
        let status = cmd.status()?;

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
    #[cfg(windows)]
    {
        let _status = cmd.spawn()?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum ClientInfo {
    Attach(String, Options),
    New(String),
    Resurrect(String, Layout),
}

impl ClientInfo {
    pub fn get_session_name(&self) -> &str {
        match self {
            Self::Attach(ref name, _) => name,
            Self::New(ref name) => name,
            Self::Resurrect(ref name, _) => name,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum InputInstruction {
    KeyEvent(InputEvent, Vec<u8>),
    SwitchToMode(InputMode),
    AnsiStdinInstructions(Vec<AnsiStdinInstruction>),
    StartedParsing,
    DoneParsing,
    Exit,
}

pub fn start_client(
    mut os_input: Box<dyn ClientOsApi>,
    opts: CliArgs,
    config: Config,
    config_options: Options,
    info: ClientInfo,
    layout: Option<Layout>,
    tab_position_to_focus: Option<usize>,
    pane_id_to_focus: Option<(u32, bool)>, // (pane_id, is_plugin)
    is_a_reconnect: bool,
) -> Option<ConnectToSession> {
    info!("Starting Zellij client!");

    let mut reconnect_to_session = None;
    let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
    let take_snapshot = "\u{1b}[?1049h";
    let bracketed_paste = "\u{1b}[?2004h";
    #[cfg(unix)]
    os_input.unset_raw_mode(0).unwrap();

    if !is_a_reconnect {
        // we don't do this for a reconnect because our controlling terminal already has the
        // attributes we want from it, and some terminals don't treat these atomically (looking at
        // your Windows Terminal...)
        let _ = os_input
            .get_stdout_writer()
            .write(take_snapshot.as_bytes())
            .unwrap();
        let _ = os_input
            .get_stdout_writer()
            .write(clear_client_terminal_attributes.as_bytes())
            .unwrap();
    }
    envs::set_zellij("0".to_string());
    config.env.set_vars();

    let palette = config
        .theme_config(&config_options)
        .unwrap_or_else(|| os_input.load_palette());

    let full_screen_ws = os_input.get_terminal_size(os_input_output::HandleType::Stdout);

    let client_attributes = ClientAttributes {
        size: full_screen_ws,
        style: Style {
            colors: palette,
            rounded_corners: config.ui.pane_frames.rounded_corners,
            hide_session_name: config.ui.pane_frames.hide_session_name,
        },
        keybinds: config.keybinds.clone(),
    };

    // TODO: Windows compatibility: Change ipc pipe to something cross platform instead of file
    let create_ipc_pipe = || -> std::path::PathBuf {
        let mut sock_dir = ZELLIJ_SOCK_DIR.clone();
        std::fs::create_dir_all(&sock_dir).unwrap();
        #[cfg(unix)]
        set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(envs::get_session_name().unwrap());
        sock_dir
    };

    let (first_msg, ipc_pipe) = match info {
        ClientInfo::Attach(name, config_options) => {
            envs::set_session_name(name.clone());
            os_input.update_session_name(name);
            let ipc_pipe = create_ipc_pipe();

            (
                ClientToServerMsg::AttachClient(
                    client_attributes,
                    config_options,
                    tab_position_to_focus,
                    pane_id_to_focus,
                ),
                ipc_pipe,
            )
        },
        ClientInfo::New(name) | ClientInfo::Resurrect(name, _) => {
            envs::set_session_name(name.clone());
            os_input.update_session_name(name);
            let ipc_pipe = create_ipc_pipe();

            spawn_server(&*ipc_pipe, opts.debug).unwrap();

            (
                ClientToServerMsg::NewClient(
                    client_attributes,
                    Box::new(opts),
                    Box::new(config_options.clone()),
                    Box::new(layout.unwrap()),
                    Some(config.plugins.clone()),
                ),
                ipc_pipe,
            )
        },
    };

    os_input.connect_to_server(&*ipc_pipe);
    os_input.send_to_server(first_msg);

    let mut command_is_executing = CommandIsExecuting::new();

    os_input.set_raw_mode(
        windows_sys::Win32::System::Console::STD_INPUT_HANDLE,
        ENABLE_VIRTUAL_TERMINAL_INPUT,
        ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT,
    );
    os_input.set_raw_mode(
        windows_sys::Win32::System::Console::STD_OUTPUT_HANDLE,
        ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        0,
    ); // & DISABLE_NEWLINE_AUTO_RETURN);
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
            #[cfg(unix)]
            if let Ok(()) = os_input.unset_raw_mode(0) {
                handle_panic(info, &send_client_instructions);
            }

            #[cfg(windows)]
            handle_panic(info, &send_client_instructions);
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
                                os_api.get_terminal_size(os_input_output::HandleType::Stdin),
                            ));
                        }
                    }),
                    Box::new({
                        let os_api = os_input.clone();
                        move || {
                            os_api.send_to_server(ClientToServerMsg::Action(
                                on_force_close.into(),
                                None,
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
                    Ok((instruction, err_ctx)) => {
                        err_ctx.update_thread_ctx();
                        if let ServerToClientMsg::Exit(_) = instruction {
                            should_break = true;
                        }
                        send_client_instructions.send(instruction.into()).unwrap();
                        if should_break {
                            break;
                        }
                    },
                    Err(ReceiveError::Disconnected) => {
                        log::error!("Pipe is disconnected");
                        break;
                    },
                    Err(ReceiveError::ReceiverLocked) => {
                        // TODO enable the lock to be returned
                        thread::sleep(std::time::Duration::from_secs_f32(0.1));
                        continue;
                    },
                    Err(e) => {
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
        #[cfg(unix)]
        os_input.unset_raw_mode(0).unwrap();
        let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
        let restore_snapshot = "\u{1b}[?1049l";
        os_input.disable_mouse().non_fatal();
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

    let mut exit_msg = String::new();
    let mut loading = true;
    let mut pending_instructions = vec![];
    let mut synchronised_output = match os_input.env_variable("TERM").as_deref() {
        Some("alacritty") => Some(SyncOutput::DCS),
        _ => None,
    };

    let mut stdout = os_input.get_stdout_writer();
    stdout
        .write_all("\u{1b}[1m\u{1b}[HLoading Zellij\u{1b}[m\n\r".as_bytes())
        .expect("cannot write to stdout");
    stdout.flush().expect("could not flush");

    loop {
        let (client_instruction, mut err_ctx) = if !loading && !pending_instructions.is_empty() {
            // there are buffered instructions, we need to go through them before processing the
            // new ones
            pending_instructions.remove(0)
        } else {
            receive_client_instructions
                .recv()
                .expect("failed to receive app instruction on channel")
        };

        if loading {
            // when the app is still loading, we buffer instructions and show a loading screen
            match client_instruction {
                ClientInstruction::StartedParsingStdinQuery => {
                    stdout
                        .write_all("Querying terminal emulator for \u{1b}[32;1mdefault colors\u{1b}[m and \u{1b}[32;1mpixel/cell\u{1b}[m ratio...".as_bytes())
                        .expect("cannot write to stdout");
                    stdout.flush().expect("could not flush");
                },
                ClientInstruction::DoneParsingStdinQuery => {
                    stdout
                        .write_all("done".as_bytes())
                        .expect("cannot write to stdout");
                    stdout.flush().expect("could not flush");
                    loading = false;
                },
                instruction => {
                    pending_instructions.push((instruction, err_ctx));
                },
            }
            continue;
        }

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
                handle_error(backtrace);
            },
            ClientInstruction::Render(output) => {
                let mut stdout = os_input.get_stdout_writer();
                if let Some(sync) = synchronised_output {
                    stdout
                        .write_all(sync.start_seq())
                        .expect("cannot write to stdout");
                }
                stdout
                    .write_all(output.as_bytes())
                    .expect("cannot write to stdout");
                if let Some(sync) = synchronised_output {
                    stdout
                        .write_all(sync.end_seq())
                        .expect("cannot write to stdout");
                }
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
            ClientInstruction::Log(lines_to_log) => {
                for line in lines_to_log {
                    log::info!("{line}");
                }
            },
            ClientInstruction::LogError(lines_to_log) => {
                for line in lines_to_log {
                    log::error!("{line}");
                }
            },
            ClientInstruction::SwitchSession(connect_to_session) => {
                reconnect_to_session = Some(connect_to_session);
                os_input.send_to_server(ClientToServerMsg::ClientExited);
                break;
            },
            ClientInstruction::SetSynchronizedOutput(enabled) => {
                synchronised_output = enabled;
            },
            _ => {},
        }
    }

    router_thread.join().unwrap();

    if reconnect_to_session.is_none() {
        let reset_style = "\u{1b}[m";
        let show_cursor = "\u{1b}[?25h";
        let restore_snapshot = "\u{1b}[?1049l";
        let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
        let goodbye_message = format!(
            "{}\n{}{}{}{}\n",
            goto_start_of_last_line, restore_snapshot, reset_style, show_cursor, exit_msg
        );

        os_input.disable_mouse().non_fatal();
        info!("{}", exit_msg);
        #[cfg(unix)]
        os_input.unset_raw_mode(0).unwrap();
        let mut stdout = os_input.get_stdout_writer();
        let _ = stdout.write(goodbye_message.as_bytes()).unwrap();
        stdout.flush().unwrap();
    } else {
        let clear_screen = "\u{1b}[2J";
        let mut stdout = os_input.get_stdout_writer();
        let _ = stdout.write(clear_screen.as_bytes()).unwrap();
        stdout.flush().unwrap();
    }

    let _ = send_input_instructions.send(InputInstruction::Exit);

    reconnect_to_session
}

#[cfg(test)]
#[path = "./unit/stdin_tests.rs"]
mod stdin_tests;

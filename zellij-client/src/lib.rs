pub mod os_input_output;

pub mod cli_client;
mod command_is_executing;
mod input_handler;
mod keyboard_parser;
pub mod old_config_converter;
mod stdin_ansi_parser;
mod stdin_handler;
#[cfg(feature = "web_server_capability")]
pub mod web_client;

use log::info;
use std::env::current_exe;
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use zellij_utils::errors::FatalError;
use zellij_utils::shared::web_server_base_url;

use crate::stdin_ansi_parser::{AnsiStdinInstruction, StdinAnsiParser, SyncOutput};
use crate::{
    command_is_executing::CommandIsExecuting, input_handler::input_loop,
    os_input_output::ClientOsApi, stdin_handler::stdin_loop,
};
use termwiz::input::InputEvent;
use zellij_utils::{
    channels::{self, ChannelWithContext, SenderWithContext},
    consts::{set_permissions, ZELLIJ_SOCK_DIR},
    data::{ClientId, ConnectToSession, KeyWithModifier, Style},
    envs,
    errors::{ClientContext, ContextType, ErrorInstruction},
    input::{
        config::{watch_config_file_changes, Config},
        options::Options,
    },
    ipc::{ClientAttributes, ClientToServerMsg, ExitReason, ServerToClientMsg},
    pane_size::Size,
};
use zellij_utils::{cli::CliArgs, input::layout::Layout};

/// Instructions related to the client-side application
#[derive(Debug, Clone)]
pub(crate) enum ClientInstruction {
    Error(String),
    Render(String),
    UnblockInputThread,
    Exit(ExitReason),
    Connected,
    StartedParsingStdinQuery,
    DoneParsingStdinQuery,
    Log(Vec<String>),
    LogError(Vec<String>),
    SwitchSession(ConnectToSession),
    SetSynchronizedOutput(Option<SyncOutput>),
    UnblockCliPipeInput(()), // String -> pipe name
    CliPipeOutput((), ()),   // String -> pipe name, String -> output
    QueryTerminalSize,
    WriteConfigToDisk {
        config: String,
    },
    StartWebServer,
    #[allow(dead_code)] // we need the session name here even though we're not currently using it
    RenamedSession(String), // String -> new session name
}

impl From<ServerToClientMsg> for ClientInstruction {
    fn from(instruction: ServerToClientMsg) -> Self {
        match instruction {
            ServerToClientMsg::Exit(e) => ClientInstruction::Exit(e),
            ServerToClientMsg::Render(buffer) => ClientInstruction::Render(buffer),
            ServerToClientMsg::UnblockInputThread => ClientInstruction::UnblockInputThread,
            ServerToClientMsg::Connected => ClientInstruction::Connected,
            ServerToClientMsg::Log(log_lines) => ClientInstruction::Log(log_lines),
            ServerToClientMsg::LogError(log_lines) => ClientInstruction::LogError(log_lines),
            ServerToClientMsg::SwitchSession(connect_to_session) => {
                ClientInstruction::SwitchSession(connect_to_session)
            },
            ServerToClientMsg::UnblockCliPipeInput(_pipe_name) => {
                ClientInstruction::UnblockCliPipeInput(())
            },
            ServerToClientMsg::CliPipeOutput(_pipe_name, _output) => {
                ClientInstruction::CliPipeOutput((), ())
            },
            ServerToClientMsg::QueryTerminalSize => ClientInstruction::QueryTerminalSize,
            ServerToClientMsg::WriteConfigToDisk { config } => {
                ClientInstruction::WriteConfigToDisk { config }
            },
            ServerToClientMsg::StartWebServer => ClientInstruction::StartWebServer,
            ServerToClientMsg::RenamedSession(name) => ClientInstruction::RenamedSession(name),
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
            ClientInstruction::Connected => ClientContext::Connected,
            ClientInstruction::Log(_) => ClientContext::Log,
            ClientInstruction::LogError(_) => ClientContext::LogError,
            ClientInstruction::StartedParsingStdinQuery => ClientContext::StartedParsingStdinQuery,
            ClientInstruction::DoneParsingStdinQuery => ClientContext::DoneParsingStdinQuery,
            ClientInstruction::SwitchSession(..) => ClientContext::SwitchSession,
            ClientInstruction::SetSynchronizedOutput(..) => ClientContext::SetSynchronisedOutput,
            ClientInstruction::UnblockCliPipeInput(..) => ClientContext::UnblockCliPipeInput,
            ClientInstruction::CliPipeOutput(..) => ClientContext::CliPipeOutput,
            ClientInstruction::QueryTerminalSize => ClientContext::QueryTerminalSize,
            ClientInstruction::WriteConfigToDisk { .. } => ClientContext::WriteConfigToDisk,
            ClientInstruction::StartWebServer => ClientContext::StartWebServer,
            ClientInstruction::RenamedSession(..) => ClientContext::RenamedSession,
        }
    }
}

impl ErrorInstruction for ClientInstruction {
    fn error(err: String) -> Self {
        ClientInstruction::Error(err)
    }
}

#[cfg(feature = "web_server_capability")]
fn spawn_web_server(opts: &CliArgs) -> Result<String, String> {
    let mut cmd = Command::new(current_exe().map_err(|e| e.to_string())?);
    if let Some(config_file_path) = Config::config_file_path(opts) {
        let config_file_path_exists = Path::new(&config_file_path).exists();
        if !config_file_path_exists {
            return Err(format!(
                "Config file: {} does not exist",
                config_file_path.display()
            ));
        }
        // this is so that if Zellij itself was started with a different config file, we'll use it
        // to start the webserver
        cmd.arg("--config");
        cmd.arg(format!("{}", config_file_path.display()));
    }
    cmd.arg("web");
    cmd.arg("-d");
    let output = cmd.output();
    match output {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).to_string())
            }
        },
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(not(feature = "web_server_capability"))]
fn spawn_web_server(_opts: &CliArgs) -> Result<String, String> {
    log::error!(
        "This version of Zellij was compiled without web server support, cannot run web server!"
    );
    Ok("".to_owned())
}

pub fn spawn_server(socket_path: &Path, debug: bool) -> io::Result<()> {
    let mut cmd = Command::new(current_exe()?);
    cmd.arg("--server");
    cmd.arg(socket_path);
    if debug {
        cmd.arg("--debug");
    }
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
    KeyWithModifierEvent(KeyWithModifier, Vec<u8>),
    AnsiStdinInstructions(Vec<AnsiStdinInstruction>),
    StartedParsing,
    DoneParsing,
    Exit,
}

pub fn start_client(
    mut os_input: Box<dyn ClientOsApi>,
    opts: CliArgs,
    config: Config,          // saved to disk (or default?)
    config_options: Options, // CLI options merged into (getting priority over) saved config options
    info: ClientInfo,
    layout: Option<Layout>,
    tab_position_to_focus: Option<usize>,
    pane_id_to_focus: Option<(u32, bool)>, // (pane_id, is_plugin)
    is_a_reconnect: bool,
    start_detached_and_exit: bool,
    layout_is_welcome_screen: bool,
) -> Option<ConnectToSession> {
    if start_detached_and_exit {
        start_server_detached(os_input, opts, config, config_options, info, layout);
        return None;
    }
    info!("Starting Zellij client!");

    let explicitly_disable_kitty_keyboard_protocol = config_options
        .support_kitty_keyboard_protocol
        .map(|e| !e)
        .unwrap_or(false);
    let should_start_web_server = config_options.web_server.map(|w| w).unwrap_or(false);
    let mut reconnect_to_session = None;
    let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
    let take_snapshot = "\u{1b}[?1049h";
    let bracketed_paste = "\u{1b}[?2004h";
    let enter_kitty_keyboard_mode = "\u{1b}[>1u";
    os_input.unset_raw_mode(0).unwrap();

    if !is_a_reconnect {
        // we don't do this for a reconnect because our controlling terminal already has the
        // attributes we want from it, and some terminals don't treat these atomically (looking at
        // you Windows Terminal...)
        let _ = os_input
            .get_stdout_writer()
            .write(take_snapshot.as_bytes())
            .unwrap();
        let _ = os_input
            .get_stdout_writer()
            .write(clear_client_terminal_attributes.as_bytes())
            .unwrap();
        if !explicitly_disable_kitty_keyboard_protocol {
            let _ = os_input
                .get_stdout_writer()
                .write(enter_kitty_keyboard_mode.as_bytes())
                .unwrap();
        }
    }
    envs::set_zellij("0".to_string());
    config.env.set_vars();

    let palette = config
        .theme_config(config_options.theme.as_ref())
        .unwrap_or_else(|| os_input.load_palette().into());

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    let client_attributes = ClientAttributes {
        size: full_screen_ws,
        style: Style {
            colors: palette,
            rounded_corners: config.ui.pane_frames.rounded_corners,
            hide_session_name: config.ui.pane_frames.hide_session_name,
        },
    };
    let web_server_ip = config_options
        .web_server_ip
        .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    let web_server_port = config_options.web_server_port.unwrap_or_else(|| 8082);
    let has_certificate =
        config_options.web_server_cert.is_some() && config_options.web_server_key.is_some();
    let enforce_https_for_localhost = config_options.enforce_https_for_localhost.unwrap_or(false);

    let create_ipc_pipe = || -> std::path::PathBuf {
        let mut sock_dir = ZELLIJ_SOCK_DIR.clone();
        std::fs::create_dir_all(&sock_dir).unwrap();
        set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(envs::get_session_name().unwrap());
        sock_dir
    };

    let (first_msg, ipc_pipe) = match info {
        ClientInfo::Attach(name, config_options) => {
            envs::set_session_name(name.clone());
            os_input.update_session_name(name);
            let ipc_pipe = create_ipc_pipe();
            let is_web_client = false;

            (
                ClientToServerMsg::AttachClient(
                    client_attributes,
                    config.clone(),
                    config_options.clone(),
                    tab_position_to_focus,
                    pane_id_to_focus,
                    is_web_client,
                ),
                ipc_pipe,
            )
        },
        ClientInfo::New(name) | ClientInfo::Resurrect(name, _) => {
            envs::set_session_name(name.clone());
            os_input.update_session_name(name);
            let ipc_pipe = create_ipc_pipe();

            spawn_server(&*ipc_pipe, opts.debug).unwrap();
            if should_start_web_server {
                if let Err(e) = spawn_web_server(&opts) {
                    log::error!("Failed to start web server: {}", e);
                }
            }

            let successfully_written_config =
                Config::write_config_to_disk_if_it_does_not_exist(config.to_string(true), &opts);
            // if we successfully wrote the config to disk, it means two things:
            // 1. It did not exist beforehand
            // 2. The config folder is writeable
            //
            // If these two are true, we should launch the setup wizard, if even one of them is
            // false, we should never launch it.
            let should_launch_setup_wizard = successfully_written_config;
            let is_web_client = false;

            (
                ClientToServerMsg::NewClient(
                    client_attributes,
                    Box::new(opts.clone()),
                    Box::new(config.clone()),
                    Box::new(config_options.clone()),
                    Box::new(layout.unwrap()),
                    Box::new(config.plugins.clone()),
                    is_web_client,
                    should_launch_setup_wizard,
                    layout_is_welcome_screen,
                ),
                ipc_pipe,
            )
        },
    };

    os_input.connect_to_server(&*ipc_pipe);
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
            move || {
                stdin_loop(
                    os_input,
                    send_input_instructions,
                    stdin_ansi_parser,
                    explicitly_disable_kitty_keyboard_protocol,
                )
            }
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
            let opts = opts.clone();
            move || {
                report_changes_in_config_file(&opts, &os_input);
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
            ClientInstruction::QueryTerminalSize => {
                os_input.send_to_server(ClientToServerMsg::TerminalResize(
                    os_input.get_terminal_size_using_fd(0),
                ));
            },
            ClientInstruction::WriteConfigToDisk { config } => {
                match Config::write_config_to_disk(config, &opts) {
                    Ok(written_config) => {
                        let _ = os_input
                            .send_to_server(ClientToServerMsg::ConfigWrittenToDisk(written_config));
                    },
                    Err(e) => {
                        let error_path = e
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(String::new);
                        log::error!("Failed to write config to disk: {}", error_path);
                        let _ = os_input
                            .send_to_server(ClientToServerMsg::FailedToWriteConfigToDisk(e));
                    },
                }
            },
            ClientInstruction::StartWebServer => {
                let web_server_base_url = web_server_base_url(
                    web_server_ip,
                    web_server_port,
                    has_certificate,
                    enforce_https_for_localhost,
                );
                match spawn_web_server(&opts) {
                    Ok(_) => {
                        let _ = os_input.send_to_server(ClientToServerMsg::WebServerStarted(
                            web_server_base_url,
                        ));
                    },
                    Err(e) => {
                        log::error!("Failed to start web_server: {}", e);
                        let _ =
                            os_input.send_to_server(ClientToServerMsg::FailedToStartWebServer(e));
                    },
                }
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
        os_input.unset_raw_mode(0).unwrap();
        let mut stdout = os_input.get_stdout_writer();
        let exit_kitty_keyboard_mode = "\u{1b}[<1u";
        if !explicitly_disable_kitty_keyboard_protocol {
            let _ = stdout.write(exit_kitty_keyboard_mode.as_bytes()).unwrap();
            stdout.flush().unwrap();
        }
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

pub fn start_server_detached(
    mut os_input: Box<dyn ClientOsApi>,
    opts: CliArgs,
    config: Config,
    config_options: Options,
    info: ClientInfo,
    layout: Option<Layout>,
) {
    envs::set_zellij("0".to_string());
    config.env.set_vars();

    let should_start_web_server = config_options.web_server.map(|w| w).unwrap_or(false);

    let palette = config
        .theme_config(config_options.theme.as_ref())
        .unwrap_or_else(|| os_input.load_palette().into());

    let client_attributes = ClientAttributes {
        size: Size { rows: 50, cols: 50 }, // just so size is not 0, it doesn't matter because we
        // immediately detach
        style: Style {
            colors: palette,
            rounded_corners: config.ui.pane_frames.rounded_corners,
            hide_session_name: config.ui.pane_frames.hide_session_name,
        },
    };

    let create_ipc_pipe = || -> std::path::PathBuf {
        let mut sock_dir = ZELLIJ_SOCK_DIR.clone();
        std::fs::create_dir_all(&sock_dir).unwrap();
        set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(envs::get_session_name().unwrap());
        sock_dir
    };

    let (first_msg, ipc_pipe) = match info {
        ClientInfo::New(name) | ClientInfo::Resurrect(name, _) => {
            envs::set_session_name(name.clone());
            os_input.update_session_name(name);
            let ipc_pipe = create_ipc_pipe();

            spawn_server(&*ipc_pipe, opts.debug).unwrap();
            if should_start_web_server {
                if let Err(e) = spawn_web_server(&opts) {
                    log::error!("Failed to start web server: {}", e);
                }
            }
            let should_launch_setup_wizard = false; // no setup wizard when starting a detached
                                                    // server
            let is_web_client = false;

            (
                ClientToServerMsg::NewClient(
                    client_attributes,
                    Box::new(opts),
                    Box::new(config.clone()),
                    Box::new(config_options.clone()),
                    Box::new(layout.unwrap()),
                    Box::new(config.plugins.clone()),
                    is_web_client,
                    should_launch_setup_wizard,
                    false,
                ),
                ipc_pipe,
            )
        },
        _ => {
            eprintln!("Session already exists");
            std::process::exit(1);
        },
    };

    os_input.connect_to_server(&*ipc_pipe);
    os_input.send_to_server(first_msg);
}

pub fn report_changes_in_config_file(opts: &CliArgs, os_input: &Box<dyn ClientOsApi>) {
    if let Some(config_file_path) = Config::config_file_path(&opts) {
        let os_input = os_input.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                watch_config_file_changes(config_file_path, move |new_config| {
                    let os_input = os_input.clone();
                    async move {
                        os_input.send_to_server(ClientToServerMsg::ConfigWrittenToDisk(new_config));
                    }
                })
                .await;
            });
        });
    }
}

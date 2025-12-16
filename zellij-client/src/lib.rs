pub mod os_input_output;

pub mod cli_client;
mod command_is_executing;
mod input_handler;
mod keyboard_parser;
pub mod old_config_converter;
#[cfg(feature = "web_server_capability")]
pub mod remote_attach;
mod stdin_ansi_parser;
mod stdin_handler;
#[cfg(feature = "web_server_capability")]
pub mod web_client;

use log::info;
use std::env::current_exe;
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use zellij_utils::errors::FatalError;
use zellij_utils::shared::web_server_base_url;

#[cfg(feature = "web_server_capability")]
use futures_util::{SinkExt, StreamExt};
#[cfg(feature = "web_server_capability")]
use tokio::runtime::Runtime;
#[cfg(feature = "web_server_capability")]
use tokio_tungstenite::tungstenite::Message;

#[cfg(feature = "web_server_capability")]
use crate::web_client::control_message::{
    WebClientToWebServerControlMessage, WebClientToWebServerControlMessagePayload,
    WebServerToWebClientControlMessage,
};

#[derive(Debug)]
pub enum RemoteClientError {
    InvalidAuthToken,
    SessionTokenExpired,
    Unauthorized,
    ConnectionFailed(String),
    UrlParseError(url::ParseError),
    IoError(std::io::Error),
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl std::fmt::Display for RemoteClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteClientError::InvalidAuthToken => write!(f, "Invalid authentication token"),
            RemoteClientError::SessionTokenExpired => write!(f, "Session token expired"),
            RemoteClientError::Unauthorized => write!(f, "Unauthorized"),
            RemoteClientError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            RemoteClientError::UrlParseError(e) => write!(f, "Invalid URL: {}", e),
            RemoteClientError::IoError(e) => write!(f, "IO error: {}", e),
            RemoteClientError::Other(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for RemoteClientError {}

impl From<url::ParseError> for RemoteClientError {
    fn from(error: url::ParseError) -> Self {
        RemoteClientError::UrlParseError(error)
    }
}

impl From<std::io::Error> for RemoteClientError {
    fn from(error: std::io::Error) -> Self {
        RemoteClientError::IoError(error)
    }
}

use crate::stdin_ansi_parser::{AnsiStdinInstruction, StdinAnsiParser, SyncOutput};
use crate::{
    command_is_executing::CommandIsExecuting, input_handler::input_loop,
    os_input_output::ClientOsApi, stdin_handler::stdin_loop,
};
use termwiz::input::InputEvent;
use zellij_utils::cli::CliArgs;
use zellij_utils::{
    channels::{self, ChannelWithContext, SenderWithContext},
    consts::{set_permissions, ZELLIJ_SOCK_DIR},
    data::{ClientId, ConnectToSession, KeyWithModifier, LayoutInfo},
    envs,
    errors::{ClientContext, ContextType, ErrorInstruction},
    input::{cli_assets::CliAssets, config::Config, options::Options},
    ipc::{ClientToServerMsg, ExitReason, ServerToClientMsg},
    pane_size::Size,
};

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
    StartWebServer,
    #[allow(dead_code)] // we need the session name here even though we're not currently using it
    RenamedSession(String), // String -> new session name
    ConfigFileUpdated,
}

impl From<ServerToClientMsg> for ClientInstruction {
    fn from(instruction: ServerToClientMsg) -> Self {
        match instruction {
            ServerToClientMsg::Exit { exit_reason } => ClientInstruction::Exit(exit_reason),
            ServerToClientMsg::Render { content } => ClientInstruction::Render(content),
            ServerToClientMsg::UnblockInputThread => ClientInstruction::UnblockInputThread,
            ServerToClientMsg::Connected => ClientInstruction::Connected,
            ServerToClientMsg::Log { lines } => ClientInstruction::Log(lines),
            ServerToClientMsg::LogError { lines } => ClientInstruction::LogError(lines),
            ServerToClientMsg::SwitchSession { connect_to_session } => {
                ClientInstruction::SwitchSession(connect_to_session)
            },
            ServerToClientMsg::UnblockCliPipeInput { .. } => {
                ClientInstruction::UnblockCliPipeInput(())
            },
            ServerToClientMsg::CliPipeOutput { .. } => ClientInstruction::CliPipeOutput((), ()),
            ServerToClientMsg::QueryTerminalSize => ClientInstruction::QueryTerminalSize,
            ServerToClientMsg::StartWebServer => ClientInstruction::StartWebServer,
            ServerToClientMsg::RenamedSession { name } => ClientInstruction::RenamedSession(name),
            ServerToClientMsg::ConfigFileUpdated => ClientInstruction::ConfigFileUpdated,
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
            ClientInstruction::StartWebServer => ClientContext::StartWebServer,
            ClientInstruction::RenamedSession(..) => ClientContext::RenamedSession,
            ClientInstruction::ConfigFileUpdated => ClientContext::ConfigFileUpdated,
        }
    }
}

impl ErrorInstruction for ClientInstruction {
    fn error(err: String) -> Self {
        ClientInstruction::Error(err)
    }
}

#[cfg(feature = "web_server_capability")]
fn spawn_web_server(cli_args: &CliArgs) -> Result<String, String> {
    let mut cmd = Command::new(current_exe().map_err(|e| e.to_string())?);
    if let Some(config_file_path) = Config::config_file_path(cli_args) {
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
fn spawn_web_server(_cli_args: &CliArgs) -> Result<String, String> {
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
    New(String, Option<LayoutInfo>, Option<PathBuf>), // PathBuf -> explicit cwd
    Resurrect(String, PathBuf, bool, Option<PathBuf>), // (name, path_to_layout, force_run_commands, cwd)
    Watch(String, Options),                            // Watch mode (read-only)
}

impl ClientInfo {
    pub fn get_session_name(&self) -> &str {
        match self {
            Self::Attach(ref name, _) => name,
            Self::New(ref name, _layout_info, _layout_cwd) => name,
            Self::Resurrect(ref name, _, _, _) => name,
            Self::Watch(ref name, _) => name,
        }
    }
    pub fn set_layout_info(&mut self, new_layout_info: LayoutInfo) {
        match self {
            ClientInfo::New(_, layout_info, _) => *layout_info = Some(new_layout_info),
            _ => {},
        }
    }
    pub fn set_cwd(&mut self, new_cwd: PathBuf) {
        match self {
            ClientInfo::New(_, _, cwd) => *cwd = Some(new_cwd),
            ClientInfo::Resurrect(_, _, _, cwd) => *cwd = Some(new_cwd),
            _ => {},
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

#[cfg(feature = "web_server_capability")]
pub async fn run_remote_client_terminal_loop(
    os_input: Box<dyn ClientOsApi>,
    mut connections: remote_attach::WebSocketConnections,
) -> Result<Option<ConnectToSession>, RemoteClientError> {
    use crate::os_input_output::{AsyncSignals, AsyncStdin};

    let synchronised_output = match os_input.env_variable("TERM").as_deref() {
        Some("alacritty") => Some(SyncOutput::DCS),
        _ => None,
    };

    let mut async_stdin: Box<dyn AsyncStdin> = os_input.get_async_stdin_reader();
    let mut async_signals: Box<dyn AsyncSignals> = os_input
        .get_async_signal_listener()
        .map_err(|e| RemoteClientError::IoError(e))?;

    let create_resize_message = |size: Size| {
        Message::Text(
            serde_json::to_string(&WebClientToWebServerControlMessage {
                web_client_id: connections.web_client_id.clone(),
                payload: WebClientToWebServerControlMessagePayload::TerminalResize(size),
            })
            .unwrap(),
        )
    };

    // send size on startup
    let new_size = os_input.get_terminal_size_using_fd(0);
    if let Err(e) = connections
        .control_ws
        .send(create_resize_message(new_size))
        .await
    {
        log::error!("Failed to send resize message: {}", e);
    }

    loop {
        tokio::select! {
            // Handle stdin input
            result = async_stdin.read() => {
                match result {
                    Ok(buf) if !buf.is_empty() => {
                        if let Err(e) = connections.terminal_ws.send(Message::Binary(buf)).await {
                            log::error!("Failed to send stdin to terminal WebSocket: {}", e);
                            break;
                        }
                    }
                    Ok(_) => {
                        // Empty buffer means EOF
                        break;
                    }
                    Err(e) => {
                        log::error!("Error reading from stdin: {}", e);
                        break;
                    }
                }
            }

            // Handle signals
            Some(signal) = async_signals.recv() => {
                match signal {
                    crate::os_input_output::SignalEvent::Resize => {
                        let new_size = os_input.get_terminal_size_using_fd(0);
                        if let Err(e) = connections.control_ws.send(create_resize_message(new_size)).await {
                            log::error!("Failed to send resize message: {}", e);
                            break;
                        }
                    }
                    crate::os_input_output::SignalEvent::Quit => {
                        break;
                    }
                }
            }

            // Handle terminal messages
            terminal_msg = connections.terminal_ws.next() => {
                match terminal_msg {
                    Some(Ok(Message::Text(text))) => {
                        let mut stdout = os_input.get_stdout_writer();
                        if let Some(sync) = synchronised_output {
                            stdout
                                .write_all(sync.start_seq())
                                .expect("cannot write to stdout");
                        }
                        stdout
                            .write_all(text.as_bytes())
                            .expect("cannot write to stdout");
                        if let Some(sync) = synchronised_output {
                            stdout
                                .write_all(sync.end_seq())
                                .expect("cannot write to stdout");
                        }
                        stdout.flush().expect("could not flush");
                    }
                    Some(Ok(Message::Binary(data))) => {
                        let mut stdout = os_input.get_stdout_writer();
                        if let Some(sync) = synchronised_output {
                            stdout
                                .write_all(sync.start_seq())
                                .expect("cannot write to stdout");
                        }
                        stdout
                            .write_all(&data)
                            .expect("cannot write to stdout");
                        if let Some(sync) = synchronised_output {
                            stdout
                                .write_all(sync.end_seq())
                                .expect("cannot write to stdout");
                        }
                        stdout.flush().expect("could not flush");
                    }
                    Some(Ok(Message::Close(_))) => {
                        break;
                    }
                    Some(Err(e)) => {
                        log::error!("Error: {}", e);
                        break;
                    }
                    None => {
                        log::error!("Received empty message from web server");
                        break;
                    }
                    _ => {}
                }
            }

            control_msg = connections.control_ws.next() => {
                match control_msg {
                    Some(Ok(Message::Text(msg))) => {
                        let deserialized_msg: Result<WebServerToWebClientControlMessage, _> =
                            serde_json::from_str(&msg);
                        match deserialized_msg {
                            Ok(WebServerToWebClientControlMessage::SetConfig(..)) => {
                                // no-op
                            }
                            Ok(WebServerToWebClientControlMessage::QueryTerminalSize) => {
                                let new_size = os_input.get_terminal_size_using_fd(0);
                                if let Err(e) = connections.control_ws.send(create_resize_message(new_size)).await {
                                    log::error!("Failed to send resize message: {}", e);
                                }
                            }
                            Ok(WebServerToWebClientControlMessage::Log { lines }) => {
                                for line in lines {
                                    log::info!("{}", line);
                                }
                            }
                            Ok(WebServerToWebClientControlMessage::LogError { lines }) => {
                                for line in lines {
                                    log::error!("{}", line);
                                }
                            }
                            Ok(WebServerToWebClientControlMessage::SwitchedSession{ .. }) => {
                                // no-op
                            }
                            Err(e) => {
                                log::error!("Failed to deserialize control message: {}", e);
                            }
                        }

                    }
                    Some(Ok(Message::Close(_))) => {
                        break;
                    }
                    Some(Err(e)) => {
                        log::error!("{}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }

        }
    }

    Ok(None)
}

#[cfg(feature = "web_server_capability")]
pub fn start_remote_client(
    mut os_input: Box<dyn ClientOsApi>,
    remote_session_url: &str,
    token: Option<String>,
    remember: bool,
    forget: bool,
) -> Result<Option<ConnectToSession>, RemoteClientError> {
    info!("Starting Zellij client!");

    let runtime = Runtime::new().map_err(|e| RemoteClientError::IoError(e))?;

    let connections = remote_attach::attach_to_remote_session(
        &runtime,
        os_input.clone(),
        remote_session_url,
        token,
        remember,
        forget,
    )?;

    let reconnect_to_session = None;
    let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
    let take_snapshot = "\u{1b}[?1049h";
    let bracketed_paste = "\u{1b}[?2004h";
    let enter_kitty_keyboard_mode = "\u{1b}[>1u";
    os_input.unset_raw_mode(0).unwrap();

    let _ = os_input
        .get_stdout_writer()
        .write(take_snapshot.as_bytes())
        .unwrap();
    let _ = os_input
        .get_stdout_writer()
        .write(clear_client_terminal_attributes.as_bytes())
        .unwrap();
    let _ = os_input
        .get_stdout_writer()
        .write(enter_kitty_keyboard_mode.as_bytes())
        .unwrap();

    envs::set_zellij("0".to_string());

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);

    os_input.set_raw_mode(0);
    let _ = os_input
        .get_stdout_writer()
        .write(bracketed_paste.as_bytes())
        .unwrap();

    std::panic::set_hook({
        use zellij_utils::errors::handle_panic;
        let os_input = os_input.clone();
        Box::new(move |info| {
            if let Ok(()) = os_input.unset_raw_mode(0) {
                handle_panic::<ClientInstruction>(info, None);
            }
        })
    });

    let reset_controlling_terminal_state = |e: String, exit_status: i32| {
        os_input.unset_raw_mode(0).unwrap();
        let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
        let restore_alternate_screen = "\u{1b}[?1049l";
        let exit_kitty_keyboard_mode = "\u{1b}[<1u";
        let reset_style = "\u{1b}[m";
        let show_cursor = "\u{1b}[?25h";
        os_input.disable_mouse().non_fatal();
        let error = format!(
            "{}{}{}{}\n{}{}\n",
            reset_style,
            show_cursor,
            restore_alternate_screen,
            exit_kitty_keyboard_mode,
            goto_start_of_last_line,
            e
        );
        let _ = os_input
            .get_stdout_writer()
            .write(error.as_bytes())
            .unwrap();
        let _ = os_input.get_stdout_writer().flush().unwrap();
        if exit_status == 0 {
            log::info!("{}", e);
        } else {
            log::error!("{}", e);
        };
        std::process::exit(exit_status);
    };

    runtime.block_on(run_remote_client_terminal_loop(
        os_input.clone(),
        connections,
    ))?;

    let exit_msg = String::from("Bye from Zellij!");

    if reconnect_to_session.is_none() {
        reset_controlling_terminal_state(exit_msg, 0);
        std::process::exit(0);
    } else {
        let clear_screen = "\u{1b}[2J";
        let mut stdout = os_input.get_stdout_writer();
        let _ = stdout.write(clear_screen.as_bytes()).unwrap();
        stdout.flush().unwrap();
    }

    Ok(reconnect_to_session)
}

pub fn start_client(
    mut os_input: Box<dyn ClientOsApi>,
    cli_args: CliArgs,
    config: Config,          // saved to disk (or default?)
    config_options: Options, // CLI options merged into (getting priority over) saved config options
    info: ClientInfo,
    tab_position_to_focus: Option<usize>,
    pane_id_to_focus: Option<(u32, bool)>, // (pane_id, is_plugin)
    is_a_reconnect: bool,
    start_detached_and_exit: bool,
) -> Option<ConnectToSession> {
    if start_detached_and_exit {
        start_server_detached(os_input, cli_args, config, config_options, info);
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

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);

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

            let cli_assets = CliAssets {
                config_file_path: Config::config_file_path(&cli_args),
                config_dir: cli_args.config_dir.clone(),
                should_ignore_config: cli_args.is_setup_clean(),
                configuration_options: Some(config_options.clone()),
                layout: cli_args
                    .layout
                    .as_ref()
                    .and_then(|l| {
                        LayoutInfo::from_config(&config_options.layout_dir, &Some(l.clone()))
                    })
                    .or_else(|| {
                        LayoutInfo::from_config(
                            &config_options.layout_dir,
                            &config_options.default_layout,
                        )
                    }),
                terminal_window_size: full_screen_ws,
                data_dir: cli_args.data_dir.clone(),
                is_debug: cli_args.debug,
                max_panes: cli_args.max_panes,
                force_run_layout_commands: false,
                cwd: None,
            };
            (
                ClientToServerMsg::AttachClient {
                    cli_assets,
                    tab_position_to_focus,
                    pane_to_focus: pane_id_to_focus.map(|(pane_id, is_plugin)| {
                        zellij_utils::ipc::PaneReference { pane_id, is_plugin }
                    }),
                    is_web_client,
                },
                ipc_pipe,
            )
        },
        ClientInfo::Watch(name, _config_options) => {
            envs::set_session_name(name.clone());
            os_input.update_session_name(name);
            let ipc_pipe = create_ipc_pipe();
            let is_web_client = false;

            (
                ClientToServerMsg::AttachWatcherClient {
                    terminal_size: full_screen_ws,
                    is_web_client,
                },
                ipc_pipe,
            )
        },
        ClientInfo::Resurrect(name, path_to_layout, force_run_commands, cwd) => {
            envs::set_session_name(name.clone());

            let cli_assets = CliAssets {
                config_file_path: Config::config_file_path(&cli_args),
                config_dir: cli_args.config_dir.clone(),
                should_ignore_config: cli_args.is_setup_clean(),
                configuration_options: Some(config_options.clone()),
                layout: Some(LayoutInfo::File(path_to_layout.display().to_string())),
                terminal_window_size: full_screen_ws,
                data_dir: cli_args.data_dir.clone(),
                is_debug: cli_args.debug,
                max_panes: cli_args.max_panes,
                force_run_layout_commands: force_run_commands,
                cwd,
            };

            os_input.update_session_name(name);
            let ipc_pipe = create_ipc_pipe();

            spawn_server(&*ipc_pipe, cli_args.debug).unwrap();
            if should_start_web_server {
                if let Err(e) = spawn_web_server(&cli_args) {
                    log::error!("Failed to start web server: {}", e);
                }
            }

            let is_web_client = false;

            (
                ClientToServerMsg::FirstClientConnected {
                    cli_assets,
                    is_web_client,
                },
                ipc_pipe,
            )
        },
        ClientInfo::New(name, layout_info, layout_cwd) => {
            envs::set_session_name(name.clone());

            let cli_assets = CliAssets {
                config_file_path: Config::config_file_path(&cli_args),
                config_dir: cli_args.config_dir.clone(),
                should_ignore_config: cli_args.is_setup_clean(),
                configuration_options: Some(config_options.clone()),
                layout: layout_info.or_else(|| {
                    cli_args
                        .layout
                        .as_ref()
                        .and_then(|l| {
                            LayoutInfo::from_config(&config_options.layout_dir, &Some(l.clone()))
                        })
                        .or_else(|| {
                            LayoutInfo::from_config(
                                &config_options.layout_dir,
                                &config_options.default_layout,
                            )
                        })
                }),
                terminal_window_size: full_screen_ws,
                data_dir: cli_args.data_dir.clone(),
                is_debug: cli_args.debug,
                max_panes: cli_args.max_panes,
                force_run_layout_commands: false,
                cwd: layout_cwd,
            };

            os_input.update_session_name(name);
            let ipc_pipe = create_ipc_pipe();

            spawn_server(&*ipc_pipe, cli_args.debug).unwrap();
            if should_start_web_server {
                if let Err(e) = spawn_web_server(&cli_args) {
                    log::error!("Failed to start web server: {}", e);
                }
            }

            let is_web_client = false;

            (
                ClientToServerMsg::FirstClientConnected {
                    cli_assets,
                    is_web_client,
                },
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
                handle_panic(info, Some(&send_client_instructions));
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
            move || {
                os_input.handle_signals(
                    Box::new({
                        let os_api = os_input.clone();
                        move || {
                            os_api.send_to_server(ClientToServerMsg::TerminalResize {
                                new_size: os_api.get_terminal_size_using_fd(0),
                            });
                        }
                    }),
                    Box::new({
                        let os_api = os_input.clone();
                        move || {
                            os_api.send_to_server(ClientToServerMsg::Action {
                                action: on_force_close.into(),
                                terminal_id: None,
                                client_id: None,
                                is_cli_client: false,
                            });
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
            let mut consecutive_unknown_messages_received = 0;
            move || loop {
                match os_input.recv_from_server() {
                    Some((instruction, err_ctx)) => {
                        consecutive_unknown_messages_received = 0;
                        err_ctx.update_thread_ctx();
                        if let ServerToClientMsg::Exit { .. } = instruction {
                            should_break = true;
                        }
                        send_client_instructions.send(instruction.into()).unwrap();
                        if should_break {
                            break;
                        }
                    },
                    None => {
                        consecutive_unknown_messages_received += 1;
                        send_client_instructions
                            .send(ClientInstruction::UnblockInputThread)
                            .unwrap();
                        log::error!("Received unknown message from server");
                        if consecutive_unknown_messages_received >= 1000 {
                            send_client_instructions
                                .send(ClientInstruction::Error(
                                    "Received empty unknown from server".to_string(),
                                ))
                                .unwrap();
                            break;
                        }
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
                os_input.send_to_server(ClientToServerMsg::TerminalResize {
                    new_size: os_input.get_terminal_size_using_fd(0),
                });
            },
            ClientInstruction::StartWebServer => {
                let web_server_base_url = web_server_base_url(
                    web_server_ip,
                    web_server_port,
                    has_certificate,
                    enforce_https_for_localhost,
                );
                match spawn_web_server(&cli_args) {
                    Ok(_) => {
                        let _ = os_input.send_to_server(ClientToServerMsg::WebServerStarted {
                            base_url: web_server_base_url,
                        });
                    },
                    Err(e) => {
                        log::error!("Failed to start web_server: {}", e);
                        let _ = os_input
                            .send_to_server(ClientToServerMsg::FailedToStartWebServer { error: e });
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
    cli_args: CliArgs,
    config: Config,
    config_options: Options,
    info: ClientInfo,
) {
    envs::set_zellij("0".to_string());
    config.env.set_vars();

    let should_start_web_server = config_options.web_server.map(|w| w).unwrap_or(false);

    let create_ipc_pipe = || -> std::path::PathBuf {
        let mut sock_dir = ZELLIJ_SOCK_DIR.clone();
        std::fs::create_dir_all(&sock_dir).unwrap();
        set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(envs::get_session_name().unwrap());
        sock_dir
    };

    let (first_msg, ipc_pipe) = match info {
        ClientInfo::Resurrect(name, path_to_layout, force_run_commands, cwd) => {
            envs::set_session_name(name.clone());

            let cli_assets = CliAssets {
                config_file_path: Config::config_file_path(&cli_args),
                config_dir: cli_args.config_dir.clone(),
                should_ignore_config: cli_args.is_setup_clean(),
                configuration_options: Some(config_options.clone()),
                layout: Some(LayoutInfo::File(path_to_layout.display().to_string())),
                terminal_window_size: Size { cols: 50, rows: 50 }, // static number until a
                // client connects
                data_dir: cli_args.data_dir.clone(),
                is_debug: cli_args.debug,
                max_panes: cli_args.max_panes,
                force_run_layout_commands: force_run_commands,
                cwd,
            };

            os_input.update_session_name(name);
            let ipc_pipe = create_ipc_pipe();

            spawn_server(&*ipc_pipe, cli_args.debug).unwrap();
            if should_start_web_server {
                if let Err(e) = spawn_web_server(&cli_args) {
                    log::error!("Failed to start web server: {}", e);
                }
            }

            let is_web_client = false;

            (
                ClientToServerMsg::FirstClientConnected {
                    cli_assets,
                    is_web_client,
                },
                ipc_pipe,
            )
        },
        ClientInfo::New(name, layout_info, layout_cwd) => {
            envs::set_session_name(name.clone());

            let cli_assets = CliAssets {
                config_file_path: Config::config_file_path(&cli_args),
                config_dir: cli_args.config_dir.clone(),
                should_ignore_config: cli_args.is_setup_clean(),
                configuration_options: cli_args.options(),
                layout: layout_info.or_else(|| {
                    cli_args
                        .layout
                        .as_ref()
                        .and_then(|l| {
                            LayoutInfo::from_config(&config_options.layout_dir, &Some(l.clone()))
                        })
                        .or_else(|| {
                            LayoutInfo::from_config(
                                &config_options.layout_dir,
                                &config_options.default_layout,
                            )
                        })
                }),
                terminal_window_size: Size { cols: 50, rows: 50 }, // static number until a
                // client connects
                data_dir: cli_args.data_dir.clone(),
                is_debug: cli_args.debug,
                max_panes: cli_args.max_panes,
                force_run_layout_commands: false,
                cwd: layout_cwd,
            };

            os_input.update_session_name(name);
            let ipc_pipe = create_ipc_pipe();

            spawn_server(&*ipc_pipe, cli_args.debug).unwrap();
            if should_start_web_server {
                if let Err(e) = spawn_web_server(&cli_args) {
                    log::error!("Failed to start web server: {}", e);
                }
            }
            let is_web_client = false;

            (
                ClientToServerMsg::FirstClientConnected {
                    cli_assets,
                    is_web_client,
                },
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

#[cfg(test)]
mod unit;

use crate::os_input_output::ClientOsApi;
use crate::web_client::control_message::WebServerToWebClientControlMessage;
use crate::web_client::session_management::build_initial_connection;
use crate::web_client::types::{ClientConnectionBus, ConnectionTable, SessionManager};
use crate::web_client::utils::terminal_init_messages;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use zellij_utils::{
    cli::CliArgs,
    data::Style,
    input::{config::Config, options::Options},
    ipc::{ClientToServerMsg, ExitReason, ServerToClientMsg},
    sessions::generate_unique_session_name,
    setup::Setup,
};

pub fn zellij_server_listener(
    os_input: Box<dyn ClientOsApi>,
    connection_table: Arc<Mutex<ConnectionTable>>,
    session_name: Option<String>,
    mut config: Config,
    mut config_options: Options,
    config_file_path: Option<PathBuf>,
    web_client_id: String,
    session_manager: Arc<dyn SessionManager>,
) {
    let _server_listener_thread = std::thread::Builder::new()
        .name("server_listener".to_string())
        .spawn({
            move || {
                let mut client_connection_bus =
                    ClientConnectionBus::new(&web_client_id, &connection_table);
                let (mut reconnect_to_session, is_welcome_screen) =
                    match build_initial_connection(session_name, &config) {
                        Ok(initial_session_connection) => initial_session_connection,
                        Err(e) => {
                            log::error!("{}", e);
                            return;
                        },
                    };
                'reconnect_loop: loop {
                    let reconnect_info = reconnect_to_session.take();
                    let path = {
                        let Some(session_name) = reconnect_info
                            .as_ref()
                            .and_then(|r| r.name.clone())
                            .or_else(generate_unique_session_name)
                        else {
                            log::error!("Failed to generate unique session name, bailing.");
                            return;
                        };
                        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
                        sock_dir.push(session_name.clone());
                        sock_dir.to_str().unwrap().to_owned()
                    };

                    reload_config_from_disk(&mut config, &mut config_options, &config_file_path);

                    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
                    let mut sent_init_messages = false;

                    let palette = config
                        .theme_config(config_options.theme.as_ref())
                        .unwrap_or_else(|| os_input.load_palette().into());
                    let client_attributes = zellij_utils::ipc::ClientAttributes {
                        size: full_screen_ws,
                        style: Style {
                            colors: palette,
                            rounded_corners: config.ui.pane_frames.rounded_corners,
                            hide_session_name: config.ui.pane_frames.hide_session_name,
                        },
                    };

                    let session_name = PathBuf::from(path.clone())
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned();

                    let is_web_client = true;
                    let (first_message, zellij_ipc_pipe) = session_manager.spawn_session_if_needed(
                        &session_name,
                        path,
                        client_attributes,
                        &config,
                        &config_options,
                        is_web_client,
                        os_input.clone(),
                        reconnect_info.as_ref().and_then(|r| r.layout.clone()),
                        is_welcome_screen,
                    );

                    os_input.connect_to_server(&zellij_ipc_pipe);
                    os_input.send_to_server(first_message);

                    client_connection_bus.send_control(
                        WebServerToWebClientControlMessage::SwitchedSession {
                            new_session_name: session_name.clone(),
                        },
                    );

                    loop {
                        match os_input.recv_from_server() {
                            Some((ServerToClientMsg::UnblockInputThread, _)) => {},
                            Some((ServerToClientMsg::Exit(exit_reason), _)) => {
                                handle_exit_reason(&mut client_connection_bus, exit_reason);
                                os_input.send_to_server(ClientToServerMsg::ClientExited);
                                break;
                            },
                            Some((ServerToClientMsg::Render(bytes), _)) => {
                                if !sent_init_messages {
                                    for message in terminal_init_messages() {
                                        client_connection_bus.send_stdout(message.to_owned())
                                    }
                                    sent_init_messages = true;
                                }
                                client_connection_bus.send_stdout(bytes);
                            },
                            Some((ServerToClientMsg::SwitchSession(connect_to_session), _)) => {
                                reconnect_to_session = Some(connect_to_session);
                                continue 'reconnect_loop;
                            },
                            Some((ServerToClientMsg::WriteConfigToDisk { config }, _)) => {
                                handle_config_write(&os_input, config);
                            },
                            Some((ServerToClientMsg::QueryTerminalSize, _)) => {
                                client_connection_bus.send_control(
                                    WebServerToWebClientControlMessage::QueryTerminalSize,
                                );
                            },
                            Some((ServerToClientMsg::Log(lines), _)) => {
                                client_connection_bus.send_control(
                                    WebServerToWebClientControlMessage::Log { lines },
                                );
                            },
                            Some((ServerToClientMsg::LogError(lines), _)) => {
                                client_connection_bus.send_control(
                                    WebServerToWebClientControlMessage::LogError { lines },
                                );
                            },
                            Some((ServerToClientMsg::RenamedSession(new_session_name), _)) => {
                                client_connection_bus.send_control(
                                    WebServerToWebClientControlMessage::SwitchedSession {
                                        new_session_name,
                                    },
                                );
                            },
                            _ => {
                                // server disconnected, stop trying to listen otherwise we retry
                                // indefinitely and get 100% CPU
                                break;
                            },
                        }
                    }
                    if reconnect_to_session.is_none() {
                        break;
                    }
                }
            }
        });
}

fn handle_exit_reason(client_connection_bus: &mut ClientConnectionBus, exit_reason: ExitReason) {
    match exit_reason {
        ExitReason::WebClientsForbidden => {
            client_connection_bus.send_stdout(format!(
                "\u{1b}[2J\n Web Clients are not allowed to attach to this session."
            ));
        },
        ExitReason::Error(e) => {
            let goto_start_of_last_line = format!("\u{1b}[{};{}H", 1, 1);
            let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
            let disable_mouse = "\u{1b}[?1006l\u{1b}[?1015l\u{1b}[?1003l\u{1b}[?1002l\u{1b}[?1000l";
            let error = format!(
                "{}{}\n{}{}\n",
                disable_mouse,
                clear_client_terminal_attributes,
                goto_start_of_last_line,
                e.to_string().replace("\n", "\n\r")
            );
            client_connection_bus.send_stdout(format!("\u{1b}[2J\n{}", error));
        },
        _ => {},
    }
    client_connection_bus.close_connection();
}

fn handle_config_write(os_input: &Box<dyn ClientOsApi>, config: String) {
    match Config::write_config_to_disk(config, &CliArgs::default()) {
        Ok(written_config) => {
            let _ = os_input.send_to_server(ClientToServerMsg::ConfigWrittenToDisk(written_config));
        },
        Err(e) => {
            let error_path = e
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(String::new);
            log::error!("Failed to write config to disk: {}", error_path);
            let _ = os_input.send_to_server(ClientToServerMsg::FailedToWriteConfigToDisk(e));
        },
    }
}

fn reload_config_from_disk(
    config_without_layout: &mut Config,
    config_options_without_layout: &mut Options,
    config_file_path: &Option<PathBuf>,
) {
    let mut cli_args = CliArgs::default();
    cli_args.config = config_file_path.clone();
    match Setup::from_cli_args(&cli_args) {
        Ok((_, _, _, reloaded_config_without_layout, reloaded_config_options_without_layout)) => {
            *config_without_layout = reloaded_config_without_layout;
            *config_options_without_layout = reloaded_config_options_without_layout;
        },
        Err(e) => {
            log::error!("Failed to reload config: {}", e);
        },
    };
}

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc};

use anyhow::{anyhow, bail, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

use zellij_client::remote_attach::{
    self, extract_server_url, ClientDeclaration, Prompting, WebSocketConnections,
};
use zellij_client::web_client::control_message::{
    TerminalMetricsPayload, WebClientToWebServerControlMessage,
    WebClientToWebServerControlMessagePayload, WebServerToWebClientControlMessage,
};
use zellij_utils::cli::AttachArgs;
use zellij_utils::input::actions::Action;
use zellij_utils::ipc::{ClientToServerMsg, IpcReceiveError, ServerToClientMsg};
use zellij_utils::remote_session_tokens;

use crate::connection::{Connection, Geometry, GeometryHandle, MessageSink, MessageSource, Role};

#[derive(Debug, Clone, PartialEq)]
pub enum Outbound {
    Control(WebClientToWebServerControlMessagePayload),
    Close,
    Dropped(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inbound {
    Frame(Vec<u8>),
    Ansi(String),
    Control(WebServerToWebClientControlMessage),
}

pub fn outbound(msg: ClientToServerMsg) -> Outbound {
    use WebClientToWebServerControlMessagePayload as Payload;
    match msg {
        ClientToServerMsg::Key {
            key,
            raw_bytes,
            is_kitty_keyboard_protocol,
        } => Outbound::Control(Payload::Key {
            key,
            raw_bytes,
            is_kitty_keyboard_protocol,
        }),
        ClientToServerMsg::Action { action, .. } => match action {
            Action::MouseEvent { event } => Outbound::Control(Payload::Mouse { event }),
            Action::Paste { chars, .. } => Outbound::Control(Payload::Paste { chars }),
            Action::WriteChars { chars } => Outbound::Control(Payload::Text { chars }),
            Action::Detach => Outbound::Control(Payload::Detach),
            other => Outbound::Dropped(format!("Action::{}", action_name(&other))),
        },
        ClientToServerMsg::TerminalResize { new_size } => {
            Outbound::Control(Payload::TerminalResize(new_size))
        },
        ClientToServerMsg::TerminalPixelDimensions { pixel_dimensions } => {
            match (
                pixel_dimensions.text_area_size,
                pixel_dimensions.character_cell_size,
            ) {
                (Some(area), Some(cell)) => {
                    Outbound::Control(Payload::TerminalMetrics(TerminalMetricsPayload {
                        cell_pixel_width: cell.width,
                        cell_pixel_height: cell.height,
                        text_area_pixel_width: area.width,
                        text_area_pixel_height: area.height,
                    }))
                },
                _ => {
                    Outbound::Dropped("TerminalPixelDimensions without both dimensions".to_owned())
                },
            }
        },
        ClientToServerMsg::RenderFrameAck { seq } => {
            Outbound::Control(Payload::RenderFrameAck { seq })
        },
        ClientToServerMsg::ForwardedReplyFromHost { token, reply_bytes } => {
            Outbound::Control(Payload::ForwardedReplyFromHost { token, reply_bytes })
        },
        ClientToServerMsg::ClientExited => Outbound::Close,
        ClientToServerMsg::AttachClient { .. } => Outbound::Dropped(
            "AttachClient (the web server attaches on the window's behalf)".to_owned(),
        ),
        ClientToServerMsg::FirstClientConnected { .. } => Outbound::Dropped(
            "FirstClientConnected (the web server creates the session)".to_owned(),
        ),
        ClientToServerMsg::AttachWatcherClient { .. } => Outbound::Dropped(
            "AttachWatcherClient (the web server has no watcher attach)".to_owned(),
        ),
        ClientToServerMsg::KittyGraphicsSupport { .. } => {
            Outbound::Dropped("KittyGraphicsSupport (declared by the web server)".to_owned())
        },
        ClientToServerMsg::SixelSupport { .. } => {
            Outbound::Dropped("SixelSupport (declared by the web server)".to_owned())
        },
        ClientToServerMsg::StructuredRenderSupport { .. } => {
            Outbound::Dropped("StructuredRenderSupport (declared by the web server)".to_owned())
        },
        ClientToServerMsg::ForegroundColor { .. } => {
            Outbound::Dropped("ForegroundColor (the window answers host queries itself)".to_owned())
        },
        ClientToServerMsg::BackgroundColor { .. } => {
            Outbound::Dropped("BackgroundColor (the window answers host queries itself)".to_owned())
        },
        ClientToServerMsg::ColorRegisters { .. } => {
            Outbound::Dropped("ColorRegisters (the window answers host queries itself)".to_owned())
        },
        other => Outbound::Dropped(message_name(&other)),
    }
}

pub fn inbound(wire: Inbound) -> std::result::Result<ServerToClientMsg, String> {
    match wire {
        Inbound::Frame(frame) => Ok(ServerToClientMsg::RenderFrame { frame }),
        Inbound::Ansi(content) => Ok(ServerToClientMsg::Render { content }),
        Inbound::Control(control) => match control {
            WebServerToWebClientControlMessage::HostTerminalThemeChanged { mode } => {
                Ok(ServerToClientMsg::HostTerminalThemeChanged { mode })
            },
            WebServerToWebClientControlMessage::ForwardQueryToHost {
                token,
                query_bytes,
                resolve_async,
            } => Ok(ServerToClientMsg::ForwardQueryToHost {
                token,
                query_bytes,
                resolve_async,
            }),
            WebServerToWebClientControlMessage::QueryTerminalSize => {
                Ok(ServerToClientMsg::QueryTerminalSize)
            },
            WebServerToWebClientControlMessage::Log { lines } => {
                Ok(ServerToClientMsg::Log { lines })
            },
            WebServerToWebClientControlMessage::LogError { lines } => {
                Ok(ServerToClientMsg::LogError { lines })
            },
            WebServerToWebClientControlMessage::Exit { reason } => Ok(ServerToClientMsg::Exit {
                exit_reason: reason,
            }),
            WebServerToWebClientControlMessage::SetConfig(_) => {
                Err("SetConfig (the browser's font and theme, not the window's)".to_owned())
            },
            WebServerToWebClientControlMessage::SetSoftKeyboard { .. } => {
                Err("SetSoftKeyboard (a window has no soft keyboard)".to_owned())
            },
            WebServerToWebClientControlMessage::MobileState { .. } => {
                Err("MobileState (a window is not a phone)".to_owned())
            },
            WebServerToWebClientControlMessage::SwitchedSession { .. } => Err(
                "SwitchedSession (a remote session switch is the web server's, and the window \
                 keeps rendering)"
                    .to_owned(),
            ),
        },
    }
}

fn action_name(action: &Action) -> String {
    let debug = format!("{:?}", action);
    debug
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

fn message_name(msg: &ClientToServerMsg) -> String {
    let debug = format!("{:?}", msg);
    debug
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

struct WebSocketSink {
    outbound: UnboundedSender<Outbound>,
}

impl MessageSink for WebSocketSink {
    fn send_msg(&self, msg: ClientToServerMsg) -> Result<()> {
        match outbound(msg) {
            Outbound::Dropped(_) => Ok(()),
            translated => self
                .outbound
                .send(translated)
                .map_err(|_| anyhow!("the remote session's websocket is gone")),
        }
    }
}

struct WebSocketSource {
    inbound: Receiver<ServerToClientMsg>,
    runtime: Option<tokio::runtime::Runtime>,
}

impl MessageSource for WebSocketSource {
    fn recv_msg(&mut self) -> std::result::Result<ServerToClientMsg, IpcReceiveError> {
        self.inbound
            .recv()
            .map_err(|_| IpcReceiveError::Disconnected)
    }
}

impl Drop for WebSocketSource {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WatchedConfig {
    pub path: PathBuf,
    pub dir: Option<PathBuf>,
}

pub fn watch_local_config(
    runtime: &tokio::runtime::Handle,
    config: WatchedConfig,
) -> UnboundedReceiver<()> {
    let (changed_tx, changed_rx) = unbounded_channel();
    runtime.spawn(async move {
        let WatchedConfig { path, dir } = config;
        zellij_utils::input::config::watch_config_file_changes(
            path,
            dir.as_deref(),
            move |_reloaded| {
                let changed_tx = changed_tx.clone();
                async move {
                    let _ = changed_tx.send(());
                }
            },
        )
        .await;
    });
    changed_rx
}

async fn next_config_change(watcher: &mut Option<UnboundedReceiver<()>>) -> Option<()> {
    match watcher {
        Some(changes) => changes.recv().await,
        None => std::future::pending().await,
    }
}

pub fn open(
    url: &str,
    attach: &AttachArgs,
    geometry: Geometry,
    local_config: Option<WatchedConfig>,
) -> Result<Connection> {
    let server_url = extract_server_url(url).map_err(|e| anyhow!("{:?}", e))?;
    refuse_without_a_token(attach, &server_url)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("zellij-window remote transport")
        .enable_all()
        .build()
        .map_err(|e| anyhow!("failed to start the remote transport's runtime: {}", e))?;

    let connections = remote_attach::attach_to_remote_session(
        runtime.handle().clone(),
        url,
        attach.token.clone(),
        attach.remember,
        attach.forget,
        attach.ca_cert.as_deref(),
        attach.insecure,
        ClientDeclaration {
            size: Some(geometry.size()),
            cell: geometry.pixel_dimensions().character_cell_size,
            structured: true,
        },
        Prompting::Never,
    )
    .map_err(|e| anyhow!("{}", describe(e)))?;

    let session_name = session_name_of(url);
    let (outbound_tx, outbound_rx) = unbounded_channel();
    let (inbound_tx, inbound_rx) = mpsc::channel();
    let config_changes =
        local_config.map(|config| watch_local_config(runtime.handle(), config.clone()));
    runtime.spawn(pump(
        connections,
        outbound_rx,
        inbound_tx,
        config_changes,
    ));

    Ok(Connection {
        sender: Arc::new(WebSocketSink {
            outbound: outbound_tx,
        }),
        receiver: Box::new(WebSocketSource {
            inbound: inbound_rx,
            runtime: Some(runtime),
        }),
        session_name,
        geometry: GeometryHandle::new(geometry),
        role: Role::Participant,
    })
}

fn refuse_without_a_token(attach: &AttachArgs, server_url: &str) -> Result<()> {
    if attach.token.is_some() {
        return Ok(());
    }
    if !attach.forget {
        if let Ok(Some(_saved)) = remote_session_tokens::get_session_token(server_url) {
            return Ok(());
        }
    }
    bail!(
        "This remote session needs an authentication token, and none was given or saved. \
         Pass `--token <TOKEN>`, and `--remember` to save it for next time."
    )
}

fn describe(error: zellij_client::RemoteClientError) -> String {
    use zellij_client::RemoteClientError as E;
    match error {
        E::InvalidAuthToken => "The remote server refused the authentication token.".to_owned(),
        E::SessionTokenExpired => {
            "The saved session token has expired. Pass `--token <TOKEN>` again.".to_owned()
        },
        E::Unauthorized => "The remote server refused this client.".to_owned(),
        other => format!("The remote session could not be opened: {:?}", other),
    }
}

fn session_name_of(url: &str) -> String {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned()
}

async fn pump(
    connections: WebSocketConnections,
    mut outbound_rx: tokio::sync::mpsc::UnboundedReceiver<Outbound>,
    inbound_tx: Sender<ServerToClientMsg>,
    mut config_changes: Option<UnboundedReceiver<()>>,
) {
    let web_client_id = connections.web_client_id.clone();
    let mut terminal_ws = connections.terminal_ws;
    let (mut control_tx, mut control_rx) = connections.control_ws.split();

    loop {
        tokio::select! {
            change = next_config_change(&mut config_changes) => {
                match change {
                    Some(()) => {
                        if inbound_tx.send(ServerToClientMsg::ConfigFileUpdated).is_err() {
                            break;
                        }
                    },
                    None => config_changes = None,
                }
            }
            item = outbound_rx.recv() => {
                let Some(item) = item else { break };
                match item {
                    Outbound::Control(payload) => {
                        let envelope = WebClientToWebServerControlMessage {
                            web_client_id: web_client_id.clone(),
                            payload,
                        };
                        let Ok(text) = serde_json::to_string(&envelope) else { continue };
                        if control_tx.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    },
                    Outbound::Close => break,
                    Outbound::Dropped(_) => {},
                }
            }
            message = terminal_ws.next() => {
                match message {
                    Some(Ok(Message::Binary(frame))) => {
                        if deliver(&inbound_tx, Inbound::Frame(frame.to_vec())).is_err() {
                            break;
                        }
                    },
                    Some(Ok(Message::Text(text))) => {
                        if deliver(&inbound_tx, Inbound::Ansi(text.to_string())).is_err() {
                            break;
                        }
                    },
                    Some(Ok(_)) => {},
                    Some(Err(_)) | None => break,
                }
            }
            message = control_rx.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<WebServerToWebClientControlMessage>(&text) {
                            Ok(control) => {
                                if deliver(&inbound_tx, Inbound::Control(control)).is_err() {
                                    break;
                                }
                            },
                            Err(_) => {},
                        }
                    },
                    Some(Ok(_)) => {},
                    Some(Err(_)) | None => break,
                }
            }
        }
    }

    let _ = control_tx.send(Message::Close(None)).await;
    let _ = terminal_ws.close(None).await;
}

fn deliver(
    inbound_tx: &Sender<ServerToClientMsg>,
    wire: Inbound,
) -> std::result::Result<(), ()> {
    match inbound(wire) {
        Ok(msg) => inbound_tx.send(msg).map_err(|_| ()),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::data::{BareKey, HostTerminalThemeMode, KeyWithModifier};
    use zellij_utils::input::mouse::MouseEvent;
    use zellij_utils::ipc::{ExitReason, PixelDimensions};
    use zellij_utils::pane_size::{Size, SizeInPixels};

    fn action(action: Action) -> ClientToServerMsg {
        ClientToServerMsg::Action {
            action,
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        }
    }

    #[test]
    fn a_key_travels_typed_rather_than_as_bytes_a_parser_must_undo() {
        let key = KeyWithModifier::new(BareKey::Char('a')).with_ctrl_modifier();
        assert_eq!(
            outbound(ClientToServerMsg::Key {
                key: key.clone(),
                raw_bytes: vec![0x01],
                is_kitty_keyboard_protocol: false,
            }),
            Outbound::Control(WebClientToWebServerControlMessagePayload::Key {
                key,
                raw_bytes: vec![0x01],
                is_kitty_keyboard_protocol: false,
            })
        );
    }

    #[test]
    fn the_three_actions_a_window_sends_have_a_typed_payload_each() {
        let mut event = MouseEvent::new();
        event.wheel_right = true;
        assert_eq!(
            outbound(action(Action::MouseEvent { event })),
            Outbound::Control(WebClientToWebServerControlMessagePayload::Mouse { event })
        );
        assert_eq!(
            outbound(action(Action::Paste {
                chars: "text".to_owned(),
                pane_id: None
            })),
            Outbound::Control(WebClientToWebServerControlMessagePayload::Paste {
                chars: "text".to_owned()
            })
        );
        assert_eq!(
            outbound(action(Action::Detach)),
            Outbound::Control(WebClientToWebServerControlMessagePayload::Detach)
        );
    }

    #[test]
    fn composed_text_travels_as_typed_text() {
        assert_eq!(
            outbound(action(Action::WriteChars {
                chars: "你好".to_owned()
            })),
            Outbound::Control(WebClientToWebServerControlMessagePayload::Text {
                chars: "你好".to_owned()
            }),
            "a commit has a lane of its own; the paste lane would bracket it"
        );
    }

    #[test]
    fn any_other_action_is_dropped_by_name() {
        assert_eq!(
            outbound(action(Action::Quit)),
            Outbound::Dropped("Action::Quit".to_owned())
        );
    }

    #[test]
    fn geometry_travels_as_the_two_messages_the_browser_sends() {
        assert_eq!(
            outbound(ClientToServerMsg::TerminalResize {
                new_size: Size {
                    rows: 40,
                    cols: 120
                }
            }),
            Outbound::Control(WebClientToWebServerControlMessagePayload::TerminalResize(
                Size {
                    rows: 40,
                    cols: 120
                }
            ))
        );
        let metrics = outbound(ClientToServerMsg::TerminalPixelDimensions {
            pixel_dimensions: PixelDimensions {
                text_area_size: Some(SizeInPixels {
                    width: 840,
                    height: 720,
                }),
                character_cell_size: Some(SizeInPixels {
                    width: 7,
                    height: 18,
                }),
            },
        });
        assert_eq!(
            metrics,
            Outbound::Control(WebClientToWebServerControlMessagePayload::TerminalMetrics(
                TerminalMetricsPayload {
                    cell_pixel_width: 7,
                    cell_pixel_height: 18,
                    text_area_pixel_width: 840,
                    text_area_pixel_height: 720,
                }
            ))
        );
    }

    #[test]
    fn the_acknowledgement_and_the_host_reply_keep_their_names() {
        assert_eq!(
            outbound(ClientToServerMsg::RenderFrameAck { seq: 17 }),
            Outbound::Control(WebClientToWebServerControlMessagePayload::RenderFrameAck {
                seq: 17
            })
        );
        assert_eq!(
            outbound(ClientToServerMsg::ForwardedReplyFromHost {
                token: 3,
                reply_bytes: b"\x1b]11;rgb:0000/0000/0000\x1b\\".to_vec(),
            }),
            Outbound::Control(
                WebClientToWebServerControlMessagePayload::ForwardedReplyFromHost {
                    token: 3,
                    reply_bytes: b"\x1b]11;rgb:0000/0000/0000\x1b\\".to_vec(),
                }
            )
        );
    }

    #[test]
    fn leaving_closes_the_sockets_rather_than_sending_anything() {
        assert_eq!(outbound(ClientToServerMsg::ClientExited), Outbound::Close);
    }

    #[test]
    fn the_handshake_the_web_server_performs_for_the_window_is_dropped_whole() {
        let dropped = [
            ClientToServerMsg::KittyGraphicsSupport {
                supported: true,
                local_media: false,
            },
            ClientToServerMsg::SixelSupport { supported: true },
            ClientToServerMsg::StructuredRenderSupport { supported: true },
            ClientToServerMsg::ForegroundColor {
                color: "rgb:0/0/0".to_owned(),
            },
            ClientToServerMsg::BackgroundColor {
                color: "rgb:0/0/0".to_owned(),
            },
            ClientToServerMsg::ColorRegisters {
                color_registers: vec![],
            },
        ];
        for msg in dropped {
            assert!(
                matches!(outbound(msg.clone()), Outbound::Dropped(_)),
                "{:?} has no remote equivalent and must be dropped",
                msg
            );
        }
    }

    #[test]
    fn a_frame_is_binary_and_ansi_is_text() {
        assert_eq!(
            inbound(Inbound::Frame(vec![1, 2, 3])),
            Ok(ServerToClientMsg::RenderFrame {
                frame: vec![1, 2, 3]
            })
        );
        assert_eq!(
            inbound(Inbound::Ansi("\u{1b}[2J".to_owned())),
            Ok(ServerToClientMsg::Render {
                content: "\u{1b}[2J".to_owned()
            }),
            "ANSI is carried through as a render so the stale-server grace still counts it"
        );
    }

    #[test]
    fn an_exit_reason_reaches_a_window_instead_of_being_painted_at_it() {
        assert_eq!(
            inbound(Inbound::Control(WebServerToWebClientControlMessage::Exit {
                reason: ExitReason::NormalDetached
            })),
            Ok(ServerToClientMsg::Exit {
                exit_reason: ExitReason::NormalDetached
            })
        );
    }

    #[test]
    fn the_control_messages_a_window_acts_on_keep_their_meaning() {
        assert_eq!(
            inbound(Inbound::Control(
                WebServerToWebClientControlMessage::HostTerminalThemeChanged {
                    mode: HostTerminalThemeMode::Light
                }
            )),
            Ok(ServerToClientMsg::HostTerminalThemeChanged {
                mode: HostTerminalThemeMode::Light
            })
        );
        assert_eq!(
            inbound(Inbound::Control(
                WebServerToWebClientControlMessage::ForwardQueryToHost {
                    token: 9,
                    query_bytes: b"\x1b]11;?\x1b\\".to_vec(),
                    resolve_async: false,
                }
            )),
            Ok(ServerToClientMsg::ForwardQueryToHost {
                token: 9,
                query_bytes: b"\x1b]11;?\x1b\\".to_vec(),
                resolve_async: false,
            })
        );
        assert_eq!(
            inbound(Inbound::Control(
                WebServerToWebClientControlMessage::QueryTerminalSize
            )),
            Ok(ServerToClientMsg::QueryTerminalSize)
        );
        assert_eq!(
            inbound(Inbound::Control(WebServerToWebClientControlMessage::Log {
                lines: vec!["one".to_owned()]
            })),
            Ok(ServerToClientMsg::Log {
                lines: vec!["one".to_owned()]
            })
        );
        assert_eq!(
            inbound(Inbound::Control(
                WebServerToWebClientControlMessage::LogError {
                    lines: vec!["one".to_owned()]
                }
            )),
            Ok(ServerToClientMsg::LogError {
                lines: vec!["one".to_owned()]
            })
        );
    }

    #[test]
    fn the_four_control_messages_a_window_has_no_use_for_are_dropped_by_name() {
        let dropped = [
            WebServerToWebClientControlMessage::SetConfig(Default::default()),
            WebServerToWebClientControlMessage::SetSoftKeyboard { on: true },
            WebServerToWebClientControlMessage::MobileState {
                payload: Default::default(),
            },
            WebServerToWebClientControlMessage::SwitchedSession {
                new_session_name: "other".to_owned(),
            },
        ];
        let names: Vec<String> = dropped
            .into_iter()
            .map(|control| {
                inbound(Inbound::Control(control)).expect_err("this message must be dropped")
            })
            .collect();
        assert!(names[0].starts_with("SetConfig"), "{:?}", names);
        assert!(names[1].starts_with("SetSoftKeyboard"), "{:?}", names);
        assert!(names[2].starts_with("MobileState"), "{:?}", names);
        assert!(names[3].starts_with("SwitchedSession"), "{:?}", names);
    }

    #[test]
    fn a_missing_token_is_refused_before_anything_is_dialled() {
        let attach = AttachArgs::default();
        let refusal = refuse_without_a_token(&attach, "http://127.0.0.1:0")
            .expect_err("a session with no token must be refused");
        assert!(refusal.to_string().contains("--token"), "{}", refusal);
    }

    #[test]
    fn a_given_token_is_enough_to_go_on() {
        let attach = AttachArgs {
            token: Some("t".to_owned()),
            ..AttachArgs::default()
        };
        assert!(refuse_without_a_token(&attach, "http://127.0.0.1:0").is_ok());
    }

    #[test]
    fn the_session_name_is_the_last_segment_of_the_url() {
        assert_eq!(session_name_of("https://example.com/work"), "work");
        assert_eq!(session_name_of("https://example.com/work/"), "work");
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn a_window_opened_on_no_local_configuration_waits_on_nothing() {
        let runtime = runtime();
        let mut watcher = None;
        let waited = runtime.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_millis(200),
                next_config_change(&mut watcher),
            )
            .await
        });
        assert!(
            waited.is_err(),
            "a window with no local configuration must never be woken by the watcher branch"
        );
    }

    #[test]
    fn an_edit_to_the_windows_own_configuration_file_wakes_a_remote_window_exactly_once() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("config.kdl");
        let write = |font_size: u32| {
            std::fs::write(&path, format!("window {{\n font_size {}\n}}\n", font_size)).unwrap()
        };
        write(14);

        let runtime = runtime();
        let mut changes = watch_local_config(
            runtime.handle(),
            WatchedConfig {
                path: path.clone(),
                dir: Some(directory.path().to_path_buf()),
            },
        );

        let armed = runtime.block_on(async {
            for _ in 0..40 {
                write(14);
                if tokio::time::timeout(std::time::Duration::from_millis(750), changes.recv())
                    .await
                    .is_ok()
                {
                    return true;
                }
            }
            false
        });
        assert!(armed, "the watcher never armed on the local configuration");
        runtime.block_on(async {
            while tokio::time::timeout(std::time::Duration::from_secs(3), changes.recv())
                .await
                .is_ok()
            {}
        });

        write(18);
        let first = runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(15), changes.recv()).await
        });
        assert_eq!(
            first.expect("the local edit never woke the remote window"),
            Some(())
        );

        let second = runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(4), changes.recv()).await
        });
        assert!(
            second.is_err(),
            "one edit woke the remote window more than once, so the reload would be applied twice"
        );
    }
}

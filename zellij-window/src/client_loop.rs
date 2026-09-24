use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use zellij_utils::ipc::{ClientToServerMsg, ExitReason, IpcReceiveError, ServerToClientMsg};

use zellij_utils::data::HostTerminalThemeMode;

use zellij_utils::data::ConnectToSession;

use crate::clipboard::ClipboardHandle;
use crate::connection::{request_detach, send, Connection, Detacher};
use crate::host_reply;
use crate::recorder::Recorder;
use crate::settings::{self, Settings};

const MAX_CONSECUTIVE_UNDECODABLE: usize = 1000;

const STRUCTURED_GRACE: Duration = Duration::from_secs(2);

fn stale_server_error(grace: Duration, discarded: usize) -> anyhow::Error {
    anyhow!(
        "this session's server predates structured rendering: {} ANSI render payloads arrived and \
         no render frames did within {:?}. Restart the session with a current zellij binary.",
        discarded,
        grace
    )
}

#[derive(Clone)]
pub struct LoopOptions {
    pub record_path: Option<PathBuf>,
    pub clipboard: Option<ClipboardHandle>,
    pub structured_grace: Duration,
    pub config_path: Option<PathBuf>,
    pub settings: Settings,
}

impl Default for LoopOptions {
    fn default() -> Self {
        LoopOptions {
            record_path: None,
            clipboard: None,
            structured_grace: STRUCTURED_GRACE,
            config_path: None,
            settings: Settings::default(),
        }
    }
}

#[derive(Debug)]
pub struct LoopOutcome {
    pub render_count: usize,
    pub discarded_ansi: usize,
    pub recorded_messages: usize,
    pub exit_reason: Option<ExitReason>,
    pub switch_to: Option<ConnectToSession>,
}

pub trait RenderSink {
    fn frame(&mut self, frame: Vec<u8>) -> bool;
    fn finished(
        &mut self,
        exit_reason: Option<&ExitReason>,
        switch_to: Option<ConnectToSession>,
    ) -> bool;
    fn acknowledges_frames(&self) -> bool;
    fn reconfigured(&mut self, _settings: Settings) -> bool {
        true
    }
    fn theme_mode(&mut self, _mode: HostTerminalThemeMode) -> bool {
        true
    }
}

pub struct DiscardingSink;

impl RenderSink for DiscardingSink {
    fn frame(&mut self, _frame: Vec<u8>) -> bool {
        true
    }
    fn finished(
        &mut self,
        _exit_reason: Option<&ExitReason>,
        _switch_to: Option<ConnectToSession>,
    ) -> bool {
        true
    }
    fn acknowledges_frames(&self) -> bool {
        false
    }
}

pub fn run(connection: Connection, options: LoopOptions) -> Result<LoopOutcome> {
    run_with_sink(connection, options, &mut DiscardingSink)
}

pub fn run_with_sink(
    connection: Connection,
    options: LoopOptions,
    sink: &mut dyn RenderSink,
) -> Result<LoopOutcome> {
    let Connection {
        sender,
        mut receiver,
        session_name,
        geometry,
        role,
    } = connection;

    let mut recorder = match &options.record_path {
        Some(path) => Some(Recorder::create(path, &session_name, geometry.get())?),
        None => None,
    };

    let mut render_count = 0usize;
    let mut discarded_ansi = 0usize;
    let mut undecodable_count = 0usize;
    let mut exit_reason = None;
    let mut switch_to: Option<ConnectToSession> = None;
    let mut stale_server: Option<anyhow::Error> = None;
    let mut dead_sink: Option<anyhow::Error> = None;
    let mut settings = options.settings.clone();
    let mut theme_mode: Option<HostTerminalThemeMode> = None;
    let mut paints = crate::options::resolve(&settings, theme_mode).paints;
    let attached_at = Instant::now();

    loop {
        let msg = match receiver.recv_msg() {
            Ok(msg) => {
                undecodable_count = 0;
                msg
            },
            Err(IpcReceiveError::Undecodable) => {
                undecodable_count += 1;
                if undecodable_count >= MAX_CONSECUTIVE_UNDECODABLE {
                    eprintln!(
                        "zellij-window: {} consecutive undecodable messages, disconnecting",
                        undecodable_count
                    );
                    break;
                }
                continue;
            },
            Err(IpcReceiveError::Disconnected) => break,
        };

        if let Some(recorder) = recorder.as_mut() {
            recorder.record(&msg)?;
        }

        match msg {
            ServerToClientMsg::RenderFrame { frame } => {
                render_count += 1;
                if !sink.acknowledges_frames() {
                    match zellij_utils::structured_render::decode(&frame) {
                        Ok(view) => send(
                            &sender,
                            ClientToServerMsg::RenderFrameAck {
                                seq: view.header().seq,
                            },
                        )?,
                        Err(e) => eprintln!(
                            "zellij-window: an undecodable frame cannot be acknowledged: {}",
                            e
                        ),
                    }
                }
                if !sink.frame(frame) {
                    dead_sink = Some(anyhow!(
                        "the window stopped accepting frames after {} of them; the event loop is gone",
                        render_count
                    ));
                    let _ = request_detach(&sender, role);
                    break;
                }
            },
            ServerToClientMsg::Render { .. } => {
                discarded_ansi += 1;
                if render_count == 0 && attached_at.elapsed() >= options.structured_grace {
                    stale_server =
                        Some(stale_server_error(options.structured_grace, discarded_ansi));
                    break;
                }
            },
            ServerToClientMsg::Exit {
                exit_reason: reason,
            } => {
                exit_reason = Some(reason);
                let _ = send(&sender, ClientToServerMsg::ClientExited);
                break;
            },
            ServerToClientMsg::SwitchSession { connect_to_session } => {
                switch_to = Some(connect_to_session);
                let _ = send(&sender, ClientToServerMsg::ClientExited);
                break;
            },
            ServerToClientMsg::QueryTerminalSize => {
                send(
                    &sender,
                    ClientToServerMsg::TerminalResize {
                        new_size: geometry.get().size(),
                    },
                )?;
            },
            ServerToClientMsg::ForwardQueryToHost {
                token, query_bytes, ..
            } => {
                let reply_bytes = host_reply::answer(
                    &query_bytes,
                    geometry.get(),
                    &paints,
                    options.clipboard.as_ref(),
                );
                send(
                    &sender,
                    ClientToServerMsg::ForwardedReplyFromHost { token, reply_bytes },
                )?;
            },
            ServerToClientMsg::ConfigFileUpdated => {
                let Some(config_path) = options.config_path.as_deref() else {
                    continue;
                };
                match settings::load(Some(config_path)) {
                    Ok(reloaded) => {
                        settings = reloaded;
                        paints = crate::options::resolve(&settings, theme_mode).paints;
                        sink.reconfigured(settings.clone());
                    },
                    Err(e) => eprintln!(
                        "zellij-window: keeping the options in force; \
                         the changed configuration at {:?} is unusable: {}",
                        config_path, e
                    ),
                }
            },
            ServerToClientMsg::HostTerminalThemeChanged { mode } => {
                theme_mode = Some(mode);
                paints = crate::options::resolve(&settings, theme_mode).paints;
                sink.theme_mode(mode);
            },
            _ => {},
        }
    }

    if !sink.finished(exit_reason.as_ref(), switch_to.clone()) {
        eprintln!("zellij-window: the window could not be told that the session ended");
    }

    let recorded_messages = match recorder {
        Some(recorder) => {
            let count = recorder.message_count();
            recorder.finish()?;
            count
        },
        None => 0,
    };

    if let Some(stale_server) = stale_server {
        let _ = send(&sender, ClientToServerMsg::ClientExited);
        return Err(stale_server);
    }

    if let Some(dead_sink) = dead_sink {
        let _ = send(&sender, ClientToServerMsg::ClientExited);
        return Err(dead_sink);
    }

    Ok(LoopOutcome {
        render_count,
        discarded_ansi,
        recorded_messages,
        exit_reason,
        switch_to,
    })
}

pub fn detach_on_signal(detacher: Detacher) {
    #[cfg(unix)]
    {
        use signal_hook::consts::{SIGINT, SIGTERM};
        use signal_hook::iterator::Signals;

        std::thread::spawn(move || {
            let mut signals = match Signals::new([SIGINT, SIGTERM]) {
                Ok(signals) => signals,
                Err(e) => {
                    eprintln!("zellij-window: failed to install signal handler: {}", e);
                    return;
                },
            };
            if signals.forever().next().is_some() {
                if let Err(e) = detacher.detach() {
                    eprintln!(
                        "zellij-window: the signal could not be passed on to a session, \
                         so the window quits: {}",
                        e
                    );
                    std::process::exit(1);
                }
            }
        });
    }
    #[cfg(not(unix))]
    {
        let _ = detacher;
    }
}

pub fn detach_after(detacher: Detacher, duration: std::time::Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(duration);
        let _ = detacher.detach();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{test_attach_at as attach_at, Capabilities, Geometry};
    use crate::test_server::FakeServer;
    use tempfile::TempDir;
    use zellij_utils::input::actions::Action;
    use zellij_utils::pane_size::Size;

    const HANDSHAKE_MESSAGES: usize = 8;

    fn geometry() -> Geometry {
        Geometry {
            rows: 40,
            cols: 120,
            cell_width: 10,
            cell_height: 20,
        }
    }

    fn render(seq: u64) -> ServerToClientMsg {
        let builder = zellij_utils::structured_render::FrameBuilder::new(120, 40, seq);
        ServerToClientMsg::RenderFrame {
            frame: builder.finish(),
        }
    }

    fn options() -> LoopOptions {
        LoopOptions::default()
    }

    #[derive(Default)]
    struct RecordingSink {
        reconfigured: Vec<Settings>,
        modes: Vec<HostTerminalThemeMode>,
    }

    impl RenderSink for RecordingSink {
        fn frame(&mut self, _frame: Vec<u8>) -> bool {
            true
        }
        fn finished(
            &mut self,
            _exit_reason: Option<&ExitReason>,
            _switch_to: Option<ConnectToSession>,
        ) -> bool {
            true
        }
        fn acknowledges_frames(&self) -> bool {
            false
        }
        fn reconfigured(&mut self, settings: Settings) -> bool {
            self.reconfigured.push(settings);
            true
        }
        fn theme_mode(&mut self, mode: HostTerminalThemeMode) -> bool {
            self.modes.push(mode);
            true
        }
    }

    fn background_query() -> ServerToClientMsg {
        ServerToClientMsg::ForwardQueryToHost {
            token: 1,
            query_bytes: b"\x1b]11;?\x1b\\".to_vec(),
            resolve_async: false,
        }
    }

    fn answered_background(replies: &[ClientToServerMsg]) -> String {
        replies
            .iter()
            .find_map(|msg| match msg {
                ClientToServerMsg::ForwardedReplyFromHost { reply_bytes, .. } => {
                    Some(String::from_utf8_lossy(reply_bytes).into_owned())
                },
                _ => None,
            })
            .expect("no host reply was sent")
    }

    fn styling(background: u8) -> zellij_utils::input::theme::Theme {
        zellij_utils::input::theme::Theme {
            sourced_from_external_file: false,
            terminal_colors: None,
            palette: zellij_utils::data::Styling {
                text_unselected: zellij_utils::data::StyleDeclaration {
                    background: zellij_utils::data::PaletteColor::Rgb((
                        background, background, background,
                    )),
                    ..zellij_utils::data::DEFAULT_STYLES.text_unselected
                },
                ..zellij_utils::data::DEFAULT_STYLES
            },
        }
    }

    fn driven(
        options: LoopOptions,
        script: impl FnOnce(&mut crate::test_server::ServerSide) + Send + 'static,
    ) -> (RecordingSink, Vec<ClientToServerMsg>) {
        let server = FakeServer::spawn(move |side| {
            side.expect(HANDSHAKE_MESSAGES);
            script(side);
            side.send(ServerToClientMsg::Exit {
                exit_reason: ExitReason::NormalDetached,
            });
            side.expect(2);
        });
        let connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        let mut sink = RecordingSink::default();
        run_with_sink(connection, options, &mut sink).unwrap();
        let received = server.finish();
        (sink, received[HANDSHAKE_MESSAGES..].to_vec())
    }

    #[test]
    fn a_theme_mode_change_reaches_the_window_and_the_colors_a_pane_is_told() {
        let mut options = options();
        options.settings = Settings {
            theme_dark: Some(styling(1)),
            theme_light: Some(styling(2)),
            ..Settings::default()
        };

        let (sink, replies) = driven(options, |side| {
            side.send(ServerToClientMsg::HostTerminalThemeChanged {
                mode: HostTerminalThemeMode::Light,
            });
            side.send(background_query());
        });

        assert_eq!(sink.modes, vec![HostTerminalThemeMode::Light]);
        assert_eq!(
            answered_background(&replies),
            "\u{1b}]11;rgb:0202/0202/0202\u{1b}\\",
            "a pane asking what its terminal looks like was told the mode the session left"
        );
    }

    #[test]
    fn a_theme_mode_change_before_any_mode_is_reported_is_the_dark_one() {
        let mut options = options();
        options.settings = Settings {
            theme_dark: Some(styling(1)),
            theme_light: Some(styling(2)),
            ..Settings::default()
        };

        let (sink, replies) = driven(options, |side| {
            side.send(background_query());
        });

        assert!(sink.modes.is_empty());
        assert_eq!(
            answered_background(&replies),
            "\u{1b}]11;rgb:0101/0101/0101\u{1b}\\"
        );
    }

    #[test]
    fn a_changed_configuration_is_re_read_and_handed_to_the_window() {
        let directory = TempDir::new().unwrap();
        let config_path = directory.path().join("config.kdl");
        std::fs::write(
            &config_path,
            "window {\n theme {\n background \"#0a0a0a\"\n }\n}\n",
        )
        .unwrap();

        let mut options = options();
        options.config_path = Some(config_path);

        let (sink, replies) = driven(options, |side| {
            side.send(ServerToClientMsg::ConfigFileUpdated);
            side.send(background_query());
        });

        assert_eq!(sink.reconfigured.len(), 1);
        assert_eq!(
            sink.reconfigured[0]
                .section
                .theme
                .as_ref()
                .unwrap()
                .background,
            Some(zellij_utils::data::PaletteColor::Rgb((10, 10, 10)))
        );
        assert_eq!(
            answered_background(&replies),
            "\u{1b}]11;rgb:0a0a/0a0a/0a0a\u{1b}\\",
            "the colours a pane is told must move with the configuration"
        );
    }

    #[test]
    fn a_local_session_reloads_once_per_edit_because_the_window_watches_no_file_of_its_own() {
        let directory = TempDir::new().unwrap();
        let config_path = directory.path().join("config.kdl");
        let write = |background: &str| {
            std::fs::write(
                &config_path,
                format!(
                    "window {{\n theme {{\n background \"{}\"\n }}\n}}\n",
                    background
                ),
            )
            .unwrap()
        };
        write("#0a0a0a");

        let mut options = options();
        options.config_path = Some(config_path.clone());

        let edited = config_path.clone();
        let (sink, _replies) = driven(options, move |side| {
            std::fs::write(
                &edited,
                "window {\n theme {\n background \"#0b0b0b\"\n }\n}\n",
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(1500));
            std::fs::write(
                &edited,
                "window {\n theme {\n background \"#0c0c0c\"\n }\n}\n",
            )
            .unwrap();
            side.send(ServerToClientMsg::ConfigFileUpdated);
            side.send(background_query());
        });

        assert_eq!(
            sink.reconfigured.len(),
            1,
            "a local session's window reloaded more than once for the one signal it was sent; \
             two edits spanning longer than a watcher's poll interval must reach it only through \
             the session"
        );
        assert_eq!(
            sink.reconfigured[0]
                .section
                .theme
                .as_ref()
                .unwrap()
                .background,
            Some(zellij_utils::data::PaletteColor::Rgb((12, 12, 12)))
        );
    }

    #[test]
    fn a_changed_configuration_that_does_not_parse_leaves_the_options_in_force() {
        let directory = TempDir::new().unwrap();
        let config_path = directory.path().join("config.kdl");
        std::fs::write(&config_path, "window {\n font_size \"enormous\"\n}\n").unwrap();

        let mut options = options();
        options.config_path = Some(config_path);
        options.settings = Settings {
            theme: Some(styling(7)),
            ..Settings::default()
        };

        let (sink, replies) = driven(options, |side| {
            side.send(ServerToClientMsg::ConfigFileUpdated);
            side.send(background_query());
        });

        assert!(
            sink.reconfigured.is_empty(),
            "the window was told about a configuration that does not parse"
        );
        assert_eq!(
            answered_background(&replies),
            "\u{1b}]11;rgb:0707/0707/0707\u{1b}\\",
            "the options in force were disturbed by a configuration that does not parse"
        );
    }

    #[test]
    fn a_client_with_no_configuration_file_ignores_the_reload_signal() {
        let (sink, replies) = driven(options(), |side| {
            side.send(ServerToClientMsg::ConfigFileUpdated);
            side.send(background_query());
        });

        assert!(sink.reconfigured.is_empty());
        assert_eq!(
            answered_background(&replies),
            "\u{1b}]11;rgb:0000/0000/0000\u{1b}\\"
        );
    }

    #[test]
    fn server_queries_are_answered_and_exit_is_acknowledged() {
        let server = FakeServer::spawn(|side| {
            side.expect(HANDSHAKE_MESSAGES);
            side.send(render(0));
            side.send(ServerToClientMsg::QueryTerminalSize);
            side.send(ServerToClientMsg::ForwardQueryToHost {
                token: 42,
                query_bytes: vec![1, 2, 3],
                resolve_async: false,
            });
            side.send(ServerToClientMsg::ForwardQueryToHost {
                token: 43,
                query_bytes: b"\x1b[16t".to_vec(),
                resolve_async: false,
            });
            side.send(ServerToClientMsg::Exit {
                exit_reason: ExitReason::NormalDetached,
            });
            side.expect(5);
        });

        let connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        let outcome = run(connection, options()).unwrap();

        assert_eq!(outcome.render_count, 1);
        assert_eq!(outcome.exit_reason, Some(ExitReason::NormalDetached));

        let replies = &server.finish()[HANDSHAKE_MESSAGES..];
        assert_eq!(
            replies,
            &[
                ClientToServerMsg::RenderFrameAck { seq: 0 },
                ClientToServerMsg::TerminalResize {
                    new_size: Size {
                        rows: 40,
                        cols: 120
                    }
                },
                ClientToServerMsg::ForwardedReplyFromHost {
                    token: 42,
                    reply_bytes: Vec::new()
                },
                ClientToServerMsg::ForwardedReplyFromHost {
                    token: 43,
                    reply_bytes: b"\x1b[6;20;10t".to_vec()
                },
                ClientToServerMsg::ClientExited,
            ]
        );
    }

    #[test]
    fn the_pixel_answer_follows_the_geometry_the_window_last_reported() {
        let server = FakeServer::spawn(|side| {
            side.expect(HANDSHAKE_MESSAGES);
            side.send(ServerToClientMsg::ForwardQueryToHost {
                token: 1,
                query_bytes: b"\x1b[14t".to_vec(),
                resolve_async: false,
            });
            side.send(ServerToClientMsg::Exit {
                exit_reason: ExitReason::NormalDetached,
            });
            side.expect(4);
        });

        let connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        crate::connection::resize(
            &connection.sender,
            &connection.geometry,
            Geometry {
                rows: 24,
                cols: 80,
                cell_width: 9,
                cell_height: 18,
            },
        )
        .unwrap();
        run(connection, options()).unwrap();

        let replies = &server.finish()[HANDSHAKE_MESSAGES + 2..];
        assert_eq!(
            replies,
            &[
                ClientToServerMsg::ForwardedReplyFromHost {
                    token: 1,
                    reply_bytes: b"\x1b[4;432;720t".to_vec()
                },
                ClientToServerMsg::ClientExited,
            ]
        );
    }

    #[test]
    fn a_clipboard_read_and_a_color_query_are_both_answered_authoritatively() {
        let server = FakeServer::spawn(|side| {
            side.expect(HANDSHAKE_MESSAGES);
            side.send(ServerToClientMsg::ForwardQueryToHost {
                token: 7,
                query_bytes: b"\x1b]52;c;?\x1b\\".to_vec(),
                resolve_async: true,
            });
            side.send(ServerToClientMsg::ForwardQueryToHost {
                token: 8,
                query_bytes: b"\x1b]11;?\x1b\\".to_vec(),
                resolve_async: false,
            });
            side.send(ServerToClientMsg::Exit {
                exit_reason: ExitReason::NormalDetached,
            });
            side.expect(3);
        });

        let mut clipboard = crate::clipboard::Clipboard::in_memory();
        clipboard.set("copied");
        let mut options = options();
        options.clipboard = Some(std::sync::Arc::new(std::sync::Mutex::new(clipboard)));

        let connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        run(connection, options).unwrap();

        let replies = &server.finish()[HANDSHAKE_MESSAGES..];
        assert_eq!(
            replies,
            &[
                ClientToServerMsg::ForwardedReplyFromHost {
                    token: 7,
                    reply_bytes: b"\x1b]52;c;Y29waWVk\x1b\\".to_vec()
                },
                ClientToServerMsg::ForwardedReplyFromHost {
                    token: 8,
                    reply_bytes: b"\x1b]11;rgb:0000/0000/0000\x1b\\".to_vec()
                },
                ClientToServerMsg::ClientExited,
            ]
        );
    }

    #[test]
    fn a_server_that_only_speaks_ansi_is_named_rather_than_shown_as_a_blank_window() {
        let server = FakeServer::spawn(|side| {
            side.expect(HANDSHAKE_MESSAGES);
            side.send(ServerToClientMsg::Render {
                content: "\u{1b}[31mnever parsed".to_owned(),
            });
            side.expect(1);
        });

        let connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        let error = run(
            connection,
            LoopOptions {
                structured_grace: Duration::ZERO,
                ..options()
            },
        )
        .unwrap_err()
        .to_string();
        server.finish();

        assert!(error.contains("predates structured rendering"), "{}", error);
        assert!(error.contains("Restart the session"), "{}", error);
    }

    #[test]
    fn ansi_inside_the_grace_period_is_discarded_and_the_loop_carries_on() {
        let server = FakeServer::spawn(|side| {
            side.expect(HANDSHAKE_MESSAGES);
            side.send(ServerToClientMsg::Render {
                content: "\u{1b}[31mnever parsed".to_owned(),
            });
            side.send(render(0));
            side.send(ServerToClientMsg::Exit {
                exit_reason: ExitReason::NormalDetached,
            });
            side.expect(1);
        });

        let connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        let outcome = run(connection, options()).unwrap();
        server.finish();

        assert_eq!(outcome.discarded_ansi, 1);
        assert_eq!(outcome.render_count, 1);
    }

    #[test]
    fn an_abrupt_disconnect_ends_the_loop_without_an_exit_reason() {
        let server = FakeServer::spawn(|side| {
            side.expect(HANDSHAKE_MESSAGES);
            side.send(render(0));
        });

        let connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        let outcome = run(connection, options()).unwrap();

        assert_eq!(outcome.render_count, 1);
        assert_eq!(outcome.exit_reason, None);
        server.finish();
    }

    #[test]
    fn recording_captures_the_whole_stream_including_non_render_messages() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("capture.jsonl");
        let server = FakeServer::spawn(|side| {
            side.expect(HANDSHAKE_MESSAGES);
            side.send(ServerToClientMsg::UnblockInputThread);
            side.send(render(0));
            side.send(ServerToClientMsg::Exit {
                exit_reason: ExitReason::NormalDetached,
            });
            side.expect(1);
        });

        let connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        let outcome = run(
            connection,
            LoopOptions {
                record_path: Some(path.clone()),
                ..options()
            },
        )
        .unwrap();
        server.finish();

        assert_eq!(outcome.recorded_messages, 3);
        let lines = std::fs::read_to_string(&path).unwrap();
        assert_eq!(lines.lines().count(), 4);
    }
}

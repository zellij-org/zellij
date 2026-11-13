use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use zellij_utils::ipc::{ClientToServerMsg, ExitReason, IpcReceiveError, ServerToClientMsg};

use zellij_utils::data::HostTerminalThemeMode;

use zellij_utils::data::ConnectToSession;

use crate::config::settings::Settings;
use crate::emulation::color::Paints;
use crate::input::clipboard::{self, ClipboardHandle};
use crate::session::connection::{
    request_detach, send, Connection, Detacher, GeometryHandle, Role, SharedSender,
};
use crate::session::host_reply;
use crate::session::recorder::Recorder;

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
    pub settings: Settings,
    pub font_families: Option<Vec<String>>,
    pub allow_remote_clipboard_read: bool,
}

impl Default for LoopOptions {
    fn default() -> Self {
        LoopOptions {
            record_path: None,
            clipboard: None,
            structured_grace: STRUCTURED_GRACE,
            settings: Settings::default(),
            font_families: None,
            allow_remote_clipboard_read: false,
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

#[derive(Debug, Clone, PartialEq)]
pub enum Ending {
    Exited(ExitReason),
    Switched(ConnectToSession),
    Lost,
    Failed(String),
}

pub trait RenderSink {
    fn frame(&mut self, frame: Vec<u8>) -> bool;
    fn finished(&mut self, ending: Ending) -> bool;
    fn acknowledges_frames(&self) -> bool;
    fn settings_pushed(&mut self, _settings: Settings) -> bool {
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
    fn finished(&mut self, _ending: Ending) -> bool {
        true
    }
    fn acknowledges_frames(&self) -> bool {
        false
    }
}

fn paste_shortcut_of(settings: &Settings) -> Option<String> {
    crate::config::options::resolve(settings, None)
        .paste_keys
        .first()
        .map(|key| key.to_string())
}

fn paste_support(settings: &Settings) -> ClientToServerMsg {
    ClientToServerMsg::ClipboardPasteSupport {
        supported: true,
        shortcut: paste_shortcut_of(settings),
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
        remote,
    } = connection;
    let clipboard_for_queries = options
        .clipboard
        .as_ref()
        .filter(|_| !remote || options.allow_remote_clipboard_read);

    let mut recorder = match &options.record_path {
        Some(path) => match Recorder::create(path, &session_name, geometry.get()) {
            Ok(recorder) => Some(recorder),
            Err(e) => {
                sink.finished(Ending::Failed(format!("{:#}", e)));
                let _ = send(&sender, ClientToServerMsg::ClientExited);
                return Err(e);
            },
        },
        None => None,
    };

    let mut served = Served::start(
        &sender,
        &geometry,
        role,
        &options,
        clipboard_for_queries,
        sink,
    );
    let mut undecodable_count = 0usize;

    loop {
        let msg = match receiver.recv_msg() {
            Ok(msg) => {
                undecodable_count = 0;
                msg
            },
            Err(IpcReceiveError::Undecodable) => {
                undecodable_count += 1;
                if undecodable_count >= MAX_CONSECUTIVE_UNDECODABLE {
                    served.failure = Some(anyhow!(
                        "{} consecutive messages from the session server could not be read, so \
                         the window disconnected. The server may be running a different version \
                         of zellij.",
                        undecodable_count
                    ));
                    break;
                }
                continue;
            },
            Err(IpcReceiveError::Disconnected) => break,
        };

        if let Some(recorder) = recorder.as_mut() {
            if let Err(e) = recorder.record(&msg) {
                served.failure = Some(e.context("failed to record a message from the session"));
                break;
            }
        }

        match served.handle(msg) {
            Ok(Flow::Continue) => {},
            Ok(Flow::Stop) => break,
            Err(e) => {
                served.failure = Some(e);
                break;
            },
        }
    }

    served.finish(recorder)
}

enum Flow {
    Continue,
    Stop,
}

struct Served<'a> {
    sender: &'a SharedSender,
    geometry: &'a GeometryHandle,
    role: Role,
    options: &'a LoopOptions,
    clipboard_for_queries: Option<&'a ClipboardHandle>,
    sink: &'a mut dyn RenderSink,
    render_count: usize,
    discarded_ansi: usize,
    exit_reason: Option<ExitReason>,
    switch_to: Option<ConnectToSession>,
    stale_server: Option<anyhow::Error>,
    dead_sink: Option<anyhow::Error>,
    failure: Option<anyhow::Error>,
    settings: Settings,
    theme_mode: Option<HostTerminalThemeMode>,
    paints: Paints,
    attached_at: Instant,
    can_paste: bool,
    paste_shortcut: Option<Option<String>>,
}

impl<'a> Served<'a> {
    fn start(
        sender: &'a SharedSender,
        geometry: &'a GeometryHandle,
        role: Role,
        options: &'a LoopOptions,
        clipboard_for_queries: Option<&'a ClipboardHandle>,
        sink: &'a mut dyn RenderSink,
    ) -> Self {
        let settings = options.settings.clone();
        let theme_mode: Option<HostTerminalThemeMode> = None;
        let paints = crate::config::options::resolve(&settings, theme_mode).paints;
        let attached_at = Instant::now();
        let can_paste = options.clipboard.is_some() && role != Role::Watcher;
        let mut paste_shortcut = None;
        if can_paste {
            paste_shortcut = Some(paste_shortcut_of(&settings));
            if let Err(e) = send(sender, paste_support(&settings)) {
                report!(
                    "the session could not be told that this window can paste: {}",
                    e
                );
            }
        }
        if let Some(families) = options.font_families.clone() {
            if let Err(e) = send(sender, ClientToServerMsg::FontFamilies { families }) {
                report!(
                    "the installed fonts could not be listed for the settings: {}",
                    e
                );
            }
        }
        Served {
            sender,
            geometry,
            role,
            options,
            clipboard_for_queries,
            sink,
            render_count: 0,
            discarded_ansi: 0,
            exit_reason: None,
            switch_to: None,
            stale_server: None,
            dead_sink: None,
            failure: None,
            settings,
            theme_mode,
            paints,
            attached_at,
            can_paste,
            paste_shortcut,
        }
    }

    fn handle(&mut self, msg: ServerToClientMsg) -> Result<Flow> {
        match msg {
            ServerToClientMsg::RenderFrame { frame } => return self.frame(frame),
            ServerToClientMsg::Render { .. } => return Ok(self.ansi_render()),
            ServerToClientMsg::Exit {
                exit_reason: reason,
            } => {
                self.exit_reason = Some(reason);
                return Ok(self.leave());
            },
            ServerToClientMsg::SwitchSession { connect_to_session } => {
                self.switch_to = Some(connect_to_session);
                return Ok(self.leave());
            },
            ServerToClientMsg::QueryTerminalSize => self.report_size()?,
            ServerToClientMsg::ForwardQueryToHost {
                token, query_bytes, ..
            } => self.answer_host_query(token, &query_bytes)?,
            ServerToClientMsg::TerminalSettingsChanged { settings } => {
                self.settings_pushed(*settings)
            },
            ServerToClientMsg::HostTerminalThemeChanged { mode } => self.theme_changed(mode),
            ServerToClientMsg::Log { lines } => {
                for line in lines {
                    log::info!("{}", line);
                }
            },
            ServerToClientMsg::LogError { lines } => {
                for line in lines {
                    log::error!("{}", line);
                }
            },
            ServerToClientMsg::PasteFromClipboard { pane_id } => self.paste(pane_id)?,
            _ => {},
        }
        Ok(Flow::Continue)
    }

    fn frame(&mut self, frame: Vec<u8>) -> Result<Flow> {
        self.render_count += 1;
        if !self.sink.acknowledges_frames() {
            match zellij_utils::structured_render::decode(&frame) {
                Ok(view) => send(
                    self.sender,
                    ClientToServerMsg::RenderFrameAck {
                        seq: view.header().seq,
                    },
                )?,
                Err(e) => report!("an undecodable frame cannot be acknowledged: {}", e),
            }
        }
        if !self.sink.frame(frame) {
            self.dead_sink = Some(anyhow!(
                "the window stopped accepting frames after {} of them; the event loop is gone",
                self.render_count
            ));
            let _ = request_detach(self.sender, self.role);
            return Ok(Flow::Stop);
        }
        Ok(Flow::Continue)
    }

    fn ansi_render(&mut self) -> Flow {
        self.discarded_ansi += 1;
        let grace = self.options.structured_grace;
        if self.render_count == 0 && self.attached_at.elapsed() >= grace {
            self.stale_server = Some(stale_server_error(grace, self.discarded_ansi));
            return Flow::Stop;
        }
        Flow::Continue
    }

    fn leave(&self) -> Flow {
        let _ = send(self.sender, ClientToServerMsg::ClientExited);
        Flow::Stop
    }

    fn report_size(&self) -> Result<()> {
        send(
            self.sender,
            ClientToServerMsg::TerminalResize {
                new_size: self.geometry.get().size(),
            },
        )
    }

    fn answer_host_query(&self, token: u32, query_bytes: &[u8]) -> Result<()> {
        let reply_bytes = host_reply::answer(
            query_bytes,
            self.geometry.get(),
            &self.paints,
            self.clipboard_for_queries,
        );
        send(
            self.sender,
            ClientToServerMsg::ForwardedReplyFromHost { token, reply_bytes },
        )
    }

    fn settings_pushed(&mut self, pushed: Settings) {
        if pushed != self.settings {
            self.settings = pushed;
            self.repaint();
            self.sink.settings_pushed(self.settings.clone());
            self.refresh_paste_shortcut();
        }
    }

    fn theme_changed(&mut self, mode: HostTerminalThemeMode) {
        self.theme_mode = Some(mode);
        self.repaint();
        self.sink.theme_mode(mode);
    }

    fn repaint(&mut self) {
        self.paints = crate::config::options::resolve(&self.settings, self.theme_mode).paints;
    }

    fn refresh_paste_shortcut(&mut self) {
        if self.can_paste && self.paste_shortcut != Some(paste_shortcut_of(&self.settings)) {
            self.paste_shortcut = Some(paste_shortcut_of(&self.settings));
            let _ = send(self.sender, paste_support(&self.settings));
        }
    }

    fn paste(&self, pane_id: Option<zellij_utils::data::PaneId>) -> Result<()> {
        let text = self
            .options
            .clipboard
            .as_ref()
            .and_then(|clipboard| clipboard.lock().ok().and_then(|mut c| c.get()))
            .filter(|text| !text.is_empty());
        match text {
            Some(chars) => send(self.sender, clipboard::paste_message_into(chars, pane_id)),
            None => Ok(()),
        }
    }

    fn finish(mut self, recorder: Option<Recorder>) -> Result<LoopOutcome> {
        if let Some(stale_server) = self.stale_server.take() {
            self.failure = Some(stale_server);
        }
        let ending = ending_of(
            self.failure.as_ref(),
            self.exit_reason.as_ref(),
            self.switch_to.as_ref(),
        );
        if !self.sink.finished(ending) {
            report!("the window could not be told that the session ended");
        }

        let recorded_messages = match recorder {
            Some(recorder) => {
                let count = recorder.message_count();
                recorder.finish()?;
                count
            },
            None => 0,
        };

        if let Some(failure) = self.failure {
            let _ = send(self.sender, ClientToServerMsg::ClientExited);
            return Err(failure);
        }

        if let Some(dead_sink) = self.dead_sink {
            let _ = send(self.sender, ClientToServerMsg::ClientExited);
            return Err(dead_sink);
        }

        Ok(LoopOutcome {
            render_count: self.render_count,
            discarded_ansi: self.discarded_ansi,
            recorded_messages,
            exit_reason: self.exit_reason,
            switch_to: self.switch_to,
        })
    }
}

fn ending_of(
    failure: Option<&anyhow::Error>,
    exit_reason: Option<&ExitReason>,
    switch_to: Option<&ConnectToSession>,
) -> Ending {
    if let Some(failure) = failure {
        return Ending::Failed(format!("{:#}", failure));
    }
    if let Some(switch_to) = switch_to {
        return Ending::Switched(switch_to.clone());
    }
    match exit_reason {
        Some(reason) => Ending::Exited(reason.clone()),
        None => Ending::Lost,
    }
}

#[cfg_attr(windows, path = "client_loop/console_events.rs")]
#[cfg_attr(not(windows), path = "client_loop/signals.rs")]
mod signals;

pub use signals::detach_on_signal;

pub fn detach_after(detacher: Detacher, duration: std::time::Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(duration);
        let _ = detacher.detach();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::connection::{test_attach_at as attach_at, Capabilities, Geometry};
    use crate::testing::test_server::FakeServer;
    use tempfile::TempDir;
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
        pushed: Vec<Settings>,
        modes: Vec<HostTerminalThemeMode>,
        endings: Vec<Ending>,
    }

    impl RenderSink for RecordingSink {
        fn frame(&mut self, _frame: Vec<u8>) -> bool {
            true
        }
        fn finished(&mut self, ending: Ending) -> bool {
            self.endings.push(ending);
            true
        }
        fn acknowledges_frames(&self) -> bool {
            false
        }
        fn settings_pushed(&mut self, settings: Settings) -> bool {
            self.pushed.push(settings);
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
        script: impl FnOnce(&mut crate::testing::test_server::ServerSide) + Send + 'static,
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
    fn a_changed_configuration_file_is_left_to_the_session_so_unsaved_settings_stay() {
        let mut pushed = Settings::default();
        pushed.section.font = Some("New Mono".to_owned());
        let sent = pushed.clone();
        let (sink, _replies) = driven(options(), move |side| {
            side.send(ServerToClientMsg::TerminalSettingsChanged {
                settings: Box::new(sent),
            });
            side.send(ServerToClientMsg::ConfigFileUpdated);
            side.send(background_query());
        });

        assert_eq!(
            sink.pushed,
            vec![pushed],
            "the window replaced the settings the session pushed with the file on disk"
        );
    }

    #[test]
    fn the_installed_fonts_are_listed_to_the_session_once_attached() {
        let families = vec!["Iosevka Term".to_owned(), "JetBrains Mono".to_owned()];
        let mut options = options();
        options.font_families = Some(families.clone());
        let (_sink, replies) = driven(options, |_side| {});
        assert_eq!(
            replies.first(),
            Some(&ClientToServerMsg::FontFamilies { families })
        );
    }

    #[test]
    fn settings_pushed_by_the_session_reach_the_window_and_the_colors_a_pane_is_told() {
        let pushed = Settings {
            theme: Some(styling(5)),
            ..Settings::default()
        };
        let sent = pushed.clone();
        let (sink, replies) = driven(options(), move |side| {
            side.send(ServerToClientMsg::TerminalSettingsChanged {
                settings: Box::new(sent.clone()),
            });
            side.send(ServerToClientMsg::TerminalSettingsChanged {
                settings: Box::new(sent),
            });
            side.send(background_query());
        });

        assert_eq!(
            sink.pushed,
            vec![pushed],
            "the same settings pushed twice are applied once"
        );
        assert_eq!(
            answered_background(&replies),
            "\u{1b}]11;rgb:0505/0505/0505\u{1b}\\",
            "a theme picked in the settings must reach the window colours"
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
        crate::session::connection::resize(
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
            side.expect(4);
        });

        let mut clipboard = crate::input::clipboard::Clipboard::in_memory();
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
                paste_support(&LoopOptions::default().settings),
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

    fn clipboard_reply_over(remote: bool, allow_remote_clipboard_read: bool) -> Vec<u8> {
        let server = FakeServer::spawn(|side| {
            side.expect(HANDSHAKE_MESSAGES);
            side.send(ServerToClientMsg::ForwardQueryToHost {
                token: 7,
                query_bytes: b"\x1b]52;c;?\x1b\\".to_vec(),
                resolve_async: true,
            });
            side.send(ServerToClientMsg::Exit {
                exit_reason: ExitReason::NormalDetached,
            });
            side.expect(3);
        });

        let mut clipboard = crate::input::clipboard::Clipboard::in_memory();
        clipboard.set("copied");
        let mut options = options();
        options.clipboard = Some(std::sync::Arc::new(std::sync::Mutex::new(clipboard)));
        options.allow_remote_clipboard_read = allow_remote_clipboard_read;

        let mut connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        connection.remote = remote;
        run(connection, options).unwrap();

        match &server.finish()[HANDSHAKE_MESSAGES + 1] {
            ClientToServerMsg::ForwardedReplyFromHost { token, reply_bytes } => {
                assert_eq!(*token, 7);
                reply_bytes.clone()
            },
            other => panic!("expected a forwarded reply, got {:?}", other),
        }
    }

    #[test]
    fn a_paste_request_pastes_the_clipboard_into_the_requested_pane() {
        let server = FakeServer::spawn(|side| {
            side.expect(HANDSHAKE_MESSAGES + 1);
            side.send(ServerToClientMsg::PasteFromClipboard {
                pane_id: Some(zellij_utils::data::PaneId::Terminal(4)),
            });
            side.send(ServerToClientMsg::Exit {
                exit_reason: ExitReason::NormalDetached,
            });
            side.expect(2);
        });

        let mut clipboard = crate::input::clipboard::Clipboard::in_memory();
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
                paste_support(&LoopOptions::default().settings),
                crate::input::clipboard::paste_message_into(
                    "copied".to_owned(),
                    Some(zellij_utils::data::PaneId::Terminal(4))
                ),
                ClientToServerMsg::ClientExited,
            ]
        );
    }

    #[test]
    fn a_remote_session_gets_no_clipboard_unless_the_window_allows_it() {
        let copied = b"\x1b]52;c;Y29waWVk\x1b\\".to_vec();
        assert_eq!(clipboard_reply_over(false, false), copied);
        assert!(!clipboard_reply_over(true, false)
            .windows(b"Y29waWVk".len())
            .any(|w| w == b"Y29waWVk"));
        assert_eq!(clipboard_reply_over(true, true), copied);
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

    fn ending_for(
        options: LoopOptions,
        script: impl FnOnce(&mut crate::testing::test_server::ServerSide) + Send + 'static,
    ) -> Ending {
        let server = FakeServer::spawn(move |side| {
            side.expect(HANDSHAKE_MESSAGES);
            script(side);
        });
        let connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        let mut sink = RecordingSink::default();
        let _ = run_with_sink(connection, options, &mut sink);
        server.finish();
        assert_eq!(
            sink.endings.len(),
            1,
            "the window is told exactly once, got {:?}",
            sink.endings
        );
        sink.endings.remove(0)
    }

    #[test]
    fn the_window_is_told_the_reason_the_server_gave() {
        for reason in [
            ExitReason::NormalDetached,
            ExitReason::KickedByHost,
            ExitReason::Error("the screen thread failed".to_owned()),
        ] {
            let sent = reason.clone();
            let ending = ending_for(options(), move |side| {
                side.send(ServerToClientMsg::Exit { exit_reason: sent });
                side.expect(1);
            });
            assert_eq!(ending, Ending::Exited(reason));
        }
    }

    #[test]
    fn the_window_is_told_when_the_connection_drops_without_a_reason() {
        let ending = ending_for(options(), |side| side.send(render(0)));
        assert_eq!(ending, Ending::Lost);
    }

    #[test]
    fn the_window_is_told_where_the_session_switches_to() {
        let target = ConnectToSession {
            name: Some("elsewhere".to_owned()),
            ..ConnectToSession::default()
        };
        let sent = target.clone();
        let ending = ending_for(options(), move |side| {
            side.send(ServerToClientMsg::SwitchSession {
                connect_to_session: sent,
            });
            side.expect(1);
        });
        assert_eq!(ending, Ending::Switched(target));
    }

    #[test]
    fn the_window_is_told_when_the_server_is_too_old_to_draw_for_it() {
        let ending = ending_for(
            LoopOptions {
                structured_grace: Duration::ZERO,
                ..options()
            },
            |side| {
                side.send(ServerToClientMsg::Render {
                    content: "\u{1b}[31mnever parsed".to_owned(),
                });
                side.expect(1);
            },
        );
        match ending {
            Ending::Failed(message) => {
                assert!(
                    message.contains("predates structured rendering"),
                    "{}",
                    message
                )
            },
            other => panic!("expected a failure, got {:?}", other),
        }
    }

    #[test]
    fn the_window_is_told_when_a_recording_cannot_be_made() {
        let dir = TempDir::new().unwrap();
        let ending = ending_for(
            LoopOptions {
                record_path: Some(dir.path().to_path_buf()),
                ..options()
            },
            |side| side.expect(1),
        );
        assert!(matches!(ending, Ending::Failed(_)), "{:?}", ending);
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

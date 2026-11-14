use crate::emulation::palette;
use crate::emulation::terminal::{FrameError, TerminalState};
use crate::session::client_loop::Ending;
use crate::session::connection::{request_detach, request_force_close, send, Geometry};
use crate::window::handover_to;
use crate::window::window_title;
use crate::window::window_to_reuse;
use crate::window::App;
use crate::window::Followed;
use crate::window::Switched;
use anyhow::{anyhow, Result};
use std::time::Instant;
use winit::window::UserAttentionType;
use zellij_utils::data::ConnectToSession;
use zellij_utils::ipc::ClientToServerMsg;

impl App {
    pub(in crate::window) fn on_frame(&mut self, frame: &[u8]) {
        let settings = self.scroll_settings();
        let cell_height = self.metrics.height;
        let now = Instant::now();
        let scroll = &mut self.scroll;
        let watching = self.momentum.watches_frames();
        let mut hints = None;
        let applied = self.state.apply_frame_with(frame, |before, view| {
            if watching {
                hints = Some(view.scroll());
            }
            scroll.note_frame(before, view, settings, cell_height, now)
        });
        if self.scroll.is_active() {
            self.schedule_draw();
        }
        match applied {
            Ok(applied) => {
                if let Some(hints) = hints {
                    self.momentum.note_frame(&hints, self.state.geometry());
                }
                self.retained.mark(&applied.damage);
                self.schedule_draw();
                self.acknowledge(applied.seq);
            },
            Err(FrameError::Unapplicable { seq, error }) => {
                self.scroll.cancel();
                self.stop_momentum();
                report!("dropping a frame the viewport has moved past: {}", error);
                self.acknowledge(seq);
                return;
            },
            Err(FrameError::Undecodable(e)) => {
                if let Err(detach_error) = self.detach() {
                    report!(
                        "failed to detach after an unreadable frame arrived: {}",
                        detach_error
                    );
                }
                self.stranded(anyhow!(
                    "The session sent a screen update this window cannot read ({}). The session \
                     may be running a different version of zellij.",
                    e
                ));
                return;
            },
        }
        self.copy();
        self.retitle();
        self.attend();
        self.refresh_hover();
        self.refresh_pointer();
    }

    pub(in crate::window) fn tell(&self, msg: ClientToServerMsg) -> Result<()> {
        match &self.sender {
            Some(sender) => send(sender, msg),
            None => Ok(()),
        }
    }

    pub(in crate::window) fn detach(&self) -> Result<()> {
        match &self.sender {
            Some(sender) => request_detach(sender, self.role),
            None => Ok(()),
        }
    }

    pub(in crate::window) fn closing(&mut self) -> bool {
        self.leaving = true;
        let result = self.detach();
        self.report_close_failure(result)
    }

    fn force_closing(&mut self) -> bool {
        self.leaving = true;
        let on_force_close = self.settings.on_force_close.unwrap_or_default();
        let result = match &self.sender {
            Some(sender) => request_force_close(sender, self.role, on_force_close),
            None => Ok(()),
        };
        self.report_close_failure(result)
    }

    fn report_close_failure(&self, result: Result<()>) -> bool {
        if let Err(e) = result {
            report!(
                "the session could not be asked to let the window go, \
                 so the window closes on its own: {}",
                e
            );
        }
        true
    }

    pub(in crate::window) fn set_title(&mut self, title: String) {
        self.title = title;
        if let Some(surfaces) = &self.surfaces {
            surfaces.window.set_title(&self.title);
        }
    }

    pub(in crate::window) fn acknowledge(&mut self, seq: u64) {
        if let Err(e) = self.tell(ClientToServerMsg::RenderFrameAck { seq }) {
            report!("failed to acknowledge a frame: {}", e);
        }
    }

    pub(in crate::window) fn retitle(&mut self) {
        let Some(title) = self.state.take_title() else {
            return;
        };
        let title = title.unwrap_or_else(|| self.title.clone());
        if let Some(surfaces) = &self.surfaces {
            surfaces.window.set_title(&title);
        }
    }

    pub(in crate::window) fn attend(&mut self) {
        let rung = self.state.take_bells() > 0;
        let notified = !self.state.take_notifications().is_empty();
        if !(rung || notified) {
            return;
        }
        if self.options.bell.attends() {
            if let Some(surfaces) = &self.surfaces {
                surfaces
                    .window
                    .request_user_attention(Some(UserAttentionType::Informational));
            }
        }
        if rung && self.options.bell.rings() {
            (self.ring)();
        }
    }

    pub(in crate::window) fn follow(
        &mut self,
        switch_to: Option<ConnectToSession>,
        showing: Option<u64>,
    ) -> Followed {
        self.stop_momentum();
        let Some(session) = self.session.as_mut() else {
            return Followed::Ended;
        };
        session.collect();
        let Some(target) = switch_to else {
            return Followed::Ended;
        };
        if let Some(window) = window_to_reuse(
            self.standalone,
            self.options.reuse_session_windows,
            self.key,
            showing,
        ) {
            session.detacher.abandon();
            self.sender = None;
            return Followed::HandedOver(handover_to(window, target));
        }
        let geometry = self.geometry.get();
        match session.reconnect(&target, geometry) {
            Ok(connection) => {
                let session_name = connection.session_name.clone();
                self.remote_session = connection.remote;
                self.sender = Some(connection.sender.clone());
                self.role = connection.role;
                self.geometry = connection.geometry.clone();
                session
                    .detacher
                    .follow(connection.sender.clone(), connection.role);
                session.loop_options.settings = self.settings.clone();
                session.spawn(connection);
                self.start_over(geometry);
                self.set_title(window_title(&session_name));
                self.session_name = session_name;
                for msg in palette::seed_messages(&self.options.paints) {
                    if let Err(e) = self.tell(msg) {
                        report!("failed to re-declare a color: {}", e);
                    }
                }
                if !self.focused {
                    self.report_focus();
                }
                Followed::Kept
            },
            Err(e) => {
                session.detacher.abandon();
                self.stranded(e);
                Followed::Kept
            },
        }
    }

    pub(crate) fn on_switch(&mut self, to: ConnectToSession, showing: Option<u64>) -> Switched {
        match self.follow(Some(to), showing) {
            Followed::Kept => Switched::Kept,
            Followed::Ended => Switched::Closed,
            Followed::HandedOver(handover) => Switched::HandedOver(handover),
        }
    }

    pub(crate) fn set_standalone(&mut self, standalone: bool) {
        self.standalone = standalone;
    }

    pub(crate) fn set_remote_session(&mut self, remote: bool) {
        self.remote_session = remote;
        self.state.set_shared_media(!remote);
    }

    pub(crate) fn adopt_history(&self, history: zellij_utils::session_history::SessionHistory) {
        if let Err(e) = self.tell(ClientToServerMsg::AdoptSessionHistory { history }) {
            report!(
                "the window could not hand its session the history it took over: {}",
                e
            );
        }
    }

    pub(in crate::window) fn start_over(&mut self, geometry: Geometry) {
        self.cancel_scroll_animations();
        self.state = TerminalState::new(geometry.rows, geometry.cols);
        self.state
            .set_cell_size(self.metrics.width, self.metrics.height);
        self.state.set_shared_media(!self.remote_session);
        self.ime.composition.cancel();
        self.ime.composed_row = None;
        self.mouse.selecting = false;
        self.set_selection(None);
        self.retained.mark_everything();
        self.schedule_draw();
    }

    pub(in crate::window) fn session_ended(&mut self, ending: Ending) -> bool {
        if let Ending::Switched(to) = ending {
            return self.follow(Some(to), None) == Followed::Ended;
        }
        self.stop_momentum();
        if let Some(session) = self.session.as_mut() {
            session.collect();
        }
        if self.failure.is_some() {
            return false;
        }
        match crate::window::notice::trouble(&ending, &self.session_name) {
            Some(trouble) if !self.leaving => {
                self.stranded(anyhow!("{}", crate::window::notice::ended(&trouble)));
                false
            },
            _ => {
                self.remember();
                true
            },
        }
    }

    pub(in crate::window) fn close_requested(&mut self) -> bool {
        self.remember();
        self.force_closing()
    }

    pub(in crate::window) fn stranded(&mut self, failure: anyhow::Error) {
        let message = format!("{:#}", failure);
        report!("{}", message);
        self.sender = None;
        self.show_notice(&message);
        self.failure = Some(failure);
    }
}

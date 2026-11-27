use crate::app::AppEvent;
use crate::session::connection::Connection;
use crate::window::conductor::Input;
use crate::window::desktop::Pending;
use crate::window::desktop::World;
use crate::window::Desktop;
use crate::window::DesktopMode;
use crate::window::{terminal_app, with_activation, Switched, Verdict, Wake};
use anyhow::Result;
use winit::event_loop::ActiveEventLoop;
use zellij_utils::app_control::OpenWindowRequest;
use zellij_utils::data::{ConnectToSession, PaneId};
use zellij_utils::input::actions::Action;

impl Desktop {
    pub(super) fn drop_window(&mut self, key: u64) {
        let Some(mut app) = self.windows.remove(&key) else {
            return;
        };
        let failure = app.take_failure();
        let session = app.take_session();
        drop(app);
        match &mut self.mode {
            DesktopMode::Single => {
                if let Some(failure) = failure {
                    self.failure = Some(failure);
                }
                if let Some(session) = session {
                    self.sessions.push(session);
                }
            },
            DesktopMode::Background(_) => {
                if let Some(failure) = failure {
                    report!("a window closed after a failure: {:#}", failure);
                }
                drop(session);
            },
        }
    }

    pub(super) fn close_window(&mut self, event_loop: &ActiveEventLoop, key: u64, by_user: bool) {
        self.drop_window(key);
        self.run(event_loop, Input::WindowClosed { key, by_user });
    }

    pub(super) fn raise(
        &mut self,
        key: u64,
        pane: Option<PaneId>,
        tab: Option<usize>,
        history: Option<zellij_utils::session_history::SessionHistory>,
        token: Option<String>,
        requester: Option<u64>,
    ) {
        let Some(app) = self.windows.get(&key) else {
            return;
        };
        if let Some(history) = history {
            app.adopt_history(history);
        }
        if let Some(window) = app.window() {
            let requester = requester
                .filter(|requester| *requester != key)
                .and_then(|requester| self.windows.get(&requester))
                .and_then(|requester| requester.window());
            self.activation.bring_forward(window, token, requester);
        }
        crate::window::activate_app();
        if let Some(pane_id) = pane {
            app.send_action(Action::FocusPaneByPaneId { pane_id });
        } else if let Some(position) = tab {
            app.send_action(Action::GoToTab {
                index: position as u32,
            });
        }
    }

    pub(super) fn switched(
        &mut self,
        event_loop: &ActiveEventLoop,
        key: u64,
        to: ConnectToSession,
    ) {
        let showing = {
            let background = match &self.mode {
                DesktopMode::Background(hosting) => Some(&hosting.background),
                DesktopMode::Single => None,
            };
            let world = World {
                windows: &self.windows,
                background,
            };
            self.conductor
                .window_showing(to.name.as_deref(), key, &world)
        };
        let switched = match self.windows.get_mut(&key) {
            Some(app) => app.on_switch(to, showing),
            None => return,
        };
        match switched {
            Switched::Kept => {},
            Switched::Closed => self.close_window(event_loop, key, false),
            Switched::HandedOver(handover) => {
                self.run(
                    event_loop,
                    Input::HandedOver {
                        handover,
                        from: key,
                    },
                );
                self.close_window(event_loop, key, false);
            },
        }
    }

    pub(super) fn open(
        &mut self,
        ticket: u64,
        request: OpenWindowRequest,
        token: Option<String>,
    ) -> bool {
        let proxy = self.proxy.clone();
        let Some(hosting) = self.hosting() else {
            return false;
        };
        let prepared = match hosting.maker.prepare(&request) {
            Ok(prepared) => prepared,
            Err(e) => {
                report!("a window could not be prepared: {:#}", e);
                hosting
                    .background
                    .say("Zellij", &format!("A window could not be opened: {:#}", e));
                return false;
            },
        };
        let connector = hosting.maker.connector();
        let geometry = prepared.geometry;
        let routed = request
            .session
            .clone()
            .filter(|_| !request.create && request.command.is_empty());
        self.pending.insert(
            ticket,
            Pending {
                prepared,
                anchor: None,
                token,
                position: None,
            },
        );
        let spawned = std::thread::Builder::new()
            .name("app_connect".to_owned())
            .spawn(move || {
                if let Some(session) = routed {
                    let standalone = crate::app::control::standalone::windows_dir();
                    if crate::app::control::standalone::raise_in(
                        &standalone,
                        &session,
                        request.pane_id,
                    ) {
                        let _ = proxy.send_event(Wake::App(AppEvent::Handled { ticket }));
                        return;
                    }
                }
                let result = connector(&request, geometry).map_err(|e| format!("{:#}", e));
                let _ = proxy.send_event(Wake::App(AppEvent::Connected { ticket, result }));
            });
        if spawned.is_err() {
            self.pending.remove(&ticket);
            return false;
        }
        true
    }

    pub(super) fn connected(
        &mut self,
        event_loop: &ActiveEventLoop,
        ticket: u64,
        result: Result<Connection, String>,
    ) {
        let error = match result {
            Ok(connection) => {
                self.arrived.insert(ticket, connection);
                None
            },
            Err(text) => {
                report!(
                    "a window or the panel could not reach its session: {}",
                    text
                );
                self.pending.remove(&ticket);
                Some(text)
            },
        };
        self.run(event_loop, Input::Connected { ticket, error });
        self.discard(ticket);
    }

    pub(super) fn discard(&mut self, ticket: u64) {
        self.pending.remove(&ticket);
        if let Some(connection) = self.arrived.remove(&ticket) {
            if let Err(e) =
                crate::session::connection::request_detach(&connection.sender, connection.role)
            {
                report!("a connection nobody waits for could not be let go: {}", e);
            }
        }
    }

    pub(super) fn show_window(&mut self, event_loop: &ActiveEventLoop, ticket: u64) -> bool {
        let proxy = self.proxy.clone();
        let (Some(pending), Some(connection)) =
            (self.pending.remove(&ticket), self.arrived.remove(&ticket))
        else {
            return false;
        };
        let Some(hosting) = self.hosting() else {
            return false;
        };
        let made = hosting.maker.make(connection, pending.prepared);
        let mut app = with_activation(terminal_app(made, ticket, proxy), pending.token);
        app.set_standalone(false);
        let verdict = app.on_resumed(event_loop);
        self.windows.insert(ticket, app);
        if verdict == Verdict::Close {
            self.drop_window(ticket);
            return false;
        }
        if let Some(window) = self.windows.get(&ticket).and_then(|app| app.window()) {
            crate::window::bring_opened_window_forward(window);
        }
        true
    }
}

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use zellij_utils::app_control::OpenWindowRequest;
use zellij_utils::input::actions::Action;

use super::conductor::{Conductor, Effect, Facts, Input};
use super::{App, Session, TerminalWindow, Wake};
use crate::app::desktop::tray::Anchor;
use crate::app::{AppEvent, Background, Intent};
use crate::config::options::Options;
use crate::config::window_state::Startup;
use crate::session::connection::{Connection, Geometry};
use crate::text::font::FontStack;

mod events;
pub(in crate::window) mod grips;
mod lifecycle;
mod panel;

use grips::panel_grip;
pub(crate) use grips::{bring_forward, hover, start_grip, Hover, WindowOps};
use panel::send_panel_view;

pub(crate) struct PreparedWindow {
    pub fonts: FontStack,
    pub startup: Startup,
    pub geometry: Geometry,
    pub options: Options,
    pub settings: crate::config::settings::Settings,
    pub border: Option<crate::emulation::color::Srgb>,
}

pub(crate) type Connector =
    Arc<dyn Fn(&OpenWindowRequest, Geometry) -> Result<Connection> + Send + Sync>;

pub(crate) type PanelConnector =
    Arc<dyn Fn(Geometry, &'static str) -> Result<Connection> + Send + Sync>;

pub(crate) trait WindowMaker {
    fn prepare(&self, request: &OpenWindowRequest) -> Result<PreparedWindow>;
    fn connector(&self) -> Connector;
    fn make(&self, connection: Connection, prepared: PreparedWindow) -> TerminalWindow;
    fn panel(&self, area: (f64, f64)) -> Result<PreparedWindow>;
    fn panel_connector(&self) -> PanelConnector;
}

pub(crate) struct Hosting {
    pub background: Background,
    pub maker: Box<dyn WindowMaker>,
    pub first: Option<Intent>,
}

pub(crate) enum DesktopMode {
    Single,
    Background(Box<Hosting>),
}

struct PanelWindow {
    app: App,
    pointer: Option<winit::dpi::PhysicalPosition<f64>>,
    on_edge: bool,
}

/// Where the panel was and how big it was when it last closed, so it opens there again.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PanelFrame {
    cols: usize,
    rows: usize,
    position: Option<(f64, f64)>,
}

struct Pending {
    prepared: PreparedWindow,
    anchor: Option<Anchor>,
    token: Option<String>,
    position: Option<(f64, f64)>,
}

struct World<'a> {
    windows: &'a BTreeMap<u64, App>,
    background: Option<&'a Background>,
}

impl Facts for World<'_> {
    fn session_of(&self, key: u64) -> Option<String> {
        self.windows
            .get(&key)
            .and_then(|app| app.session_name().map(str::to_owned))
    }
    fn has_session(&self, name: &str) -> bool {
        self.background
            .map(|background| background.snapshot.global.has_session(name))
            .unwrap_or(false)
    }
    fn live_sessions(&self) -> Vec<String> {
        self.background
            .map(|background| background.live_sessions())
            .unwrap_or_default()
    }
}

pub(crate) struct Desktop {
    windows: BTreeMap<u64, App>,
    conductor: Conductor,
    proxy: EventLoopProxy<Wake>,
    failure: Option<anyhow::Error>,
    mode: DesktopMode,
    sessions: Vec<Session>,
    pending: HashMap<u64, Pending>,
    arrived: HashMap<u64, Connection>,
    panel: Option<PanelWindow>,
    panel_frame: Option<PanelFrame>,
    activation: super::Activation,
}

impl Desktop {
    pub(crate) fn new(proxy: EventLoopProxy<Wake>, mode: DesktopMode) -> Self {
        let hosted = matches!(mode, DesktopMode::Background(_));
        Desktop {
            windows: BTreeMap::new(),
            conductor: Conductor::new(hosted),
            proxy,
            failure: None,
            mode,
            sessions: Vec::new(),
            pending: HashMap::new(),
            arrived: HashMap::new(),
            panel: None,
            panel_frame: None,
            activation: super::Activation::new(),
        }
    }

    pub(crate) fn adopt(&mut self, app: App) {
        let key = app.key();
        self.conductor.adopt(key);
        self.windows.insert(key, app);
    }

    pub(crate) fn take_failure(&mut self) -> Option<anyhow::Error> {
        self.failure.take()
    }

    pub(crate) fn into_sessions(mut self) -> Vec<Session> {
        let keys: Vec<u64> = self.windows.keys().copied().collect();
        for key in keys {
            if let Some(mut app) = self.windows.remove(&key) {
                if let Some(session) = app.take_session() {
                    self.sessions.push(session);
                }
            }
        }
        std::mem::take(&mut self.sessions)
    }

    fn hosting(&mut self) -> Option<&mut Hosting> {
        match &mut self.mode {
            DesktopMode::Background(hosting) => Some(hosting),
            DesktopMode::Single => None,
        }
    }

    fn run(&mut self, event_loop: &ActiveEventLoop, input: Input) {
        let mut queue = VecDeque::from([input]);
        while let Some(input) = queue.pop_front() {
            let background = match &self.mode {
                DesktopMode::Background(hosting) => Some(&hosting.background),
                DesktopMode::Single => None,
            };
            let world = World {
                windows: &self.windows,
                background,
            };
            let effects = self.conductor.handle(input, Instant::now(), &world);
            for effect in effects {
                self.carry_out(event_loop, effect, &mut queue);
            }
        }
    }

    fn carry_out(
        &mut self,
        event_loop: &ActiveEventLoop,
        effect: Effect,
        queue: &mut VecDeque<Input>,
    ) {
        match effect {
            Effect::OpenWindow {
                ticket,
                request,
                token,
            } => {
                if !self.open(ticket, request, token) {
                    queue.push_back(Input::OpenAbandoned { ticket });
                }
            },
            Effect::OpenPanel {
                ticket,
                view,
                at,
                token,
            } => {
                if !self.open_panel(event_loop, ticket, view, at, token) {
                    queue.push_back(Input::OpenAbandoned { ticket });
                }
            },
            Effect::ShowWindow { ticket } => {
                if !self.show_window(event_loop, ticket) {
                    queue.push_back(Input::WindowClosed {
                        key: ticket,
                        by_user: false,
                    });
                }
            },
            Effect::ShowPanel { ticket, view } => {
                if !self.show_panel(event_loop, ticket, view) {
                    queue.push_back(Input::PanelGone);
                }
            },
            Effect::Discard { ticket } => self.discard(ticket),
            Effect::ClosePanel => self.close_panel(),
            Effect::ReleasePanel => {
                if let Some(mut panel) = self.take_panel() {
                    if let Some(mut session) = panel.app.take_session() {
                        session.collect();
                    }
                }
            },
            Effect::SwitchPanelView { view } => {
                if let Some(panel) = self.panel.as_ref() {
                    send_panel_view(&panel.app, Some(view));
                    if let Some(window) = panel.app.window() {
                        window.focus_window();
                    }
                }
            },
            Effect::BringPanelForward { token } => self.bring_panel_forward(token),
            Effect::RaiseWindow {
                key,
                pane,
                tab,
                history,
                token,
                requester,
            } => self.raise(key, pane, tab, history, token, requester),
            Effect::SendAction { key, action } => {
                if let Some(app) = self.windows.get(&key) {
                    app.send_action(action);
                }
            },
            Effect::Notify { text } => {
                if let Some(hosting) = self.hosting() {
                    hosting.background.say("Zellij", &text);
                }
            },
            Effect::CloseNotice => {
                if let Some(hosting) = self.hosting() {
                    hosting.background.window_closed_by_user();
                }
            },
            Effect::SetAccessory(accessory) => {
                super::set_accessory(accessory);
            },
            Effect::Quit { stop } => {
                for app in self.windows.values_mut() {
                    app.leave();
                }
                crate::app::stop_all_sessions(stop);
                event_loop.exit();
            },
        }
    }

    fn ensure_activator(&mut self, event_loop: &ActiveEventLoop) {
        let windows = &self.windows;
        let panel = &self.panel;
        self.activation.ensure(event_loop, || {
            windows
                .values()
                .find_map(|app| app.window())
                .or_else(|| panel.as_ref().and_then(|panel| panel.app.window()))
        });
    }

    fn popup_token(&mut self) -> Option<String> {
        let window = self.panel.as_ref().and_then(|panel| panel.app.window());
        self.activation.token(window)
    }

    fn app_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        let now = Instant::now();
        let event = match event {
            AppEvent::Connected { ticket, result } => {
                self.connected(event_loop, ticket, result);
                return;
            },
            AppEvent::Terminate => {
                self.run(
                    event_loop,
                    Input::QuitRequested {
                        stop_sessions: false,
                    },
                );
                return;
            },
            AppEvent::Snapshot(snapshot) => {
                if let Some(hosting) = self.hosting() {
                    hosting.background.on_snapshot(*snapshot, now);
                }
                return;
            },
            AppEvent::Describe(answer) => {
                let _ = answer.send(self.describe());
                return;
            },
            AppEvent::RaiseWindows { pane } => {
                let keys: Vec<u64> = self.windows.keys().copied().collect();
                for key in keys {
                    self.raise(key, pane, None, None, None, None);
                }
                return;
            },
            AppEvent::DetachWindows => {
                for app in self.windows.values() {
                    app.send_action(Action::Detach);
                }
                return;
            },
            AppEvent::Handled { ticket } => {
                self.run(event_loop, Input::OpenAbandoned { ticket });
                self.discard(ticket);
                return;
            },
            other => other,
        };
        let Some(hosting) = self.hosting() else {
            return;
        };
        let intents = match event {
            AppEvent::Request(request) => hosting.background.on_request(request),
            AppEvent::Tray(event) => hosting.background.on_tray(event, now),
            AppEvent::Notification(event) => hosting.background.on_notification(event),
            AppEvent::Config(config) => {
                hosting.background.on_config(*config, now);
                vec![]
            },
            AppEvent::Connected { .. }
            | AppEvent::Terminate
            | AppEvent::Snapshot(_)
            | AppEvent::Describe(_)
            | AppEvent::RaiseWindows { .. }
            | AppEvent::DetachWindows
            | AppEvent::Handled { .. } => vec![],
        };
        if !intents.is_empty() {
            self.run(event_loop, Input::Intents(intents));
        }
    }

    fn describe(&self) -> zellij_utils::app_control::AppState {
        let (live_sessions, tray_icon) = match &self.mode {
            DesktopMode::Background(hosting) => (
                hosting.background.live_sessions(),
                hosting.background.tray_visible(),
            ),
            DesktopMode::Single => (vec![], false),
        };
        zellij_utils::app_control::AppState {
            windows: self
                .windows
                .values()
                .map(|app| zellij_utils::app_control::WindowState {
                    session: app.session_name().map(str::to_owned),
                })
                .collect(),
            panel: self.conductor.panel_state(),
            live_sessions,
            tray_icon,
            focused_session: self
                .conductor
                .most_recent_window()
                .and_then(|key| self.windows.get(&key))
                .and_then(|app| app.session_name().map(str::to_owned)),
        }
    }

    fn start(&mut self, event_loop: &ActiveEventLoop) {
        if self.conductor.started() {
            return;
        }
        let first = match self.hosting() {
            Some(hosting) => {
                hosting.background.start_on_main_thread();
                let first = hosting.first.take();
                first.map(|intent| hosting.background.adjust_intent(intent))
            },
            None => None,
        };
        self.run(event_loop, Input::Started { first });
    }
}

#[cfg(test)]
mod grip_tests;

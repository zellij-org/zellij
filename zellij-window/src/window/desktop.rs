use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event::{ElementState, MouseButton};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::window::{CursorIcon, ResizeDirection, WindowId};
use zellij_utils::app_control::{OpenWindowRequest, PopupView};
use zellij_utils::data::{ConnectToSession, PaneId};
use zellij_utils::input::actions::Action;

use super::conductor::{Conductor, Effect, Facts, Input};
use super::{
    as_panel, terminal_app, with_activation, App, Session, Switched, TerminalWindow, Verdict, Wake,
};
use crate::app::tray::Anchor;
use crate::app::{AppEvent, Background, Intent};
use crate::client_loop::Ending;
use crate::connection::{Connection, Geometry};
use crate::font::FontStack;
use crate::options::Options;
use crate::window_state::Startup;

pub(crate) struct PreparedWindow {
    pub fonts: FontStack,
    pub startup: Startup,
    pub geometry: Geometry,
    pub options: Options,
    pub settings: crate::settings::Settings,
    pub border: Option<crate::color::Srgb>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Grip {
    Move,
    Resize(ResizeDirection),
}

pub(crate) fn grip_at(
    position: (f64, f64),
    size: (u32, u32),
    margins: crate::scene::Margins,
) -> Option<Grip> {
    let (x, y) = position;
    let (width, height) = (size.0 as f64, size.1 as f64);
    let reach = |margin: u32| (margin as f64).max(4.0);
    let left = x < reach(margins.left);
    let right = x >= width - reach(margins.right);
    let bottom = y >= height - reach(margins.bottom);
    let top_edge = y < reach(margins.left.min(margins.right));
    let top_strip = y < reach(margins.top);
    let corner = reach(margins.left.max(margins.right)) * 2.0;
    use ResizeDirection::*;
    let direction = match (
        top_edge,
        bottom,
        left || x < corner && bottom,
        right || x >= width - corner && bottom,
    ) {
        (true, _, true, _) => Some(NorthWest),
        (true, _, _, true) => Some(NorthEast),
        (_, true, true, _) => Some(SouthWest),
        (_, true, _, true) => Some(SouthEast),
        (true, _, _, _) => Some(North),
        (_, true, _, _) => Some(South),
        (_, _, true, _) if left => Some(West),
        (_, _, _, true) if right => Some(East),
        _ => None,
    };
    match direction {
        Some(direction) => Some(Grip::Resize(direction)),
        None if top_strip => Some(Grip::Move),
        None => None,
    }
}

fn grip_cursor(grip: Grip) -> CursorIcon {
    use ResizeDirection::*;
    match grip {
        Grip::Move => CursorIcon::Move,
        Grip::Resize(North) => CursorIcon::NResize,
        Grip::Resize(South) => CursorIcon::SResize,
        Grip::Resize(East) => CursorIcon::EResize,
        Grip::Resize(West) => CursorIcon::WResize,
        Grip::Resize(NorthEast) => CursorIcon::NeResize,
        Grip::Resize(NorthWest) => CursorIcon::NwResize,
        Grip::Resize(SouthEast) => CursorIcon::SeResize,
        Grip::Resize(SouthWest) => CursorIcon::SwResize,
    }
}

pub(crate) trait WindowOps {
    fn set_minimized(&self, minimized: bool);
    fn set_visible(&self, visible: bool);
    fn focus_window(&self);
    fn set_cursor(&self, icon: CursorIcon);
    fn drag_window(&self) -> Result<(), String>;
    fn drag_resize_window(&self, direction: ResizeDirection) -> Result<(), String>;
}

impl WindowOps for winit::window::Window {
    fn set_minimized(&self, minimized: bool) {
        winit::window::Window::set_minimized(self, minimized)
    }
    fn set_visible(&self, visible: bool) {
        winit::window::Window::set_visible(self, visible)
    }
    fn focus_window(&self) {
        winit::window::Window::focus_window(self)
    }
    fn set_cursor(&self, icon: CursorIcon) {
        winit::window::Window::set_cursor(self, icon)
    }
    fn drag_window(&self) -> Result<(), String> {
        winit::window::Window::drag_window(self).map_err(|e| e.to_string())
    }
    fn drag_resize_window(&self, direction: ResizeDirection) -> Result<(), String> {
        winit::window::Window::drag_resize_window(self, direction).map_err(|e| e.to_string())
    }
}

pub(crate) fn bring_forward(
    window: &dyn WindowOps,
    activate: Option<&mut dyn FnMut(Option<String>)>,
    token: Option<String>,
) {
    window.set_minimized(false);
    window.set_visible(true);
    window.focus_window();
    if let Some(activate) = activate {
        activate(token);
    }
}

#[cfg_attr(
    not(all(unix, not(target_os = "macos"), not(target_os = "android"))),
    allow(dead_code)
)]
pub(crate) fn bring_panel_forward_with(
    activate: Option<&mut dyn FnMut(Option<String>)>,
    token: Option<String>,
) {
    if let (Some(activate), Some(token)) = (activate, token) {
        activate(Some(token));
    }
}

pub(crate) fn start_grip(window: &dyn WindowOps, grip: Grip) {
    let started = match grip {
        Grip::Move => window.drag_window(),
        Grip::Resize(direction) => window.drag_resize_window(direction),
    };
    if let Err(e) = started {
        report!("the panel could not be moved or resized: {}", e);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hover {
    Grip,
    BackToNormal,
    Inside,
}

pub(crate) fn hover(window: Option<&dyn WindowOps>, grip: Option<Grip>, on_edge: bool) -> Hover {
    match (grip, window) {
        (Some(grip), Some(window)) => {
            window.set_cursor(grip_cursor(grip));
            Hover::Grip
        },
        (None, _) if on_edge => Hover::BackToNormal,
        _ => Hover::Inside,
    }
}

pub(crate) fn grip_or_header(
    position: (f64, f64),
    size: (u32, u32),
    margins: crate::scene::Margins,
    header_is_blank: impl FnOnce() -> bool,
) -> Option<Grip> {
    grip_at(position, size, margins).or_else(|| header_is_blank().then_some(Grip::Move))
}

struct Pending {
    prepared: PreparedWindow,
    anchor: Option<Anchor>,
    token: Option<String>,
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
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    activator: Option<super::wayland_activation::Activator>,
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    activator_tried: bool,
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
            #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
            activator: None,
            #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
            activator_tried: false,
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
                if let Some(mut panel) = self.panel.take() {
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
                #[cfg(target_os = "macos")]
                crate::app::tray::set_accessory(accessory);
                #[cfg(not(target_os = "macos"))]
                let _ = accessory;
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

    fn drop_window(&mut self, key: u64) {
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

    fn close_window(&mut self, event_loop: &ActiveEventLoop, key: u64, by_user: bool) {
        self.drop_window(key);
        self.run(event_loop, Input::WindowClosed { key, by_user });
    }

    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    fn ensure_activator(&mut self, event_loop: &ActiveEventLoop) {
        if self.activator.is_some() || self.activator_tried || !super::is_wayland(event_loop) {
            return;
        }
        let made = self
            .windows
            .values()
            .find_map(|app| app.window())
            .or_else(|| self.panel.as_ref().and_then(|panel| panel.app.window()))
            .map(super::wayland_activation::Activator::for_window);
        match made {
            Some(Ok(activator)) => {
                self.activator_tried = true;
                self.activator = Some(activator);
            },
            Some(Err(e)) => {
                self.activator_tried = true;
                report!(
                    "windows cannot be brought forward on this compositor: {:#}",
                    e
                );
            },
            None => {},
        }
    }

    #[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "android"))))]
    fn ensure_activator(&mut self, _event_loop: &ActiveEventLoop) {}

    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    fn popup_token(&mut self) -> Option<String> {
        let window = self.panel.as_ref().and_then(|panel| panel.app.window())?;
        self.activator.as_mut()?.token(window)
    }

    #[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "android"))))]
    fn popup_token(&mut self) -> Option<String> {
        None
    }

    fn raise(
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
        #[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "android"))))]
        let _ = requester;
        if let Some(window) = app.window() {
            #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
            {
                let requester = requester
                    .filter(|requester| *requester != key)
                    .and_then(|requester| self.windows.get(&requester))
                    .and_then(|requester| requester.window());
                let mut activate = self
                    .activator
                    .as_mut()
                    .map(|activator| move |token| activator.activate(window, token, requester));
                bring_forward(
                    window,
                    activate
                        .as_mut()
                        .map(|f| f as &mut dyn FnMut(Option<String>)),
                    token,
                );
            }
            #[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "android"))))]
            bring_forward(window, None, token);
        }
        #[cfg(target_os = "macos")]
        crate::app::tray::activate();
        if let Some(pane_id) = pane {
            app.send_action(Action::FocusPaneByPaneId { pane_id });
        } else if let Some(position) = tab {
            app.send_action(Action::GoToTab {
                index: position as u32,
            });
        }
    }

    fn switched(&mut self, event_loop: &ActiveEventLoop, key: u64, to: ConnectToSession) {
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

    fn open(&mut self, ticket: u64, request: OpenWindowRequest, token: Option<String>) -> bool {
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
            },
        );
        let spawned = std::thread::Builder::new()
            .name("app_connect".to_owned())
            .spawn(move || {
                if let Some(session) = routed {
                    let standalone = crate::app::standalone::windows_dir();
                    if crate::app::standalone::raise_in(&standalone, &session, request.pane_id) {
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

    fn connected(
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

    fn discard(&mut self, ticket: u64) {
        self.pending.remove(&ticket);
        if let Some(connection) = self.arrived.remove(&ticket) {
            if let Err(e) = crate::connection::request_detach(&connection.sender, connection.role) {
                report!("a connection nobody waits for could not be let go: {}", e);
            }
        }
    }

    fn show_window(&mut self, event_loop: &ActiveEventLoop, ticket: u64) -> bool {
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
        true
    }

    fn close_panel(&mut self) {
        if let Some(mut panel) = self.panel.take() {
            panel.app.leave();
            drop(panel.app.take_session());
        }
    }

    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    fn bring_panel_forward(&mut self, token: Option<String>) {
        let window = self.panel.as_ref().and_then(|panel| panel.app.window());
        if let (Some(window), Some(activator)) = (window, self.activator.as_mut()) {
            let mut activate = move |token| activator.activate(window, token, None);
            bring_panel_forward_with(Some(&mut activate), token);
        }
    }

    #[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "android"))))]
    fn bring_panel_forward(&mut self, _token: Option<String>) {
        #[cfg(target_os = "macos")]
        crate::app::tray::activate();
    }

    fn panel_area(&self, event_loop: &ActiveEventLoop, anchor: Option<Anchor>) -> (f64, f64) {
        let (x, y) = match anchor {
            Some(Anchor::Below { x, y } | Anchor::Above { x, y } | Anchor::At { x, y }) => (x, y),
            None => (0.0, 0.0),
        };
        let monitor = event_loop
            .available_monitors()
            .find(|m| {
                let (p, s) = (m.position(), m.size());
                x >= p.x as f64
                    && y >= p.y as f64
                    && x < p.x as f64 + s.width as f64
                    && y < p.y as f64 + s.height as f64
            })
            .or_else(|| event_loop.primary_monitor())
            .or_else(|| event_loop.available_monitors().next());
        match monitor {
            Some(monitor) => {
                let size = monitor.size().to_logical::<f64>(monitor.scale_factor());
                (size.width, size.height)
            },
            None => (1280.0, 800.0),
        }
    }

    fn open_panel(
        &mut self,
        event_loop: &ActiveEventLoop,
        ticket: u64,
        view: Option<PopupView>,
        at: Option<(i32, i32)>,
        token: Option<String>,
    ) -> bool {
        let anchor = match at {
            Some((x, y)) => Some(Anchor::At {
                x: x as f64,
                y: y as f64,
            }),
            None => self.hosting().and_then(|h| h.background.tray_anchor()),
        };
        let area = self.panel_area(event_loop, anchor);
        let proxy = self.proxy.clone();
        let Some(hosting) = self.hosting() else {
            return false;
        };
        let prepared = match hosting.maker.panel(area) {
            Ok(prepared) => prepared,
            Err(e) => {
                report!("the inbox panel could not be prepared: {:#}", e);
                return false;
            },
        };
        let connector = hosting.maker.panel_connector();
        let geometry = prepared.geometry;
        let view_name = view_payload(view);
        self.pending.insert(
            ticket,
            Pending {
                prepared,
                anchor,
                token,
            },
        );
        let spawned = std::thread::Builder::new()
            .name("app_panel_connect".to_owned())
            .spawn(move || {
                let result = connector(geometry, view_name).map_err(|e| format!("{:#}", e));
                let _ = proxy.send_event(Wake::App(AppEvent::Connected { ticket, result }));
            });
        if spawned.is_err() {
            self.pending.remove(&ticket);
            return false;
        }
        true
    }

    fn show_panel(
        &mut self,
        event_loop: &ActiveEventLoop,
        ticket: u64,
        view: Option<PopupView>,
    ) -> bool {
        let proxy = self.proxy.clone();
        let (Some(pending), Some(connection)) =
            (self.pending.remove(&ticket), self.arrived.remove(&ticket))
        else {
            return false;
        };
        let Some(hosting) = self.hosting() else {
            return false;
        };
        let Pending {
            prepared,
            anchor,
            token,
        } = pending;
        let (cell_width, cell_height) =
            (prepared.geometry.cell_width, prepared.geometry.cell_height);
        let size = (
            (cell_width * prepared.geometry.cols) as u32,
            (cell_height * prepared.geometry.rows) as u32,
        );
        let border = prepared.border;
        let mut made = hosting.maker.make(connection, prepared);
        made.remember = false;
        let position = placement(event_loop, anchor, size);
        let app = with_activation(terminal_app(made, ticket, proxy), token);
        let mut app = as_panel(app, position, border);
        if app.on_resumed(event_loop) == Verdict::Close {
            drop(app.take_session());
            return false;
        }
        if let Some(window) = app.window() {
            window.focus_window();
        }
        #[cfg(target_os = "macos")]
        crate::app::tray::activate();
        send_panel_view(&app, view);
        self.panel = Some(PanelWindow {
            app,
            pointer: None,
            on_edge: false,
        });
        self.ensure_activator(event_loop);
        true
    }

    fn panel_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        let Some(panel) = self.panel.as_mut() else {
            return;
        };
        match event {
            WindowEvent::Focused(true) => {
                panel.app.on_window_event(WindowEvent::Focused(true));
                self.run(event_loop, Input::PanelFocused);
            },
            WindowEvent::Focused(false) => self.run(event_loop, Input::PanelFocusLost),
            WindowEvent::CloseRequested => self.run(event_loop, Input::PanelClosedByUser),
            moved @ WindowEvent::CursorMoved { position, .. } => {
                panel.pointer = Some(position);
                let grip = panel_grip(&panel.app, position);
                if grip.is_none() {
                    panel.app.on_window_event(moved);
                }
                let window = panel.app.window().map(|w| w as &dyn WindowOps);
                match hover(window, grip, panel.on_edge) {
                    Hover::Grip => panel.on_edge = true,
                    Hover::BackToNormal => {
                        panel.on_edge = false;
                        panel.app.pointer_back_to_normal();
                    },
                    Hover::Inside => {},
                }
            },
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if panel
                .pointer
                .and_then(|position| panel_grip(&panel.app, position))
                .is_some() =>
            {
                let grip = panel
                    .pointer
                    .and_then(|position| panel_grip(&panel.app, position));
                if let (Some(grip), Some(window)) = (grip, panel.app.window()) {
                    start_grip(window, grip);
                }
            },
            other => {
                if panel.app.on_window_event(other) == Verdict::Close {
                    self.run(event_loop, Input::PanelClosedByUser);
                }
            },
        }
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
                hosting.first.take()
            },
            None => None,
        };
        self.run(event_loop, Input::Started { first });
    }
}

fn panel_grip(app: &App, position: winit::dpi::PhysicalPosition<f64>) -> Option<Grip> {
    let size = app.window()?.inner_size();
    grip_or_header(
        (position.x, position.y),
        (size.width, size.height),
        app.edge_margins(),
        || app.header_is_blank_at(position),
    )
}

fn send_panel_view(app: &App, view: Option<PopupView>) {
    app.send_action(Action::KeybindPipe {
        name: Some(crate::app::PANEL_VIEW_PIPE.to_owned()),
        payload: Some(view_payload(view).to_owned()),
        args: None,
        plugin: None,
        plugin_id: None,
        configuration: None,
        launch_new: false,
        skip_cache: false,
        floating: None,
        in_place: None,
        cwd: None,
        pane_title: None,
    });
}

fn view_payload(view: Option<PopupView>) -> &'static str {
    match view {
        Some(PopupView::Inbox) => "inbox",
        Some(PopupView::Sessions) => "sessions",
        Some(PopupView::Quit) => "quit",
        None => "auto",
    }
}

fn placement(
    event_loop: &ActiveEventLoop,
    anchor: Option<Anchor>,
    size: (u32, u32),
) -> Option<winit::dpi::Position> {
    if super::is_wayland(event_loop) {
        return None;
    }
    let (x, y) = match anchor {
        Some(Anchor::Below { x, y }) => {
            let scale = event_loop
                .primary_monitor()
                .map(|m| m.scale_factor())
                .unwrap_or(1.0);
            (x * scale, y * scale)
        },
        Some(Anchor::Above { x, y } | Anchor::At { x, y }) => (x, y),
        None => event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next())
            .map(|m| {
                let (p, s) = (m.position(), m.size());
                (
                    p.x as f64 + s.width as f64 / 2.0,
                    p.y as f64 + s.height as f64 / 2.0,
                )
            })?,
    };
    let (left, top, right, bottom) = work_area(event_loop, x, y);
    Some(
        winit::dpi::PhysicalPosition::new(
            centered(left, right, size.0),
            centered(top, bottom, size.1),
        )
        .into(),
    )
}

fn centered(start: f64, end: f64, length: u32) -> f64 {
    let room = (end - start).max(0.0);
    start + ((room - length as f64) / 2.0).max(0.0)
}

#[cfg(windows)]
fn work_area(event_loop: &ActiveEventLoop, x: f64, y: f64) -> (f64, f64, f64, f64) {
    crate::app::tray::work_area(x as i32, y as i32)
        .unwrap_or_else(|| monitor_area(event_loop, x, y))
}

#[cfg(not(windows))]
fn work_area(event_loop: &ActiveEventLoop, x: f64, y: f64) -> (f64, f64, f64, f64) {
    monitor_area(event_loop, x, y)
}

fn monitor_area(event_loop: &ActiveEventLoop, x: f64, y: f64) -> (f64, f64, f64, f64) {
    let monitors: Vec<_> = event_loop.available_monitors().collect();
    let area = |m: &winit::monitor::MonitorHandle| {
        let p = m.position();
        let s = m.size();
        (
            p.x as f64,
            p.y as f64,
            p.x as f64 + s.width as f64,
            p.y as f64 + s.height as f64,
        )
    };
    monitors
        .iter()
        .map(area)
        .find(|(l, t, r, b)| x >= *l && x < *r && y >= *t && y < *b)
        .or_else(|| event_loop.primary_monitor().as_ref().map(area))
        .unwrap_or((0.0, 0.0, f64::MAX / 4.0, f64::MAX / 4.0))
}

impl ApplicationHandler<Wake> for Desktop {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let keys: Vec<u64> = self.windows.keys().copied().collect();
        for key in keys {
            let verdict = match self.windows.get_mut(&key) {
                Some(app) => app.on_resumed(event_loop),
                None => continue,
            };
            if verdict == Verdict::Close {
                self.close_window(event_loop, key, false);
            } else {
                self.run(event_loop, Input::WindowFocused(key));
            }
        }
        self.start(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        match event {
            Wake::Window(key, wake) if self.panel.as_ref().map(|p| p.app.key()) == Some(key) => {
                match wake {
                    super::WindowWake::Finished(Ending::Switched(to)) => {
                        let token = self.popup_token();
                        self.run(event_loop, Input::PanelSwitched { to, token });
                    },
                    super::WindowWake::Finished(_) => self.run(event_loop, Input::PanelEnded),
                    wake => {
                        let closed = self
                            .panel
                            .as_mut()
                            .map(|panel| panel.app.on_wake(wake) == Verdict::Close)
                            .unwrap_or(false);
                        if closed {
                            self.run(event_loop, Input::PanelClosedByUser);
                        }
                    },
                }
            },
            Wake::Window(key, super::WindowWake::Finished(Ending::Switched(to))) => {
                self.switched(event_loop, key, to)
            },
            Wake::Window(key, wake) => {
                let verdict = match self.windows.get_mut(&key) {
                    Some(app) => app.on_wake(wake),
                    None => return,
                };
                if verdict == Verdict::Close {
                    self.close_window(event_loop, key, false);
                }
            },
            Wake::App(event) => self.app_event(event_loop, event),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self
            .panel
            .as_ref()
            .map(|panel| panel.app.window_id() == Some(id))
            .unwrap_or(false)
        {
            self.panel_event(event_loop, event);
            return;
        }
        let Some(key) = self
            .windows
            .iter()
            .find(|(_, app)| app.window_id() == Some(id))
            .map(|(key, _)| *key)
        else {
            return;
        };
        let by_user = matches!(event, WindowEvent::CloseRequested);
        if matches!(event, WindowEvent::Focused(true)) {
            self.run(event_loop, Input::WindowFocused(key));
        }
        if let WindowEvent::KeyboardInput {
            event: key_event,
            is_synthetic: false,
            ..
        } = &event
        {
            let wants = self
                .windows
                .get(&key)
                .map(|app| app.wants_new_window(key_event))
                .unwrap_or(false);
            if wants && matches!(self.mode, DesktopMode::Background(_)) {
                self.run(
                    event_loop,
                    Input::Intents(vec![Intent::Open {
                        request: OpenWindowRequest::default(),
                        token: None,
                    }]),
                );
                return;
            }
        }
        let verdict = match self.windows.get_mut(&key) {
            Some(app) => app.on_window_event(event),
            None => return,
        };
        if verdict == Verdict::Close {
            self.close_window(event_loop, key, by_user);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if matches!(self.mode, DesktopMode::Background(_)) {
            self.ensure_activator(event_loop);
            #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
            if let Some(activator) = self.activator.as_mut() {
                activator.pump();
            }
        }
        if let Some(hosting) = self.hosting() {
            if hosting
                .background
                .deadline()
                .map(|at| at <= now)
                .unwrap_or(false)
            {
                hosting.background.tick(now);
            }
        }
        let mut deadline: Option<Instant> = None;
        let mut earliest = |at: Option<Instant>| {
            if let Some(at) = at {
                deadline = Some(deadline.map_or(at, |d: Instant| d.min(at)));
            }
        };
        for app in self.windows.values_mut() {
            earliest(app.wake_deadline(now));
        }
        if let Some(panel) = self.panel.as_mut() {
            earliest(panel.app.wake_deadline(now));
        }
        if let DesktopMode::Background(hosting) = &self.mode {
            earliest(hosting.background.deadline());
        }
        if self.conductor.should_exit() {
            event_loop.exit();
            return;
        }
        event_loop.set_control_flow(match deadline {
            Some(at) => ControlFlow::WaitUntil(at.max(now + Duration::from_millis(1))),
            None => ControlFlow::Wait,
        });
    }
}

#[cfg(test)]
mod grip_tests {
    use super::*;
    use crate::scene::Margins;

    fn margins() -> Margins {
        Margins {
            left: 6,
            top: 12,
            right: 6,
            bottom: 6,
        }
    }

    #[test]
    fn edges_resize_the_top_strip_moves_and_the_inside_belongs_to_the_panel() {
        let size = (800, 600);
        let at = |x: f64, y: f64| grip_at((x, y), size, margins());
        assert_eq!(at(400.0, 300.0), None);
        assert_eq!(at(400.0, 9.0), Some(Grip::Move));
        assert_eq!(at(400.0, 2.0), Some(Grip::Resize(ResizeDirection::North)));
        assert_eq!(at(400.0, 597.0), Some(Grip::Resize(ResizeDirection::South)));
        assert_eq!(at(2.0, 300.0), Some(Grip::Resize(ResizeDirection::West)));
        assert_eq!(at(797.0, 300.0), Some(Grip::Resize(ResizeDirection::East)));
        assert_eq!(at(2.0, 2.0), Some(Grip::Resize(ResizeDirection::NorthWest)));
        assert_eq!(
            at(797.0, 2.0),
            Some(Grip::Resize(ResizeDirection::NorthEast))
        );
        assert_eq!(
            at(3.0, 598.0),
            Some(Grip::Resize(ResizeDirection::SouthWest))
        );
        assert_eq!(
            at(797.0, 598.0),
            Some(Grip::Resize(ResizeDirection::SouthEast))
        );
        assert_eq!(
            at(8.0, 597.0),
            Some(Grip::Resize(ResizeDirection::SouthWest))
        );
    }

    #[derive(Default)]
    struct FakeWindow {
        calls: std::cell::RefCell<Vec<String>>,
    }

    impl FakeWindow {
        fn log(&self, call: impl Into<String>) {
            self.calls.borrow_mut().push(call.into());
        }
        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl WindowOps for FakeWindow {
        fn set_minimized(&self, minimized: bool) {
            self.log(format!("minimized {}", minimized));
        }
        fn set_visible(&self, visible: bool) {
            self.log(format!("visible {}", visible));
        }
        fn focus_window(&self) {
            self.log("focus");
        }
        fn set_cursor(&self, icon: CursorIcon) {
            self.log(format!("cursor {:?}", icon));
        }
        fn drag_window(&self) -> Result<(), String> {
            self.log("drag");
            Ok(())
        }
        fn drag_resize_window(&self, direction: ResizeDirection) -> Result<(), String> {
            self.log(format!("resize {:?}", direction));
            Ok(())
        }
    }

    #[test]
    fn pressing_an_edge_resizes_the_panel_in_that_direction() {
        let window = FakeWindow::default();
        let size = (800, 600);
        for (position, direction) in [
            ((2.0, 300.0), ResizeDirection::West),
            ((797.0, 300.0), ResizeDirection::East),
            ((400.0, 597.0), ResizeDirection::South),
            ((797.0, 598.0), ResizeDirection::SouthEast),
        ] {
            let grip = grip_or_header(position, size, margins(), || false).unwrap();
            start_grip(&window, grip);
            assert_eq!(
                window.calls().last().cloned(),
                Some(format!("resize {:?}", direction))
            );
        }
    }

    #[test]
    fn a_blank_header_cell_moves_the_panel_and_a_busy_one_belongs_to_the_panel() {
        let window = FakeWindow::default();
        let size = (800, 600);
        assert_eq!(
            grip_or_header((400.0, 30.0), size, margins(), || true),
            Some(Grip::Move)
        );
        assert_eq!(
            grip_or_header((400.0, 30.0), size, margins(), || false),
            None
        );
        start_grip(&window, Grip::Move);
        assert_eq!(window.calls(), vec!["drag".to_owned()]);
    }

    #[test]
    fn hovering_a_grip_shows_its_cursor_and_leaving_it_restores_the_pointer() {
        let window = FakeWindow::default();
        assert_eq!(
            hover(
                Some(&window),
                Some(Grip::Resize(ResizeDirection::NorthEast)),
                false
            ),
            Hover::Grip
        );
        assert_eq!(window.calls(), vec!["cursor NeResize".to_owned()]);
        assert_eq!(hover(Some(&window), None, true), Hover::BackToNormal);
        assert_eq!(hover(Some(&window), None, false), Hover::Inside);
        assert_eq!(window.calls().len(), 1);
    }

    #[test]
    fn raising_unminimizes_shows_and_focuses_before_activating_with_the_token() {
        let window = FakeWindow::default();
        let mut tokens = vec![];
        let mut activate = |token: Option<String>| tokens.push(token);
        bring_forward(&window, Some(&mut activate), Some("t".to_owned()));
        assert_eq!(
            window.calls(),
            vec![
                "minimized false".to_owned(),
                "visible true".to_owned(),
                "focus".to_owned()
            ]
        );
        assert_eq!(tokens, vec![Some("t".to_owned())]);
    }

    #[test]
    fn without_wayland_activation_raising_only_focuses() {
        let window = FakeWindow::default();
        bring_forward(&window, None, Some("t".to_owned()));
        assert_eq!(window.calls().last().map(String::as_str), Some("focus"));
    }

    #[test]
    fn the_panel_is_activated_on_a_view_switch_only_with_a_token() {
        let mut tokens = vec![];
        let mut activate = |token: Option<String>| tokens.push(token);
        bring_panel_forward_with(Some(&mut activate), None);
        bring_panel_forward_with(Some(&mut activate), Some("t".to_owned()));
        bring_panel_forward_with(None, Some("u".to_owned()));
        assert_eq!(tokens, vec![Some("t".to_owned())]);
    }
}

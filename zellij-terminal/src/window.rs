use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use glutin::context::PossiblyCurrentContext;
use glutin::surface::{Surface, WindowSurface};
use glutin_winit::GlWindow;
use winit::event::KeyEvent;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState};
use winit::window::{Window, WindowId};
use zellij_utils::data::{ConnectToSession, HostTerminalThemeMode};
use zellij_utils::ipc::ClientToServerMsg;

use crate::config::options::Options;
use crate::config::settings::Settings;
use crate::config::window_state::{Shown, Startup};
use crate::emulation::selection::Selection;
use crate::emulation::terminal::{self, TerminalState};
use crate::input::clipboard::ClipboardHandle;
use crate::input::composition::Composition;
use crate::input::keyboard;
use crate::input::links::{self, LinkRun};
use crate::input::momentum::Momentum;
use crate::input::mouse::PointerState;
use crate::platform::Platform;
use crate::render::atlas::GlyphCache;
use crate::render::pacing::{Decision, Pacer};
use crate::render::renderer::Renderer;
use crate::render::retained::RetainedScene;
use crate::render::scene::{self, BlinkPhase, Margins};
use crate::render::scroll_animation::{ScrollAnimations, ScrollLayer};
use crate::session::client_loop::{Ending, LoopOptions, LoopOutcome, RenderSink};
use crate::session::connection::{
    Connection, Detacher, Geometry, GeometryHandle, Role, SharedSender,
};
use crate::text::font::CellMetrics;
use crate::window::notice::Notice;

mod attributes;
mod conductor;
mod desktop;
pub(crate) mod notice;
mod platform;
mod run;
mod terminal_app;
#[cfg(freedesktop)]
mod wayland_activation;
pub(crate) use desktop::{
    Connector, Desktop, DesktopMode, Hosting, PanelConnector, PreparedWindow, WindowMaker,
};

use attributes::{
    pick_config, popup_attributes, refresh_millihertz, requires_transparent_configs,
    transparency_available, window_icon,
};
pub(crate) use attributes::{startup_attributes, WindowOptions};
use platform::{
    activate_app, bring_opened_window_forward, created_hidden, is_wayland, named, set_accessory,
    set_option_as_alt, set_see_through, show_cloaked, skip_taskbar, square_without_system_border,
    tray_work_area, uncloak, with_activation_token, with_option_as_alt, Activation, Shell,
    WindowMemory,
};
pub(crate) use run::{
    as_panel, run, run_desktop, run_notice, terminal_app, with_activation, TerminalWindow,
    Windowing,
};
use run::{window_title, Client};

const BLINK_INTERVAL: Duration = Duration::from_millis(500);
const DISPLAY_RECHECK: Duration = Duration::from_secs(1);
const ZOOM_STEP: f64 = 1.1;
pub(crate) const ICON_PNG: &[u8] = zellij_utils::desktop_integration::ICON_PNG;

pub(crate) fn application_id() -> &'static str {
    crate::config::identity::identity().app_id
}

pub(crate) enum Wake {
    Window(u64, WindowWake),
    App(crate::app::AppEvent),
}

pub(crate) enum WindowWake {
    Frame(Vec<u8>),
    Reconfigured(Settings),
    SettingsPushed(Settings),
    ThemeMode(HostTerminalThemeMode),
    Finished(Ending),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Keep,
    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Handover {
    pub window: u64,
    pub tab: Option<usize>,
    pub pane: Option<zellij_utils::data::PaneId>,
    pub history: Option<zellij_utils::session_history::SessionHistory>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Switched {
    Kept,
    Closed,
    HandedOver(Handover),
}

#[derive(Debug, Clone, PartialEq)]
enum Followed {
    Kept,
    Ended,
    HandedOver(Handover),
}

pub(crate) fn window_to_reuse(
    standalone: bool,
    reuse_session_windows: bool,
    switching: u64,
    showing: Option<u64>,
) -> Option<u64> {
    if standalone || !reuse_session_windows {
        return None;
    }
    showing.filter(|window| *window != switching)
}

fn handover_to(window: u64, target: ConnectToSession) -> Handover {
    Handover {
        window,
        tab: target.tab_position,
        pane: target.pane_id.map(|(id, plugin)| {
            if plugin {
                zellij_utils::data::PaneId::Plugin(id)
            } else {
                zellij_utils::data::PaneId::Terminal(id)
            }
        }),
        history: target.switch_context.map(|context| context.history),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum WindowLook {
    Terminal,
    Popup {
        position: Option<winit::dpi::Position>,
    },
}

#[derive(Clone)]
pub(crate) struct AppPoster(Arc<Mutex<EventLoopProxy<Wake>>>);

impl AppPoster {
    pub(crate) fn new(proxy: EventLoopProxy<Wake>) -> Self {
        AppPoster(Arc::new(Mutex::new(proxy)))
    }

    pub(crate) fn post(&self, event: crate::app::AppEvent) -> bool {
        match self.0.lock() {
            Ok(proxy) => proxy.send_event(Wake::App(event)).is_ok(),
            Err(_) => false,
        }
    }
}

struct ProxySink {
    proxy: EventLoopProxy<Wake>,
    key: u64,
}

impl ProxySink {
    fn send(&self, wake: WindowWake) -> bool {
        self.proxy.send_event(Wake::Window(self.key, wake)).is_ok()
    }
}

impl RenderSink for ProxySink {
    fn frame(&mut self, frame: Vec<u8>) -> bool {
        self.send(WindowWake::Frame(frame))
    }

    fn finished(&mut self, ending: Ending) -> bool {
        self.send(WindowWake::Finished(ending))
    }

    fn acknowledges_frames(&self) -> bool {
        true
    }

    fn reconfigured(&mut self, settings: Settings) -> bool {
        self.send(WindowWake::Reconfigured(settings))
    }

    fn settings_pushed(&mut self, settings: Settings) -> bool {
        self.send(WindowWake::SettingsPushed(settings))
    }

    fn theme_mode(&mut self, mode: HostTerminalThemeMode) -> bool {
        self.send(WindowWake::ThemeMode(mode))
    }
}

struct Surfaces {
    _shell: Shell,
    window: Window,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    renderer: Renderer,
    size: winit::dpi::PhysicalSize<u32>,
}

pub(crate) type Switching = Box<dyn FnMut(&ConnectToSession, Geometry) -> Result<Connection>>;

pub(crate) struct App {
    state: TerminalState,
    cache: GlyphCache,
    metrics: CellMetrics,
    options: Options,
    settings: Settings,
    theme_mode: Option<HostTerminalThemeMode>,
    scale: f64,
    zoom: f64,
    sender: Option<SharedSender>,
    role: Role,
    geometry: GeometryHandle,
    modifiers: ModifiersState,
    alt_keys: keyboard::AltKeys,
    mouse: Mouse,
    ime: Composing,
    origin: (i32, i32),
    clipboard: ClipboardHandle,
    title: String,
    session_name: String,
    remote_session: bool,
    notice: Option<Notice>,
    retained: RetainedScene,
    timing: Timing,
    placement: Placement,
    surfaces: Option<Surfaces>,
    failure: Option<anyhow::Error>,
    leaving: bool,
    session: Option<Session>,
    ring: fn(),
    key: u64,
    appearance: Appearance,
    activation_token: Option<String>,
    focused: bool,
    scroll: ScrollAnimations,
    layer: ScrollLayer,
    wayland: bool,
    momentum: Momentum,
    standalone: bool,
}

struct Composing {
    composition: Composition,
    composed_row: Option<usize>,
    ime_area: Option<(i32, i32, u32, u32)>,
}

struct Mouse {
    pointer: PointerState,
    hovered_link: Option<LinkRun>,
    armed_link: Option<links::Armed>,
    selection: Option<Selection>,
    selecting: bool,
    pointer_hidden: bool,
}

struct Timing {
    blink: BlinkPhase,
    blink_since: Instant,
    pacer: Pacer,
    display_checked: Instant,
}

struct Placement {
    startup: Startup,
    state_path: Option<PathBuf>,
    remembered: WindowMemory,
    shown: Shown,
    windowed_cells: (usize, usize),
    windowed_known: bool,
    before_fullscreen: Shown,
}

struct Appearance {
    look: WindowLook,
    border: Option<crate::emulation::color::Srgb>,
    transparency_available: bool,
    warned_opaque: bool,
}

pub(crate) struct Session {
    client: Client,
    detacher: Detacher,
    loop_options: LoopOptions,
    proxy: EventLoopProxy<Wake>,
    key: u64,
    switching: Switching,
    outcomes: Vec<LoopOutcome>,
}

impl App {
    pub(crate) fn key(&self) -> u64 {
        self.key
    }

    pub(crate) fn window_id(&self) -> Option<WindowId> {
        self.surfaces.as_ref().map(|surfaces| surfaces.window.id())
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.surfaces.as_ref().map(|surfaces| &surfaces.window)
    }

    pub(crate) fn session_name(&self) -> Option<&str> {
        (self.sender.is_some() && !self.session_name.is_empty())
            .then_some(self.session_name.as_str())
    }

    pub(crate) fn take_failure(&mut self) -> Option<anyhow::Error> {
        self.failure.take()
    }

    pub(crate) fn take_session(&mut self) -> Option<Session> {
        self.session.take()
    }

    pub(crate) fn send_action(&self, action: zellij_utils::input::actions::Action) {
        if let Err(e) = self.tell(ClientToServerMsg::Action {
            action,
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        }) {
            report!("the window could not pass an action to its session: {}", e);
        }
    }

    pub(crate) fn leave(&mut self) {
        self.remember();
        self.closing();
    }

    pub(crate) fn wants_new_window(&self, event: &KeyEvent) -> bool {
        if !event.state.is_pressed() || event.repeat {
            return false;
        }
        let is_n = matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("n"));
        let modifiers = self.modifiers;
        let wanted = match Platform::current() {
            Platform::MacOs => modifiers.super_key() && !modifiers.control_key(),
            _ => modifiers.control_key() && modifiers.shift_key() && !modifiers.alt_key(),
        };
        is_n && wanted
    }

    pub(crate) fn wants_to_close(&self, event: &KeyEvent) -> bool {
        event.state.is_pressed()
            && !event.repeat
            && closes_the_panel(Platform::current(), self.modifiers, &event.logical_key)
    }

    pub(crate) fn modifiers_changed(&mut self, modifiers: winit::event::Modifiers) {
        self.modifiers = modifiers.state();
        self.alt_keys = keyboard::AltKeys::of(&modifiers);
    }

    pub(crate) fn on_resumed(&mut self, event_loop: &ActiveEventLoop) -> Verdict {
        if self.surfaces.is_some() {
            return Verdict::Keep;
        }
        if let Err(e) = self.open_window(event_loop) {
            self.failure = Some(e);
            let _ = self.detach();
            return Verdict::Close;
        }
        Verdict::Keep
    }

    pub(crate) fn on_wake(&mut self, wake: WindowWake) -> Verdict {
        match wake {
            WindowWake::Frame(frame) => self.on_frame(&frame),
            WindowWake::Reconfigured(settings) => self.reconfigured(settings),
            WindowWake::SettingsPushed(settings) => self.settings_pushed(settings),
            WindowWake::ThemeMode(mode) => {
                self.theme_mode = Some(mode);
                self.reapply(false);
            },
            WindowWake::Finished(ending) => {
                if self.session_ended(ending) {
                    return Verdict::Close;
                }
            },
        }
        Verdict::Keep
    }

    pub(crate) fn on_window_event(&mut self, event: WindowEvent) -> Verdict {
        match event {
            WindowEvent::CloseRequested => {
                if self.close_requested() {
                    return Verdict::Close;
                }
            },
            WindowEvent::Resized(size) => {
                if let Some(surfaces) = &mut self.surfaces {
                    surfaces.size = size;
                    surfaces
                        .window
                        .resize_surface(&surfaces.surface, &surfaces.context);
                }
                self.schedule_draw();
                self.resized(size.width, size.height);
            },
            WindowEvent::Moved(_) => self.follow_display_occasionally(),
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                mut inner_size_writer,
            } => {
                self.follow_display();
                self.rescale_keeping_cells(scale_factor, &mut inner_size_writer);
            },
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers_changed(modifiers);
                self.refresh_pointer();
            },
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                if !is_synthetic && event.state.is_pressed() {
                    if self.sender.is_none() {
                        if self.notice_key(&event.logical_key) {
                            return Verdict::Close;
                        }
                    } else {
                        self.on_key(&event);
                    }
                }
            },
            WindowEvent::CursorMoved { position, .. } => self.on_cursor_moved(position),
            WindowEvent::MouseInput { button, state, .. } => self.on_mouse_input(button, state),
            WindowEvent::MouseWheel { delta, phase, .. } => self.on_wheel(delta, phase),
            WindowEvent::CursorEntered { .. } => {
                self.show_pointer();
                self.mouse.pointer.entered();
                self.refresh_pointer();
            },
            WindowEvent::CursorLeft { .. } => self.on_cursor_left(),
            WindowEvent::Ime(event) => self.on_ime(event),
            WindowEvent::Focused(true) => self.set_focused(true),
            WindowEvent::Focused(false) => {
                self.set_focused(false);
                self.stop_composing();
                self.on_focus_lost();
            },
            WindowEvent::RedrawRequested => self.draw(),
            // Quitting from the Dock or at logout, macOS closes every window before the
            // app hears of it, while each can still be asked about itself. Elsewhere the
            // native window may already be gone, and asking about it can panic.
            WindowEvent::Destroyed => {
                if cfg!(target_os = "macos") && !self.leaving {
                    self.leave();
                }
                return Verdict::Close;
            },
            _ => {},
        }
        Verdict::Keep
    }

    pub(crate) fn wake_deadline(&mut self, now: Instant) -> Option<Instant> {
        let gliding = self.advance_momentum(now);
        self.advance_scroll_animations(now);
        let blinks = self.anything_blinks();
        if blinks && now >= self.blink_deadline() {
            self.advance_blink();
            self.schedule_draw();
        }
        let mut deadline = blinks.then(|| self.blink_deadline());
        if let Some(glide) = gliding {
            deadline = Some(deadline.map_or(glide, |other| other.min(glide)));
        }
        match self.timing.pacer.decide(now) {
            Decision::Now => {
                if let Some(surfaces) = &self.surfaces {
                    surfaces.window.request_redraw();
                }
            },
            Decision::At(due) => {
                deadline = Some(deadline.map_or(due, |blink| blink.min(due)));
            },
            Decision::Idle => {},
        }
        deadline
    }
}

pub(crate) fn border_rects(
    color: crate::emulation::color::Srgb,
    scale: f64,
    (width, height): (u32, u32),
    (left, top): (i32, i32),
) -> [scene::Rect; 4] {
    let line = (scale.round() as u32).max(1);
    let rect = |x: u32, y: u32, width: u32, height: u32| scene::Rect {
        x: x as i32 - left,
        y: y as i32 - top,
        width,
        height,
        color,
    };
    [
        rect(0, 0, width, line),
        rect(0, height.saturating_sub(line), width, line),
        rect(0, 0, line, height),
        rect(width.saturating_sub(line), 0, line, height),
    ]
}

// The panel has no close button, so macOS greys out Close Window while it is in front and
// hands Cmd+W to the panel as a key.
fn closes_the_panel(platform: Platform, modifiers: ModifiersState, key: &Key) -> bool {
    let is_w = matches!(key, Key::Character(c) if c.eq_ignore_ascii_case("w"));
    platform == Platform::MacOs && is_w && modifiers == ModifiersState::SUPER
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GridFit {
    pub rows: usize,
    pub cols: usize,
    pub origin: (i32, i32),
}

fn has_no_area(width: u32, height: u32) -> bool {
    width == 0 || height == 0
}

pub(crate) fn fit_grid(
    window: (u32, u32),
    cell: (u32, u32),
    padding: Margins,
    balance: bool,
) -> GridFit {
    let (cell_width, cell_height) = (cell.0.max(1), cell.1.max(1));
    let room_x = window.0.saturating_sub(padding.left + padding.right);
    let room_y = window.1.saturating_sub(padding.top + padding.bottom);
    let size = terminal::clamped(
        (room_y / cell_height) as usize,
        (room_x / cell_width) as usize,
    );
    let (mut x, mut y) = (padding.left, padding.top);
    if balance {
        x += room_x.saturating_sub(size.cols as u32 * cell_width) / 2;
        y += room_y.saturating_sub(size.rows as u32 * cell_height) / 2;
    }
    GridFit {
        rows: size.rows,
        cols: size.cols,
        origin: (x as i32, y as i32),
    }
}

// The shadow of a window with a wholly clear background is drawn from what the window
// shows, which keeps the compositor busy for as long as it is on screen.

// Without a transparent config a macOS window is always opaque. Elsewhere asking for one
// can leave no config at all: EGL drops every config an X11 visual cannot make see-through.

#[cfg(test)]
mod tests;

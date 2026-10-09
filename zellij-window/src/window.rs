use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use glutin::config::{Config, ConfigTemplateBuilder};
use glutin::context::PossiblyCurrentContext;
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use winit::application::ApplicationHandler;
use winit::event::KeyEvent;
use winit::event::{ElementState, Ime, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{
    CursorIcon, Icon, ImePurpose, UserAttentionType, Window, WindowAttributes, WindowId,
};
use zellij_utils::data::{ConnectToSession, HostTerminalThemeMode};
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::ipc::ClientToServerMsg;

use crate::atlas::GlyphCache;
use crate::bell;
use crate::client_loop::{self, Ending, LoopOptions, LoopOutcome, RenderSink};
use crate::clipboard::{self, Clipboard, ClipboardHandle};
use crate::composition::{Composition, Gate, Reaction};
use crate::connection::{
    request_detach, resize, send, Connection, Detacher, Geometry, GeometryHandle, Role,
    SharedSender,
};
use crate::font::{CellMetrics, FontStack};
use crate::graphics::create_context;
use crate::input;
use crate::links::{self, LinkRun};
use crate::momentum::{self, Momentum, Touch};
use crate::mouse::{self, PointerState};
use crate::notice::Notice;
use crate::notify;
use crate::options::{Change, Options};
use crate::pacing::{Decision, Pacer};
use crate::palette;
use crate::platform::Platform;
use crate::renderer::{Frame, Renderer};
use crate::retained::{self, Damage, RetainedScene};
use crate::scene::{self, BlinkPhase, Margins, Transparency};
use crate::scroll_animation::{ScrollAnimations, ScrollLayer, ScrollSettings};
use crate::selection::Selection;
use crate::settings::Settings;
use crate::terminal::{self, FrameError, TerminalState};
use crate::window_state::{self, Shown, Startup, WindowState};
use zellij_utils::input::window::{NotificationMode, OptionAsAlt, PaddingColor, StartupMode};

const BLINK_INTERVAL: Duration = Duration::from_millis(500);
const DISPLAY_RECHECK: Duration = Duration::from_secs(1);
const ZOOM_STEP: f64 = 1.1;
const APPLICATION_ID: &str = "zellij";
pub(crate) const ICON_PNG: &[u8] = include_bytes!("../../assets/logo128.png");

pub(crate) fn application_id() -> &'static str {
    APPLICATION_ID
}

#[derive(Debug)]
enum Wake {
    Frame(Vec<u8>),
    Reconfigured(Settings),
    SettingsPushed(Settings),
    ThemeMode(HostTerminalThemeMode),
    Finished(Ending),
}

struct ProxySink {
    proxy: EventLoopProxy<Wake>,
}

impl RenderSink for ProxySink {
    fn frame(&mut self, frame: Vec<u8>) -> bool {
        self.proxy.send_event(Wake::Frame(frame)).is_ok()
    }

    fn finished(&mut self, ending: Ending) -> bool {
        self.proxy.send_event(Wake::Finished(ending)).is_ok()
    }

    fn acknowledges_frames(&self) -> bool {
        true
    }

    fn reconfigured(&mut self, settings: Settings) -> bool {
        self.proxy.send_event(Wake::Reconfigured(settings)).is_ok()
    }

    fn settings_pushed(&mut self, settings: Settings) -> bool {
        self.proxy
            .send_event(Wake::SettingsPushed(settings))
            .is_ok()
    }

    fn theme_mode(&mut self, mode: HostTerminalThemeMode) -> bool {
        self.proxy.send_event(Wake::ThemeMode(mode)).is_ok()
    }
}

struct Surfaces {
    window: Window,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    renderer: Renderer,
}

type Switching = Box<dyn FnMut(&ConnectToSession, Geometry) -> Result<Connection>>;

struct App {
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
    alt_keys: input::AltKeys,
    pointer: PointerState,
    composition: Composition,
    composed_row: Option<usize>,
    ime_area: Option<(i32, i32, u32, u32)>,
    origin: (i32, i32),
    hovered_link: Option<LinkRun>,
    armed_link: Option<links::Armed>,
    selection: Option<Selection>,
    selecting: bool,
    clipboard: ClipboardHandle,
    title: String,
    session_name: String,
    notice: Option<Notice>,
    retained: RetainedScene,
    blink: BlinkPhase,
    blink_since: Instant,
    pacer: Pacer,
    display_checked: Instant,
    startup: Startup,
    state_path: Option<PathBuf>,
    shown: Shown,
    windowed_cells: (usize, usize),
    windowed_known: bool,
    surfaces: Option<Surfaces>,
    failure: Option<anyhow::Error>,
    leaving: bool,
    quit_requested: bool,
    session: Option<Session>,
    ring: fn(),
    notify: fn(NotificationMode, &crate::kitty::Notification) -> bool,
    transparency_available: bool,
    warned_opaque: bool,
    before_fullscreen: Shown,
    pointer_hidden: bool,
    focused: bool,
    scroll: ScrollAnimations,
    layer: ScrollLayer,
    wayland: bool,
    momentum: Momentum,
}

struct Session {
    client: Client,
    detacher: Detacher,
    loop_options: LoopOptions,
    proxy: EventLoopProxy<Wake>,
    switching: Switching,
    outcomes: Vec<LoopOutcome>,
}

impl App {
    fn rescale(&mut self, scale: f64) {
        if !(scale.is_finite() && scale > 0.0) || scale == self.scale {
            return;
        }
        self.rebuild_fonts(scale);
    }

    fn rebuild_fonts(&mut self, scale: f64) -> bool {
        self.rebuild_fonts_zoomed(scale, self.zoom)
    }

    fn rebuild_fonts_zoomed(&mut self, scale: f64, zoom: f64) -> bool {
        if !self.rebuild_fonts_quietly(scale, zoom) {
            return false;
        }
        let size = self
            .surfaces
            .as_ref()
            .map(|surfaces| surfaces.window.inner_size());
        if let Some(size) = size {
            self.relayout(size.width, size.height);
        }
        true
    }

    fn rebuild_fonts_quietly(&mut self, scale: f64, zoom: f64) -> bool {
        match self.options.font.stack(scale * zoom) {
            Ok(fonts) => {
                self.cancel_scroll_animations();
                self.stop_momentum();
                self.selecting = false;
                self.set_selection(None);
                self.metrics = fonts.metrics();
                self.cache = GlyphCache::new(fonts);
                self.scale = scale;
                self.zoom = zoom;
            },
            Err(e) => {
                report!(
                    "keeping the font already in place; {} px is unusable: {}",
                    self.options.font.size * (scale * zoom) as f32,
                    e
                );
                return false;
            },
        }
        true
    }

    fn relayout(&mut self, width: u32, height: u32) {
        self.cancel_scroll_animations();
        self.retained.mark_everything();
        self.schedule_draw();
        self.reflow(width, height);
    }

    fn rescale_keeping_cells(&mut self, scale: f64, writer: &mut winit::event::InnerSizeWriter) {
        self.stop_momentum();
        if !(scale.is_finite() && scale > 0.0) || scale == self.scale {
            return;
        }
        let (windowed, cells) = (self.shown == Shown::Windowed, self.windowed_cells);
        if !self.rebuild_fonts_quietly(scale, self.zoom) {
            return;
        }
        if windowed && self.surfaces.is_some() {
            let exact = self.exact_size(cells.0, cells.1);
            if writer.request_inner_size(exact).is_ok() {
                self.relayout(exact.width, exact.height);
                return;
            }
        }
        if let Some(size) = self
            .surfaces
            .as_ref()
            .map(|surfaces| surfaces.window.inner_size())
        {
            self.relayout(size.width, size.height);
        }
    }

    fn padding_px(&self) -> Margins {
        let pixels = |logical: f32| (logical as f64 * self.scale).round().max(0.0) as u32;
        let padding = self.options.padding;
        Margins {
            left: pixels(padding.left),
            top: pixels(padding.top),
            right: pixels(padding.right),
            bottom: pixels(padding.bottom),
        }
    }

    fn exact_size(&self, cols: usize, rows: usize) -> winit::dpi::PhysicalSize<u32> {
        let padding = self.padding_px();
        winit::dpi::PhysicalSize::new(
            self.metrics.width * cols as u32 + padding.left + padding.right,
            self.metrics.height * rows as u32 + padding.top + padding.bottom,
        )
    }

    fn settle_startup_size(&mut self) {
        if self.shown != Shown::Windowed {
            return;
        }
        let exact = self.exact_size(self.startup.cols, self.startup.rows);
        let Some(surfaces) = &self.surfaces else {
            return;
        };
        if surfaces.window.inner_size() == exact {
            return;
        }
        let applied = surfaces.window.request_inner_size(exact);
        if let Some(size) = applied {
            self.resized(size.width, size.height);
        }
    }

    fn zoom_by(&mut self, factor: f64) {
        self.set_zoom(self.zoom * factor);
    }

    fn set_zoom(&mut self, zoom: f64) {
        if !(zoom.is_finite() && zoom > 0.0) || zoom == self.zoom {
            return;
        }
        self.rebuild_fonts_zoomed(self.scale, zoom);
    }

    fn reconfigured(&mut self, settings: Settings) {
        self.settings = settings;
        let zoom_reset = self.zoom != 1.0;
        self.zoom = 1.0;
        self.reapply(zoom_reset);
    }

    fn settings_pushed(&mut self, settings: Settings) {
        let font_size_changed = settings.section.font_size != self.settings.section.font_size;
        self.settings = settings;
        let zoom_reset = font_size_changed && self.zoom != 1.0;
        if zoom_reset {
            self.zoom = 1.0;
        }
        self.reapply(zoom_reset);
    }

    fn cancel_scroll_animations(&mut self) {
        if self.scroll.is_active() {
            self.scroll.cancel();
            self.schedule_draw();
        }
    }

    fn scroll_settings(&self) -> ScrollSettings {
        ScrollSettings {
            enabled: self.options.smooth_scrolling,
            duration: self.options.scroll_animation,
        }
    }

    fn advance_scroll_animations(&mut self, now: Instant) {
        if !self.scroll.is_active() {
            return;
        }
        self.scroll.retire(now);
        self.schedule_draw();
    }

    fn reapply(&mut self, zoom_reset: bool) {
        self.cancel_scroll_animations();
        self.stop_momentum();
        let next = crate::options::resolve(&self.settings, self.theme_mode);
        let change = Change::between(&self.options, &next);
        if !change.is_anything() && !zoom_reset {
            return;
        }
        let previous_font = self.options.font.clone();
        self.options = next;

        if change.paints || change.minimum_contrast {
            self.retained.forget_contrast();
        }
        if change.paints {
            for msg in palette::seed_messages(&self.options.paints) {
                if let Err(e) = self.tell(msg) {
                    report!("failed to re-declare a color: {}", e);
                }
            }
        }
        if change.fonts || zoom_reset {
            if !self.rebuild_fonts(self.scale) {
                self.options.font = previous_font;
                if zoom_reset {
                    self.rebuild_fonts(self.scale);
                }
            }
        } else if change.layout {
            let size = self
                .surfaces
                .as_ref()
                .map(|surfaces| surfaces.window.inner_size());
            if let Some(size) = size {
                self.relayout(size.width, size.height);
            }
        } else if change.needs_redraw() {
            self.retained.mark_everything();
            self.schedule_draw();
        }
        if change.open_links {
            self.refresh_pointer();
        }
        if change.selection && !self.options.shift_drag_selects {
            self.selecting = false;
            self.set_selection(None);
        }
        if change.transparency {
            self.warn_if_opaque();
        }
        if change.blur {
            if let Some(surfaces) = &self.surfaces {
                surfaces.window.set_blur(self.options.blur);
            }
        }
        if change.option_as_alt {
            if let Some(surfaces) = &self.surfaces {
                set_option_as_alt(&surfaces.window, self.options.option_as_alt);
            }
        }
    }

    fn effective_transparency(&self) -> Transparency {
        if self.transparency_available {
            self.options.transparency
        } else {
            Transparency::OPAQUE
        }
    }

    fn warn_if_opaque(&mut self) {
        if self.transparency_available
            || self.warned_opaque
            || self.options.transparency.opacity >= 1.0
        {
            return;
        }
        self.warned_opaque = true;
        report!(
            "opacity {} was asked for, but the display offers no transparent \
             surface (is a compositor running?), so the window stays opaque",
            self.options.transparency.opacity
        );
    }

    fn grid(&self, width: u32, height: u32) -> GridFit {
        fit_grid(
            (width, height),
            (self.metrics.width, self.metrics.height),
            self.padding_px(),
            self.options.padding_balance,
        )
    }

    fn fitted(&self, width: u32, height: u32) -> Geometry {
        let grid = self.grid(width, height);
        Geometry {
            rows: grid.rows,
            cols: grid.cols,
            cell_width: self.metrics.width as usize,
            cell_height: self.metrics.height as usize,
        }
    }

    fn resized(&mut self, width: u32, height: u32) {
        self.cancel_scroll_animations();
        self.stop_momentum();
        self.observe_window();
        if self.shown == Shown::Windowed {
            let fitted = self.fitted(width, height);
            self.windowed_cells = (fitted.cols, fitted.rows);
            self.windowed_known = true;
        }
        self.reflow(width, height);
    }

    fn observe_window(&mut self) {
        let Some(surfaces) = &self.surfaces else {
            return;
        };
        let window = &surfaces.window;
        self.shown = if window.fullscreen().is_some() {
            Shown::Fullscreen
        } else if window.is_maximized() {
            Shown::Maximized
        } else {
            Shown::Windowed
        };
        if self.shown == Shown::Windowed {
            let size = window.inner_size();
            let fitted = self.fitted(size.width, size.height);
            self.windowed_cells = (fitted.cols, fitted.rows);
            self.windowed_known = true;
        }
    }

    fn window_state(&self) -> WindowState {
        WindowState {
            cols: self.windowed_cells.0,
            rows: self.windowed_cells.1,
            state: self.shown,
        }
    }

    fn remember(&mut self) {
        let Some(path) = self.state_path.clone() else {
            return;
        };
        self.observe_window();
        let mut state = self.window_state();
        if !self.windowed_known {
            if let Some(previous) = window_state::load_from(&path) {
                state.cols = previous.cols;
                state.rows = previous.rows;
            }
        }
        window_state::store(&path, state);
    }

    fn reflow(&mut self, width: u32, height: u32) {
        self.selecting = false;
        self.set_selection(None);
        let grid = self.grid(width, height);
        if grid.origin != self.origin {
            self.origin = grid.origin;
            self.pointer.set_origin(grid.origin.0, grid.origin.1);
            self.retained.mark_everything();
            self.schedule_draw();
        }
        let next = self.fitted(width, height);
        self.state
            .set_cell_size(self.metrics.width, self.metrics.height);
        if self.notice.is_some() {
            self.paint_notice(next.rows, next.cols);
        }
        if let Some(sender) = &self.sender {
            if let Err(e) = resize(sender, &self.geometry, next) {
                report!("failed to report a resize: {}", e);
            }
        }
    }

    fn on_frame(&mut self, frame: &[u8]) {
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

    fn tell(&self, msg: ClientToServerMsg) -> Result<()> {
        match &self.sender {
            Some(sender) => send(sender, msg),
            None => Ok(()),
        }
    }

    fn detach(&self) -> Result<()> {
        match &self.sender {
            Some(sender) => request_detach(sender, self.role),
            None => Ok(()),
        }
    }

    fn closing(&mut self) -> bool {
        let quitting = self.role == Role::Participant;
        let pressed_again = quitting && self.quit_requested;
        self.quit_requested = quitting;
        self.leaving = !quitting || pressed_again;
        let asked = if quitting {
            self.tell(ClientToServerMsg::Action {
                action: zellij_utils::input::actions::Action::Quit,
                terminal_id: None,
                client_id: None,
                is_cli_client: false,
            })
        } else {
            self.detach()
        };
        match asked {
            Ok(()) => self.sender.is_none() || pressed_again,
            Err(e) => {
                report!(
                    "the session could not be asked to let the window go, \
                     so the window closes on its own: {}",
                    e
                );
                true
            },
        }
    }

    fn set_title(&mut self, title: String) {
        self.title = title;
        if let Some(surfaces) = &self.surfaces {
            surfaces.window.set_title(&self.title);
        }
    }

    fn acknowledge(&mut self, seq: u64) {
        if let Err(e) = self.tell(ClientToServerMsg::RenderFrameAck { seq }) {
            report!("failed to acknowledge a frame: {}", e);
        }
    }

    fn retitle(&mut self) {
        let Some(title) = self.state.take_title() else {
            return;
        };
        let title = title.unwrap_or_else(|| self.title.clone());
        if let Some(surfaces) = &self.surfaces {
            surfaces.window.set_title(&title);
        }
    }

    fn attend(&mut self) {
        let rung = self.state.take_bells() > 0;
        let notifications = self.state.take_notifications();
        let mut notified = false;
        for notification in &notifications {
            notified |= !(self.notify)(self.options.notifications, notification);
        }
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

    fn on_cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        self.show_pointer();
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let event = self.pointer.moved(position, geometry, modifiers);
        let target = self.momentum_target();
        self.momentum.pointer_moved(target);
        self.refresh_hover();
        self.refresh_pointer();
        if self.selecting {
            self.extend_selection();
            return;
        }
        self.point(event);
    }

    fn on_mouse_input(&mut self, button: winit::event::MouseButton, state: ElementState) {
        if state.is_pressed() {
            self.show_pointer();
            self.stop_momentum();
        }
        if self.selection_gesture(button, state) {
            return;
        }
        if state.is_pressed() {
            self.set_selection(None);
        }
        self.note_link_click(button, state);
        if self.middle_click_pasted(button, state) {
            return;
        }
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let event = self.pointer.button(button, state, geometry, modifiers);
        self.point(event);
    }

    fn starts_a_selection(&self) -> bool {
        self.options.shift_drag_selects
            && self.modifiers.shift_key()
            && !(self.modifiers.control_key()
                || self.modifiers.alt_key()
                || self.modifiers.super_key())
    }

    fn selection_gesture(
        &mut self,
        button: winit::event::MouseButton,
        state: ElementState,
    ) -> bool {
        if button != winit::event::MouseButton::Left {
            return false;
        }
        if state.is_pressed() {
            if !self.starts_a_selection() {
                return false;
            }
            let Some(cell) = self.selection_cell() else {
                return false;
            };
            self.note_link_click(button, state);
            self.selecting = true;
            self.set_selection(Some(Selection::at(cell.0, cell.1)));
            return true;
        }
        if !self.selecting {
            return false;
        }
        self.selecting = false;
        if self
            .selection
            .is_some_and(|selection| !selection.is_empty())
        {
            self.armed_link = None;
        } else {
            self.set_selection(None);
            self.note_link_click(button, state);
        }
        true
    }

    fn selection_cell(&self) -> Option<(usize, usize)> {
        let (x, y) = self.pointer.cell_if_inside(self.geometry.get())?;
        let size = self.state.size();
        Some((
            (y as usize).min(size.rows.saturating_sub(1)),
            (x as usize).min(size.cols.saturating_sub(1)),
        ))
    }

    fn extend_selection(&mut self) {
        let (Some(selection), Some((row, col))) = (self.selection, self.selection_cell()) else {
            return;
        };
        let extended = selection.extended_to(row, col);
        if !extended.is_empty() {
            self.armed_link = None;
        }
        self.set_selection(Some(extended));
    }

    fn set_selection(&mut self, selection: Option<Selection>) {
        if selection == self.selection {
            return;
        }
        self.selection = selection;
        self.retained.set_selection(selection);
        self.schedule_draw();
    }

    fn copy_selection(&mut self) -> bool {
        let Some(selection) = self.selection.filter(|selection| !selection.is_empty()) else {
            return false;
        };
        let text = selection.text(self.state.screen());
        self.selecting = false;
        self.set_selection(None);
        if !text.is_empty() {
            self.with_clipboard(|clipboard| {
                clipboard.set(&text);
                clipboard.set_primary(&text);
            });
        }
        true
    }

    fn middle_click_pasted(
        &mut self,
        button: winit::event::MouseButton,
        state: ElementState,
    ) -> bool {
        if button != winit::event::MouseButton::Middle {
            return false;
        }
        if state.is_pressed() {
            let takes_the_click = mouse::pane_takes_the_click(
                Some(self.pointer.cell(self.geometry.get())),
                self.state.geometry(),
            );
            let enabled = self.options.middle_click_paste;
            let primary = (enabled && !takes_the_click)
                .then(|| self.with_clipboard(|clipboard| clipboard.get_primary()))
                .flatten()
                .flatten();
            match mouse::middle_click(enabled, primary, takes_the_click) {
                mouse::MiddleClick::Paste(text) => {
                    self.pointer.swallow_middle();
                    if let Err(e) = self.tell(clipboard::paste_message(text)) {
                        report!("failed to send a primary-selection paste: {}", e);
                    }
                    true
                },
                mouse::MiddleClick::Forward => false,
            }
        } else {
            self.pointer.take_swallowed_middle()
        }
    }

    fn note_link_click(&mut self, button: winit::event::MouseButton, state: ElementState) {
        if button != winit::event::MouseButton::Left {
            return;
        }
        let cell = self.pointer.cell_if_inside(self.geometry.get());
        if state.is_pressed() {
            if self.refresh_hover() {
                self.refresh_pointer();
            }
            self.armed_link = links::armed_by_press(
                self.link_opening(),
                self.pane_wants_mouse(),
                self.modifiers,
                cell,
                self.hovered_link.as_ref(),
            );
            return;
        }
        let Some(uri) = links::released_on(self.armed_link.take(), cell) else {
            return;
        };
        if let Err(e) = links::open(&uri) {
            report!("{}", e);
        }
    }

    fn link_opening(&self) -> links::Opening {
        links::Opening {
            enabled: self.options.open_links,
            with_shift: self.options.open_links_with_shift,
        }
    }

    fn pane_wants_mouse(&self) -> bool {
        mouse::pane_takes_the_click(
            self.pointer.cell_if_inside(self.geometry.get()),
            self.state.geometry(),
        )
    }

    fn refresh_hover(&mut self) -> bool {
        let hovered = self
            .pointer
            .cell_if_inside(self.geometry.get())
            .and_then(|(x, y)| links::run_at(self.state.screen(), y as usize, x as usize));
        if hovered == self.hovered_link {
            return false;
        }
        let mut rows = retained::hover_rows(self.hovered_link.as_ref());
        rows.extend(retained::hover_rows(hovered.as_ref()));
        self.hovered_link = hovered;
        self.retained.mark(&Damage::Rows(rows));
        self.schedule_draw();
        true
    }

    fn pointer_shape(&self) -> CursorIcon {
        let cell = self.pointer.cell_if_inside(self.geometry.get());
        let reachable = links::is_reachable(
            self.link_opening(),
            self.pane_wants_mouse(),
            self.modifiers,
            self.hovered_link.as_ref(),
        );
        mouse::pointer_shape_over(cell, self.state.geometry(), reachable)
    }

    fn refresh_pointer(&mut self) {
        let shape = self.pointer_shape();
        if let Some(surfaces) = &self.surfaces {
            surfaces.window.set_cursor(shape);
        }
    }

    fn hide_pointer(&mut self) {
        if !self.options.hide_pointer_while_typing || self.pointer_hidden {
            return;
        }
        self.pointer_hidden = true;
        if let Some(surfaces) = &self.surfaces {
            surfaces.window.set_cursor_visible(false);
        }
    }

    fn show_pointer(&mut self) {
        if !self.pointer_hidden {
            return;
        }
        self.pointer_hidden = false;
        if let Some(surfaces) = &self.surfaces {
            surfaces.window.set_cursor_visible(true);
        }
    }

    fn set_focused(&mut self, focused: bool) {
        if focused == self.focused {
            return;
        }
        self.focused = focused;
        self.retained
            .mark(&Damage::Rows(vec![self.state.cursor_position().0]));
        self.schedule_draw();
    }

    fn toggle_fullscreen(&mut self) {
        let leaving = self.shown == Shown::Fullscreen;
        if leaving {
            self.shown = self.before_fullscreen;
        } else {
            self.before_fullscreen = self.shown;
            self.shown = Shown::Fullscreen;
        }
        let Some(surfaces) = &self.surfaces else {
            return;
        };
        if leaving {
            surfaces.window.set_fullscreen(None);
            if self.shown == Shown::Maximized {
                surfaces.window.set_maximized(true);
            }
        } else {
            surfaces
                .window
                .set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
    }

    fn on_wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase) {
        self.on_wheel_at(delta, phase, Instant::now());
    }

    fn on_wheel_at(&mut self, delta: MouseScrollDelta, phase: TouchPhase, now: Instant) {
        self.show_pointer();
        if self.notice.is_some() {
            self.notice_wheel(delta);
            return;
        }
        self.selecting = false;
        self.set_selection(None);
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let events = self.pointer.wheel(delta, geometry, modifiers);
        if self.momentum_enabled() {
            match delta {
                MouseScrollDelta::LineDelta(..) => self.stop_momentum(),
                MouseScrollDelta::PixelDelta(pixels) => {
                    let touch = Touch {
                        phase,
                        pixels: pixels.y,
                        at: now,
                        target: self.momentum_target(),
                        position: self.pointer.position(geometry),
                        modifiers,
                        cell_height: geometry.cell_height.max(1) as f64,
                        friction: self.options.scroll_momentum_friction,
                    };
                    self.momentum.touchpad(touch);
                },
            }
        }
        self.point(events);
    }

    fn momentum_enabled(&self) -> bool {
        self.wayland && self.options.scroll_momentum
    }

    fn momentum_target(&self) -> Option<momentum::Target> {
        momentum::target_at(
            self.state.geometry(),
            self.pointer.cell_if_inside(self.geometry.get()),
        )
    }

    fn stop_momentum(&mut self) {
        self.momentum.stop();
    }

    fn tick_momentum(&mut self, now: Instant) -> Vec<MouseEvent> {
        let Some(glide) = self.momentum.tick(now) else {
            return Vec::new();
        };
        let delta =
            MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(0.0, glide.pixels));
        let events =
            self.pointer
                .wheel_at(delta, glide.position, self.geometry.get(), glide.modifiers);
        if !events.is_empty() {
            self.momentum.sent();
        }
        events
    }

    fn advance_momentum(&mut self, now: Instant) -> Option<Instant> {
        if !self.momentum.is_coasting() {
            return None;
        }
        let events = self.tick_momentum(now);
        self.point(events);
        self.momentum
            .is_coasting()
            .then(|| now + self.pacer.interval())
    }

    fn on_cursor_left(&mut self) {
        self.show_pointer();
        self.stop_momentum();
        self.pointer.left();
        self.refresh_pointer();
    }

    fn on_focus_lost(&mut self) {
        self.stop_momentum();
        self.show_pointer();
        if std::mem::take(&mut self.selecting)
            && self.selection.is_some_and(|selection| selection.is_empty())
        {
            self.set_selection(None);
        }
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let events = self.pointer.focus_lost(geometry, modifiers);
        self.point(events);
    }

    fn on_key(&mut self, event: &KeyEvent) {
        let option_as_alt = input::option_acts_as_alt(
            Platform::current(),
            self.options.option_as_alt,
            self.alt_keys,
        );
        self.press(input::Press::of(event, self.modifiers, option_as_alt));
    }

    fn press(&mut self, press: input::Press) {
        self.quit_requested = false;
        let gate = self.composition.gate();
        if gate == Gate::Composing {
            return;
        }
        if let Key::Dead(accent) = press.logical {
            match input::dead_key_press(&press) {
                input::DeadKeyOutcome::Wait => {
                    let reaction = self.composition.dead_key(accent);
                    self.compose(reaction);
                },
                input::DeadKeyOutcome::Type(text) => {
                    let reaction = self.composition.resolve();
                    self.compose(reaction);
                    self.type_text(text);
                },
            }
            return;
        }
        if gate == Gate::Resolving {
            let reaction = self.composition.resolve();
            self.compose(reaction);
            match input::resolve_pending_accent(&press) {
                input::Resolution::Key => self.deliver(&press),
                input::Resolution::Type(text) => self.type_text(text),
                input::Resolution::TypeThenKey(text) => {
                    self.type_text(text);
                    self.deliver(&press);
                },
                input::Resolution::Nothing => {},
            }
            return;
        }
        if let Some(text) = input::unidentified_text(&press) {
            self.type_text(text);
            return;
        }
        if input::claims(&self.options.zoom_in_keys, &press) {
            self.hide_pointer();
            self.stop_momentum();
            self.zoom_by(ZOOM_STEP);
            return;
        }
        if input::claims(&self.options.zoom_out_keys, &press) {
            self.hide_pointer();
            self.stop_momentum();
            self.zoom_by(1.0 / ZOOM_STEP);
            return;
        }
        if input::claims(&self.options.zoom_reset_keys, &press) {
            self.hide_pointer();
            self.stop_momentum();
            self.set_zoom(1.0);
            return;
        }
        if input::claims(&self.options.fullscreen_keys, &press) {
            self.hide_pointer();
            self.stop_momentum();
            self.toggle_fullscreen();
            return;
        }
        if input::claims(&self.options.copy_keys, &press) && self.copy_selection() {
            self.hide_pointer();
            self.stop_momentum();
            return;
        }
        if input::claims(&self.options.paste_keys, &press) {
            self.hide_pointer();
            self.stop_momentum();
            self.paste();
            return;
        }
        self.deliver(&press);
    }

    fn deliver(&mut self, press: &input::Press) {
        let Some(msg) = input::key_message(press) else {
            return;
        };
        self.set_selection(None);
        self.stop_momentum();
        self.hide_pointer();
        if let Err(e) = self.tell(msg) {
            report!("failed to send a key: {}", e);
        }
    }

    fn type_text(&mut self, text: String) {
        let Some(msg) = input::typed_text_message(text) else {
            return;
        };
        self.set_selection(None);
        self.stop_momentum();
        self.hide_pointer();
        if let Err(e) = self.tell(msg) {
            report!("failed to send typed text: {}", e);
        }
    }

    fn on_ime(&mut self, event: Ime) {
        let reaction = self.composition.ime(event);
        let committed = match reaction {
            Reaction::Commit(text) => Some(text),
            other => {
                self.compose(other);
                None
            },
        };
        let Some(text) = committed else {
            return;
        };
        self.compose(Reaction::Redraw);
        if text.is_empty() {
            return;
        }
        self.stop_momentum();
        let Some(msg) = input::typed_text_message(text) else {
            return;
        };
        self.set_selection(None);
        if let Err(e) = self.tell(msg) {
            report!("failed to send composed text: {}", e);
        }
    }

    fn compose(&mut self, reaction: Reaction) {
        if reaction == Reaction::Nothing {
            return;
        }
        let rows = retained::composing_rows(&self.state, self.composed_row.take());
        if self.composition.shown().is_some() {
            self.composed_row = Some(self.state.cursor_position().0);
        }
        self.retained.mark(&Damage::Rows(rows));
        self.schedule_draw();
    }

    fn stop_composing(&mut self) {
        let reaction = self.composition.cancel();
        self.compose(reaction);
    }

    fn paste(&mut self) {
        self.set_selection(None);
        let Some(chars) = self.with_clipboard(|clipboard| clipboard.get()).flatten() else {
            return;
        };
        if let Err(e) = self.tell(clipboard::paste_message(chars)) {
            report!("failed to send a paste: {}", e);
        }
    }

    fn copy(&mut self) {
        if let Some(text) = self.state.take_copied_text() {
            self.with_clipboard(|clipboard| clipboard.set(&text));
        }
    }

    fn cursor_options(&self) -> scene::CursorOptions {
        scene::CursorOptions {
            shape: self.options.cursor_shape,
            blink: self.options.cursor_blink,
            hollow: self.options.cursor_unfocused_hollow && !self.focused,
        }
    }

    fn anything_blinks(&self) -> bool {
        self.state.has_blinking_cells()
            || (self.state.cursor_is_visible()
                && self
                    .cursor_options()
                    .blinks(self.state.cursor_is_blinking()))
    }

    fn blink_deadline(&self) -> Instant {
        self.blink_since + BLINK_INTERVAL
    }

    fn advance_blink(&mut self) {
        self.blink_since = Instant::now();
        self.blink = match self.blink {
            BlinkPhase::On => BlinkPhase::Off,
            BlinkPhase::Off => BlinkPhase::On,
        };
        let damage = retained::blink_damage(&self.state, self.cursor_options());
        self.retained.mark(&damage);
    }

    fn with_clipboard<T>(&self, f: impl FnOnce(&mut Clipboard) -> T) -> Option<T> {
        match self.clipboard.lock() {
            Ok(mut clipboard) => Some(f(&mut clipboard)),
            Err(_) => {
                report!("the clipboard mutex was poisoned");
                None
            },
        }
    }

    fn point<I: IntoIterator<Item = MouseEvent>>(&mut self, events: I) {
        for event in events {
            self.quit_requested = false;
            if let Err(e) = self.tell(mouse::message(event)) {
                report!("failed to send a mouse event: {}", e);
            }
        }
    }

    fn schedule_draw(&mut self) {
        self.pacer.schedule();
    }

    fn follow_cursor_area(&mut self) {
        let (row, col) = self.state.cursor_position();
        let area = (
            self.origin.0 + (col as u32 * self.metrics.width) as i32,
            self.origin.1 + (row as u32 * self.metrics.height) as i32,
            self.metrics.width,
            self.metrics.height,
        );
        if self.ime_area == Some(area) {
            return;
        }
        let Some(surfaces) = &self.surfaces else {
            return;
        };
        surfaces.window.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(area.0, area.1),
            winit::dpi::PhysicalSize::new(area.2, area.3),
        );
        self.ime_area = Some(area);
    }

    fn follow_display(&mut self) {
        let millihertz = self
            .surfaces
            .as_ref()
            .and_then(|surfaces| refresh_millihertz(&surfaces.window));
        self.pacer.follow_refresh(millihertz);
        self.display_checked = Instant::now();
    }

    fn follow_display_occasionally(&mut self) {
        if Instant::now().duration_since(self.display_checked) < DISPLAY_RECHECK {
            return;
        }
        self.follow_display();
    }

    fn draw(&mut self) {
        if self.surfaces.is_none() {
            return;
        }
        let now = Instant::now();
        self.retained
            .set_held_out(self.scroll.held_out(&self.state));
        self.refresh_scene();
        self.build_scroll_layer(now);
        self.follow_cursor_area();
        let Some(surfaces) = self.surfaces.as_mut() else {
            return;
        };
        let size = surfaces.window.inner_size();
        let target = (size.width.max(1), size.height.max(1));
        let margins = match self.options.padding_color {
            PaddingColor::Background => Vec::new(),
            PaddingColor::Extend => {
                let grid = self.state.size();
                let (grid_width, grid_height) = (
                    self.metrics.width * grid.cols as u32,
                    self.metrics.height * grid.rows as u32,
                );
                let (left, top) = (self.origin.0.max(0) as u32, self.origin.1.max(0) as u32);
                scene::padding_rects(
                    &self.state,
                    &self.options.paints,
                    self.metrics,
                    self.effective_transparency(),
                    Margins {
                        left,
                        top,
                        right: target.0.saturating_sub(left + grid_width),
                        bottom: target.1.saturating_sub(top + grid_height),
                    },
                )
            },
        };
        let Some(surfaces) = self.surfaces.as_mut() else {
            return;
        };
        surfaces.renderer.draw_retained(
            &self.retained,
            self.cache.atlases(),
            target,
            Frame {
                origin: self.origin,
                margins: &margins,
                layer: self.scroll.is_active().then_some(&self.layer),
            },
        );
        if let Err(e) = surfaces.surface.swap_buffers(&surfaces.context) {
            report!("buffer swap failed: {}", e);
        }
        self.pacer.drawn(Instant::now());
    }

    fn refresh_scene(&mut self) {
        let cursor = self.cursor_options();
        let preedit = self.composition.shown();
        self.retained
            .set_transparency(self.effective_transparency());
        self.retained.set_contrast(self.options.minimum_contrast);
        self.retained.refresh(
            &self.state,
            &mut self.cache,
            self.blink,
            &self.options.paints,
            cursor,
            self.hovered_link.as_ref(),
            preedit.as_ref(),
        );
    }

    fn build_scroll_layer(&mut self, now: Instant) {
        if !self.scroll.is_active() {
            self.layer.clear();
            return;
        }
        let cursor = self.cursor_options();
        let preedit = self.composition.shown();
        self.scroll.build_layer(
            &mut self.layer,
            &self.state,
            &mut self.cache,
            self.blink,
            &self.options.paints,
            cursor,
            preedit.as_ref(),
            self.retained.transparency(),
            self.retained.contrast(),
            self.retained.all_images(),
            now,
        );
    }

    fn follow(&mut self, switch_to: Option<ConnectToSession>) -> bool {
        self.stop_momentum();
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        session.collect();
        let Some(target) = switch_to else {
            return false;
        };
        let geometry = self.geometry.get();
        match session.reconnect(&target, geometry) {
            Ok(connection) => {
                let session_name = connection.session_name.clone();
                self.sender = Some(connection.sender.clone());
                self.role = connection.role;
                self.geometry = connection.geometry.clone();
                session
                    .detacher
                    .follow(connection.sender.clone(), connection.role);
                session.spawn(connection);
                self.start_over(geometry);
                self.set_title(window_title(&session_name));
                self.session_name = session_name;
                for msg in palette::seed_messages(&self.options.paints) {
                    if let Err(e) = self.tell(msg) {
                        report!("failed to re-declare a color: {}", e);
                    }
                }
                true
            },
            Err(e) => {
                session.detacher.abandon();
                self.stranded(e);
                true
            },
        }
    }

    fn start_over(&mut self, geometry: Geometry) {
        self.cancel_scroll_animations();
        self.state = TerminalState::new(geometry.rows, geometry.cols);
        self.state
            .set_cell_size(self.metrics.width, self.metrics.height);
        self.composition.cancel();
        self.composed_row = None;
        self.selecting = false;
        self.set_selection(None);
        self.retained.mark_everything();
        self.schedule_draw();
    }

    fn session_ended(&mut self, ending: Ending) -> bool {
        if let Ending::Switched(to) = ending {
            return !self.follow(Some(to));
        }
        self.stop_momentum();
        if let Some(session) = self.session.as_mut() {
            session.collect();
        }
        if self.failure.is_some() {
            return false;
        }
        match crate::notice::trouble(&ending, &self.session_name) {
            Some(trouble) if !self.leaving => {
                self.stranded(anyhow!("{}", crate::notice::ended(&trouble)));
                false
            },
            _ => {
                self.remember();
                true
            },
        }
    }

    fn close_requested(&mut self) -> bool {
        self.remember();
        self.closing()
    }

    fn stranded(&mut self, failure: anyhow::Error) {
        let message = format!("{:#}", failure);
        report!("{}", message);
        self.sender = None;
        self.show_notice(&message);
        self.failure = Some(failure);
    }

    fn show_notice(&mut self, message: &str) {
        let size = self.state.size();
        self.notice = Some(Notice::new(message));
        self.cancel_scroll_animations();
        self.stop_momentum();
        self.paint_notice(size.rows, size.cols);
    }

    fn paint_notice(&mut self, rows: usize, cols: usize) {
        let Some(notice) = self.notice.as_mut() else {
            return;
        };
        let frame = notice.frame(rows, cols);
        if let Err(e) = self.state.apply_frame(&frame) {
            report!("the message could not be drawn: {}", e);
            return;
        }
        self.retained.mark_everything();
        self.schedule_draw();
    }

    fn scroll_notice(&mut self, scroll: impl FnOnce(&mut Notice, usize, usize) -> bool) {
        let size = self.state.size();
        let Some(notice) = self.notice.as_mut() else {
            return;
        };
        if scroll(notice, size.rows, size.cols) {
            self.paint_notice(size.rows, size.cols);
        }
    }

    fn notice_key(&mut self, key: &Key) -> bool {
        let page = Notice::page(self.state.size().rows);
        let by = match key {
            Key::Named(NamedKey::Enter | NamedKey::Escape) => return true,
            Key::Named(NamedKey::ArrowUp) => -1,
            Key::Named(NamedKey::ArrowDown) => 1,
            Key::Named(NamedKey::PageUp) => -page,
            Key::Named(NamedKey::PageDown) => page,
            Key::Named(NamedKey::Home) => -isize::MAX,
            Key::Named(NamedKey::End) => isize::MAX,
            _ => return false,
        };
        self.scroll_notice(|notice, rows, cols| notice.scroll(by, rows, cols));
        false
    }

    fn notice_wheel(&mut self, delta: MouseScrollDelta) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => crate::notice::wheel_lines(y),
            MouseScrollDelta::PixelDelta(pixels) => -pixels.y / self.metrics.height.max(1) as f64,
        };
        self.scroll_notice(|notice, rows, cols| notice.wheel(lines, rows, cols));
    }

    fn requested_size(&self) -> winit::dpi::Size {
        let width = (self.metrics.width * self.startup.cols as u32) as f64 / self.scale;
        let height = (self.metrics.height * self.startup.rows as u32) as f64 / self.scale;
        let padding = self.options.padding;
        winit::dpi::LogicalSize::new(
            width + padding.horizontal() as f64,
            height + padding.vertical() as f64,
        )
        .into()
    }

    fn open_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let surfaces = self.bring_up(event_loop)?;
        let size = surfaces.window.inner_size();
        let scale = surfaces.window.scale_factor();
        self.surfaces = Some(surfaces);
        self.wayland = is_wayland(event_loop);
        self.shown = Shown::of(self.startup.mode);
        self.windowed_cells = (self.startup.cols, self.startup.rows);
        self.windowed_known = self.startup.mode == StartupMode::Windowed;
        self.follow_display();
        self.schedule_draw();
        self.rescale(scale);
        self.resized(size.width, size.height);
        self.settle_startup_size();
        Ok(())
    }

    fn bring_up(&mut self, event_loop: &ActiveEventLoop) -> Result<Surfaces> {
        let attributes = with_option_as_alt(
            named(startup_attributes(
                WindowAttributes::default()
                    .with_title(self.title.clone())
                    .with_window_icon(window_icon())
                    .with_transparent(true)
                    .with_blur(self.options.blur)
                    .with_inner_size(self.requested_size()),
                self.startup.mode,
            )),
            self.options.option_as_alt,
        );

        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attributes))
            .build(
                event_loop,
                ConfigTemplateBuilder::new()
                    .with_alpha_size(8)
                    .with_transparency(true),
                pick_config,
            )
            .map_err(|e| anyhow!("failed to create a window: {}", e))?;
        self.transparency_available = config.supports_transparency() != Some(false);
        self.warn_if_opaque();
        let window = window.ok_or_else(|| anyhow!("the windowing system produced no window"))?;
        window.set_ime_purpose(ImePurpose::Terminal);
        window.set_ime_allowed(true);

        let handle = window
            .window_handle()
            .context("the window has no raw handle")?
            .as_raw();
        let display = config.display();

        let context = create_context(&display, &config, handle, Platform::current())?;

        let surface_attributes = window
            .build_surface_attributes(SurfaceAttributesBuilder::new())
            .context("failed to describe the window surface")?;
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes) }
            .context("failed to create the window surface")?;

        let context = context
            .make_current(&surface)
            .context("failed to make the window context current")?;
        if let Err(e) = surface.set_swap_interval(&context, SwapInterval::DontWait) {
            report!(
                "the display refused an unsynchronized buffer swap, so a swap may wait for the next refresh: {}",
                e
            );
        }

        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|symbol| display.get_proc_address(symbol))
        };

        Ok(Surfaces {
            window,
            surface,
            context,
            renderer: Renderer::new(gl)?,
        })
    }
}

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.surfaces.is_some() {
            return;
        }
        if let Err(e) = self.open_window(event_loop) {
            self.failure = Some(e);
            let _ = self.detach();
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        match event {
            Wake::Frame(frame) => self.on_frame(&frame),
            Wake::Reconfigured(settings) => self.reconfigured(settings),
            Wake::SettingsPushed(settings) => self.settings_pushed(settings),
            Wake::ThemeMode(mode) => {
                self.theme_mode = Some(mode);
                self.reapply(false);
            },
            Wake::Finished(ending) => {
                if self.session_ended(ending) {
                    event_loop.exit();
                }
            },
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                if self.close_requested() {
                    event_loop.exit();
                }
            },
            WindowEvent::Resized(size) => {
                if let Some(surfaces) = &self.surfaces {
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
                self.modifiers = modifiers.state();
                self.alt_keys = input::AltKeys::of(&modifiers);
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
                            event_loop.exit();
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
                self.pointer.entered();
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
            WindowEvent::Destroyed => event_loop.exit(),
            _ => {},
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
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
        match self.pacer.decide(now) {
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
        event_loop.set_control_flow(match deadline {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }
}

pub struct WindowOptions {
    pub options: Options,
    pub settings: Settings,
    pub detacher: Detacher,
    pub startup: Startup,
}

pub(crate) fn startup_attributes(
    attributes: WindowAttributes,
    mode: StartupMode,
) -> WindowAttributes {
    match mode {
        StartupMode::Windowed | StartupMode::Remember => attributes,
        StartupMode::Maximized => attributes.with_maximized(true),
        StartupMode::Fullscreen => {
            attributes.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)))
        },
    }
}

pub fn run(
    connection: Connection,
    mut loop_options: LoopOptions,
    window: WindowOptions,
    fonts: FontStack,
    switching: Switching,
) -> Result<LoopOutcome> {
    let renderer = Rendering::bring_up(&window.options, fonts, window.startup);
    let windowing = Windowing::bring_up()?;
    let geometry = connection.geometry.clone();
    let role = connection.role;
    let sender = connection.sender.clone();
    let title = window_title(&connection.session_name);
    let session_name = connection.session_name.clone();
    let clipboard: ClipboardHandle = Arc::new(Mutex::new(Clipboard::open()));
    let state_path = loop_options.record_path.is_none().then(window_state::path);
    loop_options.clipboard = Some(clipboard.clone());

    let mut session = Session {
        client: Client::placeholder(),
        detacher: window.detacher,
        loop_options,
        proxy: windowing.proxy(),
        switching,
        outcomes: Vec::new(),
    };
    session.spawn(connection);

    let mut app = renderer.into_app(
        Some(sender),
        role,
        geometry,
        title,
        window.settings,
        clipboard,
        Some(session),
    );
    app.state_path = state_path;
    app.session_name = session_name;
    let outcome = windowing.drive(&mut app);
    let mut session = app.session.take();
    if let Some(session) = session.as_mut() {
        session.collect();
    }
    outcome?;
    session
        .map(Session::outcome)
        .unwrap_or_else(|| Err(anyhow!("the window never opened a session")))
}

pub fn run_notice(
    message: &str,
    geometry: Geometry,
    fonts: FontStack,
    options: &Options,
) -> Result<()> {
    let metrics = fonts.metrics();
    let mut state = TerminalState::new(geometry.rows, geometry.cols);
    state.set_cell_size(metrics.width, metrics.height);
    let windowing = Windowing::bring_up()?;
    let startup = Startup::resolve(
        options.startup_mode,
        None,
        Some(geometry.rows),
        Some(geometry.cols),
    );
    let mut app = Rendering::from_state(state, options, fonts, startup).into_app(
        None,
        Role::Watcher,
        GeometryHandle::new(geometry),
        "zellij".to_owned(),
        Settings::default(),
        Arc::new(Mutex::new(Clipboard::open())),
        None,
    );
    app.show_notice(message);
    windowing.drive(&mut app)
}

impl Session {
    fn spawn(&mut self, connection: Connection) {
        let sink = ProxySink {
            proxy: self.proxy.clone(),
        };
        self.client = Client::spawn(connection, self.loop_options.clone(), sink);
    }

    fn reconnect(&mut self, to: &ConnectToSession, geometry: Geometry) -> Result<Connection> {
        (self.switching)(to, geometry)
    }

    fn collect(&mut self) {
        match self.client.take().map(Client::join) {
            Some(Ok(outcome)) => self.outcomes.push(outcome),
            Some(Err(e)) => report!("{}", e),
            None => {},
        }
    }

    fn outcome(self) -> Result<LoopOutcome> {
        let mut outcomes = self.outcomes.into_iter();
        let Some(first) = outcomes.next() else {
            return Err(anyhow!("the window never opened a session"));
        };
        Ok(outcomes.fold(first, |total, next| LoopOutcome {
            render_count: total.render_count + next.render_count,
            discarded_ansi: total.discarded_ansi + next.discarded_ansi,
            recorded_messages: total.recorded_messages + next.recorded_messages,
            exit_reason: next.exit_reason,
            switch_to: next.switch_to,
        }))
    }
}

fn window_title(session_name: &str) -> String {
    format!("zellij: {}", session_name)
}

struct Rendering {
    state: TerminalState,
    cache: GlyphCache,
    metrics: CellMetrics,
    options: Options,
    startup: Startup,
}

impl Rendering {
    fn bring_up(options: &Options, fonts: FontStack, startup: Startup) -> Self {
        let cache = GlyphCache::new(fonts);
        let metrics = cache.metrics();
        let mut state = TerminalState::new(startup.rows, startup.cols);
        state.set_cell_size(metrics.width, metrics.height);
        Self {
            state,
            cache,
            metrics,
            options: options.clone(),
            startup,
        }
    }

    fn from_state(
        state: TerminalState,
        options: &Options,
        fonts: FontStack,
        startup: Startup,
    ) -> Self {
        let cache = GlyphCache::new(fonts);
        let metrics = cache.metrics();
        Self {
            state,
            cache,
            metrics,
            options: options.clone(),
            startup,
        }
    }

    fn into_app(
        self,
        sender: Option<SharedSender>,
        role: Role,
        geometry: GeometryHandle,
        title: String,
        settings: Settings,
        clipboard: ClipboardHandle,
        session: Option<Session>,
    ) -> App {
        App {
            state: self.state,
            cache: self.cache,
            metrics: self.metrics,
            options: self.options,
            settings,
            theme_mode: None,
            scale: 1.0,
            zoom: 1.0,
            sender,
            role,
            geometry,
            modifiers: ModifiersState::empty(),
            alt_keys: input::AltKeys::default(),
            pointer: PointerState::new(),
            composition: Composition::new(),
            composed_row: None,
            ime_area: None,
            origin: (0, 0),
            hovered_link: None,
            armed_link: None,
            selection: None,
            selecting: false,
            clipboard,
            title,
            session_name: String::new(),
            notice: None,
            retained: RetainedScene::new(),
            blink: BlinkPhase::On,
            blink_since: Instant::now(),
            pacer: Pacer::new(None),
            display_checked: Instant::now(),
            startup: self.startup,
            state_path: None,
            shown: Shown::of(self.startup.mode),
            windowed_cells: (self.startup.cols, self.startup.rows),
            windowed_known: self.startup.mode == StartupMode::Windowed,
            surfaces: None,
            failure: None,
            leaving: false,
            quit_requested: false,
            session,
            ring: bell::ring,
            notify: notify::handled,
            transparency_available: true,
            warned_opaque: false,
            before_fullscreen: Shown::Windowed,
            pointer_hidden: false,
            focused: true,
            scroll: ScrollAnimations::default(),
            layer: ScrollLayer::default(),
            wayland: false,
            momentum: Momentum::default(),
        }
    }
}

struct Windowing {
    event_loop: EventLoop<Wake>,
}

impl Windowing {
    fn bring_up() -> Result<Self> {
        Ok(Self {
            event_loop: EventLoop::<Wake>::with_user_event()
                .build()
                .context("failed to create a windowing event loop")?,
        })
    }

    fn proxy(&self) -> EventLoopProxy<Wake> {
        self.event_loop.create_proxy()
    }

    fn drive(self, app: &mut App) -> Result<()> {
        let outcome = self
            .event_loop
            .run_app(app)
            .context("the windowing event loop failed");
        match app.failure.take() {
            Some(failure) => Err(failure),
            None => outcome,
        }
    }
}

struct Client(Option<std::thread::JoinHandle<Result<LoopOutcome>>>);

impl Client {
    fn placeholder() -> Self {
        Client(None)
    }

    fn spawn(connection: Connection, options: LoopOptions, mut sink: ProxySink) -> Self {
        let proxy = sink.proxy.clone();
        Client(Some(std::thread::spawn(move || {
            guarded(
                || client_loop::run_with_sink(connection, options, &mut sink),
                |message| {
                    let _ = proxy.send_event(Wake::Finished(Ending::Failed(message)));
                },
            )
        })))
    }

    fn take(&mut self) -> Option<Client> {
        self.0.take().map(|thread| Client(Some(thread)))
    }

    fn join(self) -> Result<LoopOutcome> {
        match self.0 {
            Some(thread) => thread
                .join()
                .map_err(|_| anyhow!("the client thread panicked"))?,
            None => Err(anyhow!("there was no client thread to wait for")),
        }
    }
}

fn guarded<T>(work: impl FnOnce() -> Result<T>, on_crash: impl FnOnce(String)) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(outcome) => outcome,
        Err(payload) => {
            let message = format!(
                "The connection to the session crashed: {}",
                crate::diagnostics::panic_text(payload.as_ref())
            );
            on_crash(message.clone());
            Err(anyhow!(message))
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GridFit {
    pub rows: usize,
    pub cols: usize,
    pub origin: (i32, i32),
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

fn refresh_millihertz(window: &Window) -> Option<u32> {
    window
        .current_monitor()
        .or_else(|| window.primary_monitor())
        .and_then(|monitor| monitor.refresh_rate_millihertz())
}

fn window_icon() -> Option<Icon> {
    let image = match crate::image_io::decode(ICON_PNG) {
        Ok(image) => image,
        Err(e) => {
            report!("the embedded window icon is unreadable: {e}");
            return None;
        },
    };
    match Icon::from_rgba(image.pixels, image.width, image.height) {
        Ok(icon) => Some(icon),
        Err(e) => {
            report!("the embedded window icon was refused: {e}");
            None
        },
    }
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
fn is_wayland(event_loop: &ActiveEventLoop) -> bool {
    use winit::platform::wayland::ActiveEventLoopExtWayland;
    event_loop.is_wayland()
}

#[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "android"))))]
fn is_wayland(_event_loop: &ActiveEventLoop) -> bool {
    false
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
fn named(attributes: WindowAttributes) -> WindowAttributes {
    use winit::platform::wayland::WindowAttributesExtWayland;
    use winit::platform::x11::WindowAttributesExtX11;
    let id = application_id();
    let attributes = WindowAttributesExtX11::with_name(attributes, id, id);
    WindowAttributesExtWayland::with_name(attributes, id, id)
}

#[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "android"))))]
fn named(attributes: WindowAttributes) -> WindowAttributes {
    let _ = application_id();
    attributes
}

#[cfg(target_os = "macos")]
fn native_option_as_alt(side: OptionAsAlt) -> winit::platform::macos::OptionAsAlt {
    use winit::platform::macos::OptionAsAlt as Native;
    match side {
        OptionAsAlt::None => Native::None,
        OptionAsAlt::Left => Native::OnlyLeft,
        OptionAsAlt::Right => Native::OnlyRight,
        OptionAsAlt::Both => Native::Both,
    }
}

#[cfg(target_os = "macos")]
fn with_option_as_alt(attributes: WindowAttributes, side: OptionAsAlt) -> WindowAttributes {
    use winit::platform::macos::WindowAttributesExtMacOS;
    attributes.with_option_as_alt(native_option_as_alt(side))
}

#[cfg(not(target_os = "macos"))]
fn with_option_as_alt(attributes: WindowAttributes, _side: OptionAsAlt) -> WindowAttributes {
    attributes
}

#[cfg(target_os = "macos")]
fn set_option_as_alt(window: &Window, side: OptionAsAlt) {
    use winit::platform::macos::WindowExtMacOS;
    window.set_option_as_alt(native_option_as_alt(side));
}

#[cfg(not(target_os = "macos"))]
fn set_option_as_alt(_window: &Window, _side: OptionAsAlt) {}

fn pick_config(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    let configs: Vec<Config> = configs.collect();
    let best = best_config(
        configs
            .iter()
            .map(|config| (config.supports_transparency(), config.num_samples())),
    )
    .expect("the display offered no configs");
    configs
        .into_iter()
        .nth(best)
        .expect("the chosen config is one of those offered")
}

fn best_config(candidates: impl Iterator<Item = (Option<bool>, u8)>) -> Option<usize> {
    candidates
        .enumerate()
        .min_by_key(|(_, (transparency, samples))| {
            let rank = match transparency {
                Some(true) => 0u8,
                None => 1,
                Some(false) => 2,
            };
            (rank, *samples)
        })
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    use winit::dpi::PhysicalPosition;
    use winit::event::{MouseButton, MouseScrollDelta};
    use winit::keyboard::{Key, NamedKey, SmolStr};
    use zellij_utils::input::actions::Action;
    use zellij_utils::input::mouse::MouseEventType;
    use zellij_utils::ipc::{ClientToServerMsg, ExitReason};

    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::clipboard::Clipboard;
    use crate::color::Paints;
    use crate::connection::{test_attach_at as attach_at, Capabilities, Connection};
    use crate::font::{pixels_to_points, FontOptions, DEFAULT_FONT_SIZE, DEFAULT_LIGATURES};
    use crate::test_server::FakeServer;
    use zellij_utils::input::window::{WindowConfig, WindowTheme};

    const HANDSHAKE: usize = 8;

    fn geometry() -> Geometry {
        Geometry {
            rows: 40,
            cols: 120,
            cell_width: 10,
            cell_height: 20,
        }
    }

    fn test_options(intercept_paste: bool) -> Options {
        Options {
            font: FontOptions {
                family: None,
                size: DEFAULT_FONT_SIZE,
                system_fonts: false,
                ligatures: DEFAULT_LIGATURES,
                ..FontOptions::default()
            },
            paste_keys: if intercept_paste {
                crate::options::default_paste_keys()
            } else {
                Vec::new()
            },
            zoom_in_keys: crate::options::default_zoom_in_keys(),
            zoom_out_keys: crate::options::default_zoom_out_keys(),
            zoom_reset_keys: crate::options::default_zoom_reset_keys(),
            middle_click_paste: true,
            open_links: true,
            open_links_with_shift: true,
            shift_drag_selects: true,
            copy_keys: crate::options::default_copy_keys(),
            bell: zellij_utils::input::window::BellMode::Visual,
            notifications: zellij_utils::input::window::NotificationMode::Attention,
            paints: Paints::default(),
            cursor_shape: None,
            cursor_blink: None,
            startup_mode: StartupMode::Windowed,
            transparency: Transparency::OPAQUE,
            blur: false,
            padding: crate::options::Padding::default(),
            padding_balance: false,
            padding_color: PaddingColor::Background,
            initial_cols: None,
            initial_rows: None,
            fullscreen_keys: crate::options::default_fullscreen_keys(),
            hide_pointer_while_typing: false,
            cursor_unfocused_hollow: true,
            minimum_contrast: 1.0,
            smooth_scrolling: true,
            scroll_animation: Duration::from_millis(100),
            scroll_momentum: true,
            scroll_momentum_friction: 2.0,
            option_as_alt: OptionAsAlt::Left,
        }
    }

    struct Harness {
        app: App,
        server: FakeServer,
        clipboard: ClipboardHandle,
        _connection: Connection,
    }

    impl Harness {
        fn new(expected: usize, intercept_paste: bool, clipboard_text: &str) -> Self {
            Self::with(expected, test_options(intercept_paste), clipboard_text)
        }

        fn with(expected: usize, options: Options, clipboard_text: &str) -> Self {
            let total = HANDSHAKE + expected;
            let server = FakeServer::spawn(move |side| side.expect(total));
            let connection = attach_at(
                &server.path,
                "app-test",
                geometry(),
                Capabilities::default(),
            )
            .unwrap();

            let mut owned = Clipboard::in_memory();
            if !clipboard_text.is_empty() {
                owned.set(clipboard_text);
            }
            let clipboard: ClipboardHandle = Arc::new(Mutex::new(owned));

            let fonts = options.font.stack(1.0).unwrap();
            let startup = Startup::windowed(geometry().rows, geometry().cols);
            let app = Rendering::bring_up(&options, fonts, startup).into_app(
                Some(connection.sender.clone()),
                connection.role,
                connection.geometry.clone(),
                "test".to_owned(),
                settings(WindowConfig::default()),
                clipboard.clone(),
                None,
            );

            Self {
                app,
                server,
                clipboard,
                _connection: connection,
            }
        }

        fn clipboard_text(&self) -> Option<String> {
            self.clipboard.lock().unwrap().get()
        }

        fn press(&mut self, logical: &Key) {
            let modifiers = self.app.modifiers;
            self.app
                .press(input::Press::new(logical.clone(), modifiers));
        }

        fn set_primary(&mut self, text: &str) {
            self.clipboard.lock().unwrap().set_primary(text);
        }

        fn sent(self) -> Vec<ClientToServerMsg> {
            let mut received = self.server.finish();
            received.split_off(HANDSHAKE)
        }

        fn actions(self) -> Vec<Action> {
            self.sent()
                .into_iter()
                .filter_map(|msg| match msg {
                    ClientToServerMsg::Action { action, .. } => Some(action),
                    other => panic!("expected an action, got {:?}", other),
                })
                .collect()
        }
    }

    fn character(text: &str) -> Key {
        Key::Character(SmolStr::new(text))
    }

    fn at(x: f64, y: f64) -> PhysicalPosition<f64> {
        PhysicalPosition::new(x, y)
    }

    fn margins(left: u32, top: u32, right: u32, bottom: u32) -> Margins {
        Margins {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn without_padding_the_grid_starts_at_the_corner_and_leftovers_go_right_and_down() {
        let fit = fit_grid((805, 413), (8, 20), Margins::default(), false);
        assert_eq!((fit.cols, fit.rows, fit.origin), (100, 20, (0, 0)));
    }

    #[test]
    fn padding_is_taken_off_the_window_before_the_grid_is_fitted() {
        let fit = fit_grid((820, 430), (8, 20), margins(10, 5, 10, 5), false);
        assert_eq!((fit.cols, fit.rows), (100, 21));
        assert_eq!(fit.origin, (10, 5));
        let fit = fit_grid((819, 430), (8, 20), margins(10, 5, 10, 6), false);
        assert_eq!((fit.cols, fit.rows), (99, 20));
        assert_eq!(fit.origin, (10, 5));
    }

    #[test]
    fn balanced_padding_shares_the_leftover_pixels_on_both_sides() {
        let fit = fit_grid((827, 437), (8, 20), margins(10, 5, 10, 5), true);
        assert_eq!((fit.cols, fit.rows), (100, 21));
        assert_eq!(fit.origin, (10 + 3, 5 + 3));
        let unbalanced = fit_grid((827, 437), (8, 20), margins(10, 5, 10, 5), false);
        assert_eq!(unbalanced.origin, (10, 5));
    }

    #[test]
    fn a_window_too_small_for_padding_and_a_cell_still_holds_one_cell_at_the_padding() {
        for balance in [false, true] {
            let fit = fit_grid((15, 12), (8, 20), margins(10, 10, 10, 10), balance);
            assert_eq!((fit.cols, fit.rows), (1, 1));
            assert_eq!(fit.origin, (10, 10));
            let fit = fit_grid((0, 0), (8, 20), margins(4, 4, 4, 4), balance);
            assert_eq!((fit.cols, fit.rows, fit.origin), (1, 1, (4, 4)));
        }
    }

    #[test]
    fn a_reflow_with_padding_moves_the_pointer_and_reports_the_smaller_grid() {
        let mut options = test_options(true);
        options.padding = crate::options::Padding {
            top: 10.0,
            right: 10.0,
            bottom: 10.0,
            left: 20.0,
        };
        let mut harness = Harness::with(3, options, "");
        harness.app.reflow(350, 420);
        assert_eq!(harness.app.origin, (20, 10));
        let geometry = harness.app.geometry.get();
        assert_eq!((geometry.cols, geometry.rows), (40, 20));
        harness
            .app
            .on_cursor_moved(at(20.0 + 8.0 * 3.0 + 1.0, 10.0 + 20.0 + 1.0));
        assert_eq!(harness.app.pointer.cell(geometry), (3, 1));
        assert_eq!(
            harness.sent()[0],
            ClientToServerMsg::TerminalResize {
                new_size: zellij_utils::pane_size::Size { rows: 20, cols: 40 },
            }
        );
    }

    fn padded_harness() -> Harness {
        let mut options = test_options(true);
        options.padding = crate::options::Padding {
            top: 3.0,
            right: 5.0,
            bottom: 7.0,
            left: 11.0,
        };
        Harness::with(0, options, "")
    }

    #[test]
    fn the_first_window_asks_for_its_cells_plus_padding_as_a_logical_size() {
        let harness = padded_harness();
        let startup = harness.app.startup;
        let expected = winit::dpi::LogicalSize::new(
            (8 * startup.cols + 11 + 5) as f64,
            (20 * startup.rows + 3 + 7) as f64,
        );
        match harness.app.requested_size() {
            winit::dpi::Size::Logical(size) => assert_eq!(size, expected),
            other => panic!("the startup size must be logical, got {:?}", other),
        }
    }

    #[test]
    fn the_exact_size_holds_the_cells_and_the_padding_at_the_display_scale() {
        let mut harness = padded_harness();
        assert_eq!(
            harness.app.exact_size(100, 30),
            winit::dpi::PhysicalSize::new(8 * 100 + 16, 20 * 30 + 10)
        );
        harness.app.scale = 2.0;
        assert_eq!(
            harness.app.exact_size(100, 30),
            winit::dpi::PhysicalSize::new(8 * 100 + 32, 20 * 30 + 20),
            "padding is logical and must double with the display scale"
        );
        let exact = harness.app.exact_size(100, 30);
        let fitted = harness.app.fitted(exact.width, exact.height);
        assert_eq!((fitted.cols, fitted.rows), (100, 30));
    }

    #[test]
    fn a_window_stranded_by_a_failed_switch_can_still_be_closed() {
        let mut harness = Harness::new(0, true, "");
        harness.app.stranded(anyhow!("no session to switch to"));

        assert!(harness.app.sender.is_none());
        assert!(harness.app.failure.is_some(), "the window exits in failure");
        assert!(
            harness.app.closing(),
            "a stranded window must close on the first request"
        );
        assert!(
            harness.app.state.dump().contains("no session to switch to"),
            "the reason is shown in the window"
        );
        assert!(harness.sent().is_empty());
    }

    fn shown(harness: &Harness) -> String {
        harness
            .app
            .state
            .dump()
            .lines()
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn a_session_that_ends_normally_closes_the_window() {
        for reason in [
            ExitReason::Normal,
            ExitReason::NormalDetached,
            ExitReason::CustomExitStatus(0),
        ] {
            let mut harness = Harness::new(0, true, "");
            assert!(
                harness.app.session_ended(Ending::Exited(reason.clone())),
                "{:?} must close the window",
                reason
            );
            assert!(harness.app.failure.is_none(), "{:?}", reason);
        }
    }

    #[test]
    fn a_session_that_lets_the_window_go_for_a_reason_keeps_it_open_with_the_reason() {
        let mut harness = Harness::new(0, true, "");
        assert!(
            !harness
                .app
                .session_ended(Ending::Exited(ExitReason::KickedByHost)),
            "the window stays so the reason can be read"
        );
        assert!(shown(&harness).contains("Disconnected by host"));
        assert!(shown(&harness).contains(crate::notice::LEAVE));
        assert!(harness.app.sender.is_none());
        assert!(
            harness.app.failure.is_some(),
            "closing it later is a failure"
        );
        assert!(harness.app.closing(), "the first close request closes it");
    }

    #[test]
    fn a_server_error_is_shown_in_the_window() {
        let mut harness = Harness::new(0, true, "");
        harness.app.session_ended(Ending::Exited(ExitReason::Error(
            "the screen thread failed".to_owned(),
        )));
        let shown = shown(&harness);
        assert!(shown.contains("Error occurred in server:"), "{}", shown);
        assert!(shown.contains("the screen thread failed"), "{}", shown);
    }

    #[test]
    fn a_non_zero_exit_status_keeps_the_window_open_and_fails() {
        let mut harness = Harness::new(0, true, "");
        assert!(!harness
            .app
            .session_ended(Ending::Exited(ExitReason::CustomExitStatus(3))));
        assert!(
            shown(&harness).contains("exit status 3"),
            "{}",
            shown(&harness)
        );
        assert!(harness.app.failure.is_some());
    }

    #[test]
    fn a_disconnect_names_the_session_the_window_was_showing() {
        let mut harness = Harness::new(0, true, "");
        harness.app.session_name = "work".to_owned();
        harness
            .app
            .session_ended(Ending::Exited(ExitReason::Disconnect));
        assert!(
            shown(&harness).contains("`zellij attach work`"),
            "{}",
            shown(&harness)
        );
    }

    #[test]
    fn only_enter_or_escape_dismiss_a_message() {
        let mut harness = Harness::new(0, true, "");
        harness.app.session_ended(Ending::Lost);
        for key in [
            Key::Character(SmolStr::new("q")),
            Key::Named(NamedKey::Space),
            Key::Named(NamedKey::ArrowDown),
            Key::Named(NamedKey::Tab),
        ] {
            assert!(!harness.app.notice_key(&key), "{:?}", key);
        }
        assert!(harness.app.notice_key(&Key::Named(NamedKey::Enter)));
        assert!(harness.app.notice_key(&Key::Named(NamedKey::Escape)));
    }

    #[test]
    fn a_long_message_scrolls_with_keys_and_the_wheel() {
        let mut harness = Harness::new(0, true, "");
        let long = (0..100)
            .map(|n| format!("detail {}", n))
            .collect::<Vec<_>>()
            .join("\n");
        harness.app.stranded(anyhow!("{}", long));
        assert!(shown(&harness).contains("detail 0 "));
        assert!(!shown(&harness).contains("detail 99"));
        harness.app.notice_key(&Key::Named(NamedKey::End));
        assert!(shown(&harness).contains("detail 99"), "{}", shown(&harness));
        assert!(!shown(&harness).contains("detail 0 "));
        harness.app.notice_key(&Key::Named(NamedKey::Home));
        assert!(shown(&harness).contains("detail 0 "));
        harness
            .app
            .on_wheel(MouseScrollDelta::LineDelta(0.0, -1.0), TouchPhase::Moved);
        assert_eq!(harness.app.notice.as_ref().unwrap().top(), 3);
        harness.app.notice_key(&Key::Named(NamedKey::ArrowUp));
        assert_eq!(harness.app.notice.as_ref().unwrap().top(), 2);
    }

    #[test]
    fn a_message_is_redrawn_to_fit_a_resized_window() {
        let mut harness = Harness::new(0, true, "");
        harness.app.session_ended(Ending::Lost);
        harness.app.reflow(8 * 30, 20 * 12);
        let size = harness.app.state.size();
        assert_eq!((size.cols, size.rows), (30, 12));
        let shown = shown(&harness);
        assert!(shown.contains("closed without a"), "{}", shown);
        assert!(shown.contains("server may have crashed."), "{}", shown);
    }

    #[test]
    fn a_connection_dropped_without_a_word_is_explained() {
        let mut harness = Harness::new(0, true, "");
        assert!(!harness.app.session_ended(Ending::Lost));
        assert!(
            shown(&harness).contains("closed without a reason"),
            "{}",
            shown(&harness)
        );
    }

    #[test]
    fn a_failure_on_the_window_side_is_shown_in_the_window() {
        let mut harness = Harness::new(0, true, "");
        assert!(!harness.app.session_ended(Ending::Failed(
            "this session's server predates it".to_owned()
        )));
        assert!(shown(&harness).contains("this session's server predates it"));
    }

    #[test]
    fn a_watcher_that_asked_to_leave_closes_however_the_session_ends() {
        let mut harness = Harness::new(1, true, "");
        harness.app.role = Role::Watcher;
        assert!(!harness.app.closing());
        assert!(harness.app.session_ended(Ending::Lost));
        assert!(harness.app.failure.is_none());
    }

    #[test]
    fn a_window_that_asked_to_quit_closes_when_the_session_lets_it_go() {
        let mut harness = Harness::new(1, true, "");
        assert!(!harness.app.closing());
        assert!(harness
            .app
            .session_ended(Ending::Exited(ExitReason::Normal)));
        assert!(harness.app.failure.is_none());
        assert_eq!(harness.actions(), vec![Action::Quit]);
    }

    #[test]
    fn a_window_showing_an_error_stays_open_when_the_session_lets_it_go() {
        let mut harness = Harness::new(0, true, "");
        harness.app.session_ended(Ending::Lost);
        assert!(
            !harness
                .app
                .session_ended(Ending::Exited(ExitReason::NormalDetached)),
            "a later normal ending must not hide the message"
        );
        assert!(shown(&harness).contains("closed without a reason"));
    }

    #[test]
    fn an_unreadable_frame_leaves_the_session_and_says_why() {
        let mut harness = Harness::new(1, true, "");
        harness.app.on_frame(&[0xde, 0xad, 0xbe, 0xef]);
        assert!(harness.app.sender.is_none());
        assert!(harness.app.failure.is_some());
        assert!(
            shown(&harness).contains("cannot read"),
            "{}",
            shown(&harness)
        );
        assert!(
            !harness
                .app
                .session_ended(Ending::Exited(ExitReason::NormalDetached)),
            "the detach that follows must leave the message up"
        );
        assert_eq!(harness.actions(), vec![Action::Detach]);
    }

    #[test]
    fn a_crash_in_the_connection_is_reported_and_becomes_an_error() {
        let mut reported = None;
        let outcome: Result<()> = guarded(
            || panic!("the decoder fell over"),
            |message| reported = Some(message),
        );
        let error = outcome.expect_err("a crash must become an error");
        let reported = reported.expect("the crash must be reported");
        assert!(reported.contains("the decoder fell over"), "{}", reported);
        assert_eq!(error.to_string(), reported);
    }

    #[test]
    fn work_that_does_not_crash_passes_its_outcome_through() {
        let mut reported = false;
        assert_eq!(guarded(|| Ok(7), |_| reported = true).unwrap(), 7);
        assert!(guarded::<()>(|| Err(anyhow!("plain failure")), |_| reported = true).is_err());
        assert!(!reported);
    }

    #[test]
    fn a_close_request_sends_quit_and_keeps_the_window_until_the_server_answers() {
        let mut harness = Harness::new(1, true, "");
        assert!(
            !harness.app.closing(),
            "a window showing a live session waits for the server"
        );
        assert_eq!(harness.actions(), vec![Action::Quit]);
    }

    #[test]
    fn pressing_close_again_closes_the_window_even_if_the_session_does_not_answer() {
        let mut harness = Harness::new(2, true, "");
        assert!(!harness.app.closing());
        assert!(harness.app.closing(), "the second press closes the window");
        assert!(harness.app.leaving);
        assert_eq!(harness.actions(), vec![Action::Quit, Action::Quit]);
    }

    #[test]
    fn typing_after_the_first_close_makes_the_next_press_ask_again() {
        let mut harness = Harness::new(3, true, "");
        assert!(!harness.app.closing());
        harness.press(&Key::Named(NamedKey::Escape));
        assert!(!harness.app.closing());
    }

    #[test]
    fn a_watcher_leaves_without_quitting() {
        let mut harness = Harness::new(1, true, "");
        harness.app.role = Role::Watcher;
        assert!(!harness.app.closing());
        assert!(
            matches!(harness.sent()[0], ClientToServerMsg::Key { .. }),
            "a watcher leaves with Esc as before"
        );
    }

    #[test]
    fn with_no_session_a_window_still_closes_itself() {
        let mut harness = Harness::new(0, true, "");
        harness.app.sender = None;
        assert!(harness.app.closing());
        assert!(harness.sent().is_empty());
    }

    fn f11() -> Key {
        Key::Named(NamedKey::F11)
    }

    #[test]
    fn the_fullscreen_key_toggles_and_is_not_sent_to_the_session() {
        let mut harness = Harness::new(0, true, "");
        harness.app.shown = Shown::Windowed;
        harness.press(&f11());
        assert_eq!(harness.app.shown, Shown::Fullscreen);
        harness.press(&f11());
        assert_eq!(harness.app.shown, Shown::Windowed);
        assert!(harness.sent().is_empty());
    }

    #[test]
    fn leaving_fullscreen_returns_to_maximized_when_that_is_where_it_came_from() {
        let mut harness = Harness::new(0, true, "");
        harness.app.shown = Shown::Maximized;
        harness.press(&f11());
        assert_eq!(harness.app.shown, Shown::Fullscreen);
        harness.press(&f11());
        assert_eq!(harness.app.shown, Shown::Maximized);
    }

    #[test]
    fn the_windowed_size_survives_a_fullscreen_round_trip() {
        let mut harness = Harness::new(4, true, "");
        let dir = remembering(&mut harness);
        harness.app.resized(720, 600);
        harness.press(&f11());
        harness.app.resized(1600, 1000);
        harness.press(&f11());
        harness.app.remember();
        assert_eq!(
            remembered(&dir),
            Some(WindowState {
                cols: 90,
                rows: 30,
                state: Shown::Windowed,
            })
        );
    }

    #[test]
    fn closing_while_fullscreen_keeps_the_windowed_size_and_reopens_fullscreen() {
        let mut harness = Harness::new(4, true, "");
        let dir = remembering(&mut harness);
        harness.app.resized(720, 600);
        harness.press(&f11());
        harness.app.resized(1600, 1000);
        harness.app.remember();
        let saved = remembered(&dir).expect("the state was saved");
        assert_eq!(
            saved,
            WindowState {
                cols: 90,
                rows: 30,
                state: Shown::Fullscreen,
            }
        );
        let startup = Startup::resolve(StartupMode::Remember, Some(saved), None, None);
        assert_eq!(startup.mode, StartupMode::Fullscreen);
        assert_eq!((startup.cols, startup.rows), (90, 30));
        assert!(
            startup_attributes(WindowAttributes::default(), startup.mode)
                .fullscreen
                .is_some()
        );
    }

    #[test]
    fn closing_in_fullscreen_without_a_windowed_size_this_run_keeps_the_saved_one() {
        let mut harness = Harness::new(0, true, "");
        let dir = remembering(&mut harness);
        window_state::store(
            &dir.path().join("window-state.json"),
            WindowState {
                cols: 70,
                rows: 20,
                state: Shown::Windowed,
            },
        );
        harness.app.windowed_known = false;
        harness.app.shown = Shown::Fullscreen;
        harness.app.remember();
        assert_eq!(
            remembered(&dir),
            Some(WindowState {
                cols: 70,
                rows: 20,
                state: Shown::Fullscreen,
            })
        );
    }

    fn hiding(expected: usize) -> Harness {
        let mut options = test_options(true);
        options.hide_pointer_while_typing = true;
        Harness::with(expected, options, "")
    }

    #[test]
    fn the_pointer_hides_on_a_sent_key_and_returns_when_it_moves() {
        let mut harness = hiding(2);
        harness.press(&character("a"));
        assert!(harness.app.pointer_hidden);
        harness.app.on_cursor_moved(at(5.0, 5.0));
        assert!(!harness.app.pointer_hidden);
    }

    #[test]
    fn a_modifier_alone_does_not_hide_the_pointer() {
        let mut harness = hiding(0);
        for named in [
            NamedKey::Shift,
            NamedKey::Control,
            NamedKey::Alt,
            NamedKey::Super,
        ] {
            harness.press(&Key::Named(named));
        }
        assert!(!harness.app.pointer_hidden);
        assert!(harness.sent().is_empty());
    }

    #[test]
    fn a_window_key_setting_hides_the_pointer_and_every_pointer_event_shows_it() {
        let mut harness = hiding(1);
        harness.press(&f11());
        assert!(harness.app.pointer_hidden);
        harness
            .app
            .on_wheel(MouseScrollDelta::LineDelta(0.0, 0.0), TouchPhase::Moved);
        assert!(!harness.app.pointer_hidden);
        harness.press(&f11());
        assert!(harness.app.pointer_hidden);
        harness.app.on_focus_lost();
        assert!(!harness.app.pointer_hidden);
        harness.press(&f11());
        harness
            .app
            .on_mouse_input(MouseButton::Right, ElementState::Pressed);
        assert!(!harness.app.pointer_hidden);
    }

    #[test]
    fn with_the_setting_off_typing_leaves_the_pointer_alone() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&character("a"));
        assert!(!harness.app.pointer_hidden);
    }

    #[test]
    fn a_focus_change_turns_the_cursor_hollow_and_redraws_its_row() {
        let mut harness = Harness::new(0, true, "");
        assert!(harness.app.focused, "a new window counts as focused");
        assert!(!harness.app.cursor_options().hollow);
        harness.app.refresh_scene();
        harness.app.set_focused(false);
        assert!(harness.app.cursor_options().hollow);
        harness.app.refresh_scene();
        assert_eq!(
            harness.app.retained.rebuilt_rows(),
            &[harness.app.state.cursor_position().0]
        );
        harness.app.set_focused(true);
        assert!(!harness.app.cursor_options().hollow);
        harness.app.options.cursor_unfocused_hollow = false;
        harness.app.set_focused(false);
        assert!(!harness.app.cursor_options().hollow);
    }

    #[test]
    fn a_close_request_with_no_session_left_closes_the_window_itself() {
        let mut harness = Harness::new(0, true, "");
        harness.app.sender = None;
        assert!(
            harness.app.closing(),
            "a window with no session must close on its own"
        );
        assert!(harness.sent().is_empty());
    }

    #[test]
    fn a_key_press_is_forwarded_to_the_pane() {
        let mut harness = Harness::new(1, true, "");
        harness.press(&character("a"));
        assert_eq!(
            harness.sent(),
            vec![ClientToServerMsg::Key {
                key: zellij_utils::data::KeyWithModifier::new(zellij_utils::data::BareKey::Char(
                    'a'
                )),
                raw_bytes: b"a".to_vec(),
                is_kitty_keyboard_protocol: false,
            }]
        );
    }

    #[test]
    fn ctrl_shift_v_pastes_the_clipboard_rather_than_the_key() {
        let mut harness = Harness::new(1, true, "copied");
        harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
        harness.press(&character("V"));
        assert_eq!(
            harness.actions(),
            vec![Action::Paste {
                chars: "copied".to_owned(),
                pane_id: None
            }]
        );
    }

    #[test]
    fn shift_insert_pastes_the_clipboard_too() {
        let mut harness = Harness::new(1, true, "copied");
        harness.app.modifiers = ModifiersState::SHIFT;
        harness.press(&Key::Named(NamedKey::Insert));
        assert_eq!(
            harness.actions(),
            vec![Action::Paste {
                chars: "copied".to_owned(),
                pane_id: None
            }]
        );
    }

    #[test]
    fn with_no_paste_key_the_chord_reaches_the_pane_instead() {
        let mut harness = Harness::new(1, false, "copied");
        harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
        harness.press(&character("V"));
        match &harness.sent()[0] {
            ClientToServerMsg::Key { raw_bytes, .. } => assert_eq!(raw_bytes, &vec![0x16]),
            other => panic!("expected a key, got {:?}", other),
        }
    }

    #[test]
    fn an_empty_clipboard_sends_no_paste_at_all() {
        let mut harness = Harness::new(1, true, "");
        harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
        harness.press(&character("V"));
        harness.app.modifiers = ModifiersState::empty();
        harness.press(&character("a"));
        assert_eq!(harness.sent().len(), 1);
    }

    static RINGS_COUNTED: AtomicUsize = AtomicUsize::new(0);
    static NOTIFICATIONS_COUNTED: AtomicUsize = AtomicUsize::new(0);

    fn count_a_ring() {
        RINGS_COUNTED.fetch_add(1, Ordering::Relaxed);
    }

    fn count_a_notification(
        _mode: NotificationMode,
        _notification: &crate::kitty::Notification,
    ) -> bool {
        NOTIFICATIONS_COUNTED.fetch_add(1, Ordering::Relaxed);
        true
    }

    fn frame_with_sideband(rows: usize, cols: usize, seq: u64, sideband: &[u8]) -> Vec<u8> {
        let mut builder =
            zellij_utils::structured_render::FrameBuilder::new(cols as u16, rows as u16, seq);
        builder.sideband_mut().extend_from_slice(sideband);
        builder.finish()
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn every_option_as_alt_side_reaches_the_windowing_system() {
        use winit::platform::macos::OptionAsAlt as Native;
        for (side, native) in [
            (OptionAsAlt::None, Native::None),
            (OptionAsAlt::Left, Native::OnlyLeft),
            (OptionAsAlt::Right, Native::OnlyRight),
            (OptionAsAlt::Both, Native::Both),
        ] {
            assert_eq!(native_option_as_alt(side), native, "{:?}", side);
        }
    }

    #[test]
    fn the_startup_mode_reaches_the_attributes_the_window_is_built_with() {
        let windowed = startup_attributes(WindowAttributes::default(), StartupMode::Windowed);
        assert!(!windowed.maximized);
        assert!(windowed.fullscreen.is_none());

        let maximized = startup_attributes(WindowAttributes::default(), StartupMode::Maximized);
        assert!(maximized.maximized);
        assert!(maximized.fullscreen.is_none());

        let fullscreen = startup_attributes(WindowAttributes::default(), StartupMode::Fullscreen);
        assert!(!fullscreen.maximized);
        assert_eq!(
            fullscreen.fullscreen,
            Some(winit::window::Fullscreen::Borderless(None)),
            "a terminal must never change the display's video mode"
        );

        let remembered = startup_attributes(WindowAttributes::default(), StartupMode::Remember);
        assert!(!remembered.maximized);
        assert!(remembered.fullscreen.is_none());
    }

    fn remembering(harness: &mut Harness) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        harness.app.state_path = Some(dir.path().join("window-state.json"));
        dir
    }

    fn remembered(dir: &tempfile::TempDir) -> Option<WindowState> {
        window_state::load_from(&dir.path().join("window-state.json"))
    }

    #[test]
    fn closing_after_a_resize_remembers_the_new_size_in_cells() {
        let mut harness = Harness::new(3, true, "");
        let dir = remembering(&mut harness);
        harness.app.resized(720, 600);
        assert!(
            !harness.app.close_requested(),
            "a window showing a live session waits for the server"
        );
        assert_eq!(
            remembered(&dir),
            Some(WindowState {
                cols: 90,
                rows: 30,
                state: Shown::Windowed,
            })
        );
        let sent = harness.sent();
        assert_eq!(
            sent[0],
            ClientToServerMsg::TerminalResize {
                new_size: zellij_utils::pane_size::Size { rows: 30, cols: 90 },
            }
        );
        assert!(matches!(
            sent[2],
            ClientToServerMsg::Action {
                action: Action::Quit,
                ..
            }
        ));
    }

    #[test]
    fn a_maximized_close_keeps_the_windowed_size_already_saved() {
        let mut harness = Harness::new(0, true, "");
        let dir = remembering(&mut harness);
        window_state::save_to(
            harness.app.state_path.as_deref().unwrap(),
            WindowState {
                cols: 70,
                rows: 20,
                state: Shown::Windowed,
            },
        )
        .unwrap();
        harness.app.shown = Shown::Maximized;
        harness.app.windowed_known = false;
        harness.app.remember();
        assert_eq!(
            remembered(&dir),
            Some(WindowState {
                cols: 70,
                rows: 20,
                state: Shown::Maximized,
            })
        );
    }

    #[test]
    fn without_a_state_path_nothing_is_remembered() {
        let mut harness = Harness::new(0, true, "");
        let dir = tempfile::TempDir::new().unwrap();
        harness.app.remember();
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
        assert!(harness.app.state_path.is_none());
    }

    #[test]
    fn a_frame_carrying_osc_52_in_its_sideband_reaches_the_clipboard() {
        let mut harness = Harness::new(1, true, "");
        let size = harness.app.state.size();
        let frame = frame_with_sideband(size.rows, size.cols, 0, b"\x1b]52;c;Y29waWVk\x1b\\");
        harness.app.on_frame(&frame);
        assert_eq!(harness.clipboard_text().as_deref(), Some("copied"));
    }

    #[test]
    fn a_frame_without_osc_52_leaves_the_clipboard_alone() {
        let mut harness = Harness::new(1, true, "kept");
        let size = harness.app.state.size();
        let frame = frame_with_sideband(size.rows, size.cols, 0, b"");
        harness.app.on_frame(&frame);
        assert_eq!(harness.clipboard_text().as_deref(), Some("kept"));
    }

    #[test]
    fn an_applied_frame_is_acknowledged_by_its_sequence_number() {
        let mut harness = Harness::new(1, true, "");
        let size = harness.app.state.size();
        let frame = frame_with_sideband(size.rows, size.cols, 17, b"");
        harness.app.on_frame(&frame);
        assert_eq!(
            harness.sent(),
            vec![ClientToServerMsg::RenderFrameAck { seq: 17 }]
        );
    }

    #[test]
    fn an_unreadable_frame_detaches_rather_than_being_acknowledged() {
        let mut harness = Harness::new(1, true, "");
        harness.app.on_frame(b"nonsense");
        assert_eq!(
            harness.actions(),
            vec![zellij_utils::input::actions::Action::Detach]
        );
    }

    #[test]
    fn a_frame_for_a_viewport_the_screen_has_moved_past_is_acknowledged_and_dropped() {
        let mut harness = Harness::new(1, true, "");
        let size = harness.app.state.size();
        let mut stale =
            zellij_utils::structured_render::FrameBuilder::new(size.cols as u16 - 1, 4, 23);
        stale.push_row(0, 0, &[zellij_utils::structured_render::WireCell::BLANK]);
        harness.app.on_frame(&stale.finish());
        assert_eq!(harness.app.state.size(), size);
        assert_eq!(
            harness.sent(),
            vec![ClientToServerMsg::RenderFrameAck { seq: 23 }]
        );
    }

    #[test]
    fn a_reflow_reports_the_new_size_without_reshaping_the_screen_locally() {
        let mut harness = Harness::new(2, true, "");
        let before = harness.app.state.size();
        harness.app.reflow(320, 400);
        assert_eq!(harness.app.state.size(), before);
        assert_eq!(
            harness.sent()[0],
            ClientToServerMsg::TerminalResize {
                new_size: zellij_utils::pane_size::Size { rows: 20, cols: 40 },
            }
        );
    }

    #[test]
    fn a_title_a_bell_and_a_notification_are_all_drained_by_a_frame() {
        let mut harness = Harness::new(1, true, "");
        harness.app.ring = count_a_ring;
        harness.app.notify = count_a_notification;
        let offered_before = NOTIFICATIONS_COUNTED.load(Ordering::Relaxed);
        let size = harness.app.state.size();
        let frame = frame_with_sideband(
            size.rows,
            size.cols,
            0,
            b"\x1b]0;zellij - pane\x07\x07\x1b]99;i=1:d=0;body\x1b\\",
        );
        harness.app.on_frame(&frame);
        assert_eq!(harness.app.state.take_title(), None);
        assert_eq!(harness.app.state.take_bells(), 0);
        assert!(harness.app.state.take_notifications().is_empty());
        assert_eq!(
            NOTIFICATIONS_COUNTED.load(Ordering::Relaxed) - offered_before,
            0,
            "this OSC 99 declares d=0, so it is still being assembled and nothing may be offered \
             to the desktop yet"
        );
    }

    #[test]
    fn a_completed_notification_is_offered_to_the_desktop_exactly_once() {
        let mut harness = Harness::new(1, true, "");
        harness.app.ring = count_a_ring;
        harness.app.notify = count_a_notification;
        let offered_before = NOTIFICATIONS_COUNTED.load(Ordering::Relaxed);
        let size = harness.app.state.size();
        let frame = frame_with_sideband(size.rows, size.cols, 0, b"\x1b]9;body\x07");
        harness.app.on_frame(&frame);
        assert!(harness.app.state.take_notifications().is_empty());
        assert_eq!(
            NOTIFICATIONS_COUNTED.load(Ordering::Relaxed) - offered_before,
            1,
            "a completed notification was not offered to the desktop exactly once"
        );
    }

    #[test]
    fn a_frame_that_signals_nothing_leaves_the_title_untouched() {
        let mut harness = Harness::new(1, true, "");
        let size = harness.app.state.size();
        let frame = frame_with_sideband(size.rows, size.cols, 0, b"");
        harness.app.on_frame(&frame);
        assert_eq!(harness.app.state.take_title(), None);
    }

    #[test]
    fn a_click_reaches_the_server_as_a_mouse_action() {
        let mut harness = Harness::new(2, true, "");
        harness.app.on_cursor_moved(at(35.0, 51.0));
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        let events: Vec<_> = harness
            .actions()
            .into_iter()
            .map(|action| match action {
                Action::MouseEvent { event } => event,
                other => panic!("expected a mouse event, got {:?}", other),
            })
            .collect();
        assert_eq!(events[0].event_type, MouseEventType::Motion);
        assert_eq!(events[1].event_type, MouseEventType::Press);
        assert!(events[1].left);
        assert_eq!(
            events[1].position,
            zellij_utils::position::Position::new(2, 3)
        );
    }

    #[test]
    fn a_wheel_tick_reaches_the_server() {
        let mut harness = Harness::new(1, true, "");
        harness
            .app
            .on_wheel(MouseScrollDelta::LineDelta(0.0, 1.0), TouchPhase::Moved);
        match &harness.actions()[0] {
            Action::MouseEvent { event } => assert!(event.wheel_up),
            other => panic!("expected a mouse event, got {:?}", other),
        }
    }

    #[test]
    fn a_scale_change_rebuilds_the_cell_metrics() {
        let mut harness = Harness::new(0, true, "");
        assert_eq!(harness.app.metrics.width, 8);
        assert_eq!(harness.app.metrics.height, 20);

        harness.app.rescale(2.0);

        assert_eq!(harness.app.metrics.width, 16);
        assert_eq!(harness.app.metrics.height, 40);
        assert_eq!(harness.app.cache.metrics(), harness.app.metrics);
        assert_eq!(harness.app.scale, 2.0);
    }

    #[test]
    fn an_unusable_scale_leaves_the_previous_one_in_place() {
        let mut harness = Harness::new(0, true, "");
        let before = harness.app.metrics;

        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, 1000.0] {
            harness.app.rescale(scale);
            assert_eq!(harness.app.metrics, before, "scale {}", scale);
            assert_eq!(harness.app.scale, 1.0, "scale {}", scale);
        }
    }

    #[test]
    fn rescaling_to_the_current_scale_changes_nothing() {
        let mut harness = Harness::new(0, true, "");
        harness.app.rescale(2.0);
        let metrics = harness.app.metrics;
        harness.app.rescale(2.0);
        assert_eq!(harness.app.metrics, metrics);
    }

    #[test]
    fn a_reflow_after_a_scale_change_reports_the_scaled_cell_size() {
        let mut harness = Harness::new(2, true, "");
        harness.app.rescale(2.0);
        harness.app.reflow(320, 400);
        assert_eq!(
            harness.sent(),
            vec![
                ClientToServerMsg::TerminalResize {
                    new_size: zellij_utils::pane_size::Size { rows: 10, cols: 20 },
                },
                ClientToServerMsg::TerminalPixelDimensions {
                    pixel_dimensions: zellij_utils::ipc::PixelDimensions {
                        text_area_size: Some(zellij_utils::pane_size::SizeInPixels {
                            width: 320,
                            height: 400,
                        }),
                        character_cell_size: Some(zellij_utils::pane_size::SizeInPixels {
                            width: 16,
                            height: 40,
                        }),
                    },
                },
            ]
        );
    }

    fn styling(background: [u8; 3]) -> zellij_utils::input::theme::Theme {
        let [r, g, b] = background;
        zellij_utils::input::theme::Theme {
            sourced_from_external_file: false,
            terminal_colors: None,
            palette: zellij_utils::data::Styling {
                text_unselected: zellij_utils::data::StyleDeclaration {
                    background: zellij_utils::data::PaletteColor::Rgb((r, g, b)),
                    ..zellij_utils::data::DEFAULT_STYLES.text_unselected
                },
                ..zellij_utils::data::DEFAULT_STYLES
            },
        }
    }

    fn settings(section: WindowConfig) -> Settings {
        Settings {
            section: WindowConfig {
                font_size: section
                    .font_size
                    .or(Some(pixels_to_points(DEFAULT_FONT_SIZE))),
                ..section
            },
            ..Settings::default()
        }
    }

    impl Harness {
        fn reconfigure(&mut self, section: WindowConfig) {
            self.app.reconfigured(settings(section));
        }

        fn push(&mut self, section: WindowConfig) {
            self.app.settings_pushed(settings(section));
        }
    }

    #[test]
    fn a_reload_that_changes_the_font_size_rebuilds_the_cell_and_reports_it() {
        let mut harness = Harness::new(2, true, "");
        assert_eq!(
            (harness.app.metrics.width, harness.app.metrics.height),
            (8, 20)
        );

        harness.reconfigure(WindowConfig {
            font_size: Some(pixels_to_points(DEFAULT_FONT_SIZE * 2.0)),
            ..WindowConfig::default()
        });

        assert_eq!(harness.app.metrics.width, 16);
        assert_eq!(harness.app.metrics.height, 40);
        assert_eq!(
            harness.app.cache.metrics(),
            harness.app.metrics,
            "the glyph cache was not rebuilt with the new font"
        );

        harness.app.reflow(960, 800);
        assert_eq!(
            harness.sent()[0],
            ClientToServerMsg::TerminalResize {
                new_size: zellij_utils::pane_size::Size { rows: 20, cols: 60 },
            },
            "the session was not told the window now holds fewer, larger cells"
        );
    }

    #[test]
    fn a_reload_that_changes_the_colors_re_declares_them_without_touching_the_font() {
        let mut harness = Harness::new(3, true, "");
        let before = harness.app.metrics;

        harness.reconfigure(WindowConfig {
            theme: Some(WindowTheme {
                background: Some(zellij_utils::data::PaletteColor::Rgb((4, 5, 6))),
                ..WindowTheme::default()
            }),
            ..WindowConfig::default()
        });

        assert_eq!(harness.app.options.paints.background, [4, 5, 6]);
        assert_eq!(harness.app.metrics, before);
        let sent = harness.sent();
        assert_eq!(sent.len(), 3, "{:?}", sent);
        assert_eq!(
            sent[1],
            ClientToServerMsg::BackgroundColor {
                color: crate::palette::xparse_color([4, 5, 6])
            }
        );
    }

    #[test]
    fn a_reload_that_changes_the_paste_chords_claims_the_new_one() {
        let mut harness = Harness::new(2, true, "copied");
        harness.reconfigure(WindowConfig {
            paste_keys: Some(vec![zellij_utils::data::KeyWithModifier::new(
                zellij_utils::data::BareKey::F(5),
            )]),
            ..WindowConfig::default()
        });

        harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
        harness.press(&character("V"));
        harness.app.modifiers = ModifiersState::empty();
        harness.press(&Key::Named(NamedKey::F5));

        let sent = harness.sent();
        assert!(
            matches!(sent[0], ClientToServerMsg::Key { .. }),
            "the chord the session now owns was still swallowed: {:?}",
            sent[0]
        );
        assert_eq!(
            sent[1],
            ClientToServerMsg::Action {
                action: Action::Paste {
                    chars: "copied".to_owned(),
                    pane_id: None
                },
                terminal_id: None,
                client_id: None,
                is_cli_client: false,
            }
        );
    }

    #[test]
    fn a_reload_that_changes_nothing_disturbs_nothing() {
        let mut harness = Harness::new(0, true, "");

        let before = harness.app.metrics;
        harness.reconfigure(WindowConfig::default());
        assert_eq!(harness.app.metrics, before);
        assert_eq!(harness.app.options.font.size, DEFAULT_FONT_SIZE);
        assert_eq!(
            harness.app.options.paste_keys,
            crate::options::default_paste_keys()
        );
        assert_eq!(harness.app.options.paints, Paints::default());
    }

    #[test]
    fn a_theme_mode_change_repaints_without_a_configuration_edit() {
        let mut harness = Harness::new(6, true, "");
        harness.app.settings = Settings {
            theme_dark: Some(styling([1, 1, 1])),
            theme_light: Some(styling([2, 2, 2])),
            ..settings(WindowConfig::default())
        };
        harness.app.reapply(false);
        assert_eq!(harness.app.options.paints.background, [1, 1, 1]);

        harness.app.theme_mode = Some(HostTerminalThemeMode::Light);
        harness.app.reapply(false);
        assert_eq!(harness.app.options.paints.background, [2, 2, 2]);

        let sent = harness.sent();
        assert_eq!(sent.len(), 6, "{:?}", sent);
        assert_eq!(
            sent[1],
            ClientToServerMsg::BackgroundColor {
                color: crate::palette::xparse_color([1, 1, 1])
            }
        );
        assert_eq!(
            sent[4],
            ClientToServerMsg::BackgroundColor {
                color: crate::palette::xparse_color([2, 2, 2])
            },
            "the session was not told the colours it answers pane queries with"
        );
    }

    #[test]
    fn a_theme_mode_change_moves_nothing_when_only_one_theme_is_configured() {
        let mut harness = Harness::new(0, true, "");
        harness.app.settings = Settings {
            theme: Some(styling([3, 3, 3])),
            theme_dark: Some(styling([1, 1, 1])),
            ..settings(WindowConfig::default())
        };
        harness.app.reapply(false);
        let before = harness.app.options.paints;

        harness.app.theme_mode = Some(HostTerminalThemeMode::Light);
        harness.app.reapply(false);

        assert_eq!(harness.app.options.paints, before);
        assert_eq!(harness.app.options.paints.background, [3, 3, 3]);
    }

    #[test]
    fn a_reloaded_cursor_leaves_the_font_and_the_session_alone() {
        let mut harness = Harness::new(0, true, "");
        let before = harness.app.metrics;

        harness.reconfigure(WindowConfig {
            cursor_style: Some(zellij_utils::input::window::CursorStyle::Bar),
            cursor_blink: Some(true),
            ..WindowConfig::default()
        });

        assert_eq!(
            harness.app.options.cursor_shape,
            Some(crate::screen_buffer::CursorShape::Beam)
        );
        assert_eq!(harness.app.options.cursor_blink, Some(true));
        assert_eq!(
            harness.app.metrics, before,
            "a cursor change rebuilt the font"
        );
        assert_eq!(
            harness.app.cursor_options(),
            scene::CursorOptions {
                shape: Some(crate::screen_buffer::CursorShape::Beam),
                blink: Some(true),
                hollow: false,
            },
            "the options the scene is built with did not follow the reload"
        );
    }

    #[test]
    fn a_reload_that_changes_the_opacity_reaches_the_scene_on_the_next_frame() {
        use zellij_utils::input::window::OpacityMode;
        let mut harness = Harness::new(0, true, "");
        let before = harness.app.metrics;
        harness.app.refresh_scene();
        let identity = harness.app.retained.identity();
        harness.app.refresh_scene();
        assert!(!harness.app.retained.rebuilt_everything());
        assert_eq!(harness.app.retained.transparency(), Transparency::OPAQUE);

        harness.reconfigure(WindowConfig {
            opacity: Some(0.8),
            ..WindowConfig::default()
        });
        let expected = Transparency {
            opacity: 0.8,
            mode: OpacityMode::Background,
        };
        assert_eq!(harness.app.options.transparency, expected);
        assert_eq!(harness.app.effective_transparency(), expected);
        harness.app.refresh_scene();
        assert_eq!(harness.app.retained.transparency(), expected);
        assert!(
            harness.app.retained.rebuilt_everything(),
            "the scene was not rebuilt for the new opacity"
        );

        harness.reconfigure(WindowConfig {
            opacity: Some(0.8),
            opacity_mode: Some(OpacityMode::Everything),
            ..WindowConfig::default()
        });
        harness.app.refresh_scene();
        assert_eq!(
            harness.app.retained.transparency(),
            Transparency {
                opacity: 0.8,
                mode: OpacityMode::Everything,
            }
        );
        assert!(harness.app.retained.rebuilt_everything());

        harness.reconfigure(WindowConfig {
            opacity: Some(0.8),
            opacity_mode: Some(OpacityMode::Everything),
            blur: Some(true),
            ..WindowConfig::default()
        });
        assert!(harness.app.options.blur);
        harness.app.refresh_scene();
        assert!(
            !harness.app.retained.rebuilt_everything(),
            "a blur change is the compositor's business and must not rebuild the scene"
        );

        assert_eq!(
            harness.app.retained.identity(),
            identity,
            "the scene was replaced rather than updated"
        );
        assert_eq!(
            harness.app.metrics, before,
            "a see-through change rebuilt the font"
        );
    }

    #[test]
    fn a_display_without_transparency_draws_opaque_and_warns_once() {
        let mut harness = Harness::new(0, true, "");
        harness.app.transparency_available = false;
        harness.reconfigure(WindowConfig {
            opacity: Some(0.5),
            ..WindowConfig::default()
        });
        assert_eq!(harness.app.effective_transparency(), Transparency::OPAQUE);
        assert!(harness.app.warned_opaque);
        harness.app.refresh_scene();
        assert_eq!(harness.app.retained.transparency(), Transparency::OPAQUE);

        let mut harness = Harness::new(0, true, "");
        harness.app.transparency_available = false;
        harness.app.warn_if_opaque();
        assert!(
            !harness.app.warned_opaque,
            "a solid window has nothing to warn about"
        );
    }

    #[test]
    fn a_transparent_config_is_preferred_before_the_fewest_samples() {
        assert_eq!(
            best_config([(Some(false), 0), (Some(true), 4), (None, 0)].into_iter()),
            Some(1)
        );
        assert_eq!(
            best_config([(Some(false), 0), (None, 4), (Some(false), 0)].into_iter()),
            Some(1)
        );
        assert_eq!(
            best_config([(Some(true), 4), (Some(true), 0), (Some(true), 0)].into_iter()),
            Some(1),
            "among equals the fewest samples wins and ties keep the first offered"
        );
        assert_eq!(
            best_config([(Some(false), 2), (Some(false), 0)].into_iter()),
            Some(1)
        );
        assert_eq!(best_config(std::iter::empty()), None);
    }

    #[test]
    fn a_reloaded_font_that_cannot_be_built_leaves_the_one_in_place_alone() {
        let mut harness = Harness::new(0, true, "");
        let before = harness.app.metrics;

        harness.reconfigure(WindowConfig {
            font_size: Some(100_000.0),
            ..WindowConfig::default()
        });

        assert_eq!(harness.app.metrics, before);
        assert_eq!(harness.app.cache.metrics(), before);
        assert_eq!(
            harness.app.options.font.size, DEFAULT_FONT_SIZE,
            "an unusable font size must not be left recorded as the one in force"
        );
    }

    #[test]
    fn the_zoom_chords_rebuild_the_font_over_the_resolved_size() {
        let mut harness = Harness::new(0, true, "");
        let plain = harness.app.metrics;
        harness.app.modifiers = ModifiersState::CONTROL;

        harness.press(&character("="));
        assert_eq!(harness.app.zoom, 1.1);
        assert!(
            harness.app.metrics.height > plain.height,
            "zooming in did not grow the cell"
        );
        assert_eq!(
            harness.app.options.font.size, DEFAULT_FONT_SIZE,
            "the zoom must never be written back into the resolved size"
        );

        harness.press(&character("-"));
        assert!((harness.app.zoom - 1.0).abs() < 1e-9);
        assert_eq!(harness.app.metrics, plain);
    }

    #[test]
    fn the_shifted_plus_zooms_in_as_the_unshifted_one_does() {
        let mut harness = Harness::new(0, true, "");
        harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
        harness.press(&character("+"));
        assert_eq!(harness.app.zoom, 1.1);

        let mut harness = Harness::new(0, true, "");
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("+"));
        assert_eq!(harness.app.zoom, 1.1);
    }

    #[test]
    fn the_reset_chord_returns_to_the_resolved_size() {
        let mut harness = Harness::new(0, true, "");
        let plain = harness.app.metrics;
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("="));
        harness.press(&character("="));
        assert!(harness.app.zoom > 1.2);

        harness.press(&character("0"));
        assert_eq!(harness.app.zoom, 1.0);
        assert_eq!(harness.app.metrics, plain);
    }

    #[test]
    fn a_zoom_chord_reaches_the_pane_when_it_is_unbound() {
        let mut harness = Harness::new(1, true, "");
        harness.reconfigure(WindowConfig {
            zoom_in_keys: Some(Vec::new()),
            ..WindowConfig::default()
        });
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("="));
        assert_eq!(harness.app.zoom, 1.0);
        assert!(
            matches!(harness.sent()[0], ClientToServerMsg::Key { .. }),
            "an unbound chord must be the escape hatch it is advertised as"
        );
    }

    #[test]
    fn any_configuration_reload_resets_the_zoom() {
        let mut harness = Harness::new(0, true, "");
        let plain = harness.app.metrics;
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("="));
        assert!(harness.app.metrics.height > plain.height);

        harness.reconfigure(WindowConfig::default());

        assert_eq!(harness.app.zoom, 1.0);
        assert_eq!(
            harness.app.metrics, plain,
            "a reload that changed nothing still has to take the zoom back"
        );
    }

    #[test]
    fn a_pushed_change_that_leaves_the_font_size_alone_keeps_the_zoom() {
        let mut harness = Harness::new(0, true, "");
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("="));
        let zoomed = harness.app.metrics;

        harness.push(WindowConfig {
            blur: Some(true),
            ..WindowConfig::default()
        });

        assert_eq!(harness.app.zoom, 1.1);
        assert_eq!(harness.app.metrics, zoomed);
        assert!(
            harness.app.options.blur,
            "the pushed change was not applied"
        );
    }

    #[test]
    fn a_pushed_font_size_resets_the_zoom() {
        let mut harness = Harness::new(0, true, "");
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("="));

        harness.push(WindowConfig {
            font_size: Some(pixels_to_points(DEFAULT_FONT_SIZE * 2.0)),
            ..WindowConfig::default()
        });

        assert_eq!(harness.app.zoom, 1.0);
        assert_eq!(harness.app.metrics.height, 40);
        assert_eq!(harness.app.options.font.size, DEFAULT_FONT_SIZE * 2.0);
    }

    #[test]
    fn a_reload_that_changes_the_size_resolves_it_without_the_zoom() {
        let mut harness = Harness::new(0, true, "");
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("="));

        harness.reconfigure(WindowConfig {
            font_size: Some(pixels_to_points(DEFAULT_FONT_SIZE * 2.0)),
            ..WindowConfig::default()
        });

        assert_eq!(harness.app.zoom, 1.0);
        assert_eq!(harness.app.metrics.height, 40);
        assert_eq!(harness.app.options.font.size, DEFAULT_FONT_SIZE * 2.0);
    }

    #[test]
    fn a_zoom_the_font_cannot_be_built_at_is_refused_and_forgotten() {
        let mut harness = Harness::new(0, true, "");
        harness.app.modifiers = ModifiersState::CONTROL;
        for _ in 0..80 {
            harness.press(&character("="));
        }
        let held = harness.app.zoom;
        assert!(
            held * DEFAULT_FONT_SIZE as f64 <= 512.0,
            "the zoom ran past the size the font stack refuses"
        );
        assert_eq!(
            harness.app.cache.metrics(),
            harness.app.metrics,
            "a refused zoom left the cache and the metrics disagreeing"
        );
        harness.press(&character("-"));
        assert!(harness.app.zoom < held);
    }

    #[test]
    fn the_zoom_rides_on_top_of_the_display_scale() {
        let mut harness = Harness::new(0, true, "");
        harness.app.rescale(2.0);
        let scaled = harness.app.metrics;
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("="));
        assert_eq!(harness.app.scale, 2.0);
        assert_eq!(harness.app.zoom, 1.1);
        assert!(harness.app.metrics.height > scaled.height);

        harness.press(&character("0"));
        assert_eq!(harness.app.metrics, scaled);
        assert_eq!(harness.app.scale, 2.0);
    }

    fn with_a_link_under_the_pointer(harness: &mut Harness, uri: &str) {
        crate::screen_buffer::painter::Painter::apply(&mut harness.app.state, |painter| {
            painter.links(&[(1, uri)]);
            painter.linked(0, 0, "link", 1);
        });
        harness.app.on_cursor_moved(PhysicalPosition::new(4.0, 4.0));
    }

    fn left_click(harness: &mut Harness) {
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Released);
    }

    fn opened_urls() -> Vec<String> {
        links::opened().into_iter().map(|(_, uri)| uri).collect()
    }

    #[test]
    fn a_click_on_a_link_is_still_forwarded_to_the_pane() {
        links::forget_opened();
        let mut harness = Harness::new(3, true, "");
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        assert_eq!(
            harness
                .app
                .hovered_link
                .as_ref()
                .map(|run| run.uri.as_str()),
            Some("https://example.com"),
            "the pointer should be over the link"
        );
        left_click(&mut harness);

        let events: Vec<_> = harness
            .actions()
            .into_iter()
            .map(|action| match action {
                Action::MouseEvent { event } => event,
                other => panic!("expected a mouse event, got {:?}", other),
            })
            .collect();
        assert_eq!(
            events.len(),
            3,
            "opening a link must not cost the pane its focus or its selection"
        );
        assert_eq!(events[1].event_type, MouseEventType::Press);
        assert!(events[1].left);
        assert_eq!(events[2].event_type, MouseEventType::Release);
        assert_eq!(opened_urls(), vec!["https://example.com".to_owned()]);
    }

    #[test]
    fn a_press_over_a_link_arms_the_open_and_a_release_on_it_fires() {
        links::forget_opened();
        let mut harness = Harness::new(3, true, "");
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        assert_eq!(
            harness
                .app
                .armed_link
                .as_ref()
                .map(|armed| armed.uri.as_str()),
            Some("https://example.com")
        );
        assert!(opened_urls().is_empty(), "a press alone opens nothing");
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Released);
        assert!(
            harness.app.armed_link.is_none(),
            "the release must spend the armed open"
        );
        assert_eq!(opened_urls(), vec!["https://example.com".to_owned()]);
    }

    #[test]
    fn a_press_on_a_link_dragged_off_it_opens_nothing() {
        links::forget_opened();
        let mut harness = Harness::new(4, true, "");
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        assert!(harness.app.armed_link.is_some());
        harness
            .app
            .on_cursor_moved(PhysicalPosition::new(455.0, 4.0));
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Released);
        assert!(
            harness.app.armed_link.is_none(),
            "a drag is a selection, and it must leave nothing armed"
        );
        assert!(opened_urls().is_empty());
    }

    #[test]
    fn a_press_over_a_scheme_the_allow_list_refuses_arms_nothing() {
        links::forget_opened();
        let mut harness = Harness::new(3, true, "");
        with_a_link_under_the_pointer(&mut harness, "javascript:alert(1)");
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        assert!(harness.app.armed_link.is_none());
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Released);
        assert!(opened_urls().is_empty());
        for action in harness.actions() {
            assert!(matches!(action, Action::MouseEvent { .. }), "{:?}", action);
        }
    }

    #[test]
    fn a_press_away_from_any_link_arms_nothing() {
        links::forget_opened();
        let mut harness = Harness::new(3, true, "");
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        harness
            .app
            .on_cursor_moved(PhysicalPosition::new(455.0, 4.0));
        assert!(harness.app.hovered_link.is_none());
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        assert!(harness.app.armed_link.is_none());
    }

    #[test]
    fn opening_links_switched_off_arms_nothing() {
        links::forget_opened();
        let mut harness = Harness::new(3, true, "");
        harness.reconfigure(WindowConfig {
            open_links: Some(false),
            ..WindowConfig::default()
        });
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        assert!(harness.app.armed_link.is_none());
    }

    #[test]
    fn the_pointer_becomes_a_hand_over_a_link_and_a_beam_beside_it() {
        let mut harness = Harness::new(2, true, "");
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        assert_eq!(harness.app.pointer_shape(), CursorIcon::Pointer);
        harness
            .app
            .on_cursor_moved(PhysicalPosition::new(455.0, 4.0));
        assert_eq!(harness.app.pointer_shape(), CursorIcon::Text);
    }

    #[test]
    fn a_link_this_client_will_not_open_never_shows_the_hand() {
        let mut harness = Harness::new(1, true, "");
        with_a_link_under_the_pointer(&mut harness, "javascript:alert(1)");
        assert!(harness.app.hovered_link.is_some(), "it is still a link");
        assert_eq!(
            harness.app.pointer_shape(),
            CursorIcon::Text,
            "the hand would promise an open the allow-list refuses"
        );
    }

    #[test]
    fn switching_opening_off_takes_the_hand_away() {
        let mut harness = Harness::new(1, true, "");
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        assert_eq!(harness.app.pointer_shape(), CursorIcon::Pointer);
        harness.reconfigure(WindowConfig {
            open_links: Some(false),
            ..WindowConfig::default()
        });
        assert_eq!(harness.app.pointer_shape(), CursorIcon::Text);
    }

    #[test]
    fn the_hover_run_follows_the_pointer_across_a_link_boundary() {
        let mut harness = Harness::new(2, true, "");
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        assert_eq!(harness.app.hovered_link.as_ref().map(|run| run.id), Some(1));
        harness
            .app
            .on_cursor_moved(PhysicalPosition::new(45.0, 4.0));
        assert!(
            harness.app.hovered_link.is_none(),
            "leaving the link must drop the hover"
        );
    }

    fn shift_click(harness: &mut Harness) {
        harness.app.modifiers = ModifiersState::SHIFT;
        left_click(harness);
        harness.app.modifiers = ModifiersState::empty();
    }

    #[test]
    fn a_shift_click_on_a_link_opens_it_in_a_pane_that_is_not_watching_the_mouse() {
        links::forget_opened();
        let mut harness = Harness::new(1, true, "");
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        shift_click(&mut harness);
        assert_eq!(opened_urls(), vec!["https://example.com".to_owned()]);
        assert_eq!(harness.app.selection, None, "a click selects nothing");
        let actions = harness.actions();
        assert_eq!(
            actions.len(),
            1,
            "only the pointer motion reaches the session: {:?}",
            actions
        );
    }

    #[test]
    fn a_shift_click_on_a_link_opens_nothing_once_shift_opening_is_off() {
        links::forget_opened();
        let mut harness = Harness::new(1, true, "");
        harness.reconfigure(WindowConfig {
            open_links_with_shift: Some(false),
            ..WindowConfig::default()
        });
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        harness.app.modifiers = ModifiersState::SHIFT;
        assert_eq!(
            harness.app.pointer_shape(),
            CursorIcon::Text,
            "no hand while shift cannot open the link"
        );
        shift_click(&mut harness);
        assert!(opened_urls().is_empty());
    }

    #[test]
    fn a_shift_click_on_a_link_opens_it_and_reaches_the_session_when_selecting_is_off() {
        links::forget_opened();
        let mut harness = Harness::new(3, true, "");
        harness.reconfigure(WindowConfig {
            shift_drag_selects: Some(false),
            ..WindowConfig::default()
        });
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        shift_click(&mut harness);
        assert_eq!(opened_urls(), vec!["https://example.com".to_owned()]);
        let events: Vec<_> = harness
            .actions()
            .into_iter()
            .map(|action| match action {
                Action::MouseEvent { event } => event,
                other => panic!("expected a mouse event, got {:?}", other),
            })
            .collect();
        assert_eq!(events[1].event_type, MouseEventType::Press);
        assert!(events[1].shift);
        assert_eq!(events[2].event_type, MouseEventType::Release);
    }

    fn over_cell(harness: &Harness, row: usize, col: usize) -> PhysicalPosition<f64> {
        let geometry = harness.app.geometry.get();
        let (width, height) = (geometry.cell_width as f64, geometry.cell_height as f64);
        PhysicalPosition::new(
            col as f64 * width + width / 2.0,
            row as f64 * height + height / 2.0,
        )
    }

    fn with_an_interface_on_screen(harness: &mut Harness) {
        crate::screen_buffer::painter::Painter::apply(&mut harness.app.state, |painter| {
            painter.text(0, 0, "Tab #1  Tab #2");
            painter.text(1, 0, "│ hello │ world");
            painter.text(2, 0, "status bar");
        });
    }

    fn shift_drag(harness: &mut Harness, from: (usize, usize), to: (usize, usize)) {
        let start = over_cell(harness, from.0, from.1);
        harness.app.on_cursor_moved(start);
        harness.app.modifiers = ModifiersState::SHIFT;
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        let end = over_cell(harness, to.0, to.1);
        harness.app.on_cursor_moved(end);
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Released);
        harness.app.modifiers = ModifiersState::empty();
    }

    fn copy_key(harness: &mut Harness) {
        harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
        harness.press(&character("C"));
        harness.app.modifiers = ModifiersState::empty();
    }

    #[test]
    fn a_shift_drag_selects_across_the_whole_window_and_the_copy_key_copies_it() {
        let mut harness = Harness::new(1, true, "");
        with_an_interface_on_screen(&mut harness);
        shift_drag(&mut harness, (0, 4), (2, 5));
        assert_eq!(
            harness.app.selection,
            Some(Selection::at(0, 4).extended_to(2, 5))
        );
        assert_eq!(
            harness.clipboard_text(),
            None,
            "nothing is copied before the copy key"
        );

        copy_key(&mut harness);
        let expected = "#1  Tab #2\n│ hello │ world\nstatus";
        assert_eq!(harness.clipboard_text().as_deref(), Some(expected));
        assert_eq!(
            harness.clipboard.lock().unwrap().get_primary().as_deref(),
            Some(expected)
        );
        assert_eq!(harness.app.selection, None, "copying clears the highlight");
        assert_eq!(
            harness.actions().len(),
            1,
            "only the first pointer motion reaches the session"
        );
    }

    #[test]
    fn the_copy_key_reaches_the_session_when_nothing_is_selected() {
        let mut harness = Harness::new(1, true, "");
        copy_key(&mut harness);
        assert_eq!(harness.clipboard_text(), None);
        assert_eq!(harness.sent().len(), 1);
    }

    #[test]
    fn a_plain_click_clears_the_selection_and_reaches_the_session() {
        let mut harness = Harness::new(3, true, "");
        with_an_interface_on_screen(&mut harness);
        shift_drag(&mut harness, (0, 0), (1, 3));
        assert!(harness.app.selection.is_some());
        left_click(&mut harness);
        assert_eq!(harness.app.selection, None);
        assert_eq!(harness.actions().len(), 3);
    }

    #[test]
    fn typing_clears_the_selection() {
        let mut harness = Harness::new(2, true, "");
        with_an_interface_on_screen(&mut harness);
        shift_drag(&mut harness, (0, 0), (1, 3));
        harness.press(&character("a"));
        assert_eq!(harness.app.selection, None);
        copy_key(&mut harness);
        assert_eq!(harness.clipboard_text(), None);
    }

    #[test]
    fn a_frame_drawn_under_the_selection_leaves_it_in_place() {
        let mut harness = Harness::new(1, true, "");
        with_an_interface_on_screen(&mut harness);
        shift_drag(&mut harness, (2, 0), (2, 5));
        crate::screen_buffer::painter::Painter::apply(&mut harness.app.state, |painter| {
            painter.text(2, 0, "change");
        });
        copy_key(&mut harness);
        assert_eq!(harness.clipboard_text().as_deref(), Some("change"));
    }

    #[test]
    fn a_shift_drag_reaches_the_session_when_selecting_is_off() {
        let mut harness = Harness::new(4, true, "");
        harness.reconfigure(WindowConfig {
            shift_drag_selects: Some(false),
            ..WindowConfig::default()
        });
        with_an_interface_on_screen(&mut harness);
        shift_drag(&mut harness, (0, 0), (1, 3));
        assert_eq!(harness.app.selection, None);
        let events: Vec<_> = harness
            .actions()
            .into_iter()
            .map(|action| match action {
                Action::MouseEvent { event } => event,
                other => panic!("expected a mouse event, got {:?}", other),
            })
            .collect();
        assert_eq!(events.len(), 4);
        assert_eq!(events[1].event_type, MouseEventType::Press);
        assert!(events[1].shift);
        assert_eq!(events[3].event_type, MouseEventType::Release);
    }

    #[test]
    fn switching_selecting_off_drops_the_selection() {
        let mut harness = Harness::new(1, true, "");
        with_an_interface_on_screen(&mut harness);
        shift_drag(&mut harness, (0, 0), (1, 3));
        harness.reconfigure(WindowConfig {
            shift_drag_selects: Some(false),
            ..WindowConfig::default()
        });
        assert_eq!(harness.app.selection, None);
    }

    fn with_a_selection(harness: &mut Harness) {
        with_an_interface_on_screen(harness);
        shift_drag(harness, (0, 0), (1, 3));
        assert!(harness.app.selection.is_some());
    }

    #[test]
    fn scrolling_clears_the_selection() {
        let mut harness = Harness::new(0, true, "");
        with_a_selection(&mut harness);
        harness
            .app
            .on_wheel(MouseScrollDelta::LineDelta(0.0, 1.0), TouchPhase::Moved);
        assert_eq!(harness.app.selection, None);
    }

    #[test]
    fn resizing_clears_the_selection() {
        let mut harness = Harness::new(0, true, "");
        with_a_selection(&mut harness);
        harness.app.resized(640, 480);
        assert_eq!(harness.app.selection, None);
    }

    #[test]
    fn zooming_clears_the_selection() {
        let mut harness = Harness::new(0, true, "");
        with_a_selection(&mut harness);
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("="));
        assert_eq!(harness.app.selection, None);
    }

    #[test]
    fn a_scale_change_clears_the_selection() {
        let mut harness = Harness::new(0, true, "");
        with_a_selection(&mut harness);
        harness.app.rescale(2.0);
        assert_eq!(harness.app.selection, None);
    }

    #[test]
    fn switching_sessions_clears_the_selection() {
        let mut harness = Harness::new(0, true, "");
        with_a_selection(&mut harness);
        harness.app.start_over(geometry());
        assert_eq!(harness.app.selection, None);
        assert!(!harness.app.selecting);
    }

    #[test]
    fn pasting_clears_the_selection() {
        let mut harness = Harness::new(0, true, "pasted");
        with_a_selection(&mut harness);
        harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
        harness.press(&character("V"));
        assert_eq!(harness.app.selection, None);
    }

    #[test]
    fn text_from_the_input_method_clears_the_selection() {
        let mut harness = Harness::new(0, true, "");
        with_a_selection(&mut harness);
        harness.app.on_ime(Ime::Preedit(String::new(), None));
        assert!(
            harness.app.selection.is_some(),
            "a composition in progress types nothing yet"
        );
        harness.app.on_ime(Ime::Commit("x".to_owned()));
        assert_eq!(harness.app.selection, None);
    }

    #[test]
    fn losing_focus_mid_drag_keeps_what_was_marked_and_stops_extending_it() {
        let mut harness = Harness::new(0, true, "");
        with_an_interface_on_screen(&mut harness);
        let start = over_cell(&harness, 0, 0);
        harness.app.on_cursor_moved(start);
        harness.app.modifiers = ModifiersState::SHIFT;
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        let middle = over_cell(&harness, 1, 3);
        harness.app.on_cursor_moved(middle);
        harness.app.on_focus_lost();
        harness.app.modifiers = ModifiersState::empty();
        let marked = Some(Selection::at(0, 0).extended_to(1, 3));
        assert_eq!(harness.app.selection, marked);
        assert!(!harness.app.selecting);

        let later = over_cell(&harness, 2, 5);
        harness.app.on_cursor_moved(later);
        assert_eq!(
            harness.app.selection, marked,
            "the drag ended with the focus"
        );
        copy_key(&mut harness);
        assert_eq!(
            harness.clipboard_text().as_deref(),
            Some("Tab #1  Tab #2\n│ he")
        );
    }

    #[test]
    fn losing_focus_before_the_drag_moved_leaves_nothing_marked() {
        let mut harness = Harness::new(0, true, "");
        with_an_interface_on_screen(&mut harness);
        let start = over_cell(&harness, 0, 0);
        harness.app.on_cursor_moved(start);
        harness.app.modifiers = ModifiersState::SHIFT;
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        harness.app.on_focus_lost();
        assert_eq!(harness.app.selection, None);
        assert!(!harness.app.selecting);
    }

    fn with_a_link_in_a_pane_watching_the_mouse(harness: &mut Harness) {
        use zellij_utils::structured_render::{
            GeometryRecord, PaneRect, PANE_FOCUSED, PANE_SELECTABLE, PANE_WANTS_MOUSE,
        };
        crate::screen_buffer::painter::Painter::apply(&mut harness.app.state, |painter| {
            painter.geometry(&GeometryRecord {
                panes: vec![PaneRect {
                    x: 0,
                    y: 0,
                    cols: 120,
                    rows: 40,
                    top: 0,
                    bottom: 0,
                    left: 0,
                    right: 0,
                    flags: PANE_SELECTABLE | PANE_FOCUSED | PANE_WANTS_MOUSE,
                }],
            });
            painter.links(&[(1, "https://example.com")]);
            painter.linked(0, 0, "link", 1);
        });
        harness.app.on_cursor_moved(PhysicalPosition::new(4.0, 4.0));
        assert!(harness.app.pane_wants_mouse());
    }

    #[test]
    fn a_shift_click_on_a_link_opens_it_in_a_pane_that_is_watching_the_mouse() {
        links::forget_opened();
        let mut harness = Harness::new(1, true, "");
        with_a_link_in_a_pane_watching_the_mouse(&mut harness);
        shift_click(&mut harness);
        assert_eq!(opened_urls(), vec!["https://example.com".to_owned()]);
        assert_eq!(harness.app.selection, None);
        assert_eq!(
            harness.actions().len(),
            1,
            "only the pointer motion reaches the session"
        );
    }

    #[test]
    fn a_plain_click_on_a_link_in_a_pane_watching_the_mouse_goes_to_the_pane_only() {
        links::forget_opened();
        let mut harness = Harness::new(3, true, "");
        with_a_link_in_a_pane_watching_the_mouse(&mut harness);
        left_click(&mut harness);
        assert!(opened_urls().is_empty());
        assert_eq!(harness.actions().len(), 3);
    }

    #[test]
    fn a_shift_drag_that_starts_on_a_link_selects_and_opens_nothing() {
        links::forget_opened();
        let mut harness = Harness::new(1, true, "");
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        harness.app.modifiers = ModifiersState::SHIFT;
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        assert!(
            harness.app.armed_link.is_some(),
            "until it moves, the press may still be a click"
        );
        let away = over_cell(&harness, 0, 9);
        harness.app.on_cursor_moved(away);
        assert!(harness.app.armed_link.is_none());
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Released);
        harness.app.modifiers = ModifiersState::empty();
        assert!(opened_urls().is_empty());
        assert_eq!(
            harness.app.selection,
            Some(Selection::at(0, 0).extended_to(0, 9))
        );
        copy_key(&mut harness);
        assert_eq!(harness.clipboard_text().as_deref(), Some("link"));
        assert_eq!(harness.actions().len(), 1);
    }

    #[test]
    fn a_shift_drag_off_a_link_and_back_onto_it_opens_nothing() {
        links::forget_opened();
        let mut harness = Harness::new(0, true, "");
        with_a_link_under_the_pointer(&mut harness, "https://example.com");
        harness.app.modifiers = ModifiersState::SHIFT;
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        let away = over_cell(&harness, 0, 9);
        harness.app.on_cursor_moved(away);
        harness.app.on_cursor_moved(PhysicalPosition::new(4.0, 4.0));
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Released);
        harness.app.modifiers = ModifiersState::empty();
        assert!(opened_urls().is_empty());
        assert_eq!(harness.app.selection, None);
    }

    #[test]
    fn a_middle_click_pastes_the_primary_selection_instead_of_being_forwarded() {
        let mut harness = Harness::new(1, true, "");
        harness.set_primary("selected");
        harness
            .app
            .on_mouse_input(MouseButton::Middle, ElementState::Pressed);
        harness
            .app
            .on_mouse_input(MouseButton::Middle, ElementState::Released);
        assert_eq!(
            harness.actions(),
            vec![Action::Paste {
                chars: "selected".to_owned(),
                pane_id: None
            }],
            "the release of a swallowed press must be swallowed with it"
        );
    }

    #[test]
    fn a_middle_click_with_nothing_selected_reaches_the_session_as_before() {
        let mut harness = Harness::new(2, true, "");
        harness
            .app
            .on_mouse_input(MouseButton::Middle, ElementState::Pressed);
        harness
            .app
            .on_mouse_input(MouseButton::Middle, ElementState::Released);
        let events: Vec<_> = harness
            .actions()
            .into_iter()
            .map(|action| match action {
                Action::MouseEvent { event } => event,
                other => panic!("expected a mouse event, got {:?}", other),
            })
            .collect();
        assert_eq!(events[0].event_type, MouseEventType::Press);
        assert!(events[0].middle);
        assert_eq!(events[1].event_type, MouseEventType::Release);
    }

    #[test]
    fn a_middle_click_reaches_the_session_when_the_option_is_off() {
        let mut harness = Harness::new(2, true, "");
        harness.set_primary("selected");
        harness.reconfigure(WindowConfig {
            middle_click_paste: Some(false),
            ..WindowConfig::default()
        });
        harness
            .app
            .on_mouse_input(MouseButton::Middle, ElementState::Pressed);
        harness
            .app
            .on_mouse_input(MouseButton::Middle, ElementState::Released);
        for action in harness.actions() {
            assert!(
                matches!(action, Action::MouseEvent { .. }),
                "a middle click was still swallowed: {:?}",
                action
            );
        }
    }

    #[test]
    fn a_swallowed_middle_click_leaves_no_button_held() {
        let mut harness = Harness::new(1, true, "");
        harness.set_primary("selected");
        harness
            .app
            .on_mouse_input(MouseButton::Middle, ElementState::Pressed);
        harness.app.on_focus_lost();
        assert_eq!(
            harness.actions().len(),
            1,
            "losing focus released a button the session was never told was pressed"
        );
    }

    #[test]
    fn the_left_button_is_untouched_by_the_middle_click_path() {
        let mut harness = Harness::new(1, true, "");
        harness.set_primary("selected");
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        match &harness.actions()[0] {
            Action::MouseEvent { event } => assert!(event.left),
            other => panic!("expected a mouse event, got {:?}", other),
        }
    }

    #[test]
    fn the_bell_mode_decides_what_a_bell_does() {
        use zellij_utils::input::window::BellMode;
        for (mode, attends, rings) in [
            (BellMode::Visual, true, false),
            (BellMode::Audible, false, true),
            (BellMode::Both, true, true),
            (BellMode::None, false, false),
        ] {
            let mut harness = Harness::new(1, true, "");
            harness.reconfigure(WindowConfig {
                bell: Some(mode),
                ..WindowConfig::default()
            });
            assert_eq!(harness.app.options.bell, mode);
            assert_eq!(harness.app.options.bell.attends(), attends, "{:?}", mode);
            assert_eq!(harness.app.options.bell.rings(), rings, "{:?}", mode);

            let rang_before = RINGS_COUNTED.load(Ordering::Relaxed);
            harness.app.ring = count_a_ring;
            let size = harness.app.state.size();
            let frame = frame_with_sideband(size.rows, size.cols, 0, b"\x07");
            harness.app.on_frame(&frame);
            assert_eq!(
                harness.app.state.take_bells(),
                0,
                "{:?} left the bell undrained",
                mode
            );
            assert_eq!(
                RINGS_COUNTED.load(Ordering::Relaxed) - rang_before,
                usize::from(rings),
                "{:?} rang the host's bell the wrong number of times",
                mode
            );
        }
    }

    #[test]
    fn losing_focus_releases_a_held_button() {
        let mut harness = Harness::new(2, true, "");
        harness
            .app
            .on_mouse_input(MouseButton::Left, ElementState::Pressed);
        harness.app.on_focus_lost();
        match &harness.actions()[1] {
            Action::MouseEvent { event } => {
                assert_eq!(event.event_type, MouseEventType::Release);
                assert!(event.left);
            },
            other => panic!("expected a mouse event, got {:?}", other),
        }
    }

    mod composing {
        use super::*;
        use zellij_utils::data::{BareKey, KeyWithModifier};

        fn preedit(text: &str) -> Ime {
            let end = text.len();
            Ime::Preedit(text.to_owned(), Some((end, end)))
        }

        fn dead(accent: char) -> Key {
            Key::Dead(Some(accent))
        }

        fn a_whole_composition(harness: &mut Harness) {
            for event in [
                Ime::Enabled,
                preedit("ni"),
                preedit("ni hao"),
                Ime::Commit("你好".to_owned()),
                Ime::Disabled,
            ] {
                harness.app.on_ime(event);
            }
        }

        #[test]
        fn a_composition_reaches_the_pane_as_its_committed_text_and_nothing_else() {
            let mut harness = Harness::new(1, true, "");
            a_whole_composition(&mut harness);
            assert_eq!(
                harness.actions(),
                vec![Action::WriteChars {
                    chars: "你好".to_owned()
                }],
                "a pane is told the committed text once, and no preedit ever"
            );
        }

        #[test]
        fn a_commit_is_typed_rather_than_pasted() {
            let mut harness = Harness::new(1, true, "");
            a_whole_composition(&mut harness);
            match &harness.sent()[0] {
                ClientToServerMsg::Action {
                    action: Action::WriteChars { chars },
                    ..
                } => {
                    assert_eq!(chars, "你好");
                    assert!(
                        !chars.contains('\u{1b}'),
                        "typed text must carry no bracketed-paste markers"
                    );
                },
                other => panic!("expected typed text, got {:?}", other),
            }
        }

        #[test]
        fn a_single_character_commit_is_a_key_press_so_keybindings_see_it() {
            let mut harness = Harness::new(1, true, "");
            harness.app.on_ime(Ime::Preedit(String::new(), None));
            harness.app.on_ime(Ime::Commit("t".to_owned()));
            assert_eq!(
                harness.sent(),
                vec![ClientToServerMsg::Key {
                    key: KeyWithModifier::new(BareKey::Char('t')),
                    raw_bytes: b"t".to_vec(),
                    is_kitty_keyboard_protocol: false,
                }],
                "an input method that commits ordinary letters must not bypass the session's modes"
            );
        }

        #[test]
        fn a_multi_character_key_is_typed_rather_than_pasted_too() {
            let mut harness = Harness::new(1, true, "");
            harness.press(&character("ab"));
            assert_eq!(
                harness.actions(),
                vec![Action::WriteChars {
                    chars: "ab".to_owned()
                }],
                "a compose result the layout finished is typed text, like a commit"
            );
        }

        #[test]
        fn a_preedit_never_reaches_the_pane() {
            let mut harness = Harness::new(0, true, "");
            harness.app.on_ime(Ime::Enabled);
            harness.app.on_ime(preedit("ni"));
            harness.app.on_ime(preedit("ni hao"));
            assert!(
                harness.sent().is_empty(),
                "a half-composed syllable must not reach the shell"
            );
        }

        #[test]
        fn a_chord_pressed_mid_composition_neither_zooms_nor_reaches_the_pane() {
            let mut harness = Harness::new(0, true, "");
            harness.app.on_ime(preedit("ni"));
            harness.app.modifiers = ModifiersState::CONTROL;
            harness.press(&character("="));
            assert_eq!(harness.app.zoom, 1.0, "a composing key zoomed the window");
            assert!(harness.sent().is_empty());
        }

        #[test]
        fn a_dead_key_and_the_character_it_composed_reach_the_pane_once() {
            let mut harness = Harness::new(1, true, "");
            harness.press(&dead('`'));
            harness.press(&character("à"));
            let sent = harness.sent();
            assert_eq!(sent.len(), 1, "got {:?}", sent);
            match &sent[0] {
                ClientToServerMsg::Key { key, raw_bytes, .. } => {
                    assert_eq!(*key, KeyWithModifier::new(BareKey::Char('à')));
                    assert_eq!(raw_bytes, "à".as_bytes());
                },
                other => panic!("expected a key, got {:?}", other),
            }
        }

        #[test]
        fn a_chord_is_suspended_for_the_key_that_finishes_a_dead_key() {
            let mut harness = Harness::new(1, true, "");
            harness.press(&dead('`'));
            harness.app.modifiers = ModifiersState::CONTROL;
            harness.press(&character("="));
            assert_eq!(
                harness.app.zoom, 1.0,
                "the second half of a composition is not a chord"
            );
            assert_eq!(harness.sent().len(), 1);
        }

        #[test]
        fn a_dead_key_on_its_own_sends_nothing_and_shows_the_accent() {
            let mut harness = Harness::new(0, true, "");
            harness.press(&dead('`'));
            assert_eq!(
                harness
                    .app
                    .composition
                    .shown()
                    .map(|shown| shown.text().to_owned()),
                Some("`".to_owned())
            );
            assert!(harness.sent().is_empty());
        }

        fn press_with_text(harness: &mut Harness, logical: Key, text: &str) {
            let modifiers = harness.app.modifiers;
            harness
                .app
                .press(input::Press::new(logical, modifiers).with_text(text));
        }

        fn typed_key(character: char) -> ClientToServerMsg {
            ClientToServerMsg::Key {
                key: KeyWithModifier::new(BareKey::Char(character)),
                raw_bytes: character.to_string().into_bytes(),
                is_kitty_keyboard_protocol: false,
            }
        }

        fn typed_text(chars: &str) -> ClientToServerMsg {
            ClientToServerMsg::Action {
                action: Action::WriteChars {
                    chars: chars.to_owned(),
                },
                terminal_id: None,
                client_id: None,
                is_cli_client: false,
            }
        }

        #[test]
        fn space_after_a_dead_key_sends_the_accent_and_clears_it() {
            let mut harness = Harness::new(1, true, "");
            harness.press(&dead('^'));
            press_with_text(&mut harness, Key::Named(NamedKey::Space), "^");
            assert_eq!(harness.app.composition.shown(), None);
            assert_eq!(harness.app.composition.gate(), Gate::Open);
            assert_eq!(harness.sent(), vec![typed_key('^')]);
        }

        #[test]
        fn a_dead_key_pressed_twice_sends_what_the_system_produced() {
            let mut harness = Harness::new(1, true, "");
            harness.press(&dead('^'));
            press_with_text(&mut harness, dead('^'), "^^");
            assert_eq!(harness.app.composition.shown(), None);
            assert_eq!(harness.sent(), vec![typed_text("^^")]);
        }

        #[test]
        fn an_accent_that_cannot_combine_reaches_the_pane_with_the_letter() {
            let mut harness = Harness::new(1, true, "");
            harness.press(&dead('^'));
            press_with_text(&mut harness, character("x"), "^x");
            assert_eq!(harness.sent(), vec![typed_text("^x")]);
        }

        #[test]
        fn enter_after_a_dead_key_sends_the_accent_then_enter() {
            let mut harness = Harness::new(2, true, "");
            harness.press(&dead('^'));
            press_with_text(&mut harness, Key::Named(NamedKey::Enter), "^\r");
            let sent = harness.sent();
            assert_eq!(sent[0], typed_key('^'));
            match &sent[1] {
                ClientToServerMsg::Key { key, raw_bytes, .. } => {
                    assert_eq!(*key, KeyWithModifier::new(BareKey::Enter));
                    assert_eq!(raw_bytes, b"\r");
                },
                other => panic!("expected enter, got {:?}", other),
            }
        }

        #[test]
        fn a_cancelled_accent_with_nothing_produced_sends_nothing() {
            let mut harness = Harness::new(0, true, "");
            harness.press(&dead('^'));
            harness.press(&Key::Unidentified(winit::keyboard::NativeKey::Unidentified));
            assert_eq!(harness.app.composition.shown(), None);
            assert_eq!(harness.app.composition.gate(), Gate::Open);
            assert!(harness.sent().is_empty());
        }

        #[test]
        fn a_shortcut_after_a_dead_key_still_reaches_the_pane_as_a_shortcut() {
            let mut harness = Harness::new(1, true, "");
            harness.press(&dead('^'));
            harness.app.modifiers = ModifiersState::CONTROL;
            press_with_text(&mut harness, character("c"), "^c");
            match &harness.sent()[0] {
                ClientToServerMsg::Key { raw_bytes, .. } => assert_eq!(raw_bytes, &vec![3]),
                other => panic!("expected ctrl-c, got {:?}", other),
            }
        }

        #[test]
        fn a_character_picker_entry_reaches_the_pane() {
            let mut harness = Harness::new(1, true, "");
            press_with_text(
                &mut harness,
                Key::Unidentified(winit::keyboard::NativeKey::Unidentified),
                "′",
            );
            assert_eq!(harness.sent(), vec![typed_key('′')]);
        }

        #[test]
        fn typing_resumes_normally_after_an_accent_is_resolved() {
            let mut harness = Harness::new(2, true, "");
            harness.press(&dead('^'));
            press_with_text(&mut harness, Key::Named(NamedKey::Space), "^");
            harness.press(&character("a"));
            assert_eq!(harness.sent(), vec![typed_key('^'), typed_key('a')]);
        }

        #[test]
        fn losing_focus_mid_composition_leaves_no_residue() {
            let mut harness = Harness::new(0, true, "");
            harness.app.on_ime(preedit("ni"));
            harness.press(&dead('`'));
            harness.app.stop_composing();
            harness.app.on_focus_lost();
            assert_eq!(harness.app.composition.shown(), None);
            assert_eq!(harness.app.composed_row, None);
            assert!(harness.sent().is_empty());
        }

        #[test]
        fn a_disabled_ime_mid_composition_sends_nothing() {
            let mut harness = Harness::new(0, true, "");
            harness.app.on_ime(preedit("ni"));
            harness.app.on_ime(Ime::Disabled);
            assert_eq!(harness.app.composition.shown(), None);
            assert!(harness.sent().is_empty());
        }

        #[test]
        fn an_empty_commit_is_not_sent() {
            let mut harness = Harness::new(0, true, "");
            harness.app.on_ime(preedit("ni"));
            harness.app.on_ime(Ime::Commit(String::new()));
            assert!(harness.sent().is_empty());
        }
    }

    mod damage {
        use super::*;
        use crate::retained::Damage;
        use crate::screen_buffer::painter;

        fn rebuilt(harness: &mut Harness) -> (bool, Vec<usize>) {
            let cursor = harness.app.cursor_options();
            let (state, cache, retained, blink, paints, hovered) = (
                &harness.app.state,
                &mut harness.app.cache,
                &mut harness.app.retained,
                harness.app.blink,
                &harness.app.options.paints,
                harness.app.hovered_link.as_ref(),
            );
            let preedit = harness.app.composition.shown();
            retained.refresh(
                state,
                cache,
                blink,
                paints,
                cursor,
                hovered,
                preedit.as_ref(),
            );
            (
                retained.rebuilt_everything(),
                retained.rebuilt_rows().to_vec(),
            )
        }

        fn painted(harness: &mut Harness, paint: impl FnOnce(&mut painter::Painter)) {
            let size = harness.app.state.size();
            let frame = painter::Painter::frame(size.rows, size.cols, paint);
            harness.app.on_frame(&frame);
        }

        #[test]
        fn a_frame_damages_the_rows_it_writes_and_nothing_else() {
            let mut harness = Harness::new(4, true, "");
            let _ = rebuilt(&mut harness);
            painted(&mut harness, |painter| painter.text(3, 0, "typed"));
            assert_eq!(rebuilt(&mut harness), (false, vec![3]));
        }

        #[test]
        fn a_redraw_with_nothing_marked_rebuilds_no_row_at_all() {
            let mut harness = Harness::new(4, true, "");
            let _ = rebuilt(&mut harness);
            assert_eq!(rebuilt(&mut harness), (false, Vec::new()));
        }

        #[test]
        fn a_blink_flip_damages_only_the_rows_that_carry_a_blinking_cell() {
            let mut harness = Harness::new(4, true, "");
            painted(&mut harness, |painter| {
                painter.text(0, 0, "steady");
                painter.styled(2, 0, "blinks", |cell| {
                    cell.attrs |= zellij_utils::structured_render::ATTR_SLOW_BLINK
                });
            });
            let _ = rebuilt(&mut harness);
            harness.app.advance_blink();
            assert_eq!(rebuilt(&mut harness), (false, vec![2]));
        }

        #[test]
        fn hovering_a_link_damages_the_rows_the_underline_appears_and_disappears_on() {
            let mut harness = Harness::new(4, true, "");
            with_a_link_under_the_pointer(&mut harness, "https://example.com");
            let _ = rebuilt(&mut harness);
            harness
                .app
                .on_cursor_moved(PhysicalPosition::new(45.0, 4.0));
            assert_eq!(rebuilt(&mut harness), (false, vec![0]));
        }

        #[test]
        fn a_preedit_damages_the_row_the_cursor_sits_on_and_no_other() {
            let mut harness = Harness::new(0, true, "");
            painted(&mut harness, |painter| {
                painter.text(0, 0, "prompt");
                painter.cursor(0, 6, crate::screen_buffer::CursorShape::Block);
            });
            let _ = rebuilt(&mut harness);
            harness
                .app
                .on_ime(Ime::Preedit("ni".to_owned(), Some((2, 2))));
            assert_eq!(rebuilt(&mut harness), (false, vec![0]));
        }

        #[test]
        fn a_composition_that_ends_damages_the_row_it_was_drawn_on() {
            let mut harness = Harness::new(1, true, "");
            painted(&mut harness, |painter| {
                painter.text(2, 0, "prompt");
                painter.cursor(2, 6, crate::screen_buffer::CursorShape::Block);
            });
            harness
                .app
                .on_ime(Ime::Preedit("ni".to_owned(), Some((2, 2))));
            let _ = rebuilt(&mut harness);
            harness.app.on_ime(Ime::Commit("你".to_owned()));
            assert_eq!(rebuilt(&mut harness), (false, vec![2]));
        }

        #[test]
        fn a_theme_change_damages_every_row() {
            let mut harness = Harness::new(4, true, "");
            let _ = rebuilt(&mut harness);
            harness.app.retained.mark(&Damage::Everything);
            assert!(rebuilt(&mut harness).0);
        }
    }

    mod pacing {
        use super::*;

        fn painted(harness: &mut Harness, row: usize, text: &str) {
            let size = harness.app.state.size();
            let frame =
                crate::screen_buffer::painter::Painter::frame(size.rows, size.cols, |painter| {
                    painter.text(row, 0, text)
                });
            harness.app.on_frame(&frame);
        }

        fn rebuilt_rows(harness: &mut Harness) -> Vec<usize> {
            let cursor = harness.app.cursor_options();
            let (state, cache, retained, blink, paints, hovered) = (
                &harness.app.state,
                &mut harness.app.cache,
                &mut harness.app.retained,
                harness.app.blink,
                &harness.app.options.paints,
                harness.app.hovered_link.as_ref(),
            );
            let preedit = harness.app.composition.shown();
            retained.refresh(
                state,
                cache,
                blink,
                paints,
                cursor,
                hovered,
                preedit.as_ref(),
            );
            retained.rebuilt_rows().to_vec()
        }

        #[test]
        fn frames_applied_between_two_draws_are_coalesced_into_a_single_draw() {
            let mut harness = Harness::new(3, true, "");
            let _ = rebuilt_rows(&mut harness);
            let drawn = Instant::now();
            harness.app.pacer.drawn(drawn);

            painted(&mut harness, 1, "one");
            painted(&mut harness, 2, "two");
            painted(&mut harness, 3, "three");

            let interval = harness.app.pacer.interval();
            assert_eq!(
                harness.app.pacer.decide(drawn + Duration::from_micros(500)),
                Decision::At(drawn + interval),
                "three frames inside one interval must not ask for three draws"
            );
            assert_eq!(
                rebuilt_rows(&mut harness),
                vec![1, 2, 3],
                "the one draw owed must carry every row the coalesced frames wrote"
            );
            assert_eq!(harness.app.pacer.decide(drawn + interval), Decision::Now);
        }

        #[test]
        fn every_coalesced_frame_is_still_acknowledged_on_apply() {
            let mut harness = Harness::new(3, true, "");
            harness.app.pacer.drawn(Instant::now());
            painted(&mut harness, 1, "one");
            painted(&mut harness, 2, "two");
            painted(&mut harness, 3, "three");
            assert_eq!(
                harness.sent().len(),
                3,
                "pacing the display must not pace delivery"
            );
        }

        #[test]
        fn a_keystroke_on_a_quiet_screen_draws_at_the_next_opportunity() {
            let mut harness = Harness::new(2, true, "");
            let idle_since = Instant::now() - Duration::from_secs(1);
            harness.app.pacer.drawn(idle_since);
            painted(&mut harness, 0, "a");
            assert_eq!(harness.app.pacer.decide(Instant::now()), Decision::Now);
        }

        #[test]
        fn a_window_starts_out_pacing_at_the_fallback_rate() {
            let harness = Harness::new(0, true, "");
            assert_eq!(
                harness.app.pacer.interval(),
                crate::pacing::interval_of(Some(60_000))
            );
        }
    }

    mod smooth_scrolling {
        use super::*;
        use zellij_utils::structured_render::{
            FrameBuilder, GeometryRecord, PaneRect, ScrollEntry, ScrollRecord, WireCell,
            PANE_FRAMED,
        };

        fn whole_pane() -> GeometryRecord {
            GeometryRecord {
                panes: vec![PaneRect {
                    x: 0,
                    y: 0,
                    cols: 120,
                    rows: 40,
                    top: 1,
                    bottom: 1,
                    left: 1,
                    right: 1,
                    flags: PANE_FRAMED,
                }],
            }
        }

        fn frame(seq: u64, first: bool, base: usize, hint: Option<i16>) -> Vec<u8> {
            let mut builder = FrameBuilder::new(120, 40, seq);
            builder.set_full_repaint(first);
            builder.set_clear(first);
            for row in 1..39 {
                let text = format!("line {}", base + row);
                let cells: Vec<WireCell> = text
                    .chars()
                    .map(|character| WireCell {
                        ch: character as u32,
                        ..WireCell::BLANK
                    })
                    .collect();
                builder.push_row(row as u16, 1, &cells);
            }
            if first {
                builder.push_geometry(&whole_pane());
            }
            if let Some(lines) = hint {
                builder.push_scroll(&ScrollRecord {
                    entries: vec![ScrollEntry { pane: 0, lines }],
                });
            }
            builder.finish()
        }

        fn scrolled(expected: usize) -> Harness {
            let mut harness = Harness::new(expected, true, "");
            harness.app.on_frame(&frame(1, true, 100, None));
            harness.app.on_frame(&frame(2, false, 97, Some(3)));
            harness
        }

        #[test]
        fn a_scroll_hint_starts_an_animation_that_draws_every_refresh_until_it_ends() {
            let mut harness = scrolled(2);
            assert!(harness.app.scroll.is_active());
            let start = Instant::now();
            harness.app.pacer.drawn(start);
            harness.app.advance_scroll_animations(start);
            assert_eq!(
                harness.app.pacer.decide(start),
                Decision::At(start + harness.app.pacer.interval()),
                "a running slide must ask for the next refresh"
            );

            let after = start + Duration::from_millis(500);
            harness.app.advance_scroll_animations(after);
            assert!(!harness.app.scroll.is_active());
            harness.app.pacer.drawn(after);
            harness.app.advance_scroll_animations(after);
            assert_eq!(
                harness.app.pacer.decide(after),
                Decision::Idle,
                "nothing is drawn once every slide has ended"
            );
        }

        #[test]
        fn a_frame_without_a_hint_starts_nothing() {
            let mut harness = Harness::new(2, true, "");
            harness.app.on_frame(&frame(1, true, 100, None));
            harness.app.on_frame(&frame(2, false, 97, None));
            assert!(!harness.app.scroll.is_active());
        }

        #[test]
        fn turning_smooth_scrolling_off_ends_a_running_slide_at_its_final_position() {
            let mut harness = scrolled(2);
            assert!(harness.app.scroll.is_active());
            harness.reconfigure(WindowConfig {
                smooth_scrolling: Some(false),
                ..WindowConfig::default()
            });
            assert!(!harness.app.scroll.is_active());
            assert!(!harness.app.options.smooth_scrolling);

            harness.app.on_frame(&frame(3, false, 95, Some(2)));
            assert!(!harness.app.scroll.is_active());
        }

        #[test]
        fn the_configured_duration_is_used_after_a_reload() {
            let mut harness = Harness::new(2, true, "");
            harness.reconfigure(WindowConfig {
                scroll_animation_duration: Some(400),
                ..WindowConfig::default()
            });
            harness.app.on_frame(&frame(1, true, 100, None));
            let before = Instant::now();
            harness.app.on_frame(&frame(2, false, 97, Some(3)));
            assert!(harness.app.scroll.is_active());
            assert!(!harness
                .app
                .scroll
                .retire(before + Duration::from_millis(300)));
            assert!(harness
                .app
                .scroll
                .retire(Instant::now() + Duration::from_millis(400)));
        }

        #[test]
        fn a_configuration_reload_or_theme_change_cancels_the_slide() {
            let mut harness = scrolled(3);
            harness.reconfigure(WindowConfig::default());
            assert!(!harness.app.scroll.is_active());

            harness.app.on_frame(&frame(3, false, 95, Some(2)));
            assert!(harness.app.scroll.is_active());
            harness.app.theme_mode = Some(HostTerminalThemeMode::Dark);
            harness.app.reapply(false);
            assert!(!harness.app.scroll.is_active());
        }
    }

    mod scroll_momentum {
        use super::*;
        use crate::mouse::PointerState;
        use zellij_utils::structured_render::{
            FrameBuilder, GeometryRecord, PaneRect, ScrollEntry, ScrollRecord, PANE_FRAMED,
            PANE_SELECTABLE,
        };

        fn pane(x: u16) -> PaneRect {
            PaneRect {
                x,
                y: 0,
                cols: 60,
                rows: 40,
                top: 1,
                bottom: 1,
                left: 1,
                right: 1,
                flags: PANE_FRAMED | PANE_SELECTABLE,
            }
        }

        fn frame(seq: u64, first: bool, hints: &[(u16, i16)]) -> Vec<u8> {
            let mut builder = FrameBuilder::new(120, 40, seq);
            builder.set_full_repaint(first);
            builder.set_clear(first);
            if first {
                builder.push_geometry(&GeometryRecord {
                    panes: vec![pane(0), pane(60)],
                });
            }
            builder.push_scroll(&ScrollRecord {
                entries: hints
                    .iter()
                    .map(|&(pane, lines)| ScrollEntry { pane, lines })
                    .collect(),
            });
            builder.finish()
        }

        fn ms(millis: u64) -> Duration {
            Duration::from_millis(millis)
        }

        fn pixels(y: f64) -> MouseScrollDelta {
            MouseScrollDelta::PixelDelta(at(0.0, y))
        }

        const OVER_LEFT: (f64, f64) = (100.0, 200.0);
        const OVER_RIGHT: (f64, f64) = (800.0, 200.0);

        struct Fling {
            harness: Harness,
            seq: u64,
            start: Instant,
        }

        impl Fling {
            fn offline(wayland: bool) -> Self {
                Self::new(0, wayland, false)
            }

            fn wired(expected: usize, wayland: bool) -> Self {
                Self::new(expected, wayland, true)
            }

            fn new(expected: usize, wayland: bool, wired: bool) -> Self {
                let mut harness = Harness::new(expected, true, "");
                harness.app.wayland = wayland;
                if !wired {
                    harness.app.sender = None;
                }
                let mut fling = Self {
                    harness,
                    seq: 0,
                    start: Instant::now(),
                };
                fling.frame(true, &[]);
                fling
                    .harness
                    .app
                    .on_cursor_moved(at(OVER_LEFT.0, OVER_LEFT.1));
                fling
            }

            fn frame(&mut self, first: bool, hints: &[(u16, i16)]) {
                self.seq += 1;
                self.harness.app.on_frame(&frame(self.seq, first, hints));
            }

            fn wheel(&mut self, delta: MouseScrollDelta, phase: TouchPhase, after: u64) {
                self.harness
                    .app
                    .on_wheel_at(delta, phase, self.start + ms(after));
            }

            fn swipe(&mut self, step: f64, hinted: bool) {
                self.wheel(pixels(step), TouchPhase::Started, 0);
                for index in 1..=5 {
                    self.wheel(pixels(step), TouchPhase::Moved, 10 * index);
                }
                if hinted {
                    self.frame(false, &[(0, 2)]);
                }
                self.wheel(pixels(0.0), TouchPhase::Ended, 55);
            }

            fn tick(&mut self, after: u64) -> Vec<MouseEvent> {
                self.harness.app.tick_momentum(self.start + ms(after))
            }

            fn coasting(&self) -> bool {
                self.harness.app.momentum.is_coasting()
            }

            fn coasting_fling(wayland: bool) -> Self {
                let mut fling = Self::offline(wayland);
                fling.swipe(40.0, true);
                fling
            }

            fn assert_stopped(&mut self, why: &str) {
                assert!(!self.coasting(), "{}", why);
                assert!(self.tick(400).is_empty(), "{}", why);
                assert!(self.tick(2000).is_empty(), "{}", why);
            }
        }

        #[test]
        fn a_fast_swipe_through_scrollback_keeps_scrolling_after_the_lift() {
            let mut fling = Fling::coasting_fling(true);
            assert!(fling.coasting());
            let events = fling.tick(100);
            assert_eq!(events.len(), 1);
            assert!(events[0].wheel_up && events[0].wheel_lines > 0);
            assert_eq!(
                events[0].position,
                zellij_utils::position::Position::new(10, 10),
                "the glide scrolls where the swipe ended"
            );
        }

        #[test]
        fn a_downward_swipe_glides_downward() {
            let mut fling = Fling::offline(true);
            fling.swipe(-40.0, true);
            let events = fling.tick(100);
            assert!(events[0].wheel_down && events[0].wheel_lines > 0);
        }

        #[test]
        fn the_glide_sends_whole_lines_slows_down_and_ends() {
            let mut fling = Fling::coasting_fling(true);
            let mut lines = Vec::new();
            let mut after = 55;
            while fling.coasting() {
                after += 16;
                let events = fling.tick(after);
                for event in &events {
                    assert!(event.wheel_up && event.wheel_lines > 0);
                    lines.push((after, event.wheel_lines as u64));
                }
                if !events.is_empty() {
                    fling.frame(false, &[(0, 1)]);
                }
            }
            let total: u64 = lines.iter().map(|(_, count)| count).sum();
            assert_eq!(total, 97, "(4000 - 100) px / 2 / 20 px per line");
            let early: u64 = lines
                .iter()
                .filter(|(at, _)| *at < 555)
                .map(|(_, count)| count)
                .sum();
            assert!(early > total - early, "the glide slows down");
            assert!(fling.tick(after + 16).is_empty());
        }

        #[test]
        fn a_slow_swipe_or_a_bare_lift_starts_nothing() {
            let mut fling = Fling::offline(true);
            fling.swipe(3.0, true);
            assert!(!fling.coasting());

            let mut fling = Fling::offline(true);
            fling.frame(false, &[(0, 2)]);
            fling.wheel(pixels(0.0), TouchPhase::Ended, 0);
            assert!(!fling.coasting());
        }

        #[test]
        fn a_swipe_the_server_answered_without_a_hint_starts_nothing() {
            let mut fling = Fling::offline(true);
            fling.swipe(40.0, false);
            assert!(
                !fling.coasting(),
                "a full-screen program or a plugin pane produces no scroll hint"
            );

            let mut fling = Fling::offline(true);
            fling.wheel(pixels(40.0), TouchPhase::Started, 0);
            fling.wheel(pixels(40.0), TouchPhase::Moved, 10);
            fling.frame(false, &[(1, 2)]);
            fling.wheel(pixels(0.0), TouchPhase::Ended, 15);
            assert!(!fling.coasting(), "a hint for another pane is no evidence");
        }

        #[test]
        fn the_scrollback_running_out_stops_the_glide_once_the_frame_says_so() {
            let mut fling = Fling::coasting_fling(true);
            assert!(!fling.tick(100).is_empty());
            assert!(!fling.tick(130).is_empty());
            assert!(
                fling.coasting(),
                "no frame has answered the scroll yet, so nothing is decided"
            );
            fling.frame(false, &[(0, 3)]);
            assert!(fling.coasting());
            assert!(!fling.tick(160).is_empty());
            fling.frame(false, &[]);
            fling.assert_stopped("a frame without a hint after a sent scroll");
        }

        #[test]
        fn every_interruption_stops_the_glide_with_nothing_further_sent() {
            let interruptions: Vec<(&str, Box<dyn Fn(&mut Fling)>)> = vec![
                (
                    "fingers back down",
                    Box::new(|f: &mut Fling| f.wheel(pixels(0.0), TouchPhase::Started, 80)),
                ),
                (
                    "fingers moving",
                    Box::new(|f: &mut Fling| f.wheel(pixels(1.0), TouchPhase::Moved, 80)),
                ),
                (
                    "a mouse wheel",
                    Box::new(|f: &mut Fling| {
                        f.wheel(MouseScrollDelta::LineDelta(0.0, 1.0), TouchPhase::Moved, 80)
                    }),
                ),
                (
                    "a mouse button",
                    Box::new(|f: &mut Fling| {
                        f.harness
                            .app
                            .on_mouse_input(MouseButton::Left, ElementState::Pressed)
                    }),
                ),
                (
                    "a key sent to the session",
                    Box::new(|f: &mut Fling| f.harness.press(&character("a"))),
                ),
                (
                    "a window key",
                    Box::new(|f: &mut Fling| {
                        f.harness.app.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
                        f.harness.press(&character("V"))
                    }),
                ),
                (
                    "a font zoom",
                    Box::new(|f: &mut Fling| {
                        f.harness.app.modifiers = ModifiersState::CONTROL;
                        f.harness.press(&character("="))
                    }),
                ),
                (
                    "text from the input method",
                    Box::new(|f: &mut Fling| {
                        f.harness.app.on_ime(Ime::Preedit(String::new(), None));
                        f.harness.app.on_ime(Ime::Commit("x".to_owned()));
                    }),
                ),
                (
                    "focus loss",
                    Box::new(|f: &mut Fling| f.harness.app.on_focus_lost()),
                ),
                (
                    "the pointer leaving",
                    Box::new(|f: &mut Fling| f.harness.app.on_cursor_left()),
                ),
                (
                    "a resize",
                    Box::new(|f: &mut Fling| f.harness.app.resized(640, 480)),
                ),
                (
                    "a scale change",
                    Box::new(|f: &mut Fling| f.harness.app.rescale(2.0)),
                ),
                (
                    "a configuration reload",
                    Box::new(|f: &mut Fling| f.harness.reconfigure(WindowConfig::default())),
                ),
                (
                    "a session switch",
                    Box::new(|f: &mut Fling| {
                        f.harness.app.follow(None);
                    }),
                ),
                (
                    "the pointer moving to another pane",
                    Box::new(|f: &mut Fling| {
                        f.harness
                            .app
                            .on_cursor_moved(at(OVER_RIGHT.0, OVER_RIGHT.1))
                    }),
                ),
            ];
            for (why, interrupt) in interruptions {
                let mut fling = Fling::coasting_fling(true);
                assert!(!fling.tick(70).is_empty(), "{}", why);
                fling.frame(false, &[(0, 1)]);
                assert!(fling.coasting(), "{}", why);
                interrupt(&mut fling);
                fling.assert_stopped(why);
            }
        }

        #[test]
        fn moving_within_the_same_pane_keeps_the_glide() {
            let mut fling = Fling::coasting_fling(true);
            fling.harness.app.on_cursor_moved(at(150.0, 300.0));
            assert!(fling.coasting());
        }

        #[test]
        fn turning_momentum_off_stops_it_and_keeps_it_off() {
            let mut fling = Fling::coasting_fling(true);
            fling.harness.reconfigure(WindowConfig {
                scroll_momentum: Some(false),
                ..WindowConfig::default()
            });
            fling.assert_stopped("momentum switched off");
            fling.swipe(40.0, true);
            assert!(!fling.coasting());
        }

        #[test]
        fn momentum_works_with_smooth_scrolling_off() {
            let mut fling = Fling::offline(true);
            fling.harness.reconfigure(WindowConfig {
                smooth_scrolling: Some(false),
                ..WindowConfig::default()
            });
            fling.swipe(40.0, true);
            assert!(fling.coasting());
            assert!(!fling.tick(100).is_empty());
        }

        #[test]
        fn a_running_glide_asks_for_a_wake_up_and_an_idle_one_asks_for_nothing() {
            let mut fling = Fling::offline(true);
            let now = Instant::now();
            assert_eq!(fling.harness.app.advance_momentum(now), None);
            fling.swipe(40.0, true);
            let interval = fling.harness.app.pacer.interval();
            let now = fling.start + ms(80);
            fling.harness.app.pacer.drawn(now);
            assert_eq!(
                fling.harness.app.advance_momentum(now),
                Some(now + interval)
            );
            assert_eq!(
                fling.harness.app.pacer.decide(now),
                Decision::Idle,
                "a glide redraws only when a frame comes back"
            );
        }

        fn scenario(mirror: &mut PointerState) -> Vec<ClientToServerMsg> {
            let geometry = geometry();
            let none = ModifiersState::empty();
            let mut expected = vec![ClientToServerMsg::RenderFrameAck { seq: 1 }];
            expected.extend(
                mirror
                    .moved(at(OVER_LEFT.0, OVER_LEFT.1), geometry, none)
                    .map(mouse::message),
            );
            for _ in 0..6 {
                expected.extend(
                    mirror
                        .wheel(pixels(40.0), geometry, none)
                        .into_iter()
                        .map(mouse::message),
                );
            }
            expected.push(ClientToServerMsg::RenderFrameAck { seq: 2 });
            expected.extend(
                mirror
                    .wheel(pixels(0.0), geometry, none)
                    .into_iter()
                    .map(mouse::message),
            );
            expected
        }

        fn run(
            wayland: bool,
            momentum: bool,
            tick: bool,
        ) -> (Vec<ClientToServerMsg>, Vec<ClientToServerMsg>) {
            let expected = scenario(&mut PointerState::new());
            let mut fling = Fling::wired(expected.len(), wayland);
            if !momentum {
                fling.harness.app.options.scroll_momentum = false;
            }
            fling.swipe(40.0, true);
            if tick {
                for after in [70, 90, 200, 900, 3000] {
                    fling.harness.app.advance_momentum(fling.start + ms(after));
                }
            }
            assert_eq!(
                fling.harness.app.momentum.is_coasting(),
                wayland && momentum && !tick
            );
            (fling.harness.sent(), expected)
        }

        #[test]
        fn the_touch_phase_changes_nothing_that_each_wheel_event_sends() {
            let (sent, expected) = run(true, true, false);
            assert_eq!(sent, expected);
        }

        #[test]
        fn away_from_wayland_the_same_swipe_sends_exactly_what_it_did_before() {
            let (sent, expected) = run(false, true, true);
            assert_eq!(sent, expected);
            let mut fling = Fling::offline(false);
            fling.swipe(40.0, true);
            assert!(!fling.coasting());
            assert_eq!(
                fling.harness.app.advance_momentum(fling.start + ms(80)),
                None
            );
        }

        #[test]
        fn with_momentum_off_the_same_swipe_sends_exactly_what_it_did_before() {
            let (sent, expected) = run(true, false, true);
            assert_eq!(sent, expected);
        }
    }
}

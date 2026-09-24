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
use winit::event::{ElementState, Ime, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{
    CursorIcon, Icon, ImePurpose, UserAttentionType, Window, WindowAttributes, WindowId,
};
use zellij_utils::data::{ConnectToSession, HostTerminalThemeMode};
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::ipc::{ClientToServerMsg, ExitReason};

use crate::atlas::GlyphCache;
use crate::bell;
use crate::client_loop::{self, LoopOptions, LoopOutcome, RenderSink};
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
use crate::mouse::{self, PointerState};
use crate::notify;
use crate::options::{Change, Options};
use crate::pacing::{Decision, Pacer};
use crate::palette;
use crate::platform::Platform;
use crate::renderer::Renderer;
use crate::retained::{self, Damage, RetainedScene};
use crate::scene::{self, BlinkPhase};
use crate::settings::Settings;
use crate::terminal::{self, FrameError, TerminalState};
use zellij_utils::input::window::{NotificationMode, StartupMode};

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
    ThemeMode(HostTerminalThemeMode),
    Finished(Option<ConnectToSession>),
}

struct ProxySink {
    proxy: EventLoopProxy<Wake>,
}

impl RenderSink for ProxySink {
    fn frame(&mut self, frame: Vec<u8>) -> bool {
        self.proxy.send_event(Wake::Frame(frame)).is_ok()
    }

    fn finished(
        &mut self,
        _exit_reason: Option<&ExitReason>,
        switch_to: Option<ConnectToSession>,
    ) -> bool {
        self.proxy.send_event(Wake::Finished(switch_to)).is_ok()
    }

    fn acknowledges_frames(&self) -> bool {
        true
    }

    fn reconfigured(&mut self, settings: Settings) -> bool {
        self.proxy.send_event(Wake::Reconfigured(settings)).is_ok()
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
    pointer: PointerState,
    composition: Composition,
    composed_row: Option<usize>,
    ime_area: Option<(i32, i32)>,
    hovered_link: Option<LinkRun>,
    armed_link: Option<links::Armed>,
    clipboard: ClipboardHandle,
    title: String,
    retained: RetainedScene,
    blink: BlinkPhase,
    blink_since: Instant,
    pacer: Pacer,
    display_checked: Instant,
    initial_size: (u32, u32),
    surfaces: Option<Surfaces>,
    failure: Option<anyhow::Error>,
    session: Option<Session>,
    ring: fn(),
    notify: fn(NotificationMode, &crate::kitty::Notification) -> bool,
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
        match self.options.font.stack(scale * zoom) {
            Ok(fonts) => {
                self.metrics = fonts.metrics();
                self.cache = GlyphCache::new(fonts);
                self.scale = scale;
                self.zoom = zoom;
            },
            Err(e) => {
                eprintln!(
                    "zellij-window: keeping the font already in place; {} px is unusable: {}",
                    self.options.font.size * (scale * zoom) as f32,
                    e
                );
                return false;
            },
        }
        let size = self
            .surfaces
            .as_ref()
            .map(|surfaces| surfaces.window.inner_size());
        if let Some(size) = size {
            self.schedule_draw();
            self.reflow(size.width, size.height);
        }
        true
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

    fn reapply(&mut self, zoom_reset: bool) {
        let next = crate::options::resolve(&self.settings, self.theme_mode);
        let change = Change::between(&self.options, &next);
        if !change.is_anything() && !zoom_reset {
            return;
        }
        let previous_font = self.options.font.clone();
        self.options = next;

        if change.paints {
            for msg in palette::seed_messages(&self.options.paints) {
                if let Err(e) = self.tell(msg) {
                    eprintln!("zellij-window: failed to re-declare a color: {}", e);
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
        } else if change.needs_redraw() {
            self.retained.mark_everything();
            self.schedule_draw();
        }
        if change.open_links {
            self.refresh_pointer();
        }
    }

    fn reflow(&mut self, width: u32, height: u32) {
        let cols = (width / self.metrics.width) as usize;
        let rows = (height / self.metrics.height) as usize;
        let clamped = terminal::clamped(rows, cols);
        let next = Geometry {
            rows: clamped.rows,
            cols: clamped.cols,
            cell_width: self.metrics.width as usize,
            cell_height: self.metrics.height as usize,
        };
        self.state
            .set_cell_size(self.metrics.width, self.metrics.height);
        if let Some(sender) = &self.sender {
            if let Err(e) = resize(sender, &self.geometry, next) {
                eprintln!("zellij-window: failed to report a resize: {}", e);
            }
        }
    }

    fn on_frame(&mut self, frame: &[u8]) {
        match self.state.apply_frame(frame) {
            Ok(applied) => {
                self.retained.mark(&applied.damage);
                self.schedule_draw();
                self.acknowledge(applied.seq);
            },
            Err(FrameError::Unapplicable { seq, error }) => {
                eprintln!(
                    "zellij-window: dropping a frame the viewport has moved past: {}",
                    error
                );
                self.acknowledge(seq);
                return;
            },
            Err(FrameError::Undecodable(e)) => {
                eprintln!("zellij-window: refusing an unreadable frame: {}", e);
                if let Err(detach_error) = self.detach() {
                    self.failure = Some(
                        detach_error.context("failed to detach after an unreadable frame arrived"),
                    );
                }
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
        match self.detach() {
            Ok(()) => self.sender.is_none(),
            Err(e) => {
                eprintln!(
                    "zellij-window: the session could not be asked to let the window go, \
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
            eprintln!("zellij-window: failed to acknowledge a frame: {}", e);
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
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let event = self.pointer.moved(position, geometry, modifiers);
        self.refresh_hover();
        self.refresh_pointer();
        self.point(event);
    }

    fn on_mouse_input(&mut self, button: winit::event::MouseButton, state: ElementState) {
        self.note_link_click(button, state);
        if self.middle_click_pasted(button, state) {
            return;
        }
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let event = self.pointer.button(button, state, geometry, modifiers);
        self.point(event);
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
                        eprintln!(
                            "zellij-window: failed to send a primary-selection paste: {}",
                            e
                        );
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
                self.options.open_links,
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
            eprintln!("zellij-window: {}", e);
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
            self.options.open_links,
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

    fn on_wheel(&mut self, delta: winit::event::MouseScrollDelta) {
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let events = self.pointer.wheel(delta, geometry, modifiers);
        self.point(events);
    }

    fn on_focus_lost(&mut self) {
        let (geometry, modifiers) = (self.geometry.get(), self.modifiers);
        let events = self.pointer.focus_lost(geometry, modifiers);
        self.point(events);
    }

    fn on_key(&mut self, event: &KeyEvent) {
        self.press(input::Press::of(event, self.modifiers));
    }

    fn press(&mut self, press: input::Press) {
        let gate = self.composition.gate();
        if gate == Gate::Composing {
            return;
        }
        if let Key::Dead(accent) = press.logical {
            let reaction = self.composition.dead_key(accent);
            self.compose(reaction);
            return;
        }
        if gate == Gate::Resolving {
            let reaction = self.composition.resolve();
            self.compose(reaction);
            self.deliver(&press);
            return;
        }
        if input::claims(&self.options.zoom_in_keys, &press) {
            self.zoom_by(ZOOM_STEP);
            return;
        }
        if input::claims(&self.options.zoom_out_keys, &press) {
            self.zoom_by(1.0 / ZOOM_STEP);
            return;
        }
        if input::claims(&self.options.zoom_reset_keys, &press) {
            self.set_zoom(1.0);
            return;
        }
        if input::claims(&self.options.paste_keys, &press) {
            self.paste();
            return;
        }
        self.deliver(&press);
    }

    fn deliver(&mut self, press: &input::Press) {
        let Some(msg) = input::key_message(press) else {
            return;
        };
        if let Err(e) = self.tell(msg) {
            eprintln!("zellij-window: failed to send a key: {}", e);
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
        let committed = input::Press::new(Key::Character(text.into()), ModifiersState::empty());
        let Some(msg) = input::key_message(&committed) else {
            return;
        };
        if let Err(e) = self.tell(msg) {
            eprintln!("zellij-window: failed to send composed text: {}", e);
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
        let Some(chars) = self.with_clipboard(|clipboard| clipboard.get()).flatten() else {
            return;
        };
        if let Err(e) = self.tell(clipboard::paste_message(chars)) {
            eprintln!("zellij-window: failed to send a paste: {}", e);
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
                eprintln!("zellij-window: the clipboard mutex was poisoned");
                None
            },
        }
    }

    fn point<I: IntoIterator<Item = MouseEvent>>(&mut self, events: I) {
        for event in events {
            if let Err(e) = self.tell(mouse::message(event)) {
                eprintln!("zellij-window: failed to send a mouse event: {}", e);
            }
        }
    }

    fn schedule_draw(&mut self) {
        self.pacer.schedule();
    }

    fn follow_cursor_area(&mut self) {
        let (row, col) = self.state.cursor_position();
        let area = (
            (col as u32 * self.metrics.width) as i32,
            (row as u32 * self.metrics.height) as i32,
        );
        if self.ime_area == Some(area) {
            return;
        }
        let Some(surfaces) = &self.surfaces else {
            return;
        };
        surfaces.window.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(area.0, area.1),
            winit::dpi::PhysicalSize::new(self.metrics.width, self.metrics.height),
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
        let cursor = self.cursor_options();
        let preedit = self.composition.shown();
        self.retained.refresh(
            &self.state,
            &mut self.cache,
            self.blink,
            &self.options.paints,
            cursor,
            self.hovered_link.as_ref(),
            preedit.as_ref(),
        );
        self.follow_cursor_area();
        let Some(surfaces) = self.surfaces.as_mut() else {
            return;
        };
        let size = surfaces.window.inner_size();
        let target = (size.width.max(1), size.height.max(1));
        surfaces
            .renderer
            .draw_retained(&self.retained, self.cache.atlases(), target);
        if let Err(e) = surfaces.surface.swap_buffers(&surfaces.context) {
            eprintln!("zellij-window: buffer swap failed: {}", e);
        }
        self.pacer.drawn(Instant::now());
    }

    fn follow(&mut self, switch_to: Option<ConnectToSession>) -> bool {
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
                self.state = TerminalState::new(geometry.rows, geometry.cols);
                self.state
                    .set_cell_size(self.metrics.width, self.metrics.height);
                self.set_title(window_title(&session_name));
                self.composition.cancel();
                self.composed_row = None;
                self.retained.mark_everything();
                self.schedule_draw();
                for msg in palette::seed_messages(&self.options.paints) {
                    if let Err(e) = self.tell(msg) {
                        eprintln!("zellij-window: failed to re-declare a color: {}", e);
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

    fn stranded(&mut self, failure: anyhow::Error) {
        let message = format!("{:#}", failure);
        eprintln!("zellij-window: {}", message);
        self.sender = None;
        self.show_notice(&message);
        self.failure = Some(failure);
    }

    fn show_notice(&mut self, message: &str) {
        let size = self.state.size();
        let frame = crate::notice::frame(message, size.rows, size.cols);
        if let Err(e) = self.state.apply_frame(&frame) {
            eprintln!("zellij-window: the message could not be drawn: {}", e);
            return;
        }
        self.retained.mark_everything();
        self.schedule_draw();
    }

    fn bring_up(&mut self, event_loop: &ActiveEventLoop) -> Result<Surfaces> {
        let attributes = named(startup_attributes(
            WindowAttributes::default()
                .with_title(self.title.clone())
                .with_window_icon(window_icon())
                .with_inner_size(winit::dpi::PhysicalSize::new(
                    self.initial_size.0,
                    self.initial_size.1,
                )),
            self.options.startup_mode,
        ));

        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attributes))
            .build(
                event_loop,
                ConfigTemplateBuilder::new().with_alpha_size(8),
                pick_config,
            )
            .map_err(|e| anyhow!("failed to create a window: {}", e))?;
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
            eprintln!(
                "zellij-window: the display refused an unsynchronized buffer swap, so a swap may wait for the next refresh: {}",
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
        match self.bring_up(event_loop) {
            Ok(surfaces) => {
                let size = surfaces.window.inner_size();
                let scale = surfaces.window.scale_factor();
                self.surfaces = Some(surfaces);
                self.follow_display();
                self.schedule_draw();
                self.rescale(scale);
                self.reflow(size.width, size.height);
            },
            Err(e) => {
                self.failure = Some(e);
                let _ = self.detach();
                event_loop.exit();
            },
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        match event {
            Wake::Frame(frame) => {
                self.on_frame(&frame);
                if self.failure.is_some() {
                    event_loop.exit();
                }
            },
            Wake::Reconfigured(settings) => self.reconfigured(settings),
            Wake::ThemeMode(mode) => {
                self.theme_mode = Some(mode);
                self.reapply(false);
            },
            Wake::Finished(switch_to) => {
                if !self.follow(switch_to) {
                    event_loop.exit();
                }
            },
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                if self.closing() {
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
                self.reflow(size.width, size.height);
            },
            WindowEvent::Moved(_) => self.follow_display_occasionally(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.follow_display();
                self.rescale(scale_factor);
            },
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                self.refresh_pointer();
            },
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                if !is_synthetic && event.state.is_pressed() {
                    if self.sender.is_none() {
                        event_loop.exit();
                    } else {
                        self.on_key(&event);
                    }
                }
            },
            WindowEvent::CursorMoved { position, .. } => self.on_cursor_moved(position),
            WindowEvent::MouseInput { button, state, .. } => self.on_mouse_input(button, state),
            WindowEvent::MouseWheel { delta, .. } => self.on_wheel(delta),
            WindowEvent::CursorEntered { .. } => {
                self.pointer.entered();
                self.refresh_pointer();
            },
            WindowEvent::CursorLeft { .. } => {
                self.pointer.left();
                self.refresh_pointer();
            },
            WindowEvent::Ime(event) => self.on_ime(event),
            WindowEvent::Focused(false) => {
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
        let blinks = self.anything_blinks();
        if blinks && now >= self.blink_deadline() {
            self.advance_blink();
            self.schedule_draw();
        }
        let mut deadline = blinks.then(|| self.blink_deadline());
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
}

fn startup_attributes(attributes: WindowAttributes, mode: StartupMode) -> WindowAttributes {
    match mode {
        StartupMode::Windowed => attributes,
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
    let renderer = Rendering::bring_up(&window.options, fonts, connection.geometry.get());
    let windowing = Windowing::bring_up()?;
    let geometry = connection.geometry.clone();
    let role = connection.role;
    let sender = connection.sender.clone();
    let title = window_title(&connection.session_name);
    let clipboard: ClipboardHandle = Arc::new(Mutex::new(Clipboard::open()));
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
    state: TerminalState,
    geometry: Geometry,
    fonts: FontStack,
    options: &Options,
) -> Result<()> {
    let windowing = Windowing::bring_up()?;
    let mut app = Rendering::from_state(state, options, fonts, geometry).into_app(
        None,
        Role::Watcher,
        GeometryHandle::new(geometry),
        "zellij".to_owned(),
        Settings::default(),
        Arc::new(Mutex::new(Clipboard::open())),
        None,
    );
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
            Some(Err(e)) => eprintln!("zellij-window: {}", e),
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
    initial_size: (u32, u32),
}

impl Rendering {
    fn bring_up(options: &Options, fonts: FontStack, geometry: Geometry) -> Self {
        let cache = GlyphCache::new(fonts);
        let metrics = cache.metrics();
        let mut state = TerminalState::new(geometry.rows, geometry.cols);
        state.set_cell_size(metrics.width, metrics.height);
        Self {
            state,
            cache,
            metrics,
            options: options.clone(),
            initial_size: (
                metrics.width * geometry.cols as u32,
                metrics.height * geometry.rows as u32,
            ),
        }
    }

    fn from_state(
        state: TerminalState,
        options: &Options,
        fonts: FontStack,
        geometry: Geometry,
    ) -> Self {
        let cache = GlyphCache::new(fonts);
        let metrics = cache.metrics();
        Self {
            state,
            cache,
            metrics,
            options: options.clone(),
            initial_size: (
                metrics.width * geometry.cols as u32,
                metrics.height * geometry.rows as u32,
            ),
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
            pointer: PointerState::new(),
            composition: Composition::new(),
            composed_row: None,
            ime_area: None,
            hovered_link: None,
            armed_link: None,
            clipboard,
            title,
            retained: RetainedScene::new(),
            blink: BlinkPhase::On,
            blink_since: Instant::now(),
            pacer: Pacer::new(None),
            display_checked: Instant::now(),
            initial_size: self.initial_size,
            surfaces: None,
            failure: None,
            session,
            ring: bell::ring,
            notify: notify::handled,
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
        Client(Some(std::thread::spawn(move || {
            client_loop::run_with_sink(connection, options, &mut sink)
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
            eprintln!("zellij-window: the embedded window icon is unreadable: {e}");
            return None;
        },
    };
    match Icon::from_rgba(image.pixels, image.width, image.height) {
        Ok(icon) => Some(icon),
        Err(e) => {
            eprintln!("zellij-window: the embedded window icon was refused: {e}");
            None
        },
    }
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

fn pick_config(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    configs
        .reduce(|best, config| {
            if config.num_samples() < best.num_samples() {
                config
            } else {
                best
            }
        })
        .expect("the display offered no configs")
}

#[cfg(test)]
mod tests {
    use super::*;

    use winit::dpi::PhysicalPosition;
    use winit::event::{MouseButton, MouseScrollDelta};
    use winit::keyboard::{Key, NamedKey, SmolStr};
    use zellij_utils::input::actions::Action;
    use zellij_utils::input::mouse::MouseEventType;
    use zellij_utils::ipc::ClientToServerMsg;

    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::clipboard::Clipboard;
    use crate::color::Paints;
    use crate::connection::{test_attach_at as attach_at, Capabilities, Connection};
    use crate::font::{FontOptions, DEFAULT_FONT_SIZE, DEFAULT_LIGATURES};
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

    struct Harness {
        app: App,
        server: FakeServer,
        clipboard: ClipboardHandle,
        _connection: Connection,
    }

    impl Harness {
        fn new(expected: usize, intercept_paste: bool, clipboard_text: &str) -> Self {
            Self::with(
                expected,
                Options {
                    font: FontOptions {
                        family: None,
                        size: DEFAULT_FONT_SIZE,
                        system_fonts: false,
                        ligatures: DEFAULT_LIGATURES,
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
                    bell: zellij_utils::input::window::BellMode::Visual,
                    notifications: zellij_utils::input::window::NotificationMode::Attention,
                    paints: Paints::default(),
                    cursor_shape: None,
                    cursor_blink: None,
                    startup_mode: StartupMode::Windowed,
                },
                clipboard_text,
            )
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
            let app = Rendering::bring_up(&options, fonts, geometry()).into_app(
                Some(connection.sender.clone()),
                connection.role,
                connection.geometry.clone(),
                "test".to_owned(),
                Settings::default(),
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

    #[test]
    fn a_close_request_asks_the_session_to_let_go_and_keeps_the_window() {
        let mut harness = Harness::new(1, true, "");
        assert!(
            !harness.app.closing(),
            "a window showing a live session waits for the server"
        );
        assert_eq!(harness.actions(), vec![Action::Detach]);
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
        harness.app.on_wheel(MouseScrollDelta::LineDelta(0.0, 1.0));
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
            section,
            ..Settings::default()
        }
    }

    impl Harness {
        fn reconfigure(&mut self, section: WindowConfig) {
            self.app.reconfigured(settings(section));
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
            font_size: Some(DEFAULT_FONT_SIZE * 2.0),
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
            ..Settings::default()
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
            ..Settings::default()
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
            },
            "the options the scene is built with did not follow the reload"
        );
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
    fn a_reload_that_changes_the_size_resolves_it_without_the_zoom() {
        let mut harness = Harness::new(0, true, "");
        harness.app.modifiers = ModifiersState::CONTROL;
        harness.press(&character("="));

        harness.reconfigure(WindowConfig {
            font_size: Some(DEFAULT_FONT_SIZE * 2.0),
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
}

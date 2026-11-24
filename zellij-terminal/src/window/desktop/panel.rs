use crate::app::desktop::tray::Anchor;
use crate::app::AppEvent;
use crate::config::window_state::Startup;
use crate::window::conductor::Input;
use crate::window::desktop::hover;
use crate::window::desktop::panel_grip;
use crate::window::desktop::start_grip;
use crate::window::desktop::Hover;
use crate::window::desktop::PanelFrame;
use crate::window::desktop::PanelWindow;
use crate::window::desktop::Pending;
use crate::window::desktop::PreparedWindow;
use crate::window::desktop::WindowOps;
use crate::window::Desktop;
use crate::window::{as_panel, terminal_app, with_activation, App, Verdict, Wake};
use winit::event::WindowEvent;
use winit::event::{ElementState, MouseButton};
use winit::event_loop::ActiveEventLoop;
use zellij_utils::app_control::PopupView;
use zellij_utils::input::actions::Action;

pub(super) fn send_panel_view(app: &App, view: Option<PopupView>) {
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

pub(super) fn view_payload(view: Option<PopupView>) -> &'static str {
    match view {
        Some(PopupView::Inbox) => "inbox",
        Some(PopupView::Sessions) => "sessions",
        Some(PopupView::Quit) => "quit",
        None => "auto",
    }
}

/// Where the panel opens, given its size in logical pixels.
pub(super) fn placement(
    event_loop: &ActiveEventLoop,
    anchor: Option<Anchor>,
    size: (f64, f64),
) -> Option<winit::dpi::Position> {
    if crate::window::is_wayland(event_loop) {
        return None;
    }
    let (x, y) = anchor_point(event_loop, anchor)?;
    let scale = scale_at(event_loop, x, y);
    let (left, top) = centred_in(work_area(event_loop, x, y), size, scale);
    Some(winit::dpi::PhysicalPosition::new(left, top).into())
}

/// The work area is in physical pixels and the panel's size in logical ones, so the size is
/// scaled to the monitor the panel opens on before it is centred.
pub(super) fn centred_in(area: (f64, f64, f64, f64), size: (f64, f64), scale: f64) -> (f64, f64) {
    let (left, top, right, bottom) = area;
    (
        centered(left, right, size.0 * scale),
        centered(top, bottom, size.1 * scale),
    )
}

pub(super) fn centered(start: f64, end: f64, length: f64) -> f64 {
    let room = (end - start).max(0.0);
    start + ((room - length) / 2.0).max(0.0)
}

/// The panel window's size in logical pixels, as the window asks for it: its cells and the
/// padding around them.
fn logical_size(prepared: &PreparedWindow) -> (f64, f64) {
    size_with(prepared, prepared.geometry.cols, prepared.geometry.rows)
}

fn size_with(prepared: &PreparedWindow, cols: usize, rows: usize) -> (f64, f64) {
    let geometry = &prepared.geometry;
    let padding = prepared.options.padding;
    (
        (geometry.cell_width * cols) as f64 + padding.horizontal() as f64,
        (geometry.cell_height * rows) as f64 + padding.vertical() as f64,
    )
}

fn sized_as(prepared: &mut PreparedWindow, frame: PanelFrame) {
    prepared.geometry.cols = frame.cols;
    prepared.geometry.rows = frame.rows;
    prepared.startup = Startup::windowed(frame.rows, frame.cols);
}

/// Where a panel the user left opens again: at the same place, pulled back inside the work area
/// if it hangs over an edge, or nowhere in particular if it no longer fits there.
fn kept_in_view(
    event_loop: &ActiveEventLoop,
    position: (f64, f64),
    size: (f64, f64),
) -> Option<winit::dpi::Position> {
    if crate::window::is_wayland(event_loop) {
        return None;
    }
    let (x, y) = position;
    let scale = scale_at(event_loop, x, y);
    let pixels = (size.0 * scale, size.1 * scale);
    let (left, top) = kept_inside(work_area(event_loop, x, y), position, pixels)?;
    Some(winit::dpi::PhysicalPosition::new(left, top).into())
}

pub(super) fn kept_inside(
    area: (f64, f64, f64, f64),
    position: (f64, f64),
    pixels: (f64, f64),
) -> Option<(f64, f64)> {
    let (left, top, right, bottom) = area;
    if pixels.0 > right - left || pixels.1 > bottom - top {
        return None;
    }
    Some((
        position.0.clamp(left, right - pixels.0),
        position.1.clamp(top, bottom - pixels.1),
    ))
}

fn frame_of(app: &App) -> Option<PanelFrame> {
    let window = app.window()?;
    let state = app.window_state();
    let position = window
        .outer_position()
        .ok()
        .map(|position| (position.x as f64, position.y as f64));
    Some(PanelFrame {
        cols: state.cols,
        rows: state.rows,
        position,
    })
}

fn anchor_point(event_loop: &ActiveEventLoop, anchor: Option<Anchor>) -> Option<(f64, f64)> {
    match anchor {
        Some(Anchor::Below { x, y }) => {
            let scale = event_loop
                .primary_monitor()
                .map(|m| m.scale_factor())
                .unwrap_or(1.0);
            Some((x * scale, y * scale))
        },
        Some(Anchor::Above { x, y } | Anchor::At { x, y }) => Some((x, y)),
        None => event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next())
            .map(|m| {
                let (p, s) = (m.position(), m.size());
                (
                    p.x as f64 + s.width as f64 / 2.0,
                    p.y as f64 + s.height as f64 / 2.0,
                )
            }),
    }
}

fn monitor_at(
    event_loop: &ActiveEventLoop,
    x: f64,
    y: f64,
) -> Option<winit::monitor::MonitorHandle> {
    event_loop
        .available_monitors()
        .find(|m| {
            let (p, s) = (m.position(), m.size());
            x >= p.x as f64
                && y >= p.y as f64
                && x < p.x as f64 + s.width as f64
                && y < p.y as f64 + s.height as f64
        })
        .or_else(|| event_loop.primary_monitor())
        .or_else(|| event_loop.available_monitors().next())
}

fn scale_at(event_loop: &ActiveEventLoop, x: f64, y: f64) -> f64 {
    monitor_at(event_loop, x, y)
        .map(|m| m.scale_factor())
        .unwrap_or(1.0)
}

pub(super) fn work_area(event_loop: &ActiveEventLoop, x: f64, y: f64) -> (f64, f64, f64, f64) {
    crate::window::tray_work_area(x, y).unwrap_or_else(|| monitor_area(event_loop, x, y))
}

pub(super) fn monitor_area(event_loop: &ActiveEventLoop, x: f64, y: f64) -> (f64, f64, f64, f64) {
    monitor_at(event_loop, x, y)
        .map(|m| {
            let (p, s) = (m.position(), m.size());
            (
                p.x as f64,
                p.y as f64,
                p.x as f64 + s.width as f64,
                p.y as f64 + s.height as f64,
            )
        })
        .unwrap_or((0.0, 0.0, f64::MAX / 4.0, f64::MAX / 4.0))
}

impl Desktop {
    /// Takes the panel away, keeping its size and place for the next time it opens.
    pub(super) fn take_panel(&mut self) -> Option<PanelWindow> {
        let panel = self.panel.take()?;
        self.panel_frame = frame_of(&panel.app).or(self.panel_frame);
        Some(panel)
    }

    pub(super) fn close_panel(&mut self) {
        if let Some(mut panel) = self.take_panel() {
            panel.app.leave();
            drop(panel.app.take_session());
        }
    }

    pub(super) fn bring_panel_forward(&mut self, token: Option<String>) {
        let window = self.panel.as_ref().and_then(|panel| panel.app.window());
        self.activation.bring_panel_forward(window, token);
        crate::window::activate_app();
    }

    pub(super) fn panel_area(
        &self,
        event_loop: &ActiveEventLoop,
        anchor: Option<Anchor>,
    ) -> (f64, f64) {
        let Some((x, y)) = anchor_point(event_loop, anchor) else {
            return (1280.0, 800.0);
        };
        let scale = scale_at(event_loop, x, y);
        let (left, top, right, bottom) = work_area(event_loop, x, y);
        ((right - left) / scale, (bottom - top) / scale)
    }

    pub(super) fn open_panel(
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
        let frame = self.panel_frame;
        let Some(hosting) = self.hosting() else {
            return false;
        };
        let mut prepared = match hosting.maker.panel(area) {
            Ok(prepared) => prepared,
            Err(e) => {
                report!("the inbox panel could not be prepared: {:#}", e);
                return false;
            },
        };
        let frame = frame.filter(|frame| {
            let (width, height) = size_with(&prepared, frame.cols, frame.rows);
            width <= area.0 && height <= area.1
        });
        if let Some(frame) = frame {
            sized_as(&mut prepared, frame);
        }
        let connector = hosting.maker.panel_connector();
        let geometry = prepared.geometry;
        let view_name = view_payload(view);
        self.pending.insert(
            ticket,
            Pending {
                prepared,
                anchor,
                token,
                position: frame.and_then(|frame| frame.position),
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

    pub(super) fn show_panel(
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
            position,
        } = pending;
        let size = logical_size(&prepared);
        let border = prepared.border;
        let mut made = hosting.maker.make(connection, prepared);
        made.remember = false;
        let position = position
            .and_then(|position| kept_in_view(event_loop, position, size))
            .or_else(|| placement(event_loop, anchor, size));
        let app = with_activation(terminal_app(made, ticket, proxy), token);
        let mut app = as_panel(app, position, border);
        if app.on_resumed(event_loop) == Verdict::Close {
            drop(app.take_session());
            return false;
        }
        if let Some(window) = app.window() {
            window.focus_window();
        }
        crate::window::activate_app();
        send_panel_view(&app, view);
        self.panel = Some(PanelWindow {
            app,
            pointer: None,
            on_edge: false,
        });
        self.ensure_activator(event_loop);
        true
    }

    pub(super) fn panel_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
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
            WindowEvent::KeyboardInput {
                event: ref key_event,
                is_synthetic: false,
                ..
            } if panel.app.wants_to_close(key_event) => {
                self.run(event_loop, Input::PanelClosedByUser)
            },
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_200_percent_the_panel_is_centred_by_its_size_in_pixels() {
        let above_the_taskbar = (0.0, 0.0, 2880.0, 1704.0);
        assert_eq!(
            centred_in(above_the_taskbar, (1240.0, 693.0), 2.0),
            (200.0, 159.0),
            "1240x693 logical pixels cover 2480x1386 physical ones"
        );
    }

    #[test]
    fn at_100_percent_logical_and_physical_pixels_are_the_same() {
        assert_eq!(
            centred_in((0.0, 0.0, 1920.0, 1040.0), (1240.0, 693.0), 1.0),
            (340.0, 173.5)
        );
    }

    #[test]
    fn a_panel_larger_than_the_work_area_starts_at_its_corner() {
        assert_eq!(
            centred_in((100.0, 50.0, 900.0, 650.0), (1240.0, 693.0), 1.0),
            (100.0, 50.0)
        );
    }

    #[test]
    fn a_panel_reopens_where_it_was_left() {
        let above_the_taskbar = (0.0, 0.0, 2880.0, 1704.0);
        assert_eq!(
            kept_inside(above_the_taskbar, (100.0, 200.0), (2480.0, 1386.0)),
            Some((100.0, 200.0))
        );
    }

    #[test]
    fn a_panel_left_over_an_edge_reopens_inside_the_work_area() {
        let above_the_taskbar = (0.0, 0.0, 2880.0, 1704.0);
        assert_eq!(
            kept_inside(above_the_taskbar, (1000.0, 800.0), (2480.0, 1386.0)),
            Some((400.0, 318.0)),
            "pulled back left of the screen's edge and above the taskbar"
        );
        assert_eq!(
            kept_inside(above_the_taskbar, (-50.0, -20.0), (2480.0, 1386.0)),
            Some((0.0, 0.0))
        );
    }

    #[test]
    fn a_panel_that_no_longer_fits_is_placed_afresh() {
        assert_eq!(
            kept_inside((0.0, 0.0, 1920.0, 1040.0), (0.0, 0.0), (2480.0, 1386.0)),
            None
        );
    }
}

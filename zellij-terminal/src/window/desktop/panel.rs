use crate::app::desktop::tray::Anchor;
use crate::app::AppEvent;
use crate::window::conductor::Input;
use crate::window::desktop::hover;
use crate::window::desktop::panel_grip;
use crate::window::desktop::start_grip;
use crate::window::desktop::Hover;
use crate::window::desktop::PanelWindow;
use crate::window::desktop::Pending;
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

pub(super) fn placement(
    event_loop: &ActiveEventLoop,
    anchor: Option<Anchor>,
    size: (u32, u32),
) -> Option<winit::dpi::Position> {
    if crate::window::is_wayland(event_loop) {
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

pub(super) fn centered(start: f64, end: f64, length: u32) -> f64 {
    let room = (end - start).max(0.0);
    start + ((room - length as f64) / 2.0).max(0.0)
}

pub(super) fn work_area(event_loop: &ActiveEventLoop, x: f64, y: f64) -> (f64, f64, f64, f64) {
    crate::window::tray_work_area(x, y).unwrap_or_else(|| monitor_area(event_loop, x, y))
}

pub(super) fn monitor_area(event_loop: &ActiveEventLoop, x: f64, y: f64) -> (f64, f64, f64, f64) {
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

impl Desktop {
    pub(super) fn close_panel(&mut self) {
        if let Some(mut panel) = self.panel.take() {
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

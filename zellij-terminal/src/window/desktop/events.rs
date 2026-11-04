use crate::app::Intent;
use crate::session::client_loop::Ending;
use crate::window::conductor::Input;
use crate::window::Desktop;
use crate::window::DesktopMode;
use crate::window::{Verdict, Wake};
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::WindowId;
use zellij_utils::app_control::OpenWindowRequest;

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
                    crate::window::WindowWake::Finished(Ending::Switched(to)) => {
                        let token = self.popup_token();
                        self.run(event_loop, Input::PanelSwitched { to, token });
                    },
                    crate::window::WindowWake::Finished(_) => {
                        self.run(event_loop, Input::PanelEnded)
                    },
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
            Wake::Window(key, crate::window::WindowWake::Finished(Ending::Switched(to))) => {
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

    // Quitting from the Dock or at logout reaches the app only here, after macOS has closed
    // every window: finish it as Cmd+Q would. Any other quit is already under way by now,
    // so this does nothing.
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.run(
            event_loop,
            Input::QuitRequested {
                stop_sessions: false,
            },
        );
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if matches!(self.mode, DesktopMode::Background(_)) {
            self.ensure_activator(event_loop);
            self.activation.pump();
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

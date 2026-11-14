use crate::window::application_id;
use crate::window::desktop::bring_forward;
use crate::window::desktop::grips::bring_panel_forward_with;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

pub(in crate::window) fn with_activation_token(
    attributes: WindowAttributes,
    token: Option<String>,
) -> WindowAttributes {
    use winit::platform::startup_notify::WindowAttributesExtStartupNotify;
    match token {
        Some(token) => {
            attributes.with_activation_token(winit::window::ActivationToken::from_raw(token))
        },
        None => attributes,
    }
}

pub(in crate::window) fn is_wayland(event_loop: &ActiveEventLoop) -> bool {
    use winit::platform::wayland::ActiveEventLoopExtWayland;
    event_loop.is_wayland()
}

pub(in crate::window) fn named(attributes: WindowAttributes) -> WindowAttributes {
    use winit::platform::wayland::WindowAttributesExtWayland;
    use winit::platform::x11::WindowAttributesExtX11;
    let id = application_id();
    let attributes = WindowAttributesExtX11::with_name(attributes, id, id);
    WindowAttributesExtWayland::with_name(attributes, id, id)
}

pub(in crate::window) struct Activation {
    activator: Option<crate::window::wayland_activation::Activator>,
    tried: bool,
}

impl Activation {
    pub(in crate::window) fn new() -> Self {
        Activation {
            activator: None,
            tried: false,
        }
    }

    pub(in crate::window) fn ensure<'a>(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: impl FnOnce() -> Option<&'a Window>,
    ) {
        if self.activator.is_some() || self.tried || !is_wayland(event_loop) {
            return;
        }
        match window().map(crate::window::wayland_activation::Activator::for_window) {
            Some(Ok(activator)) => {
                self.tried = true;
                self.activator = Some(activator);
            },
            Some(Err(e)) => {
                self.tried = true;
                report!(
                    "windows cannot be brought forward on this compositor: {:#}",
                    e
                );
            },
            None => {},
        }
    }

    pub(in crate::window) fn pump(&mut self) {
        if let Some(activator) = self.activator.as_mut() {
            activator.pump();
        }
    }

    pub(in crate::window) fn token(&mut self, window: Option<&Window>) -> Option<String> {
        let window = window?;
        self.activator.as_mut()?.token(window)
    }

    pub(in crate::window) fn bring_forward(
        &mut self,
        window: &Window,
        token: Option<String>,
        requester: Option<&Window>,
    ) {
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

    pub(in crate::window) fn bring_panel_forward(
        &mut self,
        window: Option<&Window>,
        token: Option<String>,
    ) {
        if let (Some(window), Some(activator)) = (window, self.activator.as_mut()) {
            let mut activate = move |token| activator.activate(window, token, None);
            bring_panel_forward_with(Some(&mut activate), token);
        }
    }
}

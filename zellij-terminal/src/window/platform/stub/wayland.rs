use crate::window::application_id;
use crate::window::desktop::bring_forward;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes};

pub(in crate::window) fn with_activation_token(
    attributes: WindowAttributes,
    _token: Option<String>,
) -> WindowAttributes {
    attributes
}

pub(in crate::window) fn is_wayland(_event_loop: &ActiveEventLoop) -> bool {
    false
}

pub(in crate::window) fn named(attributes: WindowAttributes) -> WindowAttributes {
    let _ = application_id();
    attributes
}

pub(in crate::window) struct Activation;

impl Activation {
    pub(in crate::window) fn new() -> Self {
        Activation
    }

    pub(in crate::window) fn ensure<'a>(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window: impl FnOnce() -> Option<&'a Window>,
    ) {
    }

    pub(in crate::window) fn pump(&mut self) {}

    pub(in crate::window) fn token(&mut self, _window: Option<&Window>) -> Option<String> {
        None
    }

    pub(in crate::window) fn bring_forward(
        &mut self,
        window: &Window,
        token: Option<String>,
        _requester: Option<&Window>,
    ) {
        bring_forward(window, None, token);
    }

    pub(in crate::window) fn bring_panel_forward(
        &mut self,
        _window: Option<&Window>,
        _token: Option<String>,
    ) {
    }
}

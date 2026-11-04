use crate::config::window_state::WindowState;
use crate::session::connection::Detacher;
use std::path::PathBuf;
use winit::window::{Window, WindowAttributes};

pub(in crate::window) fn square_without_system_border(_window: &Window) {}

pub(in crate::window) fn skip_taskbar(attributes: WindowAttributes) -> WindowAttributes {
    attributes
}

pub(in crate::window) fn tray_work_area(_x: f64, _y: f64) -> Option<(f64, f64, f64, f64)> {
    None
}

#[derive(Default)]
pub(in crate::window) struct WindowMemory;

impl WindowMemory {
    pub(in crate::window) fn share(
        &self,
        _path: Option<&PathBuf>,
        _state: impl FnOnce() -> WindowState,
        _windowed_known: bool,
    ) {
    }
}

pub(in crate::window) struct Shell;

impl Shell {
    pub(in crate::window) fn attach(
        _window: &Window,
        _memory: &WindowMemory,
        _detacher: Option<Detacher>,
    ) -> Self {
        Shell
    }
}

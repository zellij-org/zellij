use crate::config::window_state::WindowState;
use crate::session::connection::Detacher;
use std::path::PathBuf;
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Window, WindowAttributes};

pub(in crate::window) fn skip_taskbar(attributes: WindowAttributes) -> WindowAttributes {
    use winit::platform::windows::WindowAttributesExtWindows;
    attributes
        .with_skip_taskbar(true)
        .with_undecorated_shadow(true)
}

pub(in crate::window) fn square_without_system_border(window: &Window) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    };
    use winit::raw_window_handle::RawWindowHandle;
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
    let corners = DWMWCP_DONOTROUND;
    let color = DWMWA_COLOR_NONE;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &corners as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&corners) as u32,
        );
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR as u32,
            &color as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&color) as u32,
        );
    }
}

pub(in crate::window) fn tray_work_area(x: f64, y: f64) -> Option<(f64, f64, f64, f64)> {
    crate::app::desktop::tray::work_area(x as i32, y as i32)
}

#[derive(Default)]
pub(in crate::window) struct WindowMemory {
    pub(in crate::window) shared: crate::windows_shell::session_end::SharedRemembered,
}

impl WindowMemory {
    pub(in crate::window) fn share(
        &self,
        path: Option<&PathBuf>,
        state: impl FnOnce() -> WindowState,
        windowed_known: bool,
    ) {
        let remembered = path.map(|path| crate::windows_shell::session_end::Remembered {
            path: path.clone(),
            state: state(),
            windowed_known,
        });
        *self.shared.lock().unwrap_or_else(|e| e.into_inner()) = remembered;
    }
}

pub(in crate::window) struct Shell {
    _taskbar: Option<crate::windows_shell::taskbar::TaskbarEntry>,
    _session_end: Option<crate::windows_shell::session_end::SessionEnd>,
}

impl Shell {
    pub(in crate::window) fn attach(
        window: &Window,
        memory: &WindowMemory,
        detacher: Option<Detacher>,
    ) -> Self {
        Shell {
            _taskbar: crate::windows_shell::taskbar::describe(window),
            _session_end: crate::windows_shell::session_end::watch(
                window,
                memory.shared.clone(),
                detacher,
            ),
        }
    }
}

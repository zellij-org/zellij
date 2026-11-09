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

/// Windows keeps a window the app opens at another program's request behind the active one.
/// Focusing it brings it to the front, as the panel already does.
pub(in crate::window) fn bring_opened_window_forward(window: &Window) {
    window.focus_window();
}

/// Windows shows a window as soon as it exists and paints it white until the app draws its
/// first frame. So a new window is created hidden, then shown cloaked, which keeps it off the
/// screen while it otherwise works as usual, and uncloaked once that first frame is drawn.
pub(in crate::window) fn created_hidden(attributes: WindowAttributes) -> WindowAttributes {
    attributes.with_visible(false)
}

/// Shows a window created hidden but keeps it off the screen, and tells whether it now waits
/// for `uncloak`.
pub(in crate::window) fn show_cloaked(window: &Window) -> bool {
    let cloaked = set_cloak(window, true);
    window.set_visible(true);
    cloaked
}

pub(in crate::window) fn uncloak(window: &Window) {
    unsafe { windows_sys::Win32::Graphics::Dwm::DwmFlush() };
    set_cloak(window, false);
}

fn set_cloak(window: &Window, cloaked: bool) -> bool {
    use windows_sys::Win32::Foundation::BOOL;
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
    let Some(hwnd) = hwnd_of(window) else {
        return false;
    };
    let value: BOOL = cloaked.into();
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CLOAK as u32,
            &value as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&value) as u32,
        )
    };
    result >= 0
}

fn hwnd_of(window: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use winit::raw_window_handle::RawWindowHandle;
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    Some(handle.hwnd.get() as windows_sys::Win32::Foundation::HWND)
}

pub(in crate::window) fn square_without_system_border(window: &Window) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    };
    let Some(hwnd) = hwnd_of(window) else {
        return;
    };
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

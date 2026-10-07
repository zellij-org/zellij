use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{WM_ENDSESSION, WM_QUERYENDSESSION};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::connection::Detacher;
use crate::window_state::{self, WindowState};

const SUBCLASS_ID: usize = 0x7a65_6c6c;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remembered {
    pub path: PathBuf,
    pub state: WindowState,
    pub windowed_known: bool,
}

pub type SharedRemembered = Arc<Mutex<Option<Remembered>>>;

struct Watch {
    remembered: SharedRemembered,
    detacher: Option<Detacher>,
}

impl Watch {
    fn session_ending(&self) {
        let remembered = self
            .remembered
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(remembered) = remembered {
            window_state::store(
                &remembered.path,
                window_state::to_remember(
                    remembered.state,
                    remembered.windowed_known,
                    &remembered.path,
                ),
            );
        }
        if let Some(detacher) = &self.detacher {
            let _ = detacher.detach();
        }
    }
}

pub struct SessionEnd {
    hwnd: HWND,
    watch: *mut Watch,
}

pub fn watch(
    window: &Window,
    remembered: SharedRemembered,
    detacher: Option<Detacher>,
) -> Option<SessionEnd> {
    let hwnd = match window.window_handle().map(|handle| handle.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get() as HWND,
        _ => return None,
    };
    let watching = watch_window(
        hwnd,
        Watch {
            remembered,
            detacher,
        },
    );
    if watching.is_none() {
        eprintln!(
            "zellij-window: signing out or shutting down will not save the window size, as the \
             window could not watch for it"
        );
    }
    watching
}

fn watch_window(hwnd: HWND, watch: Watch) -> Option<SessionEnd> {
    let watch = Box::into_raw(Box::new(watch));
    if unsafe { SetWindowSubclass(hwnd, Some(on_message), SUBCLASS_ID, watch as usize) } == 0 {
        drop(unsafe { Box::from_raw(watch) });
        return None;
    }
    Some(SessionEnd { hwnd, watch })
}

impl Drop for SessionEnd {
    fn drop(&mut self) {
        unsafe {
            RemoveWindowSubclass(self.hwnd, Some(on_message), SUBCLASS_ID);
            drop(Box::from_raw(self.watch));
        }
    }
}

unsafe extern "system" fn on_message(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass: usize,
    watch: usize,
) -> LRESULT {
    match message {
        WM_QUERYENDSESSION => 1,
        WM_ENDSESSION if wparam != 0 => {
            (*(watch as *const Watch)).session_ending();
            0
        },
        _ => DefSubclassProc(hwnd, message, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_state::Shown;
    use std::path::Path;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SendMessageW,
    };

    struct HiddenWindow(HWND);

    impl HiddenWindow {
        fn new() -> Self {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    100,
                    100,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!hwnd.is_null(), "failed to create a window to test with");
            HiddenWindow(hwnd)
        }
    }

    impl Drop for HiddenWindow {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0) };
        }
    }

    fn remembered(path: &Path, state: WindowState, windowed_known: bool) -> SharedRemembered {
        Arc::new(Mutex::new(Some(Remembered {
            path: path.to_path_buf(),
            state,
            windowed_known,
        })))
    }

    #[test]
    fn signing_out_lets_the_session_end_and_saves_the_window_state() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("window-state.json");
        let state = WindowState {
            cols: 90,
            rows: 30,
            state: Shown::Maximized,
        };
        let window = HiddenWindow::new();
        let watching = watch_window(
            window.0,
            Watch {
                remembered: remembered(&path, state, true),
                detacher: None,
            },
        )
        .expect("the window could not be watched");

        assert_eq!(
            unsafe { SendMessageW(window.0, WM_QUERYENDSESSION, 0, 0) },
            1,
            "the window must never hold up the end of a Windows session"
        );
        unsafe { SendMessageW(window.0, WM_ENDSESSION, 0, 0) };
        assert_eq!(
            window_state::load_from(&path),
            None,
            "a session end that was called off saves nothing"
        );
        unsafe { SendMessageW(window.0, WM_ENDSESSION, 1, 0) };
        assert_eq!(window_state::load_from(&path), Some(state));
        drop(watching);
    }

    #[test]
    fn without_a_windowed_size_this_run_the_saved_one_is_kept_at_a_session_end() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("window-state.json");
        window_state::save_to(
            &path,
            WindowState {
                cols: 70,
                rows: 20,
                state: Shown::Windowed,
            },
        )
        .unwrap();
        let window = HiddenWindow::new();
        let _watching = watch_window(
            window.0,
            Watch {
                remembered: remembered(
                    &path,
                    WindowState {
                        cols: 120,
                        rows: 40,
                        state: Shown::Maximized,
                    },
                    false,
                ),
                detacher: None,
            },
        )
        .unwrap();

        unsafe { SendMessageW(window.0, WM_ENDSESSION, 1, 0) };
        assert_eq!(
            window_state::load_from(&path),
            Some(WindowState {
                cols: 70,
                rows: 20,
                state: Shown::Maximized,
            })
        );
    }
}

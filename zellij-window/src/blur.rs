use winit::window::Window;

pub fn set(window: &Window, blur: bool) {
    window.set_blur(blur);
    if !platform::set(window, blur) && blur {
        announce_once();
    }
}

fn announce_once() {
    use std::sync::Once;
    static ANNOUNCED: Once = Once::new();
    ANNOUNCED.call_once(|| {
        eprintln!(
            "zellij-window: blur needs the Acrylic backdrop of Windows 11 22H2 or later, so the \
             window stays unblurred"
        );
    });
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;

    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
        DWM_SYSTEMBACKDROP_TYPE,
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    pub fn backdrop(blur: bool) -> DWM_SYSTEMBACKDROP_TYPE {
        if blur {
            DWMSBT_TRANSIENTWINDOW
        } else {
            DWMSBT_NONE
        }
    }

    pub fn set(window: &Window, blur: bool) -> bool {
        let Ok(handle) = window.window_handle() else {
            return false;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return false;
        };
        let backdrop = backdrop(blur);
        let result = unsafe {
            DwmSetWindowAttribute(
                handle.hwnd.get() as *mut c_void,
                DWMWA_SYSTEMBACKDROP_TYPE as u32,
                &backdrop as *const DWM_SYSTEMBACKDROP_TYPE as *const c_void,
                std::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
            )
        };
        result >= 0
    }
}

#[cfg(not(windows))]
mod platform {
    use winit::window::Window;

    pub fn set(_window: &Window, _blur: bool) -> bool {
        true
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::platform::backdrop;
    use windows_sys::Win32::Graphics::Dwm::{DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW};

    #[test]
    fn blur_asks_for_the_acrylic_backdrop_and_no_blur_takes_it_away() {
        assert_eq!(
            backdrop(true),
            DWMSBT_TRANSIENTWINDOW,
            "the transient-window backdrop is the Acrylic one, the one that blurs what is behind"
        );
        assert_eq!(backdrop(false), DWMSBT_NONE);
    }
}

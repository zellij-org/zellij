pub fn ring() {
    if !platform::ring() {
        announce_once();
    }
}

fn announce_once() {
    use std::sync::Once;
    static ANNOUNCED: Once = Once::new();
    ANNOUNCED.call_once(|| {
        eprintln!("zellij-window: this platform offers no bell; the window will only flash");
    });
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
mod platform {
    use std::ffi::{c_char, c_void};
    use std::ptr;
    use std::sync::OnceLock;

    use libloading::{Library, Symbol};

    type OpenDisplay = unsafe extern "C" fn(*const c_char) -> *mut c_void;
    type Bell = unsafe extern "C" fn(*mut c_void, i32) -> i32;
    type Flush = unsafe extern "C" fn(*mut c_void) -> i32;

    struct Ringer {
        _library: Library,
        display: *mut c_void,
        bell: Bell,
        flush: Flush,
    }

    unsafe impl Send for Ringer {}
    unsafe impl Sync for Ringer {}

    impl Ringer {
        fn open() -> Option<Self> {
            let library = unsafe { Library::new("libX11.so.6") }.ok()?;
            let display = unsafe {
                let open: Symbol<OpenDisplay> = library.get(b"XOpenDisplay\0").ok()?;
                open(ptr::null())
            };
            if display.is_null() {
                return None;
            }
            let bell = unsafe { *library.get::<Bell>(b"XBell\0").ok()? };
            let flush = unsafe { *library.get::<Flush>(b"XFlush\0").ok()? };
            Some(Self {
                _library: library,
                display,
                bell,
                flush,
            })
        }
    }

    pub fn ring() -> bool {
        static RINGER: OnceLock<Option<Ringer>> = OnceLock::new();
        let Some(ringer) = RINGER.get_or_init(Ringer::open) else {
            return false;
        };
        unsafe {
            (ringer.bell)(ringer.display, 0);
            (ringer.flush)(ringer.display);
        }
        true
    }
}

#[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "android"))))]
mod platform {
    pub fn ring() -> bool {
        false
    }
}

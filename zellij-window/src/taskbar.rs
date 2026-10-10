use std::path::Path;

use anyhow::{bail, Context, Result};
use windows::Win32::Foundation::{HWND, PROPERTYKEY};
use windows::Win32::Storage::EnhancedStorage::{
    PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand,
    PKEY_AppUserModel_RelaunchDisplayNameResource, PKEY_AppUserModel_RelaunchIconResource,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::com::{set_string, Com};
use crate::identity::identity;
use crate::launcher::window_command;

const PROPERTIES: [PROPERTYKEY; 4] = [
    PKEY_AppUserModel_ID,
    PKEY_AppUserModel_RelaunchCommand,
    PKEY_AppUserModel_RelaunchDisplayNameResource,
    PKEY_AppUserModel_RelaunchIconResource,
];

pub struct TaskbarEntry {
    store: IPropertyStore,
    _com: Com,
}

pub fn describe(window: &Window) -> Option<TaskbarEntry> {
    let described = window
        .window_handle()
        .context("the window has no handle")
        .and_then(|handle| match handle.as_raw() {
            RawWindowHandle::Win32(handle) => Ok(HWND(handle.hwnd.get() as *mut _)),
            _ => bail!("the window is not a Win32 window"),
        })
        .and_then(|hwnd| {
            let exe = std::env::current_exe().context("failed to locate the running binary")?;
            describe_window(hwnd, &exe)
        });
    match described {
        Ok(entry) => Some(entry),
        Err(e) => {
            eprintln!(
                "zellij-window: a pinned taskbar button will not reopen the window: {:#}",
                e
            );
            None
        },
    }
}

fn describe_window(hwnd: HWND, exe: &Path) -> Result<TaskbarEntry> {
    let com = Com::initialize()?;
    let store: IPropertyStore = unsafe { SHGetPropertyStoreForWindow(hwnd) }
        .context("failed to reach the window's shell properties")?;
    let entry = TaskbarEntry { store, _com: com };
    let identity = identity();
    set_string(
        &entry.store,
        &PKEY_AppUserModel_RelaunchCommand,
        &window_command(exe).command_line(),
    )?;
    set_string(
        &entry.store,
        &PKEY_AppUserModel_RelaunchDisplayNameResource,
        identity.display_name,
    )?;
    set_string(
        &entry.store,
        &PKEY_AppUserModel_RelaunchIconResource,
        &format!("{},0", exe.display()),
    )?;
    set_string(
        &entry.store,
        &PKEY_AppUserModel_ID,
        &identity.windows_app_id,
    )?;
    Ok(entry)
}

impl Drop for TaskbarEntry {
    fn drop(&mut self) {
        let empty = PROPVARIANT::default();
        for key in &PROPERTIES {
            let _ = unsafe { self.store.SetValue(key, &empty) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::com::get_string;
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow};

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
            HiddenWindow(HWND(hwnd))
        }

        fn property(&self, key: &PROPERTYKEY) -> String {
            let store: IPropertyStore = unsafe { SHGetPropertyStoreForWindow(self.0) }.unwrap();
            get_string(&store, key).unwrap()
        }
    }

    impl Drop for HiddenWindow {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0 .0) };
        }
    }

    #[test]
    fn a_pinned_window_relaunches_through_the_window_command_and_cleans_up_after_itself() {
        let _com = Com::initialize().unwrap();
        let window = HiddenWindow::new();
        let exe = Path::new(r"C:\no\such\zellij.exe");

        let entry = describe_window(window.0, exe).unwrap();
        assert_eq!(
            window.property(&PKEY_AppUserModel_RelaunchCommand),
            window_command(exe).command_line()
        );
        assert_eq!(
            window.property(&PKEY_AppUserModel_RelaunchDisplayNameResource),
            identity().display_name
        );
        assert_eq!(
            window.property(&PKEY_AppUserModel_RelaunchIconResource),
            r"C:\no\such\zellij.exe,0"
        );
        assert_eq!(
            window.property(&PKEY_AppUserModel_ID),
            "zellij.window",
            "the relaunch properties only count on a window with its own app id"
        );

        drop(entry);
        for key in &PROPERTIES {
            assert_eq!(
                window.property(key),
                "",
                "a property was left on the window after it let go of the taskbar"
            );
        }
    }
}

use std::sync::Once;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

use crate::app::bundle::WINDOWS_APP_ID;

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn set_string(key: HKEY, name: &str, value: &str) {
    let name = wide(name);
    let value = wide(value);
    unsafe {
        RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr() as *const u8,
            (value.len() * 2) as u32,
        );
    }
}

pub fn register_app_id() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let path = wide(&format!(
            "Software\\Classes\\AppUserModelId\\{}",
            WINDOWS_APP_ID
        ));
        let mut key: HKEY = std::ptr::null_mut();
        let created = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        if created != ERROR_SUCCESS {
            report!(
                "the app id for toasts could not be registered ({})",
                created
            );
            return;
        }
        set_string(key, "DisplayName", "Zellij");
        if let Ok(exe) = std::env::current_exe() {
            set_string(key, "IconUri", &exe.to_string_lossy());
        }
        unsafe {
            RegCloseKey(key);
        }
    });
}

pub fn set_process_app_id() {
    let id = wide(WINDOWS_APP_ID);
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
    }
}

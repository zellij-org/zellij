use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};
use zellij_utils::consts::ZELLIJ_CACHE_DIR;
use zellij_utils::desktop_integration::{windows_app_icon_path, windows_app_id_key};

use crate::config::identity::{identity, Identity};

/// Gives the app id the name and icon that Windows shows on toasts and in the notification
/// settings. An unpackaged app has no manifest to carry them, so they live in the registry.
pub fn register() -> Result<()> {
    let identity = identity();
    let icon = icon_path(&ZELLIJ_CACHE_DIR, &identity);
    if let Some(parent) = icon.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {:?}", parent))?;
    }
    std::fs::write(&icon, crate::window::ICON_PNG)
        .with_context(|| format!("failed to write {:?}", icon))?;
    let key = registration_key(&identity);
    for (name, value) in registration_values(&identity, &icon) {
        set_value(&key, name, &value)?;
    }
    Ok(())
}

fn registration_key(identity: &Identity) -> String {
    windows_app_id_key(identity.name)
}

fn icon_path(cache_dir: &Path, identity: &Identity) -> PathBuf {
    windows_app_icon_path(cache_dir, identity.name)
}

/// Windows reads `IconUri` as an image file, so it points at a PNG and not at the executable.
fn registration_values(identity: &Identity, icon: &Path) -> [(&'static str, String); 2] {
    [
        ("DisplayName", identity.display_name.to_owned()),
        ("IconUri", icon.display().to_string()),
    ]
}

fn set_value(key: &str, name: &str, value: &str) -> Result<()> {
    let mut handle: HKEY = std::ptr::null_mut();
    let created = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(key).as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut handle,
            std::ptr::null_mut(),
        )
    };
    if created != ERROR_SUCCESS {
        bail!("failed to open {} (error {})", key, created);
    }
    let data = wide(value);
    let written = unsafe {
        RegSetValueExW(
            handle,
            wide(name).as_ptr(),
            0,
            REG_SZ,
            data.as_ptr() as *const u8,
            (data.len() * std::mem::size_of::<u16>()) as u32,
        )
    };
    unsafe { RegCloseKey(handle) };
    if written != ERROR_SUCCESS {
        bail!("failed to set {} in {} (error {})", name, key, written);
    }
    Ok(())
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_id_is_registered_under_the_classes_key_for_unpackaged_apps() {
        assert_eq!(
            registration_key(&identity()),
            r"Software\Classes\AppUserModelId\zellij.terminal"
        );
    }

    #[test]
    fn the_registration_names_the_app_and_points_at_its_icon_in_the_cache() {
        let cache = Path::new(r"C:\Users\me\AppData\Local\Zellij\cache");
        let icon = icon_path(cache, &identity());
        assert_eq!(icon, cache.join("zellij.png"));
        assert_eq!(
            registration_values(&identity(), &icon),
            [
                ("DisplayName", "Zellij".to_owned()),
                (
                    "IconUri",
                    r"C:\Users\me\AppData\Local\Zellij\cache\zellij.png".to_owned()
                ),
            ]
        );
    }
}

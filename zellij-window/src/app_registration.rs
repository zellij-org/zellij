use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use zellij_utils::consts::ZELLIJ_CACHE_DIR;

use crate::identity::{identity, Identity};

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

pub fn unregister() -> Result<Vec<String>> {
    let identity = identity();
    let mut removed = Vec::new();
    let key = registration_key(&identity);
    let result = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, wide(&key).as_ptr()) };
    match result {
        ERROR_SUCCESS => removed.push(format!(r"HKEY_CURRENT_USER\{}", key)),
        ERROR_FILE_NOT_FOUND => {},
        error => bail!("failed to remove {} (error {})", key, error),
    }
    let icon = icon_path(&ZELLIJ_CACHE_DIR, &identity);
    match std::fs::remove_file(&icon) {
        Ok(()) => removed.push(icon.display().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(e) => return Err(e).with_context(|| format!("failed to remove {:?}", icon)),
    }
    Ok(removed)
}

fn registration_key(identity: &Identity) -> String {
    format!(
        r"Software\Classes\AppUserModelId\{}",
        identity.windows_app_id
    )
}

fn icon_path(cache_dir: &Path, identity: &Identity) -> PathBuf {
    cache_dir.join(format!("{}.png", identity.app_id))
}

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
            r"Software\Classes\AppUserModelId\zellij.window"
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

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{RegDeleteTreeW, HKEY_CURRENT_USER};
use zellij_utils::consts::ZELLIJ_CACHE_DIR;

use crate::config::identity::{identity, Identity};

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
        identity.windows_app_id()
    )
}

fn icon_path(cache_dir: &Path, identity: &Identity) -> PathBuf {
    cache_dir.join(format!("{}.png", identity.name))
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
}

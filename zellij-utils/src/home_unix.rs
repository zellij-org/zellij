use directories::BaseDirs;
use std::path::PathBuf;

const CONFIG_LOCATION: &str = ".config/zellij";

pub(crate) fn home_config_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|dirs| dirs.home_dir().join(CONFIG_LOCATION))
}

pub(crate) fn try_create_home_config_dir() {
    if let Some(user_dirs) = BaseDirs::new() {
        let config_dir = user_dirs.home_dir().join(CONFIG_LOCATION);
        if let Err(e) = std::fs::create_dir_all(config_dir) {
            log::error!("Failed to create config dir: {:?}", e);
        }
    }
}

/// System-wide data directory (e.g. `/usr/share/zellij` from distro packages).
pub(crate) fn system_data_dir() -> PathBuf {
    use crate::consts::SYSTEM_DEFAULT_DATA_DIR_PREFIX;
    std::path::Path::new(SYSTEM_DEFAULT_DATA_DIR_PREFIX).join("share/zellij")
}

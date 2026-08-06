use directories::BaseDirs;
use std::path::PathBuf;

const CONFIG_LOCATION: &str = ".config/zellij";

pub(crate) fn home_config_dir() -> Option<PathBuf> {
    if let Some(xdg_config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg_config_home.is_empty() {
            return Some(PathBuf::from(xdg_config_home).join("zellij"));
        }
    }
    BaseDirs::new().map(|dirs| dirs.home_dir().join(CONFIG_LOCATION))
}

pub(crate) fn try_create_home_config_dir() {
    if let Some(config_dir) = home_config_dir() {
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

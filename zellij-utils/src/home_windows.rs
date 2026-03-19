use std::path::PathBuf;

pub(crate) fn home_config_dir() -> Option<PathBuf> {
    // On Windows there is no ~/.config convention.
    // Return the ProjectDirs config_dir (Roaming AppData) directly.
    Some(crate::home::xdg_config_dir())
}

pub(crate) fn try_create_home_config_dir() {
    let config_dir = crate::home::xdg_config_dir();
    if let Err(e) = std::fs::create_dir_all(config_dir) {
        log::error!("Failed to create config dir: {:?}", e);
    }
}

/// System-wide data directory (`C:\ProgramData\Zellij\data`).
pub(crate) fn system_data_dir() -> PathBuf {
    use crate::consts::SYSTEM_DEFAULT_DATA_DIR_PREFIX;
    std::path::Path::new(SYSTEM_DEFAULT_DATA_DIR_PREFIX).join("data")
}

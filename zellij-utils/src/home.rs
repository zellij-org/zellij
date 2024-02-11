//!
//! # This module contain everything you'll need to access local system paths
//! containing configuration and layouts

use crate::consts::{SYSTEM_DEFAULT_DATA_DIR_PREFIX, ZELLIJ_PROJ_DIR};

#[cfg(not(test))]
use crate::consts::SYSTEM_DEFAULT_CONFIG_DIR;

use directories::BaseDirs;
use std::{path::Path, path::PathBuf};

pub(crate) const CONFIG_LOCATION: &str = ".config/zellij";

#[cfg(not(test))]
/// Goes through a predefined list and checks for an already
/// existing config directory, returns the first match
pub fn find_default_config_dir() -> Option<PathBuf> {
    default_config_dirs()
        .into_iter()
        .filter(|p| p.is_some())
        .find(|p| p.clone().unwrap().exists())
        .flatten()
}

#[cfg(test)]
pub fn find_default_config_dir() -> Option<PathBuf> {
    None
}

/// Order in which config directories are checked
#[cfg(not(test))]
pub(crate) fn default_config_dirs() -> Vec<Option<PathBuf>> {
    vec![
        home_config_dir(),
        Some(xdg_config_dir()),
        Some(Path::new(SYSTEM_DEFAULT_CONFIG_DIR).to_path_buf()),
    ]
}

/// Looks for an existing dir, uses that, else returns a
/// dir matching the config spec.
pub fn get_default_data_dir() -> PathBuf {
    [
        xdg_data_dir(),
        Path::new(SYSTEM_DEFAULT_DATA_DIR_PREFIX).join("share/zellij"),
    ]
    .into_iter()
    .find(|p| p.exists())
    .unwrap_or_else(xdg_data_dir)
}

pub fn xdg_config_dir() -> PathBuf {
    ZELLIJ_PROJ_DIR.config_dir().to_owned()
}

pub fn xdg_data_dir() -> PathBuf {
    ZELLIJ_PROJ_DIR.data_dir().to_owned()
}

pub fn home_config_dir() -> Option<PathBuf> {
    if let Some(user_dirs) = BaseDirs::new() {
        let config_dir = user_dirs.home_dir().join(Path::new(CONFIG_LOCATION));
        Some(config_dir)
    } else {
        None
    }
}

pub fn get_layout_dir(config_dir: Option<PathBuf>) -> Option<PathBuf> {
    config_dir.map(|dir| dir.join("layouts"))
}

pub fn default_layout_dir() -> Option<PathBuf> {
    find_default_config_dir().map(|dir| dir.join("layouts"))
}

pub fn get_theme_dir(config_dir: Option<PathBuf>) -> Option<PathBuf> {
    config_dir.map(|dir| dir.join("themes"))
}

pub fn default_theme_dir() -> Option<PathBuf> {
    find_default_config_dir().map(|dir| dir.join("themes"))
}

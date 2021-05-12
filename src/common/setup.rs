use crate::common::utils::consts::{
    SYSTEM_DEFAULT_CONFIG_DIR, SYSTEM_DEFAULT_DATA_DIR_PREFIX, VERSION, ZELLIJ_PROJ_DIR,
};
use crate::os_input_output::set_permissions;
use directories_next::BaseDirs;
use std::io::Write;
use std::{fs, path::Path, path::PathBuf};

const CONFIG_LOCATION: &str = ".config/zellij";

#[macro_export]
macro_rules! asset_map {
    ($($src:literal => $dst:literal),+ $(,)?) => {
        {
            let mut assets = std::collections::HashMap::new();
            $(
                assets.insert($dst, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/", $src)).to_vec());
            )+
            assets
        }
    }
}

pub mod install {
    use super::*;

    pub fn populate_data_dir(data_dir: &Path) {
        // First run installation of default plugins & layouts
        let mut assets = asset_map! {
            "assets/layouts/default.yaml" => "layouts/default.yaml",
            "assets/layouts/strider.yaml" => "layouts/strider.yaml",
        };
        assets.extend(asset_map! {
            "assets/plugins/status-bar.wasm" => "plugins/status-bar.wasm",
            "assets/plugins/tab-bar.wasm" => "plugins/tab-bar.wasm",
            "assets/plugins/strider.wasm" => "plugins/strider.wasm",
        });
        assets.insert("VERSION", VERSION.as_bytes().to_vec());

        let last_version = fs::read_to_string(data_dir.join("VERSION")).unwrap_or_default();
        let out_of_date = VERSION != last_version;

        for (path, bytes) in assets {
            let path = data_dir.join(path);
            let parent_path = path.parent().unwrap();
            fs::create_dir_all(parent_path).unwrap();
            set_permissions(parent_path).unwrap();
            if out_of_date || !path.exists() {
                fs::write(path, bytes).expect("Failed to install default assets!");
            }
        }
    }
}

#[cfg(not(test))]
/// Goes through a predefined list and checks for an already
/// existing config directory, returns the first match
pub fn find_default_config_dir() -> Option<PathBuf> {
    vec![
        home_config_dir(),
        Some(xdg_config_dir()),
        Some(Path::new(SYSTEM_DEFAULT_CONFIG_DIR).to_path_buf()),
    ]
    .into_iter()
    .filter(|p| p.is_some())
    .find(|p| p.clone().unwrap().exists())
    .flatten()
}

#[cfg(test)]
pub fn find_default_config_dir() -> Option<PathBuf> {
    None
}

/// Looks for an existing dir, uses that, else returns a
/// dir matching the config spec.
pub fn get_default_data_dir() -> PathBuf {
    vec![
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
        let config_dir = user_dirs.home_dir().join(CONFIG_LOCATION);
        Some(config_dir)
    } else {
        None
    }
}

pub fn dump_asset(asset: &[u8]) -> std::io::Result<()> {
    std::io::stdout().write_all(&asset)?;
    Ok(())
}

pub const DEFAULT_CONFIG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/",
    "assets/config/default.yaml"
));

pub fn dump_default_config() -> std::io::Result<()> {
    dump_asset(DEFAULT_CONFIG)
}

use crate::common::utils::consts::SYSTEM_DEFAULT_CONFIG_DIR;
use directories_next::{BaseDirs, ProjectDirs};
use std::io::Write;
use std::{fs, path::Path, path::PathBuf};

const VERSION: &str = env!("CARGO_PKG_VERSION");

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
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if out_of_date || !path.exists() {
            fs::write(path, bytes).expect("Failed to install default assets!");
        }
    }
}

pub fn default_config_dir() -> Option<PathBuf> {
    vec![
        Some(xdg_config_dir()),
        home_config_dir(),
        Some(Path::new(SYSTEM_DEFAULT_CONFIG_DIR).to_path_buf()),
    ]
    .into_iter()
    .filter(|p| p.is_some())
    .find(|p| p.clone().unwrap().exists())
    .flatten()
}

pub fn xdg_config_dir() -> PathBuf {
    let project_dirs = ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
    project_dirs.config_dir().to_owned()
}

pub fn home_config_dir() -> Option<PathBuf> {
    if let Some(user_dirs) = BaseDirs::new() {
        let config_dir = user_dirs.home_dir().join("/.config/zellij");
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

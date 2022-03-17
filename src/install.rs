#[cfg(not(feature = "disable_automatic_asset_installation"))]
use std::fs;
use std::path::Path;
#[cfg(not(feature = "disable_automatic_asset_installation"))]
use zellij_utils::{consts::VERSION, shared::set_permissions};

#[cfg(not(feature = "disable_automatic_asset_installation"))]
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

#[cfg(not(feature = "disable_automatic_asset_installation"))]
pub(crate) fn populate_data_dir(data_dir: &Path) {
    // First run installation of default plugins & layouts
    let mut assets = asset_map! {
        "assets/plugins/status-bar.wasm" => "plugins/status-bar.wasm",
        "assets/plugins/tab-bar.wasm" => "plugins/tab-bar.wasm",
        "assets/plugins/strider.wasm" => "plugins/strider.wasm",
    };
    assets.insert("VERSION", VERSION.as_bytes().to_vec());

    let last_version = fs::read_to_string(data_dir.join("VERSION")).unwrap_or_default();
    let out_of_date = VERSION != last_version;

    for (path, bytes) in assets {
        let path = data_dir.join(path);
        // TODO: Is the [path.parent()] really necessary here?
        // We already have the path and the parent through `data_dir`
        if let Some(parent_path) = path.parent() {
            fs::create_dir_all(parent_path).unwrap_or_else(|e| log::error!("{:?}", e));
            set_permissions(parent_path).unwrap_or_else(|e| log::error!("{:?}", e));
            if out_of_date || !path.exists() {
                fs::write(path, bytes)
                    .unwrap_or_else(|e| log::error!("Failed to install default assets! {:?}", e));
            }
        } else {
            log::error!("The path {:?} has no parent directory", path);
        }
    }
}

#[cfg(feature = "disable_automatic_asset_installation")]
pub(crate) fn populate_data_dir(_data_dir: &Path) {}

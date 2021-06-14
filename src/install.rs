use std::fs;
use std::path::Path;
use zellij_utils::{consts::VERSION, shared::set_permissions};

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

pub(crate) fn populate_data_dir(data_dir: &Path) {
    // First run installation of default plugins & layouts
    let mut assets = asset_map! {
        "assets/layouts/default.yaml" => "layouts/default.yaml",
        "assets/layouts/strider.yaml" => "layouts/strider.yaml",
        "assets/layouts/disable-status-bar.yaml" => "layouts/disable-status-bar.yaml",
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

//! Zellij program-wide constants.

use directories::ProjectDirs;
use include_dir::{include_dir, Dir};
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use std::path::PathBuf;
use uuid::Uuid;

pub const ZELLIJ_CONFIG_FILE_ENV: &str = "ZELLIJ_CONFIG_FILE";
pub const ZELLIJ_CONFIG_DIR_ENV: &str = "ZELLIJ_CONFIG_DIR";
pub const ZELLIJ_LAYOUT_DIR_ENV: &str = "ZELLIJ_LAYOUT_DIR";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_SCROLL_BUFFER_SIZE: usize = 10_000;
pub static SCROLL_BUFFER_SIZE: OnceCell<usize> = OnceCell::new();
pub static DEBUG_MODE: OnceCell<bool> = OnceCell::new();

pub const SYSTEM_DEFAULT_CONFIG_DIR: &str = "/etc/zellij";
pub const SYSTEM_DEFAULT_DATA_DIR_PREFIX: &str = system_default_data_dir();

pub static ZELLIJ_DEFAULT_THEMES: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/themes");

pub fn session_info_cache_file_name(session_name: &str) -> PathBuf {
    session_info_folder_for_session(session_name).join("session-metadata.kdl")
}

pub fn session_layout_cache_file_name(session_name: &str) -> PathBuf {
    session_info_folder_for_session(session_name).join("session-layout.kdl")
}

pub fn session_info_folder_for_session(session_name: &str) -> PathBuf {
    ZELLIJ_SESSION_INFO_CACHE_DIR.join(session_name)
}

const fn system_default_data_dir() -> &'static str {
    if let Some(data_dir) = std::option_env!("PREFIX") {
        data_dir
    } else {
        "/usr"
    }
}

lazy_static! {
    pub static ref ZELLIJ_PROJ_DIR: ProjectDirs =
        ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
    pub static ref ZELLIJ_CACHE_DIR: PathBuf = ZELLIJ_PROJ_DIR.cache_dir().to_path_buf();
    pub static ref ZELLIJ_SESSION_CACHE_DIR: PathBuf = ZELLIJ_PROJ_DIR
        .cache_dir()
        .to_path_buf()
        .join(format!("{}", Uuid::new_v4()));
    pub static ref ZELLIJ_PLUGIN_PERMISSIONS_CACHE: PathBuf =
        ZELLIJ_CACHE_DIR.join("permissions.kdl");
    pub static ref ZELLIJ_SESSION_INFO_CACHE_DIR: PathBuf =
        ZELLIJ_CACHE_DIR.join(VERSION).join("session_info");
    pub static ref ZELLIJ_STDIN_CACHE_FILE: PathBuf =
        ZELLIJ_CACHE_DIR.join(VERSION).join("stdin_cache");
    pub static ref ZELLIJ_PLUGIN_ARTIFACT_DIR: PathBuf = ZELLIJ_CACHE_DIR.join(VERSION);
}

pub const FEATURES: &[&str] = &[
    #[cfg(feature = "disable_automatic_asset_installation")]
    "disable_automatic_asset_installation",
];

#[cfg(not(target_family = "wasm"))]
pub use not_wasm::*;

#[cfg(not(target_family = "wasm"))]
mod not_wasm {
    use lazy_static::lazy_static;
    use std::collections::HashMap;
    use std::path::PathBuf;

    // Convenience macro to add plugins to the asset map (see `ASSET_MAP`)
    //
    // Plugins are taken from:
    //
    // - `zellij-utils/assets/plugins`: When building in release mode OR when the
    //   `plugins_from_target` feature IS NOT set
    // - `zellij-utils/../target/wasm32-wasi/debug`: When building in debug mode AND the
    //   `plugins_from_target` feature IS set
    macro_rules! add_plugin {
        ($assets:expr, $plugin:literal) => {
            $assets.insert(
                PathBuf::from("plugins").join($plugin),
                #[cfg(any(not(feature = "plugins_from_target"), not(debug_assertions)))]
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/plugins/",
                    $plugin
                ))
                .to_vec(),
                #[cfg(all(feature = "plugins_from_target", debug_assertions))]
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../target/wasm32-wasi/debug/",
                    $plugin
                ))
                .to_vec(),
            );
        };
    }

    lazy_static! {
        // Zellij asset map
        pub static ref ASSET_MAP: HashMap<PathBuf, Vec<u8>> = {
            let mut assets = std::collections::HashMap::new();
            add_plugin!(assets, "compact-bar.wasm");
            add_plugin!(assets, "status-bar.wasm");
            add_plugin!(assets, "tab-bar.wasm");
            add_plugin!(assets, "strider.wasm");
            add_plugin!(assets, "session-manager.wasm");
            assets
        };
    }
}

#[cfg(unix)]
pub use unix_only::*;

#[cfg(unix)]
mod unix_only {
    use super::*;
    use crate::envs;
    pub use crate::shared::set_permissions;
    use lazy_static::lazy_static;
    use nix::unistd::Uid;
    use std::env::temp_dir;

    pub const ZELLIJ_SOCK_MAX_LENGTH: usize = 108;

    lazy_static! {
        static ref UID: Uid = Uid::current();
        pub static ref ZELLIJ_TMP_DIR: PathBuf = temp_dir().join(format!("zellij-{}", *UID));
        pub static ref ZELLIJ_TMP_LOG_DIR: PathBuf = ZELLIJ_TMP_DIR.join("zellij-log");
        pub static ref ZELLIJ_TMP_LOG_FILE: PathBuf = ZELLIJ_TMP_LOG_DIR.join("zellij.log");
        pub static ref ZELLIJ_SOCK_DIR: PathBuf = {
            let mut ipc_dir = envs::get_socket_dir().map_or_else(
                |_| {
                    ZELLIJ_PROJ_DIR
                        .runtime_dir()
                        .map_or_else(|| ZELLIJ_TMP_DIR.clone(), |p| p.to_owned())
                },
                PathBuf::from,
            );
            ipc_dir.push(VERSION);
            ipc_dir
        };
    }
}

//! Zellij program-wide constants.

use directories_next::ProjectDirs;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use std::path::PathBuf;

pub const ZELLIJ_CONFIG_FILE_ENV: &str = "ZELLIJ_CONFIG_FILE";
pub const ZELLIJ_CONFIG_DIR_ENV: &str = "ZELLIJ_CONFIG_DIR";
pub const ZELLIJ_LAYOUT_DIR_ENV: &str = "ZELLIJ_LAYOUT_DIR";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_SCROLL_BUFFER_SIZE: usize = 10_000;
pub static SCROLL_BUFFER_SIZE: OnceCell<usize> = OnceCell::new();
pub static DEBUG_MODE: OnceCell<bool> = OnceCell::new();

pub const SYSTEM_DEFAULT_CONFIG_DIR: &str = "/etc/zellij";
pub const SYSTEM_DEFAULT_DATA_DIR_PREFIX: &str = system_default_data_dir();

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
}

pub const FEATURES: &[&str] = &[
    #[cfg(feature = "disable_automatic_asset_installation")]
    "disable_automatic_asset_installation",
];

#[cfg(unix)]
pub use unix_only::*;

#[cfg(unix)]
mod unix_only {
    use super::*;
    use crate::envs;
    use crate::shared::set_permissions;
    use lazy_static::lazy_static;
    use nix::unistd::Uid;
    use std::{env::temp_dir, fs};

    lazy_static! {
        static ref UID: Uid = Uid::current();
        pub static ref ZELLIJ_IPC_PIPE: PathBuf = {
            let mut sock_dir = ZELLIJ_SOCK_DIR.clone();
            fs::create_dir_all(&sock_dir).unwrap();
            set_permissions(&sock_dir, 0o700).unwrap();
            sock_dir.push(envs::get_session_name().unwrap());
            sock_dir
        };
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

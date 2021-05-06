//! Zellij program-wide constants.

use crate::os_input_output::set_permissions;
use directories_next::ProjectDirs;
use lazy_static::lazy_static;
use nix::unistd::Uid;
use std::path::PathBuf;
use std::{env, fs};

pub const ZELLIJ_CONFIG_FILE_ENV: &str = "ZELLIJ_CONFIG_FILE";
pub const ZELLIJ_CONFIG_DIR_ENV: &str = "ZELLIJ_CONFIG_DIR";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const SYSTEM_DEFAULT_CONFIG_DIR: &str = "/etc/zellij";
pub const SYSTEM_DEFAULT_DATA_DIR_PREFIX: &str = system_default_data_dir();

const fn system_default_data_dir() -> &'static str {
    if let Some(data_dir) = std::option_env!("PREFIX") {
        data_dir
    } else {
        &"/usr"
    }
}

lazy_static! {
    static ref UID: Uid = Uid::current();
    pub static ref SESSION_NAME: String = names::Generator::default().next().unwrap();
    pub static ref ZELLIJ_PROJ_DIR: ProjectDirs =
        ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
    pub static ref ZELLIJ_IPC_PIPE: PathBuf = {
        let mut ipc_dir = env::var("ZELLIJ_SOCKET_DIR").map_or_else(
            |_| {
                ZELLIJ_PROJ_DIR
                    .runtime_dir()
                    .map_or_else(|| ZELLIJ_TMP_DIR.clone(), |p| p.to_owned())
            },
            PathBuf::from,
        );
        ipc_dir.push(VERSION);
        fs::create_dir_all(&ipc_dir).unwrap();
        set_permissions(&ipc_dir);
        ipc_dir.push(&*SESSION_NAME);
        ipc_dir
    };
    pub static ref ZELLIJ_TMP_DIR: PathBuf =
        PathBuf::from("/tmp/zellij-".to_string() + &format!("{}", *UID));
    pub static ref ZELLIJ_TMP_LOG_DIR: PathBuf = ZELLIJ_TMP_DIR.join("zellij-log");
    pub static ref ZELLIJ_TMP_LOG_FILE: PathBuf = ZELLIJ_TMP_LOG_DIR.join("log.txt");
}

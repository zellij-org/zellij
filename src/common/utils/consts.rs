//! Zellij program-wide constants.

use directories_next::ProjectDirs;
use lazy_static::lazy_static;
use nix::unistd::Uid;
use std::path::PathBuf;

pub const ZELLIJ_TMP_DIR: &str = "/tmp/zellij";
pub const ZELLIJ_TMP_LOG_DIR: &str = "/tmp/zellij/zellij-log";
pub const ZELLIJ_TMP_LOG_FILE: &str = "/tmp/zellij/zellij-log/log.txt";

lazy_static! {
    static ref UID: Uid = Uid::current();
    pub static ref SESSION_NAME: String = names::Generator::default().next().unwrap();
    pub static ref ZELLIJ_IPC_PIPE: PathBuf = {
        let project_dir = ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
        let mut ipc_dir = project_dir
            .runtime_dir()
            .map(|p| p.to_owned())
            .unwrap_or_else(|| PathBuf::from("/tmp/zellij-".to_string() + &format!("{}", *UID)));
        std::fs::create_dir_all(&ipc_dir).unwrap();
        ipc_dir.push(&*SESSION_NAME);
        ipc_dir
    };
}

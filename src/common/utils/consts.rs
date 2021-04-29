//! Zellij program-wide constants.

pub const ZELLIJ_TMP_DIR: &str = "/tmp/zellij";
pub const ZELLIJ_TMP_LOG_DIR: &str = "/tmp/zellij/zellij-log";
pub const ZELLIJ_TMP_LOG_FILE: &str = "/tmp/zellij/zellij-log/log.txt";
pub const ZELLIJ_IPC_PIPE: &str = "/tmp/zellij/ipc";

pub const ZELLIJ_CONFIG_FILE_ENV: &str = "ZELLIJ_CONFIG_FILE";
pub const ZELLIJ_CONFIG_DIR_ENV: &str = "ZELLIJ_CONFIG_DIR";

// TODO: ${PREFIX} argument in makefile
pub const SYSTEM_DEFAULT_CONFIG_DIR: &str = "/etc/zellij";

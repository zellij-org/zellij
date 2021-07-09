//! Zellij logging utility functions.

use std::{
    fs,
    io::{self, prelude::*},
    os::unix::io::RawFd,
    path::{Path, PathBuf},
};

use log::info;

use crate::consts::{ZELLIJ_TMP_LOG_DIR, ZELLIJ_TMP_LOG_FILE};
use crate::shared::set_permissions;

pub fn configure_logger() {
    log4rs::init_file("zellij-utils/assets/config/log4rs.yml", Default::default()).unwrap();
    info!("Zellij logger initialized");
}

pub fn atomic_create_file(file_name: &Path) -> io::Result<()> {
    let _ = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(file_name)?;
    set_permissions(file_name)
}

pub fn atomic_create_dir(dir_name: &Path) -> io::Result<()> {
    let result = if let Err(e) = fs::create_dir(dir_name) {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            Ok(())
        } else {
            Err(e)
        }
    } else {
        Ok(())
    };
    if result.is_ok() {
        set_permissions(dir_name)?;
    }
    result
}

pub fn debug_log_to_file(mut message: String) -> io::Result<()> {
    message.push('\n');
    debug_log_to_file_without_newline(message)
}

pub fn debug_log_to_file_without_newline(message: String) -> io::Result<()> {
    atomic_create_file(&*ZELLIJ_TMP_LOG_FILE)?;
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&*ZELLIJ_TMP_LOG_FILE)?;
    file.write_all(message.as_bytes())
}

pub fn _debug_log_to_file_pid_3(message: String, pid: RawFd) -> io::Result<()> {
    if pid == 3 {
        debug_log_to_file(message)
    } else {
        Ok(())
    }
}

pub fn _delete_log_file() -> io::Result<()> {
    if fs::metadata(&*ZELLIJ_TMP_LOG_FILE).is_ok() {
        fs::remove_file(&*ZELLIJ_TMP_LOG_FILE)
    } else {
        Ok(())
    }
}

pub fn _delete_log_dir() -> io::Result<()> {
    if fs::metadata(&*ZELLIJ_TMP_LOG_DIR).is_ok() {
        fs::remove_dir_all(&*ZELLIJ_TMP_LOG_DIR)
    } else {
        Ok(())
    }
}

pub fn debug_to_file(message: &[u8], pid: RawFd) -> io::Result<()> {
    let mut path = PathBuf::new();
    path.push(&*ZELLIJ_TMP_LOG_DIR);
    path.push(format!("zellij-{}.log", pid.to_string()));

    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)?;
    set_permissions(&path)?;
    file.write_all(message)
}

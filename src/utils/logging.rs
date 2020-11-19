use std::{
    fs,
    io::{self, prelude::*},
    os::unix::io::RawFd,
    path::PathBuf,
};

use crate::utils::consts::{MOSAIC_TMP_LOG_DIR, MOSAIC_TMP_LOG_FILE};

pub fn debug_log_to_file(message: String) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(MOSAIC_TMP_LOG_FILE)?;
    file.write_all(message.as_bytes())?;
    file.write_all("\n".as_bytes())?;

    Ok(())
}

pub fn debug_log_to_file_pid_0(message: String, pid: RawFd) -> io::Result<()> {
    if pid == 0 {
        debug_log_to_file(message)?;
    }

    Ok(())
}

pub fn delete_log_file() -> io::Result<()> {
    if fs::metadata(MOSAIC_TMP_LOG_FILE).is_ok() {
        fs::remove_file(MOSAIC_TMP_LOG_FILE)?;
    }

    Ok(())
}

pub fn delete_log_dir() -> io::Result<()> {
    if fs::metadata(MOSAIC_TMP_LOG_DIR).is_ok() {
        fs::remove_dir_all(MOSAIC_TMP_LOG_DIR)?;
    }
    fs::create_dir_all(MOSAIC_TMP_LOG_DIR)?;

    Ok(())
}

pub fn debug_to_file(message: u8, pid: RawFd) -> io::Result<()> {
    let mut path = PathBuf::new();
    path.push(MOSAIC_TMP_LOG_DIR);
    path.push(format!("mosaic-{}.log", pid.to_string()));

    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?;
    file.write_all(&[message])?;

    Ok(())
}

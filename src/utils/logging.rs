use std::{
    fs,
    env::temp_dir,
    io::{self, prelude::*},
    os::unix::io::RawFd
};

use crate::utils::consts::{MOSAIC_TMP_LOG_FILE, MOSAIC_TMP_LOG_DIR};

pub fn _debug_log_to_file(message: String) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(MOSAIC_TMP_LOG_FILE)?;
    file.write_all(message.as_bytes())?;
    file.write_all("\n".as_bytes())?;

    Ok(())
}

pub fn _debug_log_to_file_pid_0(message: String, pid: RawFd) -> io::Result<()> {
    if pid == 0 {
        _debug_log_to_file(message)?;
    }

    Ok(())
}

pub fn _delete_log_files() -> io::Result<()> {
    if fs::metadata(MOSAIC_TMP_LOG_DIR).is_ok() {
        fs::remove_dir_all(MOSAIC_TMP_LOG_DIR)?;
    }
    fs::create_dir_all(MOSAIC_TMP_LOG_DIR)?;

    Ok(())
}

pub fn _debug_to_file(message: u8, pid: RawFd) -> io::Result<()> {
    let mut path = temp_dir();
    path.push(MOSAIC_TMP_LOG_DIR);
    path.push(
        [
            String::from("mosaic-"),
            pid.to_string(),
            String::from(".log"),
        ]
        .concat(),
    );

    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?;
    file.write_all(&[message])?;

    Ok(())
}

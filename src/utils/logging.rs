use std::{
    fs,
    io::prelude::*,
    os::unix::io::RawFd
};

use crate::utils::consts::{MOSAIC_TMP_LOG_FILE, MOSAIC_TMP_LOG_DIR};

pub fn _debug_log_to_file(message: String) {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(MOSAIC_TMP_LOG_FILE)
        .unwrap();
    file.write_all(message.as_bytes()).unwrap();
    file.write_all("\n".as_bytes()).unwrap();
}

pub fn _debug_log_to_file_pid_0(message: String, pid: RawFd) {
    if pid == 0 {
        _debug_log_to_file(message);
    }
}

pub fn _delete_log_files() -> std::io::Result<()> {
    if fs::metadata(MOSAIC_TMP_LOG_DIR).is_ok() {
        fs::remove_dir_all(MOSAIC_TMP_LOG_DIR)?;
    } else {
        fs::create_dir_all(MOSAIC_TMP_LOG_DIR)?;
    }
    Ok(())
}

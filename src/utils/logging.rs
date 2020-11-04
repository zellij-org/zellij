use std::fs::OpenOptions;
use std::io::prelude::*;
use std::os::unix::io::RawFd;

use crate::utils::consts::MOSAIC_TMP_LOG_FILE;

pub fn _debug_log_to_file(message: String) {
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(MOSAIC_TMP_LOG_FILE)
        .unwrap();
    file.write_all(message.as_bytes()).unwrap();
    file.write_all("\n".as_bytes()).unwrap();
}

pub fn _debug_log_to_file_pid_0(message: String, pid: RawFd) {
    if pid == 0 {
        _debug_log_to_file(message)
    }
}

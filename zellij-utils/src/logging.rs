//! Zellij logging utility functions.

use std::{
    fs,
    io::{self, prelude::*},
    os::unix::io::RawFd,
    path::{Path, PathBuf},
};

use log::{info, LevelFilter};

use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config, Logger, Root};
use log4rs::encode::pattern::PatternEncoder;

use crate::consts::{ZELLIJ_TMP_LOG_DIR, ZELLIJ_TMP_LOG_FILE};
use crate::shared::set_permissions;

pub fn configure_logger() {
    // {n} means platform dependent newline
    // module is padded to exactly 25 bytes and thread is padded to be between 10 and 15 bytes.
    let file_pattern = "{highlight({level:<6})} |{module:<25.25}| {date(%Y-%m-%d %H:%M:%S.%3f)} [{thread:<10.15}] [{file}:{line}]: {message} {n}";

    // default zellij appender, should be used across most of the codebase.
    let log_file = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(file_pattern)))
        .append(true)
        .build(ZELLIJ_TMP_LOG_DIR.join("zellij.log"))
        .unwrap();

    // plugin appender. To be used in loggin_pipe to forward stderr output from plugins. We do some formatting
    // in logging_pipe to print plugin name as 'module' and plugin_id instead of thread.
    let log_plugin = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{highlight({level:<6})} {message} {n}",
        )))
        .append(true)
        .build(ZELLIJ_TMP_LOG_DIR.join("zellij.log"))
        .unwrap();

    // Set the default logging level to "info" and log it to zellij.log file
    // Decrease verbosity for `wasmer_compiler_cranelift` module because it has a lot of useless info logs
    // For `zellij_server::logging_pipe`, we use custom format as we use logging macros to forward stderr output from plugins
    let config = Config::builder()
        .appender(Appender::builder().build("logFile", Box::new(log_file)))
        .appender(Appender::builder().build("logPlugin", Box::new(log_plugin)))
        .logger(
            Logger::builder()
                .appender("logFile")
                .build("wasmer_compiler_cranelift", LevelFilter::Warn),
        )
        .logger(
            Logger::builder()
                .appender("logPlugin")
                .additive(false)
                .build("zellij_server::logging_pipe", LevelFilter::Trace),
        )
        .build(Root::builder().appender("logFile").build(LevelFilter::Info))
        .unwrap();

    let _ = log4rs::init_config(config).unwrap();

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

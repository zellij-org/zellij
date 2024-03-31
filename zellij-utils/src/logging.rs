//! Zellij logging utility functions.

use std::{
    fs,
    io::{self, prelude::*},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::io::RawFd;

use log::LevelFilter;

use log4rs::append::rolling_file::{
    policy::compound::{
        roll::fixed_window::FixedWindowRoller, trigger::size::SizeTrigger, CompoundPolicy,
    },
    RollingFileAppender,
};
use log4rs::config::{Appender, Config, Logger, Root};
use log4rs::encode::pattern::PatternEncoder;

use crate::consts::{ZELLIJ_TMP_DIR, ZELLIJ_TMP_LOG_DIR, ZELLIJ_TMP_LOG_FILE};
use crate::shared::set_permissions;

const LOG_MAX_BYTES: u64 = 1024 * 1024 * 16; // 16 MiB per log

pub fn configure_logger() {
    atomic_create_dir(&*ZELLIJ_TMP_DIR).unwrap();
    atomic_create_dir(&*ZELLIJ_TMP_LOG_DIR).unwrap();
    atomic_create_file(&*ZELLIJ_TMP_LOG_FILE).unwrap();

    let trigger = SizeTrigger::new(LOG_MAX_BYTES);
    let roller = FixedWindowRoller::builder()
        .build(
            ZELLIJ_TMP_LOG_DIR
                .join("zellij.log.old.{}")
                .to_str()
                .unwrap(),
            1,
        )
        .unwrap();

    // {n} means platform dependent newline
    // module is padded to exactly 25 bytes and thread is padded to be between 10 and 15 bytes.
    let file_pattern = "{highlight({level:<6})} |{module:<25.25}| {date(%Y-%m-%d %H:%M:%S.%3f)} [{thread:<10.15}] [{file}:{line}]: {message} {n}";

    // default zellij appender, should be used across most of the codebase.
    let log_file = RollingFileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(file_pattern)))
        .build(
            &*ZELLIJ_TMP_LOG_FILE,
            Box::new(CompoundPolicy::new(
                Box::new(trigger),
                Box::new(roller.clone()),
            )),
        )
        .unwrap();

    // plugin appender. To be used in logging_pipe to forward stderr output from plugins. We do some formatting
    // in logging_pipe to print plugin name as 'module' and plugin_id instead of thread.
    let log_plugin = RollingFileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{highlight({level:<6})} {message} {n}",
        )))
        .build(
            &*ZELLIJ_TMP_LOG_FILE,
            Box::new(CompoundPolicy::new(Box::new(trigger), Box::new(roller))),
        )
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
        .build(
            Root::builder()
                .appender("logFile")
                .build(LevelFilter::Debug),
        )
        .unwrap();

    let _ = log4rs::init_config(config).unwrap();
}

pub fn atomic_create_file(file_name: &Path) -> io::Result<()> {
    let _ = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(file_name)?;
    set_permissions(file_name, 0o600)
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
        set_permissions(dir_name, 0o700)?;
    }
    result
}

#[cfg(unix)]
pub fn debug_to_file(message: &[u8], pid: RawFd) -> io::Result<()> {
    let mut path = PathBuf::new();
    path.push(&*ZELLIJ_TMP_LOG_DIR);
    path.push(format!("zellij-{}.log", pid));

    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)?;
    set_permissions(&path, 0o600)?;
    file.write_all(message)
}
#[cfg(windows)]
pub fn debug_to_file(message: &[u8]) -> io::Result<()> {
    let mut path = PathBuf::new();
    path.push(&*ZELLIJ_TMP_LOG_DIR);
    path.push(format!("zellij-{}.log", "placeholder"));

    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)?;
    set_permissions(&path, 0o600)?;
    file.write_all(message)
}

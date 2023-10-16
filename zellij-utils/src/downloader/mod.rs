pub mod download;

#[cfg(not(target_family = "wasm"))]
pub mod downloader;

use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DownloaderError {
    #[error("Request error: {0}, URL: {1}")]
    Request(reqwest::Error, String),
    #[error("Metadata error: {0}, File: {1}")]
    Metadata(std::io::Error, PathBuf),
    #[error("Io error: {0}, File: {1}")]
    Io(std::io::Error, PathBuf),
}

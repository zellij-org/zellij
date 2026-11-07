// LOCAL PATCH (isahc removal, 2026-07-15): downloads go through ureq, a
// blocking HTTP client with no background agent threads. isahc's curl agent
// threads SIGSEGV sporadically (dmesg: "isahc-agent-N ... segfault ... in
// zellij") and took down every session server they spawned in. Blocking I/O
// runs on tokio's blocking pool via spawn_blocking, never on the executor
// threads.
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use url::Url;

#[derive(Error, Debug)]
pub enum DownloaderError {
    #[error("RequestError: {0}")]
    Request(#[from] Box<ureq::Error>),
    #[error("IoError: {0}")]
    Io(#[source] std::io::Error),
    #[error("StdIoError: {0}")]
    StdIoError(#[from] std::io::Error),
    #[error("File name cannot be found in URL: {0}")]
    NotFoundFileName(String),
    #[error("Failed to parse URL body: {0}")]
    InvalidUrlBody(String),
}

#[derive(Debug, Clone, Default)]
pub struct Downloader {
    location: PathBuf,
    // the whole thing is an Arc/Mutex so that Downloader is thread safe, and the individual values of
    // the HashMap are Arc/Mutexes (Mutexi?) to represent that individual downloads should not
    // happen concurrently
    download_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl Downloader {
    pub fn new(location: PathBuf) -> Self {
        Self {
            location,
            download_locks: Default::default(),
        }
    }

    pub async fn download(&self, url: &str, file_name: Option<&str>) -> Result<(), DownloaderError> {
        let file_name = match file_name {
            Some(name) => name.to_string(),
            None => self.parse_name(url)?,
        };

        // we do this to make sure only one download of a specific url is happening at a time
        // otherwise the downloads corrupt each other (and we waste lots of system resources)
        let download_lock = self.acquire_download_lock(&file_name).await;
        // it's important that _lock remains in scope, otherwise it gets dropped and the lock is
        // released before the download is complete
        let _lock = download_lock.lock().await;

        let file_path = self.location.join(file_name.as_str());
        if file_path.exists() {
            log::debug!("File already exists: {:?}", file_path);
            return Ok(());
        }
        let file_part_path = self.location.join(format!("{}.part", file_name));
        let url = url.to_string();
        tokio::task::spawn_blocking(move || download_blocking(&url, &file_part_path, &file_path))
            .await
            .map_err(|e| {
                DownloaderError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
            })?
    }

    pub async fn download_without_cache(url: &str) -> Result<String, DownloaderError> {
        let url = url.to_string();
        tokio::task::spawn_blocking(move || {
            let response = ureq::get(&url)
                .set("Content-Type", "application/octet-stream")
                .call()
                .map_err(Box::new)?;
            let mut downloaded_bytes: Vec<u8> = Vec::new();
            response
                .into_reader()
                .read_to_end(&mut downloaded_bytes)
                .map_err(DownloaderError::Io)?;
            log::debug!("Download complete");
            let stringified = String::from_utf8(downloaded_bytes)
                .map_err(|e| DownloaderError::InvalidUrlBody(format!("{}", e)))?;
            Ok(stringified)
        })
        .await
        .map_err(|e| DownloaderError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
    }

    /// Download the content of a URL and block for the result.
    ///
    /// Wraps the `async` call to [`download_without_cache`] such that it can be used from sync
    /// code. This is achieved by either:
    ///
    /// 1. Reusing an existing async runtime in case one is present in the current thread, or
    /// 2. Spawning a new async runtime on the current thread
    ///
    /// If neither of these works, an error is returned instead.
    ///
    /// # Note
    ///
    /// At the moment, this function is only here to bridge the gap between the async
    /// [`Downloader`] impl and the sync [`Layout`] code that ultimately calls this function. This
    /// is needed since the Layout code can't trivially be turned `async` without a lot of
    /// refactoring, while the Downloader is used in many other places with async code and can't
    /// sensibly be sync. Maybe in the future, when more code around here is async, we can drop
    /// this function.
    pub fn download_without_cache_blocking(url: &str) -> Result<String, DownloaderError> {
        let runtime_handle = match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle.clone(),
            Err(e) if e.is_missing_context() => {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .thread_name("ephemeral runtime for downloader implementation")
                    .build()
                    .map_err(DownloaderError::Io)?;
                runtime.handle().clone()
            },
            _ => {
                return Err(DownloaderError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "failed to spawn runtime for download task",
                )))
            },
        };
        runtime_handle.block_on(async move { Downloader::download_without_cache(url).await })
    }

    fn parse_name(&self, url: &str) -> Result<String, DownloaderError> {
        Url::parse(url)
            .map_err(|_| DownloaderError::NotFoundFileName(url.to_string()))?
            .path_segments()
            .ok_or_else(|| DownloaderError::NotFoundFileName(url.to_string()))?
            .last()
            .ok_or_else(|| DownloaderError::NotFoundFileName(url.to_string()))
            .map(|s| s.to_string())
    }
    async fn acquire_download_lock(&self, file_name: &String) -> Arc<Mutex<()>> {
        let mut lock_dict = self.download_locks.lock().await;
        let download_lock = lock_dict
            .entry(file_name.clone())
            .or_insert_with(|| Default::default());
        download_lock.clone()
    }
}

fn download_blocking(
    url: &str,
    file_part_path: &Path,
    file_path: &Path,
) -> Result<(), DownloaderError> {
    let (mut target, file_part_size) = {
        if file_part_path.exists() {
            let file_part = fs::OpenOptions::new()
                .append(true)
                .open(file_part_path)
                .map_err(DownloaderError::Io)?;

            let file_part_size = file_part.metadata().map_err(DownloaderError::Io)?.len();

            log::debug!("Resuming download from {} bytes", file_part_size);

            (file_part, file_part_size)
        } else {
            let file_part = fs::File::create(file_part_path).map_err(DownloaderError::Io)?;

            (file_part, 0)
        }
    };
    let response = ureq::get(url)
        .set("Content-Type", "application/octet-stream")
        .set("Range", &format!("bytes={}-", file_part_size))
        .call()
        .map_err(Box::new)?;
    std::io::copy(&mut response.into_reader(), &mut target).map_err(DownloaderError::Io)?;

    log::debug!("Download complete: {:?}", file_part_path);

    fs::rename(file_part_path, file_path).map_err(DownloaderError::Io)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[ignore]
    #[tokio::test]
    async fn test_download_ok() {
        let location = tempdir().expect("Failed to create temp directory");
        let location_path = location.path();

        let downloader = Downloader::new(location_path.to_path_buf());
        let result = downloader
            .download(
                "https://github.com/imsnif/monocle/releases/download/0.39.0/monocle.wasm",
                Some("monocle.wasm"),
            )
            .await
            .is_ok();

        assert!(result);
        assert!(location_path.join("monocle.wasm").exists());

        location.close().expect("Failed to close temp directory");
    }

    #[ignore]
    #[tokio::test]
    async fn test_download_without_file_name() {
        let location = tempdir().expect("Failed to create temp directory");
        let location_path = location.path();

        let downloader = Downloader::new(location_path.to_path_buf());
        let result = downloader
            .download(
                "https://github.com/imsnif/multitask/releases/download/0.38.2v2/multitask.wasm",
                None,
            )
            .await
            .is_ok();

        assert!(result);
        assert!(location_path.join("multitask.wasm").exists());

        location.close().expect("Failed to close temp directory");
    }
}

use isahc::http::StatusCode;
use isahc::prelude::*;
use isahc::{config::RedirectPolicy, HttpClient, Request};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::{io::AsyncWriteExt as _, sync::Mutex};
use tokio_stream::StreamExt as _;
use tokio_util::compat::FuturesAsyncReadCompatExt as _;
use tokio_util::io::ReaderStream;
use url::Url;

const STREAM_BUFFER_SIZE_BYTES: usize = 65535;

#[derive(Error, Debug)]
pub enum DownloaderError {
    #[error("RequestError: {0}")]
    Request(#[from] isahc::Error),
    #[error("HttpError: {0}")]
    HttpError(#[from] isahc::http::Error),
    #[error("IoError: {0}")]
    Io(#[source] std::io::Error),
    #[error("StdIoError: {0}")]
    StdIoError(#[from] std::io::Error),
    #[error("File name cannot be found in URL: {0}")]
    NotFoundFileName(String),
    #[error("Failed to parse URL body: {0}")]
    InvalidUrlBody(String),
    #[error("HTTP client could not be initialized")]
    ClientUnavailable,
    #[error("Unexpected HTTP status: {0}")]
    UnexpectedStatus(u16),
}

#[derive(Debug, Clone)]
pub struct Downloader {
    client: Option<HttpClient>,
    location: PathBuf,
    // the whole thing is an Arc/Mutex so that Downloader is thread safe, and the individual values of
    // the HashMap are Arc/Mutexes (Mutexi?) to represent that individual downloads should not
    // happen concurrently
    download_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl Default for Downloader {
    fn default() -> Self {
        Self {
            client: HttpClient::builder()
                // TODO: timeout?
                .redirect_policy(RedirectPolicy::Follow)
                .build()
                .ok(),
            location: PathBuf::from(""),
            download_locks: Default::default(),
        }
    }
}

impl Downloader {
    pub fn new(location: PathBuf) -> Self {
        Self {
            client: HttpClient::builder()
                // TODO: timeout?
                .redirect_policy(RedirectPolicy::Follow)
                .build()
                .ok(),
            location,
            download_locks: Default::default(),
        }
    }

    pub async fn download(
        &self,
        url: &str,
        file_name: Option<&str>,
    ) -> Result<(), DownloaderError> {
        let Some(client) = &self.client else {
            log::error!("No Http client found, cannot perform requests - this is likely a misconfiguration of isahc::HttpClient");
            return Err(DownloaderError::ClientUnavailable);
        };
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
        let (mut target, mut file_part_size) = {
            if file_part_path.exists() {
                let file_part = tokio::fs::OpenOptions::new()
                    .append(true)
                    .write(true)
                    .open(&file_part_path)
                    .await
                    .map_err(|e| DownloaderError::Io(e))?;

                let file_part_size = file_part
                    .metadata()
                    .await
                    .map_err(|e| DownloaderError::Io(e))?
                    .len();

                log::debug!("Resuming download from {} bytes", file_part_size);

                (file_part, file_part_size)
            } else {
                let file_part = tokio::fs::File::create(&file_part_path)
                    .await
                    .map_err(|e| DownloaderError::Io(e))?;

                (file_part, 0)
            }
        };
        let mut res = send_ranged(client, url, file_part_size).await?;
        if file_part_size > 0 && res.status() == StatusCode::RANGE_NOT_SATISFIABLE {
            // The partial file may already be complete, or longer than the
            // current artifact. It cannot be verified, so start over rather
            // than failing every later attempt.
            target = tokio::fs::File::create(&file_part_path)
                .await
                .map_err(DownloaderError::Io)?;
            file_part_size = 0;
            res = send_ranged(client, url, 0).await?;
        } else if file_part_size > 0
            && res.status() == StatusCode::PARTIAL_CONTENT
            && content_range_start(&res) != Some(file_part_size)
        {
            // The server answered with a different range than requested, so
            // its bytes do not continue the partial file.
            target = tokio::fs::File::create(&file_part_path)
                .await
                .map_err(DownloaderError::Io)?;
            file_part_size = 0;
            res = send_ranged(client, url, 0).await?;
        }
        ensure_success_status(res.status())?;
        if file_part_size > 0 && res.status() != StatusCode::PARTIAL_CONTENT {
            // The server ignored the range and is sending the whole artifact:
            // appending it to the partial file would cache a corrupt plugin.
            target = tokio::fs::File::create(&file_part_path)
                .await
                .map_err(DownloaderError::Io)?;
        }
        let body = res.body_mut();
        let mut stream = ReaderStream::with_capacity(body.compat(), STREAM_BUFFER_SIZE_BYTES);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(DownloaderError::Io)?;
            target
                .write_all(&chunk)
                .await
                .map_err(DownloaderError::Io)?;
        }
        // Writes to a tokio file complete in the background; flush so a
        // failed final write is reported instead of renaming a short file
        // into the cache.
        target.flush().await.map_err(DownloaderError::Io)?;

        log::debug!("Download complete: {:?}", file_part_path);

        tokio::fs::rename(file_part_path, file_path)
            .await
            .map_err(|e| DownloaderError::Io(e))?;

        Ok(())
    }
    pub async fn download_without_cache(url: &str) -> Result<String, DownloaderError> {
        let request = Request::get(url)
            .header("Content-Type", "application/octet-stream")
            .body(())?;
        let client = HttpClient::builder()
            // TODO: timeout?
            .redirect_policy(RedirectPolicy::Follow)
            .build()?;

        let mut res = client.send_async(request).await?;
        ensure_success_status(res.status())?;

        let mut downloaded_bytes: Vec<u8> = vec![];
        let body = res.body_mut();
        let mut stream = ReaderStream::with_capacity(body.compat(), STREAM_BUFFER_SIZE_BYTES);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(DownloaderError::Io)?;
            downloaded_bytes.extend_from_slice(&*chunk);
        }

        log::debug!("Download complete");
        let stringified = String::from_utf8(downloaded_bytes)
            .map_err(|e| DownloaderError::InvalidUrlBody(format!("{}", e)))?;

        Ok(stringified)
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

async fn send_ranged(
    client: &HttpClient,
    url: &str,
    start: u64,
) -> Result<isahc::Response<isahc::AsyncBody>, DownloaderError> {
    let mut request = Request::get(url).header("Content-Type", "application/octet-stream");
    if start > 0 {
        request = request.header("Range", format!("bytes={}-", start));
    }
    Ok(client.send_async(request.body(())?).await?)
}

/// The first byte offset of a `Content-Range: bytes START-END/TOTAL` header.
fn content_range_start<T>(res: &isahc::Response<T>) -> Option<u64> {
    let value = res.headers().get("content-range")?.to_str().ok()?;
    let range = value.trim().strip_prefix("bytes ")?;
    let (start, _) = range.split_once('-')?;
    start.trim().parse().ok()
}

fn ensure_success_status(status: StatusCode) -> Result<(), DownloaderError> {
    if status.is_success() {
        Ok(())
    } else {
        Err(DownloaderError::UnexpectedStatus(status.as_u16()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn success_statuses_are_accepted() {
        assert!(ensure_success_status(StatusCode::OK).is_ok());
        assert!(ensure_success_status(StatusCode::PARTIAL_CONTENT).is_ok());
    }

    #[test]
    fn non_success_statuses_are_rejected() {
        assert!(matches!(
            ensure_success_status(StatusCode::NOT_FOUND),
            Err(DownloaderError::UnexpectedStatus(404))
        ));
    }

    #[tokio::test]
    async fn a_missing_http_client_is_not_reported_as_success() {
        let location = tempdir().expect("Failed to create temp directory");
        let downloader = Downloader {
            client: None,
            location: location.path().to_path_buf(),
            download_locks: Default::default(),
        };

        assert!(matches!(
            downloader
                .download("https://example.com/plugin.wasm", Some("plugin.wasm"))
                .await,
            Err(DownloaderError::ClientUnavailable)
        ));
    }

    /// Serve one canned response per connection on loopback: the response
    /// for a request carrying a Range header, or the one without.
    async fn ranged_server(with_range: &'static str, without_range: &'static str) -> String {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut head = Vec::new();
                let mut buf = [0u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let ranged = String::from_utf8_lossy(&head)
                    .to_ascii_lowercase()
                    .contains("\r\nrange:");
                let response = if ranged { with_range } else { without_range };
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        base
    }

    async fn download_over_partial(base: &str, partial: &[u8]) -> Vec<u8> {
        let location = tempdir().expect("Failed to create temp directory");
        std::fs::write(location.path().join("plugin.wasm.part"), partial).unwrap();
        Downloader::new(location.path().to_path_buf())
            .download(&format!("{}/plugin.wasm", base), Some("plugin.wasm"))
            .await
            .expect("download");
        assert!(!location.path().join("plugin.wasm.part").exists());
        std::fs::read(location.path().join("plugin.wasm")).unwrap()
    }

    #[tokio::test]
    async fn a_resume_the_server_ignores_restarts_instead_of_appending() {
        let full = "HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nWHOLEBIN";
        let base = ranged_server(full, full).await;
        assert_eq!(download_over_partial(&base, b"WHOL").await, b"WHOLEBIN");
    }

    #[tokio::test]
    async fn an_unsatisfiable_resume_restarts_from_an_empty_file() {
        let unsatisfiable = "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */8\r\n\
                             Content-Length: 0\r\nConnection: close\r\n\r\n";
        let full = "HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nWHOLEBIN";
        let base = ranged_server(unsatisfiable, full).await;
        assert_eq!(
            download_over_partial(&base, b"WHOLEBIN-and-more").await,
            b"WHOLEBIN"
        );
    }

    #[tokio::test]
    async fn a_honoured_resume_appends_to_the_partial_file() {
        let partial = "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 4-7/8\r\n\
                       Content-Length: 4\r\nConnection: close\r\n\r\nEBIN";
        let base = ranged_server(partial, "HTTP/1.1 500 X\r\nContent-Length: 0\r\n\r\n").await;
        assert_eq!(download_over_partial(&base, b"WHOL").await, b"WHOLEBIN");
    }

    #[tokio::test]
    async fn a_resume_answered_with_another_range_restarts() {
        let wrong_range = "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-7/8\r\n\
                           Content-Length: 8\r\nConnection: close\r\n\r\nWHOLEBIN";
        let full = "HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\nWHOLEBIN";
        let base = ranged_server(wrong_range, full).await;
        assert_eq!(download_over_partial(&base, b"WHOL").await, b"WHOLEBIN");
    }

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

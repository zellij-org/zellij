use isahc::http::{header::LOCATION, StatusCode};
use isahc::prelude::*;
use isahc::{config::RedirectPolicy, AsyncBody, HttpClient, Request, Response};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tokio::{io::AsyncWriteExt as _, sync::Mutex};
use tokio_stream::StreamExt as _;
use tokio_util::compat::FuturesAsyncReadCompatExt as _;
use tokio_util::io::ReaderStream;
use url::Url;

const STREAM_BUFFER_SIZE_BYTES: usize = 65535;
const MAX_DOWNLOAD_BYTES: usize = 256 * 1024 * 1024;
/// One deadline covers the whole exchange: connecting, every redirect hop,
/// response headers and the body. A server that accepts the connection but
/// never answers is bounded by the same limit as a slow body.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// A transfer that stays below `LOW_SPEED_LIMIT_BYTES_PER_SEC` for this long
/// is aborted by curl itself. Dropping the future on the tokio deadline does
/// not stop a completely silent transfer (isahc only notices cancellation
/// when curl delivers header, body or upload data), so curl needs its own
/// bound to reap the transfer.
const LOW_SPEED_TIMEOUT: Duration = Duration::from_secs(30);
const LOW_SPEED_LIMIT_BYTES_PER_SEC: u32 = 1;
const CURL_TIMEOUT_GRACE: Duration = Duration::from_secs(5);
/// Redirects are followed manually so that every hop is re-validated
/// against the https-only policy; this bounds how many hops we accept.
const MAX_REDIRECTS: usize = 10;

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
    #[error("Invalid download file name: {0}")]
    InvalidFileName(String),
    #[error("Failed to parse URL body: {0}")]
    InvalidUrlBody(String),
    #[error("HTTP client could not be initialized")]
    ClientUnavailable,
    #[error("Unexpected HTTP status: {0}")]
    UnexpectedStatus(u16),
    #[error("Download exceeds the maximum allowed size")]
    TooLarge,
    #[error("Download exceeded the deadline")]
    TimedOut,
    #[error("Refusing insecure download URL: {0}")]
    InsecureUrl(String),
    #[error("Invalid download URL: {0}")]
    InvalidUrl(String),
    #[error("Server returned a redirect without a usable Location")]
    InvalidRedirect,
    #[error("Too many redirects")]
    TooManyRedirects,
}

#[derive(Debug, Clone)]
pub struct Downloader {
    client: Option<HttpClient>,
    location: PathBuf,
    // the whole thing is an Arc/Mutex so that Downloader is thread safe, and the individual values of
    // the HashMap are Arc/Mutexes (Mutexi?) to represent that individual downloads should not
    // happen concurrently
    download_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    deadline: Duration,
}

/// Automatic redirect following is disabled: the client would otherwise
/// follow an https URL to plain http (or through a plain-http hop whose
/// Location an on-path attacker controls) without re-checking the policy.
fn build_client() -> Option<HttpClient> {
    build_client_with_low_speed_timeout(LOW_SPEED_TIMEOUT)
}

fn build_client_with_low_speed_timeout(low_speed_timeout: Duration) -> Option<HttpClient> {
    HttpClient::builder()
        .redirect_policy(RedirectPolicy::None)
        .connect_timeout(CONNECT_TIMEOUT)
        .low_speed_timeout(LOW_SPEED_LIMIT_BYTES_PER_SEC, low_speed_timeout)
        .build()
        .ok()
}

impl Default for Downloader {
    fn default() -> Self {
        Self {
            client: build_client(),
            location: PathBuf::from(""),
            download_locks: Default::default(),
            deadline: DOWNLOAD_TIMEOUT,
        }
    }
}

impl Downloader {
    pub fn new(location: PathBuf) -> Self {
        Self {
            client: build_client(),
            location,
            download_locks: Default::default(),
            deadline: DOWNLOAD_TIMEOUT,
        }
    }

    pub async fn download(
        &self,
        url: &str,
        file_name: Option<&str>,
    ) -> Result<(), DownloaderError> {
        let url = ensure_allowed_download_url(url)?;
        let Some(client) = &self.client else {
            log::error!("No Http client found, cannot perform requests - this is likely a misconfiguration of isahc::HttpClient");
            return Err(DownloaderError::ClientUnavailable);
        };
        let file_name = match file_name {
            Some(name) => name.to_string(),
            None => self.parse_name(url.as_str())?,
        };
        if !valid_download_file_name(&file_name) {
            return Err(DownloaderError::InvalidFileName(file_name));
        }

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
        // A fresh temp per attempt. Never reopen `{name}.part`: a saved
        // prefix and a later suffix are not the same object, and that path
        // is shared across processes. Creating the file is not cancellable:
        // it completes before the deadline starts, so it can never land
        // after the lock is released.
        let temp_path = unique_temp_path(&self.location, &file_name);
        let fetched = {
            let mut target = create_new_download_file(&temp_path)?;
            let fetched = tokio::time::timeout(
                self.deadline,
                Self::fetch_to_file(client, url, &mut target, self.deadline),
            )
            .await;
            // A write the deadline cut short may still be running on the
            // blocking pool after its future was dropped. Wait for it before
            // deleting the temp or releasing the lock.
            let _ = target.flush().await;
            fetched
        };
        match fetched {
            Ok(Ok(())) => {},
            Ok(Err(error)) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(error);
            },
            Err(_) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(DownloaderError::TimedOut);
            },
        }

        log::debug!("Download complete: {:?}", temp_path);
        if let Err(error) = tokio::fs::rename(&temp_path, &file_path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(DownloaderError::Io(error));
        }

        Ok(())
    }

    async fn fetch_to_file(
        client: &HttpClient,
        url: Url,
        target: &mut tokio::fs::File,
        deadline: Duration,
    ) -> Result<(), DownloaderError> {
        let started = Instant::now();
        let mut res =
            send_following_redirects(client, &url, curl_budget(started, deadline)?).await?;
        ensure_artifact_status(res.status())?;
        let body = res.body_mut();
        let mut stream = ReaderStream::with_capacity(body.compat(), STREAM_BUFFER_SIZE_BYTES);
        let mut downloaded_bytes = 0usize;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(DownloaderError::Io)?;
            downloaded_bytes = checked_download_total(downloaded_bytes, chunk.len())?;
            target
                .write_all(&chunk)
                .await
                .map_err(DownloaderError::Io)?;
        }
        // Writes to a tokio file complete in the background; flush so a
        // failed final write is reported instead of renaming a short file
        // into the cache.
        target.flush().await.map_err(DownloaderError::Io)?;
        Ok(())
    }
    pub async fn download_without_cache(url: &str) -> Result<String, DownloaderError> {
        let client = build_client().ok_or(DownloaderError::ClientUnavailable)?;
        Self::download_without_cache_with(&client, url, DOWNLOAD_TIMEOUT).await
    }

    async fn download_without_cache_with(
        client: &HttpClient,
        url: &str,
        deadline: Duration,
    ) -> Result<String, DownloaderError> {
        let url = ensure_allowed_download_url(url)?;

        let downloaded_bytes = tokio::time::timeout(deadline, async {
            let started = Instant::now();
            let mut res =
                send_following_redirects(client, &url, curl_budget(started, deadline)?).await?;
            ensure_artifact_status(res.status())?;
            let mut downloaded_bytes: Vec<u8> = vec![];
            let body = res.body_mut();
            let mut stream = ReaderStream::with_capacity(body.compat(), STREAM_BUFFER_SIZE_BYTES);
            let mut total_bytes = 0usize;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(DownloaderError::Io)?;
                total_bytes = checked_download_total(total_bytes, chunk.len())?;
                downloaded_bytes.extend_from_slice(&chunk);
            }
            Ok::<Vec<u8>, DownloaderError>(downloaded_bytes)
        })
        .await
        .map_err(|_| DownloaderError::TimedOut)??;

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
        let client = build_client().ok_or(DownloaderError::ClientUnavailable)?;
        Self::download_without_cache_blocking_with(&client, url, DOWNLOAD_TIMEOUT)
    }

    fn download_without_cache_blocking_with(
        client: &HttpClient,
        url: &str,
        deadline: Duration,
    ) -> Result<String, DownloaderError> {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
                tokio::task::block_in_place(|| {
                    handle.block_on(Downloader::download_without_cache_with(
                        client, url, deadline,
                    ))
                })
            },
            // `Handle::block_on` panics when called from an async context, and
            // `block_in_place` cannot be used on a current-thread runtime.
            Ok(_) => Self::download_on_a_dedicated_thread(client, url, deadline),
            Err(e) if e.is_missing_context() => {
                Self::download_with_ephemeral_runtime(client, url, deadline)
            },
            _ => Err(DownloaderError::Io(std::io::Error::other(
                "failed to spawn runtime for download task",
            ))),
        }
    }

    fn download_on_a_dedicated_thread(
        client: &HttpClient,
        url: &str,
        deadline: Duration,
    ) -> Result<String, DownloaderError> {
        let client = client.clone();
        let url = url.to_owned();
        std::thread::Builder::new()
            .name("zellij downloader".to_owned())
            .spawn(move || Self::download_with_ephemeral_runtime(&client, &url, deadline))
            .map_err(DownloaderError::Io)?
            .join()
            .map_err(|_| DownloaderError::Io(std::io::Error::other("download thread panicked")))?
    }

    fn download_with_ephemeral_runtime(
        client: &HttpClient,
        url: &str,
        deadline: Duration,
    ) -> Result<String, DownloaderError> {
        // `Handle::block_on` cannot drive the timer or IO of a
        // `current_thread` runtime, so the deadline would never fire. Only
        // `Runtime::block_on` on the runtime itself does.
        let ephemeral_runtime = tokio::runtime::Builder::new_current_thread()
            .thread_name("ephemeral runtime for downloader implementation")
            .enable_all()
            .build()
            .map_err(DownloaderError::Io)?;
        ephemeral_runtime.block_on(Downloader::download_without_cache_with(
            client, url, deadline,
        ))
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

/// Send a GET, following up to `MAX_REDIRECTS` redirects. Every hop is
/// resolved against the current URL and re-validated with
/// `ensure_allowed_download_url` before it is requested.
///
/// `budget` is also handed to curl as the timeout of each request (reduced by
/// the time already spent), so that the transfer is aborted by curl even when
/// the future is dropped while the peer is completely silent.
async fn send_following_redirects(
    client: &HttpClient,
    url: &Url,
    budget: Duration,
) -> Result<Response<AsyncBody>, DownloaderError> {
    let started = Instant::now();
    let mut current = url.clone();
    for _ in 0..=MAX_REDIRECTS {
        let request = Request::get(current.as_str())
            .timeout(remaining_budget(started, budget)?)
            .header("Content-Type", "application/octet-stream");
        let res = client.send_async(request.body(())?).await?;
        if !res.status().is_redirection() {
            return Ok(res);
        }
        let location = res
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok());
        current = redirect_target(&current, location)?;
    }
    Err(DownloaderError::TooManyRedirects)
}

fn remaining_budget(started: Instant, budget: Duration) -> Result<Duration, DownloaderError> {
    match budget.checked_sub(started.elapsed()) {
        Some(remaining) if !remaining.is_zero() => Ok(remaining),
        _ => Err(DownloaderError::TimedOut),
    }
}

/// Time curl itself is given: the remaining deadline plus a grace period, so
/// the tokio deadline (which reports `TimedOut`) normally fires first and
/// curl's own timeout only acts as the backstop that reaps the transfer.
fn curl_budget(started: Instant, deadline: Duration) -> Result<Duration, DownloaderError> {
    Ok(remaining_budget(started, deadline)? + CURL_TIMEOUT_GRACE)
}

/// `{file_name}.{pid}.{nanos}.part` in the cache directory. Unique per
/// attempt so a failed download is never a resume prefix for the next one.
fn unique_temp_path(directory: &Path, file_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    directory.join(format!(
        "{}.{}.{}.part",
        file_name,
        std::process::id(),
        nanos
    ))
}

fn valid_download_file_name(file_name: &str) -> bool {
    Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        == Some(file_name)
}

/// Exclusive create. `create_new` does not append, and it does not open a
/// pre-existing `{name}.part` left by an older version.
fn create_new_download_file(path: &Path) -> Result<tokio::fs::File, DownloaderError> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(DownloaderError::Io)?;
    Ok(tokio::fs::File::from_std(file))
}

fn redirect_target(current: &Url, location: Option<&str>) -> Result<Url, DownloaderError> {
    let location = location.ok_or(DownloaderError::InvalidRedirect)?;
    let next = current
        .join(location)
        .map_err(|_| DownloaderError::InvalidRedirect)?;
    ensure_allowed_download_url(next.as_str())
}

/// Only a 200 carries the artifact. Other statuses (204 No Content, 206
/// Partial Content, 416, ...) must not be published as a plugin.
fn ensure_artifact_status(status: StatusCode) -> Result<(), DownloaderError> {
    match status {
        StatusCode::OK => Ok(()),
        _ => Err(DownloaderError::UnexpectedStatus(status.as_u16())),
    }
}

fn checked_download_total(current: usize, additional: usize) -> Result<usize, DownloaderError> {
    let total = current
        .checked_add(additional)
        .ok_or(DownloaderError::TooLarge)?;
    if total > MAX_DOWNLOAD_BYTES {
        return Err(DownloaderError::TooLarge);
    }
    Ok(total)
}

fn ensure_allowed_download_url(url: &str) -> Result<Url, DownloaderError> {
    let parsed = Url::parse(url).map_err(|_| DownloaderError::InvalidUrl(url.to_owned()))?;
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if parsed.host_str().is_some_and(is_loopback_host) => Ok(parsed),
        _ => Err(DownloaderError::InsecureUrl(url.to_owned())),
    }
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn the_blocking_entry_point_works_without_an_ambient_runtime() {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let url = format!("http://{}/layout.kdl", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept");
            let mut head = Vec::new();
            let mut buf = [0u8; 1024];
            while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                match socket.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => head.extend_from_slice(&buf[..n]),
                }
            }
            let _ = socket.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nlayout",
            );
        });
        assert_eq!(
            Downloader::download_without_cache_blocking(&url).expect("blocking download"),
            "layout"
        );
        server.join().unwrap();
    }

    #[test]
    fn artifact_statuses_are_accepted() {
        assert!(ensure_artifact_status(StatusCode::OK).is_ok());
    }

    #[test]
    fn download_file_names_cannot_escape_the_cache_directory() {
        assert!(valid_download_file_name("plugin.wasm"));
        for name in ["", ".", "..", "/tmp/plugin.wasm", "dir/plugin.wasm"] {
            assert!(!valid_download_file_name(name), "{name:?}");
        }
    }

    #[test]
    fn non_success_statuses_are_rejected() {
        assert!(matches!(
            ensure_artifact_status(StatusCode::NOT_FOUND),
            Err(DownloaderError::UnexpectedStatus(404))
        ));
        assert!(matches!(
            ensure_artifact_status(StatusCode::NO_CONTENT),
            Err(DownloaderError::UnexpectedStatus(204))
        ));
        assert!(matches!(
            ensure_artifact_status(StatusCode::PARTIAL_CONTENT),
            Err(DownloaderError::UnexpectedStatus(206))
        ));
        assert!(matches!(
            ensure_artifact_status(StatusCode::RANGE_NOT_SATISFIABLE),
            Err(DownloaderError::UnexpectedStatus(416))
        ));
    }

    #[tokio::test]
    async fn an_empty_success_response_is_not_cached() {
        let (base, _seen) = scripted_server(|_| {
            Some("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_owned())
        })
        .await;
        let location = tempdir().expect("Failed to create temp directory");
        let result = Downloader::new(location.path().to_path_buf())
            .download(&format!("{}/plugin.wasm", base), Some("plugin.wasm"))
            .await;
        assert!(
            matches!(result, Err(DownloaderError::UnexpectedStatus(204))),
            "unexpected result: {:?}",
            result
        );
        assert!(!location.path().join("plugin.wasm").exists());
        assert!(part_file_names(location.path()).is_empty());
    }

    #[test]
    fn download_byte_budget_is_enforced() {
        assert_eq!(
            checked_download_total(0, MAX_DOWNLOAD_BYTES).unwrap(),
            MAX_DOWNLOAD_BYTES
        );
        assert!(matches!(
            checked_download_total(MAX_DOWNLOAD_BYTES, 1),
            Err(DownloaderError::TooLarge)
        ));
    }

    #[test]
    fn only_https_or_loopback_http_downloads_are_allowed() {
        assert!(ensure_allowed_download_url("https://example.com/plugin.wasm").is_ok());
        assert!(ensure_allowed_download_url("http://localhost/plugin.wasm").is_ok());
        assert!(ensure_allowed_download_url("http://127.0.0.1/plugin.wasm").is_ok());
        assert!(ensure_allowed_download_url("http://[::1]/plugin.wasm").is_ok());
        assert!(matches!(
            ensure_allowed_download_url("http://example.com/plugin.wasm"),
            Err(DownloaderError::InsecureUrl(_))
        ));
    }

    #[tokio::test]
    async fn a_missing_http_client_is_not_reported_as_success() {
        let location = tempdir().expect("Failed to create temp directory");
        let downloader = Downloader {
            client: None,
            location: location.path().to_path_buf(),
            download_locks: Default::default(),
            deadline: DOWNLOAD_TIMEOUT,
        };

        assert!(matches!(
            downloader
                .download("https://example.com/plugin.wasm", Some("plugin.wasm"))
                .await,
            Err(DownloaderError::ClientUnavailable)
        ));
    }

    /// Minimal scripted HTTP/1.1 server on loopback. `respond` maps the
    /// raw request head to a full raw response (or `None` to hold the
    /// connection open without answering). Returns the base URL and the
    /// request heads seen so far.
    async fn scripted_server(
        respond: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
        use tokio::io::AsyncReadExt as _;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let respond = Arc::new(respond);
        let seen_in_server = seen.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let respond = respond.clone();
                let seen = seen_in_server.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => head.extend_from_slice(&buf[..n]),
                        }
                    }
                    let head = String::from_utf8_lossy(&head).into_owned();
                    seen.lock().unwrap().push(head.clone());
                    match respond(&head) {
                        Some(response) => {
                            let _ = socket.write_all(response.as_bytes()).await;
                            let _ = socket.shutdown().await;
                        },
                        None => {
                            // Hold the connection open, never answering.
                            tokio::time::sleep(Duration::from_secs(3600)).await;
                        },
                    }
                });
            }
        });
        (base, seen)
    }

    fn ok_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn redirect_response(location: &str) -> String {
        format!(
            "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            location
        )
    }

    #[test]
    fn redirect_targets_are_resolved_and_revalidated() {
        let current = Url::parse("https://example.com/releases/plugin.wasm").unwrap();
        assert_eq!(
            redirect_target(&current, Some("/cdn/plugin.wasm"))
                .unwrap()
                .as_str(),
            "https://example.com/cdn/plugin.wasm"
        );
        assert_eq!(
            redirect_target(&current, Some("https://cdn.example.net/p.wasm"))
                .unwrap()
                .as_str(),
            "https://cdn.example.net/p.wasm"
        );
        assert!(matches!(
            redirect_target(&current, Some("http://example.com/plugin.wasm")),
            Err(DownloaderError::InsecureUrl(_))
        ));
        assert!(matches!(
            redirect_target(&current, None),
            Err(DownloaderError::InvalidRedirect)
        ));
    }

    #[tokio::test]
    async fn a_redirect_to_insecure_http_is_refused_before_it_is_followed() {
        let (base, seen) =
            scripted_server(|_| Some(redirect_response("http://example.com/evil.wasm"))).await;
        let result = Downloader::download_without_cache(&format!("{}/plugin.wasm", base)).await;
        assert!(
            matches!(result, Err(DownloaderError::InsecureUrl(ref u)) if u.starts_with("http://example.com/")),
            "unexpected result: {:?}",
            result
        );
        assert_eq!(
            seen.lock().unwrap().len(),
            1,
            "the insecure hop must not be requested"
        );
    }

    #[tokio::test]
    async fn relative_redirects_on_an_allowed_origin_are_followed() {
        let (base, seen) = scripted_server(|head| {
            if head.starts_with("GET /final ") {
                Some(ok_response("plugin bytes"))
            } else {
                Some(redirect_response("/final"))
            }
        })
        .await;
        let body = Downloader::download_without_cache(&format!("{}/start", base))
            .await
            .expect("redirected download");
        assert_eq!(body, "plugin bytes");
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn redirect_chains_are_bounded() {
        let (base, seen) = scripted_server(|_| Some(redirect_response("/again"))).await;
        let result = Downloader::download_without_cache(&format!("{}/start", base)).await;
        assert!(
            matches!(result, Err(DownloaderError::TooManyRedirects)),
            "unexpected result: {:?}",
            result
        );
        assert_eq!(seen.lock().unwrap().len(), MAX_REDIRECTS + 1);
    }

    #[tokio::test]
    async fn a_server_that_never_sends_headers_hits_the_deadline() {
        let (base, _seen) = scripted_server(|_| None).await;
        let result = Downloader::download_without_cache_with(
            &build_client().unwrap(),
            &format!("{}/plugin.wasm", base),
            Duration::from_millis(300),
        )
        .await;
        assert!(
            matches!(result, Err(DownloaderError::TimedOut)),
            "unexpected result: {:?}",
            result
        );

        let location = tempdir().expect("Failed to create temp directory");
        let downloader = Downloader {
            client: build_client(),
            location: location.path().to_path_buf(),
            download_locks: Default::default(),
            deadline: Duration::from_millis(300),
        };
        let result = downloader
            .download(&format!("{}/plugin.wasm", base), Some("plugin.wasm"))
            .await;
        assert!(
            matches!(result, Err(DownloaderError::TimedOut)),
            "unexpected result: {:?}",
            result
        );
        assert!(!location.path().join("plugin.wasm").exists());
        assert!(part_file_names(location.path()).is_empty());
    }

    /// A raw listener that accepts connections and then says nothing.
    fn silent_server() -> (String, std::net::TcpListener) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let url = format!("http://{}/layout.kdl", listener.local_addr().unwrap());
        (url, listener)
    }

    #[test]
    fn the_blocking_entry_point_times_out_without_an_ambient_runtime() {
        // No tokio runtime is ambient on this plain test thread, so the
        // ephemeral runtime path runs. The server accepts and then stalls.
        let (url, listener) = silent_server();
        let _server = std::thread::spawn(move || {
            let held = listener.accept();
            std::thread::sleep(Duration::from_secs(5));
            drop(held);
        });
        let client = build_client().unwrap();
        let started = Instant::now();
        let result = Downloader::download_without_cache_blocking_with(
            &client,
            &url,
            Duration::from_millis(300),
        );
        assert!(
            matches!(result, Err(DownloaderError::TimedOut)),
            "unexpected result: {:?}",
            result
        );
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "the deadline did not fire promptly"
        );
    }

    #[tokio::test]
    async fn the_blocking_entry_point_works_inside_a_current_thread_runtime() {
        let (url, listener) = silent_server();
        let _server = std::thread::spawn(move || {
            let held = listener.accept();
            std::thread::sleep(Duration::from_secs(5));
            drop(held);
        });
        let client = build_client().unwrap();
        let result = Downloader::download_without_cache_blocking_with(
            &client,
            &url,
            Duration::from_millis(300),
        );
        assert!(
            matches!(result, Err(DownloaderError::TimedOut)),
            "unexpected result: {:?}",
            result
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_blocking_entry_point_works_inside_a_multi_thread_runtime() {
        let (url, listener) = silent_server();
        let _server = std::thread::spawn(move || {
            let held = listener.accept();
            std::thread::sleep(Duration::from_secs(5));
            drop(held);
        });
        let client = build_client().unwrap();
        let result = Downloader::download_without_cache_blocking_with(
            &client,
            &url,
            Duration::from_millis(300),
        );
        assert!(
            matches!(result, Err(DownloaderError::TimedOut)),
            "unexpected result: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn curl_aborts_a_silent_transfer_on_its_own() {
        // The future is not wrapped in a tokio deadline here, and the
        // overall budget is far away: only the low speed limit can end it.
        let (base, _seen) = scripted_server(|_| None).await;
        let client = build_client_with_low_speed_timeout(Duration::from_secs(1)).unwrap();
        let url = Url::parse(&format!("{}/plugin.wasm", base)).unwrap();
        let started = Instant::now();
        let result = send_following_redirects(&client, &url, Duration::from_secs(600)).await;
        assert!(
            matches!(result, Err(DownloaderError::Request(_))),
            "unexpected result: {:?}",
            result.map(|r| r.status())
        );
        assert!(started.elapsed() < Duration::from_secs(20));
    }

    #[tokio::test]
    async fn curl_enforces_the_request_budget_on_its_own() {
        let (base, _seen) = scripted_server(|_| None).await;
        let client = build_client().unwrap();
        let url = Url::parse(&format!("{}/plugin.wasm", base)).unwrap();
        let started = Instant::now();
        let result = send_following_redirects(&client, &url, Duration::from_millis(300)).await;
        match result {
            Err(DownloaderError::Request(error)) => {
                assert_eq!(error.kind(), isahc::error::ErrorKind::Timeout)
            },
            other => panic!("unexpected result: {:?}", other.map(|r| r.status())),
        }
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    fn part_file_names(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("read download dir")
            .map(|entry| {
                entry
                    .expect("dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name.ends_with(".part"))
            .collect();
        names.sort();
        names
    }

    fn assert_no_range_header(head: &str) {
        assert!(
            !head.to_ascii_lowercase().contains("\r\nrange:"),
            "request must not include a Range header: {}",
            head
        );
    }

    #[tokio::test]
    async fn a_download_does_not_send_range_or_mix_in_a_seeded_part_file() {
        let (base, seen) = scripted_server(|_| Some(ok_response("CCCCDDDD"))).await;
        let location = tempdir().expect("Failed to create temp directory");
        let seeded = location.path().join("plugin.wasm.part");
        std::fs::write(&seeded, b"AAAA").unwrap();
        Downloader::new(location.path().to_path_buf())
            .download(&format!("{}/plugin.wasm", base), Some("plugin.wasm"))
            .await
            .expect("download");
        let heads = seen.lock().unwrap();
        assert_eq!(heads.len(), 1);
        assert_no_range_header(&heads[0]);
        drop(heads);
        assert_eq!(
            std::fs::read(location.path().join("plugin.wasm")).unwrap(),
            b"CCCCDDDD"
        );
        assert_eq!(std::fs::read(&seeded).unwrap(), b"AAAA");
        assert_eq!(
            part_file_names(location.path()),
            vec!["plugin.wasm.part".to_owned()]
        );
    }

    #[tokio::test]
    async fn a_timed_out_download_leaves_no_new_part_file() {
        let (base, _seen) = scripted_server(|_| None).await;
        for deadline in [Duration::ZERO, Duration::from_millis(200)] {
            let location = tempdir().expect("Failed to create temp directory");
            let seeded = location.path().join("plugin.wasm.part");
            std::fs::write(&seeded, b"AAAA").unwrap();
            let downloader = Downloader {
                client: build_client(),
                location: location.path().to_path_buf(),
                download_locks: Default::default(),
                deadline,
            };
            let result = downloader
                .download(&format!("{}/plugin.wasm", base), Some("plugin.wasm"))
                .await;
            assert!(
                matches!(result, Err(DownloaderError::TimedOut)),
                "unexpected result: {:?}",
                result
            );
            assert!(!location.path().join("plugin.wasm").exists());
            assert_eq!(std::fs::read(&seeded).unwrap(), b"AAAA");
            assert_eq!(
                part_file_names(location.path()),
                vec!["plugin.wasm.part".to_owned()]
            );
        }
    }

    #[tokio::test]
    async fn two_sequential_attempts_publish_a_complete_body_without_concatenating() {
        let (base, seen) = scripted_server(|_| Some(ok_response("CCCCDDDD"))).await;
        let location = tempdir().expect("Failed to create temp directory");
        let downloader = Downloader::new(location.path().to_path_buf());
        let url = format!("{}/plugin.wasm", base);
        let published = location.path().join("plugin.wasm");
        downloader
            .download(&url, Some("plugin.wasm"))
            .await
            .expect("first");
        assert_eq!(std::fs::read(&published).unwrap(), b"CCCCDDDD");
        // Drop the cache file so the second attempt transfers again instead
        // of returning on the existence check.
        std::fs::remove_file(&published).unwrap();
        downloader
            .download(&url, Some("plugin.wasm"))
            .await
            .expect("second");
        assert_eq!(std::fs::read(&published).unwrap(), b"CCCCDDDD");
        let heads = seen.lock().unwrap();
        assert_eq!(heads.len(), 2);
        for head in heads.iter() {
            assert_no_range_header(head);
        }
        drop(heads);
        assert!(part_file_names(location.path()).is_empty());
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

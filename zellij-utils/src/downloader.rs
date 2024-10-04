use async_std::{
    fs,
    io::{ReadExt, WriteExt},
    stream::StreamExt,
};
use isahc::prelude::*;
use isahc::{config::RedirectPolicy, HttpClient, Request};
use std::path::PathBuf;
use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum DownloaderError {
    #[error("RequestError: {0}")]
    Request(#[from] isahc::Error),
    #[error("HttpError: {0}")]
    HttpError(#[from] isahc::http::Error),
    #[error("IoError: {0}")]
    Io(#[source] std::io::Error),
    #[error("File name cannot be found in URL: {0}")]
    NotFoundFileName(String),
    #[error("Failed to parse URL body: {0}")]
    InvalidUrlBody(String),
}

#[derive(Debug)]
pub struct Downloader {
    client: Option<HttpClient>,
    location: PathBuf,
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
        }
    }

    pub async fn download(
        &self,
        url: &str,
        file_name: Option<&str>,
    ) -> Result<(), DownloaderError> {
        let Some(client) = &self.client else {
            log::error!("No Http client found, cannot perform requests - this is likely a misconfiguration of isahc::HttpClient");
            return Ok(());
        };
        let file_name = match file_name {
            Some(name) => name.to_string(),
            None => self.parse_name(url)?,
        };
        let file_path = self.location.join(file_name.as_str());
        if file_path.exists() {
            log::debug!("File already exists: {:?}", file_path);
            return Ok(());
        }
        let file_part_path = self.location.join(format!("{}.part", file_name));
        let (mut target, file_part_size) = {
            if file_part_path.exists() {
                let file_part = fs::OpenOptions::new()
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
                let file_part = fs::File::create(&file_part_path)
                    .await
                    .map_err(|e| DownloaderError::Io(e))?;

                (file_part, 0)
            }
        };
        let request = Request::get(url)
            .header("Content-Type", "application/octet-stream")
            .header("Range", format!("bytes={}-", file_part_size))
            .body(())?;
        let mut res = client.send_async(request).await?;
        let body = res.body_mut();
        let mut stream = body.bytes();
        while let Some(byte) = stream.next().await {
            let byte = byte.map_err(|e| DownloaderError::Io(e))?;
            target
                .write(&[byte])
                .await
                .map_err(|e| DownloaderError::Io(e))?;
        }

        log::debug!("Download complete: {:?}", file_part_path);

        fs::rename(file_part_path, file_path)
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

        let mut downloaded_bytes: Vec<u8> = vec![];
        let body = res.body_mut();
        let mut stream = body.bytes();
        while let Some(byte) = stream.next().await {
            let byte = byte.map_err(|e| DownloaderError::Io(e))?;
            downloaded_bytes.push(byte);
        }

        log::debug!("Download complete");
        let stringified = String::from_utf8(downloaded_bytes)
            .map_err(|e| DownloaderError::InvalidUrlBody(format!("{}", e)))?;

        Ok(stringified)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[ignore]
    #[async_std::test]
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
    #[async_std::test]
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

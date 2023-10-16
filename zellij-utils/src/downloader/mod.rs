mod download;

use futures::{stream, StreamExt, TryStreamExt};
use reqwest::Client;
use std::path::PathBuf;
use thiserror::Error;
use tokio::{fs::File, io::AsyncWriteExt};

use crate::consts::ZELLIJ_CACHE_DIR;

use self::download::Download;

#[derive(Error, Debug)]
pub enum DownloaderError {
    #[error("Request error: {0}, URL: {1}")]
    Request(reqwest::Error, String),
    #[error("Metadata error: {0}, File: {1}")]
    Metadata(std::io::Error, PathBuf),
    #[error("Io error: {0}, File: {1}")]
    Io(std::io::Error, PathBuf),
}

pub struct Downloader {
    client: Client,
    directory: PathBuf,
    concurrent: usize,
}

impl Downloader {
    const DEFAULT_CONCURRENT: usize = 4;

    pub fn new() -> Self {
        Self {
            client: Client::new(),
            directory: ZELLIJ_CACHE_DIR.to_path_buf(),
            concurrent: Downloader::DEFAULT_CONCURRENT,
        }
    }

    pub fn set_directory(&mut self, directory: PathBuf) {
        self.directory = directory;
    }

    pub fn download(&self, downloads: &[Download]) -> Vec<Result<(), DownloaderError>> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            stream::iter(downloads)
                .map(|d| self.fetch(d))
                .buffer_unordered(4)
                .collect::<Vec<_>>()
                .await
        })
    }

    async fn fetch(&self, download: &Download) -> Result<(), DownloaderError> {
        let mut output_file_size: u64 = 0;
        // TODO: A unique path using url-based hash is required.
        let output_path = self.directory.join(&download.file_name);

        if output_path.exists() {
            output_file_size = match output_path.metadata() {
                Ok(metadata) => metadata.len(),
                Err(e) => return Err(DownloaderError::Metadata(e, output_path)),
            }
        }

        let url = download.url.as_str();
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| DownloaderError::Request(e, download.url.clone()))?;

        match response.error_for_status_ref() {
            Ok(_) => {},
            Err(e) => return Err(DownloaderError::Request(e, download.url.clone())),
        }

        let length = response.content_length().unwrap();

        if length > 0 && length == output_file_size {
            return Ok(());
        }

        let mut output_file = File::create(output_path.as_path())
            .await
            .map_err(|e| DownloaderError::Io(e, output_path.clone()))?;

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream
            .try_next()
            .await
            .map_err(|e| DownloaderError::Request(e, download.url.clone()))?
        {
            let chunk_size = chunk.len() as u64;

            output_file
                .write_all(&chunk)
                .await
                .map_err(|e| DownloaderError::Io(e, output_path.clone()))?;
        }

        Ok(())
    }
}

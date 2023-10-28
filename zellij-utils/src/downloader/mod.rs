pub mod download;

use async_std::{
    fs::{create_dir_all, File},
    io::{ReadExt, WriteExt},
    stream, task,
};
use futures::{StreamExt, TryStreamExt};
use std::path::PathBuf;
use surf::Client;
use thiserror::Error;

use self::download::Download;

#[derive(Error, Debug)]
pub enum DownloaderError {
    #[error("RequestError: {0}")]
    Request(surf::Error),
    #[error("StatusError: {0}, StatusCode: {1}")]
    Status(String, surf::StatusCode),
    #[error("IoError: {0}")]
    Io(#[source] std::io::Error),
    #[error("IoPathError: {0}, File: {1}")]
    IoPath(std::io::Error, PathBuf),
}

#[derive(Default, Debug)]
pub struct Downloader {
    client: Client,
    directory: PathBuf,
}

impl Downloader {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            client: surf::client().with(surf::middleware::Redirect::default()),
            directory,
        }
    }

    pub fn set_directory(&mut self, directory: PathBuf) {
        self.directory = directory;
    }

    pub fn download(&self, downloads: &[Download]) -> Vec<Result<(), DownloaderError>> {
        task::block_on(async {
            stream::from_iter(downloads)
                .map(|download| self.fetch(download))
                .buffer_unordered(4)
                .collect::<Vec<_>>()
                .await
        })
    }

    pub async fn fetch(&self, download: &Download) -> Result<(), DownloaderError> {
        let mut file_size: usize = 0;

        let file_path = self.directory.join(&download.file_name);

        if file_path.exists() {
            file_size = match file_path.metadata() {
                Ok(metadata) => metadata.len() as usize,
                Err(e) => return Err(DownloaderError::IoPath(e, file_path)),
            }
        }

        let response = self
            .client
            .get(&download.url)
            .await
            .map_err(|e| DownloaderError::Request(e))?;
        let status = response.status();

        if status.is_client_error() || status.is_server_error() {
            return Err(DownloaderError::Status(
                status.canonical_reason().to_string(),
                status,
            ));
        }

        let length = response.len().unwrap_or(0);
        if length > 0 && length == file_size {
            return Ok(());
        }

        let mut dest = {
            create_dir_all(&self.directory)
                .await
                .map_err(|e| DownloaderError::IoPath(e, self.directory.clone()))?;
            File::create(&file_path)
                .await
                .map_err(|e| DownloaderError::IoPath(e, file_path))?
        };

        let mut bytes = response.bytes();
        while let Some(byte) = bytes.try_next().await.map_err(DownloaderError::Io)? {
            dest.write_all(&[byte]).await.map_err(DownloaderError::Io)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    #[ignore]
    fn test_fetch_plugin() {
        let dir = tempdir().expect("could not get temp dir");
        let dir_path = dir.path();

        let downloader = Downloader::new(dir_path.to_path_buf());
        let dl = Download::from(
            "https://github.com/imsnif/monocle/releases/download/0.37.2/monocle.wasm",
        );

        let result = task::block_on(downloader.fetch(&dl));

        assert!(result.is_ok());
    }

    #[test]
    #[ignore]
    fn test_download_plugins() {
        let dir = tempdir().expect("could not get temp dir");
        let dir_path = dir.path();

        let downloader = Downloader::new(dir_path.to_path_buf());
        let downloads = vec![
            Download::from(
                "https://github.com/imsnif/monocle/releases/download/0.37.2/monocle.wasm",
            ),
            Download::from(
                "https://github.com/imsnif/multitask/releases/download/0.38.2/multitask.wasm",
            ),
        ];

        let results = downloader.download(&downloads);
        for result in results {
            assert!(result.is_ok())
        }
    }
}

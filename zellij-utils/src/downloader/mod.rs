pub mod download;

use async_std::{
    fs::File,
    io::{ReadExt, WriteExt},
    stream, task,
};
use futures::{StreamExt, TryStreamExt};
use std::path::PathBuf;
use surf::Client;
use thiserror::Error;

use self::download::Download;

#[derive(Error, Debug)]
pub enum DownloaderError {}

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

    pub fn download(&self, downloads: &[Download]) {
        let _ = task::block_on(async {
            stream::from_iter(downloads)
                .map(|download| self.fetch(download))
                .buffer_unordered(4)
                .collect::<Vec<_>>()
                .await
        });
    }

    pub async fn fetch(&self, download: &Download) -> Result<(), DownloaderError> {
        let mut file_size: usize = 0;

        let directory_path = self.directory.join(&download.url_hash);
        let file_path = directory_path.join(&download.file_name);

        if file_path.exists() {
            file_size = match file_path.metadata() {
                Ok(metadata) => metadata.len() as usize,
                Err(_) => todo!("Error"),
            }
        }

        let response = self.client.get(&download.url).await.unwrap();
        let status = response.status();

        if status.is_client_error() || status.is_server_error() {
            todo!("Error")
        }

        let length = response.len().unwrap_or(0);
        if length > 0 && length == file_size {
            return Ok(());
        }

        let mut dest = {
            std::fs::create_dir_all(directory_path).unwrap();

            File::create(file_path).await.unwrap()
        };

        let mut bytes = response.bytes();
        while let Some(byte) = bytes.try_next().await.unwrap() {
            dest.write_all(&[byte]).await.unwrap();
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

        let dl = Downloader::new(dir.into_path());

        let download = Download::from(
            "https://github.com/imsnif/monocle/releases/download/0.37.2/monocle.wasm",
        );

        let _ = task::block_on(dl.fetch(&download));
    }

    #[test]
    #[ignore]
    fn test_download_plugins() {
        let dir = tempdir().expect("could not get temp dir");

        let dl = Downloader::new(dir.into_path());

        let downloads = vec![
            Download::from(
                "https://github.com/imsnif/monocle/releases/download/0.37.2/monocle.wasm",
            ),
            Download::from(
                "https://github.com/imsnif/multitask/releases/download/0.38.2/multitask.wasm",
            ),
        ];

        dl.download(&downloads);
    }
}

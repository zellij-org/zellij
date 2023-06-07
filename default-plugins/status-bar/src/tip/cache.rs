use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::{self, PathBuf};

use serde::{Deserialize, Serialize};
use tempfile::{NamedTempFile, PersistError};
use thiserror::Error;

use zellij_tile::prelude::get_zellij_version;

#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    zellij_version: String,
    cached_data: HashMap<String, usize>,
}

#[derive(Debug)]
pub struct LocalCache {
    path: PathBuf,
    metadata: Metadata,
}

pub type LocalCacheResult = Result<LocalCache, LocalCacheError>;

#[derive(Error, Debug)]
pub enum LocalCacheError {
    // Io error
    #[error("IoError: {0}")]
    Io(#[from] io::Error),
    // Io error with path context
    #[error("IoError: {0}, File: {1}")]
    IoPath(io::Error, PathBuf),
    // Deserialization error
    #[error("Deserialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("PersistError: {0}")]
    Persist(#[from] PersistError),
}

impl LocalCache {
    fn from_json(json_cache: &str) -> Result<Metadata, LocalCacheError> {
        match serde_json::from_str::<Metadata>(json_cache) {
            Ok(metadata) => Ok(metadata),
            Err(err) => {
                if json_cache.is_empty() {
                    return Ok(Metadata {
                        zellij_version: get_zellij_version(),
                        cached_data: HashMap::new(),
                    });
                }
                Err(LocalCacheError::Serde(err))
            },
        }
    }

    fn from_file(path: PathBuf) -> LocalCacheResult {
        match OpenOptions::new()
            .read(true)
            .create(true)
            .open(path.as_path())
        {
            Ok(mut file) => {
                let mut json_cache = String::new();
                file.read_to_string(&mut json_cache)
                    .map_err(LocalCacheError::Io)?;

                let res = LocalCache::from_json(&json_cache);
                match res {
                    Ok(metadata) => Ok(LocalCache { path, metadata }),
                    Err(e) => {
                        // JSON cache file is corrupted, or using a bad schema. Try to remove it
                        std::fs::remove_file(&path)?;
                        Err(e)
                    },
                }
            },
            Err(e) => Err(LocalCacheError::IoPath(e, path)),
        }
    }

    pub fn new(path: PathBuf) -> LocalCache {
        let res = LocalCache::from_file(path.clone());
        match res {
            Ok(cache) => cache,
            Err(e) => {
                eprintln!(
                    "Error loading status-bar cache from {:?}, error: {:?}",
                    path, e
                );
                LocalCache {
                    path,
                    metadata: Metadata {
                        zellij_version: get_zellij_version(),
                        cached_data: HashMap::new(),
                    },
                }
            },
        }
    }

    fn safe_parent(p: &path::Path) -> Option<&path::Path> {
        match p.parent() {
            None => None,
            Some(x) if x.as_os_str().is_empty() => Some(path::Path::new(".")),
            x => x,
        }
    }

    fn flush(&mut self) -> Result<(), LocalCacheError> {
        match serde_json::to_string(&self.metadata) {
            Ok(json_cache) => {
                let mut file = NamedTempFile::new_in(
                    LocalCache::safe_parent(self.path.as_path()).ok_or_else(|| {
                        LocalCacheError::IoPath(
                            io::Error::new(
                                io::ErrorKind::Other,
                                "Could not get a parent path for tips cache",
                            ),
                            self.path.clone(),
                        )
                    })?,
                )
                .map_err(LocalCacheError::Io)?;
                file.write_all(json_cache.as_bytes())
                    .map_err(LocalCacheError::Io)?;
                file.persist(self.path.as_path())
                    .map_err(LocalCacheError::Persist)?;
                Ok(())
            },
            Err(e) => Err(LocalCacheError::Serde(e)),
        }
    }

    pub fn clear(&mut self) {
        self.metadata.cached_data.clear();
        match self.flush() {
            Ok(_) => (),
            Err(e) => eprintln!("Error flushing local cache to disk: {:?}", e),
        }
    }

    pub fn get_version(&self) -> &String {
        &self.metadata.zellij_version
    }

    pub fn set_version<S: Into<String>>(&mut self, version: S) {
        self.metadata.zellij_version = version.into();
    }

    pub fn is_empty(&self) -> bool {
        self.metadata.cached_data.is_empty()
    }

    pub fn get_cached_data(&self) -> &HashMap<String, usize> {
        &self.metadata.cached_data
    }

    pub fn get_cached_data_set(&self) -> HashSet<String> {
        self.get_cached_data().keys().cloned().collect()
    }

    pub fn caching<S: Into<String>>(&mut self, key: S) {
        let key = key.into();
        if let Some(item) = self.metadata.cached_data.get_mut(&key) {
            *item += 1;
        } else {
            self.metadata.cached_data.insert(key, 1);
        }
        match self.flush() {
            Ok(_) => (),
            Err(e) => eprintln!("Error flushing local cache to disk: {:?}", e),
        }
    }
}

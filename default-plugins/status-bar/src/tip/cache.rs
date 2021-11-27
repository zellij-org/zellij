use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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

#[derive(Debug)]
pub enum LocalCacheError {
    Io(io::Error),
    IoPath(io::Error),
    Serde(serde_json::Error),
}

impl LocalCache {
    fn from_json(json_cache: &str) -> Result<Metadata, LocalCacheError> {
        match serde_json::from_str::<Metadata>(json_cache) {
            Ok(metadata) => Ok(metadata),
            Err(err) => {
                if json_cache.is_empty() {
                    return Ok(Metadata {
                        zellij_version: zellij_tile::shim::get_zellij_version(),
                        cached_data: HashMap::new(),
                    });
                }
                Err(LocalCacheError::Serde(err))
            }
        }
    }

    pub fn new(path: PathBuf) -> LocalCacheResult {
        match File::create(path.as_path()) {
            Ok(mut file) => {
                let mut json_cache = String::new();
                file.read_to_string(&mut json_cache)
                    .map_err(|e| LocalCacheError::Io(e))?;

                let metadata = LocalCache::from_json(&json_cache)?;
                Ok(LocalCache { path, metadata })
            }
            Err(e) => Err(LocalCacheError::IoPath(e)),
        }
    }

    pub fn flush(&mut self) -> Result<(), LocalCacheError> {
        match serde_json::to_string(&self.metadata) {
            Ok(json_cache) => {
                let mut file =
                    File::create(self.path.as_path()).map_err(|e| LocalCacheError::IoPath(e))?;
                file.write_all(json_cache.as_bytes())
                    .map_err(|e| LocalCacheError::Io(e))?;
                Ok({})
            }
            Err(e) => Err(LocalCacheError::Serde(e)),
        }
    }

    pub fn clear(&mut self) -> Result<(), LocalCacheError> {
        self.metadata.cached_data.clear();
        self.flush()
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

    pub fn caching<S: Into<String>>(&mut self, key: S) -> Result<(), LocalCacheError> {
        let key = key.into();
        if let Some(item) = self.metadata.cached_data.get_mut(&key) {
            *item += 1;
        } else {
            self.metadata.cached_data.insert(key, 1);
        }
        self.flush()
    }
}

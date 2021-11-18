use std::error::Error;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::tip::data::TIPS;

#[derive(Debug)]
pub struct LocalTipCache {
    pub path: PathBuf,
    pub data: Vec<CachedTip>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedTip {
    name: String,
    hitted: usize,
}

impl LocalTipCache {
    pub fn load(path: &Path) -> Result<Self, Box<dyn Error>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let data: Vec<CachedTip> = serde_json::from_reader(reader)?;

        Ok(Self {
            path: path.to_path_buf(),
            data,
        })
    }

    pub fn load_or_default(path: &Path) -> Result<Self, Box<dyn Error>> {
        match Self::load(path) {
            Ok(cache) => Ok(cache),
            Err(_) => {
                let default_cache = TIPS
                    .keys()
                    .map(|name| CachedTip {
                        name: name.to_string(),
                        hitted: 0,
                    })
                    .collect();

                Ok(Self {
                    path: path.to_path_buf(),
                    data: default_cache,
                })
            }
        }
    }

    pub fn write(&self) -> Result<(), Box<dyn Error>> {
        let file = File::create(self.path.as_path())?;
        let writer = BufWriter::new(file);

        serde_json::to_writer_pretty(writer, &self.data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let path = Path::new("/tmp/test.json");
        let cache = LocalTipCache::load_or_default(path).unwrap();

        println!("{:?}", cache.data);

        cache.write().unwrap();
    }
}

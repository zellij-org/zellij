use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Error as SerdeError;

pub const CACHE_FILE_PATH: &str = "/tmp/cache.json";
pub const MAX_CACHED_COUNT: usize = 10;

#[derive(Debug)]
pub struct Cache {
    pub path: PathBuf,
    pub data: Vec<CachedTip>,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Clone)]
pub struct CachedTip {
    name: String,
    hitted: usize,
}

#[derive(Debug)]
pub enum CacheError {
    Io(std::io::Error),
    Serde(SerdeError),
}

impl Cache {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            data: Vec::new(),
        }
    }

    pub fn load(path: PathBuf) -> Result<Self, CacheError> {
        let file = match File::open(path.as_path()) {
            Ok(file) => file,
            Err(err) => return Err(CacheError::Io(err)),
        };
        let reader = BufReader::new(file);

        let data: Vec<CachedTip> = match serde_json::from_reader(reader) {
            Ok(data) => data,
            Err(err) => return Err(CacheError::Serde(err)),
        };

        Ok(Self { path, data })
    }

    pub fn save(&self) -> Result<(), CacheError> {
        let file = match File::create(self.path.as_path()) {
            Ok(file) => file,
            Err(err) => return Err(CacheError::Io(err)),
        };
        let writer = BufWriter::new(file);

        serde_json::to_writer_pretty(writer, &self.data).unwrap();
        Ok(())
    }

    pub fn add(&mut self, name: &str) -> () {
        self.data.push(CachedTip::new(name))
    }

    pub fn add_and_get_mut(&mut self, name: &str) -> &mut CachedTip {
        self.add(name);
        self.data.last_mut().unwrap()
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut CachedTip> {
        self.data.iter_mut().find(|tip| tip.name == name)
    }

    pub fn get_highest_hitted_name(&self) -> Option<String> {
        self.data
            .iter()
            .cloned()
            .filter(|tip| tip.hitted < MAX_CACHED_COUNT)
            .max_by_key(|tip| tip.hitted)
            .map(|tip| tip.name)
    }

    pub fn get_tip_names(&self) -> Vec<String> {
        self.data.iter().map(|tip| tip.name.clone()).collect()
    }

    pub fn clear_data(&mut self) -> () {
        self.data.clear();
    }
}

impl CachedTip {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            hitted: 0,
        }
    }

    pub fn one_hit(&mut self) -> () {
        self.hitted += 1;
    }
}

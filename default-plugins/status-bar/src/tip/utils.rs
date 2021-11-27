use std::path::PathBuf;

use rand::prelude::{IteratorRandom, SliceRandom};

use zellij_tile::prelude::get_zellij_version;

use super::cache::LocalCache;
use super::consts::{DEFAULT_CACHE_FILE_PATH, MAX_CACHE_HITS};
use super::data::TIPS;

macro_rules! get_name_and_caching {
    ($cache:expr) => {{
        let name = get_random_tip_name();
        $cache.caching(name.clone()).unwrap();
        return name;
    }};
    ($cache:expr, $from:expr) => {{
        let name = $from.choose(&mut rand::thread_rng()).unwrap().to_string();
        $cache.caching(name.clone()).unwrap();
        return name;
    }};
}

pub fn get_random_tip_name() -> String {
    TIPS.keys()
        .choose(&mut rand::thread_rng())
        .unwrap()
        .to_string()
}

pub fn get_cached_tip_name() -> String {
    let mut local_cache = LocalCache::new(PathBuf::from(DEFAULT_CACHE_FILE_PATH)).unwrap();

    let zellij_version = get_zellij_version();
    if zellij_version.ne(local_cache.get_version()) {
        local_cache.set_version(zellij_version);
        local_cache.clear().unwrap();
    }

    if local_cache.is_empty() {
        get_name_and_caching!(local_cache);
    }

    let usable_tips = local_cache
        .get_cached_data()
        .iter()
        .filter(|(_, &v)| v < MAX_CACHE_HITS)
        .map(|(k, _)| k.to_string())
        .collect::<Vec<String>>();

    if usable_tips.is_empty() {
        let cached_set = local_cache.get_cached_data_set();
        let diff = TIPS
            .keys()
            .cloned()
            .filter(|k| !cached_set.contains(&k.to_string()))
            .collect::<Vec<&str>>();

        if diff.len() > 0 {
            get_name_and_caching!(local_cache, diff);
        } else {
            local_cache.clear().unwrap();
            get_name_and_caching!(local_cache);
        }
    } else {
        get_name_and_caching!(local_cache, usable_tips);
    }
}

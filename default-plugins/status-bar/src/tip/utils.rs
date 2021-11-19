use std::collections::HashSet;
use std::path::PathBuf;

use crate::tip::{
    cache::{Cache, CacheError, CACHE_FILE_PATH},
    data::TIPS,
};
use rand::{seq::SliceRandom, thread_rng};

pub fn get_random_tip_name() -> &'static str {
    let mut shuffled_tips: Vec<&str> = TIPS.keys().cloned().collect();
    shuffled_tips.shuffle(&mut thread_rng());

    // It is assumed that there is at least one TIP data in the TIPS HasMap.
    shuffled_tips.first().cloned().unwrap()
}

pub fn get_cached_tip_name() -> String {
    let path = PathBuf::from(CACHE_FILE_PATH);
    if !path.exists() {
        let mut cache = Cache::new(path);
        let tip_name = get_random_tip_name();

        cache.add_and_get_mut(tip_name).one_hit();
        cache.save().unwrap();

        return tip_name.to_string();
    }

    match Cache::load(path.clone()) {
        Ok(mut cache) => {
            let name = match cache.get_highest_hitted_name() {
                Some(name) => {
                    cache.get_mut(name.as_str()).unwrap().one_hit();
                    name
                }
                None => {
                    let cached_tips: HashSet<String> = HashSet::from_iter(cache.get_tip_names());
                    let mut difference: Vec<&str> = TIPS
                        .keys()
                        .cloned()
                        .filter(|name| !cached_tips.contains(&name.to_string()))
                        .collect();
                    difference.shuffle(&mut thread_rng());

                    let tip_name = match difference.len() {
                        len if len > 0 => {
                            let tip_name = difference.first().cloned().unwrap();
                            cache.add_and_get_mut(tip_name).one_hit();
                            tip_name
                        }
                        _ => {
                            let tip_name = get_random_tip_name();
                            cache.clear_data();
                            cache.add_and_get_mut(tip_name).one_hit();
                            tip_name
                        }
                    };

                    tip_name.to_string()
                }
            };

            cache.save().unwrap();
            name
        }
        Err(CacheError::Serde(_)) => {
            // The cache file exists, but it's empty or corrupted.
            let mut cache = Cache::new(path);
            let tip_name = get_random_tip_name();

            cache.add_and_get_mut(tip_name).one_hit();
            cache.save().unwrap();

            tip_name.to_string()
        }
        Err(_) => get_random_tip_name().to_string(),
    }
}

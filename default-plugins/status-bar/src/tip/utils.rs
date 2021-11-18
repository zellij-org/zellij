use rand::{seq::SliceRandom, thread_rng};

use crate::tip::data::TIPS;

pub fn load_randomly_tip_name() -> &'static str {
    let mut tip_names: Vec<&str> = TIPS.keys().cloned().collect();
    tip_names.shuffle(&mut thread_rng());

    // It is assumed that there is at least one TIP data in the TIPS HasMap.
    tip_names.first().unwrap()
}

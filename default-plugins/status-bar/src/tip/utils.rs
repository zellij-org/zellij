use rand::{seq::SliceRandom, thread_rng};

use crate::tip::data::TIPS_DATA;

pub fn need_to_function_name() -> &'static str {
    /*
     * TODO:
     *
     * This function includes following feature:
     * 1. returns random name of TipFnMap.
     * 2. (optional) Use the cache for TipFnMap selection.
     */
    let mut shuffled_tips: Vec<&&str> = TIPS_DATA.keys().collect();
    shuffled_tips.shuffle(&mut thread_rng());

    return shuffled_tips[0];
}

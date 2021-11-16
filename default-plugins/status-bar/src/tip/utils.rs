use rand::{seq::SliceRandom, thread_rng};

use crate::tip::data::{TipFnMap, TIPS_DATA};

pub fn need_to_function_name() -> &'static TipFnMap {
    /*
     * TODO:
     *
     * This function includes following feature:
     * 1. returns random TipFnMap.
     * 2. (optional) Use the cache for TipFnMap selection.
     */
    let mut shuffled_tips: Vec<(&&str, &TipFnMap)> = TIPS_DATA.iter().collect();
    shuffled_tips.shuffle(&mut thread_rng());

    return shuffled_tips[0].1;
}

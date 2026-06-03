use fuzzy_matcher::skim::SkimMatcherV2;

#[derive(Default)]
pub struct Navigation {
    pub selector_scroll_offset: usize,
    pub fuzzy_matcher: Option<SkimMatcherV2>,
}

impl Navigation {
    pub fn matcher(&mut self) -> &mut SkimMatcherV2 {
        self.fuzzy_matcher
            .get_or_insert_with(|| SkimMatcherV2::default().use_cache(true))
    }
}

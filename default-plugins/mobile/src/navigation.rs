//! Selector navigation state shared by the Sessions and Panes screens:
//! the scroll offset into whichever selector list is open, and the fuzzy
//! matcher used to score session names / pane titles. Only one selector
//! is open at a time, so they never contend; the matcher is keyed by
//! `(haystack, needle)` internally, so a pane-title cache entry cannot
//! be mistaken for a session-name one — which is why a single shared
//! instance is safe across both screens.

use fuzzy_matcher::skim::SkimMatcherV2;

/// Shared selector list state.
#[derive(Default)]
pub struct Navigation {
    /// Scroll offset into the currently-open selector's row list. 0 =
    /// first row visible at the top of the body region. Reset to 0 every
    /// time a selector is opened. The renderer clamps stale values
    /// against the row list's actual length on the next frame.
    pub selector_scroll_offset: usize,
    /// Cached `SkimMatcherV2` for fuzzy matching across selectors.
    /// Lazily initialised on first keystroke (with `use_cache(true)`) so
    /// the matcher's internal score cache survives across renders.
    /// Wrapped in `Option` because `SkimMatcherV2` does not implement
    /// `Default`. Shared by the Sessions selector (against session
    /// names) and the Panes selector (against pane titles).
    pub fuzzy_matcher: Option<SkimMatcherV2>,
}

impl Navigation {
    /// Lazily build (or return the cached) fuzzy matcher.
    pub fn matcher(&mut self) -> &mut SkimMatcherV2 {
        self.fuzzy_matcher
            .get_or_insert_with(|| SkimMatcherV2::default().use_cache(true))
    }
}

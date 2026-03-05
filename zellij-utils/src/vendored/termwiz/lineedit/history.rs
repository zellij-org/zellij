use std::borrow::Cow;
use std::collections::VecDeque;

/// Represents a position within the history.
/// Smaller numbers are assumed to be before larger numbers,
/// and the indices are assumed to be contiguous.
pub type HistoryIndex = usize;

/// Defines the history interface for the line editor.
pub trait History {
    /// Lookup the line corresponding to an index.
    fn get(&self, idx: HistoryIndex) -> Option<Cow<str>>;
    /// Return the index for the most recently added entry.
    fn last(&self) -> Option<HistoryIndex>;
    /// Add an entry.
    /// Note that the LineEditor will not automatically call
    /// the add method.
    fn add(&mut self, line: &str);

    /// Search for a matching entry relative to the specified history index.
    fn search(
        &self,
        idx: HistoryIndex,
        style: SearchStyle,
        direction: SearchDirection,
        pattern: &str,
    ) -> Option<SearchResult>;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SearchResult<'a> {
    pub line: Cow<'a, str>,
    pub idx: HistoryIndex,
    pub cursor: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SearchStyle {
    Substring,
}

impl SearchStyle {
    /// Matches pattern against line, returning the byte index of the
    /// first matching character
    pub fn match_against(&self, pattern: &str, line: &str) -> Option<usize> {
        match self {
            Self::Substring => line.find(pattern),
        }
    }
}

/// Encodes the direction the search should take, relative to the
/// current HistoryIndex.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SearchDirection {
    /// The search goes backwards towards the smaller HistoryIndex values
    /// at the beginning of history.
    Backwards,
    /// The search goes forwards towarrds the larger HistoryIndex values
    /// at the end of history.
    Forwards,
}

impl SearchDirection {
    /// Given a history index, compute the next value in the
    /// encoded search directory.
    /// Returns `None` if the search would overflow.
    pub fn next(self, idx: HistoryIndex) -> Option<HistoryIndex> {
        let (next, overflow) = match self {
            Self::Backwards => idx.overflowing_sub(1),
            Self::Forwards => idx.overflowing_add(1),
        };
        if overflow {
            None
        } else {
            Some(next)
        }
    }
}

/// A simple history implementation that holds entries in memory.
#[derive(Default)]
pub struct BasicHistory {
    entries: VecDeque<String>,
}

impl History for BasicHistory {
    fn get(&self, idx: HistoryIndex) -> Option<Cow<str>> {
        self.entries.get(idx).map(|s| Cow::Borrowed(s.as_str()))
    }

    fn last(&self) -> Option<HistoryIndex> {
        if self.entries.is_empty() {
            None
        } else {
            Some(self.entries.len() - 1)
        }
    }

    fn add(&mut self, line: &str) {
        if self.entries.back().map(String::as_str) == Some(line) {
            // Ignore duplicates
            return;
        }
        self.entries.push_back(line.to_owned());
    }

    fn search(
        &self,
        idx: HistoryIndex,
        style: SearchStyle,
        direction: SearchDirection,
        pattern: &str,
    ) -> Option<SearchResult> {
        let mut idx = idx;

        loop {
            let line = match self.entries.get(idx) {
                Some(line) => line,
                None => return None,
            };

            if let Some(cursor) = style.match_against(pattern, line) {
                return Some(SearchResult {
                    line: Cow::Borrowed(line.as_str()),
                    idx,
                    cursor,
                });
            }

            idx = match direction.next(idx) {
                None => return None,
                Some(idx) => idx,
            };
        }
    }
}

use memmem::{Searcher, TwoWaySearcher};

/// This is a simple, small, read buffer that always has the buffer
/// contents available as a contiguous slice.
#[derive(Debug)]
pub struct ReadBuffer {
    storage: Vec<u8>,
}

impl ReadBuffer {
    pub fn new() -> Self {
        Self {
            storage: Vec::with_capacity(16),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        self.storage.as_slice()
    }

    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }

    pub fn len(&self) -> usize {
        self.storage.len()
    }

    /// Mark `len` bytes as consumed, discarding them and shunting
    /// the contents of the buffer such that the remainder of the
    /// bytes are available at the front of the buffer.
    pub fn advance(&mut self, len: usize) {
        let remain = self.storage.len() - len;
        self.storage.rotate_left(len);
        self.storage.truncate(remain);
    }

    /// Append the contents of the slice to the read buffer
    pub fn extend_with(&mut self, slice: &[u8]) {
        self.storage.extend_from_slice(slice);
    }

    /// Search for `needle` starting at `offset`.  Returns its offset
    /// into the buffer if found, else None.
    pub fn find_subsequence(&self, offset: usize, needle: &[u8]) -> Option<usize> {
        let needle = TwoWaySearcher::new(needle);
        let haystack = &self.storage[offset..];
        needle.search_in(haystack).map(|x| x + offset)
    }
}

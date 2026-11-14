use super::*;

impl Contrast {
    pub fn cached_pairs(&self) -> usize {
        self.cache.borrow().len()
    }
}

pub const DEFAULT_CURSOR: Srgb = DEFAULT_FOREGROUND;

pub fn indexed(index: u8) -> Srgb {
    Paints::default().indexed(index)
}

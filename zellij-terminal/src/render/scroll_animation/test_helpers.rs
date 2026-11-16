use super::*;

impl PaneAnimation {
    pub(super) fn uncovered_rows(&self, now: Instant, cell_height: u32) -> usize {
        if cell_height == 0 {
            return 0;
        }
        let offset = self.offset_at(now);
        let needed = (offset.abs() / cell_height as f64).ceil() as usize;
        let held = if offset < 0.0 {
            self.below.len()
        } else {
            self.above.len()
        };
        needed.saturating_sub(held)
    }
}

impl ScrollAnimations {
    pub fn offset_of(&self, pane: usize, now: Instant) -> Option<f64> {
        self.panes
            .iter()
            .find(|animation| animation.pane == pane)
            .map(|animation| animation.offset_at(now))
    }

    pub fn animating(&self, pane: usize) -> bool {
        self.panes.iter().any(|animation| animation.pane == pane)
    }

    pub fn uncovered_rows(&self, pane: usize, now: Instant, cell_height: u32) -> Option<usize> {
        self.panes
            .iter()
            .find(|animation| animation.pane == pane)
            .map(|animation| animation.uncovered_rows(now, cell_height))
    }
}

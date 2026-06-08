use zellij_tile::prelude::*;

use crate::click::{slop_key, ClickAction, ClickRegion};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LastEmittedCursor {
    #[default]
    Unknown,
    Sent(Option<(usize, usize)>),
}

#[derive(Default)]
pub struct Frame {
    pub click_regions: Vec<ClickRegion>,
    pub last_render_rows: usize,
    pub last_render_cols: usize,
    pub last_emitted_cursor: LastEmittedCursor,
    pub soft_keyboard_visible: bool,
}

impl Frame {
    pub fn emit_cursor(&mut self, new_pos: Option<(usize, usize)>) {
        let needs_emit = match self.last_emitted_cursor {
            LastEmittedCursor::Unknown => true,
            LastEmittedCursor::Sent(prev) => prev != new_pos,
        };
        if needs_emit {
            show_cursor(new_pos);
            self.last_emitted_cursor = LastEmittedCursor::Sent(new_pos);
        }
    }

    pub fn click_to_action(&self, row: usize, col: usize) -> Option<ClickAction> {
        for region in &self.click_regions {
            if region.priority == 0
                && region.row_start <= row
                && row < region.row_end
                && col >= region.col_start
                && col < region.col_end
            {
                return Some(region.action.clone());
            }
        }
        let mut best: Option<(&ClickRegion, u64)> = None;
        for region in &self.click_regions {
            if region.priority == 0 {
                continue;
            }
            if row < region.row_start || row >= region.row_end {
                continue;
            }
            if col < region.col_start || col >= region.col_end {
                continue;
            }
            let Some((cx, cy)) = region.center else {
                continue;
            };
            let dx = (cx as i64 - col as i64).unsigned_abs();
            let dy = (cy as i64 - row as i64).unsigned_abs();
            let dist_sq = dx * dx + dy * dy;
            best = Some(match best {
                None => (region, dist_sq),
                Some((cur, cur_d)) if dist_sq < cur_d => (region, dist_sq),
                Some((cur, cur_d)) if dist_sq == cur_d => {
                    let cur_key = slop_key(cur);
                    let new_key = slop_key(region);
                    if new_key < cur_key {
                        (region, dist_sq)
                    } else {
                        (cur, cur_d)
                    }
                },
                Some(prev) => prev,
            });
        }
        best.map(|(r, _)| r.action.clone())
    }
}

pub fn chrome_offsets(
    rows: usize,
    suppress_top_bar: bool,
    soft_keyboard_visible: bool,
) -> (usize, usize) {
    let body_top = if suppress_top_bar { 0 } else { 1 };
    let bar_height = if soft_keyboard_visible && rows.saturating_sub(body_top) >= 2 {
        1
    } else {
        0
    };
    (body_top, bar_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::modifier_bar::CellId;

    fn kb(id: u16) -> ClickAction {
        ClickAction::Keyboard(CellId(id))
    }

    #[test]
    fn tight_wins_over_overlapping_slop() {
        let mut f = Frame::default();
        f.click_regions.push(ClickRegion::tight(5, 10, 13, kb(1)));
        f.click_regions
            .push(ClickRegion::slop(5, 9, 14, kb(1), (11, 5)));
        f.click_regions.push(ClickRegion::tight(5, 13, 16, kb(2)));
        f.click_regions
            .push(ClickRegion::slop(5, 12, 17, kb(2), (14, 5)));
        assert_eq!(f.click_to_action(5, 12), Some(kb(1)));
        assert_eq!(f.click_to_action(5, 13), Some(kb(2)));
    }

    #[test]
    fn slop_resolves_by_nearest_center() {
        let mut f = Frame::default();
        f.click_regions.push(ClickRegion::tight(5, 10, 13, kb(1)));
        for r in 4..=6 {
            f.click_regions
                .push(ClickRegion::slop(r, 9, 14, kb(1), (11, 5)));
        }
        f.click_regions.push(ClickRegion::tight(7, 10, 13, kb(2)));
        for r in 6..=8 {
            f.click_regions
                .push(ClickRegion::slop(r, 9, 14, kb(2), (11, 7)));
        }
        assert_eq!(f.click_to_action(6, 11), Some(kb(1)));
        assert_eq!(f.click_to_action(8, 11), Some(kb(2)));
    }

    #[test]
    fn miss_returns_none() {
        let mut f = Frame::default();
        f.click_regions.push(ClickRegion::tight(5, 10, 13, kb(1)));
        f.click_regions
            .push(ClickRegion::slop(5, 9, 14, kb(1), (11, 5)));
        assert!(f.click_to_action(0, 0).is_none());
        assert!(f.click_to_action(5, 20).is_none());
    }
}

use zellij_tile::prelude::*;

use crate::click::ViewportRegion;
use crate::frame::Frame;
use crate::input::Input;
use crate::keys;
use crate::render::{move_to, slice_ansi_visible, visible_width, RESET};
use crate::workspace::{pane_id_of, Workspace};

#[derive(Default)]
pub struct ViewportScreen {
    pub viewport_v_pan: usize,
    pub viewport_h_pan: usize,
    pub viewport_region: Option<ViewportRegion>,
}

impl ViewportScreen {
    pub fn reset_pan(&mut self) {
        self.viewport_v_pan = 0;
        self.viewport_h_pan = 0;
    }

    pub fn handle_key(&self, ws: &Workspace, input: &mut Input, key: KeyWithModifier) -> bool {
        if let Some(pane) = ws.current_pane() {
            let key = if input.ctrl_held || input.alt_held {
                input.merge_held_modifiers(&key)
            } else {
                key
            };
            let bytes = keys::serialize_key(&key);
            if !bytes.is_empty() {
                write_to_pane_id(bytes, pane_id_of(&pane));
            }
        }
        let consumed = input.ctrl_held || input.alt_held;
        input.ctrl_held = false;
        input.alt_held = false;
        consumed
    }

    pub fn max_viewport_v_pan(&self, ws: &Workspace) -> Option<usize> {
        let region = self.viewport_region?;
        let embed_height = region.row_end.saturating_sub(region.row_start);
        Some(ws.current_pane_viewport_len().saturating_sub(embed_height))
    }

    pub fn click_in_viewport(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        let region = self.viewport_region?;
        if row < region.row_start || row >= region.row_end {
            return None;
        }
        if col >= region.cols {
            return None;
        }
        let pane_row = region.skip + (row - region.row_start);
        let pane_col = region.h_offset + col;
        Some((pane_row, pane_col))
    }

    pub fn compute_cursor_position(
        &self,
        ws: &Workspace,
        viewport_top: usize,
        viewport_height: usize,
        cols: usize,
        skip: usize,
        h_offset: usize,
    ) -> Option<(usize, usize)> {
        if viewport_height == 0 {
            return None;
        }
        let pane = ws.current_pane()?;
        let pane_id = pane_id_of(&pane);
        let (cursor_x, cursor_y) = ws.latest_pane_contents.get(&pane_id)?.cursor?;
        if cursor_y < skip {
            return None; // above the rendered slice (user has panned up)
        }
        let row_in_slice = cursor_y - skip;
        if row_in_slice >= viewport_height {
            return None; // below the rendered slice (shouldn't normally happen)
        }
        if cursor_x < h_offset {
            return None; // left of the rendered slice (user has panned right)
        }
        let plugin_x = cursor_x - h_offset;
        if plugin_x >= cols {
            return None; // past the right edge of the rendered slice
        }
        let plugin_y = viewport_top + row_in_slice;
        Some((plugin_x, plugin_y))
    }

    pub fn render(
        &mut self,
        ws: &Workspace,
        _frame: &mut Frame,
        row_start: usize,
        row_end: usize,
        cols: usize,
    ) {
        let height = row_end - row_start;
        if height == 0 {
            return;
        }

        let pane = ws.current_pane();
        let pane_id = pane.as_ref().map(pane_id_of);
        let viewport_lines: Vec<String> = pane_id
            .and_then(|id| ws.latest_pane_contents.get(&id))
            .map(|c| c.viewport.clone())
            .unwrap_or_default();

        let max_v_pan = self
            .max_viewport_v_pan(ws)
            .unwrap_or_else(|| viewport_lines.len().saturating_sub(height));
        self.viewport_v_pan = self.viewport_v_pan.min(max_v_pan);
        let skip = max_v_pan.saturating_sub(self.viewport_v_pan);
        let pane_width = pane
            .as_ref()
            .map(|p| p.pane_content_columns)
            .filter(|&w| w > 0)
            .unwrap_or_else(|| {
                viewport_lines
                    .iter()
                    .map(|l| visible_width(l))
                    .max()
                    .unwrap_or(0)
            });
        let max_h_pan = pane_width.saturating_sub(cols);
        self.viewport_h_pan = self.viewport_h_pan.min(max_h_pan);
        let h_offset = self.viewport_h_pan;

        self.viewport_region = Some(ViewportRegion {
            row_start,
            row_end,
            cols,
            skip,
            h_offset,
        });

        // Disable autowrap (DECAWM, `\x1b[?7l`) for the duration of the
        // viewport emit.
        print!("\x1b[?7l");

        for i in 0..height {
            let row = row_start + i;
            print!("{}{}", RESET, move_to(row, 0));
            if let Some(line) = viewport_lines.get(skip + i) {
                if h_offset == 0 {
                    // Fast path: no horizontal pan, no slicing needed
                    print!("{}", line);
                } else {
                    let sliced = slice_ansi_visible(line, h_offset, cols);
                    print!("{}", sliced);
                }
            } else if i == 0 && pane_id.is_none() {
                print!("{}(no pane selected)", RESET);
            } else if i == 0 && viewport_lines.is_empty() {
                print!("{}(awaiting first render…)", RESET);
            }
            let raw_width = viewport_lines
                .get(skip + i)
                .map(|l| visible_width(l))
                .unwrap_or(0);
            let printed_width = raw_width.saturating_sub(h_offset).min(cols);
            if printed_width < cols {
                print!("{}\x1b[K", RESET);
            }
        }

        // Restore autowrap before the function returns so subsequent
        // chrome rendering on later frames is unaffected.
        print!("\x1b[?7h");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::click::ViewportRegion;

    fn state_with_viewport(
        viewport_len: usize,
        embed_height: Option<usize>,
    ) -> (Workspace, ViewportScreen) {
        let mut ws = Workspace::default();
        let mut tab = TabInfo::default();
        tab.position = 0;
        ws.tabs.push(tab);
        ws.selected_tab_position = Some(0);

        let mut pane = PaneInfo::default();
        pane.id = 42;
        pane.is_plugin = false;
        pane.is_selectable = true;
        pane.is_suppressed = false;
        ws.panes_by_tab_position.insert(0, vec![pane]);
        ws.selected_pane_id = Some(PaneId::Terminal(42));

        let mut contents = PaneContents::default();
        contents.viewport = vec![String::new(); viewport_len];
        ws.latest_pane_contents.insert(PaneId::Terminal(42), contents);

        let mut vp = ViewportScreen::default();
        if let Some(h) = embed_height {
            vp.viewport_region = Some(ViewportRegion {
                row_start: 0,
                row_end: h,
                cols: 80,
                skip: 0,
                h_offset: 0,
            });
        }
        (ws, vp)
    }

    #[test]
    fn max_viewport_v_pan_none_without_region() {
        let (ws, vp) = state_with_viewport(100, None);
        assert_eq!(vp.max_viewport_v_pan(&ws), None);
    }

    #[test]
    fn max_viewport_v_pan_some_typical() {
        let (ws, vp) = state_with_viewport(100, Some(20));
        assert_eq!(vp.max_viewport_v_pan(&ws), Some(80));
    }

    #[test]
    fn max_viewport_v_pan_saturates_when_embed_larger() {
        let (ws, vp) = state_with_viewport(10, Some(20));
        assert_eq!(vp.max_viewport_v_pan(&ws), Some(0));
    }

    #[test]
    fn max_viewport_v_pan_empty_cache() {
        let (ws, vp) = state_with_viewport(0, Some(20));
        assert_eq!(vp.max_viewport_v_pan(&ws), Some(0));
    }
}

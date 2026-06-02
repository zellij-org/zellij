//! The hamburger dropdown menu. An overlay (tracked by `open`) painted
//! over the upper-right corner of the embedded viewport. Its rows toggle
//! Fit-to-Screen and open the Change Pane / Change Session selectors;
//! a separated last row exits mobile mode.

use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::click::{ClickAction, ClickRegion};
use crate::fit::Fit;
use crate::frame::Frame;
use crate::screens::ActiveScreen;

/// The hamburger dropdown overlay state.
#[derive(Default)]
pub struct MenuScreen {
    /// True while the dropdown is open. Mutually exclusive with an open
    /// selector: opening any selector clears this, and the menu render
    /// is gated on the active screen being `Viewport`.
    pub open: bool,
}

impl MenuScreen {
    /// Toggle the dropdown. Opening it returns the body to the Viewport
    /// (the menu only overlays the viewport, never a selector).
    pub fn toggle(&mut self, active: &mut ActiveScreen) -> bool {
        if self.open {
            self.open = false;
        } else {
            *active = ActiveScreen::Viewport;
            self.open = true;
        }
        true
    }

    /// Render the hamburger dropdown in the upper-right corner of the
    /// body region. One row per item, truncated to fit within
    /// `[row_start, row_end)` so menu rows never overlap the modifier
    /// bar's click regions below.
    pub fn render(
        &self,
        fit: &Fit,
        frame: &mut Frame,
        row_start: usize,
        row_end: usize,
        cols: usize,
    ) {
        // A `Separator` between "Change Session" and "Switch to
        // Desktop" guards against an accidental tap on the destructive
        // (one-way) Switch-to-Desktop row: separators do not register
        // click regions, so they also create a column of dead pixels
        // between the two interactive groups.
        let entries: [HamburgerEntry; 5] = [
            HamburgerEntry::Item("Fit to Screen", HamburgerItem::Fit),
            HamburgerEntry::Item("Change Pane", HamburgerItem::ChangePane),
            HamburgerEntry::Item("Change Session", HamburgerItem::ChangeSession),
            HamburgerEntry::Separator,
            HamburgerEntry::Item("Switch to Desktop", HamburgerItem::SwitchToDesktop),
        ];

        let label_max = entries
            .iter()
            .filter_map(|e| match e {
                HamburgerEntry::Item(label, _) => Some(UnicodeWidthStr::width(*label)),
                HamburgerEntry::Separator => None,
            })
            .max()
            .unwrap_or(0);
        // 1 cell of left padding + label_max + 1 cell of right padding.
        let menu_w = label_max + 2;
        if label_max == 0 || menu_w > cols {
            return;
        }
        let menu_x = cols - menu_w;

        // Truncate to fit vertically. A short body (e.g. plugin keyboard
        // takes most of the screen) clips trailing entries rather than
        // overlapping the keyboard cells below.
        let max_visible = row_end.saturating_sub(row_start);
        let visible_entries = entries.len().min(max_visible);

        for (i, entry) in entries.iter().take(visible_entries).enumerate() {
            let row = row_start + i;
            match entry {
                HamburgerEntry::Item(label, item) => {
                    let label_w = UnicodeWidthStr::width(*label);
                    let trailing_pad = label_max - label_w;

                    // Build " <label><trailing-pad> ": one cell left pad,
                    // label_max cells of label-plus-trailing-pad, one cell
                    // right pad. Constant `menu_w` cells total so click
                    // regions are uniform across rows.
                    let mut text_str = String::with_capacity(menu_w);
                    text_str.push(' ');
                    text_str.push_str(label);
                    for _ in 0..trailing_pad {
                        text_str.push(' ');
                    }
                    text_str.push(' ');

                    // `color_range` is character-indexed (not cell-indexed).
                    // The leading space is one char; the label starts
                    // immediately after.
                    let label_char_start = 1;
                    let label_char_end = label_char_start + label.chars().count();

                    let armed = match item {
                        HamburgerItem::Fit => fit.active,
                        _ => false,
                    };
                    let mut t = Text::new(&text_str).selected();
                    t = if armed {
                        t.success_color_range(label_char_start..label_char_end)
                    } else {
                        t.color_range(3, label_char_start..label_char_end)
                    };
                    print_text_with_coordinates(t, menu_x, row, Some(menu_w), None);

                    let action = match item {
                        HamburgerItem::Fit => ClickAction::ToggleFit,
                        HamburgerItem::ChangePane => ClickAction::ExpandPanes,
                        HamburgerItem::ChangeSession => ClickAction::ExpandSessions,
                        HamburgerItem::SwitchToDesktop => ClickAction::ExitMobileMode,
                    };
                    frame.click_regions.push(ClickRegion::tight(
                        row,
                        menu_x,
                        menu_x + menu_w,
                        action,
                    ));
                },
                HamburgerEntry::Separator => {
                    // Same `menu_w` width as items so the row's
                    // background painting stays uniform. Filled with the
                    // light-horizontal box-drawing char so the divider
                    // reads visually as a rule rather than a blank gap.
                    // No click region is pushed: taps here fall through
                    // and resolve to no action.
                    let mut text_str = String::with_capacity(menu_w);
                    text_str.push(' ');
                    for _ in 0..label_max {
                        text_str.push('\u{2500}'); // ─
                    }
                    text_str.push(' ');
                    let t = Text::new(&text_str).selected();
                    print_text_with_coordinates(t, menu_x, row, Some(menu_w), None);
                },
            }
        }
    }
}

/// One row in the hamburger dropdown menu. Toggle items track the
/// underlying state (`Fit` mirrors `fit.active`); navigation items are
/// stateless.
enum HamburgerItem {
    Fit,
    ChangePane,
    ChangeSession,
    SwitchToDesktop,
}

/// One row in the hamburger dropdown. Either an interactive `Item` that
/// registers a click region, or a non-interactive `Separator` that
/// visually divides item groups.
enum HamburgerEntry {
    Item(&'static str, HamburgerItem),
    Separator,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;

    /// The hamburger dropdown emits one click region per visible
    /// item, in declaration order: Fit (row 0), Change Pane (row 1),
    /// Change Session (row 2), separator (row 3, no region), Switch
    /// to Desktop (row 4). The separator row must not dispatch any
    /// action — that gap is the guard against accidental taps on
    /// the destructive Switch-to-Desktop row.
    #[test]
    fn hamburger_menu_emits_four_click_regions_with_separator_above_exit() {
        let mut state = State::default();
        let cols = 40;
        // Plenty of vertical space so every entry is visible.
        state.menu.render(&state.fit, &mut state.frame, 0, 20, cols);

        assert_eq!(state.frame.click_regions.len(), 4);
        let actions: Vec<ClickAction> = state
            .frame
            .click_regions
            .iter()
            .map(|r| r.action.clone())
            .collect();
        assert!(matches!(actions[0], ClickAction::ToggleFit));
        assert!(matches!(actions[1], ClickAction::ExpandPanes));
        assert!(matches!(actions[2], ClickAction::ExpandSessions));
        assert!(matches!(actions[3], ClickAction::ExitMobileMode));

        // The interactive rows must occupy 0, 1, 2, 4 — skipping
        // row 3 (the separator). Use the row span to confirm the
        // gap is exactly where expected.
        let rows: Vec<usize> = state
            .frame
            .click_regions
            .iter()
            .map(|r| r.row_start)
            .collect();
        assert_eq!(rows, vec![0, 1, 2, 4]);

        // Tapping the separator row at any column inside the menu
        // width must resolve to no action.
        let menu_x = state.frame.click_regions[0].col_start;
        let menu_end = state.frame.click_regions[0].col_end;
        for c in menu_x..menu_end {
            assert_eq!(
                state.frame.click_to_action(3, c),
                None,
                "separator row should be non-interactive at col {c}",
            );
        }
    }
}

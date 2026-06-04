use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::click::{ClickAction, ClickRegion};
use crate::fit::Fit;
use crate::frame::Frame;
use crate::screens::ActiveScreen;

const H_PAD: usize = 1;
const SEPARATOR_CHAR: char = '\u{2500}';

const ENTRIES: [HamburgerEntry; 5] = [
    HamburgerEntry::Item(HamburgerItem::Fit),
    HamburgerEntry::Item(HamburgerItem::ChangePane),
    HamburgerEntry::Item(HamburgerItem::ChangeSession),
    HamburgerEntry::Separator,
    HamburgerEntry::Item(HamburgerItem::SwitchToDesktop),
];

#[derive(Default)]
pub struct MenuScreen {
    pub open: bool,
}

impl MenuScreen {
    pub fn toggle(&mut self, active: &mut ActiveScreen) -> bool {
        if self.open {
            self.open = false;
        } else {
            *active = ActiveScreen::Viewport;
            self.open = true;
        }
        true
    }

    pub fn render(
        &self,
        fit: &Fit,
        frame: &mut Frame,
        row_start: usize,
        row_end: usize,
        cols: usize,
    ) {
        let Some(layout) = MenuLayout::compute(&ENTRIES, cols) else {
            return;
        };

        let max_visible = row_end.saturating_sub(row_start);
        for (i, entry) in ENTRIES.iter().take(max_visible).enumerate() {
            let row = row_start + i;
            match entry {
                HamburgerEntry::Item(item) => draw_item_row(frame, fit, item, &layout, row),
                HamburgerEntry::Separator => draw_separator_row(&layout, row),
            }
        }
    }
}

enum HamburgerItem {
    Fit,
    ChangePane,
    ChangeSession,
    SwitchToDesktop,
}

impl HamburgerItem {
    fn label(&self) -> &'static str {
        match self {
            HamburgerItem::Fit => "Fit to Screen",
            HamburgerItem::ChangePane => "Change Pane",
            HamburgerItem::ChangeSession => "Change Session",
            HamburgerItem::SwitchToDesktop => "Switch to Desktop",
        }
    }

    fn action(&self) -> ClickAction {
        match self {
            HamburgerItem::Fit => ClickAction::ToggleFit,
            HamburgerItem::ChangePane => ClickAction::ExpandPanes,
            HamburgerItem::ChangeSession => ClickAction::ExpandSessions,
            HamburgerItem::SwitchToDesktop => ClickAction::ExitMobileMode,
        }
    }

    fn is_armed(&self, fit: &Fit) -> bool {
        matches!(self, HamburgerItem::Fit) && fit.active
    }
}

enum HamburgerEntry {
    Item(HamburgerItem),
    Separator,
}

impl HamburgerEntry {
    fn label_width(&self) -> Option<usize> {
        match self {
            HamburgerEntry::Item(item) => Some(UnicodeWidthStr::width(item.label())),
            HamburgerEntry::Separator => None,
        }
    }
}

struct MenuLayout {
    menu_x: usize,
    menu_w: usize,
    label_max: usize,
}

impl MenuLayout {
    fn compute(entries: &[HamburgerEntry], cols: usize) -> Option<Self> {
        let label_max = entries.iter().filter_map(HamburgerEntry::label_width).max().unwrap_or(0);
        let menu_w = label_max + 2 * H_PAD;
        if label_max == 0 || menu_w > cols {
            return None;
        }
        Some(MenuLayout {
            menu_x: cols - menu_w,
            menu_w,
            label_max,
        })
    }

    fn padded_row(&self, content: &str, content_w: usize) -> String {
        let pad = " ".repeat(H_PAD);
        let trailing = " ".repeat(self.label_max.saturating_sub(content_w));
        format!("{pad}{content}{trailing}{pad}")
    }
}

fn draw_item_row(frame: &mut Frame, fit: &Fit, item: &HamburgerItem, layout: &MenuLayout, row: usize) {
    let label = item.label();
    let text = layout.padded_row(label, UnicodeWidthStr::width(label));
    let label_chars = H_PAD..H_PAD + label.chars().count();

    let mut t = Text::new(&text).selected();
    t = if item.is_armed(fit) {
        t.success_color_range(label_chars)
    } else {
        t.color_range(3, label_chars)
    };
    print_text_with_coordinates(t, layout.menu_x, row, Some(layout.menu_w), None);

    frame.click_regions.push(ClickRegion::tight(
        row,
        layout.menu_x,
        layout.menu_x + layout.menu_w,
        item.action(),
    ));
}

fn draw_separator_row(layout: &MenuLayout, row: usize) {
    let rule: String = std::iter::repeat(SEPARATOR_CHAR).take(layout.label_max).collect();
    let text = layout.padded_row(&rule, layout.label_max);
    print_text_with_coordinates(Text::new(&text).selected(), layout.menu_x, row, Some(layout.menu_w), None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;

    #[test]
    fn hamburger_menu_emits_four_click_regions_with_separator_above_exit() {
        let mut state = State::default();
        let cols = 40;
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

        let rows: Vec<usize> = state
            .frame
            .click_regions
            .iter()
            .map(|r| r.row_start)
            .collect();
        assert_eq!(rows, vec![0, 1, 2, 4]);

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

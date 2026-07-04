use fuzzy_matcher::FuzzyMatcher;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::click::{ClickAction, ClickRegion};
use crate::frame::Frame;
use crate::navigation::Navigation;
use crate::screens::ActiveScreen;
use crate::workspace::{pane_id_of, Workspace};

const CARD_BULLET: &str = "- ";
const CARD_INDENT_W: usize = 2;
const META_SEPARATOR: &str = ", ";
const TITLE: &str = "Switch Pane";
const PROMPT_LABEL: &str = "Pane: ";
const CURRENT_PANE_LABEL: &str = "[CURRENT PANE]";
const NEW_TAB_LABEL: &str = "+ New Tab";
const NEW_PANE_LABEL: &str = "+ New Pane";
const FOOTER_GAP: usize = 4;

#[derive(Default)]
pub struct PanesScreen {
    pub panes_search: String,
}

impl PanesScreen {
    pub fn handle_key(
        &mut self,
        active: &mut ActiveScreen,
        nav: &mut Navigation,
        ws: &Workspace,
        key: KeyWithModifier,
    ) -> Option<(usize, PaneId)> {
        match key.bare_key {
            BareKey::Esc => {
                if !self.panes_search.is_empty() {
                    self.panes_search.clear();
                    nav.selector_scroll_offset = 0;
                } else {
                    *active = ActiveScreen::Viewport;
                }
                None
            },
            BareKey::Enter => {
                if let Some((tab_position, pane_id)) = self.panes_top_match(ws, nav) {
                    self.panes_search.clear();
                    nav.selector_scroll_offset = 0;
                    Some((tab_position, pane_id))
                } else {
                    None
                }
            },
            BareKey::Backspace => {
                self.panes_search.pop();
                nav.selector_scroll_offset = 0;
                None
            },
            BareKey::Char(c) => {
                self.panes_search.push(c);
                nav.selector_scroll_offset = 0;
                None
            },
            _ => None,
        }
    }

    pub fn panes_top_match(
        &mut self,
        ws: &Workspace,
        nav: &mut Navigation,
    ) -> Option<(usize, PaneId)> {
        let tabs: Vec<TabInfo> = ws.tabs_in_order().into_iter().cloned().collect();
        let mut entries: Vec<(String, usize, PaneId)> = Vec::new();
        for tab in &tabs {
            for pane in ws.panes_for_tab(tab.position) {
                entries.push((pane_title(pane), tab.position, pane_id_of(pane)));
            }
        }
        if entries.is_empty() {
            return None;
        }
        let search = self.panes_search.clone();
        if search.is_empty() {
            let first = entries.into_iter().next()?;
            return Some((first.1, first.2));
        }
        let matcher = nav.matcher();
        let mut best: Option<(i64, String, usize, PaneId)> = None;
        for (title, tab_pos, pane_id) in entries.into_iter() {
            if let Some((score, _)) = matcher.fuzzy_indices(&title, &search) {
                let take = match &best {
                    None => true,
                    Some((bs, bn, _, _)) => score > *bs || (score == *bs && &title < bn),
                };
                if take {
                    best = Some((score, title, tab_pos, pane_id));
                }
            }
        }
        best.map(|(_, _, tab_pos, pane_id)| (tab_pos, pane_id))
    }

    pub fn render(
        &mut self,
        ws: &Workspace,
        nav: &mut Navigation,
        frame: &mut Frame,
        row_start: usize,
        row_end: usize,
        cols: usize,
    ) {
        if row_start < row_end {
            draw_back_button(frame, row_start);
        }

        let body_start = row_start.saturating_add(1);
        let body_end = row_end.saturating_sub(1);
        let body_height = body_end.saturating_sub(body_start);
        if body_height == 0 || cols == 0 {
            return;
        }

        let new_pane_target_tab = ws.current_tab().map(|t| t.position);
        let search = self.panes_search.clone();
        let cards = self.ordered_cards(ws, nav, &search);
        let layout = PickerLayout::compute(
            nav,
            &cards,
            body_start,
            body_end,
            body_height,
            cols,
            new_pane_target_tab.is_some(),
        );
        let visible: Vec<&PaneCard> = cards
            .iter()
            .skip(layout.offset)
            .take(layout.visible_count)
            .collect();

        draw_title(&layout);
        draw_prompt(&search, &layout);
        draw_scroll_indicators(&layout);
        draw_cards(frame, &visible, &layout);
        draw_footer(frame, new_pane_target_tab, &layout);
    }

    fn ordered_cards(&self, ws: &Workspace, nav: &mut Navigation, search: &str) -> Vec<PaneCard> {
        let cards = self.collect_cards(ws);
        if search.is_empty() {
            return cards;
        }
        let matcher = nav.matcher();
        let mut scored: Vec<(i64, PaneCard)> = cards
            .into_iter()
            .filter_map(|mut card| {
                matcher
                    .fuzzy_indices(&card.title_label, search)
                    .map(|(score, indices)| {
                        card.title_indices = indices;
                        (score, card)
                    })
            })
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.title_label.cmp(&b.1.title_label))
        });
        scored.into_iter().map(|(_, card)| card).collect()
    }

    fn collect_cards(&self, ws: &Workspace) -> Vec<PaneCard> {
        let now = crate::unix_now();
        let current_tab_position = ws.current_tab().map(|t| t.position);
        let current_pane_id = ws.current_pane().as_ref().map(pane_id_of);
        let tabs: Vec<TabInfo> = ws.tabs_in_order().into_iter().cloned().collect();

        let mut cards = Vec::new();
        for tab in &tabs {
            for pane in ws.panes_for_tab(tab.position) {
                let id = pane_id_of(pane);
                let is_current =
                    current_tab_position == Some(tab.position) && current_pane_id == Some(id);
                cards.push(PaneCard {
                    title_label: pane_title(pane),
                    tab_label: tab.name.clone(),
                    activity_label: pane_activity_label(ws, id, is_current, now),
                    action: ClickAction::SelectPane {
                        tab_position: tab.position,
                        pane_id: id,
                    },
                    title_indices: Vec::new(),
                    is_current,
                });
            }
        }
        cards
    }
}

fn pane_title(pane: &PaneInfo) -> String {
    if pane.title.is_empty() {
        format!("#{}", pane.id)
    } else {
        pane.title.clone()
    }
}

fn pane_activity_label(ws: &Workspace, id: PaneId, is_current: bool, now: u64) -> String {
    if is_current {
        CURRENT_PANE_LABEL.to_string()
    } else {
        crate::ansi::format_time_ago(ws.pane_last_activity.get(&id).copied(), now)
    }
}

struct PaneCard {
    title_label: String,
    tab_label: String,
    activity_label: String,
    action: ClickAction,
    title_indices: Vec<usize>,
    is_current: bool,
}

impl PaneCard {
    fn meta_width(&self) -> usize {
        UnicodeWidthStr::width(self.tab_label.as_str())
            + META_SEPARATOR.len()
            + UnicodeWidthStr::width(self.activity_label.as_str())
    }

    fn content_width(&self) -> usize {
        UnicodeWidthStr::width(self.title_label.as_str()).max(self.meta_width())
    }

    fn draw(
        &self,
        frame: &mut Frame,
        card_x: usize,
        content_x: usize,
        row_title: usize,
        row_meta: usize,
        row_end: usize,
    ) {
        print_text_with_coordinates(Text::new(CARD_BULLET), card_x, row_title, None, None);

        let mut title_text = Text::new(&self.title_label).color_range(2, ..);
        if !self.title_indices.is_empty() {
            title_text = title_text.color_indices(3, self.title_indices.clone());
        }
        print_text_with_coordinates(title_text, content_x, row_title, None, None);

        if row_meta < row_end {
            self.draw_meta_row(content_x, row_meta);
        }

        frame.click_regions.push(ClickRegion::tight_range(
            row_title,
            row_meta + 1,
            card_x,
            content_x + self.content_width(),
            self.action.clone(),
        ));
    }

    fn draw_meta_row(&self, content_x: usize, row: usize) {
        // The host reapplies bold per color-range without composing state, so a
        // single Text would bleed the tab segment's bold onto the activity
        // segment; emit each segment as its own Text.
        let tab_w = UnicodeWidthStr::width(self.tab_label.as_str());
        print_text_with_coordinates(
            Text::new(&self.tab_label).color_range(1, ..),
            content_x,
            row,
            None,
            None,
        );

        let sep_x = content_x + tab_w;
        print_text_with_coordinates(Text::new(META_SEPARATOR), sep_x, row, None, None);

        let activity_x = sep_x + META_SEPARATOR.len();
        let activity_text = if self.is_current {
            Text::new(&self.activity_label).color_range(0, ..)
        } else {
            Text::new(&self.activity_label).unbold_all()
        };
        print_text_with_coordinates(activity_text, activity_x, row, None, None);
    }
}

struct PickerLayout {
    cols: usize,
    row_end: usize,
    top_y: usize,
    block_height: usize,
    card_x: usize,
    content_x: usize,
    offset: usize,
    visible_count: usize,
    hidden_above: usize,
    hidden_below: usize,
    new_tab_x: usize,
    new_tab_w: usize,
    new_pane_x: usize,
    new_pane_w: usize,
}

impl PickerLayout {
    fn compute(
        nav: &mut Navigation,
        cards: &[PaneCard],
        body_start: usize,
        body_end: usize,
        body_height: usize,
        cols: usize,
        show_new_pane: bool,
    ) -> Self {
        let total_cards = cards.len();
        const CHROME_ROWS: usize = 6;
        const ROWS_PER_CARD: usize = 2;
        let max_visible_cards =
            (body_height.saturating_sub(CHROME_ROWS) / ROWS_PER_CARD).min(total_cards);
        let max_offset = total_cards.saturating_sub(max_visible_cards);
        let offset = nav.selector_scroll_offset.min(max_offset);
        nav.selector_scroll_offset = offset;
        let visible_count = total_cards.saturating_sub(offset).min(max_visible_cards);

        let block_height = if visible_count == 0 {
            5.min(body_height)
        } else {
            (6 + 2 * visible_count).min(body_height)
        };
        let top_y = body_start + body_height.saturating_sub(block_height) / 2;

        let content_w = cards
            .iter()
            .skip(offset)
            .take(visible_count)
            .map(PaneCard::content_width)
            .max()
            .unwrap_or(0);
        let card_x = cols.saturating_sub(CARD_INDENT_W + content_w) / 2;
        let content_x = card_x + CARD_INDENT_W;

        let new_tab_w = UnicodeWidthStr::width(NEW_TAB_LABEL);
        let new_pane_w = UnicodeWidthStr::width(NEW_PANE_LABEL);
        let (new_tab_x, new_pane_x) = if show_new_pane {
            let total = new_tab_w + FOOTER_GAP + new_pane_w;
            let block_x = cols.saturating_sub(total) / 2;
            (block_x, block_x + new_tab_w + FOOTER_GAP)
        } else {
            (cols.saturating_sub(new_tab_w) / 2, 0)
        };

        PickerLayout {
            cols,
            row_end: body_end,
            top_y,
            block_height,
            card_x,
            content_x,
            offset,
            visible_count,
            hidden_above: offset,
            hidden_below: total_cards.saturating_sub(offset + visible_count),
            new_tab_x,
            new_tab_w,
            new_pane_x,
            new_pane_w,
        }
    }

    fn centered_x(&self, label_w: usize) -> usize {
        self.cols.saturating_sub(label_w) / 2
    }

    fn prompt_x(&self) -> usize {
        if self.visible_count > 0 {
            self.content_x
        } else {
            self.new_tab_x
        }
    }
}

fn draw_back_button(frame: &mut Frame, row: usize) {
    let back_label = "[← BACK]";
    let back_w = UnicodeWidthStr::width(back_label);
    print_text_with_coordinates(Text::new(back_label).color_range(3, ..), 0, row, None, None);
    frame.click_regions.push(ClickRegion::tight(
        row,
        0,
        back_w,
        ClickAction::CollapseSelector,
    ));
}

fn draw_title(layout: &PickerLayout) {
    let title_y = layout.top_y;
    if title_y < layout.row_end {
        let title_x = layout.centered_x(UnicodeWidthStr::width(TITLE));
        print_text_with_coordinates(Text::new(TITLE), title_x, title_y, None, None);
    }
}

fn draw_prompt(search: &str, layout: &PickerLayout) {
    let prompt_y = layout.top_y + 2;
    if prompt_y >= layout.row_end {
        return;
    }
    let prompt_full = format!("{}{}_", PROMPT_LABEL, search);
    let label_chars = PROMPT_LABEL.chars().count();
    let total_chars = prompt_full.chars().count();
    let prompt_text = Text::new(&prompt_full).color_range(3, label_chars..total_chars);
    print_text_with_coordinates(prompt_text, layout.prompt_x(), prompt_y, None, None);
}

fn draw_scroll_indicators(layout: &PickerLayout) {
    if layout.visible_count == 0 {
        return;
    }
    if layout.hidden_above > 0 {
        let y = layout.top_y + 3;
        if y < layout.row_end {
            let label = format!("\u{2191} [+{}]", layout.hidden_above);
            let w = UnicodeWidthStr::width(label.as_str());
            print_text_with_coordinates(
                Text::new(&label).color_range(1, ..),
                layout.centered_x(w),
                y,
                None,
                None,
            );
        }
    }
    if layout.hidden_below > 0 {
        let y = layout.top_y + 4 + 2 * layout.visible_count;
        if y < layout.row_end {
            let label = format!("\u{2193} [+{}]", layout.hidden_below);
            let w = UnicodeWidthStr::width(label.as_str());
            print_text_with_coordinates(
                Text::new(&label).color_range(1, ..),
                layout.centered_x(w),
                y,
                None,
                None,
            );
        }
    }
}

fn draw_cards(frame: &mut Frame, visible: &[&PaneCard], layout: &PickerLayout) {
    let first_card_y = layout.top_y + 4;
    for (i, card) in visible.iter().enumerate() {
        let row_title = first_card_y + i * 2;
        if row_title >= layout.row_end {
            break;
        }
        card.draw(
            frame,
            layout.card_x,
            layout.content_x,
            row_title,
            row_title + 1,
            layout.row_end,
        );
    }
}

fn draw_footer(frame: &mut Frame, new_pane_target_tab: Option<usize>, layout: &PickerLayout) {
    let footer_y = layout.top_y + layout.block_height.saturating_sub(1);
    if footer_y >= layout.row_end {
        return;
    }
    print_text_with_coordinates(
        Text::new(NEW_TAB_LABEL).color_range(3, ..),
        layout.new_tab_x,
        footer_y,
        None,
        None,
    );
    frame.click_regions.push(ClickRegion::tight(
        footer_y,
        layout.new_tab_x,
        layout.new_tab_x + layout.new_tab_w,
        ClickAction::NewTab,
    ));

    if let Some(tab_position) = new_pane_target_tab {
        print_text_with_coordinates(
            Text::new(NEW_PANE_LABEL).color_range(3, ..),
            layout.new_pane_x,
            footer_y,
            None,
            None,
        );
        frame.click_regions.push(ClickRegion::tight(
            footer_y,
            layout.new_pane_x,
            layout.new_pane_x + layout.new_pane_w,
            ClickAction::NewPaneInTab { tab_position },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    fn state_with_tabs_and_panes(tab_count: usize) -> State {
        use zellij_tile::prelude::TabInfo;
        let mut state = State::default();
        for i in 0..tab_count {
            let mut tab = TabInfo::default();
            tab.position = i;
            tab.name = format!("Tab {}", i);
            state.workspace.tabs.push(tab);
            let mut pane = PaneInfo::default();
            pane.id = (100 + i) as u32;
            pane.is_plugin = false;
            pane.is_selectable = true;
            pane.is_suppressed = false;
            state.workspace.panes_by_tab_position.insert(i, vec![pane]);
        }
        state.workspace.selected_tab_position = Some(0);
        state.workspace.selected_pane_id = Some(PaneId::Terminal(100));
        state
    }
    #[test]
    fn panes_menu_one_tab_emits_four_click_regions() {
        let mut state = state_with_tabs_and_panes(1);
        let cols = 40;
        state.panes.render(
            &state.workspace,
            &mut state.navigation,
            &mut state.frame,
            0,
            20,
            cols,
        );
        assert_eq!(state.frame.click_regions.len(), 4);
        let actions: Vec<ClickAction> = state
            .frame
            .click_regions
            .iter()
            .map(|r| r.action.clone())
            .collect();
        assert!(matches!(actions[0], ClickAction::CollapseSelector));
        assert!(matches!(
            actions[1],
            ClickAction::SelectPane {
                tab_position: 0,
                pane_id: PaneId::Terminal(100)
            }
        ));
        assert!(matches!(actions[2], ClickAction::NewTab));
        assert!(matches!(
            actions[3],
            ClickAction::NewPaneInTab { tab_position: 0 }
        ));
    }

    #[test]
    fn panes_menu_two_tabs_emits_single_footer_new_pane() {
        let mut state = state_with_tabs_and_panes(2);
        let cols = 40;
        state.panes.render(
            &state.workspace,
            &mut state.navigation,
            &mut state.frame,
            0,
            20,
            cols,
        );
        assert_eq!(state.frame.click_regions.len(), 5);

        let new_panes: Vec<usize> = state
            .frame
            .click_regions
            .iter()
            .filter_map(|r| match &r.action {
                ClickAction::NewPaneInTab { tab_position } => Some(*tab_position),
                _ => None,
            })
            .collect();
        assert_eq!(new_panes, vec![0]);

        let new_tabs = state
            .frame
            .click_regions
            .iter()
            .filter(|r| matches!(r.action, ClickAction::NewTab))
            .count();
        assert_eq!(new_tabs, 1);
    }

    #[test]
    fn footer_affordances_share_a_row_and_order_left_to_right() {
        let mut state = state_with_tabs_and_panes(1);
        state.panes.render(
            &state.workspace,
            &mut state.navigation,
            &mut state.frame,
            0,
            20,
            40,
        );
        let new_tab = state
            .frame
            .click_regions
            .iter()
            .find(|r| matches!(r.action, ClickAction::NewTab))
            .expect("expected a NewTab region")
            .clone();
        let new_pane = state
            .frame
            .click_regions
            .iter()
            .find(|r| matches!(r.action, ClickAction::NewPaneInTab { .. }))
            .expect("expected a NewPaneInTab region")
            .clone();
        assert_eq!(
            new_tab.row_start, new_pane.row_start,
            "footer affordances must share a row",
        );
        assert!(
            new_tab.col_end <= new_pane.col_start,
            "+ New Tab ({}..{}) must sit left of + New Pane ({}..)",
            new_tab.col_start,
            new_tab.col_end,
            new_pane.col_start,
        );
    }

    #[test]
    fn click_on_new_pane_row_resolves_to_action() {
        let mut state = state_with_tabs_and_panes(2);
        state.workspace.selected_tab_position = Some(1);
        state.workspace.selected_pane_id = Some(PaneId::Terminal(101));
        state.panes.render(
            &state.workspace,
            &mut state.navigation,
            &mut state.frame,
            0,
            20,
            40,
        );
        let new_pane_region = state
            .frame
            .click_regions
            .iter()
            .find(|r| matches!(r.action, ClickAction::NewPaneInTab { tab_position: 1 }))
            .expect("expected NewPaneInTab targeting the selected tab")
            .clone();
        assert_eq!(
            state
                .frame
                .click_to_action(new_pane_region.row_start, new_pane_region.col_start,),
            Some(ClickAction::NewPaneInTab { tab_position: 1 })
        );
    }

    #[test]
    fn panes_menu_back_button_at_top_left_collapses_selector() {
        let mut state = state_with_tabs_and_panes(1);
        state.panes.render(
            &state.workspace,
            &mut state.navigation,
            &mut state.frame,
            0,
            20,
            40,
        );
        let first = state
            .frame
            .click_regions
            .first()
            .expect("expected at least one click region");
        assert_eq!(first.row_start, 0);
        assert_eq!(first.col_start, 0);
        assert!(matches!(first.action, ClickAction::CollapseSelector));
        assert_eq!(
            state.frame.click_to_action(0, 0),
            Some(ClickAction::CollapseSelector)
        );
    }

    #[test]
    fn pane_card_click_region_spans_two_rows() {
        let mut state = state_with_tabs_and_panes(1);
        state.panes.render(
            &state.workspace,
            &mut state.navigation,
            &mut state.frame,
            0,
            20,
            40,
        );
        let pane_region = state
            .frame
            .click_regions
            .iter()
            .find(|r| {
                matches!(
                    r.action,
                    ClickAction::SelectPane {
                        tab_position: 0,
                        pane_id: PaneId::Terminal(100),
                    }
                )
            })
            .expect("expected a SelectPane region")
            .clone();
        assert_eq!(
            pane_region.row_end - pane_region.row_start,
            2,
            "pane card click region should cover two rows",
        );
        let expected = Some(ClickAction::SelectPane {
            tab_position: 0,
            pane_id: PaneId::Terminal(100),
        });
        assert_eq!(
            state
                .frame
                .click_to_action(pane_region.row_start, pane_region.col_start),
            expected,
            "tap on title row should select the pane",
        );
        assert_eq!(
            state
                .frame
                .click_to_action(pane_region.row_start + 1, pane_region.col_start,),
            expected,
            "tap on activity row should select the pane",
        );
    }

    #[test]
    fn panes_menu_fuzzy_filter_keeps_footer_visible() {
        let mut state = state_with_tabs_and_panes(2);
        for (i, panes) in state.workspace.panes_by_tab_position.iter_mut() {
            for pane in panes.iter_mut() {
                pane.title = format!("alpha-{}", i);
            }
        }
        state.panes.panes_search = "alpha".to_string();
        state.panes.render(
            &state.workspace,
            &mut state.navigation,
            &mut state.frame,
            0,
            20,
            60,
        );

        let new_tab_count = state
            .frame
            .click_regions
            .iter()
            .filter(|r| matches!(r.action, ClickAction::NewTab))
            .count();
        assert_eq!(
            new_tab_count, 1,
            "+ New Tab must stay visible during search"
        );
        let new_pane_count = state
            .frame
            .click_regions
            .iter()
            .filter(|r| matches!(r.action, ClickAction::NewPaneInTab { .. }))
            .count();
        assert_eq!(
            new_pane_count, 1,
            "+ New Pane must stay visible during search",
        );
        assert_eq!(state.frame.click_regions.len(), 5);
    }

    #[test]
    fn current_pane_card_span_includes_current_pane_label() {
        let mut state = state_with_tabs_and_panes(2);
        state.workspace.selected_tab_position = Some(0);
        state.workspace.selected_pane_id = Some(PaneId::Terminal(100));
        state.panes.render(
            &state.workspace,
            &mut state.navigation,
            &mut state.frame,
            0,
            20,
            60,
        );

        let select_regions: Vec<&ClickRegion> = state
            .frame
            .click_regions
            .iter()
            .filter(|r| matches!(r.action, ClickAction::SelectPane { .. }))
            .collect();
        assert_eq!(select_regions.len(), 2);

        let (current, other): (&&ClickRegion, &&ClickRegion) = {
            let current = select_regions
                .iter()
                .find(|r| {
                    matches!(
                        r.action,
                        ClickAction::SelectPane {
                            pane_id: PaneId::Terminal(100),
                            ..
                        }
                    )
                })
                .expect("expected a SelectPane region for pane 100");
            let other = select_regions
                .iter()
                .find(|r| {
                    matches!(
                        r.action,
                        ClickAction::SelectPane {
                            pane_id: PaneId::Terminal(101),
                            ..
                        }
                    )
                })
                .expect("expected a SelectPane region for pane 101");
            (current, other)
        };

        let current_span = current.col_end.saturating_sub(current.col_start);
        assert_eq!(current_span, 23);

        let other_span = other.col_end.saturating_sub(other.col_start);
        assert_eq!(other_span, 10);

        assert_eq!(
            current.col_start, other.col_start,
            "both pane cards must anchor at the card column's left edge",
        );
    }

    #[test]
    fn panes_menu_fuzzy_filter_narrows_to_matching_pane() {
        let mut state = state_with_tabs_and_panes(2);
        state.workspace.panes_by_tab_position.get_mut(&0).unwrap()[0].title = "alpha".to_string();
        state.workspace.panes_by_tab_position.get_mut(&1).unwrap()[0].title = "bravo".to_string();
        state.panes.panes_search = "brv".to_string();
        state.panes.render(
            &state.workspace,
            &mut state.navigation,
            &mut state.frame,
            0,
            20,
            60,
        );

        let select_panes: Vec<ClickAction> = state
            .frame
            .click_regions
            .iter()
            .filter(|r| matches!(r.action, ClickAction::SelectPane { .. }))
            .map(|r| r.action.clone())
            .collect();
        assert_eq!(
            select_panes.len(),
            1,
            "only the 'bravo' pane should survive the 'brv' filter",
        );
        assert!(matches!(
            select_panes[0],
            ClickAction::SelectPane {
                tab_position: 1,
                pane_id: PaneId::Terminal(101),
            }
        ));
    }
}

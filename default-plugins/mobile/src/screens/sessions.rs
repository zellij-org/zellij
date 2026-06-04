use fuzzy_matcher::FuzzyMatcher;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::click::{ClickAction, ClickRegion};
use crate::frame::Frame;
use crate::navigation::Navigation;
use crate::screens::ActiveScreen;

const CARD_BULLET: &str = "- ";
const CARD_INDENT_W: usize = 2;
const NEW_SESSION_LABEL: &str = "+ New Session";

#[derive(Default)]
pub struct SessionsScreen {
    pub sessions: Vec<SessionInfo>,
    pub welcome_search: String,
    pub is_welcome_screen: bool,
}

impl SessionsScreen {
    pub fn select_session(&self, active: &mut ActiveScreen, name: &str) -> bool {
        switch_session(Some(name));
        *active = ActiveScreen::Viewport;
        true
    }

    pub fn handle_key(
        &mut self,
        active: &mut ActiveScreen,
        nav: &mut Navigation,
        key: KeyWithModifier,
    ) -> bool {
        match key.bare_key {
            BareKey::Esc => {
                if !self.welcome_search.is_empty() {
                    self.welcome_search.clear();
                    nav.selector_scroll_offset = 0;
                } else if !self.is_welcome_screen {
                    *active = ActiveScreen::Viewport;
                }
            },
            BareKey::Enter => {
                if let Some(name) = self.top_match_name(nav) {
                    switch_session(Some(&name));
                    *active = ActiveScreen::Viewport;
                }
            },
            BareKey::Backspace => {
                self.welcome_search.pop();
                nav.selector_scroll_offset = 0;
            },
            BareKey::Char(c) => {
                self.welcome_search.push(c);
                nav.selector_scroll_offset = 0;
            },
            _ => {},
        }
        true
    }

    pub fn top_match_name(&mut self, nav: &mut Navigation) -> Option<String> {
        let search = self.welcome_search.clone();
        if search.is_empty() {
            return self
                .selectable_sessions()
                .map(|s| s.name.clone())
                .min();
        }
        let matcher = nav.matcher();
        let mut best: Option<(i64, String)> = None;
        for s in self.selectable_sessions() {
            if let Some((score, _)) = matcher.fuzzy_indices(&s.name, &search) {
                let take = match &best {
                    None => true,
                    Some((bs, bn)) => score > *bs || (score == *bs && &s.name < bn),
                };
                if take {
                    best = Some((score, s.name.clone()));
                }
            }
        }
        best.map(|(_, name)| name)
    }

    fn selectable_sessions(&self) -> impl Iterator<Item = &SessionInfo> {
        self.sessions.iter().filter(|s| !s.is_current_session)
    }

    fn header(&self) -> (&'static str, bool) {
        if self.is_welcome_screen {
            ("Hi from Zellij!", false)
        } else {
            ("Switch Session", true)
        }
    }

    pub fn render(
        &mut self,
        nav: &mut Navigation,
        frame: &mut Frame,
        row_start: usize,
        row_end: usize,
        cols: usize,
    ) {
        let (title, show_back_button) = self.header();

        if show_back_button && row_start < row_end {
            draw_back_button(frame, row_start);
        }

        let body_start = if show_back_button {
            row_start.saturating_add(1)
        } else {
            row_start
        };
        let body_end = row_end.saturating_sub(1);
        let body_height = body_end.saturating_sub(body_start);
        if body_height == 0 || cols == 0 {
            return;
        }

        let search = self.welcome_search.clone();
        let cards = self.ordered_cards(nav, &search);
        let layout = PickerLayout::compute(nav, &cards, body_start, body_end, body_height, cols);
        let visible: Vec<&Card> = cards
            .iter()
            .skip(layout.offset)
            .take(layout.visible_count)
            .collect();

        draw_title(title, &layout);
        draw_prompt(&search, &layout);
        draw_scroll_indicators(&layout);
        draw_cards(frame, &visible, &layout);
        draw_new_session_button(frame, &layout);
    }

    fn ordered_cards(&self, nav: &mut Navigation, search: &str) -> Vec<Card> {
        self.scored_session_order(nav, search)
            .into_iter()
            .map(|(idx, name_indices)| Card::from_session(&self.sessions[idx], name_indices))
            .collect()
    }

    fn scored_session_order(&self, nav: &mut Navigation, search: &str) -> Vec<(usize, Vec<usize>)> {
        if search.is_empty() {
            let mut indexed: Vec<(usize, &str)> = self
                .sessions
                .iter()
                .enumerate()
                .filter(|(_, s)| !s.is_current_session)
                .map(|(i, s)| (i, s.name.as_str()))
                .collect();
            indexed.sort_by(|a, b| a.1.cmp(b.1));
            return indexed.into_iter().map(|(i, _)| (i, Vec::new())).collect();
        }

        let matcher = nav.matcher();
        let mut scored: Vec<(usize, i64, Vec<usize>)> = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_current_session)
            .filter_map(|(i, s)| {
                matcher
                    .fuzzy_indices(&s.name, search)
                    .map(|(score, indices)| (i, score, indices))
            })
            .collect();
        scored.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| self.sessions[a.0].name.cmp(&self.sessions[b.0].name))
        });
        scored
            .into_iter()
            .map(|(i, _, indices)| (i, indices))
            .collect()
    }
}

struct Card {
    name_label: String,
    counts_label: String,
    action: ClickAction,
    tab_range: std::ops::Range<usize>,
    pane_range: std::ops::Range<usize>,
    client_range: std::ops::Range<usize>,
    name_indices: Vec<usize>,
}

impl Card {
    fn from_session(s: &SessionInfo, name_indices: Vec<usize>) -> Self {
        let pane_count: usize = s
            .panes
            .panes
            .values()
            .map(|panes| {
                panes
                    .iter()
                    .filter(|p| p.is_selectable && !p.is_suppressed)
                    .count()
            })
            .sum();
        let name = s.name.clone();
        let tab_str = format!("{}", s.tabs.len());
        let pane_str = format!("{}", pane_count);
        let conn_str = format!("{}", s.connected_clients);
        let client_word = if s.connected_clients == 1 {
            "client"
        } else {
            "clients"
        };
        let counts_label = format!(
            "{} tabs, {} panes, {} {}",
            tab_str, pane_str, conn_str, client_word
        );
        // Color ranges mirror the session-manager welcome screen
        // (UnifiedResultsRenderCache::rebuild): tab count color 1, pane and
        // client counts color 2.
        let tab_end = tab_str.len();
        let pane_offset = tab_str.len() + " tabs, ".len();
        let pane_end = pane_offset + pane_str.len();
        let conn_offset = pane_end + " panes, ".len();
        let conn_end = conn_offset + conn_str.len();
        Card {
            name_label: name.clone(),
            counts_label,
            action: ClickAction::SelectSession(name),
            tab_range: 0..tab_end,
            pane_range: pane_offset..pane_end,
            client_range: conn_offset..conn_end,
            name_indices,
        }
    }

    fn content_width(&self) -> usize {
        UnicodeWidthStr::width(self.name_label.as_str())
            .max(UnicodeWidthStr::width(self.counts_label.as_str()))
    }

    fn draw(
        &self,
        frame: &mut Frame,
        card_x: usize,
        content_x: usize,
        row_name: usize,
        row_counts: usize,
        row_end: usize,
    ) {
        print_text_with_coordinates(Text::new(CARD_BULLET), card_x, row_name, None, None);

        let mut name_text = Text::new(&self.name_label).color_range(0, ..);
        if !self.name_indices.is_empty() {
            name_text = name_text.color_indices(3, self.name_indices.clone());
        }
        print_text_with_coordinates(name_text, content_x, row_name, None, None);

        if row_counts < row_end {
            let counts_text = Text::new(&self.counts_label)
                .color_range(1, self.tab_range.clone())
                .color_range(2, self.pane_range.clone())
                .color_range(2, self.client_range.clone());
            print_text_with_coordinates(counts_text, content_x, row_counts, None, None);
        }

        frame.click_regions.push(ClickRegion::tight_range(
            row_name,
            row_counts + 1,
            card_x,
            content_x + self.content_width(),
            self.action.clone(),
        ));
    }
}

struct PickerLayout {
    cols: usize,
    row_end: usize,
    top_y: usize,
    block_height: usize,
    card_x: usize,
    content_x: usize,
    new_session_x: usize,
    new_session_w: usize,
    offset: usize,
    visible_count: usize,
    hidden_above: usize,
    hidden_below: usize,
}

impl PickerLayout {
    fn compute(
        nav: &mut Navigation,
        cards: &[Card],
        body_start: usize,
        body_end: usize,
        body_height: usize,
        cols: usize,
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
            .map(Card::content_width)
            .max()
            .unwrap_or(0);
        let card_w = CARD_INDENT_W + content_w;
        let card_x = cols.saturating_sub(card_w) / 2;
        let content_x = card_x + CARD_INDENT_W;

        let new_session_w = UnicodeWidthStr::width(NEW_SESSION_LABEL);
        let new_session_x = cols.saturating_sub(new_session_w) / 2;

        PickerLayout {
            cols,
            row_end: body_end,
            top_y,
            block_height,
            card_x,
            content_x,
            new_session_x,
            new_session_w,
            offset,
            visible_count,
            hidden_above: offset,
            hidden_below: total_cards.saturating_sub(offset + visible_count),
        }
    }

    fn centered_x(&self, label_w: usize) -> usize {
        self.cols.saturating_sub(label_w) / 2
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

fn draw_title(title: &str, layout: &PickerLayout) {
    let title_y = layout.top_y;
    if title_y < layout.row_end {
        let title_x = layout.centered_x(UnicodeWidthStr::width(title));
        print_text_with_coordinates(Text::new(title), title_x, title_y, None, None);
    }
}

fn draw_prompt(search: &str, layout: &PickerLayout) {
    let prompt_y = layout.top_y + 2;
    if prompt_y >= layout.row_end {
        return;
    }
    let prompt_label = "Session: ";
    let prompt_full = format!("{}{}_", prompt_label, search);
    let prompt_x = if layout.visible_count > 0 {
        layout.content_x
    } else {
        layout.new_session_x
    };
    let label_chars = prompt_label.chars().count();
    let total_chars = prompt_full.chars().count();
    let prompt_text = Text::new(&prompt_full).color_range(3, label_chars..total_chars);
    print_text_with_coordinates(prompt_text, prompt_x, prompt_y, None, None);
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

fn draw_cards(frame: &mut Frame, visible: &[&Card], layout: &PickerLayout) {
    let first_card_y = layout.top_y + 4;
    for (i, card) in visible.iter().enumerate() {
        let row_name = first_card_y + i * 2;
        if row_name >= layout.row_end {
            break;
        }
        card.draw(
            frame,
            layout.card_x,
            layout.content_x,
            row_name,
            row_name + 1,
            layout.row_end,
        );
    }
}

fn draw_new_session_button(frame: &mut Frame, layout: &PickerLayout) {
    let new_session_y = layout.top_y + layout.block_height.saturating_sub(1);
    if new_session_y >= layout.row_end {
        return;
    }
    print_text_with_coordinates(
        Text::new(NEW_SESSION_LABEL).color_range(3, ..),
        layout.new_session_x,
        new_session_y,
        None,
        None,
    );
    frame.click_regions.push(ClickRegion::tight(
        new_session_y,
        layout.new_session_x,
        layout.new_session_x + layout.new_session_w,
        ClickAction::OpenNewSessionPrompt,
    ));
}

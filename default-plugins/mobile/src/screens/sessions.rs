use fuzzy_matcher::FuzzyMatcher;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::click::{ClickAction, ClickRegion};
use crate::frame::Frame;
use crate::navigation::Navigation;
use crate::screens::ActiveScreen;

#[derive(Default)]
pub struct SessionsScreen {
    pub sessions: Vec<SessionInfo>,
    /// Fuzzy-search buffer for the "Session:" prompt. Empty when the
    /// prompt has no input. Cleared when the selector closes.
    pub welcome_search: String,
    /// does this session function as the mobile version of the welcome screen
    pub is_welcome_screen: bool,
}

impl SessionsScreen {
    pub fn select_session(&self, active: &mut ActiveScreen, name: &str) -> bool {
        switch_session(Some(name));
        *active = ActiveScreen::Viewport;
        true
    }

    /// Capture keys for the "Session:" fuzzy-search prompt.
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
                if let Some(name) = self.welcome_top_match_name(nav) {
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

    pub fn welcome_top_match_name(&mut self, nav: &mut Navigation) -> Option<String> {
        let search = self.welcome_search.clone();
        if search.is_empty() {
            return self
                .sessions
                .iter()
                .filter(|s| !s.is_current_session)
                .map(|s| s.name.clone())
                .min();
        }
        let matcher = nav.matcher();
        let mut best: Option<(i64, String)> = None;
        for s in self.sessions.iter() {
            if s.is_current_session {
                continue;
            }
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

    pub fn render(
        &mut self,
        nav: &mut Navigation,
        frame: &mut Frame,
        row_start: usize,
        row_end: usize,
        cols: usize,
    ) {
        let (title, show_back_button) = if self.is_welcome_screen {
            ("Hi from Zellij!", false)
        } else {
            ("Switch Session", true)
        };
        self.render_welcome(nav, frame, row_start, row_end, cols, title, show_back_button);
    }

    fn render_welcome(
        &mut self,
        nav: &mut Navigation,
        frame: &mut Frame,
        row_start: usize,
        row_end: usize,
        cols: usize,
        title: &str,
        show_back_button: bool,
    ) {
        if show_back_button && row_start < row_end {
            let back_label = "[← BACK]";
            let back_w = UnicodeWidthStr::width(back_label);
            print_text_with_coordinates(
                Text::new(back_label).color_range(3, ..),
                0,
                row_start,
                None,
                None,
            );
            frame.click_regions.push(ClickRegion::tight(
                row_start,
                0,
                back_w,
                ClickAction::CollapseSelector,
            ));
        }
        let row_start = if show_back_button {
            row_start.saturating_add(1)
        } else {
            row_start
        };

        let row_end = row_end.saturating_sub(1);
        let body_height = row_end.saturating_sub(row_start);
        if body_height == 0 || cols == 0 {
            return;
        }

        let new_session_label = "+ New Session";

        struct Card {
            name_label: String,
            counts_label: String,
            action: ClickAction,
            tab_range: std::ops::Range<usize>,
            pane_range: std::ops::Range<usize>,
            client_range: std::ops::Range<usize>,
            name_indices: Vec<usize>,
        }

        let search = self.welcome_search.clone();

        let order: Vec<(usize, Vec<usize>)> = if search.is_empty() {
            let mut indexed: Vec<(usize, &str)> = self
                .sessions
                .iter()
                .enumerate()
                .filter(|(_, s)| !s.is_current_session)
                .map(|(i, s)| (i, s.name.as_str()))
                .collect();
            indexed.sort_by(|a, b| a.1.cmp(b.1));
            indexed.into_iter().map(|(i, _)| (i, Vec::new())).collect()
        } else {
            let matcher = nav.matcher();
            let mut scored: Vec<(usize, i64, Vec<usize>)> = self
                .sessions
                .iter()
                .enumerate()
                .filter(|(_, s)| !s.is_current_session)
                .filter_map(|(i, s)| {
                    matcher
                        .fuzzy_indices(&s.name, &search)
                        .map(|(score, indices)| (i, score, indices))
                })
                .collect();
            scored.sort_by(|a, b| {
                b.1.cmp(&a.1).then_with(|| {
                    self.sessions[a.0]
                        .name
                        .cmp(&self.sessions[b.0].name)
                })
            });
            scored.into_iter().map(|(i, _, indices)| (i, indices)).collect()
        };

        let cards: Vec<Card> = order
            .into_iter()
            .map(|(session_idx, indices)| {
                let s = &self.sessions[session_idx];
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
                // Byte-offset color ranges mirror the session-manager
                // welcome screen (`UnifiedResultsRenderCache::rebuild`):
                // tab count in color 1; pane and client counts in color 2.
                // Digits are ASCII so byte offsets equal column offsets.
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
                    name_indices: indices,
                }
            })
            .collect();

        let total_cards = cards.len();
        let max_visible_cards = body_height.saturating_sub(6) / 2;
        let max_visible_cards = max_visible_cards.min(total_cards);

        let max_offset = total_cards.saturating_sub(max_visible_cards);
        let offset = nav.selector_scroll_offset.min(max_offset);
        nav.selector_scroll_offset = offset;
        let visible_count = total_cards.saturating_sub(offset).min(max_visible_cards);

        let block_height = if visible_count == 0 {
            5.min(body_height)
        } else {
            (6 + 2 * visible_count).min(body_height)
        };

        let top_y = row_start + body_height.saturating_sub(block_height) / 2;

        let visible_slice: Vec<&Card> = cards.iter().skip(offset).take(visible_count).collect();
        const CARD_BULLET: &str = "- ";
        const CARD_INDENT_W: usize = 2;
        let content_w = visible_slice
            .iter()
            .map(|c| {
                UnicodeWidthStr::width(c.name_label.as_str())
                    .max(UnicodeWidthStr::width(c.counts_label.as_str()))
            })
            .max()
            .unwrap_or(0);
        let card_w = CARD_INDENT_W + content_w;
        let card_x = cols.saturating_sub(card_w) / 2;
        let content_x = card_x + CARD_INDENT_W;

        let title_w = UnicodeWidthStr::width(title);
        let title_x = cols.saturating_sub(title_w) / 2;
        let title_y = top_y;
        if title_y < row_end {
            print_text_with_coordinates(Text::new(title), title_x, title_y, None, None);
        }

        let prompt_label = "Session: ";
        let prompt_body = format!("{}_", search);
        let prompt_full = format!("{}{}", prompt_label, prompt_body);
        let new_session_w = UnicodeWidthStr::width(new_session_label);
        let new_session_x = cols.saturating_sub(new_session_w) / 2;
        let prompt_x = if visible_count > 0 {
            content_x
        } else {
            new_session_x
        };
        let prompt_y = top_y + 2;
        if prompt_y < row_end {
            let label_chars = prompt_label.chars().count();
            let total_chars = prompt_full.chars().count();
            let prompt_text =
                Text::new(&prompt_full).color_range(3, label_chars..total_chars);
            print_text_with_coordinates(prompt_text, prompt_x, prompt_y, None, None);
        }

        let hidden_above = offset;
        let hidden_below = total_cards.saturating_sub(offset + visible_count);
        let indicator_x = |label_w: usize| -> usize {
            cols.saturating_sub(label_w) / 2
        };
        if visible_count > 0 && hidden_above > 0 {
            let top_indicator_y = top_y + 3;
            if top_indicator_y < row_end {
                let label = format!("\u{2191} [+{}]", hidden_above);
                let label_w = UnicodeWidthStr::width(label.as_str());
                print_text_with_coordinates(
                    Text::new(&label).color_range(1, ..),
                    indicator_x(label_w),
                    top_indicator_y,
                    None,
                    None,
                );
            }
        }
        if visible_count > 0 && hidden_below > 0 {
            let bottom_indicator_y = top_y + 4 + 2 * visible_count;
            if bottom_indicator_y < row_end {
                let label = format!("\u{2193} [+{}]", hidden_below);
                let label_w = UnicodeWidthStr::width(label.as_str());
                print_text_with_coordinates(
                    Text::new(&label).color_range(1, ..),
                    indicator_x(label_w),
                    bottom_indicator_y,
                    None,
                    None,
                );
            }
        }

        let sessions_start_y = top_y + 4;
        for (i, c) in visible_slice.iter().enumerate() {
            let row_name = sessions_start_y + i * 2;
            let row_counts = row_name + 1;
            if row_name >= row_end {
                break;
            }
            print_text_with_coordinates(
                Text::new(CARD_BULLET),
                card_x,
                row_name,
                None,
                None,
            );
            let mut name_text = Text::new(&c.name_label).color_range(0, ..);
            if !c.name_indices.is_empty() {
                name_text = name_text.color_indices(3, c.name_indices.clone());
            }
            print_text_with_coordinates(name_text, content_x, row_name, None, None);
            if row_counts < row_end {
                let counts_text = Text::new(&c.counts_label)
                    .color_range(1, c.tab_range.clone())
                    .color_range(2, c.pane_range.clone())
                    .color_range(2, c.client_range.clone());
                print_text_with_coordinates(counts_text, content_x, row_counts, None, None);
            }
            let content_click_w = UnicodeWidthStr::width(c.name_label.as_str())
                .max(UnicodeWidthStr::width(c.counts_label.as_str()));
            frame.click_regions.push(ClickRegion::tight_range(
                row_name,
                row_counts + 1,
                card_x,
                content_x + content_click_w,
                c.action.clone(),
            ));
        }

        let new_session_y = top_y + block_height.saturating_sub(1);
        if new_session_y < row_end {
            print_text_with_coordinates(
                Text::new(new_session_label).color_range(3, ..),
                new_session_x,
                new_session_y,
                None,
                None,
            );
            frame.click_regions.push(ClickRegion::tight(
                new_session_y,
                new_session_x,
                new_session_x + new_session_w,
                ClickAction::OpenNewSessionPrompt,
            ));
        }
    }
}

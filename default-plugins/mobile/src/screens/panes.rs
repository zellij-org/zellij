//! The unified Change Pane navigator — a welcome-style centered list of
//! every pane grouped (inline) under its tab. Owns the fuzzy-search
//! buffer. Selecting a pane updates the shared workspace selection; the
//! footer carries the "+ New Tab" / "+ New Pane" affordances.

use fuzzy_matcher::FuzzyMatcher;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::click::{ClickAction, ClickRegion};
use crate::frame::Frame;
use crate::navigation::Navigation;
use crate::screens::ActiveScreen;
use crate::workspace::{pane_id_of, Workspace};

/// Pane card entry — the only kind of item in the Change Pane
/// navigator's scrollable list. Two rows: `title_label` above
/// `"<tab_label>, <activity_label>"`.
struct PaneCard {
    title_label: String,
    tab_label: String,
    activity_label: String,
    action: ClickAction,
    title_indices: Vec<usize>,
    is_current: bool,
}

/// Panes selector state.
#[derive(Default)]
pub struct PanesScreen {
    /// Fuzzy-search buffer for the "Pane:" prompt. Empty when the prompt
    /// has no input. Cleared when the selector opens / closes.
    pub panes_search: String,
}

impl PanesScreen {
    /// Capture keys for the "Pane:" fuzzy-search prompt. Enter resolves
    /// to the highest-scoring pane and returns it so the caller can
    /// apply the cross-module selection side effects (the selection
    /// touches the workspace, fit, and viewport panning). Esc with a
    /// non-empty buffer clears it; an empty Esc closes the selector.
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

    /// Pick the highest-scoring pane for the prompt — what `Enter`
    /// should select. With an empty search term, returns the first pane
    /// in tab/display order. Returns `None` only when no panes are
    /// visible at all. Matching is against pane titles (falling back to
    /// `#<id>` when the title is empty).
    pub fn panes_top_match(
        &mut self,
        ws: &Workspace,
        nav: &mut Navigation,
    ) -> Option<(usize, PaneId)> {
        let tabs: Vec<TabInfo> = ws.tabs_in_order().into_iter().cloned().collect();
        let mut entries: Vec<(String, usize, PaneId)> = Vec::new();
        for tab in &tabs {
            for pane in ws.panes_for_tab(tab.position) {
                let id = pane_id_of(pane);
                let title = if pane.title.is_empty() {
                    format!("#{}", pane.id)
                } else {
                    pane.title.clone()
                };
                entries.push((title, tab.position, id));
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

    /// Render the unified Change Pane selector.
    pub fn render(
        &mut self,
        ws: &Workspace,
        nav: &mut Navigation,
        frame: &mut Frame,
        row_start: usize,
        row_end: usize,
        cols: usize,
    ) {
    // Back button — same affordance as the Switch Session view. The
    // pane-menu is only reachable from the embedded viewport (via
    // the hamburger menu's "Change Pane" item or the top-bar pane-
    // segment tap), so the back action always has a meaningful
    // return target. `CollapseSelector` is the single source of
    // truth for "leave the selector, return to the viewport".
    if row_start < row_end {
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
    let row_start = row_start.saturating_add(1);

    // Bottom-row reservation matches `SessionsScreen::render` so
    // "+ New Tab" never sits flush against the modifier bar (when the
    // soft keyboard is up) or the screen edge (when it is not).
    let row_end = row_end.saturating_sub(1);
    let body_height = row_end.saturating_sub(row_start);
    if body_height == 0 || cols == 0 {
        return;
    }

    let title = "Switch Pane";
    let prompt_label = "Pane: ";
    let new_tab_label = "+ New Tab";
    let new_pane_label = "+ New Pane";
    /// Bullet rendered at the start of every pane card's title row.
    /// Two cells wide (`-` + space). The meta row is indented by
    /// the same amount so the metadata text aligns under the title
    /// text rather than the bullet — the bullet visually marks the
    /// top of each two-row block, making cards distinguishable
    /// when many are stacked together.
    const CARD_BULLET: &str = "- ";
    /// Cells reserved at the left of the card column for
    /// `CARD_BULLET` on the title row (and a matching blank indent
    /// on the meta row). Width of `CARD_BULLET` evaluated at
    /// definition time so the constant can be used in `const`
    /// arithmetic and width-derived calculations.
    const CARD_INDENT_W: usize = 2;
    /// Label that replaces the activity timestamp on the
    /// currently-viewed pane's card. Painted in emphasis-0 so it
    /// stands out from the standard activity row (`Active <n>
    /// ago`, unbold) without using the marker palette already
    /// reserved for action affordances ("+ New …", "[← BACK]") in
    /// emphasis-3.
    const CURRENT_PANE_LABEL: &str = "[CURRENT PANE]";
    /// Cells of whitespace between the "+ New Tab" and "+ New Pane"
    /// affordances on the footer row. Small enough that the combined
    /// label still fits on a 40-column mobile screen, large enough
    /// to read as two separate affordances rather than one merged
    /// label.
    const FOOTER_GAP: usize = 4;
    let search = self.panes_search.clone();

    // Target tab for the footer's "+ New Pane" affordance. Defaults
    // to the currently-viewed tab (`current_tab`), which falls back
    // to the first visible tab if nothing is selected yet. `None`
    // means there are no tabs at all — render only "+ New Tab" in
    // that case, since the host has nowhere to attach a new pane.
    let new_pane_target_tab: Option<usize> =
        ws.current_tab().map(|t| t.position);

    // Identify the currently-viewed pane so the card list can mark
    // it. Uses `current_tab` + `current_pane` (rather than reading
    // `selected_*` directly) so the marker honours the same
    // fallbacks the embedded viewport uses — i.e., when no explicit
    // selection has been made yet, the first visible pane of the
    // first visible tab is what the user actually sees, and that is
    // what should be marked.
    let current_tab_position: Option<usize> =
        ws.current_tab().map(|t| t.position);
    let current_pane_id: Option<PaneId> =
        ws.current_pane().as_ref().map(pane_id_of);

    // Snapshot every pane in tab/display order. Each entry carries
    // the data the renderer needs and the per-pane scoring inputs
    // (`title_label` is matched against `search`).
    let now = crate::unix_now();
    let tabs: Vec<TabInfo> = ws.tabs_in_order().into_iter().cloned().collect();
    struct PaneEntry {
        title_label: String,
        tab_label: String,
        activity_label: String,
        action: ClickAction,
        is_current: bool,
    }
    let mut entries: Vec<PaneEntry> = Vec::new();
    for tab in &tabs {
        for pane in ws.panes_for_tab(tab.position).into_iter().cloned().collect::<Vec<_>>() {
            let id = pane_id_of(&pane);
            let title_label = if pane.title.is_empty() {
                format!("#{}", pane.id)
            } else {
                pane.title.clone()
            };
            let is_current = current_tab_position == Some(tab.position)
                && current_pane_id == Some(id);
            // Current pane swaps the `Active <n> ago` timestamp for
            // a fixed `[CURRENT PANE]` label. Computing the label
            // once at entry-build time keeps the meta-row width
            // calculation consistent — `card_w` reflects whichever
            // text will actually be painted.
            let activity_label = if is_current {
                CURRENT_PANE_LABEL.to_string()
            } else {
                let last_activity = ws.pane_last_activity.get(&id).copied();
                crate::render::format_time_ago(last_activity, now)
            };
            entries.push(PaneEntry {
                title_label,
                tab_label: tab.name.clone(),
                activity_label,
                action: ClickAction::SelectPane {
                    tab_position: tab.position,
                    pane_id: id,
                },
                is_current,
            });
        }
    }

    // Convert each `PaneEntry` into a `PaneCard`, optionally
    // annotated with fuzzy-match indices.
    let make_card = |entry: PaneEntry, title_indices: Vec<usize>| -> PaneCard {
        PaneCard {
            title_label: entry.title_label,
            tab_label: entry.tab_label,
            activity_label: entry.activity_label,
            action: entry.action,
            title_indices,
            is_current: entry.is_current,
        }
    };

    // Build the scrollable card list. Empty search keeps the panes
    // in tab/display order so the user sees the same ordering the
    // embedded viewport navigates. Non-empty search filters by
    // fuzzy-match against the title, sorted by score descending and
    // tie-broken alphabetically.
    let cards: Vec<PaneCard> = if search.is_empty() {
        entries
            .into_iter()
            .map(|entry| make_card(entry, Vec::new()))
            .collect()
    } else {
        let matcher = nav.matcher();
        let mut scored: Vec<(i64, Vec<usize>, PaneEntry)> = entries
            .into_iter()
            .filter_map(|entry| {
                matcher
                    .fuzzy_indices(&entry.title_label, &search)
                    .map(|(score, indices)| (score, indices, entry))
            })
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.2.title_label.cmp(&b.2.title_label))
        });
        scored
            .into_iter()
            .map(|(_, indices, entry)| make_card(entry, indices))
            .collect()
    };

    // Fixed-row budget: title + blank + prompt + blank-or-scroll-up
    // + blank-or-scroll-down + footer = 6. The cards area (each
    // card is two rows) gets whatever remains.
    let max_items_rows = body_height.saturating_sub(6);
    let max_visible_cards = max_items_rows / 2;
    let max_visible_cards = max_visible_cards.min(cards.len());

    let max_offset = cards.len().saturating_sub(max_visible_cards);
    let offset = nav.selector_scroll_offset.min(max_offset);
    nav.selector_scroll_offset = offset;

    let visible_count = cards.len().saturating_sub(offset).min(max_visible_cards);
    let visible_slice: Vec<&PaneCard> =
        cards.iter().skip(offset).take(visible_count).collect();

    // Card column width: `CARD_INDENT_W` (for the bullet) plus the
    // max of any visible card's title or meta row. All card content
    // anchors at `card_x + CARD_INDENT_W`; the bullet sits at
    // `card_x` on the title row and the meta row leaves the slot
    // blank so it indents under the title text.
    let content_w = visible_slice
        .iter()
        .map(|c| {
            // Meta row is `"<tab>, <activity>"`. Account for the
            // ", " separator (two cells, ASCII) so the click
            // region and centering use the actual painted span.
            let meta_w = UnicodeWidthStr::width(c.tab_label.as_str())
                + 2
                + UnicodeWidthStr::width(c.activity_label.as_str());
            UnicodeWidthStr::width(c.title_label.as_str()).max(meta_w)
        })
        .max()
        .unwrap_or(0);
    let card_w = CARD_INDENT_W + content_w;
    let card_x = cols.saturating_sub(card_w) / 2;
    // Column where each card's text content begins — the bullet
    // sits to the left of this column on the title row, and the
    // meta row indents flush to this column with the slot blank.
    let content_x = card_x + CARD_INDENT_W;

    // Footer geometry. When a target tab exists, render "+ New Tab"
    // and "+ New Pane" side by side separated by `FOOTER_GAP`
    // whitespace cells, centered as one block. When no tab is
    // available, fall back to "+ New Tab" alone (centered on its
    // own width); `new_pane_x` is left at zero — the renderer below
    // guards on `new_pane_target_tab.is_some()` before reading it.
    let new_tab_w = UnicodeWidthStr::width(new_tab_label);
    let new_pane_w = UnicodeWidthStr::width(new_pane_label);
    let (new_tab_x, new_pane_x) = if new_pane_target_tab.is_some() {
        let total = new_tab_w + FOOTER_GAP + new_pane_w;
        let block_x = cols.saturating_sub(total) / 2;
        (block_x, block_x + new_tab_w + FOOTER_GAP)
    } else {
        let block_x = cols.saturating_sub(new_tab_w) / 2;
        (block_x, 0)
    };

    // Block height: 6 fixed rows (title + blank + prompt + scroll-up
    // or blank + scroll-down or blank + footer) plus 2 rows per
    // visible card. Empty-state collapses the two scroll rows to a
    // single blank, matching the 5-row "title + blank + prompt +
    // blank + footer" minimal layout used by
    // `SessionsScreen::render`.
    let visible_items_height = 2 * visible_count;
    let block_height = if visible_count == 0 {
        5.min(body_height)
    } else {
        (6 + visible_items_height).min(body_height)
    };
    let top_y = row_start + body_height.saturating_sub(block_height) / 2;

    // Title row — unstyled, centered on `cols` so it sits on the
    // screen's vertical axis regardless of `card_w`.
    let title_w = UnicodeWidthStr::width(title);
    let title_x = cols.saturating_sub(title_w) / 2;
    let title_y = top_y;
    if title_y < row_end {
        print_text_with_coordinates(Text::new(title), title_x, title_y, None, None);
    }

    // "Pane: <buffer>_" fuzzy-search prompt. "Pane: " is unstyled;
    // the user-typed buffer plus a trailing underscore cursor glyph
    // are emphasis-3 (mirrors the welcome screen's "Session:"
    // prompt). Anchored to the leftmost edge of the visible card
    // column when there are cards, falling back to the footer's
    // left edge when there are none, so the prompt visually
    // anchors to the same column the user is scanning below it.
    let prompt_body = format!("{}_", search);
    let prompt_full = format!("{}{}", prompt_label, prompt_body);
    // Prompt aligns with the text content column (`content_x`) so
    // the typed buffer sits directly above each card's title text,
    // skipping the bullet column to its left. When no cards are
    // visible it falls back to the footer's left edge.
    let prompt_x = if visible_count > 0 { content_x } else { new_tab_x };
    let prompt_y = top_y + 2;
    if prompt_y < row_end {
        let label_chars = prompt_label.chars().count();
        let total_chars = prompt_full.chars().count();
        let prompt_text =
            Text::new(&prompt_full).color_range(3, label_chars..total_chars);
        print_text_with_coordinates(prompt_text, prompt_x, prompt_y, None, None);
    }

    // Scroll indicators flank the cards area. They replace the
    // blank rows when there is content hidden in that direction;
    // otherwise those rows render empty. Emphasis-1 distinguishes
    // them from the title (unstyled) and the footer affordances
    // (emphasis-3), matching the welcome-session scroll-indicator
    // styling.
    let total_cards = cards.len();
    let hidden_above = offset;
    let hidden_below = total_cards.saturating_sub(offset + visible_count);
    let indicator_x = |label_w: usize| cols.saturating_sub(label_w) / 2;
    if visible_count > 0 && hidden_above > 0 {
        let up_y = top_y + 3;
        if up_y < row_end {
            let label = format!("\u{2191} [+{}]", hidden_above);
            let lw = UnicodeWidthStr::width(label.as_str());
            print_text_with_coordinates(
                Text::new(&label).color_range(1, ..),
                indicator_x(lw),
                up_y,
                None,
                None,
            );
        }
    }
    if visible_count > 0 && hidden_below > 0 {
        let down_y = top_y + 4 + visible_items_height;
        if down_y < row_end {
            let label = format!("\u{2193} [+{}]", hidden_below);
            let lw = UnicodeWidthStr::width(label.as_str());
            print_text_with_coordinates(
                Text::new(&label).color_range(1, ..),
                indicator_x(lw),
                down_y,
                None,
                None,
            );
        }
    }

    // Cards area: starts at `top_y + 4` (title + blank + prompt +
    // blank/scroll-up). Each card takes two rows so the loop bumps
    // `cursor_y` by 2 per iteration.
    let mut cursor_y = top_y + 4;
    for card in &visible_slice {
        if cursor_y >= row_end {
            break;
        }
        let activity_y = cursor_y + 1;
        // Bullet — `CARD_BULLET` painted at the card column's left
        // edge on the title row only. The meta row leaves the slot
        // blank so its content indents under the title text.
        print_text_with_coordinates(
            Text::new(CARD_BULLET),
            card_x,
            cursor_y,
            None,
            None,
        );
        // Title row: emphasis-2 base, with fuzzy-match indices
        // painted in emphasis-3 (matches the welcome-session card's
        // emphasis-3 hits).
        let mut title_text = Text::new(&card.title_label).color_range(2, ..);
        if !card.title_indices.is_empty() {
            title_text = title_text.color_indices(3, card.title_indices.clone());
        }
        print_text_with_coordinates(title_text, content_x, cursor_y, None, None);
        // Meta row split into three texts so the tab segment can
        // carry emphasis-1 cleanly while the activity segment
        // carries its own styling (`unbold_all` for ordinary cards,
        // `color_range(0, ..)` for the current pane's
        // `[CURRENT PANE]` label). Combining both styles into a
        // single `Text` was attempted and produced ambiguous
        // output — the host's `style_of_index` reapplies styling
        // per-color-range without composing bold-state changes, so
        // the activity segment kept the tab segment's emphasis
        // bold attribute. Three separate prints avoid the problem
        // and keep each segment self-contained.
        let tab_w = UnicodeWidthStr::width(card.tab_label.as_str());
        let activity_w = UnicodeWidthStr::width(card.activity_label.as_str());
        let sep = ", ";
        let sep_w = sep.len(); // ASCII → bytes == cells
        if activity_y < row_end {
            let tab_text = Text::new(&card.tab_label).color_range(1, ..);
            print_text_with_coordinates(
                tab_text,
                content_x,
                activity_y,
                None,
                None,
            );
            let sep_x = content_x + tab_w;
            print_text_with_coordinates(Text::new(sep), sep_x, activity_y, None, None);
            let activity_x = sep_x + sep_w;
            // Current pane → `[CURRENT PANE]` in emphasis-0 (a
            // dedicated "this one" data-label colour, distinct from
            // the emphasis-3 reserved for action affordances).
            // Other panes → activity timestamp in unbold.
            let activity_text = if card.is_current {
                Text::new(&card.activity_label).color_range(0, ..)
            } else {
                Text::new(&card.activity_label).unbold_all()
            };
            print_text_with_coordinates(
                activity_text,
                activity_x,
                activity_y,
                None,
                None,
            );
        }
        let meta_w = tab_w + sep_w + activity_w;
        let content_click_w =
            UnicodeWidthStr::width(card.title_label.as_str()).max(meta_w);
        // Click region spans the bullet slot too so a tap that
        // lands on the bullet still selects the pane — the bullet
        // is part of the card visually and should be part of the
        // tap target.
        frame.click_regions.push(ClickRegion::tight_range(
            cursor_y,
            activity_y + 1,
            card_x,
            content_x + content_click_w,
            card.action.clone(),
        ));
        cursor_y += 2;
    }

    // Footer row pinned at the bottom of the block. `block_height -
    // 1` lands on the final row of the centered block regardless of
    // how many cards are visible, matching how
    // `SessionsScreen::render` pins "+ New Session". Renders
    // "+ New Tab" and (when a target tab exists) "+ New Pane" side
    // by side as a single centered block.
    let footer_y = top_y + block_height.saturating_sub(1);
    if footer_y < row_end {
        print_text_with_coordinates(
            Text::new(new_tab_label).color_range(3, ..),
            new_tab_x,
            footer_y,
            None,
            None,
        );
        frame.click_regions.push(ClickRegion::tight(
            footer_y,
            new_tab_x,
            new_tab_x + new_tab_w,
            ClickAction::NewTab,
        ));
        if let Some(tab_position) = new_pane_target_tab {
            print_text_with_coordinates(
                Text::new(new_pane_label).color_range(3, ..),
                new_pane_x,
                footer_y,
                None,
                None,
            );
            frame.click_regions.push(ClickRegion::tight(
                footer_y,
                new_pane_x,
                new_pane_x + new_pane_w,
                ClickAction::NewPaneInTab { tab_position },
            ));
        }
    }
}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    /// Build a `State` carrying `tab_count` tabs each with one
    /// terminal pane. Tabs are at positions 0..tab_count, panes use
    /// ids 100..100+tab_count. Selected tab/pane are tab 0 / pane 100.
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
    /// With one tab + one pane the Panes selector emits these click
    /// regions in render order: "[← BACK]" (top-left), the pane
    /// card, then the two sister affordances on the footer row
    /// ("+ New Tab" followed by "+ New Pane"). The tab name is
    /// inlined on the pane card's metadata row rather than rendered
    /// as a separate non-interactive header — see
    /// `render_panes_menu` for the inline-tab layout.
    #[test]
    fn panes_menu_one_tab_emits_four_click_regions() {
        let mut state = state_with_tabs_and_panes(1);
        let cols = 40;
        // Plenty of vertical space so every item is visible.
        state.panes.render(&state.workspace, &mut state.navigation, &mut state.frame, 0, 20, cols);
        // [← BACK] + 1 PaneCard + footer (NewTab + NewPane) = 4
        // regions.
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

    /// Tab count does not multiply the footer affordances — "+ New
    /// Tab" and "+ New Pane" are global sisters now, not per-tab.
    /// With two tabs the selector emits chrome (back) + one card per
    /// pane + the two footer regions. "+ New Pane" targets the
    /// currently-viewed tab (`state.current_tab()`), which the
    /// fixture pins at tab 0.
    #[test]
    fn panes_menu_two_tabs_emits_single_footer_new_pane() {
        let mut state = state_with_tabs_and_panes(2);
        let cols = 40;
        state.panes.render(&state.workspace, &mut state.navigation, &mut state.frame, 0, 20, cols);
        // [← BACK] + 2 PaneCards + footer (NewTab + NewPane) = 5
        // regions.
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
        // Single + New Pane affordance, targeting the currently-
        // viewed tab (the fixture selects tab 0).
        assert_eq!(new_panes, vec![0]);

        let new_tabs = state
            .frame
            .click_regions
            .iter()
            .filter(|r| matches!(r.action, ClickAction::NewTab))
            .count();
        assert_eq!(new_tabs, 1);
    }

    /// "+ New Tab" and "+ New Pane" share the footer row — same
    /// `row_start`, with "+ New Pane" to the right of "+ New Tab"
    /// so the two read left-to-right as sisters.
    #[test]
    fn footer_affordances_share_a_row_and_order_left_to_right() {
        let mut state = state_with_tabs_and_panes(1);
        state.panes.render(&state.workspace, &mut state.navigation, &mut state.frame, 0, 20, 40);
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

    /// Click dispatch round-trip: tapping inside the footer's
    /// "+ New Pane" span resolves to `ClickAction::NewPaneInTab`
    /// targeting the currently-viewed tab. The footer anchors the
    /// affordance to a centered block (not column 0), so the test
    /// reads the region's own `col_start`.
    #[test]
    fn click_on_new_pane_row_resolves_to_action() {
        let mut state = state_with_tabs_and_panes(2);
        // Move the selection to tab 1 so "+ New Pane" targets it,
        // proving the footer follows the user's current tab.
        state.workspace.selected_tab_position = Some(1);
        state.workspace.selected_pane_id = Some(PaneId::Terminal(101));
        state.panes.render(&state.workspace, &mut state.navigation, &mut state.frame, 0, 20, 40);
        let new_pane_region = state
            .frame
            .click_regions
            .iter()
            .find(|r| {
                matches!(
                    r.action,
                    ClickAction::NewPaneInTab { tab_position: 1 }
                )
            })
            .expect("expected NewPaneInTab targeting the selected tab")
            .clone();
        assert_eq!(
            state.frame.click_to_action(
                new_pane_region.row_start,
                new_pane_region.col_start,
            ),
            Some(ClickAction::NewPaneInTab { tab_position: 1 })
        );
    }

    /// The Panes selector's first click region is always the
    /// "[← BACK]" affordance at row 0, col 0 (matching the Switch
    /// Session view's welcome-style layout). Tapping that cell
    /// dispatches `CollapseSelector`, returning the user to the
    /// embedded viewport.
    #[test]
    fn panes_menu_back_button_at_top_left_collapses_selector() {
        let mut state = state_with_tabs_and_panes(1);
        state.panes.render(&state.workspace, &mut state.navigation, &mut state.frame, 0, 20, 40);
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

    /// Pane cards are two rows (title + activity); the click region
    /// must span both rows so a tap on either row selects the pane.
    /// Verifies the row-range spans exactly two rows and that
    /// taps on both rows dispatch the same `SelectPane`.
    #[test]
    fn pane_card_click_region_spans_two_rows() {
        let mut state = state_with_tabs_and_panes(1);
        state.panes.render(&state.workspace, &mut state.navigation, &mut state.frame, 0, 20, 40);
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
            state.frame.click_to_action(pane_region.row_start, pane_region.col_start),
            expected,
            "tap on title row should select the pane",
        );
        assert_eq!(
            state.frame.click_to_action(
                pane_region.row_start + 1,
                pane_region.col_start,
            ),
            expected,
            "tap on activity row should select the pane",
        );
    }

    /// The footer affordances ("+ New Tab" and "+ New Pane") stay
    /// visible even with an active fuzzy-search buffer — they are
    /// global sisters that the user can reach regardless of how
    /// they have filtered the card list.
    #[test]
    fn panes_menu_fuzzy_filter_keeps_footer_visible() {
        let mut state = state_with_tabs_and_panes(2);
        // Give the panes distinct titles so the matcher has
        // something to score against.
        for (i, panes) in state.workspace.panes_by_tab_position.iter_mut() {
            for pane in panes.iter_mut() {
                pane.title = format!("alpha-{}", i);
            }
        }
        state.panes.panes_search = "alpha".to_string();
        state.panes.render(&state.workspace, &mut state.navigation, &mut state.frame, 0, 20, 60);

        // Both footer affordances must still be present.
        let new_tab_count = state
            .frame
            .click_regions
            .iter()
            .filter(|r| matches!(r.action, ClickAction::NewTab))
            .count();
        assert_eq!(new_tab_count, 1, "+ New Tab must stay visible during search");
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
        // [← BACK] + 2 matching pane cards + footer (2) = 5.
        assert_eq!(state.frame.click_regions.len(), 5);
    }

    /// The current-pane card's meta row substitutes its activity
    /// timestamp for the `[CURRENT PANE]` label, so the click
    /// region for the current card widens to include those 14
    /// cells of text. The non-current card's span only carries its
    /// activity timestamp, which on the fixture (no recorded
    /// activity) is `—` (1 cell).
    ///
    /// Painted content is not asserted directly — `print_text_with_
    /// coordinates` writes to stdout, which libtest swallows. The
    /// click-region geometry is what guarantees the indicator
    /// label has actually been allocated row-and-column space, so
    /// geometry is the right level of contract to test against
    /// here.
    #[test]
    fn current_pane_card_span_includes_current_pane_label() {
        let mut state = state_with_tabs_and_panes(2);
        // Pane 100 is on tab 0, pane 101 is on tab 1. Selecting
        // (tab 0, pane 100) makes pane 100 the current pane; pane
        // 101 keeps its ordinary activity row.
        state.workspace.selected_tab_position = Some(0);
        state.workspace.selected_pane_id = Some(PaneId::Terminal(100));
        state.panes.render(&state.workspace, &mut state.navigation, &mut state.frame, 0, 20, 60);

        let select_regions: Vec<&ClickRegion> = state
            .frame
            .click_regions
            .iter()
            .filter(|r| matches!(r.action, ClickAction::SelectPane { .. }))
            .collect();
        assert_eq!(select_regions.len(), 2);

        // Walk both regions and identify which one targets the
        // current pane (#100) versus the non-current pane (#101).
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

        // Current card span = `CARD_INDENT_W` (2) + max(title_w=4,
        // meta_w) where meta_w = `Tab 0` (5) + `, ` (2) +
        // `[CURRENT PANE]` (14) = 21. So the click span must be
        // 2 + 21 = 23 cells.
        let current_span =
            current.col_end.saturating_sub(current.col_start);
        assert_eq!(
            current_span, 23,
            "current pane card must allocate room for [CURRENT PANE] beside the bullet",
        );

        // Non-current card span = `CARD_INDENT_W` (2) +
        // max(title=4, meta=`Tab 1, —` = 8) = 2 + 8 = 10.
        let other_span = other.col_end.saturating_sub(other.col_start);
        assert_eq!(
            other_span, 10,
            "non-current pane card span = bullet slot + activity timestamp width",
        );

        // Cards share `col_start` — both anchor to the same card
        // column even though their content widths differ. The
        // narrower card paints its content flush-left within the
        // column.
        assert_eq!(
            current.col_start, other.col_start,
            "both pane cards must anchor at the card column's left edge",
        );
    }

    /// A search term that matches only one pane title narrows the
    /// list to that single pane card. Confirms the fuzzy filter
    /// actually drops misses (not just hides "+ New Pane" rows).
    #[test]
    fn panes_menu_fuzzy_filter_narrows_to_matching_pane() {
        let mut state = state_with_tabs_and_panes(2);
        // Distinct titles so the matcher can discriminate.
        state.workspace.panes_by_tab_position.get_mut(&0).unwrap()[0].title =
            "alpha".to_string();
        state.workspace.panes_by_tab_position.get_mut(&1).unwrap()[0].title =
            "bravo".to_string();
        state.panes.panes_search = "brv".to_string();
        state.panes.render(&state.workspace, &mut state.navigation, &mut state.frame, 0, 20, 60);

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

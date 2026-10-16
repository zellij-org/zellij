use std::collections::BTreeMap;
use std::ops::Range;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::app_panel::focus_session_pane;

use crate::panel::{fit_ansi, truncate_plain};

pub const INBOX_PREVIEW_REFRESH_SECONDS: f64 = 0.3;
pub const JUST_NOW_SECONDS: u64 = 10;
pub const PREFERRED_TEXT_WIDTH: usize = 24;
pub const WATCHING_TEXT: &str = "watching";
pub const AGE_CLOCK_MAX_MS: u64 = 60_000;
pub const AGE_CLOCK_SLACK_SECONDS: f64 = 0.05;
pub const CARD_ITEM_LIMIT: usize = 5;
pub const CARD_WIDTH: usize = 56;
pub const INBOX_SHORTCUT: &str = "<F9>";
pub const OPEN_INBOX_TEXT: &str = "Open inbox ›";
pub const DONT_SHOW_AGAIN_TEXT: &str = " Don't show again ";
pub const SESSION_MAX_WIDTH: usize = 16;
pub const PANE_MAX_WIDTH: usize = 20;
pub const MIN_PANE_WIDTH: usize = 6;
pub const MIN_TEXT_WIDTH: usize = 12;
pub const GROUPED_ROW_INDENT: usize = 2;
pub const LIST_INDENT: usize = 1;
pub const LIST_SCROLL_MARGIN: usize = 2;
pub const FILTER_TOGGLE_GAP: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BrowserTab {
    #[default]
    Inbox,
    Sessions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemKey {
    pub session: String,
    pub pane_id: PaneId,
}

impl ItemKey {
    pub fn of(item: &GlobalAttentionItem) -> Self {
        ItemKey {
            session: item.session.clone(),
            pane_id: item.item.pane_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    Item(ItemKey),
    Group(InboxGroup),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InboxRow {
    Item(usize),
    Group(InboxGroup, usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyTarget {
    pub key: ItemKey,
    pub status: AttentionStatus,
    pub place: String,
    pub session_label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardTarget {
    Item(ItemKey),
    OpenInbox,
    Close,
    DontShowAgain,
    BackTo(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserTarget {
    Tab(BrowserTab),
    Row(usize),
    Preview,
    HereOnly,
    CollapseAll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InboxGroup {
    Watched,
    NeedsYou,
    Failed,
    Done,
    Working,
    Activity,
    Muted,
}

pub const INBOX_GROUPS: [InboxGroup; 7] = [
    InboxGroup::Watched,
    InboxGroup::NeedsYou,
    InboxGroup::Failed,
    InboxGroup::Done,
    InboxGroup::Working,
    InboxGroup::Activity,
    InboxGroup::Muted,
];

impl InboxGroup {
    pub fn of(item: &GlobalAttentionItem) -> Option<InboxGroup> {
        if item.is_muted() {
            return Some(InboxGroup::Muted);
        }
        if is_watch_item(item) {
            return Some(InboxGroup::Watched);
        }
        if item.is_activity() {
            return Some(InboxGroup::Activity);
        }
        match item.item.attention.effective_status() {
            AttentionStatus::NeedsYou => Some(InboxGroup::NeedsYou),
            AttentionStatus::Failed => Some(InboxGroup::Failed),
            AttentionStatus::Done => Some(InboxGroup::Done),
            AttentionStatus::Working => Some(InboxGroup::Working),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            InboxGroup::Watched => "Watched",
            InboxGroup::NeedsYou => "Needs you",
            InboxGroup::Failed => "Failed",
            InboxGroup::Done => "Done",
            InboxGroup::Working => "Working",
            InboxGroup::Activity => "Activity",
            InboxGroup::Muted => "Muted",
        }
    }

    pub fn always_listed(&self) -> bool {
        *self != InboxGroup::Watched
    }

    pub fn expanded_by_default(&self) -> bool {
        !matches!(self, InboxGroup::Activity | InboxGroup::Muted)
    }

    fn style(&self, text: Text, range: Range<usize>) -> Text {
        match self {
            InboxGroup::Watched => text.color_range(1, range),
            InboxGroup::NeedsYou => style_status(text, AttentionStatus::NeedsYou, range),
            InboxGroup::Failed => style_status(text, AttentionStatus::Failed, range),
            InboxGroup::Done => style_status(text, AttentionStatus::Done, range),
            InboxGroup::Working => style_status(text, AttentionStatus::Working, range),
            InboxGroup::Activity | InboxGroup::Muted => text,
        }
    }
}

pub fn is_watch_item(item: &GlobalAttentionItem) -> bool {
    item.item.attention.watched || item.item.attention.from_watch
}

pub fn item_text(item: &GlobalAttentionItem) -> String {
    let text = item.text();
    if text.is_empty() && item.item.attention.watched {
        WATCHING_TEXT.to_owned()
    } else {
        text
    }
}

pub fn watch_symbol(simplified_ui: bool) -> &'static str {
    if simplified_ui {
        "@"
    } else {
        "◉"
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardContent {
    pub title: String,
    pub items: Vec<usize>,
    pub hidden_items: usize,
    pub activity: usize,
    pub nothing: Option<String>,
    pub back_to: Option<String>,
}

#[derive(Default)]
pub struct Inbox {
    pub global: Option<GlobalAttention>,
    pub history: Option<SessionHistoryInfo>,
    pub tab: BrowserTab,
    pub tab_chosen: bool,
    pub only_here: bool,
    pub expanded: BTreeMap<InboxGroup, bool>,
    pub selection: Option<Selection>,
    pub reply: Option<ReplyTarget>,
    pub preview: Option<SessionPreview>,
    pub preview_key: Option<ItemKey>,
    pub timer_armed: bool,
    pub clock_armed: bool,
    pub card_targets: Vec<(usize, Range<usize>, CardTarget)>,
    pub browser_targets: Vec<(usize, Range<usize>, BrowserTarget)>,
    pub card_hovered: Option<CardTarget>,
    pub away_since_ms: Option<u64>,
    pub list_scroll: usize,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn age_label(since_ms: u64, now_ms: u64, compact: bool) -> String {
    let secs = now_ms.saturating_sub(since_ms) / 1000;
    let ago = if compact { "" } else { " ago" };
    if since_ms == 0 {
        String::new()
    } else if secs < JUST_NOW_SECONDS {
        if compact { "now" } else { "just now" }.to_owned()
    } else if secs < 60 {
        format!("<1m{}", ago)
    } else if secs < 3600 {
        format!("{}m{}", secs / 60, ago)
    } else if secs < 86400 {
        format!("{}h{}", secs / 3600, ago)
    } else {
        format!("{}d{}", secs / 86400, ago)
    }
}

pub fn age_label_changes_in_ms(since_ms: u64, now_ms: u64) -> u64 {
    let age = now_ms.saturating_sub(since_ms);
    let step = |unit: u64| unit - age % unit;
    if age < JUST_NOW_SECONDS * 1000 {
        JUST_NOW_SECONDS * 1000 - age
    } else if age < 60_000 {
        60_000 - age
    } else if age < 3_600_000 {
        step(60_000)
    } else if age < 86_400_000 {
        step(3_600_000)
    } else {
        step(86_400_000)
    }
}

pub fn away_text(away_ms: u64) -> String {
    let secs = away_ms / 1000;
    if secs < 60 {
        "moments".to_owned()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d", secs / 86400)
    }
}

pub fn style_status(text: Text, status: AttentionStatus, range: Range<usize>) -> Text {
    match status {
        AttentionStatus::NeedsYou => text.color_range(0, range),
        AttentionStatus::Failed => text.error_color_range(range),
        AttentionStatus::Done => text.success_color_range(range),
        AttentionStatus::Working => text.color_range(1, range),
        _ => text.dim_range(range),
    }
}

fn pad_to(text: &str, width: usize) -> String {
    let text = truncate_plain(text, width);
    let padding = width.saturating_sub(text.width());
    format!("{}{}", text, " ".repeat(padding))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnWidths {
    pub session: usize,
    pub pane: usize,
    pub age: usize,
    pub compact_age: bool,
}

impl ColumnWidths {
    pub fn measure<'a>(
        items: impl Iterator<Item = (&'a GlobalAttentionItem, usize)>,
        now: u64,
        width: usize,
    ) -> Self {
        let mut widths = ColumnWidths {
            session: 0,
            pane: 0,
            age: 0,
            compact_age: false,
        };
        let mut compact_age = 0;
        for (item, indent) in items {
            widths.session = widths
                .session
                .max(item.session_label().width().min(SESSION_MAX_WIDTH) + indent);
            widths.pane = widths
                .pane
                .max(item.pane_label().width().min(PANE_MAX_WIDTH));
            let since_ms = item.item.attention.since_ms;
            widths.age = widths.age.max(age_label(since_ms, now, false).width());
            compact_age = compact_age.max(age_label(since_ms, now, true).width());
        }
        if width.saturating_sub(widths.fixed()) < PREFERRED_TEXT_WIDTH {
            widths.age = compact_age;
            widths.compact_age = true;
        }
        let text_width = width.saturating_sub(widths.fixed());
        if text_width < MIN_TEXT_WIDTH {
            let shortfall = MIN_TEXT_WIDTH - text_width;
            widths.pane = widths
                .pane
                .saturating_sub(shortfall)
                .max(widths.pane.min(MIN_PANE_WIDTH));
        }
        widths
    }

    fn fixed(&self) -> usize {
        2 + self.session + 1 + self.pane + 2 + 2 + self.age
    }

    pub fn text(&self, width: usize) -> usize {
        width.saturating_sub(self.fixed())
    }
}

pub struct ItemLine {
    pub line: String,
    pub symbol_len: usize,
    pub session_len: usize,
    pub pane_start: usize,
    pub pane_len: usize,
}

impl ItemLine {
    pub fn style(&self, item: &GlobalAttentionItem) -> Text {
        let text = Text::from(self.line.as_str());
        if item.is_muted() {
            return text.dim_all();
        }
        let session_start = self.symbol_len + 1;
        let status = item.item.attention.effective_status();
        let text = if status.is_idle() && item.item.attention.watched {
            text.color_range(1, 0..self.symbol_len)
        } else {
            style_status(text, status, 0..self.symbol_len)
        };
        text.color_range(1, session_start..session_start + self.session_len)
            .color_range(2, self.pane_start..self.pane_start + self.pane_len)
    }
}

fn pad_left_to(text: &str, width: usize) -> String {
    let text = truncate_plain(text, width);
    let padding = width.saturating_sub(text.width());
    format!("{}{}", " ".repeat(padding), text)
}

pub fn item_line(
    item: &GlobalAttentionItem,
    widths: &ColumnWidths,
    indent: usize,
    width: usize,
    now: u64,
    simplified_ui: bool,
) -> ItemLine {
    let status = item.item.attention.effective_status();
    let symbol = if item.is_muted() {
        if simplified_ui {
            "-"
        } else {
            "⊘"
        }
    } else if status.marker(simplified_ui).is_empty() {
        if item.item.attention.watched {
            watch_symbol(simplified_ui)
        } else {
            " "
        }
    } else {
        status.marker(simplified_ui)
    };
    let session_width = widths.session.saturating_sub(indent);
    let session = pad_to(&item.session_label(), session_width);
    let pane = pad_to(&item.pane_label(), widths.pane);
    let text = pad_to(&item_text(item), widths.text(width));
    let age = pad_left_to(
        &age_label(item.item.attention.since_ms, now, widths.compact_age),
        widths.age,
    );
    let line = format!("{} {} {}  {}  {}", symbol, session, pane, text, age);
    let symbol_len = symbol.chars().count();
    ItemLine {
        line,
        symbol_len,
        session_len: truncate_plain(&item.session_label(), session_width)
            .chars()
            .count(),
        pane_start: symbol_len + 1 + session.chars().count() + 1,
        pane_len: truncate_plain(&item.pane_label(), widths.pane)
            .chars()
            .count(),
    }
}

impl Inbox {
    pub fn update_global(&mut self, global: GlobalAttention) -> bool {
        if self.global.as_ref() == Some(&global) {
            return false;
        }
        let previous_rows = self.visible_rows();
        let previous_index = self.selected_row_index(&previous_rows);
        self.global = Some(global);
        if let Some(reply) = self.reply.clone() {
            let still_same = self
                .find(&reply.key)
                .map(|item| item.item.attention.effective_status() == reply.status)
                .unwrap_or(false);
            if !still_same {
                self.reply = None;
            }
        }
        if let Some(Selection::Item(key)) = self.selection.clone() {
            if self.find(&key).is_none() {
                let rows = self.visible_rows();
                let next = previous_index.and_then(|index| {
                    let index = index.min(rows.len().saturating_sub(1));
                    let is_item = |row: &&InboxRow| matches!(row, InboxRow::Item(_));
                    rows[index..]
                        .iter()
                        .find(is_item)
                        .or_else(|| rows[..index].iter().rev().find(is_item))
                        .or_else(|| rows.get(index))
                        .cloned()
                });
                self.selection = next.map(|row| self.selection_for(&row));
            }
        }
        if !self.tab_chosen {
            self.tab = if self
                .global
                .as_ref()
                .map(|g| g.items.is_empty())
                .unwrap_or(true)
            {
                BrowserTab::Sessions
            } else {
                BrowserTab::Inbox
            };
        }
        true
    }

    pub fn follow_selection(&mut self, selected: Option<usize>, list_rows: usize, total: usize) {
        self.list_scroll = match selected {
            Some(selected) => crate::panel::scroll_with_margin(
                self.list_scroll,
                selected,
                list_rows,
                total,
                LIST_SCROLL_MARGIN,
            ),
            None => self.list_scroll.min(total.saturating_sub(list_rows.max(1))),
        };
    }

    pub fn choose_inbox_tab(&mut self) {
        self.tab = BrowserTab::Inbox;
        self.tab_chosen = true;
    }

    pub fn update_history(&mut self, history: SessionHistoryInfo) -> bool {
        if self.history.as_ref() == Some(&history) {
            return false;
        }
        self.history = Some(history);
        true
    }

    pub fn previous_session(&self) -> Option<String> {
        self.history.as_ref().and_then(|h| h.previous.clone())
    }

    pub fn find(&self, key: &ItemKey) -> Option<&GlobalAttentionItem> {
        self.global
            .as_ref()
            .and_then(|g| g.find(&key.session, &key.pane_id))
    }

    fn items(&self) -> &[GlobalAttentionItem] {
        self.global
            .as_ref()
            .map(|g| g.items.as_slice())
            .unwrap_or(&[])
    }

    pub fn matches_filter(&self, item: &GlobalAttentionItem) -> bool {
        !self.only_here || item.is_current
    }

    pub fn flip_here_only(&mut self) {
        self.only_here = !self.only_here;
        self.list_scroll = 0;
    }

    pub fn is_expanded(&self, group: InboxGroup) -> bool {
        self.expanded
            .get(&group)
            .copied()
            .unwrap_or_else(|| group.expanded_by_default())
    }

    fn set_expanded(&mut self, group: InboxGroup, expanded: bool) {
        self.expanded.insert(group, expanded);
    }

    pub fn all_collapsed(&self) -> bool {
        INBOX_GROUPS.iter().all(|group| !self.is_expanded(*group))
    }

    pub fn flip_collapse_all(&mut self) {
        if self.all_collapsed() {
            self.expanded.clear();
        } else {
            for group in INBOX_GROUPS {
                self.expanded.insert(group, false);
            }
        }
        self.list_scroll = 0;
    }

    fn render_toggle(
        &mut self,
        hint: &str,
        label: &str,
        on: bool,
        target: BrowserTarget,
        x: usize,
        y: usize,
        right_edge: usize,
    ) -> Option<usize> {
        let mut widget = Toggle::new(Text::from(label).color_range(1, ..), on);
        let column = x + hint.width() + 1;
        let end = column + widget.width();
        if end > right_edge {
            return None;
        }
        print_text_with_coordinates(
            Text::from(hint).color_range(3, ..),
            x,
            y,
            Some(hint.width()),
            None,
        );
        widget.render(column, y);
        self.browser_targets.push((y, x..end, target));
        Some(end)
    }

    fn render_filters(&mut self, x: usize, y: usize, width: usize) {
        let right_edge = x + width;
        let only_here = self.only_here;
        let Some(end) = self.render_toggle(
            "<f>",
            "here only",
            only_here,
            BrowserTarget::HereOnly,
            x,
            y,
            right_edge,
        ) else {
            return;
        };
        let all_collapsed = self.all_collapsed();
        self.render_toggle(
            "<a>",
            "collapse all",
            all_collapsed,
            BrowserTarget::CollapseAll,
            end + FILTER_TOGGLE_GAP,
            y,
            right_edge,
        );
    }

    fn render_selected_text(&self, x: usize, y: usize, width: usize) {
        let Some(item) = self.selected_item() else {
            return;
        };
        let text = truncate_plain(&item_text(item), width);
        print_text_with_coordinates(Text::from(text.as_str()), x, y, Some(width), None);
    }

    pub fn visible_rows(&self) -> Vec<InboxRow> {
        let mut grouped: BTreeMap<InboxGroup, Vec<usize>> = BTreeMap::new();
        for (index, item) in self.items().iter().enumerate() {
            if !self.matches_filter(item) {
                continue;
            }
            if let Some(group) = InboxGroup::of(item) {
                grouped.entry(group).or_default().push(index);
            }
        }
        let mut rows = vec![];
        for group in INBOX_GROUPS {
            let items = grouped.remove(&group).unwrap_or_default();
            if items.is_empty() && !group.always_listed() {
                continue;
            }
            rows.push(InboxRow::Group(group, items.len()));
            if self.is_expanded(group) {
                rows.extend(items.into_iter().map(InboxRow::Item));
            }
        }
        rows
    }

    fn selection_for(&self, row: &InboxRow) -> Selection {
        match row {
            InboxRow::Item(index) => Selection::Item(ItemKey::of(&self.items()[*index])),
            InboxRow::Group(group, _) => Selection::Group(*group),
        }
    }

    pub fn selected_row_index(&self, rows: &[InboxRow]) -> Option<usize> {
        if rows.is_empty() {
            return None;
        }
        let position = match self.selection.as_ref() {
            Some(Selection::Item(key)) => rows.iter().position(|row| match row {
                InboxRow::Item(index) => ItemKey::of(&self.items()[*index]) == *key,
                _ => false,
            }),
            Some(Selection::Group(group)) => rows
                .iter()
                .position(|row| matches!(row, InboxRow::Group(g, _) if g == group)),
            None => None,
        };
        Some(position.unwrap_or_else(|| {
            rows.iter()
                .position(|row| matches!(row, InboxRow::Item(_)))
                .unwrap_or(0)
        }))
    }

    pub fn select_row(&mut self, index: usize) {
        let rows = self.visible_rows();
        if let Some(row) = rows.get(index) {
            self.selection = Some(self.selection_for(row));
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        let rows = self.visible_rows();
        if rows.is_empty() {
            self.selection = None;
            return;
        }
        let current = self.selected_row_index(&rows).unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, rows.len() as isize - 1) as usize;
        self.selection = Some(self.selection_for(&rows[next]));
    }

    pub fn selected_row(&self) -> Option<InboxRow> {
        let rows = self.visible_rows();
        self.selected_row_index(&rows)
            .and_then(|index| rows.get(index).cloned())
    }

    pub fn selected_item(&self) -> Option<&GlobalAttentionItem> {
        match self.selected_row()? {
            InboxRow::Item(index) => self.items().get(index),
            _ => None,
        }
    }

    pub fn select_item(&mut self, key: ItemKey) {
        if let Some(group) = self.find(&key).and_then(InboxGroup::of) {
            self.set_expanded(group, true);
        }
        self.selection = Some(Selection::Item(key));
    }

    pub fn select_next_counted(&mut self) -> bool {
        let Some(global) = self.global.as_ref() else {
            return false;
        };
        let current = match self.selection {
            Some(Selection::Item(_)) => self.selected_item().map(ItemKey::of),
            _ => None,
        };
        let next = match current {
            Some(key) => global.next_counted_after(&key.session, Some(&key.pane_id)),
            None => global.next_counted_after("", None),
        }
        .map(ItemKey::of);
        match next {
            Some(key) => {
                self.select_item(key);
                true
            },
            None => false,
        }
    }

    fn group_of_row(&self, row: &InboxRow) -> Option<InboxGroup> {
        match row {
            InboxRow::Group(group, _) => Some(*group),
            InboxRow::Item(index) => self.items().get(*index).and_then(InboxGroup::of),
        }
    }

    pub fn expand_selected_group(&mut self) -> bool {
        match self.selected_row() {
            Some(InboxRow::Group(group, _)) if !self.is_expanded(group) => {
                self.set_expanded(group, true);
                true
            },
            _ => false,
        }
    }

    pub fn collapse_selected_group(&mut self) -> bool {
        let Some(group) = self.selected_row().and_then(|row| self.group_of_row(&row)) else {
            return false;
        };
        if !self.is_expanded(group) {
            return false;
        }
        self.set_expanded(group, false);
        self.selection = Some(Selection::Group(group));
        true
    }

    pub fn toggle_selected_watch(&self) {
        if let Some(item) = self.selected_item() {
            toggle_session_pane_watch(&item.session, item.item.pane_id);
        }
    }

    pub fn toggle_header(&mut self) -> bool {
        match self.selected_row() {
            Some(InboxRow::Group(group, _)) => {
                let expanded = self.is_expanded(group);
                self.set_expanded(group, !expanded);
                true
            },
            _ => false,
        }
    }

    pub fn counted_count(&self) -> usize {
        self.global
            .as_ref()
            .map(|g| g.counted_items().len())
            .unwrap_or(0)
    }

    pub fn running_count(&self) -> usize {
        self.global.as_ref().map(|g| g.running()).unwrap_or(0)
    }

    fn changed_while_away(&self, item: &GlobalAttentionItem) -> bool {
        match self.away_since_ms {
            Some(since) => {
                item.item.attention.since_ms >= since || item.item.attention.changed_ms >= since
            },
            None => true,
        }
    }

    pub fn card_content(&self, now: u64) -> CardContent {
        let counted: Vec<usize> = self
            .items()
            .iter()
            .enumerate()
            .filter(|(_, item)| item.is_counted() && self.changed_while_away(item))
            .map(|(index, _)| index)
            .collect();
        let activity = self
            .items()
            .iter()
            .filter(|i| i.is_activity() && self.changed_while_away(i))
            .count();
        let title = match self.away_since_ms {
            Some(since) => format!(
                "While you were away · {}",
                away_text(now.saturating_sub(since))
            ),
            None => format!("Inbox ({})", self.counted_count()),
        };
        let (nothing, back_to) = if counted.is_empty() {
            (
                Some(format!(
                    "Nothing needs you · {} running",
                    self.running_count()
                )),
                self.previous_session(),
            )
        } else {
            (None, None)
        };
        let hidden_items = counted.len().saturating_sub(CARD_ITEM_LIMIT);
        CardContent {
            title,
            items: counted.into_iter().take(CARD_ITEM_LIMIT).collect(),
            hidden_items,
            activity,
            nothing,
            back_to,
        }
    }

    pub fn card_dimensions(&self, screen_rows: usize) -> (usize, usize) {
        let content = self.card_content(now_ms());
        let mut rows = content.items.len();
        if content.nothing.is_some() {
            rows += 1;
        }
        if content.back_to.is_some() {
            rows += 1;
        }
        rows += 1;
        let mut height = rows + 2;
        if screen_rows > 0 {
            height = height.min(screen_rows.saturating_sub(1).max(5));
        }
        (CARD_WIDTH, height)
    }

    pub fn arm_timer(&mut self) {
        if !self.timer_armed {
            crate::timers::start(
                crate::timers::TimerOwner::InboxPreview,
                INBOX_PREVIEW_REFRESH_SECONDS,
            );
            self.timer_armed = true;
        }
    }

    pub fn seconds_until_an_age_changes(&self, now: u64) -> Option<f64> {
        self.items()
            .iter()
            .filter(|item| item.item.attention.since_ms > 0)
            .map(|item| age_label_changes_in_ms(item.item.attention.since_ms, now))
            .min()
            .map(|ms| (ms.min(AGE_CLOCK_MAX_MS) as f64 / 1000.0) + AGE_CLOCK_SLACK_SECONDS)
    }

    pub fn arm_clock(&mut self, now: u64) {
        if self.clock_armed {
            return;
        }
        if let Some(seconds) = self.seconds_until_an_age_changes(now) {
            crate::timers::start(crate::timers::TimerOwner::Clock, seconds);
            self.clock_armed = true;
        }
    }

    pub fn request_preview(&mut self) {
        let Some(item) = self.selected_item().cloned() else {
            self.preview = None;
            self.preview_key = None;
            return;
        };
        let key = ItemKey::of(&item);
        if self.preview_key.as_ref() != Some(&key) {
            self.preview = None;
        }
        self.preview_key = Some(key);
        let anchor = PreviewAnchor::for_status(item.item.attention.effective_status());
        get_session_pane_preview(&item.session, item.item.pane_id, anchor);
    }

    pub fn accepts_preview(&self, preview: &SessionPreview) -> bool {
        let Some(key) = self.preview_key.as_ref() else {
            return false;
        };
        let pane = match key.pane_id {
            PaneId::Terminal(id) => (id, false),
            PaneId::Plugin(id) => (id, true),
        };
        preview.session_name == key.session && preview.pane_id == Some(pane)
    }

    pub fn start_reply(&mut self) -> bool {
        let Some(item) = self.selected_item().cloned() else {
            return false;
        };
        self.reply = Some(ReplyTarget {
            key: ItemKey::of(&item),
            status: item.item.attention.effective_status(),
            place: item.place(),
            session_label: item.session_label(),
        });
        true
    }

    pub fn send_reply_key(&self, key: KeyWithModifier) {
        if let Some(reply) = self.reply.as_ref() {
            send_key_to_session_pane(&reply.key.session, reply.key.pane_id, key);
        }
    }

    pub fn go_to_selected(&self) -> bool {
        match self.selected_item() {
            Some(item) => {
                focus_session_pane(&item.session, item.item.pane_id);
                true
            },
            None => false,
        }
    }

    pub fn mark_selected_seen(&self) {
        if let Some(item) = self.selected_item() {
            mark_session_pane_seen(&item.session, item.item.pane_id);
        }
    }

    pub fn cycle_selected_mute(&self) {
        let Some(item) = self.selected_item() else {
            return;
        };
        let attention = &item.item.attention;
        let (session, pane_id) = (item.session.as_str(), item.item.pane_id);
        if attention.is_muted() {
            toggle_session_pane_mute(session, pane_id);
            if attention.ignore_output {
                toggle_session_pane_ignore_output(session, pane_id);
            }
        } else if !attention.ignore_output && attention.status == AttentionStatus::NewOutput {
            toggle_session_pane_ignore_output(session, pane_id);
        } else {
            toggle_session_pane_mute(session, pane_id);
        }
    }

    pub fn browser_target_at(&self, line: isize, column: usize) -> Option<BrowserTarget> {
        if line < 0 {
            return None;
        }
        let line = line as usize;
        self.browser_targets
            .iter()
            .find(|(y, range, _)| *y == line && range.contains(&column))
            .map(|(_, _, target)| target.clone())
    }

    pub fn card_target_at(&self, line: isize, column: usize) -> Option<CardTarget> {
        if line < 0 {
            return None;
        }
        let line = line as usize;
        self.card_targets
            .iter()
            .find(|(y, range, _)| *y == line && range.contains(&column))
            .map(|(_, _, target)| target.clone())
    }

    pub fn tab_titles(&self, sessions: usize) -> (String, String) {
        (
            format!("Inbox ({})", self.counted_count()),
            format!("Sessions ({})", sessions),
        )
    }

    pub fn render_tabs(&mut self, sessions: usize, x: usize, y: usize) {
        let (inbox, sessions) = self.tab_titles(sessions);
        let line = format!(" {} │ {}", inbox, sessions);
        let inbox_range = 1..1 + inbox.chars().count();
        let sessions_start = inbox_range.end + 3;
        let sessions_range = sessions_start..sessions_start + sessions.chars().count();
        let mut text = Text::from(line.as_str());
        match self.tab {
            BrowserTab::Inbox => {
                text = text.color_range(2, inbox_range.clone());
                text = text.dim_range(sessions_range.clone());
            },
            BrowserTab::Sessions => {
                text = text.color_range(2, sessions_range.clone());
                text = text.dim_range(inbox_range.clone());
            },
        }
        print_text_with_coordinates(text, x, y, None, None);
        self.browser_targets
            .retain(|(_, _, t)| !matches!(t, BrowserTarget::Tab(_)));
        let inbox_width = inbox.width();
        self.browser_targets.push((
            y,
            x + 1..x + 1 + inbox_width,
            BrowserTarget::Tab(BrowserTab::Inbox),
        ));
        let sessions_x = x + 1 + inbox_width + 3;
        self.browser_targets.push((
            y,
            sessions_x..sessions_x + sessions.width(),
            BrowserTarget::Tab(BrowserTab::Sessions),
        ));
    }

    pub fn render_browser(
        &mut self,
        x: usize,
        y: usize,
        cols: usize,
        rows: usize,
        simplified_ui: bool,
    ) {
        self.browser_targets
            .retain(|(_, _, t)| matches!(t, BrowserTarget::Tab(_)));
        let now = now_ms();
        let list_y = y + 2;
        let help_y = (y + rows).saturating_sub(1);
        let list_rows = help_y.saturating_sub(list_y + 1);
        let show_preview = cols >= 60;
        let list_cols = if show_preview {
            (cols / 2).max(30)
        } else {
            cols
        };
        self.render_filters(
            x + LIST_INDENT,
            y,
            list_cols.saturating_sub(LIST_INDENT + 1),
        );
        let rows_to_show = self.visible_rows();
        let selected = self.selected_row_index(&rows_to_show);
        self.follow_selection(selected, list_rows, rows_to_show.len());
        let line_width = list_cols.saturating_sub(3);
        let widths = ColumnWidths::measure(
            self.items().iter().map(|item| (item, GROUPED_ROW_INDENT)),
            now,
            line_width,
        );
        for (offset, row) in rows_to_show
            .iter()
            .enumerate()
            .skip(self.list_scroll)
            .take(list_rows)
        {
            let row_y = list_y + offset - self.list_scroll;
            let is_selected = Some(offset) == selected;
            let indent = match row {
                InboxRow::Item(_) => GROUPED_ROW_INDENT,
                InboxRow::Group(..) => 0,
            };
            let text = match row {
                InboxRow::Item(index) => {
                    let item = &self.items()[*index];
                    item_line(item, &widths, indent, line_width, now, simplified_ui).style(item)
                },
                InboxRow::Group(group, count) => self.group_header(*group, *count),
            };
            let text = if is_selected { text.selected() } else { text };
            print_text_with_coordinates(
                text,
                x + LIST_INDENT + indent,
                row_y,
                Some(line_width.saturating_sub(indent)),
                None,
            );
            self.browser_targets
                .push((row_y, x..x + list_cols, BrowserTarget::Row(offset)));
        }
        if show_preview {
            let preview_x = x + list_cols + 1;
            let preview_width = cols.saturating_sub(list_cols + 1);
            self.render_selected_text(preview_x + 1, y, preview_width.saturating_sub(2));
            let preview_y = y + 1;
            let preview_height = help_y.saturating_sub(preview_y);
            self.render_preview(preview_x, preview_y, preview_width, preview_height);
        }
        self.render_help(x, help_y, cols);
    }

    pub fn group_header(&self, group: InboxGroup, count: usize) -> Text {
        let arrow = if self.is_expanded(group) {
            "▾"
        } else {
            "▸"
        };
        let line = format!("{} {} ({})", arrow, group.label(), count);
        let label_start = arrow.chars().count() + 1;
        let label_end = label_start + group.label().chars().count();
        match group {
            InboxGroup::Activity | InboxGroup::Muted => Text::from(line.as_str()).dim_all(),
            _ => group.style(Text::from(line.as_str()), label_start..label_end),
        }
    }

    fn render_preview(&mut self, x: usize, y: usize, width: usize, height: usize) {
        if width < 10 || height < 4 {
            return;
        }
        self.browser_targets
            .push((y, x..x + width, BrowserTarget::Preview));
        let item = self.selected_item().cloned();
        let inner = width.saturating_sub(2);
        let title = " Preview ";
        let top = format!(
            "┌{}{}┐",
            title,
            "─".repeat(inner.saturating_sub(title.width()))
        );
        let replying = self.reply.is_some();
        let top_text = if replying {
            Text::from(top.as_str()).color_range(3, ..)
        } else {
            Text::from(top.as_str()).color_range(2, 1..1 + title.chars().count())
        };
        print_text_with_coordinates(top_text, x, y, Some(width), None);
        let border = |text: &str| {
            if replying {
                Text::from(text).color_range(3, ..)
            } else {
                Text::from(text)
            }
        };
        let body_y = y + 1;
        let bottom_y = y + height - 1;
        let body_rows = bottom_y.saturating_sub(body_y);
        let lines: Vec<String> = match (item.as_ref(), self.preview.as_ref()) {
            (Some(_), Some(preview)) if self.preview_key.is_some() => match &preview.error {
                Some(error) => vec![format!("Preview unavailable: {}", error)],
                None => preview.contents.lines().map(|l| l.to_owned()).collect(),
            },
            (Some(_), _) => vec!["Loading…".to_owned()],
            (None, _) => vec!["Select an item to see its preview".to_owned()],
        };
        let start = match self.preview.as_ref().and_then(|p| p.anchor_line) {
            Some(anchor) => anchor.min(lines.len().saturating_sub(body_rows)),
            None => lines.len().saturating_sub(body_rows),
        };
        for row in 0..body_rows {
            let line_y = body_y + row;
            print_text_with_coordinates(border("│"), x, line_y, Some(1), None);
            let line = lines.get(start + row).map(|l| l.as_str()).unwrap_or("");
            print!("\u{1b}[{};{}H{}", line_y + 1, x + 2, fit_ansi(line, inner));
            print_text_with_coordinates(border("│"), x + width - 1, line_y, Some(1), None);
        }
        let bottom = format!("└{}┘", "─".repeat(inner));
        print_text_with_coordinates(border(&bottom), x, bottom_y, Some(width), None);
    }

    fn render_help(&self, x: usize, y: usize, cols: usize) {
        let text = match self.reply.as_ref() {
            Some(reply) => {
                let prefix = "REPLYING TO ";
                let line = format!(
                    "{}{} · <ESC> back to list · <ENTER in list> open pane",
                    prefix,
                    sanitize_display_text(&reply.place)
                );
                let session_start = prefix.chars().count();
                let session_end = session_start + reply.session_label.chars().count();
                key_hint_text(&line)
                    .color_range(0, 0.."REPLYING TO".chars().count())
                    .color_range(0, session_start..session_end)
            },
            None => {
                let full = "<↓↑> select  <←→> open/close  <ENTER> go  <r> reply  <c> seen  <C> all seen  <m> mute  <w> watch  <f> here only  <a> collapse all  <TAB> sessions  <ESC> close";
                let short = "<↓↑> <←→> <ENTER> go  <r> reply  <c> seen  <C> all seen  <m> mute  <w> watch  <ESC>";
                let line = if full.width() <= cols { full } else { short };
                key_hint_text(line)
            },
        };
        print_text_with_coordinates(text, x + 1, y, Some(cols.saturating_sub(1)), None);
    }

    pub fn render_card(
        &mut self,
        cols: usize,
        rows: usize,
        popup_focused: bool,
        opened_at_startup: bool,
        simplified_ui: bool,
    ) {
        let now = now_ms();
        let content = self.card_content(now);
        self.card_targets.clear();
        let width = cols;
        let inner = width.saturating_sub(2);
        let content_x = 2;
        let content_width = width.saturating_sub(4);
        let title = format!(" {} {} ", content.title, INBOX_SHORTCUT);
        let title = truncate_plain(&title, inner.saturating_sub(4));
        let top = format!(
            "┌{}{} × ─┐",
            title,
            "─".repeat(inner.saturating_sub(title.width() + 4))
        );
        let title_main_len = content.title.chars().count();
        let close_index = top.chars().count().saturating_sub(4);
        let top_text = Text::from(top.as_str())
            .color_range(2, 2..2 + title_main_len)
            .color_range(3, close_index..close_index + 1);
        print_text_with_coordinates(top_text, 0, 0, Some(width), None);
        let close_x = width.saturating_sub(4);
        self.card_targets
            .push((0, close_x.saturating_sub(1)..close_x + 2, CardTarget::Close));
        let mut y = 1;
        let last_y = rows.saturating_sub(2);
        let widths = ColumnWidths::measure(
            content.items.iter().map(|index| (&self.items()[*index], 0)),
            now,
            content_width,
        );
        for index in content.items.iter() {
            if y > last_y {
                break;
            }
            let item = self.items()[*index].clone();
            let key = ItemKey::of(&item);
            let mut text =
                item_line(&item, &widths, 0, content_width, now, simplified_ui).style(&item);
            if self.card_hovered == Some(CardTarget::Item(key.clone())) {
                text = text.selected();
            }
            print_text_with_coordinates(text, content_x, y, Some(content_width), None);
            self.card_targets
                .push((y, 1..width.saturating_sub(1), CardTarget::Item(key)));
            y += 1;
        }
        if let Some(nothing) = content.nothing.as_ref() {
            if y <= last_y {
                print_text_with_coordinates(
                    Text::from(nothing.as_str()).dim_all(),
                    content_x,
                    y,
                    Some(content_width),
                    None,
                );
                y += 1;
            }
        }
        if let Some(previous) = content.back_to.as_ref() {
            if y <= last_y {
                let line = format!("Back to {} ›", sanitize_display_text(previous));
                let mut text = Text::from(line.as_str()).color_range(3, ..);
                if self.card_hovered == Some(CardTarget::BackTo(previous.clone())) {
                    text = text.italic_all();
                }
                print_text_with_coordinates(text, content_x, y, Some(content_width), None);
                self.card_targets.push((
                    y,
                    content_x..content_x + line.width(),
                    CardTarget::BackTo(previous.clone()),
                ));
                y += 1;
            }
        }
        let footer_y = y.min(last_y);
        let mut extra = vec![];
        if content.hidden_items > 0 {
            extra.push(format!("+ {} more", content.hidden_items));
        }
        if content.activity > 0 {
            extra.push(format!("+ {} activity", content.activity));
        }
        if !extra.is_empty() {
            print_text_with_coordinates(
                Text::from(extra.join("  ").as_str()).dim_all(),
                content_x,
                footer_y,
                Some(content_width),
                None,
            );
        }
        let open = if popup_focused {
            format!("<ENTER> {}", OPEN_INBOX_TEXT)
        } else {
            OPEN_INBOX_TEXT.to_owned()
        };
        let open_x = (content_x + content_width).saturating_sub(open.width());
        let mut open_text = Text::from(open.as_str()).color_range(3, ..);
        if self.card_hovered == Some(CardTarget::OpenInbox) {
            open_text = open_text.italic_all();
        }
        print_text_with_coordinates(open_text, open_x, footer_y, Some(open.width()), None);
        self.card_targets.push((
            footer_y,
            open_x..open_x + open.width(),
            CardTarget::OpenInbox,
        ));
        let bottom_y = footer_y + 1;
        for side_y in 1..bottom_y {
            print_text_with_coordinates(Text::from("│"), 0, side_y, Some(1), None);
            print_text_with_coordinates(
                Text::from("│"),
                width.saturating_sub(1),
                side_y,
                Some(1),
                None,
            );
        }
        let bottom = format!("└{}┘", "─".repeat(inner));
        print_text_with_coordinates(Text::from(bottom.as_str()), 0, bottom_y, Some(width), None);
        if opened_at_startup && inner > DONT_SHOW_AGAIN_TEXT.width() + 2 {
            let text_width = DONT_SHOW_AGAIN_TEXT.width();
            let x = width.saturating_sub(text_width + 2);
            let mut text = Text::from(DONT_SHOW_AGAIN_TEXT).dim_all();
            if self.card_hovered == Some(CardTarget::DontShowAgain) {
                text = text.italic_all();
            }
            print_text_with_coordinates(text, x, bottom_y, Some(text_width), None);
            self.card_targets.push((
                bottom_y,
                x + 1..x + text_width - 1,
                CardTarget::DontShowAgain,
            ));
        }
    }
}

pub fn key_hint_text(line: &str) -> Text {
    let mut text = Text::from(line);
    let mut start = None;
    for (index, c) in line.chars().enumerate() {
        match c {
            '<' => start = Some(index),
            '>' => {
                if let Some(s) = start.take() {
                    text = text.color_range(3, s..index + 1);
                }
            },
            _ => {},
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(session: &str, id: u32, status: AttentionStatus, since_ms: u64) -> GlobalAttentionItem {
        GlobalAttentionItem {
            session: session.to_owned(),
            is_current: session == "here",
            item: AttentionItem {
                pane_id: PaneId::Terminal(id),
                tab_id: 0,
                tab_name: "tab".to_owned(),
                pane_title: format!("pane{}", id),
                attention: PaneAttention {
                    status,
                    text: format!("text {}", id),
                    since_ms,
                    ..Default::default()
                },
            },
        }
    }

    fn global(items: Vec<GlobalAttentionItem>) -> GlobalAttention {
        let mut sessions: Vec<(String, SessionAttention)> =
            vec![("here".to_owned(), Default::default())];
        for item in items {
            match sessions.iter_mut().find(|(name, _)| *name == item.session) {
                Some((_, attention)) => {
                    attention.counts.add(&item.item.attention);
                    attention.items.push(item.item);
                },
                None => {
                    let mut attention = SessionAttention::default();
                    attention.counts.add(&item.item.attention);
                    attention.items.push(item.item);
                    sessions.push((item.session, attention));
                },
            }
        }
        GlobalAttention::build("here", sessions)
    }

    fn sample() -> Inbox {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 100),
            item("here", 2, AttentionStatus::Failed, 90),
            item("build", 3, AttentionStatus::NewOutput, 80),
            item("build", 4, AttentionStatus::Quiet, 70),
        ]));
        inbox
    }

    fn filled(inbox: &Inbox) -> Vec<InboxRow> {
        inbox
            .visible_rows()
            .into_iter()
            .filter(|row| !matches!(row, InboxRow::Group(_, 0)))
            .collect()
    }

    fn select_first_item(inbox: &mut Inbox) {
        let index = inbox
            .visible_rows()
            .iter()
            .position(|row| *row == InboxRow::Item(0))
            .unwrap();
        inbox.select_row(index);
    }

    #[test]
    fn every_section_but_watched_is_always_listed() {
        let inbox = Inbox::default();
        let groups: Vec<InboxGroup> = inbox
            .visible_rows()
            .into_iter()
            .map(|row| match row {
                InboxRow::Group(group, 0) => group,
                other => panic!("unexpected row {:?}", other),
            })
            .collect();
        let expected: Vec<InboxGroup> = INBOX_GROUPS
            .into_iter()
            .filter(|group| *group != InboxGroup::Watched)
            .collect();
        assert_eq!(groups, expected);
    }

    #[test]
    fn watched_panes_and_finished_watches_are_listed_under_watched() {
        let mut watching = item("api", 5, AttentionStatus::Idle, 0);
        watching.item.attention.watched = true;
        watching.item.attention.text = String::new();
        let mut finished = item("here", 6, AttentionStatus::NeedsYou, 50);
        finished.item.attention.from_watch = true;
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            watching,
            finished,
            item("api", 1, AttentionStatus::NeedsYou, 100),
        ]));
        let rows = filled(&inbox);
        assert_eq!(rows[0], InboxRow::Group(InboxGroup::Watched, 2));
        assert_eq!(rows[3], InboxRow::Group(InboxGroup::NeedsYou, 1));
        let watched_index = inbox
            .items()
            .iter()
            .position(|i| i.item.pane_id == PaneId::Terminal(5))
            .unwrap();
        let widths = ColumnWidths::measure(inbox.items().iter().map(|i| (i, 0)), 200, 60);
        let line = item_line(&inbox.items()[watched_index], &widths, 0, 60, 200, false).line;
        assert!(line.starts_with("◉ api"), "{}", line);
        assert!(line.contains(WATCHING_TEXT), "{}", line);
    }

    #[test]
    fn collapse_all_closes_every_section_and_opens_them_again() {
        let mut inbox = sample();
        assert!(!inbox.all_collapsed());
        inbox.flip_collapse_all();
        assert!(inbox.all_collapsed());
        assert!(inbox
            .visible_rows()
            .iter()
            .all(|row| matches!(row, InboxRow::Group(..))));
        inbox.flip_collapse_all();
        assert!(!inbox.all_collapsed());
        assert!(inbox.is_expanded(InboxGroup::NeedsYou));
        assert!(!inbox.is_expanded(InboxGroup::Activity));
    }

    #[test]
    fn each_status_has_an_open_group_and_activity_starts_closed() {
        let mut inbox = sample();
        assert_eq!(
            filled(&inbox),
            vec![
                InboxRow::Group(InboxGroup::NeedsYou, 1),
                InboxRow::Item(0),
                InboxRow::Group(InboxGroup::Failed, 1),
                InboxRow::Item(1),
                InboxRow::Group(InboxGroup::Activity, 2),
            ]
        );
        select_activity(&mut inbox);
        assert!(inbox.toggle_header());
        assert_eq!(inbox.visible_rows().len(), 10);
        assert_eq!(inbox.tab, BrowserTab::Inbox);
    }

    #[test]
    fn without_a_selection_the_first_item_is_selected_not_its_group_line() {
        let inbox = sample();
        assert_eq!(inbox.selected_row(), Some(InboxRow::Item(0)));
    }

    #[test]
    fn here_only_shows_just_this_session() {
        let mut inbox = sample();
        inbox.flip_here_only();
        assert!(inbox.only_here);
        assert_eq!(
            filled(&inbox),
            vec![InboxRow::Group(InboxGroup::Failed, 1), InboxRow::Item(1)]
        );
        inbox.flip_here_only();
        assert_eq!(inbox.visible_rows().len(), 8);
    }

    #[test]
    fn status_groups_collapse_like_activity() {
        let mut inbox = sample();
        inbox.select_row(0);
        assert!(inbox.collapse_selected_group());
        assert_eq!(
            filled(&inbox),
            vec![
                InboxRow::Group(InboxGroup::NeedsYou, 1),
                InboxRow::Group(InboxGroup::Failed, 1),
                InboxRow::Item(1),
                InboxRow::Group(InboxGroup::Activity, 2),
            ]
        );
        assert!(inbox.expand_selected_group());
        assert_eq!(inbox.visible_rows().len(), 8);
        inbox.select_item(ItemKey {
            session: "build".to_owned(),
            pane_id: PaneId::Terminal(3),
        });
        assert!(inbox.is_expanded(InboxGroup::Activity));
    }

    #[test]
    fn narrow_lists_drop_ago_to_keep_room_for_the_text() {
        let inbox = sample();
        let now = 100 + 5 * 60_000;
        let items = || inbox.items().iter().map(|item| (item, 0));
        let wide = ColumnWidths::measure(items(), now, 120);
        assert!(!wide.compact_age);
        let narrow = ColumnWidths::measure(items(), now, 40);
        assert!(narrow.compact_age);
        let with_ago = ColumnWidths {
            compact_age: false,
            age: wide.age,
            ..narrow
        };
        assert!(narrow.text(40) > with_ago.text(40));
        let line = item_line(&inbox.items()[0], &narrow, 0, 40, now, false).line;
        assert!(line.ends_with("5m"), "{}", line);
    }

    #[test]
    fn when_the_selected_item_clears_the_next_one_is_selected() {
        let mut inbox = sample();
        select_first_item(&mut inbox);
        assert!(inbox.start_reply());
        inbox.update_global(global(vec![
            item("here", 2, AttentionStatus::Failed, 90),
            item("build", 3, AttentionStatus::NewOutput, 80),
        ]));
        assert!(inbox.reply.is_none());
        assert_eq!(
            inbox.selected_item().map(ItemKey::of),
            Some(ItemKey {
                session: "here".to_owned(),
                pane_id: PaneId::Terminal(2)
            })
        );
    }

    #[test]
    fn reply_mode_ends_when_the_status_changes() {
        let mut inbox = sample();
        select_first_item(&mut inbox);
        inbox.start_reply();
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 100),
            item("here", 2, AttentionStatus::Failed, 90),
        ]));
        assert!(inbox.reply.is_some());
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::Working, 150),
            item("here", 2, AttentionStatus::Failed, 90),
        ]));
        assert!(inbox.reply.is_none());
    }

    #[test]
    fn next_moves_through_counted_items_only() {
        let mut inbox = sample();
        assert!(inbox.select_next_counted());
        assert_eq!(inbox.selected_item().unwrap().session, "api");
        assert!(inbox.select_next_counted());
        assert_eq!(inbox.selected_item().unwrap().session, "here");
        assert!(inbox.select_next_counted());
        assert_eq!(inbox.selected_item().unwrap().session, "api");
    }

    #[test]
    fn the_card_lists_needs_and_offers_the_way_back_when_calm() {
        let inbox = sample();
        let content = inbox.card_content(200);
        assert_eq!(content.title, "Inbox (2)");
        assert_eq!(content.items.len(), 2);
        assert_eq!(content.activity, 2);
        assert_eq!(content.nothing, None);
        let mut calm = Inbox::default();
        calm.update_global(global(vec![item("api", 1, AttentionStatus::Quiet, 1)]));
        calm.update_history(SessionHistoryInfo {
            current: "here".to_owned(),
            previous: Some("api".to_owned()),
            ..Default::default()
        });
        let content = calm.card_content(200);
        assert_eq!(
            content.nothing.as_deref(),
            Some("Nothing needs you · 2 running")
        );
        assert_eq!(content.back_to.as_deref(), Some("api"));
        assert_eq!(calm.tab, BrowserTab::Inbox);
    }

    #[test]
    fn after_reattaching_the_card_lists_what_changed() {
        let mut inbox = sample();
        inbox.away_since_ms = Some(95);
        let content = inbox.card_content(95 + 2 * 3_600_000);
        assert_eq!(content.title, "While you were away · 2h 0m");
        assert_eq!(content.items.len(), 1);
        assert_eq!(content.activity, 0);
    }

    #[test]
    fn the_reattach_card_lists_new_items_and_items_whose_status_changed() {
        let mut inbox = Inbox::default();
        let mut changed = item("api", 2, AttentionStatus::Failed, 50);
        changed.item.attention.changed_ms = 120;
        let mut untouched = item("api", 3, AttentionStatus::Done, 40);
        untouched.item.attention.changed_ms = 60;
        let mut quiet = item("build", 4, AttentionStatus::Quiet, 130);
        quiet.item.attention.changed_ms = 130;
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 110),
            changed,
            untouched,
            quiet,
        ]));
        inbox.away_since_ms = Some(100);
        let content = inbox.card_content(200);
        let listed: Vec<PaneId> = content
            .items
            .iter()
            .map(|index| inbox.items()[*index].item.pane_id)
            .collect();
        assert_eq!(listed, vec![PaneId::Terminal(1), PaneId::Terminal(2)]);
        assert_eq!(content.activity, 1);
    }

    #[test]
    fn scrolling_starts_two_rows_before_the_edge() {
        let mut inbox = Inbox::default();
        inbox.follow_selection(Some(7), 10, 30);
        assert_eq!(inbox.list_scroll, 0);
        inbox.follow_selection(Some(8), 10, 30);
        assert_eq!(inbox.list_scroll, 1);
        inbox.follow_selection(Some(29), 10, 30);
        assert_eq!(inbox.list_scroll, 20);
        inbox.follow_selection(Some(22), 10, 30);
        assert_eq!(inbox.list_scroll, 20);
        inbox.follow_selection(Some(21), 10, 30);
        assert_eq!(inbox.list_scroll, 19);
        inbox.follow_selection(Some(0), 10, 30);
        assert_eq!(inbox.list_scroll, 0);
    }

    #[test]
    fn ages_are_short() {
        assert_eq!(age_label(0, 5_000, false), "");
        assert_eq!(age_label(1_000, 6_000, false), "just now");
        assert_eq!(age_label(1_000, 11_000, false), "<1m ago");
        assert_eq!(age_label(1, 120_001, false), "2m ago");
        assert_eq!(age_label(1, 7_200_001, false), "2h ago");
        assert_eq!(age_label(1, 3 * 86_400_000 + 1, false), "3d ago");
        assert_eq!(age_label(1_000, 6_000, true), "now");
        assert_eq!(age_label(1_000, 11_000, true), "<1m");
        assert_eq!(age_label(1, 120_001, true), "2m");
    }

    #[test]
    fn the_clock_wakes_up_when_the_next_age_label_changes() {
        assert_eq!(age_label_changes_in_ms(1_000, 4_000), 7_000);
        assert_eq!(age_label_changes_in_ms(0, 30_000), 30_000);
        assert_eq!(age_label_changes_in_ms(0, 90_000), 30_000);
        assert_eq!(age_label_changes_in_ms(0, 3_600_000 + 5_000), 3_595_000);
        let mut inbox = sample();
        let now = 100 + 2 * 3_600_000;
        let seconds = inbox.seconds_until_an_age_changes(now).unwrap();
        assert!(seconds <= AGE_CLOCK_MAX_MS as f64 / 1000.0 + AGE_CLOCK_SLACK_SECONDS);
        inbox.update_global(global(vec![item(
            "api",
            1,
            AttentionStatus::NeedsYou,
            now - 2_000,
        )]));
        let seconds = inbox.seconds_until_an_age_changes(now).unwrap();
        assert!((seconds - 8.05).abs() < 0.001);
        inbox.update_global(global(vec![]));
        assert_eq!(inbox.seconds_until_an_age_changes(now), None);
    }

    #[test]
    fn untrusted_text_never_carries_controls() {
        let mut bad = item("api", 1, AttentionStatus::NeedsYou, 1);
        bad.item.attention.text = "evil\u{1b}[31m\u{202e}text".to_owned();
        let widths = ColumnWidths {
            session: 4,
            pane: 6,
            age: 2,
            compact_age: false,
        };
        let line = item_line(&bad, &widths, 0, 60, 2, false).line;
        assert!(!line.contains('\u{1b}'));
        assert!(!line.contains('\u{202e}'));
    }

    fn select_activity(inbox: &mut Inbox) {
        let index = inbox
            .visible_rows()
            .iter()
            .position(|row| matches!(row, InboxRow::Group(InboxGroup::Activity, _)))
            .unwrap();
        inbox.select_row(index);
    }

    #[test]
    fn right_opens_and_left_closes_a_group() {
        let mut inbox = sample();
        select_activity(&mut inbox);
        assert!(!inbox.collapse_selected_group());
        assert!(inbox.expand_selected_group());
        assert!(inbox.is_expanded(InboxGroup::Activity));
        assert_eq!(inbox.visible_rows().len(), 10);
        assert!(!inbox.expand_selected_group());
        assert!(inbox.collapse_selected_group());
        assert!(!inbox.is_expanded(InboxGroup::Activity));
        assert_eq!(inbox.visible_rows().len(), 8);
        assert_eq!(
            inbox.selection,
            Some(Selection::Group(InboxGroup::Activity))
        );
    }

    #[test]
    fn left_on_a_grouped_item_closes_its_group_and_selects_the_group_line() {
        let mut inbox = sample();
        select_activity(&mut inbox);
        inbox.expand_selected_group();
        inbox.move_selection(1);
        assert_eq!(
            inbox.selected_item().unwrap().item.pane_id,
            PaneId::Terminal(4)
        );
        assert!(inbox.collapse_selected_group());
        assert_eq!(
            inbox.selection,
            Some(Selection::Group(InboxGroup::Activity))
        );
        assert!(!inbox.is_expanded(InboxGroup::Activity));
        inbox.select_row(1);
        assert!(inbox.collapse_selected_group());
        assert_eq!(
            inbox.selection,
            Some(Selection::Group(InboxGroup::NeedsYou))
        );
    }

    #[test]
    fn the_place_separates_session_and_pane_with_a_space() {
        let inbox = sample();
        let item = &inbox.global.as_ref().unwrap().items[0];
        assert_eq!(item.place(), "api pane1");
        assert!(!item.place().contains('›'));
        let widths = ColumnWidths {
            session: 5,
            pane: 6,
            age: 2,
            compact_age: false,
        };
        let line = item_line(item, &widths, 0, 60, 200, false);
        assert_eq!(line.session_len, 3);
        assert_eq!(line.pane_start, 8);
        assert_eq!(line.pane_len, 5);
        assert!(line.line.starts_with("● api   pane1   text 1"));
    }

    fn column_starts(line: &str, needles: &[&str]) -> Vec<usize> {
        needles
            .iter()
            .map(|needle| line[..line.find(needle).unwrap()].width())
            .collect()
    }

    #[test]
    fn every_row_lines_up_its_columns_including_rows_inside_a_group() {
        let mut long = item("a-much-longer-session", 7, AttentionStatus::Failed, 1);
        long.item.pane_title = "p".to_owned();
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 100),
            long,
            item("build", 3, AttentionStatus::NewOutput, 80),
        ]));
        let now = 200_000;
        let width = 80;
        let widths = ColumnWidths::measure(
            inbox.items().iter().map(|item| (item, GROUPED_ROW_INDENT)),
            now,
            width,
        );
        let rows: Vec<(String, usize, String)> = inbox
            .items()
            .iter()
            .map(|item| {
                let indent = if item.is_counted() {
                    0
                } else {
                    GROUPED_ROW_INDENT
                };
                let line = item_line(item, &widths, indent, width, now, false).line;
                (
                    format!("{}{}", " ".repeat(indent), line),
                    indent,
                    item.pane_label(),
                )
            })
            .collect();
        let pane_starts: Vec<usize> = rows
            .iter()
            .map(|(line, _, pane)| column_starts(line, &[pane.as_str()])[0])
            .collect();
        assert!(pane_starts.windows(2).all(|w| w[0] == w[1]), "{:?}", rows);
        let text_starts: Vec<usize> = rows
            .iter()
            .map(|(line, _, _)| column_starts(line, &["text "])[0])
            .collect();
        assert!(text_starts.windows(2).all(|w| w[0] == w[1]), "{:?}", rows);
        assert!(rows.iter().all(|(line, _, _)| line.width() == width));
    }

    #[test]
    fn an_explicitly_chosen_inbox_tab_stays_when_empty() {
        let mut inbox = Inbox::default();
        inbox.choose_inbox_tab();
        inbox.update_global(global(vec![]));
        assert_eq!(inbox.tab, BrowserTab::Inbox);
        assert!(filled(&inbox).is_empty());
        inbox.update_global(global(vec![item("api", 1, AttentionStatus::Quiet, 1)]));
        assert_eq!(inbox.tab, BrowserTab::Inbox);
        let mut unchosen = Inbox::default();
        unchosen.update_global(global(vec![]));
        assert_eq!(unchosen.tab, BrowserTab::Sessions);
    }
}

use std::ops::Range;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::app_panel::focus_session_pane;

use crate::panel::{fit_ansi_from, preview_lines, truncate_plain, PanMove, PreviewPan};

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
pub const TAB_SWITCH_HINT: &str = "<TAB> switch";
pub const EMPTY_INBOX_TEXT: &str = "Nothing in the inbox";

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
    Item(ItemKey),
    Action(ItemAction),
    ShowQuiet,
    Preview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemAction {
    Go,
    Reply,
    Dismiss,
    Preview,
}

pub const BLOCK_MAX_WIDTH: usize = 76;
pub const ITEM_TEXT_LINES: usize = 4;
pub const SIDE_GAP: usize = 3;
pub const SIDE_PREVIEW_MIN_COLS: usize = 70;
pub const SIDE_PREVIEW_MAX_COLS: usize = 100;
pub const SIDE_PREVIEW_MIN_ROWS: usize = 12;

enum BlockLine {
    Blank,
    Text {
        text: Text,
        gutter: Option<AttentionStatus>,
        target: Option<BrowserTarget>,
    },
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
    pub show_quiet: bool,
    pub preview_open: bool,
    pub selection: Option<ItemKey>,
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
    pub selected_range: Range<usize>,
    pub item_ranges: Vec<Range<usize>>,
    pub preview_pan: PreviewPan,
    pub help_hints: Vec<crate::help::Hint>,
    pub select_newest_on_arrival: bool,
}

pub fn status_word(status: AttentionStatus) -> &'static str {
    match status {
        AttentionStatus::NeedsYou => "NEEDS YOU",
        AttentionStatus::Failed => "FAILED",
        AttentionStatus::Done => "DONE",
        AttentionStatus::Working => "WORKING",
        AttentionStatus::Quiet => "WENT QUIET",
        AttentionStatus::NewOutput => "NEW OUTPUT",
        AttentionStatus::Idle => "WATCHING",
    }
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
        let previous_order = self.ordered();
        let previous_position = self.selected_position(&previous_order);
        self.global = Some(global);
        if self.select_newest_on_arrival && self.counted_count() > 0 {
            self.select_newest_on_arrival = false;
            self.tab = BrowserTab::Inbox;
            self.tab_chosen = true;
            self.select_newest_counted();
        }
        if let Some(reply) = self.reply.clone() {
            let still_same = self
                .find(&reply.key)
                .map(|item| item.item.attention.effective_status() == reply.status)
                .unwrap_or(false);
            if !still_same {
                self.reply = None;
            }
        }
        if let Some(key) = self.selection.clone() {
            if self.find(&key).is_none() {
                let order = self.ordered();
                self.selection = previous_position
                    .and_then(|position| order.get(position.min(order.len().saturating_sub(1))))
                    .map(|index| ItemKey::of(&self.items()[*index]));
            }
        }
        if !self.tab_chosen {
            self.tab = if self.counted_count() == 0 {
                BrowserTab::Sessions
            } else {
                BrowserTab::Inbox
            };
        }
        true
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

    pub fn toggle_show_quiet(&mut self) {
        self.show_quiet = !self.show_quiet;
        self.list_scroll = 0;
    }

    pub fn ordered(&self) -> Vec<usize> {
        let items = self.items();
        let mut first = vec![];
        let mut rest = vec![];
        for (index, item) in items.iter().enumerate() {
            if !self.matches_filter(item) {
                continue;
            }
            if item.is_counted() {
                first.push(index);
            } else {
                rest.push(index);
            }
        }
        first.sort_by(|a, b| {
            let (a, b) = (&items[*a].item.attention, &items[*b].item.attention);
            a.seen.cmp(&b.seen).then(b.since_ms.cmp(&a.since_ms))
        });
        if self.show_quiet {
            rest.sort_by(|a, b| {
                items[*b]
                    .item
                    .attention
                    .since_ms
                    .cmp(&items[*a].item.attention.since_ms)
            });
            first.extend(rest);
        }
        first
    }

    pub fn quiet_count(&self) -> usize {
        self.items()
            .iter()
            .filter(|item| self.matches_filter(item) && !item.is_counted())
            .count()
    }

    fn selected_position(&self, order: &[usize]) -> Option<usize> {
        if order.is_empty() {
            return None;
        }
        let found = self.selection.as_ref().and_then(|key| {
            order
                .iter()
                .position(|index| ItemKey::of(&self.items()[*index]) == *key)
        });
        Some(found.unwrap_or(0))
    }

    pub fn move_selection(&mut self, delta: isize) {
        let order = self.ordered();
        let Some(current) = self.selected_position(&order) else {
            self.selection = None;
            return;
        };
        let next = (current as isize + delta).clamp(0, order.len() as isize - 1) as usize;
        self.selection = Some(ItemKey::of(&self.items()[order[next]]));
    }

    pub fn selected_item(&self) -> Option<&GlobalAttentionItem> {
        let order = self.ordered();
        self.selected_position(&order)
            .map(|position| &self.items()[order[position]])
    }

    pub fn is_selected(&self, key: &ItemKey) -> bool {
        self.selected_item().map(ItemKey::of).as_ref() == Some(key)
    }

    pub fn select_item(&mut self, key: ItemKey) {
        if self.find(&key).map(|item| !item.is_counted()).unwrap_or(false) {
            self.show_quiet = true;
        }
        self.selection = Some(key);
    }

    pub fn select_newest_counted(&mut self) -> bool {
        let newest = self
            .ordered()
            .first()
            .map(|index| &self.items()[*index])
            .filter(|item| item.is_counted())
            .map(ItemKey::of);
        match newest {
            Some(key) => {
                self.selection = Some(key);
                true
            },
            None => false,
        }
    }

    pub fn toggle_selected_watch(&self) {
        if let Some(item) = self.selected_item() {
            toggle_session_pane_watch(&item.session, item.item.pane_id);
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
            self.preview_pan.reset();
        }
        self.preview_key = Some(key);
        let anchor = PreviewAnchor::for_status(item.item.attention.effective_status());
        get_session_pane_preview(&item.session, item.item.pane_id, anchor);
    }

    pub fn pan_preview(&mut self, pan_move: PanMove) {
        let anchor = self
            .preview
            .as_ref()
            .and_then(|p| p.anchor_line)
            .unwrap_or(0);
        self.preview_pan.apply(pan_move, anchor);
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

    pub fn render_tabs(&mut self, sessions: usize, x: usize, y: usize, cols: usize) {
        let (inbox, sessions) = self.tab_titles(sessions);
        let base = format!(" {} │ {}", inbox, sessions);
        let tab_hint = [TAB_SWITCH_HINT, "<TAB>", ""]
            .into_iter()
            .find(|hint| base.width() + 2 + hint.width() < cols)
            .unwrap_or("");
        let line = if tab_hint.is_empty() {
            base
        } else {
            format!("{}  {}", base, tab_hint)
        };
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
        if !tab_hint.is_empty() {
            let hint_start = sessions_range.end + 2;
            text = text.color_range(3, hint_start..hint_start + "<TAB>".chars().count());
        }
        print_text_with_coordinates(text, x, y, Some(cols.max(1)), None);
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
        let body_y = y + 1;
        let body_rows = rows.saturating_sub(1);
        let has_item = self.selected_item().is_some();
        let showing_preview = has_item && (self.preview_open || self.reply.is_some());
        let available_width = cols.saturating_sub(4).max(10);
        let side_preview = showing_preview && available_width >= SIDE_PREVIEW_MIN_COLS;
        let (list_width, preview_width) = if side_preview {
            let list = (available_width.saturating_sub(SIDE_GAP) * 9 / 20).clamp(28, BLOCK_MAX_WIDTH);
            let preview = available_width
                .saturating_sub(list + SIDE_GAP)
                .min(SIDE_PREVIEW_MAX_COLS);
            (list, preview)
        } else {
            (available_width.min(BLOCK_MAX_WIDTH), 0)
        };
        let block_width = if side_preview {
            list_width + SIDE_GAP + preview_width
        } else {
            list_width
        };
        let left = x + cols.saturating_sub(block_width) / 2;
        let help = self.help_block(block_width.max(list_width));
        let help_height = help.len();
        let stacked_preview = showing_preview && !side_preview;
        let lines = self.block_lines(list_width, now, simplified_ui);
        let needed = lines.len().max(1);
        let gap = if body_rows >= needed + help_height + 4 { 2 } else { 1 };
        let room = body_rows.saturating_sub(help_height + gap + 1);
        let selected_len = self.selected_range.len().max(1);
        let stacked_height = if stacked_preview {
            (room / 2)
                .clamp(4, 14)
                .min(room.saturating_sub(selected_len + 3))
        } else {
            0
        };
        let stacked_height = if stacked_height >= 4 { stacked_height } else { 0 };
        let list_room = room.saturating_sub(if stacked_height > 0 { stacked_height + 1 } else { 0 });
        let overflowing = needed > list_room;
        let list_rows = needed.min(list_room);
        let side_height = if side_preview {
            needed.max(SIDE_PREVIEW_MIN_ROWS).min(room)
        } else {
            0
        };
        let block_height = list_rows.max(side_height)
            + if stacked_height > 0 { stacked_height + 1 } else { 0 };
        let total = block_height + gap + help_height;
        let top = body_y + 1 + body_rows.saturating_sub(total + 1) / 2;
        let indicator_rows = if overflowing { 2 } else { 0 };
        let item_rows = list_rows.saturating_sub(indicator_rows).max(1);
        let scroll = self.scroll_for(item_rows, lines.len());
        let item_top = top + if overflowing { 1 } else { 0 };
        if overflowing {
            let above = self.item_ranges.iter().filter(|r| r.start < scroll).count();
            let below = self
                .item_ranges
                .iter()
                .filter(|r| r.end > scroll + item_rows)
                .count();
            if above > 0 {
                print_text_with_coordinates(
                    Text::from(format!("↑ {} more", above).as_str()).dim_all(),
                    left,
                    top,
                    None,
                    None,
                );
            }
            if below > 0 {
                print_text_with_coordinates(
                    Text::from(format!("↓ {} more", below).as_str()).dim_all(),
                    left,
                    item_top + item_rows,
                    None,
                    None,
                );
            }
        }
        for (row, line) in lines.into_iter().enumerate().skip(scroll).take(item_rows) {
            let line_y = item_top + row - scroll;
            if let BlockLine::Text {
                text,
                gutter,
                target,
            } = line
            {
                if let Some(status) = gutter {
                    print_text_with_coordinates(
                        style_status(Text::from("▌"), status, 0..1),
                        left.saturating_sub(2),
                        line_y,
                        Some(1),
                        None,
                    );
                }
                print_text_with_coordinates(text, left, line_y, Some(list_width), None);
                if let Some(target) = target {
                    self.browser_targets.push((
                        line_y,
                        left.saturating_sub(2)..left + list_width,
                        target,
                    ));
                }
            }
        }
        if side_preview && side_height >= 4 {
            self.render_preview_titled(
                left + list_width + SIDE_GAP,
                top,
                preview_width,
                side_height,
                Some(" Latest output "),
            );
        }
        if stacked_height > 0 {
            self.render_preview_titled(
                left,
                top + list_rows + 1,
                list_width,
                stacked_height,
                Some(" Latest output "),
            );
        }
        let help_y = top + block_height + gap;
        self.render_help_block(&help, left, help_y, block_width.max(list_width));
    }

    fn scroll_for(&mut self, item_rows: usize, total: usize) -> usize {
        let selected = self.selected_range.clone();
        let mut scroll = self.list_scroll.min(total.saturating_sub(item_rows));
        if selected.end > scroll + item_rows {
            scroll = self
                .item_ranges
                .iter()
                .map(|r| r.start)
                .find(|start| selected.end <= start + item_rows)
                .unwrap_or(selected.end.saturating_sub(item_rows));
        }
        if selected.start < scroll {
            scroll = selected.start;
        }
        if let Some(start) = self
            .item_ranges
            .iter()
            .map(|r| r.start)
            .filter(|start| *start <= scroll)
            .last()
        {
            if scroll > start && selected.end <= start + item_rows {
                scroll = start;
            }
        }
        self.list_scroll = scroll;
        scroll
    }

    fn primary_hints(&self) -> Vec<(crate::help::Hint, ItemAction)> {
        if self.selected_item().is_none() {
            return vec![];
        }
        let showing_preview = self.preview_open || self.reply.is_some();
        vec![
            (crate::help::hint("<ENTER>", "go there"), ItemAction::Go),
            (crate::help::hint("<r>", "reply"), ItemAction::Reply),
            (crate::help::hint("<c>", "dismiss"), ItemAction::Dismiss),
            (
                crate::help::hint(
                    "<p>",
                    if showing_preview {
                        "hide output"
                    } else {
                        "show output"
                    },
                ),
                ItemAction::Preview,
            ),
        ]
    }

    fn help_block(&self, width: usize) -> Vec<crate::help::BlockLine> {
        if let Some(reply) = self.reply.as_ref() {
            let line = format!(
                "REPLYING TO {} · <ESC> back to the list",
                sanitize_display_text(&reply.place)
            );
            return vec![crate::help::BlockLine {
                text: truncate_plain(&line, width),
                secondary: false,
            }];
        }
        let primary: Vec<crate::help::Hint> =
            self.primary_hints().into_iter().map(|(h, _)| h).collect();
        crate::help::block_lines(
            &primary,
            &self.help_hints,
            &crate::help::hint("<?>", "all keys"),
            width,
        )
    }

    fn render_help_block(
        &mut self,
        help: &[crate::help::BlockLine],
        x: usize,
        y: usize,
        width: usize,
    ) {
        if self.reply.is_some() {
            if let Some(line) = help.first() {
                let prefix = "REPLYING TO";
                let text = key_hint_text(&line.text).color_range(0, 0..prefix.chars().count());
                print_text_with_coordinates(text, x, y, Some(width), None);
            }
            return;
        }
        crate::help::render_block(help, x, y, width);
        let actions = self.primary_hints();
        for (index, line) in help.iter().enumerate() {
            if line.secondary {
                continue;
            }
            for (hint, action) in &actions {
                if let Some(columns) = crate::help::hint_columns(&line.text, hint) {
                    self.browser_targets.push((
                        y + index,
                        x + columns.start..x + columns.end,
                        BrowserTarget::Action(*action),
                    ));
                }
            }
        }
    }

    fn block_lines(
        &mut self,
        width: usize,
        now: u64,
        simplified_ui: bool,
    ) -> Vec<BlockLine> {
        let mut lines = vec![];
        self.selected_range = 0..0;
        self.item_ranges.clear();
        if self.only_here {
            lines.push(BlockLine::Text {
                text: key_hint_text("Only this session · <f> show all sessions").dim_all(),
                gutter: None,
                target: None,
            });
            lines.push(BlockLine::Blank);
        }
        let order = self.ordered();
        let selected = self.selected_position(&order);
        for (position, index) in order.iter().enumerate() {
            let item = self.items()[*index].clone();
            let is_selected = Some(position) == selected;
            let start = lines.len();
            self.item_block(&item, is_selected, width, now, simplified_ui, &mut lines);
            self.item_ranges.push(start..lines.len());
            if is_selected {
                self.selected_range = start..lines.len();
            }
            lines.push(BlockLine::Blank);
        }
        if order.iter().all(|index| !self.items()[*index].is_counted()) && !self.show_quiet {
            lines.push(BlockLine::Text {
                text: Text::from(EMPTY_INBOX_TEXT).color_range(2, ..),
                gutter: None,
                target: None,
            });
            lines.push(BlockLine::Blank);
        }
        let quiet = self.quiet_count();
        if quiet > 0 {
            let line = if self.show_quiet {
                "<a> hide items with only new output, quiet or muted".to_owned()
            } else if quiet == 1 {
                "1 more item with only new output, quiet or muted · <a> show".to_owned()
            } else {
                format!(
                    "{} more items with only new output, quiet or muted · <a> show",
                    quiet
                )
            };
            lines.push(BlockLine::Text {
                text: key_hint_text(&truncate_plain(&line, width)).dim_all(),
                gutter: None,
                target: Some(BrowserTarget::ShowQuiet),
            });
        }
        while matches!(lines.last(), Some(BlockLine::Blank)) {
            lines.pop();
        }
        lines
    }

    fn item_block(
        &self,
        item: &GlobalAttentionItem,
        selected: bool,
        width: usize,
        now: u64,
        simplified_ui: bool,
        lines: &mut Vec<BlockLine>,
    ) {
        let key = ItemKey::of(item);
        let attention = &item.item.attention;
        let status = attention.effective_status();
        let gutter = selected.then_some(status);
        let target = Some(BrowserTarget::Item(key));
        let marker = if item.is_muted() {
            if simplified_ui {
                "-"
            } else {
                "⊘"
            }
        } else if status.marker(simplified_ui).is_empty() {
            watch_symbol(simplified_ui)
        } else {
            status.marker(simplified_ui)
        };
        let word = format!("{} {}", marker, status_word(status));
        let mut details = vec![];
        let age = age_label(attention.since_ms, now, false);
        if !age.is_empty() {
            details.push(age);
        }
        if item.is_muted() {
            details.push("muted".to_owned());
        } else if attention.seen {
            details.push("seen".to_owned());
        }
        if attention.watched {
            details.push("watched".to_owned());
        }
        let heading = if details.is_empty() {
            word.clone()
        } else {
            format!("{} · {}", word, details.join(" · "))
        };
        let heading = truncate_plain(&heading, width);
        let word_len = word.chars().count().min(heading.chars().count());
        let heading_text = Text::from(heading.as_str()).dim_range(word_len..);
        let heading_text = if item.is_muted() {
            heading_text.dim_range(0..word_len)
        } else {
            style_status(heading_text, status, 0..word_len)
        };
        lines.push(BlockLine::Text {
            text: heading_text,
            gutter,
            target: target.clone(),
        });
        let pane = item.pane_label();
        let place = if item.is_current {
            format!("{} in this session", pane)
        } else {
            format!("{} in session {}", pane, item.session_label())
        };
        let place = truncate_plain(&place, width);
        let pane_len = pane.chars().count().min(place.chars().count());
        let place_text = Text::from(place.as_str()).color_range(2, 0..pane_len);
        let place_text = if item.is_current {
            place_text
        } else {
            let session_start = place.chars().count().saturating_sub(item.session_label().chars().count());
            place_text.color_range(1, session_start..)
        };
        lines.push(BlockLine::Text {
            text: place_text,
            gutter,
            target: target.clone(),
        });
        let text = item_text(item);
        if !text.is_empty() {
            let max_lines = ITEM_TEXT_LINES;
            let mut wrapped: Vec<String> = crate::wrap_words(&text, width)
                .into_iter()
                .map(|line| truncate_plain(&line, width))
                .collect();
            if wrapped.len() > max_lines {
                wrapped.truncate(max_lines);
                if let Some(last) = wrapped.last_mut() {
                    let short = truncate_plain(last, width.saturating_sub(1));
                    *last = if short.ends_with('…') {
                        short
                    } else {
                        format!("{}…", short)
                    };
                }
            }
            for line in wrapped {
                lines.push(BlockLine::Text {
                    text: Text::from(line.as_str()),
                    gutter,
                    target: target.clone(),
                });
            }
        }
    }

    fn render_preview_titled(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        fixed_title: Option<&str>,
    ) {
        if width < 10 || height < 4 {
            return;
        }
        self.browser_targets
            .push((y, x..x + width, BrowserTarget::Preview));
        let item = self.selected_item().cloned();
        let inner = width.saturating_sub(2);
        let title = match (fixed_title, item.as_ref()) {
            (Some(title), _) => title.to_owned(),
            (None, Some(item)) => format!(
                " {} ",
                truncate_plain(&item_text(item), inner.saturating_sub(3))
            ),
            (None, None) => " Preview ".to_owned(),
        };
        let title = title.as_str();
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
                None => preview_lines(&preview.contents),
            },
            (Some(_), _) => vec!["Loading…".to_owned()],
            (None, _) => vec!["Select an item to see its preview".to_owned()],
        };
        let window = self.preview_pan.place(&lines, body_rows, inner);
        for row in 0..body_rows {
            let line_y = body_y + row;
            print_text_with_coordinates(border("│"), x, line_y, Some(1), None);
            let line = lines
                .get(window.start + row)
                .map(|l| l.as_str())
                .unwrap_or("");
            print!(
                "\u{1b}[{};{}H{}",
                line_y + 1,
                x + 2,
                fit_ansi_from(line, window.skip_columns, inner)
            );
            print_text_with_coordinates(border("│"), x + width - 1, line_y, Some(1), None);
        }
        let label = window
            .label()
            .map(|label| truncate_plain(&format!(" {} ", label), inner.saturating_sub(1)))
            .unwrap_or_default();
        let bottom = format!(
            "└─{}{}┘",
            label,
            "─".repeat(inner.saturating_sub(1 + label.width()))
        );
        let label_len = label.chars().count();
        let bottom_text = border(&bottom).dim_range(2..2 + label_len);
        print_text_with_coordinates(bottom_text, x, bottom_y, Some(width), None);
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

    fn sessions_in_order(inbox: &Inbox) -> Vec<String> {
        inbox
            .ordered()
            .into_iter()
            .map(|index| inbox.items()[index].session.clone())
            .collect()
    }

    #[test]
    fn what_needs_you_is_listed_newest_unseen_first_and_the_rest_waits_behind_a_line() {
        let mut inbox = Inbox::default();
        let mut old_seen = item("api", 1, AttentionStatus::NeedsYou, 500);
        old_seen.item.attention.seen = true;
        inbox.update_global(global(vec![
            old_seen,
            item("web", 2, AttentionStatus::Done, 300),
            item("docs", 3, AttentionStatus::Failed, 100),
            item("build", 4, AttentionStatus::Quiet, 900),
        ]));
        assert_eq!(sessions_in_order(&inbox), vec!["web", "docs", "api"]);
        assert_eq!(inbox.quiet_count(), 1);
        assert!(inbox.select_newest_counted());
        assert_eq!(inbox.selected_item().unwrap().session, "web");
        inbox.move_selection(1);
        assert_eq!(inbox.selected_item().unwrap().session, "docs");
        inbox.move_selection(10);
        assert_eq!(inbox.selected_item().unwrap().session, "api");
        inbox.toggle_show_quiet();
        assert_eq!(sessions_in_order(&inbox), vec!["web", "docs", "api", "build"]);
        inbox.toggle_show_quiet();
        inbox.select_item(ItemKey {
            session: "build".to_owned(),
            pane_id: PaneId::Terminal(4),
        });
        assert!(inbox.show_quiet, "selecting a quiet item shows it");
        let mut calm = Inbox::default();
        calm.update_global(global(vec![item("build", 4, AttentionStatus::Quiet, 900)]));
        assert!(!calm.select_newest_counted());
        assert!(calm.ordered().is_empty());
    }

    #[test]
    fn this_session_only_lists_just_this_session() {
        let mut inbox = sample();
        inbox.flip_here_only();
        assert_eq!(sessions_in_order(&inbox), vec!["here"]);
        inbox.flip_here_only();
        assert_eq!(sessions_in_order(&inbox), vec!["api", "here"]);
    }

    #[test]
    fn an_inbox_asked_for_before_the_items_arrive_selects_the_newest_item() {
        let mut inbox = Inbox::default();
        inbox.select_newest_on_arrival = true;
        inbox.update_global(global(vec![
            item("docs", 3, AttentionStatus::Failed, 100),
            item("web", 2, AttentionStatus::Done, 300),
        ]));
        assert_eq!(inbox.tab, BrowserTab::Inbox);
        assert!(!inbox.select_newest_on_arrival);
        assert_eq!(inbox.selected_item().unwrap().session, "web");
    }

    #[test]
    fn the_list_keeps_its_layout_whichever_item_is_selected_or_previewed() {
        let mut inbox = sample();
        let shape = |inbox: &mut Inbox| -> Vec<bool> {
            inbox
                .block_lines(60, 200, false)
                .iter()
                .map(|line| matches!(line, BlockLine::Blank))
                .collect()
        };
        let first = shape(&mut inbox);
        inbox.move_selection(1);
        assert_eq!(shape(&mut inbox), first);
        inbox.preview_open = true;
        assert_eq!(shape(&mut inbox), first);
        inbox.help_hints = vec![crate::help::hint("<↓↑>", "select")];
        let wide = inbox.help_block(80);
        assert_eq!(wide.len(), 2);
        assert!(wide[0].text.starts_with("<ENTER> go there"));
        assert!(wide[0].text.contains("<p> hide output"));
        assert!(wide[1].secondary);
        assert!(inbox.help_block(30).len() > 2);
    }

    #[test]
    fn scrolling_moves_by_whole_notifications_and_keeps_the_selected_one_in_view() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(
            (1..=6)
                .map(|id| item("api", id, AttentionStatus::NeedsYou, 100 + id as u64))
                .collect(),
        ));
        let lines = inbox.block_lines(60, 200, false);
        let starts: Vec<usize> = inbox.item_ranges.iter().map(|r| r.start).collect();
        assert_eq!(inbox.scroll_for(8, lines.len()), 0);
        inbox.move_selection(3);
        let lines = inbox.block_lines(60, 200, false);
        let scroll = inbox.scroll_for(8, lines.len());
        assert!(starts.contains(&scroll), "{} is not an item start {:?}", scroll, starts);
        let selected = inbox.selected_range.clone();
        assert!(selected.start >= scroll && selected.end <= scroll + 8);
        inbox.move_selection(-3);
        inbox.block_lines(60, 200, false);
        assert_eq!(inbox.scroll_for(8, lines.len()), 0);
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
        inbox.select_item(ItemKey { session: "api".to_owned(), pane_id: PaneId::Terminal(1) });
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
        inbox.select_item(ItemKey { session: "api".to_owned(), pane_id: PaneId::Terminal(1) });
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
        assert_eq!(calm.tab, BrowserTab::Sessions);
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
            inbox.items().iter().map(|item| (item, 2)),
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
                    2
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
        assert!(inbox.ordered().is_empty());
        inbox.update_global(global(vec![item("api", 1, AttentionStatus::Quiet, 1)]));
        assert_eq!(inbox.tab, BrowserTab::Inbox);
        let mut unchosen = Inbox::default();
        unchosen.update_global(global(vec![]));
        assert_eq!(unchosen.tab, BrowserTab::Sessions);
    }

    #[test]
    fn a_card_opened_on_an_empty_inbox_stays_on_the_sessions_until_something_counted_arrives() {
        let mut inbox = Inbox::default();
        inbox.tab = BrowserTab::Sessions;
        inbox.tab_chosen = true;
        inbox.select_newest_on_arrival = true;
        inbox.update_global(global(vec![item("api", 1, AttentionStatus::Quiet, 1)]));
        assert_eq!(inbox.tab, BrowserTab::Sessions);
        assert!(inbox.select_newest_on_arrival);
        inbox.update_global(global(vec![item("api", 1, AttentionStatus::NeedsYou, 1)]));
        assert_eq!(inbox.tab, BrowserTab::Inbox);
        assert!(!inbox.select_newest_on_arrival);
        assert_eq!(inbox.selected_item().unwrap().session, "api");
    }

    #[test]
    fn an_unchosen_tab_opens_on_sessions_unless_something_is_counted() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![item("api", 1, AttentionStatus::Quiet, 1)]));
        assert_eq!(inbox.tab, BrowserTab::Sessions);
        inbox.update_global(global(vec![item("api", 1, AttentionStatus::NeedsYou, 1)]));
        assert_eq!(inbox.tab, BrowserTab::Inbox);
    }
}

use std::collections::VecDeque;
use std::ops::Range;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::app_panel::focus_session_pane;

use crate::panel::truncate_plain;

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
pub const LIST_MIN_COLS: usize = 28;
pub const LIST_MAX_COLS: usize = 48;
pub const LIVE_VIEW_MIN_COLS: usize = 30;
pub const LIVE_VIEW_MIN_ROWS: usize = 8;
pub const LIVE_VIEW_GAP: usize = 2;
pub const SESSIONS_VIEW_MIN_COLS: usize = 20;
pub const SESSIONS_VIEW_MIN_ROWS: usize = 3;
pub const LIVE_VIEW_REFUSED_TEXT: &str = "Typing was refused: focus the inbox and try again";
pub const TYPE_KEY: &str = "<Alt ENTER>";
pub const TYPE_INTO_IT_HINT: &str = "<Alt ENTER> type into it";
pub const SESSION_COLUMN_MAX: usize = 12;
pub const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const SPINNER_FRAME_MS: u64 = 100;
pub const WATCHING_EMPHASIS: usize = 3;
pub const SESSION_EMPHASIS: usize = 1;

pub fn spinner_frame(now: u64) -> &'static str {
    SPINNER_FRAMES[((now / SPINNER_FRAME_MS) % SPINNER_FRAMES.len() as u64) as usize]
}

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
    LiveView,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemAction {
    Go,
    Type,
    Dismiss,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveViewStatus {
    pub target: Option<PaneAddress>,
    pub state: MirrorState,
    pub input_captured: bool,
    pub input_released_by: Option<LiveViewRelease>,
    pub resize_allowed: bool,
    pub pinned: bool,
    pub source_size: Option<Size>,
    pub release_key: String,
}

impl LiveViewStatus {
    pub fn from_event(event: &Event) -> Option<Self> {
        match event {
            Event::LiveViewStateChanged {
                target,
                state,
                input_captured,
                input_released_by,
                resize_allowed,
                pinned,
                source_size,
                release_key,
                ..
            } => Some(LiveViewStatus {
                target: target.clone(),
                state: *state,
                input_captured: *input_captured,
                input_released_by: *input_released_by,
                resize_allowed: *resize_allowed,
                pinned: *pinned,
                source_size: *source_size,
                release_key: release_key.clone(),
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionsLiveTarget {
    pub key: ItemKey,
    pub tab: String,
    pub pane: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveLayout {
    pub list_x: usize,
    pub list_width: usize,
    pub top: usize,
    pub list_rows: usize,
    pub box_x: usize,
    pub box_width: usize,
    pub box_height: usize,
    pub status_y: usize,
    pub help_y: usize,
}

impl LiveLayout {
    pub fn help_width(cols: usize) -> usize {
        cols.saturating_sub(3).max(10)
    }

    pub fn compute(
        x: usize,
        y: usize,
        cols: usize,
        rows: usize,
        help_height: usize,
    ) -> Option<LiveLayout> {
        let list_x = x + 2;
        let available = cols.saturating_sub(3);
        let list_width = (available * 3 / 10).clamp(LIST_MIN_COLS, LIST_MAX_COLS);
        let box_x = list_x + list_width + LIVE_VIEW_GAP;
        let box_width = (x + cols).saturating_sub(box_x + 1);
        let top = y + 1;
        let help_y = (y + rows).saturating_sub(help_height);
        let status_y = help_y.checked_sub(1)?;
        let box_height = status_y.checked_sub(top)?;
        if box_width < LIVE_VIEW_MIN_COLS + 2 || box_height < LIVE_VIEW_MIN_ROWS + 2 {
            return None;
        }
        Some(LiveLayout {
            list_x,
            list_width,
            top,
            list_rows: box_height - 2,
            box_x,
            box_width,
            box_height,
            status_y,
            help_y,
        })
    }

    pub fn around_box(x: usize, y: usize, width: usize, height: usize) -> Option<LiveLayout> {
        let box_height = height.checked_sub(1)?;
        if width < SESSIONS_VIEW_MIN_COLS + 2 || box_height < SESSIONS_VIEW_MIN_ROWS + 2 {
            return None;
        }
        Some(LiveLayout {
            list_x: x,
            list_width: 0,
            top: y,
            list_rows: 0,
            box_x: x,
            box_width: width,
            box_height,
            status_y: y + box_height,
            help_y: y + height,
        })
    }

    pub fn view_rect(&self) -> (usize, usize, usize, usize) {
        (
            self.box_x + 1,
            self.top + 1,
            self.box_width - 2,
            self.box_height - 2,
        )
    }
}

pub fn closed_reason_text(reason: MirrorClosedReason) -> &'static str {
    match reason {
        MirrorClosedReason::PaneClosed => "the pane was closed",
        MirrorClosedReason::PaneNotFound => "the pane was not found",
        MirrorClosedReason::SessionEnding => "the session ended",
        MirrorClosedReason::Rejected => "the session refused",
        MirrorClosedReason::VersionMismatch => "preview not available",
        MirrorClosedReason::UnsupportedHost => "unsupported host",
        MirrorClosedReason::ConnectionFailed => "preview not available",
        MirrorClosedReason::ConnectionLost => "preview not available",
        MirrorClosedReason::ProtocolError => "protocol error",
        MirrorClosedReason::ContainsViewer => "it runs this session",
    }
}

pub const BLOCK_MAX_WIDTH: usize = 76;

enum BlockLine {
    Blank,
    Item {
        text: Text,
        target: Option<BrowserTarget>,
    },
    Text {
        text: Text,
        gutter: Option<AttentionStatus>,
        target: Option<BrowserTarget>,
    },
}

pub fn status_title(item: &GlobalAttentionItem) -> &'static str {
    let attention = &item.item.attention;
    match attention.effective_status() {
        AttentionStatus::NeedsYou => "Needs You",
        AttentionStatus::Failed => "Failed",
        AttentionStatus::Done => "Done",
        AttentionStatus::Working => "Working",
        AttentionStatus::Quiet => "Went Quiet",
        AttentionStatus::NewOutput => "New Output",
        AttentionStatus::Idle if attention.watched => "Watching",
        AttentionStatus::Idle => "Idle",
    }
}

pub fn status_and_text(item: &GlobalAttentionItem) -> (String, usize) {
    let status = status_title(item);
    let detail = row_text(item);
    let redundant = detail.is_empty()
        || detail.eq_ignore_ascii_case(status)
        || detail.eq_ignore_ascii_case(WATCHING_TEXT);
    let text = if redundant {
        status.to_owned()
    } else {
        format!("{} · {}", status, detail)
    };
    (text, status.chars().count())
}

pub fn row_text(item: &GlobalAttentionItem) -> String {
    let text = item_text(item);
    let attention = &item.item.attention;
    if attention.detail.kind != Some(BlockedKind::Question) {
        return text;
    }
    let prefix = format!("{}: ", BlockedKind::Question.label());
    match text.strip_prefix(&prefix) {
        Some(question) if !question.is_empty() => question.to_owned(),
        _ => text,
    }
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
    pub group_by_session: bool,
    pub selection: Option<ItemKey>,
    pub clock_armed: bool,
    pub spinner_armed: bool,
    pub card_targets: Vec<(usize, Range<usize>, CardTarget)>,
    pub browser_targets: Vec<(usize, Range<usize>, BrowserTarget)>,
    pub card_hovered: Option<CardTarget>,
    pub away_since_ms: Option<u64>,
    pub list_scroll: usize,
    pub selected_range: Range<usize>,
    pub item_ranges: Vec<Range<usize>>,
    pub help_hints: Vec<crate::help::Hint>,
    pub offer_hints: Vec<crate::help::Hint>,
    pub select_newest_on_arrival: bool,
    pub live: Option<LiveViewStatus>,
    pub resize_allowed: Option<bool>,
    pub own_resize_allowed: bool,
    pub current_session: Option<String>,
    pub live_layout: Option<LiveLayout>,
    pub live_view_drawn: bool,
    pub live_mode: bool,
    pub live_view_sent: Option<LiveView>,
    pub capture_target: Option<ItemKey>,
    capture_tab: Option<BrowserTab>,
    pub capture_position: Option<usize>,
    pub capture_labels: Option<(String, String)>,
    pub live_notice: Option<String>,
    pub resize_requests_in_flight: VecDeque<bool>,
    pub sessions_target: Option<SessionsLiveTarget>,
    pub listed: Vec<GlobalAttentionItem>,
    pub held: Vec<ItemKey>,
    viewed: Vec<ItemKey>,
    dropped: Vec<DroppedItem>,
    shown_items: Vec<GlobalAttentionItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DroppedItem {
    key: ItemKey,
    status: AttentionStatus,
    since_ms: u64,
}

impl DroppedItem {
    fn of(item: &GlobalAttentionItem) -> Self {
        DroppedItem {
            key: ItemKey::of(item),
            status: item.item.attention.effective_status(),
            since_ms: item.item.attention.since_ms,
        }
    }

    fn still_matches(&self, item: &GlobalAttentionItem) -> bool {
        item.item.attention.effective_status() == self.status
            && item.item.attention.since_ms <= self.since_ms
    }
}

fn always_listed(item: &GlobalAttentionItem) -> bool {
    item.is_counted() || item.item.attention.watched
}

fn ends_the_pane(state: MirrorState) -> bool {
    matches!(
        state,
        MirrorState::Closed(
            MirrorClosedReason::PaneClosed
                | MirrorClosedReason::PaneNotFound
                | MirrorClosedReason::SessionEnding
        )
    )
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
        AttentionStatus::Working => text,
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
        if width.saturating_sub(widths.fixed()) < PREFERRED_TEXT_WIDTH {
            widths.pane = 0;
        }
        let text_width = width.saturating_sub(widths.fixed());
        if widths.pane > 0 && text_width < MIN_TEXT_WIDTH {
            let shortfall = MIN_TEXT_WIDTH - text_width;
            widths.pane = widths
                .pane
                .saturating_sub(shortfall)
                .max(widths.pane.min(MIN_PANE_WIDTH));
        }
        widths
    }

    fn fixed(&self) -> usize {
        let session = if self.session == 0 {
            0
        } else {
            1 + self.session
        };
        let pane = if self.pane == 0 { 0 } else { 1 + self.pane };
        let age = if self.age == 0 { 0 } else { 2 + self.age };
        1 + session + pane + 2 + age
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
    pub status_start: usize,
    pub status_len: usize,
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
            text.color_range(WATCHING_EMPHASIS, 0..self.symbol_len)
        } else {
            style_status(text, status, 0..self.symbol_len)
        };
        let text = text
            .color_range(
                SESSION_EMPHASIS,
                session_start..session_start + self.session_len,
            )
            .color_range(2, self.pane_start..self.pane_start + self.pane_len);
        style_status(
            text,
            status,
            self.status_start..self.status_start + self.status_len,
        )
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
    let session = if widths.session == 0 {
        String::new()
    } else {
        format!(" {}", pad_to(&item.session_label(), session_width))
    };
    let pane = if widths.pane == 0 {
        String::new()
    } else {
        format!(" {}", pad_to(&item.pane_label(), widths.pane))
    };
    let (status_text, status_len) = status_and_text(item);
    let text_width = widths.text(width);
    let text = pad_to(&status_text, text_width);
    let age = if widths.age == 0 {
        String::new()
    } else {
        format!(
            "  {}",
            pad_left_to(
                &age_label(item.item.attention.since_ms, now, widths.compact_age),
                widths.age,
            )
        )
    };
    let line = format!("{}{}{}  {}{}", symbol, session, pane, text, age);
    let symbol_len = symbol.chars().count();
    let status_start = symbol_len + session.chars().count() + pane.chars().count() + 2;
    ItemLine {
        line,
        symbol_len,
        session_len: if widths.session == 0 {
            0
        } else {
            truncate_plain(&item.session_label(), session_width)
                .chars()
                .count()
        },
        pane_start: symbol_len + session.chars().count() + 1,
        pane_len: truncate_plain(&item.pane_label(), widths.pane)
            .chars()
            .count(),
        status_start,
        status_len: status_len.min(text_width),
    }
}

impl Inbox {
    pub fn update_global(&mut self, global: GlobalAttention) -> bool {
        if self.global.as_ref() == Some(&global) {
            return false;
        }
        let previous_order = self.ordered();
        let previous_position = self.selected_position(&previous_order);
        let previously_found = self
            .selection
            .as_ref()
            .map(|key| self.find(key).is_some())
            .unwrap_or(false);
        self.global = Some(global);
        self.refresh_listed();
        let capturing = self.is_capturing();
        if self.select_newest_on_arrival && self.has_rows() && !capturing {
            self.select_newest_on_arrival = false;
            self.tab = BrowserTab::Inbox;
            self.tab_chosen = true;
            self.select_newest_counted();
        }
        if capturing {
            if previously_found {
                self.capture_position = previous_position;
            }
        } else if let Some(key) = self.selection.clone() {
            if self.find(&key).is_none() {
                let order = self.ordered();
                self.selection = previous_position
                    .and_then(|position| order.get(position.min(order.len().saturating_sub(1))))
                    .map(|index| ItemKey::of(&self.items()[*index]));
            }
        }
        if !self.tab_chosen {
            self.tab = if !self.has_rows() {
                BrowserTab::Sessions
            } else {
                BrowserTab::Inbox
            };
        }
        true
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
        self.items()
            .iter()
            .find(|item| item.is_same_pane(&key.session, &key.pane_id))
    }

    fn items(&self) -> &[GlobalAttentionItem] {
        &self.shown_items
    }

    fn server_items(&self) -> &[GlobalAttentionItem] {
        self.global
            .as_ref()
            .map(|g| g.items.as_slice())
            .unwrap_or(&[])
    }

    fn lists(&self, item: &GlobalAttentionItem) -> bool {
        self.show_quiet || always_listed(item)
    }

    pub fn is_held(&self, key: &ItemKey) -> bool {
        self.held.contains(key)
    }

    fn holds(&self, known: &GlobalAttentionItem, server: Option<&GlobalAttentionItem>) -> bool {
        !known.is_muted()
            && !matches!(
                known.item.attention.effective_status(),
                AttentionStatus::Idle | AttentionStatus::Working
            )
            && self.viewed.contains(&ItemKey::of(known))
            && server
                .map(|item| !item.is_muted() && item.item.attention.status.is_idle())
                .unwrap_or(true)
    }

    fn note_viewed(&mut self) {
        let Some(key) = self.live_view_target() else {
            return;
        };
        if self.is_dropped(&key) || self.viewed.contains(&key) {
            return;
        }
        self.viewed.push(key);
    }

    pub fn has_rows(&self) -> bool {
        self.counted_count() > 0 || !self.held.is_empty()
    }

    fn server_item(&self, key: &ItemKey) -> Option<&GlobalAttentionItem> {
        self.server_items()
            .iter()
            .find(|item| item.is_same_pane(&key.session, &key.pane_id))
    }

    fn session_runs(&self, session: &str) -> bool {
        self.global
            .as_ref()
            .map(|global| global.has_session(session))
            .unwrap_or(true)
    }

    fn refresh_listed(&mut self) {
        let dropped = std::mem::take(&mut self.dropped);
        self.dropped = dropped
            .into_iter()
            .filter(|dropped| {
                self.server_item(&dropped.key)
                    .map(|item| dropped.still_matches(item))
                    .unwrap_or(false)
            })
            .collect();
        let listed = std::mem::take(&mut self.listed);
        self.listed = listed
            .into_iter()
            .filter(|known| self.session_runs(&known.session))
            .filter_map(|known| {
                let server = self.server_item(&ItemKey::of(&known));
                if self.holds(&known, server) {
                    let mut kept = known;
                    kept.item.attention.watched = server
                        .map(|item| item.item.attention.watched)
                        .unwrap_or(false);
                    return Some(kept);
                }
                server.filter(|item| self.lists(item)).cloned()
            })
            .collect();
        self.rebuild_items();
    }

    fn rebuild_items(&mut self) {
        let mut items = vec![];
        let mut held = vec![];
        for item in self.server_items() {
            let key = ItemKey::of(item);
            let known = self.listed.iter().find(|known| ItemKey::of(known) == key);
            match known {
                Some(known) if self.holds(known, Some(item)) => {
                    items.push(known.clone());
                    held.push(key);
                },
                _ => items.push(item.clone()),
            }
        }
        for known in &self.listed {
            let key = ItemKey::of(known);
            if self.server_item(&key).is_none() && self.holds(known, None) {
                items.push(known.clone());
                held.push(key);
            }
        }
        self.shown_items = items;
        self.held = held;
    }

    fn is_dropped(&self, key: &ItemKey) -> bool {
        self.dropped.iter().any(|dropped| dropped.key == *key)
    }

    fn drop_key(&mut self, key: &ItemKey) {
        if self.is_dropped(key) {
            return;
        }
        let known = self
            .server_item(key)
            .or_else(|| self.find(key))
            .map(DroppedItem::of);
        if let Some(dropped) = known {
            self.dropped.push(dropped);
        }
    }

    fn remember_listed(&mut self, order: &[usize]) {
        for index in order {
            let item = &self.shown_items[*index];
            let key = ItemKey::of(item);
            if self.is_dropped(&key) {
                continue;
            }
            match self
                .listed
                .iter_mut()
                .find(|known| ItemKey::of(known) == key)
            {
                Some(known) => *known = item.clone(),
                None => self.listed.push(item.clone()),
            }
        }
    }

    pub fn forget_listed(&mut self) -> bool {
        self.dropped.clear();
        self.viewed.clear();
        if self.listed.is_empty() {
            return false;
        }
        self.listed.clear();
        self.rebuild_items();
        true
    }

    fn forget_listed_where(&mut self, gone: impl Fn(&GlobalAttentionItem) -> bool) -> bool {
        let before = self.listed.len();
        let previous_position = self.selected_position(&self.ordered());
        self.listed.retain(|known| !gone(known));
        if self.listed.len() == before {
            return false;
        }
        self.rebuild_items();
        self.keep_selection_at(previous_position);
        true
    }

    fn keep_selection_at(&mut self, previous_position: Option<usize>) {
        if self.is_capturing() {
            return;
        }
        let Some(key) = self.selection.clone() else {
            return;
        };
        if self.find(&key).is_some() {
            return;
        }
        let order = self.ordered();
        self.selection = previous_position
            .and_then(|position| order.get(position.min(order.len().saturating_sub(1))))
            .map(|index| ItemKey::of(&self.items()[*index]));
    }

    pub fn forget_closed_panes_here(&mut self, open_panes: &[u32]) -> bool {
        self.forget_listed_where(|known| {
            known.is_current
                && matches!(known.item.pane_id, PaneId::Terminal(id) if !open_panes.contains(&id))
        })
    }

    pub fn dismiss_selected(&mut self) {
        let Some(key) = self.selected_item().map(ItemKey::of) else {
            return;
        };
        self.mark_selected_seen();
        self.drop_key(&key);
        self.viewed.retain(|viewed| *viewed != key);
        self.forget_listed_where(|known| ItemKey::of(known) == key);
    }

    pub fn dismiss_all(&mut self) {
        mark_all_seen(None);
        let previous_position = self.selected_position(&self.ordered());
        let keys: Vec<ItemKey> = self.items().iter().map(ItemKey::of).collect();
        for key in keys {
            self.drop_key(&key);
        }
        self.viewed.clear();
        self.listed.clear();
        self.rebuild_items();
        self.keep_selection_at(previous_position);
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
        self.rebuild_items();
    }

    pub fn toggle_grouping(&mut self) {
        self.group_by_session = !self.group_by_session;
        self.list_scroll = 0;
    }

    pub fn sections(&self) -> Vec<(Group, Vec<usize>)> {
        let mut sections: Vec<(Group, Vec<usize>)> = vec![];
        for index in self.flat_ordered() {
            let item = &self.items()[index];
            let section = if self.group_by_session {
                Group::Session(item.session.clone(), item.is_current)
            } else {
                Group::Status(Section::of(item))
            };
            match sections.iter_mut().find(|(s, _)| *s == section) {
                Some((_, members)) => members.push(index),
                None => sections.push((section, vec![index])),
            }
        }
        sections
    }

    pub fn ordered(&self) -> Vec<usize> {
        self.sections()
            .into_iter()
            .flat_map(|(_, members)| members)
            .collect()
    }

    fn position_of(&self, order: &[usize], key: &ItemKey) -> Option<usize> {
        order
            .iter()
            .position(|index| ItemKey::of(&self.items()[*index]) == *key)
    }

    fn flat_ordered(&self) -> Vec<usize> {
        let items = self.items();
        let mut shown: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                self.matches_filter(item)
                    && (self.show_quiet
                        || self.always_listed(item)
                        || self.is_held(&ItemKey::of(item)))
            })
            .map(|(index, _)| index)
            .collect();
        shown.sort_by(|a, b| {
            let (a, b) = (&items[*a], &items[*b]);
            let by_section = Section::of(a).cmp(&Section::of(b));
            let within = if Section::of(a) == Section::Watching {
                b.is_current
                    .cmp(&a.is_current)
                    .then(a.session.cmp(&b.session))
                    .then(a.item.pane_id.cmp(&b.item.pane_id))
            } else {
                let (x, y) = (&a.item.attention, &b.item.attention);
                y.since_ms
                    .cmp(&x.since_ms)
                    .then(a.session.cmp(&b.session))
                    .then(a.item.pane_id.cmp(&b.item.pane_id))
            };
            by_section.then(within)
        });
        shown
    }

    pub fn quiet_count(&self) -> usize {
        self.items()
            .iter()
            .filter(|item| {
                self.matches_filter(item)
                    && !self.always_listed(item)
                    && !self.is_held(&ItemKey::of(item))
            })
            .count()
    }

    fn always_listed(&self, item: &GlobalAttentionItem) -> bool {
        item.is_counted() || item.item.attention.watched
    }

    fn selected_position(&self, order: &[usize]) -> Option<usize> {
        if order.is_empty() {
            return None;
        }
        let found = self
            .selection
            .as_ref()
            .and_then(|key| self.position_of(order, key));
        Some(found.unwrap_or(0))
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.live_notice = None;
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
        self.live_notice = None;
        if self
            .find(&key)
            .map(|item| !self.always_listed(item))
            .unwrap_or(false)
        {
            self.show_quiet = true;
            self.rebuild_items();
        }
        self.selection = Some(key);
    }

    pub fn select_newest_counted(&mut self) -> bool {
        let order = self.ordered();
        let counted = || {
            order
                .iter()
                .map(|index| &self.items()[*index])
                .filter(|item| item.is_counted() && !self.is_held(&ItemKey::of(item)))
        };
        let newest = counted()
            .find(|item| !item.item.attention.watched)
            .or_else(|| counted().next())
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
        if let Some(item) = self.selected_target() {
            toggle_session_pane_watch(&item.session, item.item.pane_id);
        }
    }

    pub fn selected_target(&self) -> Option<&GlobalAttentionItem> {
        let shown = self.selected_item()?;
        Some(self.server_item(&ItemKey::of(shown)).unwrap_or(shown))
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
            .server_items()
            .iter()
            .enumerate()
            .filter(|(_, item)| item.is_counted() && self.changed_while_away(item))
            .map(|(index, _)| index)
            .collect();
        let activity = self
            .server_items()
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

    pub fn seconds_until_an_age_changes(&self, now: u64) -> Option<f64> {
        self.items()
            .iter()
            .filter(|item| item.item.attention.since_ms > 0)
            .map(|item| age_label_changes_in_ms(item.item.attention.since_ms, now))
            .min()
            .map(|ms| (ms.min(AGE_CLOCK_MAX_MS) as f64 / 1000.0) + AGE_CLOCK_SLACK_SECONDS)
    }

    pub fn has_working_items(&self) -> bool {
        self.ordered()
            .iter()
            .any(|index| Section::of(&self.items()[*index]) == Section::Working)
    }

    pub fn arm_spinner(&mut self) {
        if self.spinner_armed || !self.has_working_items() {
            return;
        }
        crate::timers::start(
            crate::timers::TimerOwner::Spinner,
            SPINNER_FRAME_MS as f64 / 1000.0,
        );
        self.spinner_armed = true;
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

    pub fn go_to_selected(&self) -> bool {
        match self.selected_target() {
            Some(item) => {
                focus_session_pane(&item.session, item.item.pane_id);
                true
            },
            None => false,
        }
    }

    pub fn mark_selected_seen(&self) {
        if let Some(item) = self.selected_target() {
            mark_session_pane_seen(&item.session, item.item.pane_id);
        }
    }

    pub fn cycle_selected_mute(&self) {
        let Some(item) = self.selected_target() else {
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

    pub fn is_capturing(&self) -> bool {
        self.live
            .as_ref()
            .map(|live| live.input_captured)
            .unwrap_or(false)
    }

    pub fn shows_live_view(&self) -> bool {
        self.live_layout.is_some()
    }

    pub fn current_resize_allowed(&self) -> bool {
        if self.views_own_session() {
            return self.own_resize_allowed;
        }
        self.resize_allowed
            .or_else(|| {
                self.live
                    .as_ref()
                    .filter(|live| {
                        live.target
                            .as_ref()
                            .map(|target| !self.is_own_session(&target.session))
                            .unwrap_or(false)
                    })
                    .map(|live| live.resize_allowed)
            })
            .unwrap_or(true)
    }

    pub fn toggle_resize(&mut self) {
        if self.views_own_session() {
            self.own_resize_allowed = !self.own_resize_allowed;
            return;
        }
        self.resize_allowed = Some(!self.current_resize_allowed());
    }

    pub fn is_own_session(&self, session: &str) -> bool {
        self.current_session.as_deref() == Some(session)
    }

    fn views_own_session(&self) -> bool {
        self.live_view_target()
            .map(|key| self.is_own_session(&key.session))
            .unwrap_or(false)
    }

    fn requested_resize_allowed(&self) -> Option<bool> {
        if self.views_own_session() {
            Some(self.own_resize_allowed)
        } else {
            self.resize_allowed
        }
    }

    pub fn release_key_text(&self) -> String {
        let text = self
            .live
            .as_ref()
            .map(|live| live.release_key.clone())
            .filter(|key| !key.is_empty())
            .unwrap_or_else(|| DEFAULT_LIVE_VIEW_RELEASE_KEY.to_owned());
        LiveViewReleaseGesture::display(&text)
    }

    pub fn live_view_target(&self) -> Option<ItemKey> {
        if self.is_capturing() && self.capture_tab == Some(self.tab) {
            if let Some(key) = self.capture_target.as_ref() {
                return Some(key.clone());
            }
        }
        if let Some(target) = self.sessions_target.as_ref() {
            return Some(target.key.clone());
        }
        if self.tab != BrowserTab::Inbox {
            return None;
        }
        self.selected_target().map(ItemKey::of)
    }

    pub fn leave_tab(&mut self) {
        if self.is_capturing() || self.capture_target.is_some() {
            set_live_view_input(false);
        }
        self.capture_target = None;
        self.capture_tab = None;
    }

    pub fn live_view_request(&self, layout: &LiveLayout) -> LiveView {
        let (x, y, cols, rows) = layout.view_rect();
        let target = match self.live_view_target() {
            Some(key) => PaneAddress::local(key.session, key.pane_id),
            None => PaneAddress::local("", PaneId::Terminal(0)),
        };
        LiveView {
            target,
            x,
            y,
            cols,
            rows,
            resize_allowed: self.requested_resize_allowed(),
            view_only: false,
        }
    }

    pub fn show_live_view(&mut self, layout: &LiveLayout) {
        let request = self.live_view_request(layout);
        match self.live_view_sent.as_ref() {
            Some(sent) => {
                if let Some(resize_allowed) = request.resize_allowed {
                    if sent.resize_allowed != Some(resize_allowed) {
                        self.resize_requests_in_flight.push_back(resize_allowed);
                    }
                }
            },
            None => self.resize_requests_in_flight.clear(),
        }
        set_live_view(Some(request.clone()));
        self.live_view_sent = Some(request);
        self.live_view_drawn = true;
    }

    pub fn plan_live_layout(
        &self,
        x: usize,
        y: usize,
        cols: usize,
        rows: usize,
    ) -> Option<LiveLayout> {
        let help = self.help_block(LiveLayout::help_width(cols), true);
        LiveLayout::compute(x, y, cols, rows, help.len())
    }

    pub fn prepare_live_view(&mut self, x: usize, y: usize, cols: usize, rows: usize) {
        match self.plan_live_layout(x, y, cols, rows) {
            Some(layout) => {
                self.note_viewed();
                self.show_live_view(&layout);
            },
            None => self.clear_live_view(),
        }
    }

    pub fn hide_live_view(&mut self) {
        let Some(sent) = self.live_view_sent.as_ref() else {
            return;
        };
        let hidden = LiveView {
            target: PaneAddress::local("", PaneId::Terminal(0)),
            x: sent.x,
            y: sent.y,
            cols: 0,
            rows: 0,
            resize_allowed: sent.resize_allowed,
            view_only: false,
        };
        if *sent == hidden {
            return;
        }
        set_live_view(Some(hidden.clone()));
        self.live_view_sent = Some(hidden);
    }

    pub fn clear_live_view(&mut self) {
        if self.live_view_sent.take().is_some() {
            self.resize_requests_in_flight.clear();
            set_live_view(None);
        }
    }

    pub fn start_typing(&mut self) -> bool {
        if !self.shows_live_view() || self.is_capturing() {
            return false;
        }
        let Some(key) = self.live_view_target() else {
            return false;
        };
        self.live_notice = None;
        self.capture_target = Some(key);
        self.capture_tab = Some(self.tab);
        self.capture_position = self.selected_position(&self.ordered());
        set_live_view_input(true);
        true
    }

    fn adopt_resize_allowed(&mut self, status: &LiveViewStatus) {
        let Some(target) = status.target.as_ref() else {
            self.resize_requests_in_flight.clear();
            return;
        };
        if let Some(requested) = self.resize_requests_in_flight.front() {
            if *requested == status.resize_allowed {
                self.resize_requests_in_flight.pop_front();
            }
            return;
        }
        let own = self.is_own_session(&target.session);
        let reported = Some(status.resize_allowed);
        let previous = self
            .live
            .as_ref()
            .filter(|live| {
                live.target
                    .as_ref()
                    .map(|previous| self.is_own_session(&previous.session) == own)
                    .unwrap_or(false)
            })
            .map(|live| live.resize_allowed);
        let changed_elsewhere = previous.is_some() && previous != reported;
        if own {
            if changed_elsewhere {
                self.own_resize_allowed = status.resize_allowed;
            }
            return;
        }
        if self.resize_allowed.is_none() || changed_elsewhere {
            self.resize_allowed = reported;
        }
    }

    pub fn apply_live_view_status(&mut self, status: LiveViewStatus) -> bool {
        let was_captured = self.is_capturing();
        self.adopt_resize_allowed(&status);
        let mut changed = self.live.as_ref() != Some(&status);
        if let Some(target) = status
            .target
            .as_ref()
            .filter(|_| ends_the_pane(status.state))
        {
            let key = ItemKey {
                session: target.session.clone(),
                pane_id: target.pane_id,
            };
            self.drop_key(&key);
            let session_ended =
                status.state == MirrorState::Closed(MirrorClosedReason::SessionEnding);
            changed |= self.forget_listed_where(|known| {
                ItemKey::of(known) == key || (session_ended && known.session == key.session)
            });
        }
        if status.input_captured {
            self.live_notice = None;
            if self.capture_target.is_none() {
                let inbox_item = match self.tab {
                    BrowserTab::Inbox => self.selected_item().map(ItemKey::of),
                    BrowserTab::Sessions => None,
                };
                self.capture_target = self
                    .sessions_target
                    .as_ref()
                    .map(|target| target.key.clone())
                    .or(inbox_item);
                self.capture_tab = Some(self.tab);
                self.capture_position = self.selected_position(&self.ordered());
            }
            self.live = Some(status);
            return changed;
        }
        if status.input_released_by == Some(LiveViewRelease::NotAllowed) {
            self.live_notice = Some(LIVE_VIEW_REFUSED_TEXT.to_owned());
        }
        self.live = Some(status);
        if was_captured || self.capture_target.is_some() {
            self.capture_target = None;
            self.capture_tab = None;
            self.settle_selection_after_capture();
        }
        true
    }

    fn settle_selection_after_capture(&mut self) {
        let position = self.capture_position.take();
        let Some(key) = self.selection.clone() else {
            return;
        };
        if self.find(&key).is_some() {
            return;
        }
        let order = self.ordered();
        self.selection = position
            .and_then(|position| order.get(position.min(order.len().saturating_sub(1))))
            .map(|index| ItemKey::of(&self.items()[*index]));
    }

    fn list_selected_position(&self, order: &[usize]) -> Option<usize> {
        if self.is_capturing() {
            let key = self.capture_target.as_ref().or(self.selection.as_ref())?;
            return self.position_of(order, key);
        }
        self.selected_position(order)
    }

    pub fn live_view_state_word(&self) -> String {
        match self.live.as_ref().map(|live| live.state) {
            None | Some(MirrorState::Connecting) => String::new(),
            Some(MirrorState::NoTarget) => "no pane selected".to_owned(),
            Some(MirrorState::Live) => String::new(),
            Some(MirrorState::Closed(reason)) => {
                format!("closed: {}", closed_reason_text(reason))
            },
        }
    }

    pub fn live_view_details_text(&self) -> String {
        let Some(live) = self.live.as_ref().filter(|live| live.target.is_some()) else {
            return String::new();
        };
        let mut parts = vec![];
        if !live.resize_allowed {
            parts.push("resize off".to_owned());
        }
        if live.resize_allowed || live.state != MirrorState::Live {
            return parts.join(" · ");
        }
        let view_size = self
            .live_layout
            .map(|layout| layout.view_rect())
            .map(|(_, _, cols, rows)| (cols, rows));
        if let (Some(source), Some(view)) = (live.source_size, view_size) {
            if (source.cols, source.rows) != view {
                parts.push(format!("cropped from {}x{}", source.cols, source.rows));
            }
        }
        parts.join(" · ")
    }

    pub fn live_view_hint_text(&self) -> String {
        if self.is_capturing() {
            let (session, pane) = self
                .capture_labels
                .clone()
                .unwrap_or_else(|| ("?".to_owned(), "?".to_owned()));
            return format!(
                "typing into {} › {} · {} or click outside to return",
                session,
                pane,
                self.release_key_text()
            );
        }
        if let Some(notice) = self.live_notice.as_ref() {
            return notice.clone();
        }
        if self.live_view_target().is_some() {
            return TYPE_INTO_IT_HINT.to_owned();
        }
        String::new()
    }

    fn selected_detail(&self, width: usize) -> Option<Text> {
        let item = self.live_view_target().and_then(|key| self.find(&key))?;
        let status = status_title(item);
        let detail = row_text(item);
        let redundant = detail.is_empty()
            || detail.eq_ignore_ascii_case(status)
            || detail.eq_ignore_ascii_case(WATCHING_TEXT);
        let line = if redundant {
            status.to_owned()
        } else {
            format!("{} · {}", status, detail)
        };
        let line = truncate_plain(&line, width);
        let total = line.chars().count();
        let status_end = status.chars().count().min(total);
        let text = Text::from(line.as_str());
        Some(if item.is_muted() {
            text.dim_all()
        } else {
            style_status(text, item.item.attention.effective_status(), 0..status_end)
        })
    }

    fn view_labels(&mut self) -> Option<(String, String)> {
        let target = self.live_view_target();
        let from_sessions = self
            .sessions_target
            .as_ref()
            .filter(|sessions| Some(&sessions.key) == target.as_ref())
            .map(|sessions| {
                (
                    sanitize_display_text(&sessions.key.session),
                    format!(
                        "{} › {}",
                        sanitize_display_text(&sessions.tab),
                        sanitize_display_text(&sessions.pane)
                    ),
                )
            });
        let found = from_sessions.or_else(|| {
            target.and_then(|key| self.find(&key).cloned()).map(|item| {
                (
                    sanitize_display_text(&item.session_label()),
                    sanitize_display_text(&item.pane_label()),
                )
            })
        });
        match found {
            Some(labels) => {
                self.capture_labels = Some(labels.clone());
                Some(labels)
            },
            None if self.is_capturing() => self.capture_labels.clone(),
            None => None,
        }
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
        self.live_layout = None;
        let help_width = LiveLayout::help_width(cols);
        let live_help = self.help_block(help_width, true);
        if let Some(layout) = LiveLayout::compute(x, y, cols, rows, live_help.len()) {
            self.live_layout = Some(layout);
            self.render_live_browser(&layout, &live_help, help_width, simplified_ui);
            return;
        }
        self.render_list_browser(x, y, cols, rows, simplified_ui);
    }

    fn render_live_browser(
        &mut self,
        layout: &LiveLayout,
        help: &[crate::help::BlockLine],
        help_width: usize,
        simplified_ui: bool,
    ) {
        let now = now_ms();
        let lines = self.block_lines(layout.list_width, now, simplified_ui);
        self.render_list(
            lines,
            layout.list_x,
            layout.top + 1,
            layout.list_width,
            layout.list_rows,
        );
        self.render_live_view_box(layout);
        self.render_help_block(help, layout.list_x, layout.help_y, help_width, true);
    }

    fn render_list_browser(
        &mut self,
        x: usize,
        y: usize,
        cols: usize,
        rows: usize,
        simplified_ui: bool,
    ) {
        let now = now_ms();
        let body_y = y + 1;
        let body_rows = rows.saturating_sub(1);
        let available_width = cols.saturating_sub(4).max(10);
        let list_width = available_width.min(BLOCK_MAX_WIDTH);
        let left = x + cols.saturating_sub(list_width) / 2;
        let help = self.help_block(list_width, false);
        let help_height = help.len();
        let lines = self.block_lines(list_width, now, simplified_ui);
        let needed = lines.len().max(1);
        let gap = if body_rows >= needed + help_height + 4 {
            2
        } else {
            1
        };
        let room = body_rows.saturating_sub(help_height + gap + 1);
        let list_rows = needed.min(room);
        let total = list_rows + gap + help_height;
        let top = body_y + 1 + body_rows.saturating_sub(total + 1) / 2;
        self.render_list(lines, left, top, list_width, room);
        let help_y = top + list_rows + gap;
        self.render_help_block(&help, left, help_y, list_width, false);
    }

    fn render_list(
        &mut self,
        lines: Vec<BlockLine>,
        left: usize,
        top: usize,
        list_width: usize,
        list_room: usize,
    ) -> usize {
        let needed = lines.len().max(1);
        let overflowing = needed > list_room;
        let list_rows = needed.min(list_room);
        let indicator_rows = if overflowing { 2 } else { 0 };
        let item_rows = list_rows.saturating_sub(indicator_rows).max(1);
        let scroll = self.scroll_for(item_rows, lines.len());
        let shown_end = self.shown_end(&lines, scroll, item_rows);
        let item_top = top + if overflowing { 1 } else { 0 };
        let (above, below) = self.hidden_counts(scroll, shown_end);
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
        for (row, line) in lines
            .into_iter()
            .enumerate()
            .skip(scroll)
            .take(shown_end.saturating_sub(scroll))
        {
            let line_y = item_top + row - scroll;
            if let BlockLine::Item { text, target } = line {
                print_text_with_coordinates(text, left, line_y, Some(list_width), None);
                if let Some(target) = target {
                    self.browser_targets
                        .push((line_y, left..left + list_width, target));
                }
                continue;
            }
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
        list_rows
    }

    fn live_view_top_text(&mut self, inner: usize, capturing: bool) -> Text {
        let age = self
            .live_view_target()
            .and_then(|key| self.find(&key))
            .map(|item| age_label(item.item.attention.since_ms, now_ms(), true))
            .filter(|age| !age.is_empty());
        let labels = self.view_labels();
        let target_item = self
            .live_view_target()
            .and_then(|key| self.find(&key).cloned());
        let symbol = target_item
            .as_ref()
            .map(|item| item_symbol(item, false))
            .filter(|symbol| !symbol.trim().is_empty());
        let state_word = self.live_view_state_word();
        let state_word = if state_word.is_empty() {
            age.clone().unwrap_or_default()
        } else {
            state_word
        };
        let state = if state_word.is_empty() {
            String::new()
        } else {
            truncate_plain(&format!(" {} ", state_word), inner / 2)
        };
        let title_room = inner.saturating_sub(state.width());
        let prefix = symbol
            .map(|symbol| format!(" {}", symbol))
            .unwrap_or_default();
        let (title, session_len, pane_len) = match labels {
            Some((session, pane)) => {
                let title = format!("{} {} › {} ", prefix, session, pane);
                (title, session.chars().count(), pane.chars().count())
            },
            None => (" No item selected ".to_owned(), 0, 0),
        };
        let title = if title_room >= 4 {
            truncate_plain(&title, title_room)
        } else {
            String::new()
        };
        let fill = inner.saturating_sub(title.width() + state.width());
        let top = format!("┌{}{}{}┐", title, "─".repeat(fill), state);
        let title_len = title.chars().count();
        let state_start = 1 + title_len + fill;
        let border = |text: &str| view_border(text, capturing);
        let title_end = 1 + title_len;
        let clamp = |range: Range<usize>| range.start.min(title_end)..range.end.min(title_end);
        let prefix_len = prefix.chars().count();
        let session_start = 1 + prefix_len + 1;
        let session_end = session_start + session_len;
        let pane_start = session_end + 3;
        let pane_end = pane_start + pane_len;
        let mut top_text = border(&top);
        if session_len > 0 {
            top_text = top_text
                .color_range(SESSION_EMPHASIS, clamp(session_start..session_end))
                .dim_range(clamp(session_end..pane_start))
                .dim_range(clamp(pane_end..title_end));
            if let (Some(item), true) = (target_item.as_ref(), prefix_len > 0) {
                top_text = shift_symbol_style(top_text, item, 2);
            }
        } else {
            top_text = top_text.color_range(2, clamp(1..title_end));
        }
        top_text.dim_range(state_start..state_start + state.chars().count())
    }

    pub fn render_live_view_box(&mut self, layout: &LiveLayout) {
        let x = layout.box_x;
        let y = layout.top;
        let width = layout.box_width;
        let height = layout.box_height;
        for row in y..y + height {
            self.browser_targets
                .push((row, x..x + width, BrowserTarget::LiveView));
        }
        let inner = width.saturating_sub(2);
        let capturing = self.is_capturing();
        let top_text = self.live_view_top_text(inner, capturing);
        print_text_with_coordinates(top_text, x, y, Some(width), None);
        let border = |text: &str| view_border(text, capturing);
        for row in y + 1..y + height - 1 {
            print_text_with_coordinates(border("│"), x, row, Some(1), None);
            print_text_with_coordinates(border("│"), x + width - 1, row, Some(1), None);
        }
        let details = self.live_view_details_text();
        let details = if details.is_empty() {
            String::new()
        } else {
            truncate_plain(&format!(" {} ", details), inner.saturating_sub(1))
        };
        let bottom = format!(
            "└─{}{}┘",
            details,
            "─".repeat(inner.saturating_sub(1 + details.width()))
        );
        let details_len = details.chars().count();
        print_text_with_coordinates(
            border(&bottom).dim_range(2..2 + details_len),
            x,
            y + height - 1,
            Some(width),
            None,
        );
        let hint = truncate_plain(&self.live_view_hint_text(), width);
        let detail_room = if hint.is_empty() {
            width
        } else {
            width.saturating_sub(hint.width() + 2)
        };
        let detail = if self.sessions_target.is_none()
            && !capturing
            && self.live_notice.is_none()
            && detail_room >= MIN_TEXT_WIDTH
        {
            self.selected_detail(detail_room)
        } else {
            None
        };
        let (hint_x, hint_width) = if detail.is_some() {
            (x + width - hint.width(), hint.width())
        } else {
            (x, width)
        };
        if let Some(detail) = detail {
            print_text_with_coordinates(detail, x, layout.status_y, Some(detail_room), None);
        }
        if !hint.is_empty() {
            let text = self.view_hint(&hint, capturing);
            print_text_with_coordinates(text, hint_x, layout.status_y, Some(hint_width), None);
        }
    }

    fn view_hint(&self, hint: &str, capturing: bool) -> Text {
        if capturing {
            key_hint_text(hint).color_range(0, 0.."typing into".chars().count())
        } else if self.live_notice.is_some() {
            Text::from(hint).error_color_all()
        } else {
            crate::help::line_text(&crate::help::BlockLine {
                text: hint.to_owned(),
                secondary: true,
            })
        }
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

    fn shown_end(&self, lines: &[BlockLine], scroll: usize, item_rows: usize) -> usize {
        let mut end = (scroll + item_rows).min(lines.len());
        if let Some(cut) = self
            .item_ranges
            .iter()
            .find(|r| r.start > scroll && r.start < end && r.end > end)
        {
            end = cut.start;
        }
        while end > scroll && matches!(lines.get(end - 1), Some(BlockLine::Blank)) {
            end -= 1;
        }
        end
    }

    fn hidden_counts(&self, scroll: usize, shown_end: usize) -> (usize, usize) {
        let above = self.item_ranges.iter().filter(|r| r.start < scroll).count();
        let below = self
            .item_ranges
            .iter()
            .filter(|r| r.start >= scroll && r.end > shown_end)
            .count();
        (above, below)
    }

    fn primary_hints(&self, live: bool) -> Vec<(crate::help::Hint, ItemAction)> {
        if self.selected_item().is_none() {
            return vec![];
        }
        if !live {
            return vec![
                (crate::help::hint("<ENTER>", "go there"), ItemAction::Go),
                (crate::help::hint("<c>", "dismiss"), ItemAction::Dismiss),
            ];
        }
        vec![
            (crate::help::hint("<ENTER>", "go"), ItemAction::Go),
            (crate::help::hint(TYPE_KEY, "type"), ItemAction::Type),
            (crate::help::hint("<c>", "dismiss"), ItemAction::Dismiss),
        ]
    }

    fn help_block(&self, width: usize, live: bool) -> Vec<crate::help::BlockLine> {
        let primary: Vec<crate::help::Hint> = self
            .primary_hints(live)
            .into_iter()
            .map(|(h, _)| h)
            .collect();
        if live {
            let more = crate::help::hint("<?>", "more");
            let mut lines: Vec<crate::help::BlockLine> = if self.offer_hints.is_empty() {
                vec![]
            } else {
                crate::help::footer_lines(&self.offer_hints, &more, width, 1)
                    .into_iter()
                    .map(|text| crate::help::BlockLine {
                        text,
                        secondary: true,
                    })
                    .collect()
            };
            let mut hints = primary;
            hints.push(more.clone());
            lines.extend(
                crate::help::footer_lines(&hints, &more, width, 1)
                    .into_iter()
                    .map(|text| crate::help::BlockLine {
                        text,
                        secondary: false,
                    }),
            );
            return lines;
        }
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
        live: bool,
    ) {
        crate::help::render_block(help, x, y, width);
        let actions = self.primary_hints(live);
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

    fn block_lines(&mut self, width: usize, now: u64, simplified_ui: bool) -> Vec<BlockLine> {
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
        self.remember_listed(&order);
        let selected = self.list_selected_position(&order);
        let show_age = self.live_layout.is_none();
        let mut position = 0;
        let session_width = if self.group_by_session {
            0
        } else {
            order
                .iter()
                .map(|index| session_name(&self.items()[*index]).width())
                .max()
                .unwrap_or(0)
                .min(SESSION_COLUMN_MAX)
        };
        for (section_index, (section, members)) in self.sections().into_iter().enumerate() {
            if section_index > 0 {
                lines.push(BlockLine::Blank);
            }
            let header_start = lines.len();
            let counted_members = members
                .iter()
                .filter(|index| !self.is_held(&ItemKey::of(&self.items()[**index])))
                .count();
            lines.push(BlockLine::Text {
                text: group_heading(&section, counted_members, width),
                gutter: None,
                target: None,
            });
            for (member, index) in members.iter().enumerate() {
                let item = self.items()[*index].clone();
                let is_selected = Some(position) == selected;
                let start = if member == 0 {
                    header_start
                } else {
                    lines.len()
                };
                let held = self.is_held(&ItemKey::of(&item));
                let mut text = pane_row(
                    &item,
                    session_width,
                    width,
                    now,
                    show_age,
                    simplified_ui,
                    held,
                );
                if is_selected {
                    text = text.selected();
                }
                lines.push(BlockLine::Item {
                    text,
                    target: Some(BrowserTarget::Item(ItemKey::of(&item))),
                });
                self.item_ranges.push(start..lines.len());
                if is_selected {
                    self.selected_range = start..lines.len();
                }
                position += 1;
            }
        }
        if !order.is_empty() {
            lines.push(BlockLine::Blank);
        }
        if order.is_empty() && !self.show_quiet {
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
            content
                .items
                .iter()
                .map(|index| (&self.server_items()[*index], 0)),
            now,
            content_width,
        );
        for index in content.items.iter() {
            if y > last_y {
                break;
            }
            let item = self.server_items()[*index].clone();
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

fn view_border(text: &str, capturing: bool) -> Text {
    if capturing {
        Text::from(text).color_range(3, ..)
    } else {
        Text::from(text)
    }
}

fn item_symbol(item: &GlobalAttentionItem, simplified_ui: bool) -> &'static str {
    let status = item.item.attention.effective_status();
    if item.is_muted() {
        if simplified_ui {
            "-"
        } else {
            "⊘"
        }
    } else if !status.marker(simplified_ui).is_empty() {
        status.marker(simplified_ui)
    } else if item.item.attention.watched {
        watch_symbol(simplified_ui)
    } else {
        " "
    }
}

fn shift_symbol_style(text: Text, item: &GlobalAttentionItem, at: usize) -> Text {
    let status = item.item.attention.effective_status();
    let range = at..at + 1;
    if item.is_muted() {
        text.dim_range(range)
    } else if status.is_idle() && item.item.attention.watched {
        text.color_range(WATCHING_EMPHASIS, range)
    } else {
        style_status(text, status, range)
    }
}

fn style_symbol(text: Text, item: &GlobalAttentionItem) -> Text {
    let status = item.item.attention.effective_status();
    if item.is_muted() {
        text.dim_range(0..1)
    } else if status.is_idle() && item.item.attention.watched {
        text.color_range(WATCHING_EMPHASIS, 0..1)
    } else {
        style_status(text, status, 0..1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Watching,
    NeedsYou,
    Failed,
    Done,
    Working,
    WentQuiet,
    NewOutput,
    Idle,
    Muted,
}

impl Section {
    pub fn of(item: &GlobalAttentionItem) -> Self {
        if item.item.attention.watched {
            return Section::Watching;
        }
        if item.is_muted() {
            return Section::Muted;
        }
        match item.item.attention.effective_status() {
            AttentionStatus::NeedsYou => Section::NeedsYou,
            AttentionStatus::Failed => Section::Failed,
            AttentionStatus::Done => Section::Done,
            AttentionStatus::Working => Section::Working,
            AttentionStatus::Quiet => Section::WentQuiet,
            AttentionStatus::NewOutput => Section::NewOutput,
            AttentionStatus::Idle => Section::Idle,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Section::Watching => "Watching",
            Section::NeedsYou => "Needs You",
            Section::Failed => "Failed",
            Section::Done => "Done",
            Section::Working => "Working",
            Section::WentQuiet => "Went Quiet",
            Section::NewOutput => "New Output",
            Section::Idle => "Idle",
            Section::Muted => "Muted",
        }
    }

    fn style(&self, text: Text, range: Range<usize>) -> Text {
        match self {
            Section::Watching => text.color_range(WATCHING_EMPHASIS, range),
            Section::NeedsYou => style_status(text, AttentionStatus::NeedsYou, range),
            Section::Failed => style_status(text, AttentionStatus::Failed, range),
            Section::Done => style_status(text, AttentionStatus::Done, range),
            Section::Working => text,
            _ => text.dim_range(range),
        }
    }
}

fn session_name(item: &GlobalAttentionItem) -> String {
    if item.is_current {
        CURRENT_SESSION_LABEL.to_owned()
    } else {
        sanitize_display_text(&item.session)
    }
}

pub fn truncate_middle(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    if width <= 1 {
        return truncate_plain(text, width);
    }
    let tail_width = (width - 1) / 2;
    let head_width = width - 1 - tail_width;
    let mut head = String::new();
    let mut taken = 0;
    for c in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if taken + w > head_width {
            break;
        }
        taken += w;
        head.push(c);
    }
    let mut tail = String::new();
    let mut taken = 0;
    for c in text.chars().rev() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if taken + w > tail_width {
            break;
        }
        taken += w;
        tail.insert(0, c);
    }
    format!("{}…{}", head, tail)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Group {
    Status(Section),
    Session(String, bool),
}

fn group_heading(group: &Group, count: usize, width: usize) -> Text {
    match group {
        Group::Status(section) => section_heading(*section, count, width),
        Group::Session(session, is_current) => {
            let name = if *is_current {
                CURRENT_SESSION_LABEL.to_owned()
            } else {
                sanitize_display_text(session)
            };
            let line = if count > 1 {
                format!("{} ({})", name, count)
            } else {
                name.clone()
            };
            let line = truncate_plain(&line, width);
            let total = line.chars().count();
            let name_end = name.chars().count().min(total);
            Text::from(line.as_str())
                .color_range(SESSION_EMPHASIS, 0..name_end)
                .dim_range(name_end..total)
        },
    }
}

fn section_heading(section: Section, count: usize, width: usize) -> Text {
    let title = if section == Section::Working {
        format!("{} {}", section.title(), spinner_frame(now_ms()))
    } else {
        section.title().to_owned()
    };
    let title = title.as_str();
    let line = if count > 1 {
        format!("{} ({})", title, count)
    } else {
        title.to_owned()
    };
    let line = truncate_plain(&line, width);
    let total = line.chars().count();
    let title_end = title.chars().count().min(total);
    section
        .style(Text::from(line.as_str()), 0..title_end)
        .dim_range(title_end..total)
}

fn pane_row(
    item: &GlobalAttentionItem,
    session_width: usize,
    width: usize,
    now: u64,
    show_age: bool,
    simplified_ui: bool,
    held: bool,
) -> Text {
    let symbol = item_symbol(item, simplified_ui);
    let session = truncate_plain(&session_name(item), session_width);
    let session_cell = format!(
        "{}{}",
        session,
        " ".repeat(session_width.saturating_sub(session.width()))
    );
    let head = if session_width == 0 {
        format!("{} ", symbol)
    } else {
        format!("{} {}  ", symbol, session_cell)
    };
    let head_len = head.chars().count();
    let mut tail = String::new();
    if show_age {
        let detail = row_text(item);
        if !detail.is_empty() && !detail.eq_ignore_ascii_case(WATCHING_TEXT) {
            tail.push_str(" · ");
            tail.push_str(&detail);
        }
        let age = age_label(item.item.attention.since_ms, now, true);
        if !age.is_empty() {
            tail.push_str(" · ");
            tail.push_str(&age);
        }
    }
    let room = width.saturating_sub(head.width());
    let pane = sanitize_display_text(&item.pane_label());
    let pane_room = if tail.is_empty() {
        room
    } else {
        room.min(pane.width().max(room / 2))
    };
    let pane = truncate_middle(&pane, pane_room);
    let pane_len = pane.chars().count();
    let line = truncate_plain(&format!("{}{}{}", head, pane, tail), width);
    let padded = format!("{}{}", line, " ".repeat(width.saturating_sub(line.width())));
    let total = line.chars().count();
    if held {
        return Text::from(padded.as_str()).dim_all();
    }
    let session_len = if session_width == 0 {
        0
    } else {
        session.chars().count()
    };
    let text = Text::from(padded.as_str()).color_range(SESSION_EMPHASIS, 2..2 + session_len);
    let text = if item.is_muted() {
        text.dim_range(0..total)
    } else {
        style_symbol(text, item)
    };
    let pane_end = (head_len + pane_len).min(total);
    text.dim_range(pane_end..total)
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
    fn counted_items_are_listed_by_status_and_the_rest_waits_behind_a_line() {
        let mut inbox = Inbox::default();
        let mut old_seen = item("api", 1, AttentionStatus::NeedsYou, 500);
        old_seen.item.attention.seen = true;
        inbox.update_global(global(vec![
            old_seen,
            item("web", 2, AttentionStatus::Done, 300),
            item("docs", 3, AttentionStatus::Failed, 100),
            item("build", 4, AttentionStatus::Quiet, 900),
        ]));
        assert_eq!(sessions_in_order(&inbox), vec!["api", "docs", "web"]);
        assert_eq!(inbox.quiet_count(), 1);
        assert!(inbox.select_newest_counted());
        assert_eq!(inbox.selected_item().unwrap().session, "api");
        inbox.move_selection(1);
        assert_eq!(inbox.selected_item().unwrap().session, "docs");
        inbox.move_selection(10);
        assert_eq!(inbox.selected_item().unwrap().session, "web");
        inbox.toggle_show_quiet();
        assert_eq!(
            sessions_in_order(&inbox),
            vec!["api", "docs", "web", "build"]
        );
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

    fn watched(mut item: GlobalAttentionItem) -> GlobalAttentionItem {
        item.item.attention.watched = true;
        item
    }

    #[test]
    fn watched_panes_are_permanent_rows_at_the_top_in_a_steady_order() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("web", 2, AttentionStatus::Done, 300),
            watched(item("zed", 5, AttentionStatus::Idle, 0)),
            watched(item("api", 7, AttentionStatus::NeedsYou, 900)),
            watched(item("here", 9, AttentionStatus::Idle, 0)),
            item("build", 4, AttentionStatus::Quiet, 900),
        ]));
        assert_eq!(sessions_in_order(&inbox), vec!["here", "api", "zed", "web"]);
        assert_eq!(inbox.quiet_count(), 1);
        assert!(inbox.select_newest_counted());
        assert_eq!(inbox.selected_item().unwrap().session, "web");
        inbox.select_item(ItemKey {
            session: "zed".to_owned(),
            pane_id: PaneId::Terminal(5),
        });
        assert!(
            !inbox.show_quiet,
            "a watched pane is listed without showing quiet items"
        );
        let mut only_watched = Inbox::default();
        only_watched.update_global(global(vec![
            watched(item("api", 7, AttentionStatus::NeedsYou, 900)),
            watched(item("zed", 5, AttentionStatus::Idle, 0)),
        ]));
        assert!(only_watched.select_newest_counted());
        assert_eq!(only_watched.selected_item().unwrap().session, "api");
    }

    #[test]
    fn a_question_row_shows_the_question_itself() {
        let mut question = item("api", 1, AttentionStatus::NeedsYou, 100);
        question.item.attention.text = "has a question: Deploy now?".to_owned();
        question.item.attention.detail.kind = Some(BlockedKind::Question);
        assert_eq!(row_text(&question), "Deploy now?");
        let mut approval = question.clone();
        approval.item.attention.text = "needs approval: Edit main.rs".to_owned();
        approval.item.attention.detail.kind = Some(BlockedKind::Permission);
        assert_eq!(row_text(&approval), "needs approval: Edit main.rs");
        let mut bare = question.clone();
        bare.item.attention.text = "has a question".to_owned();
        assert_eq!(row_text(&bare), "has a question");
    }

    #[test]
    fn every_item_takes_one_row() {
        let mut inbox = sample();
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 100),
            item("here", 2, AttentionStatus::Failed, 90),
            watched(item("zed", 5, AttentionStatus::Idle, 0)),
        ]));
        let lines = inbox.block_lines(60, 200, false);
        assert_eq!(
            contents(&lines),
            vec![
                "Watching",
                "◉ zed   pane5 · text 5",
                "",
                "Needs You",
                "● api   pane1 · text 1 · now",
                "",
                "Failed",
                "✗ here  pane2 · text 2 · now",
            ]
        );
        assert_eq!(inbox.item_ranges, vec![0..2, 3..5, 6..8]);
        assert_eq!(inbox.selected_range, 0..2);
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
    fn an_inbox_asked_for_before_the_items_arrive_selects_the_first_counted_item() {
        let mut inbox = Inbox::default();
        inbox.select_newest_on_arrival = true;
        inbox.update_global(global(vec![
            item("docs", 3, AttentionStatus::Failed, 100),
            item("web", 2, AttentionStatus::Done, 300),
        ]));
        assert_eq!(inbox.tab, BrowserTab::Inbox);
        assert!(!inbox.select_newest_on_arrival);
        assert_eq!(inbox.selected_item().unwrap().session, "docs");
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
        inbox.help_hints = vec![crate::help::hint("<↓↑>", "select")];
        let wide = inbox.help_block(80, false);
        assert_eq!(wide.len(), 2);
        assert!(wide[0].text.starts_with("<ENTER> go there  <c> dismiss"));
        assert!(wide[1].secondary);
        let live = inbox.help_block(80, true);
        assert_eq!(
            live,
            vec![crate::help::BlockLine {
                text: "<ENTER> go  <Alt ENTER> type  <c> dismiss  <?> more".to_owned(),
                secondary: false,
            }]
        );
        let narrow: Vec<String> = inbox
            .help_block(30, true)
            .into_iter()
            .map(|line| line.text)
            .collect();
        assert_eq!(narrow, vec!["<ENTER> go  <?> more"]);
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
        assert!(
            starts.contains(&scroll),
            "{} is not an item start {:?}",
            scroll,
            starts
        );
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
        inbox.select_item(ItemKey {
            session: "api".to_owned(),
            pane_id: PaneId::Terminal(1),
        });
        inbox.update_global(global(vec![
            item("here", 2, AttentionStatus::Failed, 90),
            item("build", 3, AttentionStatus::NewOutput, 80),
        ]));
        assert_eq!(
            inbox.selected_item().map(ItemKey::of),
            Some(ItemKey {
                session: "here".to_owned(),
                pane_id: PaneId::Terminal(2)
            })
        );
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
        assert!(
            line.line.starts_with("● api   pane1   Needs You · text 1"),
            "{}",
            line.line
        );
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
        let widths = ColumnWidths::measure(inbox.items().iter().map(|item| (item, 2)), now, width);
        let rows: Vec<(String, usize, String)> = inbox
            .items()
            .iter()
            .map(|item| {
                let indent = if item.is_counted() { 0 } else { 2 };
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
        let status_starts: Vec<usize> = inbox
            .items()
            .iter()
            .zip(rows.iter())
            .map(|(item, (line, _, _))| column_starts(line, &[status_title(item)])[0])
            .collect();
        assert!(status_starts.windows(2).all(|w| w[0] == w[1]), "{:?}", rows);
        assert!(rows.iter().all(|(line, _, _)| line.width() == width));
    }

    #[test]
    fn an_explicitly_chosen_inbox_tab_stays_when_empty() {
        let mut inbox = Inbox::default();
        inbox.tab = BrowserTab::Inbox;
        inbox.tab_chosen = true;
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

    fn render(inbox: &mut Inbox, cols: usize, rows: usize, simplified_ui: bool) {
        inbox.live_view_drawn = false;
        inbox.prepare_live_view(0, 1, cols, rows);
        inbox.render_browser(0, 1, cols, rows, simplified_ui);
    }

    fn browser_hints() -> Vec<crate::help::Hint> {
        vec![
            crate::help::hint("<↓↑>", "select"),
            crate::help::hint("<?>", "all keys"),
        ]
    }

    fn key(session: &str, id: u32) -> ItemKey {
        ItemKey {
            session: session.to_owned(),
            pane_id: PaneId::Terminal(id),
        }
    }

    #[test]
    fn clicks_on_the_help_actions_and_the_hidden_items_line_hit_what_is_drawn_there() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 100),
            item("build", 4, AttentionStatus::Quiet, 70),
        ]));
        inbox.help_hints = browser_hints();
        let help = inbox.help_block(56, false);
        assert!(help[0].text.starts_with("<ENTER> go there  <c> dismiss"));
        render(&mut inbox, 60, 15, false);
        assert_eq!(inbox.live_layout, None);
        assert_eq!(inbox.browser_target_at(5, 8), None);
        assert_eq!(
            inbox.browser_target_at(6, 8),
            Some(BrowserTarget::Item(key("api", 1)))
        );
        assert_eq!(inbox.browser_target_at(7, 8), None);
        assert_eq!(
            inbox.browser_target_at(8, 8),
            Some(BrowserTarget::ShowQuiet)
        );
        assert_eq!(inbox.browser_target_at(9, 8), None);
        assert_eq!(
            inbox.browser_target_at(11, 2),
            Some(BrowserTarget::Action(ItemAction::Go))
        );
        assert_eq!(
            inbox.browser_target_at(11, 17),
            Some(BrowserTarget::Action(ItemAction::Go))
        );
        assert_eq!(inbox.browser_target_at(11, 18), None);
        assert_eq!(
            inbox.browser_target_at(11, 22),
            Some(BrowserTarget::Action(ItemAction::Dismiss))
        );
        assert_eq!(inbox.browser_target_at(12, 8), None);
        render(&mut inbox, 60, 15, false);
        assert_eq!(
            inbox.browser_target_at(11, 22),
            Some(BrowserTarget::Action(ItemAction::Dismiss))
        );
        assert_eq!(
            inbox
                .browser_targets
                .iter()
                .filter(|(_, _, t)| matches!(t, BrowserTarget::Action(ItemAction::Go)))
                .count(),
            1
        );
    }

    #[test]
    fn the_whole_live_view_can_be_clicked_and_its_rectangle_is_inside_the_drawn_border() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![item("api", 1, AttentionStatus::NeedsYou, 100)]));
        inbox.help_hints = browser_hints();
        render(&mut inbox, 88, 15, false);
        let layout = inbox.live_layout.expect("88 columns show the live view");
        let rows: Vec<usize> = inbox
            .browser_targets
            .iter()
            .filter(|(_, _, t)| *t == BrowserTarget::LiveView)
            .map(|(y, _, _)| *y)
            .collect();
        assert_eq!(
            rows,
            (layout.top..layout.top + layout.box_height).collect::<Vec<_>>()
        );
        let (_, columns, _) = inbox
            .browser_targets
            .iter()
            .find(|(_, _, t)| *t == BrowserTarget::LiveView)
            .unwrap()
            .clone();
        assert_eq!(columns, layout.box_x..layout.box_x + layout.box_width);
        let sent = inbox.live_view_sent.clone().expect("the view was set");
        assert_eq!(sent.x, layout.box_x + 1);
        assert_eq!(sent.y, layout.top + 1);
        assert_eq!(sent.cols, layout.box_width - 2);
        assert_eq!(sent.rows, layout.box_height - 2);
        assert_eq!(sent.x + sent.cols + 1, columns.end);
        assert_eq!(sent.y + sent.rows + 1, layout.top + layout.box_height);
        assert_eq!(sent.target, PaneAddress::local("api", PaneId::Terminal(1)));
        assert!(!sent.view_only);
        assert!(layout.list_x + layout.list_width < layout.box_x);
        assert!(layout.status_y < layout.help_y);
        assert_eq!(layout.status_y, layout.top + layout.box_height);
        assert_eq!(
            inbox.browser_target_at((layout.top + 3) as isize, layout.box_x + 5),
            Some(BrowserTarget::LiveView)
        );
    }

    #[test]
    fn the_list_takes_three_tenths_and_the_view_needs_thirty_columns_and_eight_rows() {
        let layout = LiveLayout::compute(0, 1, 120, 15, 2).unwrap();
        assert_eq!(layout.list_x, 2);
        assert_eq!(layout.list_width, 35);
        assert_eq!(layout.box_x, 39);
        assert_eq!(layout.box_width, 80);
        assert_eq!(layout.view_rect(), (40, 3, 78, 9));
        let wide = LiveLayout::compute(0, 1, 300, 30, 2).unwrap();
        assert_eq!(wide.list_width, LIST_MAX_COLS);
        let narrowest = LiveLayout::compute(0, 1, 65, 15, 2).unwrap();
        assert_eq!(narrowest.list_width, LIST_MIN_COLS);
        assert_eq!(narrowest.view_rect().2, LIVE_VIEW_MIN_COLS);
        assert_eq!(LiveLayout::compute(0, 1, 64, 15, 2), None);
        assert_eq!(LiveLayout::compute(0, 1, 60, 30, 2), None);
        let shortest = LiveLayout::compute(0, 1, 88, 14, 2).unwrap();
        assert_eq!(shortest.view_rect().3, LIVE_VIEW_MIN_ROWS);
        assert_eq!(LiveLayout::compute(0, 1, 88, 13, 2), None);
        assert_eq!(LiveLayout::compute(0, 1, 88, 15, 4), None);
        let mut inbox = sample();
        render(&mut inbox, 64, 15, false);
        assert_eq!(inbox.live_layout, None);
        assert_eq!(inbox.live_view_sent, None);
        assert!(!inbox.live_view_drawn);
        render(&mut inbox, 65, 15, false);
        assert!(inbox.live_layout.is_some());
        assert!(inbox.live_view_drawn);
    }

    fn live_status(target: Option<ItemKey>, captured: bool) -> LiveViewStatus {
        LiveViewStatus {
            target: target.map(|key| PaneAddress::local(key.session, key.pane_id)),
            state: MirrorState::Live,
            input_captured: captured,
            input_released_by: None,
            resize_allowed: true,
            pinned: captured,
            source_size: None,
            release_key: "F9".to_owned(),
        }
    }

    #[test]
    fn the_target_stays_while_typing_and_the_selection_moves_on_once_typing_ends() {
        let mut inbox = sample();
        inbox.select_item(key("here", 2));
        render(&mut inbox, 88, 15, false);
        assert!(inbox.start_typing());
        assert!(inbox.apply_live_view_status(live_status(Some(key("here", 2)), true)));
        assert!(inbox.is_capturing());
        inbox.update_global(global(vec![
            item("new", 9, AttentionStatus::NeedsYou, 500),
            item("api", 1, AttentionStatus::NeedsYou, 100),
            item("here", 2, AttentionStatus::Failed, 90),
        ]));
        assert_eq!(inbox.live_view_target(), Some(key("here", 2)));
        inbox.update_global(global(vec![
            item("new", 9, AttentionStatus::NeedsYou, 500),
            item("api", 1, AttentionStatus::NeedsYou, 100),
            item("other", 5, AttentionStatus::Done, 50),
        ]));
        assert_eq!(inbox.selection, Some(key("here", 2)));
        assert_eq!(inbox.live_view_target(), Some(key("here", 2)));
        render(&mut inbox, 88, 15, false);
        assert_eq!(
            inbox
                .live_view_sent
                .as_ref()
                .map(|view| view.target.clone()),
            Some(PaneAddress::local("here", PaneId::Terminal(2)))
        );
        assert_eq!(
            inbox.list_selected_position(&inbox.ordered()),
            Some(2),
            "the cleared target stays listed while the inbox is open"
        );
        assert_eq!(
            inbox.live_view_hint_text(),
            "typing into here › pane2 · <F9> or click outside to return"
        );
        let mut released = live_status(Some(key("here", 2)), false);
        released.input_released_by = Some(LiveViewRelease::Key);
        inbox.apply_live_view_status(released);
        assert!(!inbox.is_capturing());
        assert_eq!(
            inbox.selected_item().map(ItemKey::of),
            Some(key("here", 2)),
            "a listed item the server cleared stays while the inbox is open"
        );
        assert!(inbox.is_held(&key("here", 2)));
        inbox.dismiss_selected();
        assert_eq!(
            inbox.selected_item().map(ItemKey::of),
            Some(key("other", 5))
        );
        assert_eq!(inbox.live_view_target(), Some(key("other", 5)));
    }

    #[test]
    fn a_refused_capture_is_reported_without_retrying() {
        let mut inbox = sample();
        render(&mut inbox, 88, 15, false);
        assert!(inbox.start_typing());
        let mut refused = live_status(inbox.live_view_target(), false);
        refused.input_released_by = Some(LiveViewRelease::NotAllowed);
        inbox.apply_live_view_status(refused);
        assert_eq!(inbox.live_view_hint_text(), LIVE_VIEW_REFUSED_TEXT);
        assert_eq!(inbox.capture_target, None);
        inbox.move_selection(1);
        assert_eq!(inbox.live_view_hint_text(), TYPE_INTO_IT_HINT);
    }

    #[test]
    fn z_flips_resizing_for_the_next_set_live_view() {
        let mut inbox = sample();
        render(&mut inbox, 88, 15, false);
        assert_eq!(
            inbox
                .live_view_sent
                .as_ref()
                .map(|view| view.resize_allowed),
            Some(None)
        );
        let mut reported = live_status(inbox.live_view_target(), false);
        reported.resize_allowed = false;
        inbox.apply_live_view_status(reported);
        assert_eq!(inbox.resize_allowed, Some(false));
        inbox.toggle_resize();
        render(&mut inbox, 88, 15, false);
        assert_eq!(
            inbox
                .live_view_sent
                .as_ref()
                .map(|view| view.resize_allowed),
            Some(Some(true))
        );
        inbox.toggle_resize();
        render(&mut inbox, 88, 15, false);
        assert_eq!(
            inbox
                .live_view_sent
                .as_ref()
                .map(|view| view.resize_allowed),
            Some(Some(false))
        );
        let mut reported = live_status(inbox.live_view_target(), false);
        reported.resize_allowed = false;
        reported.source_size = Some(Size { cols: 80, rows: 24 });
        inbox.apply_live_view_status(reported.clone());
        assert_eq!(
            inbox.live_view_details_text(),
            "resize off · cropped from 80x24"
        );
        reported.state = MirrorState::Connecting;
        inbox.apply_live_view_status(reported.clone());
        assert_eq!(inbox.live_view_details_text(), "resize off");
        let (_, _, view_cols, view_rows) = inbox.live_layout.unwrap().view_rect();
        reported.state = MirrorState::Live;
        reported.source_size = Some(Size {
            cols: view_cols,
            rows: view_rows,
        });
        inbox.apply_live_view_status(reported);
        assert_eq!(inbox.live_view_details_text(), "resize off");
        let mut resizing = live_status(inbox.live_view_target(), true);
        resizing.source_size = Some(Size { cols: 80, rows: 24 });
        inbox.toggle_resize();
        render(&mut inbox, 88, 15, false);
        inbox.apply_live_view_status(resizing);
        assert_eq!(inbox.live_view_details_text(), "");
    }

    #[test]
    fn an_empty_inbox_asks_for_no_pane() {
        let mut inbox = Inbox::default();
        inbox.tab = BrowserTab::Inbox;
        inbox.tab_chosen = true;
        inbox.update_global(global(vec![]));
        render(&mut inbox, 88, 15, false);
        let sent = inbox.live_view_sent.clone().unwrap();
        assert!(sent.target.session.is_empty());
        assert!(!inbox.start_typing());
    }

    #[test]
    fn a_list_longer_than_the_room_scrolls_by_rows_and_counts_the_rest_as_more() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(
            (1..=12)
                .map(|id| item("api", id, AttentionStatus::NeedsYou, 100 + id as u64))
                .collect(),
        ));
        inbox.help_hints = browser_hints();
        let lines = inbox.block_lines(76, 200, false);
        assert_eq!(lines.len(), 13);
        assert_eq!(inbox.scroll_for(5, lines.len()), 0);
        assert_eq!(inbox.shown_end(&lines, 0, 5), 5);
        assert_eq!(inbox.hidden_counts(0, 5), (0, 8));
        inbox.move_selection(8);
        let lines = inbox.block_lines(76, 200, false);
        let scroll = inbox.scroll_for(5, lines.len());
        let selected = inbox.selected_range.clone();
        assert!(selected.start >= scroll && selected.end <= scroll + 5);
        let end = inbox.shown_end(&lines, scroll, 5);
        let (above, below) = inbox.hidden_counts(scroll, end);
        let shown = inbox
            .item_ranges
            .iter()
            .filter(|range| range.start >= scroll && range.end <= end)
            .count();
        assert_eq!(above + below + shown, 12);
        assert!(above > 0);
        render(&mut inbox, 60, 17, false);
        let mut rows: Vec<usize> = inbox
            .browser_targets
            .iter()
            .filter(|(_, _, t)| matches!(t, BrowserTarget::Item(_)))
            .map(|(y, _, _)| *y)
            .collect();
        rows.sort();
        assert!(!rows.is_empty());
        assert!(rows.windows(2).all(|w| w[1] == w[0] + 1), "{:?}", rows);
    }

    #[test]
    fn an_unchosen_tab_opens_on_sessions_unless_something_is_counted() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![item("api", 1, AttentionStatus::Quiet, 1)]));
        assert_eq!(inbox.tab, BrowserTab::Sessions);
        inbox.update_global(global(vec![item("api", 1, AttentionStatus::NeedsYou, 1)]));
        assert_eq!(inbox.tab, BrowserTab::Inbox);
    }

    fn contents(lines: &[BlockLine]) -> Vec<String> {
        lines
            .iter()
            .map(|line| match line {
                BlockLine::Blank => String::new(),
                BlockLine::Item { text, .. } | BlockLine::Text { text, .. } => {
                    text.content().trim_end().to_owned()
                },
            })
            .collect()
    }

    fn level(text: &Text, level: usize) -> Vec<usize> {
        StyledText::from(text.clone())
            .indices
            .get(level)
            .cloned()
            .unwrap_or_default()
    }

    fn muted(mut item: GlobalAttentionItem) -> GlobalAttentionItem {
        item.item.attention.muted = Some(MuteScope::Pane);
        item
    }

    fn mixed() -> Inbox {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            watched(item("zed", 5, AttentionStatus::Idle, 0)),
            watched(item("api", 7, AttentionStatus::NewOutput, 50)),
            watched(item("here", 9, AttentionStatus::Done, 900)),
            item("api", 1, AttentionStatus::NeedsYou, 100),
            item("docs", 3, AttentionStatus::NeedsYou, 300),
            item("here", 2, AttentionStatus::Failed, 90),
            item("web", 6, AttentionStatus::Done, 50),
            item("work", 8, AttentionStatus::Working, 70),
            item("build", 4, AttentionStatus::Quiet, 900),
            item("logs", 10, AttentionStatus::NewOutput, 800),
            item("calm", 11, AttentionStatus::Idle, 0),
            muted(item("noisy", 12, AttentionStatus::NeedsYou, 950)),
        ]));
        inbox
    }

    fn places_in_order(inbox: &Inbox) -> Vec<(String, u32)> {
        inbox
            .ordered()
            .into_iter()
            .map(|index| {
                let item = &inbox.items()[index];
                let id = match item.item.pane_id {
                    PaneId::Terminal(id) | PaneId::Plugin(id) => id,
                };
                (item.session.clone(), id)
            })
            .collect()
    }

    fn places(list: &[(&str, u32)]) -> Vec<(String, u32)> {
        list.iter()
            .map(|(session, id)| (session.to_string(), *id))
            .collect()
    }

    fn section_order(inbox: &Inbox) -> Vec<Group> {
        inbox
            .sections()
            .into_iter()
            .map(|(group, _)| group)
            .collect()
    }

    #[test]
    fn items_are_grouped_by_status_with_watching_first_in_a_fixed_order_and_newest_first_elsewhere()
    {
        let mut inbox = mixed();
        assert_eq!(
            section_order(&inbox),
            vec![
                Group::Status(Section::Watching),
                Group::Status(Section::NeedsYou),
                Group::Status(Section::Failed),
                Group::Status(Section::Done),
                Group::Status(Section::Working),
            ]
        );
        assert_eq!(
            places_in_order(&inbox),
            places(&[
                ("here", 9),
                ("api", 7),
                ("zed", 5),
                ("docs", 3),
                ("api", 1),
                ("here", 2),
                ("web", 6),
                ("work", 8),
            ])
        );
        inbox.toggle_show_quiet();
        assert_eq!(
            section_order(&inbox)[5..].to_vec(),
            vec![
                Group::Status(Section::WentQuiet),
                Group::Status(Section::NewOutput),
                Group::Status(Section::Idle),
                Group::Status(Section::Muted),
            ]
        );
        let mut seen_and_changed = mixed();
        let mut docs_seen = item("docs", 3, AttentionStatus::NeedsYou, 300);
        docs_seen.item.attention.seen = true;
        seen_and_changed.update_global(global(vec![
            watched(item("zed", 5, AttentionStatus::NeedsYou, 2_000)),
            watched(item("api", 7, AttentionStatus::Idle, 0)),
            watched(item("here", 9, AttentionStatus::Failed, 1_000)),
            item("api", 1, AttentionStatus::NeedsYou, 100),
            docs_seen,
        ]));
        assert_eq!(
            places_in_order(&seen_and_changed),
            places(&[("here", 9), ("api", 7), ("zed", 5), ("docs", 3), ("api", 1),]),
            "watched panes keep their order whatever their status, and being seen moves nothing"
        );
    }

    #[test]
    fn s_groups_by_session_with_the_watched_panes_of_each_session_first_and_back() {
        let mut inbox = mixed();
        inbox.toggle_grouping();
        assert_eq!(
            section_order(&inbox),
            vec![
                Group::Session("here".to_owned(), true),
                Group::Session("api".to_owned(), false),
                Group::Session("zed".to_owned(), false),
                Group::Session("docs".to_owned(), false),
                Group::Session("web".to_owned(), false),
                Group::Session("work".to_owned(), false),
            ]
        );
        assert_eq!(
            places_in_order(&inbox),
            places(&[
                ("here", 9),
                ("here", 2),
                ("api", 7),
                ("api", 1),
                ("zed", 5),
                ("docs", 3),
                ("web", 6),
                ("work", 8),
            ])
        );
        inbox.live_layout = LiveLayout::compute(0, 1, 120, 30, 1);
        let lines = inbox.block_lines(40, 200, false);
        assert_eq!(
            contents(&lines)[..7].to_vec(),
            vec![
                "here (2)",
                "✓ pane9",
                "✗ pane2",
                "",
                "api (2)",
                "• pane7",
                "● pane1"
            ]
        );
        let BlockLine::Text { text: heading, .. } = &lines[0] else {
            panic!("the first line is a heading");
        };
        assert_eq!(level(heading, SESSION_EMPHASIS), vec![0, 1, 2, 3]);
        assert_eq!(level(heading, 4), vec![4, 5, 6, 7]);
        let BlockLine::Text { text: single, .. } = &lines[8] else {
            panic!("the zed heading");
        };
        assert_eq!(single.content(), "zed");
        for line in &lines {
            if let BlockLine::Item { text, .. } = line {
                assert!(
                    level(text, SESSION_EMPHASIS).is_empty(),
                    "a row grouped by session has no session column: {:?}",
                    text.content()
                );
            }
        }
        inbox.toggle_grouping();
        assert_eq!(section_order(&inbox)[0], Group::Status(Section::Watching));
    }

    #[test]
    fn rows_are_one_line_with_a_capped_session_column_and_the_pane_cut_in_the_middle() {
        assert_eq!(truncate_middle("abcdefghij", 5), "ab…ij");
        assert_eq!(truncate_middle("abcdefghij", 6), "abc…ij");
        assert_eq!(truncate_middle("short", 10), "short");
        assert_eq!(truncate_middle("abc", 1), "…");
        let mut long = item(
            "a-very-long-session-name",
            1,
            AttentionStatus::NeedsYou,
            100,
        );
        long.item.pane_title = "build-the-whole-workspace-now".to_owned();
        let row = pane_row(&long, SESSION_COLUMN_MAX, 40, 200, false, false, false);
        assert_eq!(row.content(), "● a-very-long…  build-the-wh…rkspace-now");
        assert_eq!(
            level(&row, SESSION_EMPHASIS),
            (2..2 + SESSION_COLUMN_MAX).collect::<Vec<_>>()
        );
        assert_eq!(level(&row, 0), vec![0]);
        assert!(level(&row, 4).is_empty());
        let listed = pane_row(
            &long,
            SESSION_COLUMN_MAX,
            60,
            200 + 180_000,
            true,
            false,
            false,
        );
        assert_eq!(
            listed.content().trim_end(),
            "● a-very-long…  build-the-whole-workspace-now · text 1 · 3m"
        );
        let pane_end = "● a-very-long…  build-the-whole-workspace-now"
            .chars()
            .count();
        let total = listed.content().trim_end().chars().count();
        assert_eq!(level(&listed, 4), (pane_end..total).collect::<Vec<_>>());
        let here = item("here", 2, AttentionStatus::Failed, 100);
        let row = pane_row(&here, 4, 30, 200, false, false, false);
        assert_eq!(row.content().trim_end(), "✗ here  pane2");
        assert_eq!(level(&row, 6), vec![0]);
        let by_session = pane_row(&here, 0, 30, 200, false, false, false);
        assert_eq!(by_session.content().trim_end(), "✗ pane2");
        assert!(level(&by_session, SESSION_EMPHASIS).is_empty());
        let mut inbox = mixed();
        inbox.live_layout = LiveLayout::compute(0, 1, 120, 30, 1);
        for line in inbox.block_lines(40, 200, false) {
            if let BlockLine::Item { text, .. } = line {
                assert!(
                    !text.content().contains(" · "),
                    "rows beside the live view carry no detail or age: {:?}",
                    text.content()
                );
            }
        }
    }

    #[test]
    fn headings_count_their_items_in_status_colours_and_working_has_a_spinner() {
        let needs_you = section_heading(Section::NeedsYou, 3, 40);
        assert_eq!(needs_you.content(), "Needs You (3)");
        assert_eq!(level(&needs_you, 0), (0..9).collect::<Vec<_>>());
        assert_eq!(level(&needs_you, 4), (9..13).collect::<Vec<_>>());
        assert_eq!(
            section_heading(Section::NeedsYou, 1, 40).content(),
            "Needs You"
        );
        assert_eq!(
            level(&section_heading(Section::Failed, 2, 40), 6),
            (0..6).collect::<Vec<_>>()
        );
        assert_eq!(
            level(&section_heading(Section::Done, 2, 40), 7),
            (0..4).collect::<Vec<_>>()
        );
        assert_eq!(
            level(
                &section_heading(Section::Watching, 2, 40),
                WATCHING_EMPHASIS
            ),
            (0..8).collect::<Vec<_>>()
        );
        for quiet in [
            Section::WentQuiet,
            Section::NewOutput,
            Section::Idle,
            Section::Muted,
        ] {
            let heading = section_heading(quiet, 1, 40);
            assert_eq!(heading.content(), quiet.title());
            assert_eq!(
                level(&heading, 4),
                (0..quiet.title().chars().count()).collect::<Vec<_>>()
            );
        }
        let working = section_heading(Section::Working, 3, 40);
        let content = working.content().to_owned();
        let words: Vec<&str> = content.split(' ').collect();
        assert_eq!(words.len(), 3, "{}", content);
        assert_eq!(words[0], "Working");
        assert!(SPINNER_FRAMES.contains(&words[1]), "{}", content);
        assert_eq!(words[2], "(3)");
        assert!((0..=3).all(|emphasis| level(&working, emphasis).is_empty()));
        assert_eq!(level(&working, 4), (9..13).collect::<Vec<_>>());
        let single = section_heading(Section::Working, 1, 40);
        assert_eq!(
            single.content().chars().count(),
            "Working ⠋".chars().count()
        );
        assert!(!single.content().contains('('));
    }

    #[test]
    fn the_view_title_shows_the_symbol_the_place_and_the_age_or_a_state_but_never_connecting_or_live(
    ) {
        let now = now_ms();
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![item(
            "api",
            1,
            AttentionStatus::NeedsYou,
            now - 180_000,
        )]));
        render(&mut inbox, 88, 15, false);
        let top = |inbox: &mut Inbox| inbox.live_view_top_text(80, false).content().to_owned();
        let with_age = top(&mut inbox);
        assert!(with_age.starts_with("┌ ● api › pane1 ─"), "{}", with_age);
        assert!(with_age.ends_with("─ 3m ┐"), "{}", with_age);
        assert_eq!(with_age.chars().count(), 82);
        for state in [MirrorState::Connecting, MirrorState::Live] {
            let mut status = live_status(Some(key("api", 1)), false);
            status.state = state;
            inbox.apply_live_view_status(status);
            let shown = top(&mut inbox);
            assert!(shown.ends_with("─ 3m ┐"), "{}", shown);
            assert!(
                !shown.contains("connecting") && !shown.contains("live"),
                "{}",
                shown
            );
        }
        let mut closed = live_status(Some(key("api", 1)), false);
        closed.state = MirrorState::Closed(MirrorClosedReason::ConnectionLost);
        inbox.apply_live_view_status(closed);
        let shown = top(&mut inbox);
        assert!(
            shown.ends_with("─ closed: preview not available ┐"),
            "{}",
            shown
        );
        assert!(!shown.contains("3m"), "{}", shown);
        let mut nothing = live_status(None, false);
        nothing.state = MirrorState::NoTarget;
        inbox.apply_live_view_status(nothing);
        assert!(top(&mut inbox).ends_with("─ no pane selected ┐"));
    }

    #[test]
    fn a_resize_change_made_elsewhere_is_adopted_but_the_plugins_own_change_is_kept() {
        let mut inbox = sample();
        render(&mut inbox, 88, 15, false);
        let target = inbox.live_view_target();
        let reported = |resize_allowed: bool| {
            let mut status = live_status(target.clone(), false);
            status.resize_allowed = resize_allowed;
            status
        };
        inbox.apply_live_view_status(reported(true));
        assert_eq!(inbox.resize_allowed, Some(true));
        render(&mut inbox, 88, 15, false);
        assert_eq!(inbox.resize_requests_in_flight, [true]);
        inbox.apply_live_view_status(reported(true));
        assert!(inbox.resize_requests_in_flight.is_empty());
        inbox.apply_live_view_status(reported(false));
        assert_eq!(
            inbox.resize_allowed,
            Some(false),
            "the action flipped resizing on the server"
        );
        render(&mut inbox, 88, 15, false);
        inbox.toggle_resize();
        assert_eq!(inbox.resize_allowed, Some(true));
        render(&mut inbox, 88, 15, false);
        let mut unrelated = reported(false);
        unrelated.source_size = Some(Size { cols: 80, rows: 24 });
        inbox.apply_live_view_status(unrelated);
        assert_eq!(
            inbox.resize_allowed,
            Some(true),
            "a report sent before the server saw z does not undo it"
        );
        inbox.apply_live_view_status(reported(true));
        assert_eq!(inbox.resize_allowed, Some(true));
        assert_eq!(
            inbox
                .live_view_sent
                .as_ref()
                .map(|view| view.resize_allowed),
            Some(Some(true))
        );
    }

    #[test]
    fn z_twice_before_any_reply_ends_with_resizing_on_and_a_later_action_is_still_adopted() {
        let mut inbox = sample();
        render(&mut inbox, 88, 15, false);
        let target = inbox.live_view_target();
        let reported = |resize_allowed: bool| {
            let mut status = live_status(target.clone(), false);
            status.resize_allowed = resize_allowed;
            status
        };
        inbox.apply_live_view_status(reported(true));
        render(&mut inbox, 88, 15, false);
        inbox.apply_live_view_status(reported(true));
        assert!(inbox.resize_requests_in_flight.is_empty());
        inbox.toggle_resize();
        render(&mut inbox, 88, 15, false);
        inbox.toggle_resize();
        render(&mut inbox, 88, 15, false);
        assert_eq!(inbox.resize_requests_in_flight, [false, true]);
        let mut unrelated = reported(true);
        unrelated.pinned = true;
        inbox.apply_live_view_status(unrelated);
        inbox.apply_live_view_status(reported(false));
        assert_eq!(
            inbox.resize_allowed,
            Some(true),
            "the late reply to the first z is not a change made elsewhere"
        );
        inbox.apply_live_view_status(reported(true));
        assert_eq!(inbox.resize_allowed, Some(true));
        assert!(inbox.resize_requests_in_flight.is_empty());
        render(&mut inbox, 88, 15, false);
        assert_eq!(
            inbox
                .live_view_sent
                .as_ref()
                .map(|view| view.resize_allowed),
            Some(Some(true))
        );
        inbox.apply_live_view_status(reported(false));
        assert_eq!(
            inbox.resize_allowed,
            Some(false),
            "the action is still adopted once the replies are in"
        );
    }

    fn with_sessions(items: Vec<GlobalAttentionItem>, names: &[&str]) -> GlobalAttention {
        let mut global = global(items);
        for name in names {
            if !global.has_session(name) {
                global.sessions.push(GlobalSessionAttention {
                    name: (*name).to_owned(),
                    is_current: false,
                    attention: Default::default(),
                });
            }
        }
        global
    }

    fn browsed() -> Inbox {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 300),
            item("web", 2, AttentionStatus::Done, 200),
            item("docs", 3, AttentionStatus::Done, 100),
        ]));
        inbox.select_item(key("web", 2));
        render(&mut inbox, 88, 15, false);
        inbox
    }

    fn row_for(inbox: &mut Inbox, session: &str) -> Text {
        inbox
            .block_lines(60, 200, false)
            .into_iter()
            .find_map(|line| match line {
                BlockLine::Item { text, .. } if text.content().contains(session) => Some(text),
                _ => None,
            })
            .expect("the row is listed")
    }

    #[test]
    fn a_done_item_seen_while_browsing_stays_dimmed_and_selected_until_the_inbox_closes() {
        let mut inbox = browsed();
        assert_eq!(inbox.counted_count(), 3);
        inbox.update_global(with_sessions(
            vec![
                item("api", 1, AttentionStatus::NeedsYou, 300),
                item("docs", 3, AttentionStatus::Done, 100),
            ],
            &["web"],
        ));
        assert_eq!(inbox.counted_count(), 2, "the count drops at once");
        assert_eq!(inbox.tab_titles(1).0, "Inbox (2)");
        assert_eq!(
            sessions_in_order(&inbox),
            vec!["api", "web", "docs"],
            "the seen item keeps its place"
        );
        assert_eq!(inbox.selected_item().map(ItemKey::of), Some(key("web", 2)));
        assert!(inbox.is_held(&key("web", 2)));
        let held_row = row_for(&mut inbox, "web");
        assert_eq!(
            level(&held_row, 4).len(),
            held_row.content().chars().count()
        );
        assert!(level(&held_row, 4).len() > 0);
        let docs_row = row_for(&mut inbox, "docs");
        assert!(level(&docs_row, 4).len() < docs_row.content().chars().count());
        render(&mut inbox, 88, 15, false);
        assert_eq!(
            inbox
                .live_view_sent
                .as_ref()
                .map(|view| view.target.clone()),
            Some(PaneAddress::local("web", PaneId::Terminal(2))),
            "the seen item is still shown live"
        );
        inbox.move_selection(1);
        inbox.move_selection(-1);
        assert_eq!(inbox.selected_item().map(ItemKey::of), Some(key("web", 2)));
        assert!(inbox.forget_listed(), "closing the inbox drops it");
        assert_eq!(sessions_in_order(&inbox), vec!["api", "docs"]);
        render(&mut inbox, 88, 15, false);
        assert_eq!(sessions_in_order(&inbox), vec!["api", "docs"]);
    }

    #[test]
    fn a_seen_item_that_comes_back_is_shown_as_the_server_has_it() {
        let mut inbox = browsed();
        inbox.update_global(with_sessions(
            vec![item("api", 1, AttentionStatus::NeedsYou, 300)],
            &["web", "docs"],
        ));
        assert!(inbox.is_held(&key("web", 2)));
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 300),
            item("web", 2, AttentionStatus::Failed, 400),
        ]));
        assert!(!inbox.is_held(&key("web", 2)));
        assert_eq!(status_title(inbox.find(&key("web", 2)).unwrap()), "Failed");
    }

    #[test]
    fn dismissing_a_seen_item_removes_it_while_the_inbox_stays_open() {
        let mut inbox = browsed();
        inbox.update_global(with_sessions(
            vec![
                item("api", 1, AttentionStatus::NeedsYou, 300),
                item("docs", 3, AttentionStatus::Done, 100),
            ],
            &["web"],
        ));
        assert!(inbox.is_held(&key("web", 2)));
        inbox.dismiss_selected();
        assert_eq!(sessions_in_order(&inbox), vec!["api", "docs"]);
        assert_eq!(
            inbox.selected_item().map(ItemKey::of),
            Some(key("docs", 3)),
            "the selection stays in place"
        );
        render(&mut inbox, 88, 15, false);
        assert_eq!(sessions_in_order(&inbox), vec!["api", "docs"]);
    }

    #[test]
    fn a_seen_item_goes_once_its_pane_or_session_is_gone() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("here", 4, AttentionStatus::Done, 300),
            item("web", 2, AttentionStatus::Done, 200),
            item("docs", 3, AttentionStatus::Done, 100),
        ]));
        view(&mut inbox, &[key("web", 2), key("docs", 3), key("here", 4)]);
        let mut sessions_left = with_sessions(vec![], &["docs", "web"]);
        inbox.update_global(sessions_left.clone());
        assert_eq!(sessions_in_order(&inbox), vec!["here", "web", "docs"]);
        assert!(!inbox.forget_closed_panes_here(&[4]));
        assert!(inbox.forget_closed_panes_here(&[1]));
        assert_eq!(sessions_in_order(&inbox), vec!["web", "docs"]);
        let mut closed = live_status(Some(key("web", 2)), false);
        closed.state = MirrorState::Closed(MirrorClosedReason::PaneClosed);
        inbox.apply_live_view_status(closed);
        assert_eq!(sessions_in_order(&inbox), vec!["docs"]);
        sessions_left
            .sessions
            .retain(|session| session.name != "docs");
        inbox.update_global(sessions_left);
        assert!(sessions_in_order(&inbox).is_empty());
    }

    #[test]
    fn muting_unwatching_or_dismissing_everything_lets_listed_items_go() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 300),
            watched(item("zed", 5, AttentionStatus::Idle, 0)),
            item("web", 2, AttentionStatus::Done, 200),
        ]));
        render(&mut inbox, 88, 15, false);
        inbox.update_global(with_sessions(
            vec![
                muted(item("api", 1, AttentionStatus::NeedsYou, 300)),
                item("web", 2, AttentionStatus::Done, 200),
            ],
            &["zed"],
        ));
        assert_eq!(sessions_in_order(&inbox), vec!["web"]);
        assert!(inbox.held.is_empty());
        render(&mut inbox, 88, 15, false);
        inbox.dismiss_all();
        render(&mut inbox, 88, 15, false);
        inbox.update_global(with_sessions(vec![], &["api", "zed", "web"]));
        assert!(sessions_in_order(&inbox).is_empty());
    }

    fn view(inbox: &mut Inbox, keys: &[ItemKey]) {
        for key in keys {
            inbox.select_item(key.clone());
            render(inbox, 88, 15, false);
        }
    }

    #[test]
    fn only_an_item_shown_live_is_held_once_it_goes_away_whatever_its_status() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 400),
            item("work", 2, AttentionStatus::Working, 300),
            item("web", 3, AttentionStatus::Done, 200),
            item("docs", 4, AttentionStatus::Failed, 100),
        ]));
        view(&mut inbox, &[key("api", 1)]);
        inbox.update_global(with_sessions(
            vec![item("work", 2, AttentionStatus::Quiet, 300)],
            &["api", "web", "docs"],
        ));
        assert_eq!(sessions_in_order(&inbox), vec!["api"]);
        assert!(inbox.is_held(&key("api", 1)));
        assert!(!inbox.is_held(&key("web", 3)));
        assert!(!inbox.is_held(&key("docs", 4)));
        assert!(!inbox.is_held(&key("work", 2)));
        assert!(inbox.find(&key("web", 3)).is_none());
        assert!(inbox.find(&key("docs", 4)).is_none());
        assert_eq!(inbox.counted_count(), 0);
        assert!(inbox.has_rows(), "the read item still counts as a row");
        assert_eq!(inbox.selected_item().map(ItemKey::of), Some(key("api", 1)));
        assert!(
            !inbox.has_working_items(),
            "no Working row or spinner is left"
        );
        assert_eq!(
            status_title(inbox.find(&key("work", 2)).unwrap()),
            "Went Quiet"
        );
        inbox.update_global(with_sessions(
            vec![item("api", 1, AttentionStatus::NeedsYou, 500)],
            &["work", "web", "docs"],
        ));
        assert!(!inbox.is_held(&key("api", 1)));
        assert!(inbox.find(&key("work", 2)).is_none());
        assert_eq!(sessions_in_order(&inbox), vec!["api"]);
    }

    #[test]
    fn an_item_with_new_output_shown_live_stays_listed_until_the_inbox_closes() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 300),
            item("log", 2, AttentionStatus::NewOutput, 200),
        ]));
        inbox.toggle_show_quiet();
        view(&mut inbox, &[key("log", 2)]);
        inbox.update_global(with_sessions(
            vec![item("api", 1, AttentionStatus::NeedsYou, 300)],
            &["log"],
        ));
        inbox.toggle_show_quiet();
        assert!(inbox.is_held(&key("log", 2)));
        assert_eq!(sessions_in_order(&inbox), vec!["api", "log"]);
        assert_eq!(inbox.selected_item().map(ItemKey::of), Some(key("log", 2)));
        assert_eq!(inbox.quiet_count(), 0);
        assert!(inbox.forget_listed(), "closing the inbox lets it go");
        assert_eq!(sessions_in_order(&inbox), vec!["api"]);
    }

    #[test]
    fn the_newest_unread_item_is_picked_over_a_held_one() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("api", 1, AttentionStatus::NeedsYou, 300),
            item("web", 2, AttentionStatus::Done, 200),
        ]));
        view(&mut inbox, &[key("api", 1)]);
        inbox.update_global(with_sessions(
            vec![item("web", 2, AttentionStatus::Done, 200)],
            &["api"],
        ));
        assert!(inbox.is_held(&key("api", 1)));
        assert!(inbox.select_newest_counted());
        assert_eq!(inbox.selected_item().map(ItemKey::of), Some(key("web", 2)));
    }

    #[test]
    fn a_pane_of_this_session_is_shown_without_resizing_until_z_allows_it() {
        let sent = |inbox: &Inbox| inbox.live_view_sent.as_ref().and_then(|v| v.resize_allowed);
        let mut inbox = sample();
        inbox.current_session = Some("here".to_owned());
        inbox.select_item(key("here", 2));
        render(&mut inbox, 88, 15, false);
        assert_eq!(sent(&inbox), Some(false));
        assert!(!inbox.current_resize_allowed());
        inbox.toggle_resize();
        render(&mut inbox, 88, 15, false);
        assert_eq!(sent(&inbox), Some(true));
        inbox.select_item(key("api", 1));
        render(&mut inbox, 88, 15, false);
        assert_eq!(sent(&inbox), None, "other sessions keep their own setting");
    }

    #[test]
    fn the_sessions_tab_never_shows_or_types_into_the_inbox_selection() {
        let mut inbox = sample();
        render(&mut inbox, 88, 15, false);
        assert!(inbox.live_view_target().is_some());
        inbox.tab = BrowserTab::Sessions;
        assert_eq!(inbox.live_view_target(), None);
        inbox.apply_live_view_status(live_status(None, true));
        assert_eq!(inbox.capture_target, None);
    }

    #[test]
    fn a_seen_done_result_on_a_watched_pane_is_held_in_its_section() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            watched(item("zed", 5, AttentionStatus::Done, 300)),
            watched(item("api", 7, AttentionStatus::Idle, 0)),
            item("web", 2, AttentionStatus::Failed, 200),
        ]));
        inbox.select_item(key("zed", 5));
        render(&mut inbox, 88, 15, false);
        let sections_before = section_order(&inbox);
        let places_before = places_in_order(&inbox);
        inbox.update_global(global(vec![
            watched(item("zed", 5, AttentionStatus::Idle, 0)),
            watched(item("api", 7, AttentionStatus::Idle, 0)),
            item("web", 2, AttentionStatus::Failed, 200),
        ]));
        assert!(inbox.is_held(&key("zed", 5)));
        assert_eq!(status_title(inbox.find(&key("zed", 5)).unwrap()), "Done");
        assert_eq!(section_order(&inbox), sections_before);
        assert_eq!(places_in_order(&inbox), places_before);
        assert_eq!(inbox.selected_item().map(ItemKey::of), Some(key("zed", 5)));
        assert!(inbox.forget_listed(), "closing the inbox lets it go");
        assert!(!inbox.is_held(&key("zed", 5)));
        assert_eq!(
            status_title(inbox.find(&key("zed", 5)).unwrap()),
            "Watching"
        );
    }

    #[test]
    fn actions_on_a_held_item_use_the_servers_copy_and_the_row_follows_the_watch_state() {
        let api = item("api", 1, AttentionStatus::NeedsYou, 300);
        let docs = item("docs", 3, AttentionStatus::Done, 100);
        let mut inbox = browsed();
        inbox.update_global(with_sessions(vec![api.clone(), docs.clone()], &["web"]));
        assert!(inbox.is_held(&key("web", 2)));
        assert_eq!(
            inbox.selected_target().map(status_title),
            Some("Done"),
            "with no server copy the held one is all there is"
        );
        let mut watched_web = watched(item("web", 2, AttentionStatus::Idle, 0));
        watched_web.item.attention.ignore_output = true;
        inbox.update_global(global(vec![api.clone(), watched_web, docs.clone()]));
        assert!(inbox.is_held(&key("web", 2)));
        let shown = inbox.find(&key("web", 2)).unwrap().clone();
        assert_eq!(status_title(&shown), "Done", "the held copy is shown");
        assert!(
            shown.item.attention.watched,
            "the row shows the new watch state"
        );
        assert_eq!(Section::of(&shown), Section::Watching);
        assert_eq!(inbox.selected_item().map(ItemKey::of), Some(key("web", 2)));
        let target = inbox.selected_target().unwrap();
        assert_eq!(
            (
                status_title(target),
                target.item.attention.ignore_output,
                target.item.attention.since_ms
            ),
            ("Watching", true, 0),
            "m, n and Enter act on the server's copy"
        );
        assert_eq!(inbox.live_view_target(), Some(key("web", 2)));
        inbox.update_global(with_sessions(vec![api, docs], &["web"]));
        assert!(inbox.is_held(&key("web", 2)));
        assert!(!inbox.find(&key("web", 2)).unwrap().item.attention.watched);
    }

    #[test]
    fn a_held_item_of_another_session_goes_when_its_session_ends_or_its_pane_is_reported_closed() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("web", 2, AttentionStatus::Done, 300),
            item("web", 3, AttentionStatus::Failed, 250),
            item("docs", 4, AttentionStatus::Done, 200),
            item("api", 5, AttentionStatus::Done, 100),
        ]));
        view(
            &mut inbox,
            &[key("web", 2), key("web", 3), key("api", 5), key("docs", 4)],
        );
        let all_seen = with_sessions(vec![], &["web", "docs", "api"]);
        inbox.update_global(all_seen.clone());
        assert_eq!(sessions_in_order(&inbox), vec!["web", "web", "docs", "api"]);
        let mut not_found = live_status(Some(key("docs", 4)), false);
        not_found.state = MirrorState::Closed(MirrorClosedReason::PaneNotFound);
        inbox.apply_live_view_status(not_found);
        assert_eq!(sessions_in_order(&inbox), vec!["web", "web", "api"]);
        assert_ne!(
            inbox.selected_target().map(ItemKey::of),
            Some(key("docs", 4)),
            "Enter never goes to the closed pane"
        );
        render(&mut inbox, 88, 15, false);
        assert_eq!(sessions_in_order(&inbox), vec!["web", "web", "api"]);
        let mut ended = live_status(Some(key("web", 2)), false);
        ended.state = MirrorState::Closed(MirrorClosedReason::SessionEnding);
        inbox.apply_live_view_status(ended);
        assert_eq!(sessions_in_order(&inbox), vec!["api"]);
        let mut api_left = all_seen;
        api_left.sessions.retain(|session| session.name != "api");
        inbox.update_global(api_left);
        assert!(sessions_in_order(&inbox).is_empty());
        assert_eq!(inbox.selected_target(), None);
    }

    #[test]
    fn a_working_item_dismissed_with_c_is_held_again_once_it_is_done_and_seen() {
        let api = item("api", 1, AttentionStatus::NeedsYou, 300);
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            api.clone(),
            item("work", 2, AttentionStatus::Working, 100),
        ]));
        inbox.select_item(key("work", 2));
        render(&mut inbox, 88, 15, false);
        inbox.dismiss_selected();
        render(&mut inbox, 88, 15, false);
        assert!(
            !inbox
                .listed
                .iter()
                .any(|known| ItemKey::of(known) == key("work", 2)),
            "a dismissed item is not remembered while it keeps working"
        );
        inbox.update_global(global(vec![
            api.clone(),
            item("work", 2, AttentionStatus::Done, 500),
        ]));
        render(&mut inbox, 88, 15, false);
        inbox.update_global(with_sessions(vec![api], &["work"]));
        assert!(inbox.is_held(&key("work", 2)));
        assert_eq!(status_title(inbox.find(&key("work", 2)).unwrap()), "Done");
    }

    #[test]
    fn headings_and_the_quiet_line_count_only_what_the_server_still_lists() {
        let mut inbox = Inbox::default();
        inbox.update_global(global(vec![
            item("web", 2, AttentionStatus::Done, 400),
            item("docs", 3, AttentionStatus::Done, 300),
            item("api", 4, AttentionStatus::Done, 200),
            item("zed", 6, AttentionStatus::Done, 150),
            item("build", 5, AttentionStatus::Working, 100),
        ]));
        view(&mut inbox, &[key("docs", 3), key("web", 2)]);
        inbox.update_global(with_sessions(
            vec![
                item("api", 4, AttentionStatus::Done, 200),
                item("zed", 6, AttentionStatus::Done, 150),
                item("build", 5, AttentionStatus::Quiet, 100),
            ],
            &["web", "docs"],
        ));
        assert_eq!(inbox.tab_titles(1).0, "Inbox (2)");
        assert_eq!(sessions_in_order(&inbox), vec!["web", "docs", "api", "zed"]);
        let lines = contents(&inbox.block_lines(60, 1000, false));
        assert!(lines.contains(&"Done (2)".to_owned()), "{:?}", lines);
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("1 more item with only new output")),
            "{:?}",
            lines
        );
        assert_eq!(inbox.quiet_count(), 1);
        inbox.update_global(with_sessions(
            vec![item("build", 5, AttentionStatus::Quiet, 100)],
            &["web", "docs", "api", "zed"],
        ));
        let lines = contents(&inbox.block_lines(60, 1000, false));
        assert!(lines.contains(&"Done".to_owned()), "{:?}", lines);
        assert_eq!(inbox.quiet_count(), 1);
        inbox.toggle_grouping();
        let lines = contents(&inbox.block_lines(60, 1000, false));
        assert!(lines.contains(&"web".to_owned()), "{:?}", lines);
        assert!(!lines.iter().any(|line| line.contains("(")), "{:?}", lines);
    }

    #[test]
    fn alt_enter_is_coloured_as_a_key_in_the_hint_lines_and_the_key_list() {
        let key_range: Vec<usize> = (0..TYPE_KEY.chars().count()).collect();
        let inbox = sample();
        let under_view = inbox.view_hint(TYPE_INTO_IT_HINT, false);
        assert_eq!(level(&under_view, 3), key_range);
        assert!(level(&under_view, 4)
            .iter()
            .all(|index| *index >= TYPE_KEY.chars().count()));
        assert!(!level(&under_view, 4).is_empty(), "the label is dimmed");
        let all_keys = crate::help::all_keys_lines(
            &[
                crate::help::hint("<ENTER>", "go to the pane"),
                crate::help::hint(TYPE_KEY, "type into the shown pane"),
            ],
            80,
        );
        let listed = key_hint_text(&all_keys[1]);
        assert_eq!(level(&listed, 3), key_range);
        let line = "<ENTER> go  <Alt ENTER> type  <c> dismiss  <?> more";
        let footer = crate::help::line_text(&crate::help::BlockLine {
            text: line.to_owned(),
            secondary: false,
        });
        let start = line.find(TYPE_KEY).unwrap();
        assert!((start..start + TYPE_KEY.len()).all(|index| level(&footer, 3).contains(&index)));
        assert!(!level(&footer, 3).contains(&(start + TYPE_KEY.len())));
    }
}

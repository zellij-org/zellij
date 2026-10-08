use std::collections::BTreeMap;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

pub const CARD_MAX_WIDTH: usize = 72;
pub const CARD_PADDING: usize = 1;
pub const CARD_EMPTY_TEXT: &str = "No other sessions yet";
pub const CARD_NO_LIVE_TEXT: &str = "No other live sessions";
pub const MANAGER_SUGGESTION_LIMIT: usize = 500;
pub const PANEL_SIZE_PIPE: &str = "session_panel_size";
pub const SHOW_ALL_TEXT: &str = "Show all and previews ›";
pub const SHOW_ALL_SHORTCUT: &str = "<Ctrl f>";
pub const CARD_TITLE: &str = "Running Sessions";
pub const CARD_SHORTCUT: &str = "<F9>";
pub const CARD_MIN_LIST_ROWS: usize = 5;
pub const CARD_SCROLL_MARGIN: usize = 2;
pub const DONT_SHOW_AGAIN_TEXT: &str = " Don't show again ";
pub const NO_OTHER_SESSIONS_TEXT: &str = "No other sessions.";
pub const EMPTY_PREVIEW_TEXT: &str = "Select a session to see its preview";
pub const EMPTY_PREVIEW_HINT: &str = "You can do this by navigating with ";
pub const NAVIGATE_KEYS: &str = "<←↓↑→>";
pub const PREVIEW_REFRESH_SECONDS: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelSize {
    Card,
    Manager,
}

impl PanelSize {
    pub fn as_str(&self) -> &'static str {
        match self {
            PanelSize::Card => "card",
            PanelSize::Manager => "manager",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardTarget {
    Row(usize),
    ShowAll,
    Close,
    DontShowAgain,
}

#[derive(Clone, Debug, Default)]
pub struct DisplayArea {
    pub columns: usize,
    pub rows: usize,
    pub viewport_columns: usize,
    pub viewport_rows: usize,
}

#[derive(Default)]
pub struct Panel {
    pub size: Option<PanelSize>,
    pub is_popup: bool,
    pub popup_focused: bool,
    pub needs_initial_layout: bool,
    pub suggestions: Option<SessionSuggestions>,
    pub hovered: Option<CardTarget>,
    pub command_submitted: bool,
    pub display: DisplayArea,
    pub card_targets: Vec<(usize, std::ops::Range<usize>, CardTarget)>,
    pub preview: Option<SessionPreview>,
    pub saved_preview: Option<SavedSessionPreview>,
    pub preview_tab: Option<usize>,
    pub preview_session: Option<String>,
    pub preview_timer_armed: bool,
    pub preview_area: Option<(usize, usize, usize, usize)>,
    pub own_plugin_id: Option<u32>,
    pub card_scroll: usize,
    pub card_page_rows: usize,
    pub card_follow_selection: bool,
    pub tree: crate::session_tree::SessionTree,
    pub preview_pane: Option<usize>,
    pub preview_pane_id: Option<(u32, bool)>,
    pub opened_at_startup: bool,
}

impl Panel {
    pub fn is_open(&self) -> bool {
        self.size.is_some()
    }

    fn card_candidates(&self) -> impl Iterator<Item = &SessionSuggestion> {
        self.suggestions
            .iter()
            .flat_map(|s| s.suggestions.iter())
    }

    pub fn card_suggestions(&self) -> Vec<SessionSuggestion> {
        self.card_candidates()
            .filter(|s| s.is_running)
            .cloned()
            .collect()
    }

    pub fn move_card_selection(&mut self, delta: isize) {
        self.card_follow_selection = true;
        let count = self.card_suggestions().len();
        if count == 0 {
            self.hovered = Some(CardTarget::ShowAll);
            return;
        }
        let current = match self.hovered {
            Some(CardTarget::Row(index)) => Some(index.min(count - 1) as isize),
            Some(CardTarget::ShowAll) => Some(count as isize),
            _ => None,
        };
        let next = match current {
            Some(current) => (current + delta).clamp(0, count as isize - 1) as usize,
            None if delta < 0 => count - 1,
            None => 0,
        };
        self.hovered = Some(CardTarget::Row(next));
    }

    pub fn select_card_edge(&mut self, last: bool) {
        self.card_follow_selection = true;
        let count = self.card_suggestions().len();
        self.hovered = match (count, last) {
            (0, _) => Some(CardTarget::ShowAll),
            (_, false) => Some(CardTarget::Row(0)),
            (count, true) => Some(CardTarget::Row(count - 1)),
        };
    }

    pub fn card_resumable_count(&self) -> usize {
        self.card_candidates().filter(|s| !s.is_running).count()
    }

    pub fn card_empty_text(&self) -> &'static str {
        if self.card_resumable_count() > 0 {
            CARD_NO_LIVE_TEXT
        } else {
            CARD_EMPTY_TEXT
        }
    }

    pub fn resumable_text(&self) -> Option<String> {
        match self.card_resumable_count() {
            0 => None,
            1 => Some("+1 resurrectable session".to_owned()),
            count => Some(format!("+{} resurrectable sessions", count)),
        }
    }

    pub fn context_branch(&self) -> Option<&str> {
        self.suggestions.as_ref().and_then(|s| s.branch.as_deref())
    }

    pub fn suggestion_limit(&self) -> usize {
        MANAGER_SUGGESTION_LIMIT
    }

    pub fn request_suggestions(&self) {
        get_session_suggestions(self.suggestion_limit());
    }

    pub fn report_size(&self) {
        let payload = self.size.map(|s| s.as_str()).unwrap_or("closed");
        let mut args = BTreeMap::new();
        if let Some(plugin_id) = self.own_plugin_id {
            args.insert("plugin_id".to_owned(), plugin_id.to_string());
        }
        pipe_message_to_plugin(
            MessageToPlugin::new(PANEL_SIZE_PIPE)
                .with_payload(payload)
                .with_args(args),
        );
    }

    pub fn manager_coordinates(&self) -> FloatingPaneCoordinates {
        let columns = self.display.columns.max(20);
        let rows = self.display.viewport_rows.max(self.display.rows).max(10);
        let width = (columns * 3 / 4).max(40).min(columns);
        let height = (rows * 3 / 4).max(10).min(rows);
        let x = columns.saturating_sub(width) / 2;
        let y = rows.saturating_sub(height) / 2;
        FloatingPaneCoordinates::default()
            .with_x_fixed(x)
            .with_y_fixed(y)
            .with_width_fixed(width)
            .with_height_fixed(height)
    }

    pub fn card_coordinates(&self, width: usize, height: usize) -> FloatingPaneCoordinates {
        let columns = self.display.columns.max(width);
        FloatingPaneCoordinates::default()
            .with_x_fixed(columns.saturating_sub(width))
            .with_y_fixed(1)
            .with_width_fixed(width)
            .with_height_fixed(height)
    }

    pub fn apply_size(&mut self, size: PanelSize) {
        let own_pane = self.own_plugin_id.map(PaneId::Plugin);
        match size {
            PanelSize::Card => {
                if !self.is_popup {
                    if let Some(own_pane) = own_pane {
                        let (width, height) = self.card_dimensions();
                        change_floating_panes_coordinates(vec![(
                            own_pane,
                            self.card_coordinates(width, height),
                        )]);
                    }
                }
            },
            PanelSize::Manager => {
                let coordinates = self.manager_coordinates();
                if self.is_popup {
                    popup_to_floating_pane(Some(coordinates.clone()));
                    self.is_popup = false;
                }
                if let Some(own_pane) = own_pane {
                    change_floating_panes_coordinates(vec![(own_pane, coordinates)]);
                }
            },
        }
        let previous = self.size;
        self.size = Some(size);
        if previous != Some(size) {
            self.report_size();
            if previous.map(|p| p == PanelSize::Card).unwrap_or(true) || size == PanelSize::Card {
                self.request_suggestions();
            }
        }
        if size == PanelSize::Card {
            self.clear_preview();
        }
    }

    pub fn close(&mut self) {
        self.size = None;
        self.report_size();
        close_self();
    }

    pub fn clear_preview(&mut self) {
        self.preview = None;
        self.saved_preview = None;
        self.preview_session = None;
        self.preview_tab = None;
        self.preview_pane = None;
        self.preview_pane_id = None;
    }

    pub fn card_dimensions(&self) -> (usize, usize) {
        let rows = self.card_suggestions();
        let table = build_card_table(
            &rows,
            self.suggestions
                .as_ref()
                .map(|s| s.columns.clone())
                .unwrap_or_default(),
            CARD_MAX_WIDTH.saturating_sub(2 + 2 * CARD_PADDING),
            self.context_branch(),
        );
        let footer_width = match self.resumable_text() {
            Some(text) => text.width() + 2 + self.show_all_text().width(),
            None => self.show_all_text().width(),
        };
        let content_width = table
            .width
            .max(self.card_empty_text().width())
            .max(footer_width + 2)
            .max(CARD_TITLE.width() + CARD_SHORTCUT.width() + 3 + " × ".width());
        let width = (content_width + 2 + 2 * CARD_PADDING).min(CARD_MAX_WIDTH);
        let header_rows = if table.grid.is_some() { 1 } else { 0 };
        let list_rows = rows.len().min(self.card_list_row_limit()).max(1);
        let mut height = list_rows + header_rows + 4 + 2 * CARD_PADDING;
        let screen_rows = self.display.viewport_rows.max(self.display.rows);
        if screen_rows > 0 {
            height = height.min(screen_rows.saturating_sub(1).max(8));
        }
        (width, height)
    }

    pub fn show_all_text(&self) -> String {
        if self.popup_focused {
            format!("{} {}", SHOW_ALL_SHORTCUT, SHOW_ALL_TEXT)
        } else {
            SHOW_ALL_TEXT.to_owned()
        }
    }

    pub fn card_list_row_limit(&self) -> usize {
        let screen_rows = self.display.viewport_rows.max(self.display.rows);
        (screen_rows / 8).max(CARD_MIN_LIST_ROWS)
    }

    pub fn request_tree_preview(
        &mut self,
        name: &str,
        is_running: bool,
        is_current: bool,
        tab: Option<usize>,
        pane: Option<usize>,
    ) {
        let session_changed = self.preview_session.as_deref() != Some(name);
        let tab_changed = self.preview_tab != tab;
        let pane_id = match (tab, pane) {
            (Some(tab), Some(pane)) => self
                .tree
                .children
                .get(name)
                .and_then(|tabs| tabs.get(tab))
                .and_then(|t| t.panes.get(pane))
                .map(|p| (p.id, p.is_plugin)),
            _ => None,
        };
        let pane_changed = self.preview_pane_id != pane_id;
        self.preview_pane_id = pane_id;
        if session_changed {
            self.preview = None;
            self.saved_preview = None;
        }
        self.preview_session = Some(name.to_owned());
        self.preview_tab = tab;
        self.preview_pane = pane;
        if is_current {
            return;
        }
        if is_running {
            if session_changed || tab_changed || pane_changed || self.preview.is_none() {
                get_session_preview(name, tab, pane_id);
            }
        } else if session_changed || self.saved_preview.is_none() {
            get_saved_session_preview(name);
        }
    }

    pub fn arm_preview_timer(&mut self) {
        if !self.preview_timer_armed {
            set_timeout(PREVIEW_REFRESH_SECONDS);
            self.preview_timer_armed = true;
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CardTable {
    pub grid: Option<Vec<Vec<Text>>>,
    pub width: usize,
}

pub fn format_age(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}

fn age_text(suggestion: &SessionSuggestion) -> String {
    format!("{} ago", format_age(suggestion.last_used_secs_ago))
}

pub fn shorten_path(path: &str, max_width: usize) -> String {
    let trimmed = path.trim_end_matches('/');
    let home = std::env::var("HOME").ok();
    let display = match home {
        Some(home) if !home.is_empty() && trimmed.starts_with(&home) => {
            format!("~{}", &trimmed[home.len()..])
        },
        _ => trimmed.to_owned(),
    };
    if display.width() <= max_width {
        return display;
    }
    let parts: Vec<&str> = display.split('/').collect();
    let mut shortened: Vec<String> = parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            if index + 1 == parts.len() || part.is_empty() || *part == "~" {
                part.to_string()
            } else {
                part.chars().next().map(|c| c.to_string()).unwrap_or_default()
            }
        })
        .collect();
    let mut candidate = shortened.join("/");
    while candidate.width() > max_width && shortened.len() > 1 {
        shortened.remove(0);
        candidate = format!("…/{}", shortened.join("/"));
    }
    if candidate.width() > max_width {
        let keep = max_width.saturating_sub(1);
        let tail: String = candidate
            .chars()
            .rev()
            .take(keep)
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        candidate = format!("…{}", tail);
    }
    candidate
}

fn folder_name(folder: &str) -> String {
    let trimmed = folder.trim_end_matches('/');
    match trimmed.rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => folder.to_owned(),
    }
}

fn folder_cell(suggestion: &SessionSuggestion, max_width: usize) -> String {
    let Some(folder) = suggestion.folder.as_deref() else {
        return "—".to_owned();
    };
    match suggestion.folder_relation {
        FolderRelation::Here => folder_name(folder),
        _ => shorten_path(folder, max_width),
    }
}

pub fn is_full_match(suggestion: &SessionSuggestion, context_branch: Option<&str>) -> bool {
    suggestion.folder_relation == FolderRelation::Here
        && context_branch.is_some()
        && suggestion.branch.as_deref() == context_branch
}

struct CardColumn {
    fact: String,
    title: &'static str,
    is_custom: bool,
}

fn card_columns(columns: &[SessionSuggestionColumn]) -> Vec<CardColumn> {
    let mut result = vec![CardColumn {
        fact: "directory".to_owned(),
        title: "Folder",
        is_custom: false,
    }];
    result.push(CardColumn {
        fact: "git_branch".to_owned(),
        title: "Branch",
        is_custom: false,
    });
    for column in columns {
        let is_builtin = matches!(
            column.fact.as_str(),
            "directory" | "git_branch" | "git_repo" | "git_worktree" | "hostname"
        );
        if !is_builtin {
            result.push(CardColumn {
                fact: column.fact.clone(),
                title: "",
                is_custom: true,
            });
        }
    }
    result
}

fn pad(text: &str, width: usize) -> String {
    let current = text.width();
    if current >= width {
        text.to_owned()
    } else {
        format!("{}{}", text, " ".repeat(width - current))
    }
}

pub fn build_card_table(
    suggestions: &[SessionSuggestion],
    columns: Vec<SessionSuggestionColumn>,
    max_width: usize,
    context_branch: Option<&str>,
) -> CardTable {
    let mut columns = card_columns(&columns);
    let mut path_width = 24;
    loop {
        let table = layout_card_grid(suggestions, &columns, path_width, context_branch);
        if table.width <= max_width {
            return table;
        }
        if path_width > 8 {
            path_width -= 4;
            continue;
        }
        match columns.iter().rposition(|c| c.is_custom) {
            Some(index) => {
                columns.remove(index);
            },
            None => return table,
        }
    }
}

fn layout_card_grid(
    suggestions: &[SessionSuggestion],
    columns: &[CardColumn],
    path_width: usize,
    context_branch: Option<&str>,
) -> CardTable {
    if suggestions.is_empty() {
        return CardTable::default();
    }
    let has_labels = suggestions.iter().any(|s| s.script_label.is_some());
    let has_resumable = suggestions.iter().any(|s| !s.is_running);
    let mut header: Vec<Text> = vec![Text::from(" ")];
    for column in columns {
        header.push(
            Text::from(if column.title.is_empty() {
                " "
            } else {
                column.title
            })
            .dim_all(),
        );
    }
    header.push(Text::from(" "));
    if has_labels {
        header.push(Text::from(" "));
    }
    if has_resumable {
        header.push(Text::from(" "));
    }
    let mut grid = vec![header];
    for s in suggestions {
        let mut row: Vec<Text> = vec![];
        let style = |text: Text, level: Option<usize>| -> Text {
            match level {
                Some(level) => text.color_range(level, ..),
                None if !s.is_running => text.dim_all(),
                None => text,
            }
        };
        let name_level = if is_full_match(s, context_branch) {
            Some(2)
        } else {
            None
        };
        row.push(style(Text::from(s.name.as_str()), name_level));
        for column in columns {
            let cell = match column.fact.as_str() {
                "directory" => folder_cell(s, path_width),
                "git_branch" => s.branch.clone().unwrap_or_else(|| "—".to_owned()),
                fact => s.fact_value(fact).unwrap_or_else(|| "—".to_owned()),
            };
            let matches = s.column_matches(&column.fact);
            let level = match column.fact.as_str() {
                "directory" if matches => Some(1),
                "git_branch" if matches => Some(0),
                _ if column.is_custom && matches => Some(3),
                _ if column.is_custom && s.is_running => Some(0),
                _ => None,
            };
            row.push(style(Text::from(cell), level));
        }
        row.push(Text::from(age_text(s)).dim_all());
        if has_labels {
            let label = s.script_label.clone().unwrap_or_else(|| " ".to_owned());
            row.push(style(Text::from(label), None));
        }
        if has_resumable {
            let resume = if s.is_running { " " } else { "resume" };
            row.push(Text::from(resume).dim_all());
        }
        grid.push(row);
    }
    let column_count = grid[0].len();
    let width = (0..column_count)
        .map(|i| grid.iter().map(|r| r[i].content().width()).max().unwrap_or(0))
        .sum::<usize>()
        + column_count;
    CardTable {
        grid: Some(grid),
        width,
    }
}

pub fn render_card(panel: &mut Panel, rows: usize, cols: usize) {
    let suggestions = panel.card_suggestions();
    let width = cols;
    let inner = width.saturating_sub(2);
    let content_x = 1 + CARD_PADDING;
    let content_width = width.saturating_sub(2 + 2 * CARD_PADDING);
    let table = build_card_table(
        &suggestions,
        panel
            .suggestions
            .as_ref()
            .map(|s| s.columns.clone())
            .unwrap_or_default(),
        content_width,
        panel.context_branch(),
    );
    panel.card_targets.clear();
    let title_main = CARD_TITLE;
    let title = format!(" {} {} ", title_main, CARD_SHORTCUT);
    let title_width = title.width();
    let close_x = width.saturating_sub(4);
    let top = format!(
        "┌{}{} × ─┐",
        title,
        "─".repeat(inner.saturating_sub(title_width + 4))
    );
    let shortcut_start = 2 + title_main.chars().count() + 1;
    let top_text = Text::from(top)
        .color_range(2, 2..2 + title_main.chars().count())
        .color_range(0, shortcut_start..shortcut_start + CARD_SHORTCUT.chars().count());
    let close_index = top_text.content().chars().count().saturating_sub(4);
    let top_text = top_text.color_range(3, close_index..close_index + 1);
    print_text_with_coordinates(top_text, 0, 0, Some(width), None);
    if panel.hovered == Some(CardTarget::Close) {
        print_text_with_coordinates(
            Text::from("×").color_range(3, ..).italic_all(),
            close_x,
            0,
            Some(1),
            None,
        );
    }
    panel
        .card_targets
        .push((0, close_x.saturating_sub(1)..close_x + 2, CardTarget::Close));
    let first_row_y = 1 + CARD_PADDING;
    let last_content_y = rows.saturating_sub(3 + CARD_PADDING);
    let mut y = first_row_y;
    if table.grid.is_none() {
        print_text_with_coordinates(
            Text::from(panel.card_empty_text()).dim_all(),
            content_x,
            y,
            Some(content_width),
            None,
        );
        y += 1;
    }
    let row_count = suggestions.len();
    let header_rows = if table.grid.is_some() { 1 } else { 0 };
    let capacity = last_content_y
        .saturating_sub(y)
        .saturating_sub(header_rows)
        .min(panel.card_list_row_limit());
    panel.card_page_rows = capacity.max(1);
    let mut scroll = panel.card_scroll.min(row_count.saturating_sub(capacity));
    if std::mem::take(&mut panel.card_follow_selection) {
        if let Some(CardTarget::Row(index)) = panel.hovered {
            scroll = scroll_with_margin(scroll, index, capacity, row_count, CARD_SCROLL_MARGIN);
        }
    }
    panel.card_scroll = scroll;
    let visible = row_count.saturating_sub(scroll).min(capacity);
    let header_y = y;
    if let Some(grid) = table.grid.as_ref() {
        if y < last_content_y {
            let mut table = Table::new().add_styled_row(grid[0].clone());
            for index in scroll..scroll + visible {
                let hovered = panel.hovered == Some(CardTarget::Row(index));
                let cells = grid[index + 1]
                    .iter()
                    .map(|cell| if hovered { cell.clone().selected() } else { cell.clone() })
                    .collect();
                table = table.add_styled_row(cells);
            }
            print_table_with_coordinates(table, content_x, y, Some(content_width), Some(visible + 1));
            for offset in 0..visible {
                panel.card_targets.push((
                    y + 1 + offset,
                    1..width.saturating_sub(1),
                    CardTarget::Row(scroll + offset),
                ));
            }
            y += visible + 1;
        }
    }
    let more_above = scroll;
    let more_below = row_count.saturating_sub(scroll + visible);
    if more_above > 0 && header_rows > 0 {
        let text = format!("↑ [+{}]", more_above);
        print_text_with_coordinates(
            Text::from(text.as_str()).color_range(1, ..),
            (content_x + content_width).saturating_sub(text.width()),
            header_y,
            None,
            None,
        );
    }
    if more_below > 0 {
        let text = format!("↓ [+{}]", more_below);
        print_text_with_coordinates(
            Text::from(text.as_str()).color_range(1, ..),
            (content_x + content_width).saturating_sub(text.width()),
            y,
            None,
            None,
        );
    }
    let show_all_y = y + 1;
    if let Some(resumable) = panel.resumable_text() {
        let resumable_width = resumable.width().min(content_width);
        print_text_with_coordinates(
            Text::from(resumable.as_str()).dim_all(),
            content_x,
            show_all_y,
            Some(resumable_width),
            None,
        );
    }
    let show_all_text = panel.show_all_text();
    let show_all_width = show_all_text.width().min(content_width);
    let show_all_x = (content_x + content_width).saturating_sub(show_all_width);
    let shortcut_len = if panel.popup_focused {
        SHOW_ALL_SHORTCUT.chars().count() + 1
    } else {
        0
    };
    let mut show_all = Text::from(show_all_text.as_str()).color_range(3, shortcut_len..);
    if shortcut_len > 0 {
        show_all = show_all.color_range(0, ..shortcut_len - 1);
    }
    if panel.hovered == Some(CardTarget::ShowAll) {
        show_all = show_all.italic_all();
    }
    print_text_with_coordinates(show_all, show_all_x, show_all_y, Some(show_all_width), None);
    panel.card_targets.push((
        show_all_y,
        show_all_x..show_all_x + show_all_width,
        CardTarget::ShowAll,
    ));
    let bottom_y = show_all_y + CARD_PADDING + 1;
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
    print_text_with_coordinates(Text::from(bottom), 0, bottom_y, Some(width), None);
    if panel.opened_at_startup && inner > DONT_SHOW_AGAIN_TEXT.width() + 2 {
        let text_width = DONT_SHOW_AGAIN_TEXT.width();
        let x = width.saturating_sub(text_width + 2);
        let mut text = Text::from(DONT_SHOW_AGAIN_TEXT).dim_all();
        if panel.hovered == Some(CardTarget::DontShowAgain) {
            text = text.italic_all();
        }
        print_text_with_coordinates(text, x, bottom_y, Some(text_width), None);
        panel
            .card_targets
            .push((bottom_y, x + 1..x + text_width - 1, CardTarget::DontShowAgain));
    }
}

fn scroll_with_margin(
    offset: usize,
    index: usize,
    rows: usize,
    total: usize,
    margin: usize,
) -> usize {
    if rows == 0 {
        return offset;
    }
    let margin = margin.min(rows.saturating_sub(1) / 2);
    let mut offset = offset;
    if index < offset + margin {
        offset = index.saturating_sub(margin);
    } else if index + 1 + margin > offset + rows {
        offset = (index + 1 + margin).saturating_sub(rows);
    }
    offset.min(total.saturating_sub(rows))
}

pub fn card_target_at(panel: &Panel, line: isize, column: usize) -> Option<CardTarget> {
    if line < 0 {
        return None;
    }
    let line = line as usize;
    panel
        .card_targets
        .iter()
        .find(|(y, range, _)| *y == line && range.contains(&column))
        .map(|(_, _, target)| target.clone())
}

pub fn render_session_tree(
    panel: &mut Panel,
    cache: &crate::ui::components::UnifiedResultsRenderCache,
    selected_index: Option<usize>,
    max_rows: usize,
    max_cols: usize,
    x: usize,
    y: usize,
) {
    use crate::session_tree::TreeNode;
    use crate::ui::components::CachedRowKind;
    panel.tree.row_targets.clear();
    panel.tree.marker_targets.clear();
    let sessions: Vec<(usize, String)> = cache
        .rows
        .iter()
        .map(|r| (r.original_index, r.session_name.clone()))
        .collect();
    let rows = panel.tree.flatten(&sessions);
    if rows.is_empty() || max_rows < 2 {
        panel.tree.rows = rows;
        return;
    }
    let cached: BTreeMap<usize, &crate::ui::components::CachedRowData> =
        cache.rows.iter().map(|r| (r.original_index, r)).collect();
    let suggestions: BTreeMap<&str, &SessionSuggestion> = panel
        .suggestions
        .as_ref()
        .map(|s| s.suggestions.iter().map(|s| (s.name.as_str(), s)).collect())
        .unwrap_or_default();
    let cursor = panel.tree.cursor_position(&rows, selected_index);
    let capacity = max_rows.saturating_sub(2).max(1);
    panel.tree.page_rows = capacity;
    let mut offset = panel.tree.offset.min(rows.len().saturating_sub(capacity));
    if std::mem::take(&mut panel.tree.follow) {
        if let Some(cursor) = cursor {
            offset = scroll_with_margin(offset, cursor, capacity, rows.len(), CARD_SCROLL_MARGIN);
        }
    }
    panel.tree.offset = offset;
    let visible = rows.len().saturating_sub(offset).min(capacity);
    let session_name_text = |row: &crate::ui::components::CachedRowData| match row.kind {
        CachedRowKind::Resurrectable => format!("{} [EXITED]", row.session_name),
        CachedRowKind::Active => row.session_name.clone(),
    };
    let folder_text = |name: &str| {
        suggestions
            .get(name)
            .map(|s| folder_cell(s, 24))
            .unwrap_or_else(|| "—".to_owned())
    };
    let branch_text = |name: &str| {
        suggestions
            .get(name)
            .and_then(|s| s.branch.clone())
            .unwrap_or_else(|| "—".to_owned())
    };
    let name_width = cache
        .rows
        .iter()
        .map(|r| session_name_text(r).width())
        .max()
        .unwrap_or(0);
    let folder_width = cache
        .rows
        .iter()
        .map(|r| folder_text(&r.session_name).width())
        .max()
        .unwrap_or(0)
        .max("Folder".width());
    let branch_width = cache
        .rows
        .iter()
        .map(|r| branch_text(&r.session_name).width())
        .max()
        .unwrap_or(0)
        .max("Branch".width());
    let total_session_width = name_width + 2 + folder_width + 2 + branch_width;
    let abbreviate = total_session_width + 2 + 24 + 3 > max_cols;
    let header_x = x + 3 + name_width + 2;

    print_text_with_coordinates(Text::from("Folder").dim_all(), header_x, y, None, None);
    print_text_with_coordinates(
        Text::from("Branch").dim_all(),
        header_x + folder_width + 2,
        y,
        None,
        None,
    );
    let mut items = vec![];
    for (offset_index, row) in rows[offset..offset + visible].iter().enumerate() {
        let mut text = match row.node {
            TreeNode::Session => {
                let Some(cached_row) = cached.get(&row.original_index) else {
                    continue;
                };
                let name = session_name_text(cached_row);
                let folder = folder_text(&row.session);
                let branch = branch_text(&row.session);
                let (details, ranges) = if abbreviate {
                    (cached_row.abbr_details.clone(), &cached_row.abbr_details_color_ranges)
                } else {
                    (cached_row.full_details.clone(), &cached_row.details_color_ranges)
                };
                let line = format!(
                    "{}  {}  {}  {}",
                    pad(&name, name_width),
                    pad(&folder, folder_width),
                    pad(&branch, branch_width),
                    details
                );
                let name_len = cached_row.session_name.chars().count();
                let mut text = Text::from(line).color_range(1, ..name_len);
                let indices: Vec<usize> = cached_row
                    .indices
                    .iter()
                    .filter(|&&i| i < name_len)
                    .cloned()
                    .collect();
                if !indices.is_empty() {
                    text = text.color_indices(3, indices);
                }
                if matches!(cached_row.kind, CachedRowKind::Resurrectable) {
                    let start = name_len + 2;
                    text = text.error_color_range(start..start + "EXITED".len());
                }
                let folder_start = name_width + 2;
                let suggestion = suggestions.get(row.session.as_str());
                if suggestion.map(|s| s.column_matches("directory")).unwrap_or(false) {
                    text = text.color_range(1, folder_start..folder_start + folder.chars().count());
                }
                let branch_start = folder_start + folder_width + 2;
                if suggestion.map(|s| s.column_matches("git_branch")).unwrap_or(false) {
                    text = text.color_range(0, branch_start..branch_start + branch.chars().count());
                }
                let details_start = branch_start + branch_width + 2;
                for (level, range) in &ranges.ranges {
                    text = text.color_range(*level, details_start + range.start..details_start + range.end);
                }
                text
            },
            TreeNode::Tab(tab_index) => {
                let tab = panel
                    .tree
                    .children
                    .get(&row.session)
                    .and_then(|tabs| tabs.get(tab_index));
                let name = tab.map(|t| t.name.clone()).unwrap_or_default();
                let pane_count = tab.map(|t| t.panes.len()).unwrap_or(0);
                let suffix = format!(
                    " ({} {})",
                    pane_count,
                    if pane_count == 1 { "pane" } else { "panes" }
                );
                let active = tab.map(|t| t.active).unwrap_or(false);
                let line = format!("{}{}{}", name, suffix, if active { " active" } else { "" });
                let name_end = name.chars().count();
                Text::from(line)
                    .color_range(2, ..name_end)
                    .dim_range(name_end..)
            },
            TreeNode::Pane(tab_index, pane_index) => {
                let pane = panel
                    .tree
                    .children
                    .get(&row.session)
                    .and_then(|tabs| tabs.get(tab_index))
                    .and_then(|tab| tab.panes.get(pane_index));
                let title = pane.map(|p| p.title.clone()).unwrap_or_default();
                let focused = pane.map(|p| p.focused).unwrap_or(false);
                let line = format!("{}{}", title, if focused { " focused" } else { "" });
                let title_end = title.chars().count();
                Text::from(line).dim_range(title_end..)
            },
        };
        let flat_index = offset + offset_index;
        if cursor == Some(flat_index) {
            text = text.selected();
        }
        items.push(NestedListItem::new(text).indent(row.depth));
        let row_y = y + 1 + offset_index;
        panel
            .tree
            .row_targets
            .push((row_y, x..x + max_cols, flat_index));
        if row.expandable {
            panel
                .tree
                .marker_targets
                .push((row_y, x + row.depth * 2 + 1, flat_index));
        }
    }
    print_nested_list_with_coordinates(items, x, y + 1, Some(max_cols), Some(visible));
    if offset > 0 {
        let text = format!("↑ [+{}]", offset);
        print_text_with_coordinates(
            Text::from(text.as_str()).color_range(1, ..),
            (x + max_cols).saturating_sub(text.width()),
            y,
            None,
            None,
        );
    }
    let below = rows.len().saturating_sub(offset + visible);
    if below > 0 {
        let text = format!("↓ [+{}]", below);
        print_text_with_coordinates(
            Text::from(text.as_str()).color_range(1, ..),
            (x + max_cols).saturating_sub(text.width()),
            y + 1 + visible,
            None,
            None,
        );
    }
    panel.tree.rows = rows;
}

pub fn render_preview(panel: &mut Panel, x: usize, y: usize, width: usize, height: usize) {
    panel.preview_area = Some((x, y, width, height));
    if width < 10 || height < 3 {
        return;
    }
    if panel.preview_session.is_none() {
        render_empty_preview(x, y, width, height);
        return;
    }
    let session_name = panel.preview_session.clone().unwrap_or_default();
    let mut title = format!(" {}", session_name);
    let tab = panel
        .preview_tab
        .and_then(|tab| panel.tree.children.get(&session_name).and_then(|tabs| tabs.get(tab)));
    if let Some(tab) = tab {
        title.push_str(&format!(" › {}", tab.name));
        if let Some(pane) = panel.preview_pane.and_then(|pane| tab.panes.get(pane)) {
            title.push_str(&format!(" › {}", pane.title));
        }
    }
    title.push(' ');
    let inner_width = width.saturating_sub(2);
    let title = truncate_plain(&title, inner_width.saturating_sub(1));
    let filler = inner_width.saturating_sub(title.width());
    let top = format!("┌{}{}┐", title, "─".repeat(filler));
    let top_text = Text::from(top).color_range(2, 1..1 + title.chars().count());
    print_text_with_coordinates(top_text, x, y, Some(width), None);
    let mut body: Vec<String> = vec![];
    let mut details: Vec<Text> = vec![];
    let mut summary: Option<String> = None;
    if let Some(preview) = panel.preview.as_ref() {
        if let Some(error) = &preview.error {
            body.push(format!("Preview unavailable: {}", error));
        } else {
            body.extend(preview.contents.lines().map(|l| l.to_owned()));
        }
        if preview.error.is_none() {
            summary = Some(format!(
                "Tabs: {}  Panes: {}  Clients attached: {}",
                preview.tab_names.len(),
                preview.pane_count,
                preview.connected_clients
            ));
        }
    } else if let Some(saved) = panel.saved_preview.as_ref() {
        if let Some(error) = &saved.error {
            body.push(format!("Preview unavailable: {}", error));
        } else {
            let tab_index = panel
                .preview_tab
                .or_else(|| saved.tabs.iter().position(|t| t.focused))
                .unwrap_or(0);
            if let Some(tab) = saved.tabs.get(tab_index) {
                let pane = panel
                    .preview_pane
                    .and_then(|index| tab.panes.get(index))
                    .or_else(|| tab.panes.iter().find(|p| p.focused))
                    .or_else(|| tab.panes.first());
                if let Some(contents) = pane.and_then(|p| p.contents.as_ref()) {
                    body.extend(contents.lines().map(|l| l.to_owned()));
                } else if let Some(command) = pane.and_then(|p| p.command.as_ref()) {
                    body.push(format!("$ {}", command));
                }
            }
            let pane_count: usize = saved.tabs.iter().map(|t| t.panes.len()).sum();
            summary = Some(format!(
                "Folder: {}  Tabs: {}  Panes: {}",
                saved.folder.as_deref().unwrap_or("—"),
                saved.tabs.len(),
                pane_count
            ));
            let commands: Vec<String> = saved
                .tabs
                .iter()
                .flat_map(|t| t.panes.iter().filter_map(|p| p.command.clone()))
                .collect();
            if !commands.is_empty() {
                details.push(Text::from("Commands that will run again:").color_range(2, ..));
                for command in commands {
                    details.push(Text::from(format!("  {}", command)).color_range(0, ..));
                }
            }
        }
    } else {
        body.push(EMPTY_PREVIEW_TEXT.to_owned());
    }
    let max_details = height.saturating_sub(2) / 3;
    let details_height = details.len().min(max_details);
    let body_height = height.saturating_sub(2 + details_height);
    for row in 0..body_height {
        let line = body.get(row).map(|l| l.as_str()).unwrap_or("");
        print!(
            "\u{1b}[{};{}H\u{1b}[m│{}│",
            y + row + 2,
            x + 1,
            fit_ansi(line, inner_width),
        );
    }
    for (index, detail) in details.into_iter().take(details_height).enumerate() {
        let row_y = y + 1 + body_height + index;
        print!(
            "\u{1b}[{};{}H\u{1b}[m│{}│",
            row_y + 1,
            x + 1,
            " ".repeat(inner_width)
        );
        print_text_with_coordinates(detail, x + 1, row_y, Some(inner_width), None);
    }
    let bottom = match summary {
        Some(summary) if inner_width > 4 => {
            let summary = truncate_plain(&format!(" {} ", summary), inner_width.saturating_sub(1));
            let filler = inner_width.saturating_sub(1 + summary.width());
            let summary_len = summary.chars().count();
            Text::from(format!("└─{}{}┘", summary, "─".repeat(filler)))
                .dim_range(2..2 + summary_len)
        },
        _ => Text::from(format!("└{}┘", "─".repeat(inner_width))),
    };
    print_text_with_coordinates(bottom, x, y + height.saturating_sub(1), Some(width), None);
}

fn render_empty_preview(x: usize, y: usize, width: usize, height: usize) {
    let inner_width = width.saturating_sub(2);
    print_text_with_coordinates(
        Text::from(format!("┌{}┐", "─".repeat(inner_width))),
        x,
        y,
        Some(width),
        None,
    );
    for row in 1..height.saturating_sub(1) {
        print!(
            "\u{1b}[{};{}H\u{1b}[m│{}│",
            y + row + 1,
            x + 1,
            " ".repeat(inner_width)
        );
    }
    print_text_with_coordinates(
        Text::from(format!("└{}┘", "─".repeat(inner_width))),
        x,
        y + height.saturating_sub(1),
        Some(width),
        None,
    );
    let message = EMPTY_PREVIEW_TEXT;
    let message_width = message.width().min(inner_width);
    let message_x = x + 1 + inner_width.saturating_sub(message_width) / 2;
    let message_y = y + height.saturating_sub(1) / 2;
    print_text_with_coordinates(
        Text::from(message),
        message_x,
        message_y,
        Some(message_width),
        None,
    );
    let hint = format!("{}{}", EMPTY_PREVIEW_HINT, NAVIGATE_KEYS);
    let hint_width = hint.width().min(inner_width);
    let hint_x = x + 1 + inner_width.saturating_sub(hint_width) / 2;
    let keys_start = EMPTY_PREVIEW_HINT.chars().count();
    print_text_with_coordinates(
        Text::from(hint).color_range(3, keys_start..),
        hint_x,
        message_y + 1,
        Some(hint_width),
        None,
    );
}

fn fit_ansi(line: &str, width: usize) -> String {
    let mut result = String::new();
    let mut visible = 0;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    let mut sequence = String::from("\u{1b}[");
                    while let Some(c) = chars.next() {
                        sequence.push(c);
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                    if sequence.ends_with('m') {
                        result.push_str(&sequence);
                    }
                },
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' {
                            break;
                        }
                        if c == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                },
                _ => {},
            }
            continue;
        }
        if c == '\t' {
            let spaces = (8 - visible % 8).min(width.saturating_sub(visible));
            result.push_str(&" ".repeat(spaces));
            visible += spaces;
            if visible >= width {
                break;
            }
            continue;
        }
        if c.is_control() {
            continue;
        }
        let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if visible + char_width > width {
            break;
        }
        visible += char_width;
        result.push(c);
    }
    result.push_str("\u{1b}[m");
    result.push_str(&" ".repeat(width.saturating_sub(visible)));
    result
}

#[cfg(test)]
pub fn strip_ansi(line: &str) -> String {
    let mut result = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    while let Some(c) = chars.next() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                },
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' {
                            break;
                        }
                        if c == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                },
                _ => {},
            }
            continue;
        }
        result.push(c);
    }
    result
}

fn truncate_plain(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_owned();
    }
    let mut result = String::new();
    for c in text.chars() {
        let width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if result.width() + width + 1 > max_width {
            break;
        }
        result.push(c);
    }
    result.push('…');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggestion(name: &str, running: bool, relation: FolderRelation) -> SessionSuggestion {
        SessionSuggestion {
            name: name.to_owned(),
            is_running: running,
            last_used_secs_ago: 120,
            folder: Some("/home/someone/projects/zellij/".to_owned()),
            folder_relation: relation,
            branch: Some("main".to_owned()),
            matching_columns: vec!["directory".to_owned()],
            ..Default::default()
        }
    }

    fn row_text(table: &CardTable, row: usize) -> Vec<String> {
        table.grid.as_ref().unwrap()[row]
            .iter()
            .map(|cell| cell.content().to_owned())
            .collect()
    }

    fn panel_with(suggestions: Vec<SessionSuggestion>) -> Panel {
        Panel {
            size: Some(PanelSize::Card),
            suggestions: Some(SessionSuggestions {
                branch: Some("main".to_owned()),
                suggestions,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn ages_are_short() {
        assert_eq!(format_age(5), "5s");
        assert_eq!(format_age(120), "2m");
        assert_eq!(format_age(7200), "2h");
        assert_eq!(format_age(172800), "2d");
    }

    #[test]
    fn long_paths_are_shortened() {
        let short = shorten_path("/very/long/path/to/some/project/folder", 20);
        assert!(short.width() <= 20, "{}", short);
        assert!(short.ends_with("folder"));
        assert_eq!(shorten_path("/a/b", 20), "/a/b");
    }

    #[test]
    fn the_card_table_has_titled_folder_and_branch_columns() {
        let table = build_card_table(
            &[suggestion("api", true, FolderRelation::Here)],
            vec![],
            68,
            Some("main"),
        );
        assert_eq!(row_text(&table, 0), vec![" ", "Folder", "Branch", " "]);
        assert_eq!(row_text(&table, 1), vec!["api", "zellij", "main", "2m ago"]);
    }

    #[test]
    fn the_table_width_includes_the_space_after_every_column() {
        let table = build_card_table(
            &[suggestion("api", true, FolderRelation::Here)],
            vec![],
            68,
            Some("main"),
        );
        let cells = row_text(&table, 1);
        let widths: usize = (0..cells.len())
            .map(|i| {
                table.grid.as_ref().unwrap()
                    .iter()
                    .map(|row| row[i].content().width())
                    .max()
                    .unwrap_or(0)
            })
            .sum();
        assert_eq!(table.width, widths + cells.len());
    }

    #[test]
    fn folders_are_shown_by_name_here_and_by_path_elsewhere() {
        let table = build_card_table(
            &[
                suggestion("here", true, FolderRelation::Here),
                suggestion("below", true, FolderRelation::Subfolder),
                suggestion("other", true, FolderRelation::Other),
            ],
            vec![],
            68,
            None,
        );
        assert_eq!(row_text(&table, 1)[1], "zellij");
        assert!(row_text(&table, 2)[1].ends_with("zellij"));
        assert!(row_text(&table, 2)[1].contains('/'));
        assert!(row_text(&table, 3)[1].contains('/'));
        assert!(!row_text(&table, 2)[1].contains("subfolder"));
    }

    #[test]
    fn resumable_rows_get_a_resume_column() {
        let table = build_card_table(
            &[
                suggestion("api", true, FolderRelation::Here),
                suggestion("web", false, FolderRelation::Other),
            ],
            vec![],
            68,
            None,
        );
        assert_eq!(row_text(&table, 1).last().unwrap(), " ");
        assert_eq!(row_text(&table, 2).last().unwrap(), "resume");
    }

    #[test]
    fn custom_columns_are_dropped_when_too_wide() {
        let mut s = suggestion("a-very-long-session-name", true, FolderRelation::Other);
        s.facts.insert(
            "project".to_owned(),
            "an-extremely-long-project-value-here".to_owned(),
        );
        let table = build_card_table(
            &[s],
            vec![SessionSuggestionColumn {
                fact: "project".to_owned(),
                title: "project".to_owned(),
            }],
            58,
            None,
        );
        assert!(table.width <= 58, "{}", table.width);
        assert!(!row_text(&table, 1)
            .iter()
            .any(|cell| cell.contains("an-extremely-long")));
    }

    #[test]
    fn a_full_match_needs_the_same_folder_and_branch() {
        let here = suggestion("api", true, FolderRelation::Here);
        assert!(is_full_match(&here, Some("main")));
        assert!(!is_full_match(&here, Some("dev")));
        assert!(!is_full_match(&here, None));
        let below = suggestion("api", true, FolderRelation::Subfolder);
        assert!(!is_full_match(&below, Some("main")));
    }

    #[test]
    fn the_card_lists_only_running_sessions_and_counts_the_rest() {
        let panel = panel_with(vec![
            suggestion("a", true, FolderRelation::Here),
            suggestion("b", false, FolderRelation::Here),
            suggestion("c", false, FolderRelation::Here),
        ]);
        let names: Vec<String> = panel.card_suggestions().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["a"]);
        assert_eq!(
            panel.resumable_text(),
            Some("+2 resurrectable sessions".to_owned())
        );
        let one = panel_with(vec![suggestion("b", false, FolderRelation::Here)]);
        assert_eq!(
            one.resumable_text(),
            Some("+1 resurrectable session".to_owned())
        );
        assert_eq!(one.card_empty_text(), CARD_NO_LIVE_TEXT);
        assert_eq!(panel_with(vec![]).card_empty_text(), CARD_EMPTY_TEXT);
    }

    #[test]
    fn sessions_without_a_folder_or_a_branch_are_listed_with_placeholders() {
        let no_folder = SessionSuggestion {
            folder: None,
            ..suggestion("no-folder", true, FolderRelation::Other)
        };
        let no_branch = SessionSuggestion {
            branch: None,
            ..suggestion("no-branch", true, FolderRelation::Other)
        };
        let neither = SessionSuggestion {
            name: "neither".to_owned(),
            is_running: true,
            ..Default::default()
        };
        let panel = panel_with(vec![no_folder, no_branch, neither]);
        let listed = panel.card_suggestions();
        let names: Vec<&str> = listed.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["no-folder", "no-branch", "neither"]);
        let table = build_card_table(&listed, vec![], 68, Some("main"));
        assert_eq!(
            row_text(&table, 1),
            vec!["no-folder", "—", "main", "2m ago"]
        );
        let no_branch_row = row_text(&table, 2);
        assert!(no_branch_row[1].ends_with("zellij"), "{:?}", no_branch_row);
        assert_eq!(no_branch_row[2], "—");
        assert_eq!(row_text(&table, 3), vec!["neither", "—", "—", "0s ago"]);
    }

    #[test]
    fn keyboard_selection_stays_inside_the_list() {
        let mut panel = panel_with(
            (0..4)
                .map(|i| suggestion(&format!("s{}", i), true, FolderRelation::Other))
                .collect(),
        );
        panel.move_card_selection(1);
        assert_eq!(panel.hovered, Some(CardTarget::Row(0)));
        panel.move_card_selection(10);
        assert_eq!(panel.hovered, Some(CardTarget::Row(3)));
        panel.move_card_selection(-10);
        assert_eq!(panel.hovered, Some(CardTarget::Row(0)));
        panel.select_card_edge(true);
        assert_eq!(panel.hovered, Some(CardTarget::Row(3)));
        panel.select_card_edge(false);
        assert_eq!(panel.hovered, Some(CardTarget::Row(0)));
        let mut empty = panel_with(vec![]);
        empty.move_card_selection(1);
        assert_eq!(empty.hovered, Some(CardTarget::ShowAll));
    }

    #[test]
    fn the_show_all_shortcut_is_only_offered_when_focused() {
        let mut panel = panel_with(vec![]);
        assert_eq!(panel.show_all_text(), SHOW_ALL_TEXT);
        panel.popup_focused = true;
        assert_eq!(
            panel.show_all_text(),
            format!("{} {}", SHOW_ALL_SHORTCUT, SHOW_ALL_TEXT)
        );
    }

    #[test]
    fn the_card_list_is_limited_to_an_eighth_of_the_screen() {
        let mut panel = panel_with(
            (0..30)
                .map(|i| suggestion(&format!("s{}", i), true, FolderRelation::Other))
                .collect(),
        );
        panel.display.rows = 20;
        assert_eq!(panel.card_list_row_limit(), CARD_MIN_LIST_ROWS);
        let (_, small_height) = panel.card_dimensions();
        panel.display.rows = 80;
        assert_eq!(panel.card_list_row_limit(), 10);
        let (_, large_height) = panel.card_dimensions();
        assert_eq!(large_height, small_height + 5);
        let few = panel_with(vec![suggestion("a", true, FolderRelation::Other)]);
        let (width, height) = few.card_dimensions();
        assert!(width <= CARD_MAX_WIDTH);
        assert_eq!(height, 1 + 1 + 4 + 2 * CARD_PADDING);
    }

    #[test]
    fn scrolling_keeps_a_margin_around_the_selection() {
        assert_eq!(scroll_with_margin(0, 0, 5, 20, 2), 0);
        assert_eq!(scroll_with_margin(0, 2, 5, 20, 2), 0);
        assert_eq!(scroll_with_margin(0, 3, 5, 20, 2), 1);
        assert_eq!(scroll_with_margin(10, 11, 5, 20, 2), 9);
        assert_eq!(scroll_with_margin(0, 19, 5, 20, 2), 15);
        assert_eq!(scroll_with_margin(0, 2, 3, 20, 2), 1);
    }

    #[test]
    fn ansi_lines_are_truncated_by_visible_width() {
        let line = "\u{1b}[31mhello world\u{1b}[0m";
        let fitted = fit_ansi(line, 5);
        assert_eq!(strip_ansi(&fitted), "hello");
        assert!(fitted.starts_with("\u{1b}[31m"));
    }

    #[test]
    fn preview_lines_keep_only_colours_and_fill_the_width() {
        let line = "a\u{1b}[2Kb\u{1b}[10Cc\u{1b}]0;title\u{7}d\tz\r";
        let fitted = fit_ansi(line, 12);
        assert!(!fitted.contains("\u{1b}[2K"));
        assert!(!fitted.contains("\u{1b}[10C"));
        assert!(!fitted.contains("title"));
        assert_eq!(strip_ansi(&fitted), "abcd    z   ");
        let wide = fit_ansi("ab漢字", 5);
        assert_eq!(strip_ansi(&wide), "ab漢 ");
    }
}

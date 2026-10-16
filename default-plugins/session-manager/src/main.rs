mod app_panel;
mod inbox;
mod new_session_info;
mod panel;
mod resurrectable_sessions;
mod session_list;
mod session_tree;
mod single_screen;
mod timers;
mod ui;
use std::collections::BTreeMap;
use uuid::Uuid;
use zellij_tile::prelude::*;

use app_panel::{
    close_self, hide_self, switch_session, switch_session_and_close_current,
    switch_session_with_focus, switch_session_with_options,
};

use new_session_info::NewSessionInfo;
use single_screen::{DeleteTarget, SingleScreenMode, SingleScreenState, UnifiedSearchResult};
use ui::{
    components::{
        render_controls_line, render_error, render_new_session_block, render_prompt,
        render_renaming_session_screen, render_screen_toggle, render_single_screen_prompt,
        render_unsaved_changes_line, Colors,
    },
    SessionUiInfo,
};

use inbox::{BrowserTab, BrowserTarget, CardTarget as InboxCardTarget, Inbox, InboxRow, ItemKey};
use panel::{render_preview, render_session_tree, Panel, PanelSize, NO_OTHER_SESSIONS_TEXT};
use resurrectable_sessions::ResurrectableSessions;
use session_list::SessionList;

#[derive(Clone, Debug, Copy, PartialEq)]
enum ActiveScreen {
    NewSession,
    AttachToSession,
    ResurrectSession,
    SingleScreen,
}

impl Default for ActiveScreen {
    fn default() -> Self {
        ActiveScreen::AttachToSession
    }
}

#[derive(Default)]
pub(crate) struct State {
    app_quit: bool,
    app_panel_initial_view: Option<String>,
    app_panel_auto_pending: bool,
    app_panel_cols: usize,
    session_name: Option<String>,
    pub(crate) sessions: SessionList,
    pub(crate) resurrectable_sessions: ResurrectableSessions,
    search_term: String,
    new_session_info: NewSessionInfo,
    pub(crate) renaming_session_name: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) active_screen: ActiveScreen,
    pub(crate) colors: Colors,
    pub(crate) is_welcome_screen: bool,
    is_multi_screen: bool,
    pub(crate) single_screen_state: SingleScreenState,
    pub(crate) show_kill_all_sessions_warning: bool,
    pub(crate) pending_session_stop: Option<String>,
    request_ids: Vec<String>,
    is_web_client: bool,
    current_session_last_saved_time: Option<u64>,
    is_visible: bool,
    pub(crate) panel: Panel,
    current_panes: PaneManifest,
    last_tab_count: Option<usize>,
    top_offset: usize,
    last_terminal_pane_count: Option<usize>,
    pub(crate) inbox: Inbox,
    simplified_ui: bool,
}

const WELCOME_TEXT: &str = "Enter a session name to create it, attach to it if it's running or resurrect it if it's exited.";

register_plugin!(State);

fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut current = String::new();
    for word in text.split_whitespace() {
        let needed = if current.is_empty() {
            zellij_tile::graphemes::display_width(word)
        } else {
            zellij_tile::graphemes::display_width(&current)
                + 1
                + zellij_tile::graphemes::display_width(word)
        };
        if needed > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.is_welcome_screen = configuration
            .get("welcome_screen")
            .map(|v| v == "true")
            .unwrap_or(false);
        if self.is_welcome_screen {
            self.active_screen = ActiveScreen::NewSession;
        }
        self.new_session_info.is_welcome_screen = self.is_welcome_screen;
        self.is_multi_screen = configuration
            .get("multi_screen")
            .map(|v| v == "true")
            .unwrap_or(false);
        if !self.is_multi_screen {
            self.active_screen = ActiveScreen::SingleScreen;
        }
        self.single_screen_state.is_welcome_screen = self.is_welcome_screen;
        self.is_visible = true;
        subscribe(&[
            EventType::ModeUpdate,
            EventType::SessionUpdate,
            EventType::Key,
            EventType::RunCommandResult,
            EventType::Timer,
            EventType::Visible,
            EventType::BeforeClose,
        ]);
        let plugin_id = get_plugin_ids().plugin_id;
        rename_plugin_pane(plugin_id, "Session Manager");
        {
            self.panel.own_plugin_id = Some(plugin_id);
            self.single_screen_state.ordered_names = Some(vec![]);
            let role = configuration.get("role").map(|r| r.as_str());
            match role {
                Some(app_panel::ROLE) => {
                    app_panel::enable();
                    self.app_panel_initial_view = Some(
                        configuration
                            .get("view")
                            .cloned()
                            .unwrap_or_else(|| "auto".to_owned()),
                    );
                    self.panel.popup_focused = true;
                    self.panel.is_popup = false;
                    self.panel.size = Some(PanelSize::Manager);
                    self.panel.needs_initial_layout = false;
                    self.inbox.choose_inbox_tab();
                },
                Some("card_startup") => {
                    self.panel.opened_at_startup = true;
                    self.panel.popup_focused = false;
                    self.panel.is_popup = true;
                    self.panel.needs_initial_layout = false;
                    self.panel.size = Some(PanelSize::Card);
                    self.inbox.away_since_ms = configuration
                        .get("away_since_ms")
                        .and_then(|v| v.parse::<u64>().ok());
                },
                Some("card") | Some("card_focused") => {
                    self.panel.popup_focused = true;
                    self.panel.is_popup = true;
                    self.panel.size = Some(PanelSize::Manager);
                    self.panel.needs_initial_layout = true;
                    self.inbox.choose_inbox_tab();
                    self.inbox.select_next_counted();
                },
                _ => {
                    self.panel.size = Some(PanelSize::Manager);
                    self.panel.needs_initial_layout = !self.is_welcome_screen;
                },
            }
            subscribe(&[
                EventType::Mouse,
                EventType::TabUpdate,
                EventType::PaneUpdate,
                EventType::AvailableLayoutInfo,
                EventType::SessionSuggestions,
                EventType::SessionCountsUpdate,
                EventType::TerminalCommandSubmitted,
                EventType::SessionPreview,
                EventType::SavedSessionPreview,
                EventType::GlobalAttentionUpdate,
                EventType::SessionHistoryUpdate,
            ]);
            self.panel.request_suggestions();
            self.panel.report_size();
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        app_panel::begin_event();
        if pipe_message.name == app_panel::VIEW_PIPE {
            if !app_panel::is_enabled() {
                return false;
            }
            self.app_panel_initial_view = None;
            let view = pipe_message.payload.unwrap_or_else(|| "auto".to_owned());
            self.app_panel_auto_pending = view == "auto";
            self.show_app_panel_view(Some(&view));
            return true;
        }
        let should_render = self.handle_pipe(pipe_message);
        self.keep_ages_fresh();
        should_render
    }
    fn update(&mut self, event: Event) -> bool {
        app_panel::begin_event();
        let user_input = matches!(event, Event::Key(_) | Event::Mouse(_));
        let attention = matches!(event, Event::GlobalAttentionUpdate(_));
        let mut should_render = self.handle_event(event);
        should_render |= self.settle_app_panel_view(user_input, attention);
        self.keep_ages_fresh();
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        self.render_plugin(rows, cols);
    }
}

impl State {
    fn keep_ages_fresh(&mut self) {
        if self.is_visible && self.shows_ages() {
            self.inbox.arm_clock(inbox::now_ms());
        }
    }
    fn handle_pipe(&mut self, pipe_message: PipeMessage) -> bool {
        if self.panel.size.is_some() {
            if let Some(should_render) = self.handle_panel_pipe(&pipe_message) {
                return should_render;
            }
        }
        if pipe_message.name == "filepicker_result" {
            match (pipe_message.payload, pipe_message.args.get("request_id")) {
                (Some(payload), Some(request_id)) => {
                    match self.request_ids.iter().position(|p| p == request_id) {
                        Some(request_id_position) => {
                            self.request_ids.remove(request_id_position);
                            let new_session_folder = std::path::PathBuf::from(payload);
                            if !self.is_multi_screen {
                                self.single_screen_state.new_session_folder =
                                    Some(new_session_folder.clone());
                            }
                            self.new_session_info.new_session_folder = Some(new_session_folder);
                        },
                        None => {
                            eprintln!("request id not found");
                        },
                    }
                },
                _ => {},
            }
            true
        } else {
            false
        }
    }
    fn handle_event(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::Timer(_) => match timers::fired() {
                Some(timers::TimerOwner::Clock) => {
                    self.inbox.clock_armed = false;
                    return self.is_visible && self.shows_ages();
                },
                Some(timers::TimerOwner::InboxPreview) => {
                    self.inbox.timer_armed = false;
                    if self.is_visible && self.inbox_is_shown() {
                        self.inbox.request_preview();
                        self.inbox.arm_timer();
                    }
                    return false;
                },
                Some(timers::TimerOwner::SessionsPreview) | None => {
                    self.panel.preview_timer_armed = false;
                    if !self.is_visible || self.inbox_is_shown() {
                        return false;
                    }
                    let new_saved_time = current_session_last_saved_time();
                    if new_saved_time != self.current_session_last_saved_time {
                        self.current_session_last_saved_time = new_saved_time;
                        should_render = true;
                    }
                    if self.refresh_selected_preview() {
                        should_render = true;
                    }
                },
            },
            Event::BeforeClose => {
                if self.panel.size.is_some() {
                    self.panel.size = None;
                    self.panel.report_size();
                }
            },
            Event::Visible(is_visible) => {
                let was_visible = self.is_visible;
                self.is_visible = is_visible;
                if is_visible && !was_visible {
                    self.panel.request_suggestions();
                }
            },
            Event::ModeUpdate(mode_info) => {
                if let Some(session_name) = mode_info.session_name.as_ref() {
                    if !self.is_welcome_screen {
                        self.session_name = Some(session_name.clone());
                    }
                }
                self.colors = Colors::new(mode_info.style.colors);
                self.simplified_ui = mode_info.capabilities.arrow_fonts;
                let was_web_client = self.is_web_client;
                self.is_web_client = mode_info.is_web_client.unwrap_or(false);
                if was_web_client != self.is_web_client && self.is_welcome_screen {
                    self.panel.request_suggestions();
                }
                should_render = true;
            },
            Event::Key(key) => {
                should_render = self.handle_key(key);
            },
            Event::PermissionRequestResult(_result) => {
                should_render = true;
            },
            Event::SessionUpdate(session_infos, _) if self.panel.size.is_some() => {
                for session_info in &session_infos {
                    if session_info.is_current_session {
                        self.new_session_info
                            .update_layout_list(session_info.available_layouts.clone());
                        self.session_name = Some(session_info.name.clone());
                    }
                }
                self.refresh_layout_list();
                should_render = self.single_screen_state.mode == SingleScreenMode::SelectingLayout;
            },
            Event::AvailableLayoutInfo(layouts, _) => {
                self.new_session_info.update_layout_list(layouts);
                self.refresh_layout_list();
                should_render = true;
            },
            Event::TabUpdate(tabs) => {
                let grew = self
                    .last_tab_count
                    .map(|previous| tabs.len() > previous)
                    .unwrap_or(false);
                self.last_tab_count = Some(tabs.len());
                if grew && self.panel.size == Some(PanelSize::Card) {
                    self.panel.close();
                    return false;
                }
                if let Some(active_tab) = tabs.iter().find(|t| t.active) {
                    self.panel.display.columns = active_tab.display_area_columns;
                    self.panel.display.rows = active_tab.display_area_rows;
                    self.panel.display.viewport_columns = active_tab.viewport_columns;
                    self.panel.display.viewport_rows = active_tab.viewport_rows;
                }
                if self.panel.needs_initial_layout && self.panel.display.columns > 0 {
                    self.panel.needs_initial_layout = false;
                    if let Some(size) = self.panel.size {
                        self.panel.size = None;
                        self.panel.apply_size(size);
                    }
                }
            },
            Event::PaneUpdate(pane_manifest) => {
                let terminal_panes = pane_manifest
                    .panes
                    .values()
                    .flatten()
                    .filter(|p| !p.is_plugin)
                    .count();
                let grew = self
                    .last_terminal_pane_count
                    .map(|previous| terminal_panes > previous)
                    .unwrap_or(false);
                self.last_terminal_pane_count = Some(terminal_panes);
                self.current_panes = pane_manifest;
                if grew && self.panel.size == Some(PanelSize::Card) {
                    self.panel.close();
                    return false;
                }
            },
            Event::GlobalAttentionUpdate(global) => {
                let previous_preview = self.inbox.selected_item().map(ItemKey::of);
                self.panel.attention = Some(global.clone());
                should_render = self.inbox.update_global(global);
                if self.away_card_is_empty() {
                    close_self();
                    return false;
                }
                if self.panel.size == Some(PanelSize::Card) {
                    self.resize_card_popup();
                }
                if self.inbox_is_shown()
                    && self.inbox.selected_item().map(ItemKey::of) != previous_preview
                {
                    self.inbox.request_preview();
                }
            },
            Event::SessionHistoryUpdate(history) => {
                self.panel.previous_session = history.previous.clone();
                should_render = self.inbox.update_history(history);
                if self.panel.size == Some(PanelSize::Card) {
                    self.resize_card_popup();
                }
            },
            Event::SessionSuggestions(suggestions) => {
                self.panel.command_submitted |= suggestions.command_submitted;
                self.panel.suggestions = Some(suggestions);
                if self.panel.size != Some(PanelSize::Card) {
                    self.apply_suggestions_to_list();
                }
                if self.panel.size == Some(PanelSize::Card) {
                    self.resize_card_popup();
                }
                should_render = true;
            },
            Event::SessionCountsUpdate(_) => {
                if self.panel.is_open() {
                    self.panel.request_suggestions();
                }
            },
            Event::TerminalCommandSubmitted => {
                self.panel.command_submitted = true;
                if self.panel.size == Some(PanelSize::Card) {
                    self.panel.close();
                    return false;
                }
            },
            Event::SessionPreview(preview) if self.inbox.accepts_preview(&preview) => {
                should_render = self.inbox.preview.as_ref() != Some(&preview);
                self.inbox.preview = Some(preview);
            },
            Event::SessionPreview(preview) => {
                let known = self
                    .panel
                    .tree
                    .children
                    .get(&preview.session_name)
                    .map(|tabs| !tabs.is_empty())
                    .unwrap_or(false);
                let tree_changed = !known
                    && !preview.tabs.is_empty()
                    && self
                        .panel
                        .tree
                        .set_children_from_tabs(&preview.session_name, &preview.tabs);
                should_render = self.apply_session_preview(preview) || tree_changed;
            },
            Event::SavedSessionPreview(preview) => {
                if self.panel.tree.set_children_from_saved(&preview) {
                    should_render = true;
                }
                if self.panel.preview_session.as_deref() == Some(preview.session_name.as_str())
                    && self.panel.saved_preview.as_ref() != Some(&preview)
                {
                    self.panel.saved_preview = Some(preview);
                    self.panel.preview = None;
                    should_render = true;
                }
            },
            Event::Mouse(Mouse::LeftClick(line, column))
                if app_panel::is_enabled()
                    && app_panel::is_close_button(line, column, self.app_panel_cols) =>
            {
                self.app_quit = false;
                self.panel.close();
            },
            Event::Mouse(_) if self.app_quit => {},
            Event::Mouse(mouse) => {
                should_render = self.handle_mouse(mouse);
            },
            Event::SessionUpdate(session_infos, resurrectable_session_list) => {
                for session_info in &session_infos {
                    if session_info.is_current_session {
                        self.new_session_info
                            .update_layout_list(session_info.available_layouts.clone());
                    }
                }
                self.resurrectable_sessions
                    .update(resurrectable_session_list);
                self.update_session_infos(session_infos);
                if !self.is_multi_screen {
                    self.single_screen_state.update_search_term(
                        &self.sessions.session_ui_infos,
                        &self.resurrectable_sessions.all_resurrectable_sessions,
                    );
                    let previous_selection =
                        self.single_screen_state.layout_list.selected_layout_index;
                    let previous_search_term = self
                        .single_screen_state
                        .layout_list
                        .layout_search_term
                        .clone();
                    self.single_screen_state.layout_list =
                        self.new_session_info.get_layout_list_clone();
                    self.single_screen_state.layout_list.layout_search_term = previous_search_term;
                    self.single_screen_state.layout_list.update_search_term();
                    self.single_screen_state.layout_list.selected_layout_index =
                        previous_selection.min(self.single_screen_state.layout_list.max_index());
                }
                should_render = true;
            },
            _ => (),
        };
        should_render
    }

    fn render_plugin(&mut self, rows: usize, cols: usize) {
        self.render_plugin_body(rows, cols);
        if app_panel::is_enabled() {
            self.app_panel_cols = cols;
            app_panel::render_close_button(cols);
        }
    }

    fn render_plugin_body(&mut self, rows: usize, cols: usize) {
        if self.app_quit {
            self.render_app_quit(rows, cols);
            return;
        }
        if self.panel.size == Some(PanelSize::Manager) && self.shows_browser_tabs() {
            let sessions = self.running_session_count();
            self.inbox.render_tabs(sessions, 0, 0);
            if self.inbox.tab == BrowserTab::Inbox {
                let simplified_ui = self.simplified_ui;
                self.inbox
                    .render_browser(0, 1, cols, rows.saturating_sub(1), simplified_ui);
                return;
            }
        }
        match self.panel.size {
            Some(PanelSize::Card) => {
                let (popup_focused, opened_at_startup, simplified_ui) = (
                    self.panel.popup_focused,
                    self.panel.opened_at_startup,
                    self.simplified_ui,
                );
                self.inbox
                    .render_card(cols, rows, popup_focused, opened_at_startup, simplified_ui);
            },
            Some(PanelSize::Manager) if self.no_other_sessions() => {
                self.render_welcome_text(cols);
                self.render_classic(rows, cols);
            },
            Some(PanelSize::Manager)
                if self.single_screen_state.mode == SingleScreenMode::SelectingLayout =>
            {
                self.render_welcome_text(cols);
                self.render_classic(rows, cols);
            },
            Some(PanelSize::Manager) if cols >= 60 => {
                let list_cols = (cols / 2).max(30);
                self.render_welcome_text(list_cols);
                self.render_classic(rows, list_cols);
                let top = self.top_offset;
                render_preview(
                    &mut self.panel,
                    list_cols + 1,
                    top,
                    cols.saturating_sub(list_cols + 1),
                    rows.saturating_sub(top),
                );
            },
            _ => {
                self.render_welcome_text(cols);
                self.render_classic(rows, cols);
            },
        }
    }
}

impl State {
    fn render_welcome_text(&mut self, list_cols: usize) {
        self.top_offset =
            if self.panel.size == Some(PanelSize::Manager) && self.shows_browser_tabs() {
                2
            } else {
                0
            };
        if !self.is_welcome_screen
            || self.panel.size.is_none()
            || self.single_screen_state.mode != SingleScreenMode::SearchAndSelect
        {
            return;
        }
        let content_width = std::cmp::min(list_cols, 90);
        let x = list_cols.saturating_sub(content_width) / 2;
        let lines = wrap_words(WELCOME_TEXT, content_width.max(10));
        for (index, line) in lines.iter().enumerate() {
            print_text_with_coordinates(
                Text::from(line.as_str()).color_range(2, ..),
                x,
                index + 1,
                None,
                None,
            );
        }
        self.top_offset = lines.len() + 2;
    }
    fn render_classic(&mut self, rows: usize, cols: usize) {
        let (x, y, width, height) = self.main_menu_size(rows, cols);

        let background = self.colors.palette.text_unselected.background;

        if self.active_screen != ActiveScreen::SingleScreen {
            render_screen_toggle(
                self.active_screen,
                x,
                y,
                width.saturating_sub(2),
                &background,
            );
        }

        match self.active_screen {
            ActiveScreen::NewSession => {
                render_new_session_block(
                    &self.new_session_info,
                    self.colors,
                    height.saturating_sub(2),
                    width,
                    x,
                    y + 2,
                );
            },
            ActiveScreen::AttachToSession => {
                if let Some(new_session_name) = self.renaming_session_name.as_ref() {
                    render_renaming_session_screen(&new_session_name, height, width, x, y + 2);
                } else if self.show_kill_all_sessions_warning {
                    self.render_kill_all_sessions_warning(height, width, x, y);
                } else {
                    render_prompt(&self.search_term, self.colors, x, y + 2);
                    let bottom_lines = 7;
                    let room_for_list = height.saturating_sub(bottom_lines);
                    self.sessions.update_rows(room_for_list);
                    let list =
                        self.sessions
                            .render(room_for_list, width.saturating_sub(7), self.colors); // 7 for various ui
                    for (i, line) in list.iter().enumerate() {
                        print!("\u{1b}[{};{}H{}", y + i + 5, x, line.render());
                    }
                }
            },
            ActiveScreen::ResurrectSession => {
                self.resurrectable_sessions.render(height, width, x, y);
            },
            ActiveScreen::SingleScreen => {
                match self.single_screen_state.mode {
                    SingleScreenMode::SearchAndSelect => {
                        if let Some(new_session_name) = self.renaming_session_name.as_ref() {
                            render_renaming_session_screen(new_session_name, height, width, x, y);
                        } else if self.show_kill_all_sessions_warning {
                            self.render_kill_all_sessions_warning(height, width, x, y);
                        } else if let Some(name) = self.pending_session_stop.as_ref() {
                            render_stop_session_warning(name, height, width, x, y);
                        } else {
                            // Use max_table_rows as fixed content height so the
                            // prompt position stays stable regardless of result count
                            let max_table_rows = height.saturating_sub(5);
                            let content_height = if self.no_other_sessions() {
                                3
                            } else {
                                2 + max_table_rows
                            };
                            // Available space above help lines (2 help rows at bottom)
                            let available = height.saturating_sub(3);
                            let y_offset = y + available.saturating_sub(content_height) / 2;

                            // Horizontal centering: cap content block and center
                            // within the full pane width
                            let content_width = std::cmp::min(width, 90);
                            let x_centered = x + (width.saturating_sub(content_width)) / 2;

                            let selected_result = self.single_screen_state.get_selected_result();
                            let enter_action = if let (Some(result), true) =
                                (selected_result, self.panel.size.is_some())
                            {
                                match result {
                                    UnifiedSearchResult::ActiveSession { .. } => Some("Attach"),
                                    UnifiedSearchResult::ResurrectableSession { .. } => {
                                        Some("Resurrect")
                                    },
                                }
                            } else if self.single_screen_state.search_term.is_empty()
                                && self.panel.size.is_some()
                            {
                                Some("New session with a random name")
                            } else if !self.single_screen_state.search_term.is_empty() {
                                if let Some(result) = self.single_screen_state.get_selected_result()
                                {
                                    match result {
                                        UnifiedSearchResult::ActiveSession { .. } => Some("Attach"),
                                        UnifiedSearchResult::ResurrectableSession { .. } => {
                                            Some("Resurrect")
                                        },
                                    }
                                } else {
                                    let typed = &self.single_screen_state.search_term;
                                    if self.sessions.has_session(typed) {
                                        Some("Attach")
                                    } else if self.resurrectable_sessions.has_session(typed) {
                                        Some("Resurrect")
                                    } else {
                                        Some("Create new")
                                    }
                                }
                            } else {
                                None
                            };
                            render_single_screen_prompt(
                                &self.single_screen_state.search_term,
                                enter_action,
                                self.colors,
                                x_centered,
                                y_offset,
                            );
                            if self.no_other_sessions() {
                                print_text_with_coordinates(
                                    Text::from(NO_OTHER_SESSIONS_TEXT),
                                    x_centered,
                                    y_offset + 2,
                                    None,
                                    None,
                                );
                            } else {
                                render_session_tree(
                                    &mut self.panel,
                                    &self.single_screen_state.render_cache,
                                    !self.single_screen_state.search_term.is_empty(),
                                    self.single_screen_state.selected_index,
                                    max_table_rows,
                                    content_width,
                                    x_centered,
                                    y_offset + 2,
                                );
                            }
                        }
                    },
                    SingleScreenMode::SelectingLayout => {
                        let new_session_name = if self.single_screen_state.search_term.is_empty() {
                            "<RANDOM>"
                        } else {
                            &self.single_screen_state.search_term
                        };
                        let esc = self.colors.shortcuts("<ESC>");
                        println!(
                            "\u{1b}[m\u{1b}[{};{}H{}: {} ({} to go back)",
                            y + 1,
                            x + 1,
                            self.colors.session_name_prompt("New session name"),
                            self.colors.session_and_folder_entry(new_session_name),
                            esc,
                        );

                        // Render layout selection
                        let layout_search_term =
                            &self.single_screen_state.layout_list.layout_search_term;
                        let search_term_len = layout_search_term.len();
                        let layout_indication_line = if width > 73 + search_term_len {
                            Text::from(format!(
                                "New session layout: {}_ (Search and select from list, <ENTER> when done)",
                                layout_search_term
                            ))
                            .color_range(2, ..20 + search_term_len)
                            .color_range(3, 20..20 + search_term_len)
                            .color_range(3, 52 + search_term_len..59 + search_term_len)
                        } else {
                            Text::from(format!(
                                "New session layout: {}_ <ENTER>",
                                layout_search_term
                            ))
                            .color_range(2, ..20 + search_term_len)
                            .color_range(3, 20..20 + search_term_len)
                            .color_range(3, 22 + search_term_len..)
                        };
                        print_text_with_coordinates(layout_indication_line, x, y + 2, None, None);
                        println!();

                        let max_layout_rows = height.saturating_sub(8);
                        let mut table = Table::new();
                        for (i, (layout_info, indices, is_selected)) in self
                            .single_screen_state
                            .layout_list
                            .layouts_to_render(max_layout_rows)
                            .into_iter()
                            .enumerate()
                        {
                            let layout_name = layout_info.name();
                            let layout_name_len = layout_name.len();
                            let is_builtin = layout_info.is_builtin();
                            if i > max_layout_rows.saturating_sub(1) {
                                break;
                            }
                            let mut layout_cell = if is_builtin {
                                Text::from(format!("{} (built-in)", layout_name))
                                    .color_range(1, 0..layout_name_len)
                                    .color_range(0, layout_name_len + 1..)
                                    .color_indices(3, indices)
                            } else {
                                Text::from(format!("{}", layout_name))
                                    .color_range(1, ..)
                                    .color_indices(3, indices)
                            };
                            if is_selected {
                                layout_cell = layout_cell.selected();
                            }
                            let arrow_cell = if is_selected {
                                Text::from(format!("<↓↑>")).selected().color_range(3, ..)
                            } else {
                                Text::from(format!("    ")).color_range(3, ..)
                            };
                            table = table.add_styled_row(vec![arrow_cell, layout_cell]);
                        }
                        print_table_with_coordinates(table, x, y + 4, None, None);

                        // Render folder prompt
                        self.render_single_screen_folder_prompt(
                            x,
                            (y + height).saturating_sub(3),
                            width,
                        );
                    },
                }
            },
        }
        if let Some(error) = self.error.as_ref() {
            render_error(&error, height, width, x, y);
        } else if self.active_screen == ActiveScreen::AttachToSession
            || self.active_screen == ActiveScreen::SingleScreen
        {
            let help_x = if self.active_screen == ActiveScreen::SingleScreen
                && self.single_screen_state.mode == SingleScreenMode::SearchAndSelect
            {
                let content_width = std::cmp::min(width, 90);
                x + (width.saturating_sub(content_width)) / 2
            } else {
                x
            };
            let help_offset = render_controls_line(
                self.active_screen,
                width,
                self.colors,
                help_x,
                rows.saturating_sub(1),
                self.panel.size == Some(PanelSize::Manager),
            );
            let adjusted_x = help_x + help_offset;
            let adjusted_width = width.saturating_sub(help_offset);
            if !self.is_welcome_screen && !app_panel::is_enabled() {
                render_unsaved_changes_line(
                    adjusted_width,
                    adjusted_x,
                    rows,
                    self.current_session_last_saved_time,
                );
            }
        } else {
            let _ = render_controls_line(self.active_screen, width, self.colors, x, rows, false);
        }
    }

    fn reset_selected_index(&mut self) {
        self.sessions.reset_selected_index();
    }
    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if self.app_quit {
            match key.bare_key {
                BareKey::Enter | BareKey::Char('y') | BareKey::Char('Y')
                    if key.has_no_modifiers() || key.has_only_modifiers(&[KeyModifier::Shift]) =>
                {
                    app_panel::request_quit();
                },
                BareKey::Esc | BareKey::Char('n') | BareKey::Char('N') => {
                    self.app_quit = false;
                    self.panel.close();
                },
                _ => return false,
            }
            return true;
        }
        if self.error.is_some() {
            self.error = None;
            return true;
        }
        if self.inbox_is_shown() {
            return self.handle_inbox_key(key);
        }
        if let Some(should_render) = self.handle_panel_key(&key) {
            return should_render;
        }
        match self.active_screen {
            ActiveScreen::NewSession => self.handle_new_session_key(key),
            ActiveScreen::AttachToSession => self.handle_attach_to_session(key),
            ActiveScreen::ResurrectSession => self.handle_resurrect_session_key(key),
            ActiveScreen::SingleScreen => self.handle_single_screen_key(key),
        }
    }
    fn handle_new_session_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        match key.bare_key {
            BareKey::Down if key.has_no_modifiers() => {
                self.new_session_info.handle_key(key);
                should_render = true;
            },
            BareKey::Up if key.has_no_modifiers() => {
                self.new_session_info.handle_key(key);
                should_render = true;
            },
            BareKey::Enter if key.has_no_modifiers() => {
                self.handle_selection();
                should_render = true;
            },
            BareKey::Char(character) if key.has_no_modifiers() => {
                if character == '\n' {
                    self.handle_selection();
                } else {
                    self.new_session_info.handle_key(key);
                }
                should_render = true;
            },
            BareKey::Backspace if key.has_no_modifiers() => {
                self.new_session_info.handle_key(key);
                should_render = true;
            },
            BareKey::Char('w') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.active_screen = ActiveScreen::NewSession;
                should_render = true;
            },
            BareKey::Tab if key.has_no_modifiers() => {
                self.toggle_active_screen();
                should_render = true;
            },
            BareKey::Char('f') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                let request_id = Uuid::new_v4();
                let mut config = BTreeMap::new();
                let mut args = BTreeMap::new();
                self.request_ids.push(request_id.to_string());
                // we insert this into the config so that a new plugin will be opened (the plugin's
                // uniqueness is determined by its name/url as well as its config)
                config.insert("request_id".to_owned(), request_id.to_string());
                // we also insert this into the args so that the plugin will have an easier access to
                // it
                args.insert("request_id".to_owned(), request_id.to_string());
                pipe_message_to_plugin(
                    MessageToPlugin::new("filepicker")
                        .with_plugin_url("filepicker")
                        .with_plugin_config(config)
                        .new_plugin_instance_should_have_pane_title(
                            "Select folder for the new session...",
                        )
                        .new_plugin_instance_should_be_focused()
                        .with_args(args),
                );
                should_render = true;
            },
            BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.new_session_info.new_session_folder = None;
                should_render = true;
            },
            BareKey::Esc if key.has_no_modifiers() => {
                self.new_session_info.handle_key(key);
                should_render = true;
            },
            _ => {},
        }
        should_render
    }
    fn handle_attach_to_session(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        if self.show_kill_all_sessions_warning {
            match key.bare_key {
                BareKey::Char('y') if key.has_no_modifiers() => {
                    let all_other_sessions = self.sessions.all_other_sessions();
                    let was_searching = self.sessions.is_searching;
                    let prev_search_idx = self.sessions.selected_search_index;
                    let prev_top_idx = self.sessions.selected_index.0;
                    match kill_sessions(&all_other_sessions) {
                        Ok(()) => {
                            self.sessions
                                .session_ui_infos
                                .retain(|s| !all_other_sessions.contains(&s.name));
                            self.sessions
                                .update_search_term(&self.search_term, &self.colors);
                            self.sessions.restore_selection_after_delete(
                                was_searching,
                                prev_search_idx,
                                prev_top_idx,
                            );
                        },
                        Err(e) => {
                            self.show_error(&format!("Failed to kill sessions: {}", e));
                        },
                    }
                    self.show_kill_all_sessions_warning = false;
                    should_render = true;
                },
                BareKey::Char('n') | BareKey::Esc if key.has_no_modifiers() => {
                    self.show_kill_all_sessions_warning = false;
                    should_render = true;
                },
                BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    self.show_kill_all_sessions_warning = false;
                    should_render = true;
                },
                _ => {},
            }
        } else {
            match key.bare_key {
                BareKey::Right if key.has_no_modifiers() => {
                    self.sessions.result_expand();
                    should_render = true;
                },
                BareKey::Left if key.has_no_modifiers() => {
                    self.sessions.result_shrink();
                    should_render = true;
                },
                BareKey::Down if key.has_no_modifiers() => {
                    self.sessions.move_selection_down();
                    should_render = true;
                },
                BareKey::Up if key.has_no_modifiers() => {
                    self.sessions.move_selection_up();
                    should_render = true;
                },
                BareKey::Enter if key.has_no_modifiers() => {
                    self.handle_selection();
                    should_render = true;
                },
                BareKey::Char(character) if key.has_no_modifiers() => {
                    if character == '\n' {
                        self.handle_selection();
                    } else if let Some(new_session_name) = self.renaming_session_name.as_mut() {
                        new_session_name.push(character);
                    } else {
                        self.search_term.push(character);
                        self.sessions
                            .update_search_term(&self.search_term, &self.colors);
                    }
                    should_render = true;
                },
                BareKey::Backspace if key.has_no_modifiers() => {
                    if let Some(new_session_name) = self.renaming_session_name.as_mut() {
                        if new_session_name.is_empty() {
                            self.renaming_session_name = None;
                        } else {
                            zellij_tile::graphemes::pop_cluster(new_session_name);
                        }
                    } else {
                        zellij_tile::graphemes::pop_cluster(&mut self.search_term);
                        self.sessions
                            .update_search_term(&self.search_term, &self.colors);
                    }
                    should_render = true;
                },
                BareKey::Char('w') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    self.active_screen = ActiveScreen::NewSession;
                    should_render = true;
                },
                BareKey::Char('r') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    self.renaming_session_name = Some(String::new());
                    should_render = true;
                },
                BareKey::Delete if key.has_no_modifiers() => {
                    if let Some(selected_session_name) = self.sessions.get_selected_session_name() {
                        let was_searching = self.sessions.is_searching;
                        let prev_search_idx = self.sessions.selected_search_index;
                        let prev_top_idx = self.sessions.selected_index.0;
                        match kill_sessions(&[selected_session_name.clone()]) {
                            Ok(()) => {
                                self.sessions
                                    .session_ui_infos
                                    .retain(|s| s.name != selected_session_name);
                                self.sessions
                                    .update_search_term(&self.search_term, &self.colors);
                                self.sessions.restore_selection_after_delete(
                                    was_searching,
                                    prev_search_idx,
                                    prev_top_idx,
                                );
                            },
                            Err(e) => {
                                self.show_error(&format!("Failed to kill session: {}", e));
                            },
                        }
                    } else {
                        self.show_error("Must select session before killing it.");
                    }
                    should_render = true;
                },
                BareKey::Char('d') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    let all_other_sessions = self.sessions.all_other_sessions();
                    if all_other_sessions.is_empty() {
                        self.show_error("No other sessions to kill. Quit to kill the current one.");
                    } else {
                        self.show_kill_all_sessions_warning = true;
                    }
                    should_render = true;
                },
                BareKey::Char('x') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    if !app_panel::is_enabled() {
                        disconnect_other_clients()
                    }
                },
                BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    if !self.search_term.is_empty() {
                        self.search_term.clear();
                        self.sessions
                            .update_search_term(&self.search_term, &self.colors);
                        self.reset_selected_index();
                    } else if !self.is_welcome_screen {
                        self.reset_selected_index();
                        close_self();
                    }
                    should_render = true;
                },
                BareKey::Tab if key.has_no_modifiers() => {
                    self.toggle_active_screen();
                    should_render = true;
                },
                BareKey::Esc if key.has_no_modifiers() => {
                    if self.renaming_session_name.is_some() {
                        self.renaming_session_name = None;
                        should_render = true;
                    } else if !self.is_welcome_screen {
                        close_self();
                    }
                },
                BareKey::Char('a') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    if !self.is_welcome_screen {
                        // we don't want to save welcome screen sessions
                        if let Err(e) = save_session() {
                            self.show_error(&format!("Couldn't save session: {}", e));
                        }
                    }
                },
                _ => {},
            }
        }
        should_render
    }
    fn handle_resurrect_session_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        match key.bare_key {
            BareKey::Down if key.has_no_modifiers() => {
                self.resurrectable_sessions.move_selection_down();
                should_render = true;
            },
            BareKey::Up if key.has_no_modifiers() => {
                self.resurrectable_sessions.move_selection_up();
                should_render = true;
            },
            BareKey::Enter if key.has_no_modifiers() => {
                self.handle_selection();
                should_render = true;
            },
            BareKey::Char(character) if key.has_no_modifiers() => {
                if character == '\n' {
                    self.handle_selection();
                } else {
                    self.resurrectable_sessions.handle_character(character);
                }
                should_render = true;
            },
            BareKey::Backspace if key.has_no_modifiers() => {
                self.resurrectable_sessions.handle_backspace();
                should_render = true;
            },
            BareKey::Char('w') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.active_screen = ActiveScreen::NewSession;
                should_render = true;
            },
            BareKey::Tab if key.has_no_modifiers() => {
                self.toggle_active_screen();
                should_render = true;
            },
            BareKey::Delete if key.has_no_modifiers() => {
                self.resurrectable_sessions.delete_selected_session();
                should_render = true;
            },
            BareKey::Char('d') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.resurrectable_sessions
                    .show_delete_all_sessions_warning();
                should_render = true;
            },
            BareKey::Esc if key.has_no_modifiers() => {
                if !self.is_welcome_screen {
                    close_self();
                }
            },
            _ => {},
        }
        should_render
    }
    fn handle_single_screen_key(&mut self, key: KeyWithModifier) -> bool {
        match self.single_screen_state.mode {
            SingleScreenMode::SearchAndSelect => self.handle_single_screen_search_key(key),
            SingleScreenMode::SelectingLayout => self.handle_single_screen_layout_key(key),
        }
    }
    fn handle_single_screen_search_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;

        if let Some(name) = self.pending_session_stop.clone() {
            match key.bare_key {
                BareKey::Char('y') if key.has_no_modifiers() => {
                    self.pending_session_stop = None;
                    self.delete_session_target(DeleteTarget::Active(name));
                },
                BareKey::Char('n') | BareKey::Esc if key.has_no_modifiers() => {
                    self.pending_session_stop = None;
                },
                BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    self.pending_session_stop = None;
                },
                _ => return false,
            }
            return true;
        }

        // Handle kill-all warning overlay first
        if self.show_kill_all_sessions_warning {
            match key.bare_key {
                BareKey::Char('y') if key.has_no_modifiers() => {
                    let all_other_sessions = self.sessions.all_other_sessions();
                    let previous_index = self.single_screen_state.selected_index;
                    match kill_sessions(&all_other_sessions) {
                        Ok(()) => {
                            self.sessions
                                .session_ui_infos
                                .retain(|s| !all_other_sessions.contains(&s.name));
                            self.single_screen_state.update_search_term(
                                &self.sessions.session_ui_infos,
                                &self.resurrectable_sessions.all_resurrectable_sessions,
                            );
                            self.single_screen_state
                                .restore_selection_after_delete(previous_index);
                        },
                        Err(e) => {
                            self.show_error(&format!("Failed to kill sessions: {}", e));
                        },
                    }
                    self.show_kill_all_sessions_warning = false;
                    should_render = true;
                },
                BareKey::Char('n') | BareKey::Esc if key.has_no_modifiers() => {
                    self.show_kill_all_sessions_warning = false;
                    should_render = true;
                },
                BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    self.show_kill_all_sessions_warning = false;
                    should_render = true;
                },
                _ => {},
            }
            return should_render;
        }

        // Handle rename overlay
        if self.renaming_session_name.is_some() {
            match key.bare_key {
                BareKey::Enter if key.has_no_modifiers() => {
                    self.handle_selection();
                    should_render = true;
                },
                BareKey::Char(c) if key.has_no_modifiers() => {
                    if c == '\n' {
                        self.handle_selection();
                    } else if let Some(name) = self.renaming_session_name.as_mut() {
                        name.push(c);
                    }
                    should_render = true;
                },
                BareKey::Backspace if key.has_no_modifiers() => {
                    if let Some(name) = self.renaming_session_name.as_mut() {
                        if name.is_empty() {
                            self.renaming_session_name = None;
                        } else {
                            zellij_tile::graphemes::pop_cluster(name);
                        }
                    }
                    should_render = true;
                },
                BareKey::Esc if key.has_no_modifiers() => {
                    self.renaming_session_name = None;
                    should_render = true;
                },
                _ => {},
            }
            return should_render;
        }

        match key.bare_key {
            BareKey::Char(character) if key.has_no_modifiers() => {
                if character == '\n' {
                    self.handle_selection();
                } else {
                    self.single_screen_state.search_term.push(character);
                    self.single_screen_state.update_search_term(
                        &self.sessions.session_ui_infos,
                        &self.resurrectable_sessions.all_resurrectable_sessions,
                    );
                }
                should_render = true;
            },
            BareKey::Backspace if key.has_no_modifiers() => {
                zellij_tile::graphemes::pop_cluster(&mut self.single_screen_state.search_term);
                self.single_screen_state.update_search_term(
                    &self.sessions.session_ui_infos,
                    &self.resurrectable_sessions.all_resurrectable_sessions,
                );
                should_render = true;
            },
            BareKey::Enter if key.has_no_modifiers() => {
                self.handle_selection();
                should_render = true;
            },
            BareKey::Down if key.has_no_modifiers() => {
                self.single_screen_state.move_selection_down();
                should_render = true;
            },
            BareKey::Up if key.has_no_modifiers() => {
                self.single_screen_state.move_selection_up();
                should_render = true;
            },
            BareKey::Tab if key.has_no_modifiers() => {
                self.single_screen_state.tab_complete(
                    &self.sessions.session_ui_infos,
                    &self.resurrectable_sessions.all_resurrectable_sessions,
                );
                should_render = true;
            },
            BareKey::Char('r') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.renaming_session_name = Some(String::new());
                should_render = true;
            },
            BareKey::Char('l') if key.has_only_modifiers(&[KeyModifier::Ctrl]) => {
                self.single_screen_state.selected_index = None;
                self.single_screen_state.transition_to_layout_selection();
                should_render = true;
            },
            BareKey::Delete if key.has_no_modifiers() => {
                let selected = self
                    .single_screen_state
                    .get_selected_result()
                    .map(|r| r.as_delete_target());
                match selected {
                    Some(DeleteTarget::Active(name)) => {
                        self.pending_session_stop = Some(name);
                    },
                    Some(target) => self.delete_session_target(target),
                    None => {},
                }
                should_render = true;
            },
            BareKey::Char('d') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                let all_other_sessions = self.sessions.all_other_sessions();
                if all_other_sessions.is_empty() {
                    self.show_error("No other sessions to kill. Quit to kill the current one.");
                } else {
                    self.show_kill_all_sessions_warning = true;
                }
                should_render = true;
            },
            BareKey::Char('x') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                if !app_panel::is_enabled() {
                    disconnect_other_clients();
                }
            },
            BareKey::Char('a') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                if !self.is_welcome_screen && !app_panel::is_enabled() {
                    if let Err(e) = save_session() {
                        self.show_error(&format!("Couldn't save session: {}", e));
                    }
                }
            },
            BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                if !self.single_screen_state.search_term.is_empty() {
                    self.single_screen_state.search_term.clear();
                    self.single_screen_state.update_search_term(
                        &self.sessions.session_ui_infos,
                        &self.resurrectable_sessions.all_resurrectable_sessions,
                    );
                } else if !self.is_welcome_screen {
                    close_self();
                }
                should_render = true;
            },
            BareKey::Esc if key.has_no_modifiers() => {
                if self.single_screen_state.selected_index.is_some() {
                    self.single_screen_state.selected_index = None;
                    should_render = true;
                } else if !self.is_welcome_screen {
                    close_self();
                }
            },
            _ => {},
        }
        should_render
    }
    fn handle_single_screen_layout_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        match key.bare_key {
            BareKey::Down if key.has_no_modifiers() => {
                self.single_screen_state.layout_list.move_selection_down();
                should_render = true;
            },
            BareKey::Up if key.has_no_modifiers() => {
                self.single_screen_state.layout_list.move_selection_up();
                should_render = true;
            },
            BareKey::Enter if key.has_no_modifiers() => {
                self.handle_selection();
                should_render = true;
            },
            BareKey::Char(character) if key.has_no_modifiers() => {
                if character == '\n' {
                    self.handle_selection();
                } else {
                    self.single_screen_state
                        .layout_list
                        .layout_search_term
                        .push(character);
                    self.single_screen_state.layout_list.update_search_term();
                }
                should_render = true;
            },
            BareKey::Backspace if key.has_no_modifiers() => {
                zellij_tile::graphemes::pop_cluster(
                    &mut self.single_screen_state.layout_list.layout_search_term,
                );
                self.single_screen_state.layout_list.update_search_term();
                should_render = true;
            },
            BareKey::Char('f') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                let request_id = Uuid::new_v4();
                let mut config = BTreeMap::new();
                let mut args = BTreeMap::new();
                self.request_ids.push(request_id.to_string());
                config.insert("request_id".to_owned(), request_id.to_string());
                args.insert("request_id".to_owned(), request_id.to_string());
                pipe_message_to_plugin(
                    MessageToPlugin::new("filepicker")
                        .with_plugin_url("filepicker")
                        .with_plugin_config(config)
                        .new_plugin_instance_should_have_pane_title(
                            "Select folder for the new session...",
                        )
                        .new_plugin_instance_should_be_focused()
                        .with_args(args),
                );
                should_render = true;
            },
            BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.single_screen_state.new_session_folder = None;
                should_render = true;
            },
            BareKey::Esc if key.has_no_modifiers() => {
                self.single_screen_state.transition_to_search();
                should_render = true;
            },
            _ => {},
        }
        should_render
    }
    fn handle_selection(&mut self) {
        match self.active_screen {
            ActiveScreen::NewSession => {
                if self.new_session_info.name().len() >= 108 {
                    // this is due to socket path limitations
                    // TODO: get this from Zellij (for reference: this is part of the interprocess
                    // package, we should get if from there if possible because it's configurable
                    // through the package)
                    self.show_error("Session name must be shorter than 108 bytes");
                    return;
                } else if self.new_session_info.name().contains('/') {
                    self.show_error("Session name cannot contain '/'");
                    return;
                } else if self
                    .sessions
                    .has_forbidden_session(self.new_session_info.name())
                {
                    self.show_error("This session exists and web clients cannot attach to it.");
                    return;
                }
                self.new_session_info.handle_selection(&self.session_name);
            },
            ActiveScreen::AttachToSession => {
                if let Some(renaming_session_name) = &self.renaming_session_name.take() {
                    if renaming_session_name.is_empty() {
                        self.show_error("New name must not be empty.");
                        return; // so that we don't hide self
                    } else if self.session_name.as_ref() == Some(renaming_session_name) {
                        // noop - we're already called that!
                        return; // so that we don't hide self
                    } else if self.sessions.has_session(&renaming_session_name) {
                        self.show_error("A session by this name already exists.");
                        return; // so that we don't hide self
                    } else if self
                        .resurrectable_sessions
                        .has_session(&renaming_session_name)
                    {
                        self.show_error("A resurrectable session by this name already exists.");
                        return; // s that we don't hide self
                    } else {
                        if renaming_session_name.contains('/') {
                            self.show_error("Session names cannot contain '/'");
                            return;
                        }
                        self.update_current_session_name_in_ui(&renaming_session_name);
                        rename_session(&renaming_session_name);
                        return; // s that we don't hide self
                    }
                }
                let mut switched_session = false;
                if let Some(selected_session_name) = self.sessions.get_selected_session_name() {
                    let selected_tab = self.sessions.get_selected_tab_position();
                    let selected_pane = self.sessions.get_selected_pane_id();
                    let is_current_session = self.sessions.selected_is_current_session();
                    if is_current_session {
                        if let Some((pane_id, is_plugin)) = selected_pane {
                            if is_plugin {
                                focus_plugin_pane(pane_id, true, false);
                            } else {
                                focus_terminal_pane(pane_id, true, false);
                            }
                        } else if let Some(tab_position) = selected_tab {
                            go_to_tab(tab_position as u32);
                        } else {
                            self.show_error("Already attached...");
                        }
                    } else {
                        switch_session_with_focus(
                            &selected_session_name,
                            selected_tab,
                            selected_pane,
                        );
                        switched_session = true;
                    }
                }
                self.reset_selected_index();
                self.search_term.clear();
                self.sessions
                    .update_search_term(&self.search_term, &self.colors);
                if self.is_welcome_screen {
                    // the welcome screen has done its job and now we need to quit this temporary
                    // session so as not to leave garbage sessions behind
                    quit_zellij();
                } else if switched_session {
                    close_self();
                } else {
                    hide_self();
                }
            },
            ActiveScreen::ResurrectSession => {
                if let Some(session_name_to_resurrect) =
                    self.resurrectable_sessions.get_selected_session_name()
                {
                    switch_session(Some(&session_name_to_resurrect));
                    if self.is_welcome_screen {
                        // the welcome screen has done its job and now we need to quit this temporary
                        // session so as not to leave garbage sessions behind
                        quit_zellij();
                    } else {
                        close_self();
                    }
                }
            },
            ActiveScreen::SingleScreen => {
                // Handle rename
                if let Some(renaming_session_name) = &self.renaming_session_name.take() {
                    if renaming_session_name.is_empty() {
                        self.show_error("New name must not be empty.");
                        return;
                    } else if self.session_name.as_ref() == Some(renaming_session_name) {
                        return;
                    } else if self.sessions.has_session(&renaming_session_name) {
                        self.show_error("A session by this name already exists.");
                        return;
                    } else if self
                        .resurrectable_sessions
                        .has_session(&renaming_session_name)
                    {
                        self.show_error("A resurrectable session by this name already exists.");
                        return;
                    } else {
                        if renaming_session_name.contains('/') {
                            self.show_error("Session names cannot contain '/'");
                            return;
                        }
                        self.update_current_session_name_in_ui(&renaming_session_name);
                        rename_session(&renaming_session_name);
                        return;
                    }
                }

                match self.single_screen_state.mode {
                    SingleScreenMode::SearchAndSelect => {
                        if let Some(result) = self.single_screen_state.get_selected_result() {
                            // User navigated to a specific result
                            let session_name = result.session_name().to_owned();
                            let mut switched_session = false;
                            match result {
                                UnifiedSearchResult::ActiveSession {
                                    is_current_session, ..
                                } => {
                                    if *is_current_session {
                                        self.show_error("Already attached...");
                                    } else {
                                        self.open_session(&session_name, true);
                                        switched_session = true;
                                    }
                                },
                                UnifiedSearchResult::ResurrectableSession { .. } => {
                                    self.open_session(&session_name, false);
                                    switched_session = true;
                                },
                            }
                            self.single_screen_state.search_term.clear();
                            self.single_screen_state.selected_index = None;
                            if self.is_welcome_screen {
                                quit_zellij();
                            } else if switched_session {
                                close_self();
                            } else {
                                hide_self();
                            }
                        } else {
                            // No navigation - use typed name
                            let typed_name = self.single_screen_state.search_term.clone();

                            // Validate name
                            if typed_name.len() >= 108 {
                                self.show_error("Session name must be shorter than 108 bytes");
                                return;
                            }
                            if typed_name.contains('/') {
                                self.show_error("Session name cannot contain '/'");
                                return;
                            }
                            if self.sessions.has_forbidden_session(&typed_name) {
                                self.show_error(
                                    "This session exists and web clients cannot attach to it.",
                                );
                                return;
                            }

                            // Check exact match against active sessions
                            if self.sessions.has_session(&typed_name) {
                                if self.session_name.as_deref() == Some(&typed_name) {
                                    self.show_error("Already attached...");
                                } else {
                                    self.open_session(&typed_name, true);
                                    if self.is_welcome_screen {
                                        quit_zellij();
                                    } else {
                                        close_self();
                                    }
                                }
                                return;
                            }
                            // Check exact match against resurrectable sessions
                            if self.resurrectable_sessions.has_session(&typed_name) {
                                self.open_session(&typed_name, false);
                                if self.is_welcome_screen {
                                    quit_zellij();
                                } else {
                                    close_self();
                                }
                                return;
                            }
                            if self.panel.size.is_some() {
                                self.create_session_with_default_layout();
                            } else {
                                self.single_screen_state.transition_to_layout_selection();
                            }
                        }
                    },
                    SingleScreenMode::SelectingLayout => {
                        let new_session_name = if self.single_screen_state.search_term.is_empty() {
                            None
                        } else {
                            Some(self.single_screen_state.search_term.as_str())
                        };
                        let layout = self.single_screen_state.layout_list.selected_layout_info();
                        let cwd = self.single_screen_state.new_session_folder.clone();

                        let switched_session =
                            new_session_name != self.session_name.as_ref().map(|s| s.as_str());
                        if switched_session {
                            match layout {
                                Some(layout_info) => {
                                    self.switch_to_new_session(
                                        new_session_name,
                                        Some(layout_info),
                                        cwd,
                                    );
                                },
                                None => {
                                    self.switch_to_new_session(new_session_name, None, None);
                                },
                            }
                        }
                        self.single_screen_state.search_term.clear();
                        self.single_screen_state.transition_to_search();
                        if self.is_welcome_screen {
                            quit_zellij();
                        } else if switched_session {
                            close_self();
                        } else {
                            hide_self();
                        }
                    },
                }
            },
        }
    }
    fn create_session_with_default_layout(&mut self) {
        let typed_name = self.single_screen_state.search_term.clone();
        let name = if typed_name.is_empty() {
            None
        } else {
            Some(typed_name.as_str())
        };
        let cwd = self.single_screen_state.new_session_folder.clone();
        let layout = self.default_layout_info();
        self.switch_to_new_session(name, Some(layout), cwd);
        self.single_screen_state.search_term.clear();
        if self.is_welcome_screen {
            quit_zellij();
        } else {
            close_self();
        }
    }
    fn switch_to_new_session(
        &self,
        name: Option<&str>,
        layout: Option<LayoutInfo>,
        cwd: Option<std::path::PathBuf>,
    ) {
        switch_session_with_options(ConnectToSession {
            name: name.map(|n| n.to_owned()),
            layout,
            cwd,
            switch_context: if self.is_welcome_screen {
                Some(SwitchContext::with_startup_card(StartupCard::Never))
            } else {
                None
            },
            ..Default::default()
        });
    }
    fn default_layout_info(&self) -> LayoutInfo {
        let configured = read_config()
            .setting(SettingKey::DefaultLayout)
            .and_then(|setting| setting.current_value.clone())
            .map(|value| value.trim().trim_matches('"').to_owned())
            .filter(|value| !value.is_empty() && value != "welcome");
        let wanted = configured.unwrap_or_else(|| "default".to_owned());
        let stem = |name: &str| {
            std::path::Path::new(name)
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_else(|| name.to_owned())
        };
        self.single_screen_state
            .layout_list
            .layout_list
            .iter()
            .find(|layout| layout.name() == wanted || stem(layout.name()) == stem(&wanted))
            .cloned()
            .unwrap_or_else(|| LayoutInfo::BuiltIn("default".to_owned()))
    }
    fn toggle_active_screen(&mut self) {
        self.active_screen = match self.active_screen {
            ActiveScreen::NewSession => ActiveScreen::AttachToSession,
            ActiveScreen::AttachToSession => ActiveScreen::ResurrectSession,
            ActiveScreen::ResurrectSession => ActiveScreen::NewSession,
            ActiveScreen::SingleScreen => ActiveScreen::SingleScreen, // no-op
        };
    }
    fn show_error(&mut self, error_text: &str) {
        self.error = Some(error_text.to_owned());
    }
    fn update_current_session_name_in_ui(&mut self, new_name: &str) {
        if let Some(old_session_name) = self.session_name.as_ref() {
            self.sessions
                .update_session_name(&old_session_name, new_name);
        }
        self.session_name = Some(new_name.to_owned());
    }

    fn update_session_infos(&mut self, session_infos: Vec<SessionInfo>) {
        let session_ui_infos: Vec<SessionUiInfo> = session_infos
            .iter()
            .filter_map(|s| {
                if self.is_web_client && !s.web_clients_allowed {
                    None
                } else if self.is_welcome_screen && s.is_current_session {
                    // do not display current session if we're the welcome screen
                    // because:
                    // 1. attaching to the welcome screen from the welcome screen is not a thing
                    // 2. it can cause issues on the web (since we're disconnecting and
                    //    reconnecting to a session we just closed by disconnecting...)
                    None
                } else {
                    Some(SessionUiInfo::from_session_info(s))
                }
            })
            .collect();
        let forbidden_sessions: Vec<SessionUiInfo> = session_infos
            .iter()
            .filter_map(|s| {
                if self.is_web_client && !s.web_clients_allowed {
                    Some(SessionUiInfo::from_session_info(s))
                } else {
                    None
                }
            })
            .collect();
        let current_session_name = session_infos.iter().find_map(|s| {
            if s.is_current_session {
                Some(s.name.clone())
            } else {
                None
            }
        });
        if let Some(current_session_name) = current_session_name {
            self.session_name = Some(current_session_name);
        }
        self.sessions
            .set_sessions(session_ui_infos, forbidden_sessions);
    }
    fn main_menu_size(&self, rows: usize, cols: usize) -> (usize, usize, usize, usize) {
        let y = self.top_offset;
        (0, y, cols, rows.saturating_sub(y))
    }
    fn render_single_screen_folder_prompt(&self, x: usize, y: usize, max_cols: usize) {
        match self.single_screen_state.new_session_folder.as_ref() {
            Some(new_session_folder) => {
                let folder_prompt = "New session folder:";
                let new_session_folder_str = new_session_folder.display().to_string();
                let change_folder_shortcut = self.colors.shortcuts("<Ctrl f>");
                let reset_folder_shortcut = self.colors.shortcuts("<Ctrl c>");
                if max_cols >= folder_prompt.len() + new_session_folder_str.len() + 30 {
                    print!(
                        "\u{1b}[m\u{1b}[{};{}H{} {} ({} to change, {} to reset)",
                        y + 1,
                        x + 1,
                        self.colors.session_name_prompt(folder_prompt),
                        self.colors
                            .session_and_folder_entry(&new_session_folder_str),
                        change_folder_shortcut,
                        reset_folder_shortcut,
                    );
                } else {
                    print!(
                        "\u{1b}[m\u{1b}[{};{}H{} {} ({}/{})",
                        y + 1,
                        x + 1,
                        self.colors.session_name_prompt("Folder:"),
                        self.colors
                            .session_and_folder_entry(&new_session_folder_str),
                        change_folder_shortcut,
                        reset_folder_shortcut,
                    );
                }
            },
            None => {
                let folder_prompt = "New session folder:";
                let change_folder_shortcut = self.colors.shortcuts("<Ctrl f>");
                print!(
                    "\u{1b}[m\u{1b}[{};{}H{} ({} to set)",
                    y + 1,
                    x + 1,
                    self.colors.session_name_prompt(folder_prompt),
                    change_folder_shortcut,
                );
            },
        }
    }
    fn delete_session_target(&mut self, target: DeleteTarget) {
        let previous_index = self.single_screen_state.selected_index;
        let outcome: Result<(), String> = match &target {
            DeleteTarget::Active(name) => kill_sessions(&[name.clone()]).map(|()| {
                self.sessions.session_ui_infos.retain(|s| s.name != *name);
            }),
            DeleteTarget::Resurrectable(name) => delete_dead_session(name).map(|()| {
                self.resurrectable_sessions
                    .all_resurrectable_sessions
                    .retain(|(n, _)| n != name);
            }),
        };
        match outcome {
            Ok(()) => {
                self.single_screen_state.update_search_term(
                    &self.sessions.session_ui_infos,
                    &self.resurrectable_sessions.all_resurrectable_sessions,
                );
                self.single_screen_state
                    .restore_selection_after_delete(previous_index);
            },
            Err(e) => {
                self.show_error(&format!("Failed to delete session: {}", e));
            },
        }
    }
    pub(crate) fn render_kill_all_sessions_warning(
        &self,
        rows: usize,
        columns: usize,
        x: usize,
        y: usize,
    ) {
        if rows == 0 || columns == 0 {
            return;
        }
        let session_count = self.sessions.all_other_sessions().len();
        let session_count_len = session_count.to_string().chars().count();
        let warning_description_text = format!("This will kill {} active sessions", session_count);
        let confirmation_text = "Are you sure? (y/n)";
        let warning_y_location = y + (rows / 2).saturating_sub(1);
        let confirmation_y_location = y + (rows / 2) + 1;
        let warning_x_location =
            x + columns.saturating_sub(warning_description_text.chars().count()) / 2;
        let confirmation_x_location =
            x + columns.saturating_sub(confirmation_text.chars().count()) / 2;
        print_text_with_coordinates(
            Text::from(warning_description_text).color_range(0, 15..16 + session_count_len),
            warning_x_location,
            warning_y_location,
            None,
            None,
        );
        print_text_with_coordinates(
            Text::from(confirmation_text).color_indices(2, vec![15, 17]),
            confirmation_x_location,
            confirmation_y_location,
            None,
            None,
        );
    }
}

pub(crate) fn stop_session_warning_lines(name: &str) -> (String, String) {
    (
        format!("This will stop the session {}", sanitize_display_text(name)),
        "Are you sure? (y/n)".to_owned(),
    )
}

fn render_stop_session_warning(name: &str, rows: usize, columns: usize, x: usize, y: usize) {
    if rows == 0 || columns == 0 {
        return;
    }
    let (warning, confirmation) = stop_session_warning_lines(name);
    let prefix_len = "This will stop the session ".chars().count();
    let warning_x = x + columns.saturating_sub(warning.chars().count()) / 2;
    let confirmation_x = x + columns.saturating_sub(confirmation.chars().count()) / 2;
    print_text_with_coordinates(
        Text::from(warning.as_str()).color_range(0, prefix_len..warning.chars().count()),
        warning_x,
        y + (rows / 2).saturating_sub(1),
        Some(columns),
        None,
    );
    print_text_with_coordinates(
        Text::from(confirmation.as_str()).color_indices(2, vec![15, 17]),
        confirmation_x,
        y + (rows / 2) + 1,
        None,
        None,
    );
}

impl State {
    fn open_session(&self, name: &str, is_running: bool) {
        self.open_session_at(name, is_running, None, None);
    }
    fn open_session_at(
        &self,
        name: &str,
        is_running: bool,
        tab: Option<usize>,
        pane: Option<(u32, bool)>,
    ) {
        if self.is_welcome_screen {
            if is_running {
                switch_session_with_focus(name, tab, pane);
            } else {
                switch_session(Some(name));
            }
        } else if !self.panel.command_submitted {
            switch_session_and_close_current(name, tab, pane);
        } else if is_running {
            switch_session_with_focus(name, tab, pane);
        } else {
            switch_session(Some(name));
        }
    }
    fn go_to_size(&mut self, size: PanelSize) {
        let was_card = self.panel.size == Some(PanelSize::Card);
        self.panel.apply_size(size);
        if size != PanelSize::Card {
            if was_card {
                self.apply_suggestions_to_list();
            }
            if self.inbox.tab == BrowserTab::Sessions {
                self.preselect_previous_session();
            }
            self.request_preview_for_selection();
        }
    }
    fn size_down(&mut self) {
        if self.is_welcome_screen {
            return;
        }
        match self.panel.size {
            Some(PanelSize::Manager) => self.panel.close(),
            Some(PanelSize::Card) => self.panel.close(),
            None => {},
        }
    }
    fn handle_panel_pipe(&mut self, pipe_message: &PipeMessage) -> Option<bool> {
        match pipe_message.name.as_str() {
            "expand" => {
                match self.panel.size {
                    Some(PanelSize::Card) => self.open_browser(None),
                    _ => {},
                }
                Some(true)
            },
            "close" => {
                self.panel.close();
                Some(false)
            },
            "focus_card" => {
                if self.panel.size == Some(PanelSize::Card) {
                    self.panel.popup_focused = true;
                    self.open_browser(None);
                }
                Some(true)
            },
            "escape" => {
                if self.panel.size == Some(PanelSize::Card) {
                    self.panel.close();
                }
                Some(false)
            },
            _ => None,
        }
    }
    fn apply_suggestions_to_list(&mut self) {
        let Some(suggestions) = self.panel.suggestions.as_ref() else {
            return;
        };
        let mut running = vec![];
        let mut resumable = vec![];
        let previous = self.inbox.previous_session();
        let mut ordered: Vec<&SessionSuggestion> = suggestions.suggestions.iter().collect();
        ordered.sort_by_key(|s| {
            if previous.as_deref() == Some(s.name.as_str()) && s.is_running {
                0
            } else if s.is_running {
                1
            } else {
                2
            }
        });
        let ordered_names: Vec<String> = ordered.iter().map(|s| s.name.clone()).collect();
        for suggestion in &suggestions.suggestions {
            if suggestion.is_running {
                let tab_count = suggestion.tabs.len();
                let pane_count = suggestion
                    .tabs
                    .iter()
                    .flat_map(|t| t.panes.iter())
                    .filter(|p| !p.is_plugin)
                    .count();
                let connected_users = suggestion.connected_clients;
                let tabs = (0..tab_count)
                    .map(|position| crate::ui::TabUiInfo {
                        name: format!("Tab #{}", position + 1),
                        panes: if position == 0 {
                            (0..pane_count)
                                .map(|pane_id| crate::ui::PaneUiInfo {
                                    name: String::new(),
                                    exit_code: None,
                                    pane_id: pane_id as u32,
                                    is_plugin: false,
                                })
                                .collect()
                        } else {
                            vec![]
                        },
                        position,
                    })
                    .collect();
                running.push(SessionUiInfo {
                    name: suggestion.name.clone(),
                    tabs,
                    connected_users,
                    is_current_session: false,
                    creation_time: std::time::Duration::from_secs(suggestion.created_secs_ago),
                });
            } else {
                resumable.push((
                    suggestion.name.clone(),
                    std::time::Duration::from_secs(suggestion.created_secs_ago),
                ));
            }
        }
        for suggestion in &suggestions.suggestions {
            if suggestion.is_running && !suggestion.tabs.is_empty() {
                self.panel
                    .tree
                    .set_children_from_tabs(&suggestion.name, &suggestion.tabs);
            }
        }
        self.single_screen_state.ordered_names = Some(ordered_names);
        self.sessions.set_sessions(running, vec![]);
        self.resurrectable_sessions.update(resumable);
        self.single_screen_state.update_search_term(
            &self.sessions.session_ui_infos,
            &self.resurrectable_sessions.all_resurrectable_sessions,
        );
    }
    fn apply_session_preview(&mut self, preview: SessionPreview) -> bool {
        let mut should_render = false;
        if self.panel.preview_session.as_deref() == Some(preview.session_name.as_str())
            && self.panel.preview.as_ref() != Some(&preview)
        {
            self.panel.preview = Some(preview);
            self.panel.saved_preview = None;
            should_render = true;
        }
        should_render
    }
    fn selected_session(&self) -> Option<(String, bool, bool)> {
        self.single_screen_state
            .get_selected_result()
            .map(|result| match result {
                UnifiedSearchResult::ActiveSession {
                    session_name,
                    is_current_session,
                    ..
                } => (session_name.clone(), true, *is_current_session),
                UnifiedSearchResult::ResurrectableSession { session_name, .. } => {
                    (session_name.clone(), false, false)
                },
            })
    }
    fn request_preview_for_selection(&mut self) {
        if !matches!(self.panel.size, Some(PanelSize::Manager)) {
            return;
        }
        if self.on_exited_header() {
            self.panel.clear_preview();
            return;
        }
        match self.selected_session() {
            Some((name, is_running, is_current)) => {
                let (tab, pane) = if self.panel.tree.node_session == name {
                    match self.panel.tree.node {
                        session_tree::TreeNode::Exited | session_tree::TreeNode::Session => {
                            (None, None)
                        },
                        session_tree::TreeNode::Tab(tab) => (Some(tab), None),
                        session_tree::TreeNode::Pane(tab, pane) => (Some(tab), Some(pane)),
                    }
                } else {
                    (None, None)
                };
                self.panel
                    .request_tree_preview(&name, is_running, is_current, tab, pane);
                if is_current {
                    self.preview_current_session();
                }
                if is_running {
                    self.panel.arm_preview_timer();
                }
            },
            None => self.panel.clear_preview(),
        }
    }
    fn preview_current_session(&mut self) {
        let focused = self
            .current_panes
            .panes
            .values()
            .flatten()
            .find(|p| p.is_focused && !p.is_plugin)
            .map(|p| p.id);
        let contents = focused
            .and_then(|id| get_pane_scrollback(PaneId::Terminal(id), false).ok())
            .map(|c| c.viewport.join("\n"))
            .unwrap_or_default();
        self.panel.preview = Some(SessionPreview {
            session_name: self.session_name.clone().unwrap_or_default(),
            contents,
            ..Default::default()
        });
    }
    fn refresh_selected_preview(&mut self) -> bool {
        if !self.is_visible || !matches!(self.panel.size, Some(PanelSize::Manager)) {
            return false;
        }
        match self.selected_session() {
            Some((name, true, is_current)) => {
                if is_current {
                    self.preview_current_session();
                } else {
                    get_session_preview(&name, self.panel.preview_tab, self.panel.preview_pane_id);
                }
                self.panel.arm_preview_timer();
                is_current
            },
            _ => false,
        }
    }
    fn refresh_layout_list(&mut self) {
        let previous_selection = self.single_screen_state.layout_list.selected_layout_index;
        let previous_search_term = self
            .single_screen_state
            .layout_list
            .layout_search_term
            .clone();
        self.single_screen_state.layout_list = self.new_session_info.get_layout_list_clone();
        self.single_screen_state.layout_list.layout_search_term = previous_search_term;
        self.single_screen_state.layout_list.update_search_term();
        self.single_screen_state.layout_list.selected_layout_index =
            previous_selection.min(self.single_screen_state.layout_list.max_index());
    }
    fn handle_panel_key(&mut self, key: &KeyWithModifier) -> Option<bool> {
        let size = self.panel.size?;
        if self.renaming_session_name.is_some()
            || self.show_kill_all_sessions_warning
            || self.pending_session_stop.is_some()
            || self.single_screen_state.mode != single_screen::SingleScreenMode::SearchAndSelect
        {
            return None;
        }
        match size {
            PanelSize::Card => self.handle_card_key(key),
            PanelSize::Manager if self.on_exited_header() && !self.inbox_is_shown() => {
                match key.bare_key {
                    BareKey::Enter if key.has_no_modifiers() => {
                        self.open_tree_child();
                        Some(true)
                    },
                    BareKey::Delete => Some(false),
                    BareKey::Char('r') if key.has_modifiers(&[KeyModifier::Ctrl]) => Some(false),
                    _ => self.handle_manager_key(key),
                }
            },
            PanelSize::Manager => self.handle_manager_key(key),
        }
    }
    fn handle_manager_key(&mut self, key: &KeyWithModifier) -> Option<bool> {
        {
            match key.bare_key {
                BareKey::Tab
                    if key.has_no_modifiers()
                        && self.shows_browser_tabs()
                        && self.single_screen_state.search_term.is_empty() =>
                {
                    self.switch_browser_tab(BrowserTab::Inbox);
                    Some(true)
                },
                BareKey::Esc if key.has_no_modifiers() => {
                    if self.single_screen_state.selected_index.is_some()
                        && !self.single_screen_state.search_term.is_empty()
                    {
                        return None;
                    }
                    self.size_down();
                    Some(true)
                },
                BareKey::Up if key.has_no_modifiers() => {
                    self.tree_move(-1);
                    Some(true)
                },
                BareKey::Down if key.has_no_modifiers() => {
                    self.tree_move(1);
                    Some(true)
                },
                BareKey::PageUp if key.has_no_modifiers() => {
                    let page = self.panel.tree.page_rows.max(1) as isize;
                    self.tree_move(-page);
                    Some(true)
                },
                BareKey::PageDown if key.has_no_modifiers() => {
                    let page = self.panel.tree.page_rows.max(1) as isize;
                    self.tree_move(page);
                    Some(true)
                },
                BareKey::Char('u') if key.has_only_modifiers(&[KeyModifier::Ctrl]) => {
                    let half = (self.panel.tree.page_rows / 2).max(1) as isize;
                    self.tree_move(-half);
                    Some(true)
                },
                BareKey::Char('d') if key.has_only_modifiers(&[KeyModifier::Ctrl]) => {
                    let half = (self.panel.tree.page_rows / 2).max(1) as isize;
                    self.tree_move(half);
                    Some(true)
                },
                BareKey::Home if key.has_no_modifiers() => {
                    self.tree_select(0);
                    Some(true)
                },
                BareKey::End if key.has_no_modifiers() => {
                    let last = self.panel.tree.rows.len().saturating_sub(1);
                    self.tree_select(last);
                    Some(true)
                },
                BareKey::Right if key.has_no_modifiers() => {
                    self.tree_right();
                    Some(true)
                },
                BareKey::Left if key.has_no_modifiers() => {
                    self.tree_left();
                    Some(true)
                },
                BareKey::Enter if key.has_no_modifiers() => {
                    if self.open_tree_child() {
                        Some(false)
                    } else {
                        None
                    }
                },
                _ => None,
            }
        }
    }
    fn no_other_sessions(&self) -> bool {
        self.panel
            .suggestions
            .as_ref()
            .map(|s| s.suggestions.is_empty())
            .unwrap_or(false)
    }
    fn on_exited_header(&self) -> bool {
        self.tree_cursor()
            .and_then(|cursor| self.panel.tree.rows.get(cursor))
            .map(|row| row.node == session_tree::TreeNode::Exited)
            .unwrap_or(false)
    }
    fn tree_cursor(&self) -> Option<usize> {
        self.panel.tree.cursor_position(
            &self.panel.tree.rows,
            self.single_screen_state.selected_index,
        )
    }
    fn tree_select(&mut self, index: usize) {
        self.tree_point_at(index);
        self.panel.tree.follow = true;
    }
    fn tree_point_at(&mut self, index: usize) {
        let Some(row) = self.panel.tree.rows.get(index).cloned() else {
            return;
        };
        self.single_screen_state.selected_index = Some(row.original_index);
        self.panel.tree.node = row.node;
        self.panel.tree.node_session = row.session;
        self.request_preview_for_selection();
    }
    fn tree_move(&mut self, delta: isize) {
        let len = self.panel.tree.rows.len();
        if len == 0 {
            return;
        }
        let next = match self.tree_cursor() {
            Some(cursor) => (cursor as isize + delta).clamp(0, len as isize - 1) as usize,
            None if delta < 0 => len - 1,
            None => 0,
        };
        self.tree_select(next);
    }
    fn tree_right(&mut self) {
        let Some(cursor) = self.tree_cursor() else {
            self.tree_move(1);
            return;
        };
        let Some(row) = self.panel.tree.rows.get(cursor).cloned() else {
            return;
        };
        if !row.expandable {
            return;
        }
        if row.expanded {
            let has_child = self
                .panel
                .tree
                .rows
                .get(cursor + 1)
                .map(|next| next.depth > row.depth)
                .unwrap_or(false);
            if has_child {
                self.tree_select(cursor + 1);
            }
        } else {
            self.panel.tree.toggle(&row);
            self.panel.tree.follow = true;
        }
    }
    fn tree_left(&mut self) {
        let Some(cursor) = self.tree_cursor() else {
            return;
        };
        let Some(row) = self.panel.tree.rows.get(cursor).cloned() else {
            return;
        };
        if row.expandable && row.expanded {
            self.panel.tree.toggle(&row);
            self.panel.tree.follow = true;
            return;
        }
        if row.depth == 0 {
            return;
        }
        let parent = self.panel.tree.rows[..cursor]
            .iter()
            .rposition(|r| r.depth < row.depth);
        if let Some(parent) = parent {
            self.tree_select(parent);
        }
    }
    fn open_tree_child(&mut self) -> bool {
        use session_tree::TreeNode;
        let Some(row) = self
            .tree_cursor()
            .and_then(|c| self.panel.tree.rows.get(c).cloned())
        else {
            return false;
        };
        let (tab, pane) = match row.node {
            TreeNode::Exited => {
                self.panel.tree.toggle(&row);
                self.panel.tree.follow = true;
                return true;
            },
            TreeNode::Session => return false,
            TreeNode::Tab(tab) => (tab, None),
            TreeNode::Pane(tab, pane) => (tab, Some(pane)),
        };
        let is_running = matches!(self.selected_session(), Some((_, true, _)));
        if !is_running {
            return false;
        }
        let pane_id = pane.and_then(|pane| {
            self.panel
                .tree
                .children
                .get(&row.session)
                .and_then(|tabs| tabs.get(tab))
                .and_then(|t| t.panes.get(pane))
                .map(|p| (p.id, p.is_plugin))
        });
        self.open_session_at(&row.session, true, Some(tab), pane_id);
        if self.is_welcome_screen {
            quit_zellij();
            return true;
        }
        self.panel.size = None;
        self.panel.report_size();
        close_self();
        true
    }
    fn tree_row_at(&self, line: isize, column: usize) -> Option<usize> {
        if line < 0 {
            return None;
        }
        let line = line as usize;
        self.panel
            .tree
            .row_targets
            .iter()
            .find(|(y, range, _)| *y == line && range.contains(&column))
            .map(|(_, _, index)| *index)
    }
    fn tree_marker_at(&self, line: isize, column: usize) -> Option<usize> {
        if line < 0 {
            return None;
        }
        let line = line as usize;
        self.panel
            .tree
            .marker_targets
            .iter()
            .find(|(y, x, _)| *y == line && *x == column)
            .map(|(_, _, index)| *index)
    }
    fn away_card_is_empty(&self) -> bool {
        self.panel.size == Some(PanelSize::Card)
            && self.panel.opened_at_startup
            && !self.panel.popup_focused
            && self.inbox.away_since_ms.is_some()
            && self.inbox.card_content(inbox::now_ms()).items.is_empty()
    }

    fn resize_card_popup(&self) {
        if self.panel.size == Some(PanelSize::Card) && self.panel.is_popup {
            let screen_rows = self
                .panel
                .display
                .viewport_rows
                .max(self.panel.display.rows);
            let (width, height) = self.inbox.card_dimensions(screen_rows);
            set_popup_size(width, height);
        }
    }
    fn shows_browser_tabs(&self) -> bool {
        !self.is_welcome_screen
    }
    fn shows_ages(&self) -> bool {
        self.panel.size == Some(PanelSize::Card) || self.inbox_is_shown()
    }
    fn inbox_is_shown(&self) -> bool {
        self.panel.size == Some(PanelSize::Manager)
            && self.shows_browser_tabs()
            && self.inbox.tab == BrowserTab::Inbox
            && self.renaming_session_name.is_none()
    }
    fn running_session_count(&self) -> usize {
        self.inbox
            .global
            .as_ref()
            .map(|g| g.running())
            .unwrap_or_else(|| self.sessions.session_ui_infos.len())
    }
    fn settle_app_panel_view(&mut self, user_input: bool, attention: bool) -> bool {
        if !app_panel::is_enabled() {
            return false;
        }
        if user_input {
            self.app_panel_auto_pending = false;
        }
        if let Some(view) = self.app_panel_initial_view.take() {
            self.app_panel_auto_pending = view == "auto";
            self.show_app_panel_view(Some(&view));
            return true;
        }
        if attention && self.app_panel_auto_pending && self.inbox.counted_count() > 0 {
            self.app_panel_auto_pending = false;
            if self.inbox.tab != BrowserTab::Inbox {
                self.show_app_panel_view(Some("inbox"));
                return true;
            }
        }
        false
    }

    fn app_panel_sessions(&self) -> Vec<String> {
        let mut names: Vec<String> = match self.inbox.global.as_ref() {
            Some(global) => global.sessions.iter().map(|s| s.name.clone()).collect(),
            None => self
                .sessions
                .session_ui_infos
                .iter()
                .filter(|s| !s.is_current_session)
                .map(|s| s.name.clone())
                .collect(),
        };
        names.sort();
        names.dedup();
        names
    }

    fn render_app_quit(&self, rows: usize, cols: usize) {
        let lines = app_panel::quit_lines(&self.app_panel_sessions());
        let (x, y, shown) = app_panel::centered_block(&lines, rows, cols);
        let last = shown.len().saturating_sub(1);
        for (index, line) in shown.iter().enumerate() {
            let text = Text::from(line.as_str());
            let text = if index == 0 {
                text.color_range(2, ..)
            } else if index == last {
                text.color_substring(3, "<Enter>")
                    .color_substring(3, "<Esc>")
            } else if line.starts_with("  ") {
                text.color_range(1, ..)
            } else {
                text
            };
            print_text_with_coordinates(text, x, y + index, None, None);
        }
    }

    fn set_app_panel_title(&self, view: &str) {
        if let Some(plugin_id) = self.panel.own_plugin_id {
            rename_plugin_pane(plugin_id, &app_panel::title(view));
        }
    }

    fn show_app_panel_view(&mut self, view: Option<&str>) {
        self.app_quit = view == Some("quit");
        self.panel.size = Some(PanelSize::Manager);
        if self.app_quit {
            self.set_app_panel_title("Quit");
            return;
        }
        self.inbox.reply = None;
        self.single_screen_state.search_term.clear();
        self.single_screen_state.selected_index = None;
        let tab = match view {
            Some("sessions") => BrowserTab::Sessions,
            Some("inbox") => BrowserTab::Inbox,
            _ if self.inbox.counted_count() == 0 => BrowserTab::Sessions,
            _ => BrowserTab::Inbox,
        };
        self.switch_browser_tab(tab);
        if tab == BrowserTab::Inbox {
            self.inbox.select_next_counted();
        }
    }

    fn follow_app_panel_title(&self) {
        if app_panel::is_enabled() && !self.app_quit {
            self.set_app_panel_title(match self.inbox.tab {
                BrowserTab::Inbox => "Inbox",
                BrowserTab::Sessions => "Sessions",
            });
        }
    }

    fn switch_browser_tab(&mut self, tab: BrowserTab) {
        self.inbox.tab = tab;
        self.inbox.tab_chosen = true;
        self.inbox.reply = None;
        self.follow_app_panel_title();
        match tab {
            BrowserTab::Inbox => {
                self.inbox.request_preview();
                self.inbox.arm_timer();
            },
            BrowserTab::Sessions => {
                self.preselect_previous_session();
                self.request_preview_for_selection();
            },
        }
    }
    fn preselect_previous_session(&mut self) {
        let Some(previous) = self.inbox.previous_session() else {
            return;
        };
        if !self.single_screen_state.search_term.is_empty() {
            return;
        }
        if let Some(index) = self
            .single_screen_state
            .ordered_names
            .as_ref()
            .and_then(|names| names.iter().position(|name| *name == previous))
        {
            self.single_screen_state.selected_index = Some(index);
        }
    }
    fn open_browser(&mut self, select: Option<ItemKey>) {
        match select {
            Some(key) => {
                self.inbox.select_item(key);
                self.inbox.tab = BrowserTab::Inbox;
                self.inbox.tab_chosen = true;
            },
            None => {
                if self.inbox.selected_item().is_none() {
                    self.inbox.select_next_counted();
                }
            },
        }
        self.go_to_size(PanelSize::Manager);
        if self.inbox.tab == BrowserTab::Inbox {
            self.inbox.request_preview();
            self.inbox.arm_timer();
        }
    }
    fn go_back_to_previous(&mut self) {
        run_action(actions::Action::ToggleSession, BTreeMap::new());
        self.panel.size = None;
        self.panel.report_size();
        close_self();
    }
    fn activate_card_target(&mut self, target: InboxCardTarget) {
        match target {
            InboxCardTarget::Item(key) => {
                self.panel.popup_focused = true;
                self.open_browser(Some(key));
            },
            InboxCardTarget::OpenInbox => {
                self.panel.popup_focused = true;
                self.open_browser(None);
            },
            InboxCardTarget::Close => self.panel.close(),
            InboxCardTarget::DontShowAgain => {
                reconfigure(SettingKey::AttentionStartupCard.kdl_snippet("never"), true);
                self.panel.close();
            },
            InboxCardTarget::BackTo(_) => self.go_back_to_previous(),
        }
    }
    fn card_target_list(&self) -> Vec<InboxCardTarget> {
        self.inbox
            .card_targets
            .iter()
            .map(|(_, _, target)| target.clone())
            .filter(|target| {
                !matches!(
                    target,
                    InboxCardTarget::Close | InboxCardTarget::DontShowAgain
                )
            })
            .collect()
    }
    fn move_card_hover(&mut self, delta: isize) {
        let targets = self.card_target_list();
        if targets.is_empty() {
            return;
        }
        let current = self
            .inbox
            .card_hovered
            .as_ref()
            .and_then(|hovered| targets.iter().position(|t| t == hovered));
        let next = match current {
            Some(index) => (index as isize + delta).clamp(0, targets.len() as isize - 1) as usize,
            None if delta < 0 => targets.len() - 1,
            None => 0,
        };
        self.inbox.card_hovered = targets.get(next).cloned();
    }
    fn handle_card_key(&mut self, key: &KeyWithModifier) -> Option<bool> {
        let plain = key.has_no_modifiers();
        let ctrl = key.has_only_modifiers(&[KeyModifier::Ctrl]);
        match key.bare_key {
            BareKey::Esc if plain => {
                self.size_down();
                Some(true)
            },
            BareKey::Enter if plain => {
                match self.inbox.card_hovered.clone() {
                    Some(target) => self.activate_card_target(target),
                    None => self.open_browser(None),
                }
                Some(true)
            },
            BareKey::Down | BareKey::Char('j') | BareKey::Tab if plain => {
                self.move_card_hover(1);
                Some(true)
            },
            BareKey::Up | BareKey::Char('k') if plain => {
                self.move_card_hover(-1);
                Some(true)
            },
            BareKey::Char('f') if ctrl => {
                self.open_browser(None);
                Some(true)
            },
            _ => Some(false),
        }
    }
    fn handle_card_mouse(&mut self, mouse: Mouse) -> bool {
        match mouse {
            Mouse::LeftClick(line, column) => {
                if let Some(target) = self.inbox.card_target_at(line, column) {
                    if self.panel.is_popup && !self.panel.popup_focused {
                        self.panel.popup_focused = true;
                        set_popup_focused(true);
                    }
                    self.activate_card_target(target);
                }
                true
            },
            Mouse::Hover(line, column) => {
                let hovered = self.inbox.card_target_at(line, column);
                if hovered != self.inbox.card_hovered {
                    self.inbox.card_hovered = hovered;
                    true
                } else {
                    false
                }
            },
            _ => false,
        }
    }
    fn after_inbox_selection_change(&mut self) {
        self.inbox.request_preview();
        self.inbox.arm_timer();
    }
    fn handle_inbox_key(&mut self, key: KeyWithModifier) -> bool {
        if self.inbox.reply.is_some() {
            if key.bare_key == BareKey::Esc && key.has_no_modifiers() {
                self.inbox.reply = None;
            } else {
                self.inbox.send_reply_key(key);
                self.inbox.arm_timer();
            }
            return true;
        }
        let plain = key.has_no_modifiers();
        let shift = key.has_only_modifiers(&[KeyModifier::Shift]);
        match key.bare_key {
            BareKey::Esc if plain => self.size_down(),
            BareKey::Tab if plain => self.switch_browser_tab(BrowserTab::Sessions),
            BareKey::Down if plain => {
                self.inbox.move_selection(1);
                self.after_inbox_selection_change();
            },
            BareKey::Up if plain => {
                self.inbox.move_selection(-1);
                self.after_inbox_selection_change();
            },
            BareKey::PageDown if plain => {
                self.inbox.move_selection(10);
                self.after_inbox_selection_change();
            },
            BareKey::PageUp if plain => {
                self.inbox.move_selection(-10);
                self.after_inbox_selection_change();
            },
            BareKey::Right if plain => {
                if self.inbox.expand_selected_group() {
                    self.after_inbox_selection_change();
                }
            },
            BareKey::Left if plain => {
                if self.inbox.collapse_selected_group() {
                    self.after_inbox_selection_change();
                }
            },
            BareKey::Enter if plain => {
                if self.inbox.toggle_header() {
                    return true;
                }
                self.go_to_selected_inbox_item();
            },
            BareKey::Char('r') if plain => {
                self.inbox.start_reply();
            },
            BareKey::Char('c') if plain => self.inbox.mark_selected_seen(),
            BareKey::Char('C') if plain || shift => mark_all_seen(None),
            BareKey::Char('m') if plain => self.inbox.cycle_selected_mute(),
            BareKey::Char('f') if plain => {
                self.inbox.flip_here_only();
                self.after_inbox_selection_change();
            },
            BareKey::Char('a') if plain => {
                self.inbox.flip_collapse_all();
                self.after_inbox_selection_change();
            },
            BareKey::Char('w') if plain => self.inbox.toggle_selected_watch(),
            _ => return false,
        }
        true
    }
    fn go_to_selected_inbox_item(&mut self) {
        if self.inbox.go_to_selected() {
            self.panel.size = None;
            self.panel.report_size();
            close_self();
        }
    }

    fn handle_inbox_mouse(&mut self, mouse: Mouse) -> bool {
        match mouse {
            Mouse::LeftClick(line, column) => match self.inbox.browser_target_at(line, column) {
                Some(BrowserTarget::Tab(tab)) => {
                    self.switch_browser_tab(tab);
                    true
                },
                Some(BrowserTarget::Row(index)) => {
                    self.inbox.reply = None;
                    let rows = self.inbox.visible_rows();
                    let already_selected = self.inbox.selected_row_index(&rows) == Some(index);
                    self.inbox.select_row(index);
                    if matches!(self.inbox.selected_row(), Some(InboxRow::Group(..))) {
                        self.inbox.toggle_header();
                    } else if already_selected {
                        self.go_to_selected_inbox_item();
                        return true;
                    }
                    self.after_inbox_selection_change();
                    true
                },
                Some(BrowserTarget::Preview) => self.inbox.start_reply(),
                Some(BrowserTarget::CollapseAll) => {
                    self.inbox.flip_collapse_all();
                    self.after_inbox_selection_change();
                    true
                },
                Some(BrowserTarget::HereOnly) => {
                    self.inbox.flip_here_only();
                    self.after_inbox_selection_change();
                    true
                },
                None => false,
            },
            Mouse::ScrollDown(lines) if self.inbox.reply.is_none() => {
                self.inbox.move_selection(lines.max(1) as isize);
                self.after_inbox_selection_change();
                true
            },
            Mouse::ScrollUp(lines) if self.inbox.reply.is_none() => {
                self.inbox.move_selection(-(lines.max(1) as isize));
                self.after_inbox_selection_change();
                true
            },
            _ => false,
        }
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        match self.panel.size {
            Some(PanelSize::Card) => self.handle_card_mouse(mouse),
            Some(PanelSize::Manager) if self.inbox_is_shown() => self.handle_inbox_mouse(mouse),
            Some(PanelSize::Manager) => match mouse {
                Mouse::LeftClick(line, column)
                    if self.shows_browser_tabs()
                        && matches!(
                            self.inbox.browser_target_at(line, column),
                            Some(BrowserTarget::Tab(_))
                        ) =>
                {
                    if let Some(BrowserTarget::Tab(tab)) =
                        self.inbox.browser_target_at(line, column)
                    {
                        self.switch_browser_tab(tab);
                    }
                    true
                },
                Mouse::LeftClick(line, column) => {
                    if let Some(index) = self.tree_marker_at(line, column) {
                        if let Some(row) = self.panel.tree.rows.get(index).cloned() {
                            self.panel.tree.toggle(&row);
                        }
                        return true;
                    }
                    match self.tree_row_at(line, column) {
                        Some(index) => {
                            self.tree_point_at(index);
                            if !self.open_tree_child() {
                                self.handle_selection();
                            }
                            true
                        },
                        None => false,
                    }
                },
                Mouse::Hover(line, column) => match self.tree_row_at(line, column) {
                    Some(index) if self.tree_cursor() != Some(index) => {
                        self.tree_point_at(index);
                        true
                    },
                    _ => false,
                },
                Mouse::ScrollDown(lines) => {
                    self.tree_move(lines.max(1) as isize);
                    true
                },
                Mouse::ScrollUp(lines) => {
                    self.tree_move(-(lines.max(1) as isize));
                    true
                },
                _ => false,
            },
            _ => false,
        }
    }
}

#[cfg(test)]
mod welcome_text_tests {
    use super::wrap_words;

    #[test]
    fn welcome_text_is_wrapped_by_word() {
        assert_eq!(
            wrap_words("one two three four", 9),
            vec!["one two", "three", "four"]
        );
        assert_eq!(
            wrap_words("averyveryverylongword x", 5),
            vec!["averyveryverylongword", "x"]
        );
        assert!(wrap_words("", 10).is_empty());
    }
}

mod app_panel;
mod badge_colors;
mod help;
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

use inbox::{
    BrowserTab, BrowserTarget, CardTarget as InboxCardTarget, Inbox, ItemAction, ItemKey,
    LiveLayout, SessionsLiveTarget,
};
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
    app_panel_view_shown: bool,
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
    pub(crate) pending_delete_exited: bool,
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
    open_disconnecting: bool,
    inbox_requested: bool,
    remote_client: Option<String>,
    pending_remote_window: Option<PendingWindow>,
    remote_window_confirmed: bool,
    footer_deferred: bool,
    show_all_keys: bool,
    all_keys_scroll: usize,
    last_cols: usize,
    desktop_offer: DesktopOffer,
    sessions_live_layout: Option<inbox::LiveLayout>,
}

#[derive(Default)]
struct DesktopOffer {
    state: Option<desktop_integration::IntegrationState>,
    reminders: Option<desktop_integration::ReminderContext>,
    asked: bool,
    shown: bool,
}

#[derive(Debug, Clone)]
struct PendingWindow {
    request: SessionTerminalRequest,
    leave: bool,
    from_layout_list: bool,
}

fn reminder_context(mode_info: &ModeInfo) -> desktop_integration::ReminderContext {
    let snapshot = read_config();
    let enabled = |key: SettingKey| {
        snapshot
            .setting(key)
            .and_then(|setting| setting.current_value.clone())
            .map(|value| value != "false")
            .unwrap_or(true)
    };
    let web_client = mode_info.is_web_client.unwrap_or(false);
    desktop_integration::ReminderContext {
        show_startup_tips: enabled(SettingKey::ShowStartupTips),
        show_release_notes: enabled(SettingKey::ShowReleaseNotes),
        inside_desktop_terminal: mode_info.remote_client.is_none() && !web_client,
        web_client,
    }
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
            EventType::DesktopIntegration,
        ]);
        request_desktop_integration(&desktop_integration::Request::GetState);
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
                    self.inbox.tab = BrowserTab::Sessions;
                    self.inbox.tab_chosen = true;
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
                Some("manager_focused") => {
                    self.panel.popup_focused = true;
                    self.panel.is_popup = false;
                    self.panel.size = Some(PanelSize::Manager);
                    self.panel.needs_initial_layout = false;
                    self.inbox.tab = BrowserTab::Sessions;
                    self.inbox.tab_chosen = true;
                    self.inbox.select_newest_on_arrival = true;
                },
                _ => {
                    self.panel.size = Some(PanelSize::Manager);
                    self.panel.needs_initial_layout = !self.is_welcome_screen;
                    self.inbox.tab = BrowserTab::Sessions;
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
                EventType::LiveViewStateChanged,
            ]);
            request_permission(&[
                PermissionType::ReadPaneContents,
                PermissionType::WriteToStdin,
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
            self.app_panel_auto_pending = app_panel::waits_for_items(&view);
            self.show_app_panel_view(Some(&view));
            self.settle_inbox_hold();
            return true;
        }
        let mut should_render = self.handle_pipe(pipe_message);
        should_render |= self.settle_inbox_hold();
        self.keep_ages_fresh();
        should_render
    }
    fn update(&mut self, event: Event) -> bool {
        app_panel::begin_event();
        let user_input = is_user_choice(&event);
        let attention = matches!(event, Event::GlobalAttentionUpdate(_));
        if user_input {
            self.inbox.select_newest_on_arrival = false;
        }
        let mut should_render = self.handle_event(event);
        should_render |= self.settle_app_panel_view(user_input, attention);
        should_render |= self.settle_inbox_hold();
        self.keep_ages_fresh();
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        self.render_plugin(rows, cols);
    }
}

impl State {
    fn settle_inbox_hold(&mut self) -> bool {
        if self.is_visible && self.browser_is_open() {
            return self.forget_closed_inbox_panes();
        }
        self.inbox.forget_listed();
        false
    }
    fn forget_closed_inbox_panes(&mut self) -> bool {
        if self.current_panes.panes.is_empty() {
            return false;
        }
        let open_panes: Vec<u32> = self
            .current_panes
            .panes
            .values()
            .flatten()
            .filter(|pane| !pane.is_plugin)
            .map(|pane| pane.id)
            .collect();
        self.inbox.forget_closed_panes_here(&open_panes)
    }
    fn keep_ages_fresh(&mut self) {
        if self.is_visible && self.shows_ages() {
            self.inbox.arm_clock(inbox::now_ms());
        }
        if self.is_visible && self.inbox_is_shown() {
            self.inbox.arm_spinner();
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
                Some(timers::TimerOwner::Spinner) => {
                    self.inbox.spinner_armed = false;
                    return self.is_visible && self.inbox_is_shown();
                },
                None => return false,
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
                if !is_visible {
                    self.inbox.clear_live_view();
                    self.app_panel_view_shown = false;
                }
                if is_visible && !was_visible {
                    self.panel.request_suggestions();
                    should_render = true;
                    if !app_panel::is_enabled()
                        && self.panel.size == Some(PanelSize::Manager)
                        && self.shows_browser_tabs()
                        && !self.inbox.is_capturing()
                        && self.inbox.select_newest_counted()
                    {
                        self.inbox.tab = BrowserTab::Inbox;
                        self.inbox.tab_chosen = true;
                        should_render = true;
                    }
                }
            },
            Event::DesktopIntegration(payload) => {
                if let desktop_integration::Response::Done(state) =
                    desktop_integration::Response::from_json(&payload)
                {
                    self.desktop_offer.state = Some(state);
                }
            },
            Event::ModeUpdate(mode_info) => {
                if self.desktop_offer.reminders.is_none() {
                    self.desktop_offer.reminders = Some(reminder_context(&mode_info));
                }
                if let Some(session_name) = mode_info.session_name.as_ref() {
                    if !self.is_welcome_screen {
                        self.set_session_name(session_name);
                    }
                }
                self.colors = Colors::new(mode_info.style.colors);
                self.simplified_ui = mode_info.capabilities.arrow_fonts;
                let was_web_client = self.is_web_client;
                self.is_web_client = mode_info.is_web_client.unwrap_or(false);
                self.remote_client = mode_info.remote_client.clone();
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
                        self.set_session_name(&session_info.name);
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
                badge_colors::set(&global.badge_colors);
                self.panel.attention = Some(global.clone());
                should_render = self.inbox.update_global(global);
                if self.away_card_is_empty() {
                    close_self();
                    return false;
                }
                if self.panel.size == Some(PanelSize::Card) {
                    self.resize_card_popup();
                }
            },
            event @ Event::LiveViewStateChanged { .. } => {
                if let Some(status) = inbox::LiveViewStatus::from_event(&event) {
                    should_render = self.inbox.apply_live_view_status(status);
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
        self.inbox.live_view_drawn = false;
        self.inbox.live_layout = None;
        self.inbox.sessions_target = None;
        self.sessions_live_layout = None;
        if self.panel.size == Some(PanelSize::Manager)
            && self.shows_browser_tabs()
            && self.inbox.tab == BrowserTab::Inbox
        {
            self.refresh_inbox_window_help();
            self.inbox.live_mode = self
                .inbox
                .plan_live_layout(0, 1, cols, rows.saturating_sub(1))
                .is_some();
        }
        if !self.is_visible {
            self.inbox.clear_live_view();
        } else if self.inbox_browser_will_render() {
            self.inbox
                .prepare_live_view(0, 1, cols, rows.saturating_sub(1));
        } else if let Some((layout, target)) = self.plan_sessions_live_view(rows, cols) {
            self.inbox.sessions_target = Some(target);
            self.inbox.show_live_view(&layout);
            self.sessions_live_layout = Some(layout);
        } else if self.sessions_preview_will_render(cols)
            && (self.selected_session().is_some() || self.on_exited_header())
        {
            self.inbox.hide_live_view();
        } else {
            self.inbox.clear_live_view();
        }
        self.render_plugin_body(rows, cols);
        if app_panel::is_enabled() {
            self.app_panel_cols = cols;
            app_panel::render_close_button(cols);
        }
    }

    fn inbox_browser_will_render(&self) -> bool {
        !(self.panel.is_popup && self.panel.needs_initial_layout)
            && !self.app_quit
            && self.pending_remote_window.is_none()
            && !self.show_all_keys
            && self.panel.size == Some(PanelSize::Manager)
            && self.shows_browser_tabs()
            && self.inbox.tab == BrowserTab::Inbox
    }

    fn sessions_preview_will_render(&self, cols: usize) -> bool {
        !(self.panel.is_popup && self.panel.needs_initial_layout)
            && !self.app_quit
            && self.pending_remote_window.is_none()
            && !self.show_all_keys
            && self.panel.size == Some(PanelSize::Manager)
            && !self.is_welcome_screen
            && self.inbox.tab == BrowserTab::Sessions
            && !self.no_other_sessions()
            && self.single_screen_state.mode != SingleScreenMode::SelectingLayout
            && cols >= 60
    }

    fn sessions_list_cols(cols: usize) -> usize {
        (cols / 2).max(30)
    }

    fn sessions_preview_area(&self, rows: usize, cols: usize) -> (usize, usize, usize, usize) {
        let list_cols = Self::sessions_list_cols(cols);
        let top = if self.panel.size == Some(PanelSize::Manager) && self.shows_browser_tabs() {
            1
        } else {
            0
        };
        let footer = if self.error.is_none() {
            self.footer_rows_for(cols)
        } else {
            0
        };
        (
            list_cols + 1,
            top,
            cols.saturating_sub(list_cols + 1),
            rows.saturating_sub(top + footer),
        )
    }

    fn plan_sessions_live_view(
        &self,
        rows: usize,
        cols: usize,
    ) -> Option<(LiveLayout, SessionsLiveTarget)> {
        if !self.sessions_preview_will_render(cols) {
            return None;
        }
        let target = self.sessions_live_target()?;
        let (x, y, width, height) = self.sessions_preview_area(rows, cols);
        let layout = LiveLayout::around_box(x, y, width, height)?;
        Some((layout, target))
    }

    fn sessions_live_target(&self) -> Option<SessionsLiveTarget> {
        if self.on_exited_header() {
            return None;
        }
        let (name, running, _) = self.selected_session()?;
        if !running {
            return None;
        }
        let node = if self.panel.tree.node_session == name {
            self.panel.tree.node
        } else {
            session_tree::TreeNode::Session
        };
        let (tab_index, pane_index, pane_id) = self.panel.tree.live_pane(&name, node)?;
        let tab = self.panel.tree.children.get(&name)?.get(tab_index)?;
        let pane = tab.panes.get(pane_index)?;
        Some(SessionsLiveTarget {
            key: ItemKey {
                session: name,
                pane_id: PaneId::Terminal(pane_id),
            },
            tab: tab.name.clone(),
            pane: pane.title.clone(),
        })
    }

    fn selected_session_is_exited(&self) -> bool {
        matches!(self.selected_session(), Some((_, false, _)))
    }

    fn render_plugin_body(&mut self, rows: usize, cols: usize) {
        self.inbox.browser_targets.clear();
        if self.panel.is_popup && self.panel.needs_initial_layout {
            return;
        }
        if self.app_quit {
            self.render_app_quit(rows, cols);
            return;
        }
        if self.pending_remote_window.is_some() {
            self.render_remote_window_warning(rows, cols);
            return;
        }
        self.last_cols = cols;
        if self.show_all_keys {
            let hints = self.all_keys();
            let mut scroll = self.all_keys_scroll;
            help::render_all_keys(help::ALL_KEYS_TITLE, &hints, rows, cols, &mut scroll);
            self.all_keys_scroll = scroll;
            return;
        }
        if self.panel.size == Some(PanelSize::Manager) && self.shows_browser_tabs() {
            let sessions = self.running_session_count();
            self.inbox.render_tabs(sessions, 0, 0, cols);
            if self.inbox.tab == BrowserTab::Inbox {
                self.refresh_inbox_window_help();
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
                let list_cols = Self::sessions_list_cols(cols);
                self.render_welcome_text(list_cols);
                self.footer_deferred = true;
                self.render_classic(rows, list_cols);
                self.footer_deferred = false;
                let top = self.top_offset;
                let footer = if self.error.is_none() {
                    self.footer_rows()
                } else {
                    0
                };
                match self.sessions_live_layout {
                    Some(layout) => {
                        self.inbox.live_layout = Some(layout);
                        self.inbox.render_live_view_box(&layout);
                    },
                    None => {
                        let pannable = self.selected_session_is_exited();
                        render_preview(
                            &mut self.panel,
                            list_cols + 1,
                            top,
                            cols.saturating_sub(list_cols + 1),
                            rows.saturating_sub(top + footer),
                            pannable,
                        );
                    },
                }
                if self.error.is_none() {
                    self.render_footer(0, cols, rows, false);
                }
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
                1
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
    fn enter_action(&self) -> Option<&'static str> {
        let selected_result = self.single_screen_state.get_selected_result();
        if let (Some(result), true) = (selected_result, self.panel.size.is_some()) {
            match result {
                UnifiedSearchResult::ActiveSession { .. } => Some("Attach"),
                UnifiedSearchResult::ResurrectableSession { .. } => Some("Resurrect"),
            }
        } else if self.single_screen_state.search_term.is_empty() && self.panel.size.is_some() {
            Some("New session with a random name")
        } else if !self.single_screen_state.search_term.is_empty() {
            if let Some(result) = self.single_screen_state.get_selected_result() {
                match result {
                    UnifiedSearchResult::ActiveSession { .. } => Some("Attach"),
                    UnifiedSearchResult::ResurrectableSession { .. } => Some("Resurrect"),
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
        }
    }
    fn sessions_help_groups(&self) -> (Vec<help::Hint>, Vec<help::Hint>) {
        use help::hint;
        if self.renaming_session_name.is_some()
            || self.show_kill_all_sessions_warning
            || self.pending_delete_exited
            || self.pending_session_stop.is_some()
            || self.single_screen_state.mode == SingleScreenMode::SelectingLayout
        {
            return (self.sessions_hints(false), vec![]);
        }
        let mut primary = vec![];
        if let Some(action) = self.enter_action() {
            primary.push(hint("<ENTER>", action.to_lowercase()));
        }
        let target = self.sessions_open_target();
        primary.extend(self.open_way_hints(Some(&target), false));
        let mut secondary = vec![
            hint("<↓↑>", "select"),
            hint("<←→>", "tabs and panes"),
            hint("<type>", "search or name a new session"),
        ];
        if self.shows_browser_tabs() {
            secondary.push(hint("<TAB>", "inbox"));
        }
        if !self.is_welcome_screen {
            secondary.push(hint("<ESC>", "close"));
        }
        if self.exited_session_count() > 0 {
            secondary.push(hint("<Shift Del>", "delete all exited"));
        }
        secondary.push(hint("<F1>", "all keys"));
        (primary, secondary)
    }
    fn sessions_help_block(&self, width: usize) -> Vec<help::BlockLine> {
        let (primary, secondary) = self.sessions_help_groups();
        help::block_lines(&primary, &secondary, &help::hint("<F1>", "all keys"), width)
    }
    fn help_width(&self, cols: usize) -> usize {
        cols.saturating_sub(4).max(10)
    }
    fn footer_rows(&self) -> usize {
        self.footer_rows_for(self.last_cols)
    }
    fn footer_rows_for(&self, cols: usize) -> usize {
        self.sessions_help_block(self.help_width(cols)).len() + 2
    }
    fn render_footer(&self, x: usize, width: usize, rows: usize, centered: bool) {
        if self.active_screen == ActiveScreen::SingleScreen {
            let help_width = self.help_width(width);
            let lines = self.sessions_help_block(help_width);
            let y = rows.saturating_sub(lines.len() + 1);
            help::render_block(&lines, x + 2, y, help_width);
            return;
        }
        let help_x = if centered
            && self.active_screen == ActiveScreen::SingleScreen
            && self.single_screen_state.mode == SingleScreenMode::SearchAndSelect
        {
            let content_width = std::cmp::min(width, 90);
            x + (width.saturating_sub(content_width)) / 2
        } else {
            x
        };
        let width = width.saturating_sub(help_x - x);
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
                rows.saturating_sub(1),
                self.current_session_last_saved_time,
            );
        }
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
                        } else if self.pending_delete_exited {
                            render_delete_exited_warning(
                                self.exited_session_count(),
                                height,
                                width,
                                x,
                                y,
                            );
                        } else if let Some(name) = self.pending_session_stop.as_ref() {
                            render_stop_session_warning(name, height, width, x, y);
                        } else {
                            // Use max_table_rows as fixed content height so the
                            // prompt position stays stable regardless of result count
                            let max_table_rows = height.saturating_sub(3 + self.footer_rows());
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

                            render_single_screen_prompt(
                                &self.single_screen_state.search_term,
                                None,
                                &[],
                                content_width,
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
            if !self.footer_deferred {
                self.render_footer(x, width, rows, true);
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
        if self.pending_remote_window.is_some() {
            return self.handle_remote_window_key(&key);
        }
        if self.show_all_keys {
            match key.bare_key {
                BareKey::Down if key.has_no_modifiers() => self.all_keys_scroll += 1,
                BareKey::Up if key.has_no_modifiers() => {
                    self.all_keys_scroll = self.all_keys_scroll.saturating_sub(1)
                },
                _ => {
                    self.show_all_keys = false;
                    self.all_keys_scroll = 0;
                },
            }
            return true;
        }
        if self.panel.size == Some(PanelSize::Manager)
            && (key.bare_key == BareKey::F(1)
                || (self.inbox_is_shown()
                    && key.bare_key == BareKey::Char('?')
                    && (key.has_no_modifiers() || key.has_only_modifiers(&[KeyModifier::Shift]))))
        {
            if !self.inbox_is_shown() {
                self.current_session_last_saved_time = current_session_last_saved_time();
            }
            self.show_all_keys = true;
            return true;
        }
        if self.error.is_some() {
            self.error = None;
            return true;
        }
        if let Some(way) = open_way(&key) {
            if let Some(should_render) = self.handle_open_way(way) {
                return should_render;
            }
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
                BareKey::Char('x') if key.has_only_modifiers(&[KeyModifier::Alt]) => {
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

        if self.pending_delete_exited {
            match key.bare_key {
                BareKey::Char('y') if key.has_no_modifiers() => {
                    self.pending_delete_exited = false;
                    self.delete_all_exited_sessions();
                },
                BareKey::Char('n') | BareKey::Esc if key.has_no_modifiers() => {
                    self.pending_delete_exited = false;
                },
                BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    self.pending_delete_exited = false;
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
                    Some(DeleteTarget::Active(name))
                        if self.session_name.as_deref() == Some(name.as_str()) =>
                    {
                        self.show_error("This is the current session. Quit to stop it.");
                    },
                    Some(DeleteTarget::Active(name)) => {
                        self.pending_session_stop = Some(name);
                    },
                    Some(target) => self.delete_session_target(target),
                    None => {},
                }
                should_render = true;
            },
            BareKey::Delete if key.has_only_modifiers(&[KeyModifier::Shift]) => {
                if self.exited_session_count() == 0 {
                    self.show_error("There are no exited sessions to delete.");
                } else {
                    self.pending_delete_exited = true;
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
                                        if self.panel.size.is_some() {
                                            self.panel.size = None;
                                            self.panel.report_size();
                                            switched_session = true;
                                        } else {
                                            self.show_error("Already attached...");
                                        }
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
        self.set_session_name(new_name);
    }

    fn set_session_name(&mut self, name: &str) {
        if self.session_name.as_deref() == Some(name) {
            return;
        }
        self.session_name = Some(name.to_owned());
        self.inbox.current_session = self.session_name.clone();
        if self.panel.size == Some(PanelSize::Manager) {
            self.apply_suggestions_to_list();
        }
    }

    fn listed_current_session(&self) -> Option<String> {
        if app_panel::is_enabled() || self.is_welcome_screen {
            return None;
        }
        self.session_name.clone()
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
    fn exited_session_count(&self) -> usize {
        self.resurrectable_sessions.all_resurrectable_sessions.len()
    }
    fn delete_all_exited_sessions(&mut self) {
        let previous_index = self.single_screen_state.selected_index;
        match delete_all_dead_sessions() {
            Ok(()) => {
                self.resurrectable_sessions
                    .all_resurrectable_sessions
                    .clear();
                self.single_screen_state.update_search_term(
                    &self.sessions.session_ui_infos,
                    &self.resurrectable_sessions.all_resurrectable_sessions,
                );
                self.single_screen_state
                    .restore_selection_after_delete(previous_index);
            },
            Err(e) => {
                self.show_error(&format!("Failed to delete the exited sessions: {}", e));
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

pub(crate) fn delete_exited_warning_lines(count: usize) -> (String, String) {
    (
        format!(
            "This will delete all {} exited {}",
            count,
            if count == 1 { "session" } else { "sessions" }
        ),
        "Are you sure? (y/n)".to_owned(),
    )
}

fn render_delete_exited_warning(count: usize, rows: usize, columns: usize, x: usize, y: usize) {
    if rows == 0 || columns == 0 {
        return;
    }
    let (warning, confirmation) = delete_exited_warning_lines(count);
    let count_start = "This will delete all ".chars().count();
    let count_end = count_start + count.to_string().chars().count();
    let warning_x = x + columns.saturating_sub(warning.chars().count()) / 2;
    let confirmation_x = x + columns.saturating_sub(confirmation.chars().count()) / 2;
    print_text_with_coordinates(
        Text::from(warning.as_str()).color_range(0, count_start..count_end),
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

fn remote_window_warning_lines(remote: &str, leave: bool) -> Vec<String> {
    let mut lines = vec![
        format!("You are connected {}.", remote),
        String::new(),
        "The window opens on the desktop of the machine running Zellij.".to_owned(),
        "If you are typing on another machine, you will not see it.".to_owned(),
    ];
    if leave {
        lines.push(String::new());
        lines.push("Detaching here also ends this connection to Zellij:".to_owned());
        lines.push("you will be left with nothing on your screen.".to_owned());
    }
    lines.push(String::new());
    lines.push("<Enter> open it anyway    <Esc> cancel".to_owned());
    lines
}

fn run_window_request(request: SessionTerminalRequest) {
    request_session_terminal(request);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenWay {
    Window,
    WindowAndLeave,
    DisconnectOthers,
}

fn is_alt_enter(key: &KeyWithModifier) -> bool {
    key.bare_key == BareKey::Enter && key.has_only_modifiers(&[KeyModifier::Alt])
}

fn open_way(key: &KeyWithModifier) -> Option<OpenWay> {
    if !key.has_only_modifiers(&[KeyModifier::Alt]) {
        return None;
    }
    match key.bare_key {
        BareKey::Char('w') => Some(OpenWay::Window),
        BareKey::Char('d') => Some(OpenWay::WindowAndLeave),
        BareKey::Char('x') => Some(OpenWay::DisconnectOthers),
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct OpenTarget {
    name: Option<String>,
    exists: bool,
    running: bool,
    current: bool,
    pane: Option<PaneId>,
}

impl OpenTarget {
    fn is_here(&self) -> bool {
        self.current || self.name.is_none()
    }
}

impl State {
    fn desktop_app_running(&self) -> bool {
        self.panel
            .suggestions
            .as_ref()
            .map(|s| s.desktop_app_running)
            .unwrap_or(false)
    }
    fn selected_has_window(&self, name: &str) -> bool {
        self.panel
            .suggestions
            .as_ref()
            .map(|s| s.desktop_window_sessions.iter().any(|w| w == name))
            .unwrap_or(false)
    }
    fn can_close_this_window(&self) -> bool {
        self.desktop_app_running() && !app_panel::is_enabled()
    }
    fn sessions_open_target(&self) -> OpenTarget {
        if let Some((name, running, current)) = self.selected_session() {
            let pane = if running {
                self.selected_tree_pane(&name)
            } else {
                None
            };
            return OpenTarget {
                name: Some(name),
                exists: true,
                running,
                current,
                pane,
            };
        }
        let typed = self.single_screen_state.search_term.clone();
        if typed.is_empty() {
            return OpenTarget {
                name: None,
                exists: false,
                running: false,
                current: false,
                pane: None,
            };
        }
        let running = self.sessions.has_session(&typed);
        let exists = running || self.resurrectable_sessions.has_session(&typed);
        let current = running && self.session_name.as_deref() == Some(typed.as_str());
        OpenTarget {
            name: Some(typed),
            exists,
            running,
            current,
            pane: None,
        }
    }
    fn inbox_open_target(&self) -> Option<OpenTarget> {
        self.selected_inbox_target()
            .map(|(session, pane_id, is_current)| OpenTarget {
                name: Some(session),
                exists: true,
                running: true,
                current: is_current,
                pane: Some(pane_id),
            })
    }
    fn selected_tree_pane(&self, session: &str) -> Option<PaneId> {
        if self.panel.tree.node_session != session {
            return None;
        }
        let session_tree::TreeNode::Pane(tab, pane) = self.panel.tree.node else {
            return None;
        };
        self.panel
            .tree
            .children
            .get(session)
            .and_then(|tabs| tabs.get(tab))
            .and_then(|t| t.panes.get(pane))
            .map(|p| {
                if p.is_plugin {
                    PaneId::Plugin(p.id)
                } else {
                    PaneId::Terminal(p.id)
                }
            })
    }
    fn way_available(&self, way: OpenWay, target: &OpenTarget, inbox: bool) -> bool {
        match way {
            OpenWay::Window => true,
            OpenWay::WindowAndLeave => self.can_close_this_window() && !target.current,
            OpenWay::DisconnectOthers if inbox => !target.current || !app_panel::is_enabled(),
            OpenWay::DisconnectOthers if target.is_here() => {
                !app_panel::is_enabled() && !self.is_welcome_screen
            },
            OpenWay::DisconnectOthers => target.running,
        }
    }
    fn window_request_for(&self, target: &OpenTarget) -> Result<SessionTerminalRequest, String> {
        match (target.name.as_deref(), target.exists, target.running) {
            (Some(name), true, true) => {
                Ok(SessionTerminalRequest::existing(name, target.pane, false))
            },
            (Some(name), true, false) => Ok(SessionTerminalRequest::existing(name, None, true)),
            (name, _, _) => {
                if let Some(name) = name {
                    if name.len() >= 108 {
                        return Err("Session name must be shorter than 108 bytes".to_owned());
                    }
                    if name.contains('/') {
                        return Err("Session name cannot contain '/'".to_owned());
                    }
                }
                Ok(SessionTerminalRequest::new_session(
                    name,
                    Some(self.default_layout_info()),
                    self.single_screen_state.new_session_folder.clone(),
                ))
            },
        }
    }
    fn handle_open_way(&mut self, way: OpenWay) -> Option<bool> {
        if self.panel.size == Some(PanelSize::Card) {
            return None;
        }
        if self.inbox_is_shown() {
            let Some(target) = self.inbox_open_target() else {
                return Some(false);
            };
            return Some(self.open_another_way(way, target, true));
        }
        if self.active_screen != ActiveScreen::SingleScreen
            || self.renaming_session_name.is_some()
            || self.show_kill_all_sessions_warning
            || self.pending_delete_exited
            || self.pending_session_stop.is_some()
        {
            return None;
        }
        match self.single_screen_state.mode {
            SingleScreenMode::SelectingLayout => Some(match way {
                OpenWay::Window => self.new_session_window_from_layout_list(false),
                OpenWay::WindowAndLeave => self.new_session_window_from_layout_list(true),
                OpenWay::DisconnectOthers => false,
            }),
            SingleScreenMode::SearchAndSelect => {
                if self.panel.size == Some(PanelSize::Manager) && self.on_exited_header() {
                    return Some(false);
                }
                let target = self.sessions_open_target();
                Some(self.open_another_way(way, target, false))
            },
        }
    }
    fn open_another_way(&mut self, way: OpenWay, target: OpenTarget, inbox: bool) -> bool {
        if !self.way_available(way, &target, inbox) {
            return false;
        }
        match way {
            OpenWay::DisconnectOthers if inbox => {
                self.inbox_go_disconnecting();
            },
            OpenWay::DisconnectOthers => self.sessions_disconnect_others(&target),
            OpenWay::Window | OpenWay::WindowAndLeave => match self.window_request_for(&target) {
                Ok(request) => self.dispatch_window_request(PendingWindow {
                    request,
                    leave: way == OpenWay::WindowAndLeave,
                    from_layout_list: false,
                }),
                Err(message) => self.show_error(&message),
            },
        }
        true
    }
    fn sessions_disconnect_others(&mut self, target: &OpenTarget) {
        if target.is_here() {
            disconnect_other_clients();
            if !self.open_tree_child() {
                self.single_screen_state.search_term.clear();
                self.single_screen_state.selected_index = None;
                self.panel.size = None;
                self.panel.report_size();
                close_self();
            }
            return;
        }
        self.open_disconnecting = true;
        if !self.open_tree_child() {
            self.handle_selection();
        }
        self.open_disconnecting = false;
    }
    fn dispatch_window_request(&mut self, pending: PendingWindow) {
        let needs_confirmation =
            self.remote_client.is_some() && (pending.leave || !self.remote_window_confirmed);
        if needs_confirmation {
            self.pending_remote_window = Some(pending);
        } else {
            self.carry_out_window_request(pending);
        }
    }
    fn carry_out_window_request(&mut self, pending: PendingWindow) {
        run_window_request(pending.request);
        self.single_screen_state.search_term.clear();
        if pending.from_layout_list {
            self.single_screen_state.transition_to_search();
        } else {
            self.single_screen_state.selected_index = None;
        }
        self.finish_window_request(pending.leave);
    }
    fn handle_remote_window_key(&mut self, key: &KeyWithModifier) -> bool {
        match key.bare_key {
            BareKey::Enter | BareKey::Char('y') | BareKey::Char('Y')
                if key.has_no_modifiers() || key.has_only_modifiers(&[KeyModifier::Shift]) =>
            {
                if let Some(pending) = self.pending_remote_window.take() {
                    if !pending.leave {
                        self.remote_window_confirmed = true;
                    }
                    self.carry_out_window_request(pending);
                }
            },
            BareKey::Esc | BareKey::Char('n') | BareKey::Char('N') => {
                self.pending_remote_window = None;
            },
            BareKey::Char('c') if key.has_only_modifiers(&[KeyModifier::Ctrl]) => {
                self.pending_remote_window = None;
            },
            _ => return false,
        }
        true
    }
    fn render_remote_window_warning(&self, rows: usize, cols: usize) {
        let Some(pending) = self.pending_remote_window.as_ref() else {
            return;
        };
        let remote = self.remote_client.clone().unwrap_or_default();
        let lines = remote_window_warning_lines(&remote, pending.leave);
        let (x, y, shown) = app_panel::centered_block(&lines, rows, cols);
        let last = shown.len().saturating_sub(1);
        for (index, line) in shown.iter().enumerate() {
            let text = Text::from(line.as_str());
            let text = if index == 0 {
                text.color_range(2, ..)
            } else if index == last {
                text.color_substring(3, "<Enter>")
                    .color_substring(3, "<Esc>")
            } else if line.starts_with("Detaching") {
                text.error_color_all()
            } else {
                text
            };
            print_text_with_coordinates(text, x, y + index, Some(cols), None);
        }
    }
    fn new_session_window_from_layout_list(&mut self, leave: bool) -> bool {
        if leave && !self.can_close_this_window() {
            return false;
        }
        let typed = self.single_screen_state.search_term.clone();
        if typed.contains('/') || typed.len() >= 108 {
            self.show_error("Session names must be shorter than 108 bytes and cannot contain '/'");
            return true;
        }
        let name = if typed.is_empty() {
            None
        } else {
            Some(typed.as_str())
        };
        let request = SessionTerminalRequest::new_session(
            name,
            self.single_screen_state.layout_list.selected_layout_info(),
            self.single_screen_state.new_session_folder.clone(),
        );
        self.dispatch_window_request(PendingWindow {
            request,
            leave,
            from_layout_list: true,
        });
        true
    }
    fn finish_window_request(&mut self, leave: bool) {
        if leave {
            if self.is_welcome_screen {
                quit_zellij();
            } else {
                self.leave_after_window_request();
            }
        } else if !self.is_welcome_screen {
            self.panel.size = None;
            self.panel.report_size();
            close_self();
        }
    }
    fn leave_after_window_request(&mut self) {
        self.panel.size = None;
        self.panel.report_size();
        detach();
    }
    fn selected_inbox_target(&self) -> Option<(String, PaneId, bool)> {
        self.inbox
            .selected_target()
            .map(|item| (item.session.clone(), item.item.pane_id, item.is_current))
    }
    fn inbox_go_disconnecting(&mut self) -> bool {
        let Some((session, pane_id, is_current)) = self.selected_inbox_target() else {
            return false;
        };
        if is_current {
            if !self.inbox.go_to_selected() {
                return false;
            }
            if !app_panel::is_enabled() {
                disconnect_other_clients();
            }
        } else {
            let pane = match pane_id {
                PaneId::Terminal(id) => (id, false),
                PaneId::Plugin(id) => (id, true),
            };
            switch_session_with_options(ConnectToSession {
                name: Some(session),
                pane_id: Some(pane),
                switch_context: SwitchContext::from_plugin_request(None, true),
                ..Default::default()
            });
        }
        self.panel.size = None;
        self.panel.report_size();
        close_self();
        true
    }
    fn refresh_inbox_window_help(&mut self) {
        self.maybe_offer_desktop_app();
        self.inbox.help_hints = self.inbox_hints(false);
        self.inbox.offer_hints = if self.desktop_offer.shown {
            self.inbox.help_hints.clone()
        } else {
            vec![]
        };
    }
    fn maybe_offer_desktop_app(&mut self) {
        if self.desktop_offer.asked || app_panel::is_enabled() || !self.inbox_is_shown() {
            return;
        }
        let (Some(state), Some(reminders)) =
            (&self.desktop_offer.state, &self.desktop_offer.reminders)
        else {
            return;
        };
        self.desktop_offer.asked = true;
        if state.reminders_allowed(reminders) {
            self.desktop_offer.shown = true;
            request_desktop_integration(&desktop_integration::Request::SetFlag(
                desktop_integration::Flag::DesktopAppIntroduced,
            ));
        }
    }
    fn leave_here_text(&self) -> &'static str {
        if self.is_welcome_screen {
            return "then leave the welcome screen";
        }
        match self.remote_client.as_deref() {
            None => "then close this window",
            Some(remote) if remote.starts_with("through the web") => "then detach this browser",
            Some(_) => "then detach this terminal",
        }
    }
    fn window_label(&self, target: &OpenTarget) -> &'static str {
        match (target.name.as_deref(), target.exists, target.running) {
            (Some(name), true, true) if self.selected_has_window(name) => "go to its window",
            (_, true, true) => "open it in a window",
            (_, true, false) => "resurrect it in a window",
            (Some(_), false, _) => "create it in a window",
            (None, _, _) => "new session in a window",
        }
    }
    fn open_way_hints(&self, target: Option<&OpenTarget>, inbox: bool) -> Vec<help::Hint> {
        let Some(target) = target else {
            return vec![];
        };
        let mut hints = vec![];
        if self.way_available(OpenWay::Window, target, inbox) {
            hints.push(help::hint("<Alt w>", self.window_label(target)));
        }
        if self.way_available(OpenWay::WindowAndLeave, target, inbox) {
            hints.push(help::hint(
                "<Alt d>",
                format!("same, {}", self.leave_here_text()),
            ));
        }
        let has_window_hints = !hints.is_empty();
        if self.way_available(OpenWay::DisconnectOthers, target, inbox) {
            hints.push(help::hint(
                "<Alt x>",
                if inbox {
                    "go there, disconnect others"
                } else if target.is_here() {
                    "disconnect others from this session"
                } else {
                    "open here, disconnect others"
                },
            ));
        }
        if has_window_hints && self.remote_client.is_some() {
            hints.push(help::hint("(!)", "windows open on the Zellij machine"));
        }
        hints
    }
    fn layout_window_hints(&self) -> Vec<help::Hint> {
        let mut hints = vec![help::hint("<Alt w>", "create it in a window")];
        if self.can_close_this_window() {
            hints.push(help::hint(
                "<Alt d>",
                format!("same, {}", self.leave_here_text()),
            ));
        }
        if self.remote_client.is_some() {
            hints.push(help::hint("(!)", "windows open on the Zellij machine"));
        }
        hints
    }
    fn sessions_hints(&self, all: bool) -> Vec<help::Hint> {
        use help::hint;
        if self.renaming_session_name.is_some() {
            return vec![hint("<ENTER>", "rename"), hint("<ESC>", "cancel")];
        }
        if self.show_kill_all_sessions_warning
            || self.pending_delete_exited
            || self.pending_session_stop.is_some()
        {
            return vec![hint("<y>", "confirm"), hint("<n>", "cancel")];
        }
        let mut hints = vec![];
        if self.single_screen_state.mode == SingleScreenMode::SelectingLayout {
            hints.push(hint("<ENTER>", "create with this layout"));
            hints.push(hint("<↓↑>", "select a layout"));
            hints.push(hint("<ESC>", "back"));
            hints.extend(self.layout_window_hints());
            hints.push(hint("<type>", "filter layouts"));
            hints.push(hint("<Ctrl f>", "choose a folder"));
            hints.push(hint("<Ctrl c>", "reset the folder"));
            return hints;
        }
        if let Some(action) = self.enter_action() {
            hints.push(hint("<ENTER>", action.to_lowercase()));
        }
        hints.push(hint("<↓↑>", "select"));
        if !self.is_welcome_screen {
            hints.push(hint("<ESC>", "close"));
        }
        let target = self.sessions_open_target();
        hints.extend(self.open_way_hints(Some(&target), false));
        hints.push(hint("<type>", "search, or name a new session"));
        hints.push(hint("<←→>", "show tabs and panes"));
        hints.push(hint("<Ctrl l>", "new session with a layout"));
        hints.push(hint("<Ctrl r>", "rename this session"));
        hints.push(hint("<Del>", "stop or delete the selected session"));
        if self.exited_session_count() > 0 {
            hints.push(hint("<Shift Del>", "delete all exited sessions"));
        }
        if !app_panel::is_enabled() && !self.is_welcome_screen {
            hints.push(hint(
                "<Ctrl a>",
                format!(
                    "save this session {}",
                    ui::components::saved_text(self.current_session_last_saved_time)
                ),
            ));
        }
        if self.sessions_live_target().is_some() {
            hints.push(hint(inbox::TYPE_KEY, "type into the shown pane"));
            hints.push(hint("<click the view>", "type into the shown pane"));
            hints.push(hint(
                self.inbox.release_key_text(),
                "stop typing (or click outside the view)",
            ));
        }
        if self.selected_session_is_exited() {
            hints.push(hint("<Shift ←↓↑→>", "scroll the preview"));
        }
        if all {
            if self.shows_browser_tabs() {
                hints.push(hint("<TAB>", "switch to the inbox"));
            }
            hints.push(hint("<Ctrl c>", "clear the search"));
            hints.push(hint("<F1>", "this list"));
        }
        hints
    }
    fn inbox_hints(&self, all: bool) -> Vec<help::Hint> {
        use help::hint;
        let target = self.inbox_open_target();
        if !all {
            if self.desktop_offer.shown {
                return vec![
                    hint("", "Get desktop notifications for this:"),
                    hint("<Alt w>", "open in a window (it opens on this computer)"),
                    hint("<x>", "don't show again"),
                    hint("<ESC>", "close"),
                ];
            }
            let mut hints: Vec<help::Hint> = self
                .open_way_hints(target.as_ref(), true)
                .into_iter()
                .filter(|h| h.key == "<Alt w>")
                .collect();
            hints.push(hint("<↓↑>", "select"));
            if self.shows_browser_tabs() {
                hints.push(hint("<TAB>", "sessions"));
            }
            hints.push(hint("<ESC>", "close"));
            hints.push(hint("<?>", "all keys"));
            return hints;
        }
        let item = self.inbox.selected_target();
        let muted = item.map(|i| i.item.attention.is_muted()).unwrap_or(false);
        let watched = item.map(|i| i.item.attention.watched).unwrap_or(false);
        let mut hints = if self.inbox.live_mode {
            vec![
                hint("<ENTER>", "go to the pane"),
                hint(inbox::TYPE_KEY, "type into the shown pane"),
                hint("<click the view>", "type into the shown pane"),
                hint(
                    "<z>",
                    format!(
                        "resize the pane to the view ({})",
                        if self.inbox.current_resize_allowed() {
                            "on"
                        } else {
                            "off"
                        }
                    ),
                ),
                hint(
                    self.inbox.release_key_text(),
                    "stop typing (or click outside the view)",
                ),
            ]
        } else {
            vec![hint("<ENTER>", "go to the pane")]
        };
        hints.extend([
            hint("<↓↑>", "select"),
            hint("<c>", "dismiss (mark seen)"),
            hint("<ESC>", "close"),
            hint("<m>", if muted { "unmute" } else { "mute" }),
            hint(
                "<n>",
                if watched {
                    "stop watching"
                } else {
                    "watch (notify me)"
                },
            ),
            hint("<C>", "dismiss everything"),
        ]);
        hints.extend(self.open_way_hints(target.as_ref(), true));
        let on_off = |on: bool| if on { "on" } else { "off" };
        hints.push(hint(
            "<f>",
            format!("this session only ({})", on_off(self.inbox.only_here)),
        ));
        hints.push(hint(
            "<s>",
            if self.inbox.group_by_session {
                "group by status"
            } else {
                "group by session"
            },
        ));
        hints.push(hint(
            "<a>",
            format!(
                "show items with only new output, quiet or muted ({})",
                on_off(self.inbox.show_quiet)
            ),
        ));
        hints.push(hint("<TAB>", "switch to sessions"));
        hints.push(hint("<?>", "this list"));
        hints
    }
    fn all_keys(&self) -> Vec<help::Hint> {
        if self.inbox_is_shown() {
            self.inbox_hints(true)
        } else {
            self.sessions_hints(true)
        }
    }
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
        if is_running && self.listed_current_session().as_deref() == Some(name) {
            match (pane, tab) {
                (Some((pane_id, true)), _) => focus_session_pane(name, PaneId::Plugin(pane_id)),
                (Some((pane_id, false)), _) => focus_session_pane(name, PaneId::Terminal(pane_id)),
                (None, Some(tab)) => go_to_tab(tab as u32),
                (None, None) => {},
            }
            return;
        }
        if self.open_disconnecting && is_running {
            switch_session_with_options(ConnectToSession {
                name: Some(name.to_owned()),
                tab_position: tab,
                pane_id: pane,
                switch_context: SwitchContext::from_plugin_request(None, true),
                ..Default::default()
            });
        } else if self.is_welcome_screen {
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
        let current = self
            .listed_current_session()
            .filter(|name| !suggestions.suggestions.iter().any(|s| s.name == *name));
        let mut ordered_names: Vec<String> = ordered.iter().map(|s| s.name.clone()).collect();
        if let Some(current) = current.as_ref() {
            let first_exited = ordered
                .iter()
                .position(|s| !s.is_running)
                .unwrap_or(ordered.len());
            ordered_names.insert(first_exited, current.clone());
            let tab_count = self.last_tab_count.unwrap_or(1).max(1);
            let pane_count = self.last_terminal_pane_count.unwrap_or(0);
            running.push(SessionUiInfo {
                name: current.clone(),
                tabs: (0..tab_count)
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
                    .collect(),
                connected_users: 1,
                is_current_session: true,
                creation_time: std::time::Duration::from_secs(0),
            });
        }
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
        self.panel.current_session = current;
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
            Some((name, is_running, _)) => {
                let fetch = !is_running || self.sessions_live_target().is_none();
                let (tab, pane) = if self.panel.tree.node_session == name {
                    match self.panel.tree.node {
                        session_tree::TreeNode::Exited
                        | session_tree::TreeNode::Recent
                        | session_tree::TreeNode::Session => (None, None),
                        session_tree::TreeNode::Tab(tab) => (Some(tab), None),
                        session_tree::TreeNode::Pane(tab, pane) => (Some(tab), Some(pane)),
                    }
                } else {
                    (None, None)
                };
                self.panel
                    .request_tree_preview(&name, is_running, tab, pane, fetch);
            },
            None => self.panel.clear_preview(),
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
            || self.pending_delete_exited
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
                    BareKey::Delete if key.has_no_modifiers() => Some(false),
                    BareKey::Char('r') if key.has_modifiers(&[KeyModifier::Ctrl]) => Some(false),
                    _ => self.handle_manager_key(key),
                }
            },
            PanelSize::Manager => self.handle_manager_key(key),
        }
    }
    fn handle_manager_key(&mut self, key: &KeyWithModifier) -> Option<bool> {
        if is_alt_enter(key) {
            return Some(self.inbox.start_typing());
        }
        if let Some(pan_move) = panel::pan_move_for_key(key) {
            if self.selected_session_is_exited() {
                self.panel.preview_pan.apply(pan_move, 0);
                return Some(true);
            }
        }
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
        let index = if self.panel.tree.is_heading(index) {
            index + 1
        } else {
            index
        };
        self.tree_point_at(index);
        self.panel.tree.follow = true;
    }
    fn is_click_on_selected(&self, index: usize) -> bool {
        let exited_header = self
            .panel
            .tree
            .rows
            .get(index)
            .map(|row| row.node == session_tree::TreeNode::Exited)
            .unwrap_or(false);
        exited_header || self.tree_cursor() == Some(index)
    }
    fn hover_tree_row(&mut self, index: Option<usize>) -> bool {
        let hovered = index
            .filter(|index| !self.panel.tree.is_heading(*index))
            .and_then(|index| self.panel.tree.identity(index));
        if hovered == self.panel.tree.hovered {
            return false;
        }
        self.panel.tree.hovered = hovered;
        true
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
        let mut next = match self.tree_cursor() {
            Some(cursor) => (cursor as isize + delta).clamp(0, len as isize - 1) as usize,
            None if delta < 0 => len - 1,
            None => 0,
        };
        if self.panel.tree.is_heading(next) && delta < 0 && next > 0 {
            next -= 1;
        }
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
            .rposition(|r| r.depth < row.depth)
            .filter(|parent| !self.panel.tree.is_heading(*parent));
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
            TreeNode::Recent | TreeNode::Session => return false,
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
        self.browser_is_open()
            && self.inbox.tab == BrowserTab::Inbox
            && self.renaming_session_name.is_none()
    }
    fn browser_is_open(&self) -> bool {
        self.panel.size == Some(PanelSize::Manager) && self.shows_browser_tabs()
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
            self.app_panel_auto_pending = app_panel::waits_for_items(&view);
            self.show_app_panel_view(Some(&view));
            return true;
        }
        if attention && self.app_panel_auto_pending && self.inbox.has_rows() {
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
        let keeps_the_inbox = self.app_panel_view_shown
            && !self.app_quit
            && self.panel.size == Some(PanelSize::Manager)
            && self.inbox.tab == BrowserTab::Inbox
            && matches!(view, None | Some("auto") | Some("inbox"));
        self.app_panel_view_shown = true;
        self.panel.tree.hovered = None;
        self.inbox.hovered = None;
        if keeps_the_inbox {
            return;
        }
        self.app_quit = view == Some("quit");
        self.panel.size = Some(PanelSize::Manager);
        if self.app_quit {
            self.set_app_panel_title("Quit");
            return;
        }
        self.single_screen_state.search_term.clear();
        self.single_screen_state.selected_index = None;
        let tab = match view {
            Some("sessions") => BrowserTab::Sessions,
            _ if !self.inbox.has_rows() => BrowserTab::Sessions,
            _ => BrowserTab::Inbox,
        };
        self.switch_browser_tab(tab);
        if tab == BrowserTab::Inbox && !self.inbox.is_capturing() {
            self.inbox.select_newest_counted();
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
        if self.inbox.tab != tab {
            self.inbox.leave_tab();
        }
        self.inbox.tab = tab;
        self.inbox.tab_chosen = true;
        self.inbox.select_newest_on_arrival = false;
        self.follow_app_panel_title();
        match tab {
            BrowserTab::Inbox => {},
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
            None if !self.inbox.has_rows() && !self.inbox_requested => {
                self.inbox.tab = BrowserTab::Sessions;
                self.inbox.tab_chosen = true;
            },
            None => {
                self.inbox.tab = BrowserTab::Inbox;
                self.inbox.tab_chosen = true;
                self.inbox.select_newest_counted();
            },
        }
        self.inbox_requested = false;
        self.go_to_size(PanelSize::Manager);
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
                self.inbox_requested = true;
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
    fn item_action(&mut self, action: ItemAction) -> bool {
        match action {
            ItemAction::Go => self.go_to_selected_inbox_item(),
            ItemAction::Type => {
                self.inbox.start_typing();
            },
            ItemAction::Dismiss => self.inbox.dismiss_selected(),
        }
        true
    }
    fn handle_inbox_key(&mut self, key: KeyWithModifier) -> bool {
        let plain = key.has_no_modifiers();
        let shift = key.has_only_modifiers(&[KeyModifier::Shift]);
        let live = self.inbox.shows_live_view();
        match key.bare_key {
            BareKey::Esc if plain => self.size_down(),
            BareKey::Tab if plain => self.switch_browser_tab(BrowserTab::Sessions),
            BareKey::Down if plain => self.inbox.move_selection(1),
            BareKey::Up if plain => self.inbox.move_selection(-1),
            BareKey::PageDown if plain => self.inbox.move_selection(10),
            BareKey::PageUp if plain => self.inbox.move_selection(-10),
            BareKey::Enter if is_alt_enter(&key) => {
                self.inbox.start_typing();
            },
            BareKey::Enter if plain => self.go_to_selected_inbox_item(),
            BareKey::Char('z') if plain && live => self.inbox.toggle_resize(),
            BareKey::Char('c') if plain => self.inbox.dismiss_selected(),
            BareKey::Char('C') if plain || shift => self.inbox.dismiss_all(),
            BareKey::Char('m') if plain => self.inbox.cycle_selected_mute(),
            BareKey::Char('f') if plain => self.inbox.flip_here_only(),
            BareKey::Char('a') if plain => self.inbox.toggle_show_quiet(),
            BareKey::Char('n') if plain => self.inbox.toggle_selected_watch(),
            BareKey::Char('x') if plain && self.desktop_offer.shown => {
                self.desktop_offer.shown = false;
                request_desktop_integration(&desktop_integration::Request::SetFlag(
                    desktop_integration::Flag::RemindersOff,
                ));
            },
            BareKey::Char('s') if plain => self.inbox.toggle_grouping(),
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
        let typing = self.inbox.is_capturing();
        match mouse {
            Mouse::LeftClick(line, column) => self.handle_inbox_click(line, column, typing),
            Mouse::ScrollDown(_) | Mouse::ScrollUp(_) if typing => false,
            Mouse::ScrollDown(lines) => {
                self.inbox.move_selection(lines.max(1) as isize);
                true
            },
            Mouse::ScrollUp(lines) => {
                self.inbox.move_selection(-(lines.max(1) as isize));
                true
            },
            Mouse::Hover(line, column) if !typing => {
                let hovered = match self.inbox.browser_target_at(line, column) {
                    Some(BrowserTarget::Item(key)) => Some(key),
                    _ => None,
                };
                if hovered == self.inbox.hovered {
                    return false;
                }
                self.inbox.hovered = hovered;
                true
            },
            _ => false,
        }
    }
    fn handle_inbox_click(&mut self, line: isize, column: usize, typing: bool) -> bool {
        match self.inbox.browser_target_at(line, column) {
            Some(BrowserTarget::Tab(tab)) => {
                self.switch_browser_tab(tab);
                true
            },
            Some(BrowserTarget::Item(key)) => {
                if !self.inbox.is_selected(&key) {
                    self.inbox.select_item(key);
                } else if !typing {
                    self.go_to_selected_inbox_item();
                }
                true
            },
            Some(BrowserTarget::LiveView) => {
                self.inbox.start_typing();
                true
            },
            Some(BrowserTarget::Action(action)) => self.item_action(action),
            Some(BrowserTarget::ShowQuiet) => {
                self.inbox.toggle_show_quiet();
                true
            },
            None => false,
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
                Mouse::LeftClick(line, column)
                    if self.inbox.browser_target_at(line, column)
                        == Some(BrowserTarget::LiveView) =>
                {
                    self.inbox.start_typing();
                    true
                },
                Mouse::LeftClick(line, column) => {
                    if let Some(index) = self.tree_marker_at(line, column) {
                        if let Some(row) = self.panel.tree.rows.get(index).cloned() {
                            self.panel.tree.toggle(&row);
                        }
                        return true;
                    }
                    let typing = self.inbox.is_capturing();
                    match self.tree_row_at(line, column) {
                        Some(index) if self.panel.tree.is_heading(index) => false,
                        Some(index) if typing => {
                            self.tree_point_at(index);
                            true
                        },
                        Some(index) if !self.is_click_on_selected(index) => {
                            self.tree_point_at(index);
                            true
                        },
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
                Mouse::Hover(..) | Mouse::ScrollDown(_) | Mouse::ScrollUp(_)
                    if self.inbox.is_capturing() =>
                {
                    false
                },
                Mouse::Hover(line, column) => {
                    let index = self.tree_row_at(line, column);
                    self.hover_tree_row(index)
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

fn is_user_choice(event: &Event) -> bool {
    match event {
        Event::Key(_) => true,
        Event::Mouse(mouse) => !matches!(mouse, Mouse::Hover(..) | Mouse::Release(..)),
        _ => false,
    }
}

#[cfg(test)]
mod user_choice_tests {
    use super::*;

    #[test]
    fn the_release_of_the_click_that_opened_the_panel_is_not_a_choice() {
        assert!(!is_user_choice(&Event::Mouse(Mouse::Release(-1, 0))));
        assert!(!is_user_choice(&Event::Mouse(Mouse::Hover(3, 4))));
        assert!(is_user_choice(&Event::Mouse(Mouse::LeftClick(3, 4))));
        assert!(is_user_choice(&Event::Mouse(Mouse::ScrollDown(1))));
        assert!(is_user_choice(&Event::Key(KeyWithModifier::new(
            BareKey::Down
        ))));
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

#[cfg(test)]
mod remote_window_tests {
    use super::*;

    #[test]
    fn the_remote_warning_says_where_the_window_opens_and_warns_before_detaching() {
        let lines = remote_window_warning_lines("from a terminal, over SSH from 10.0.0.7", false);
        assert_eq!(
            lines[0],
            "You are connected from a terminal, over SSH from 10.0.0.7."
        );
        assert!(lines.iter().any(|l| l.contains("you will not see it")));
        assert!(!lines.iter().any(|l| l.starts_with("Detaching")));
        let leaving = remote_window_warning_lines("from a terminal", true);
        assert!(leaving
            .iter()
            .any(|l| l.starts_with("Detaching here also ends")));
        assert_eq!(
            leaving.last().map(|l| l.as_str()),
            Some("<Enter> open it anyway    <Esc> cancel")
        );
    }
}

#[cfg(test)]
mod live_view_state_tests {
    use super::*;

    fn attention_item(session: &str, id: u32) -> AttentionItem {
        AttentionItem {
            pane_id: PaneId::Terminal(id),
            tab_id: 0,
            tab_name: "tab".to_owned(),
            pane_title: format!("pane{}", id),
            attention: PaneAttention {
                status: AttentionStatus::NeedsYou,
                text: format!("question from {}", session),
                since_ms: 100,
                ..Default::default()
            },
        }
    }

    fn browser() -> State {
        let mut state = State::default();
        state.is_visible = true;
        state.panel.size = Some(PanelSize::Manager);
        let session = |name: &str, id: u32| {
            let mut attention = SessionAttention::default();
            let item = attention_item(name, id);
            attention.counts.add(&item.attention);
            attention.items.push(item);
            (name.to_owned(), attention)
        };
        state.inbox.update_global(GlobalAttention::build(
            "here",
            vec![
                ("here".to_owned(), Default::default()),
                session("api", 1),
                session("web", 2),
            ],
        ));
        state.inbox.tab = BrowserTab::Inbox;
        state.inbox.tab_chosen = true;
        state.render_plugin(16, 88);
        assert!(
            state.inbox.live_view_sent.is_some(),
            "the inbox shows the live view"
        );
        state
    }

    #[test]
    fn the_all_keys_list_clears_the_live_view() {
        let mut state = browser();
        state.show_all_keys = true;
        state.render_plugin(16, 88);
        assert_eq!(state.inbox.live_view_sent, None);
        state.show_all_keys = false;
        state.render_plugin(16, 88);
        assert!(state.inbox.live_view_sent.is_some());
    }

    #[test]
    fn the_remote_window_warning_clears_the_live_view() {
        let mut state = browser();
        state.remote_client = Some("from a terminal".to_owned());
        state.pending_remote_window = Some(PendingWindow {
            request: SessionTerminalRequest::existing("api", None, true),
            leave: false,
            from_layout_list: false,
        });
        state.render_plugin(16, 88);
        assert_eq!(state.inbox.live_view_sent, None);
    }

    #[test]
    fn the_quit_question_clears_the_live_view() {
        let mut state = browser();
        state.app_quit = true;
        state.render_plugin(16, 88);
        assert_eq!(state.inbox.live_view_sent, None);
    }

    #[test]
    fn the_sessions_view_without_a_selected_session_clears_the_live_view() {
        let mut state = browser();
        state.switch_browser_tab(BrowserTab::Sessions);
        state.render_plugin(16, 88);
        assert_eq!(state.inbox.live_view_sent, None);
    }

    #[test]
    fn hiding_the_plugin_clears_the_live_view() {
        let mut state = browser();
        state.update(Event::Visible(false));
        assert_eq!(state.inbox.live_view_sent, None);
        state.render_plugin(16, 88);
        assert_eq!(
            state.inbox.live_view_sent, None,
            "a hidden plugin does not show the view again when it redraws"
        );
        state.update(Event::Visible(true));
        state.render_plugin(16, 88);
        assert!(state.inbox.live_view_sent.is_some());
    }

    fn only_web_is_left() -> GlobalAttention {
        let mut attention = SessionAttention::default();
        let item = attention_item("web", 2);
        attention.counts.add(&item.attention);
        attention.items.push(item);
        GlobalAttention::build(
            "here",
            vec![
                ("here".to_owned(), Default::default()),
                ("api".to_owned(), Default::default()),
                ("web".to_owned(), attention),
            ],
        )
    }

    fn nothing_is_left() -> GlobalAttention {
        GlobalAttention::build(
            "here",
            vec![
                ("here".to_owned(), Default::default()),
                ("api".to_owned(), Default::default()),
                ("web".to_owned(), Default::default()),
            ],
        )
    }

    #[test]
    fn an_item_read_in_the_inbox_stays_across_a_tab_switch_until_the_browser_closes() {
        let mut state = browser();
        let api = key_of("api", 1);
        state.inbox.update_global(only_web_is_left());
        state.settle_inbox_hold();
        assert!(state.inbox.is_held(&api));
        state.switch_browser_tab(BrowserTab::Sessions);
        state.render_plugin(16, 88);
        state.settle_inbox_hold();
        state.switch_browser_tab(BrowserTab::Inbox);
        state.render_plugin(16, 88);
        state.settle_inbox_hold();
        assert!(state.inbox.is_held(&api), "the tab switch keeps it");
        state.update(Event::Visible(false));
        assert!(!state.inbox.is_held(&api), "closing lets it go");
    }

    #[test]
    fn a_repeated_auto_view_keeps_the_inbox_and_the_item_just_read() {
        let mut state = browser();
        let api = key_of("api", 1);
        state.show_app_panel_view(Some("auto"));
        state.render_plugin(16, 88);
        assert!(state.inbox.is_selected(&api));
        state.inbox.update_global(nothing_is_left());
        assert_eq!(state.inbox.counted_count(), 0);
        state.show_app_panel_view(Some("auto"));
        assert_eq!(state.inbox.tab, BrowserTab::Inbox);
        assert!(state.inbox.is_selected(&api));
        state.show_app_panel_view(Some("inbox"));
        assert_eq!(state.inbox.tab, BrowserTab::Inbox);
        assert!(state.inbox.is_selected(&api));
        state.update(Event::Visible(false));
        state.show_app_panel_view(Some("auto"));
        assert_eq!(
            state.inbox.tab,
            BrowserTab::Sessions,
            "a fresh open with nothing left shows the sessions"
        );
    }

    #[test]
    fn a_narrow_browser_lists_the_items_without_a_live_view() {
        let mut state = browser();
        state.render_plugin(16, 50);
        assert_eq!(state.inbox.live_view_sent, None);
        assert!(!state.inbox.shows_live_view());
    }

    fn alt_enter() -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Enter).with_alt_modifier()
    }

    #[test]
    fn enter_goes_to_the_pane_and_alt_enter_types_into_the_view() {
        let mut state = browser();
        assert!(state.handle_inbox_key(alt_enter()));
        assert_eq!(
            state.inbox.capture_target,
            Some(ItemKey {
                session: "api".to_owned(),
                pane_id: PaneId::Terminal(1),
            })
        );
        assert_eq!(state.panel.size, Some(PanelSize::Manager));
        let mut go = browser();
        assert!(go.handle_inbox_key(KeyWithModifier::new(BareKey::Enter)));
        assert_eq!(go.inbox.capture_target, None);
        assert_eq!(go.panel.size, None);
        let mut narrow = browser();
        narrow.render_plugin(16, 50);
        narrow.handle_inbox_key(alt_enter());
        assert_eq!(narrow.inbox.capture_target, None);
        assert_eq!(narrow.panel.size, Some(PanelSize::Manager));
        narrow.handle_inbox_key(KeyWithModifier::new(BareKey::Enter));
        assert_eq!(narrow.panel.size, None);
    }

    #[test]
    fn g_and_right_do_nothing_in_the_inbox() {
        let mut state = browser();
        for bare_key in [BareKey::Char('g'), BareKey::Right] {
            assert!(!state.handle_inbox_key(KeyWithModifier::new(bare_key)));
            assert_eq!(state.inbox.capture_target, None);
            assert_eq!(state.panel.size, Some(PanelSize::Manager));
        }
    }

    #[test]
    fn the_inbox_key_line_and_all_keys_list_name_enter_and_alt_enter() {
        let mut state = browser();
        let hints = state.inbox_hints(true);
        let keys: Vec<&str> = hints.iter().map(|hint| hint.key.as_str()).collect();
        assert!(keys.contains(&"<ENTER>"));
        assert!(keys.contains(&inbox::TYPE_KEY));
        assert!(keys.contains(&"<click the view>"));
        assert!(keys.contains(&"<F9>"));
        assert!(!keys
            .iter()
            .any(|key| key.contains("<g>") || key.contains("→")));
        state.render_plugin(16, 88);
        assert_eq!(state.inbox.live_view_hint_text(), inbox::TYPE_INTO_IT_HINT);
    }

    fn key_of(session: &str, id: u32) -> ItemKey {
        ItemKey {
            session: session.to_owned(),
            pane_id: PaneId::Terminal(id),
        }
    }

    fn row_of(state: &State, key: &ItemKey) -> (isize, usize) {
        state
            .inbox
            .browser_targets
            .iter()
            .find(|(_, _, target)| *target == BrowserTarget::Item(key.clone()))
            .map(|(y, columns, _)| (*y as isize, columns.start))
            .expect("the row is drawn")
    }

    fn report(state: &mut State, captured: bool, released_by: Option<LiveViewRelease>) {
        let status = inbox::LiveViewStatus {
            target: Some(PaneAddress::local("api", PaneId::Terminal(1))),
            state: MirrorState::Live,
            input_captured: captured,
            input_released_by: released_by,
            resize_allowed: true,
            pinned: false,
            source_size: None,
            release_key: "F9".to_owned(),
        };
        state.inbox.apply_live_view_status(status);
        state.render_plugin(16, 88);
    }

    #[test]
    fn a_click_on_the_selected_row_that_ends_typing_neither_types_nor_goes_there() {
        let mut state = browser();
        let api = key_of("api", 1);
        assert!(state.inbox.is_selected(&api));
        let (line, column) = row_of(&state, &api);
        assert!(state.handle_inbox_key(alt_enter()));
        report(&mut state, true, None);
        assert!(state.handle_inbox_mouse(Mouse::LeftClick(line, column)));
        report(&mut state, false, Some(LiveViewRelease::ClickOutside));
        assert_eq!(
            state.inbox.capture_target, None,
            "the click that ended typing arrives before the release and does not start it again"
        );
        assert!(!state.inbox.is_capturing());
        assert_eq!(
            state.panel.size,
            Some(PanelSize::Manager),
            "the click that ended typing does not go to the pane"
        );
        state.handle_inbox_mouse(Mouse::LeftClick(line, column));
        assert_eq!(state.inbox.capture_target, None);
        assert_eq!(
            state.panel.size, None,
            "the next click on the selected row goes to the pane"
        );
    }

    #[test]
    fn a_click_on_the_selected_row_after_a_release_the_plugin_never_saw_goes_there() {
        for unseen in [
            LiveViewRelease::ClickOutside,
            LiveViewRelease::FocusLost,
            LiveViewRelease::Key,
        ] {
            let mut state = browser();
            let (line, column) = row_of(&state, &key_of("api", 1));
            assert!(state.handle_inbox_key(alt_enter()));
            report(&mut state, true, None);
            report(&mut state, false, Some(unseen));
            state.handle_inbox_mouse(Mouse::LeftClick(line, column));
            assert_eq!(state.inbox.capture_target, None);
            assert_eq!(
                state.panel.size, None,
                "the first click after a release by {:?} goes to the pane",
                unseen
            );
        }
    }

    #[test]
    fn clicking_a_row_selects_it_and_clicking_it_again_goes_there() {
        let mut state = browser();
        let web = key_of("web", 2);
        let (line, column) = row_of(&state, &web);
        assert!(state.handle_inbox_mouse(Mouse::LeftClick(line, column)));
        assert!(state.inbox.is_selected(&web));
        assert_eq!(state.inbox.capture_target, None, "the first click selects");
        state.render_plugin(16, 88);
        assert_eq!(
            state.panel.size,
            Some(PanelSize::Manager),
            "the first click does not go to the pane"
        );
        let (line, column) = row_of(&state, &web);
        assert!(state.handle_inbox_mouse(Mouse::LeftClick(line, column)));
        assert_eq!(state.inbox.capture_target, None, "a row click never types");
        assert_eq!(state.panel.size, None, "the second click goes to the pane");
    }

    #[test]
    fn a_hover_on_an_inbox_row_only_marks_it() {
        let mut state = browser();
        let web = key_of("web", 2);
        let selected = state.inbox.selection.clone();
        let (line, column) = row_of(&state, &web);
        assert!(state.handle_inbox_mouse(Mouse::Hover(line, column)));
        assert_eq!(state.inbox.hovered, Some(web.clone()));
        assert_eq!(state.inbox.selection, selected);
        assert!(!state.inbox.is_selected(&web));
        assert!(!state.handle_inbox_mouse(Mouse::Hover(line, column)));
    }

    #[test]
    fn while_typing_a_click_on_a_row_selects_it_without_starting_typing_and_the_wheel_does_nothing()
    {
        let mut state = browser();
        let (api, web) = (key_of("api", 1), key_of("web", 2));
        let (api_line, api_column) = row_of(&state, &api);
        let (web_line, web_column) = row_of(&state, &web);
        assert!(state.handle_inbox_key(alt_enter()));
        report(&mut state, true, None);
        assert!(state.inbox.is_capturing());
        assert!(!state.handle_inbox_mouse(Mouse::ScrollDown(1)));
        assert!(!state.handle_inbox_mouse(Mouse::ScrollUp(1)));
        assert_eq!(state.inbox.selection, None);
        assert!(state.inbox.is_selected(&api));
        state.handle_inbox_mouse(Mouse::LeftClick(api_line, api_column));
        assert_eq!(state.inbox.capture_target, Some(api.clone()));
        state.handle_inbox_mouse(Mouse::LeftClick(web_line, web_column));
        assert_eq!(state.inbox.selection, Some(web.clone()));
        assert_eq!(state.inbox.capture_target, Some(api));
        report(&mut state, false, Some(LiveViewRelease::Key));
        assert!(state.handle_inbox_mouse(Mouse::ScrollUp(1)));
        assert!(state.inbox.is_selected(&key_of("api", 1)));
    }
}

#[cfg(test)]
mod sessions_live_view_tests {
    use super::*;

    const ROWS: usize = 30;
    const COLS: usize = 120;

    fn pane(id: u32, title: &str, focused: bool, is_plugin: bool) -> SessionPreviewPane {
        SessionPreviewPane {
            id,
            is_plugin,
            title: title.to_owned(),
            focused,
            contents: None,
        }
    }

    fn tab(name: &str, active: bool, panes: Vec<SessionPreviewPane>) -> SessionPreviewTab {
        SessionPreviewTab {
            name: name.to_owned(),
            active,
            panes,
        }
    }

    fn suggestion(name: &str, is_running: bool, tabs: Vec<SessionPreviewTab>) -> SessionSuggestion {
        SessionSuggestion {
            name: name.to_owned(),
            is_running,
            tabs,
            ..Default::default()
        }
    }

    fn sessions_browser() -> State {
        let mut state = State::default();
        state.active_screen = ActiveScreen::SingleScreen;
        state.is_visible = true;
        state.panel.size = Some(PanelSize::Manager);
        state.inbox.tab = BrowserTab::Sessions;
        state.inbox.tab_chosen = true;
        state.handle_event(Event::SessionSuggestions(SessionSuggestions {
            suggestions: vec![
                suggestion(
                    "api",
                    true,
                    vec![
                        tab(
                            "main",
                            true,
                            vec![
                                pane(1, "shell", false, false),
                                pane(2, "server", true, false),
                            ],
                        ),
                        tab(
                            "tools",
                            false,
                            vec![
                                pane(5, "strider", true, true),
                                pane(6, "logs", false, false),
                            ],
                        ),
                    ],
                ),
                suggestion(
                    "web",
                    true,
                    vec![tab("main", true, vec![pane(3, "picker", true, true)])],
                ),
                suggestion("old", false, vec![]),
            ],
            ..Default::default()
        }));
        state.render_plugin(ROWS, COLS);
        state.tree_select(0);
        state.render_plugin(ROWS, COLS);
        state
    }

    fn shown(state: &State) -> Option<(String, PaneId)> {
        state
            .inbox
            .live_view_sent
            .as_ref()
            .filter(|view| !view.target.session.is_empty())
            .map(|view| (view.target.session.clone(), view.target.pane_id))
    }

    fn hidden(state: &State) -> bool {
        state
            .inbox
            .live_view_sent
            .as_ref()
            .map(|view| view.target.session.is_empty() && view.cols == 0 && view.rows == 0)
            .unwrap_or(false)
    }

    fn api(id: u32) -> Option<(String, PaneId)> {
        Some(("api".to_owned(), PaneId::Terminal(id)))
    }

    fn select(state: &mut State, index: usize) {
        state.tree_select(index);
        state.render_plugin(ROWS, COLS);
    }

    fn expand(state: &mut State) {
        state.tree_right();
        state.render_plugin(ROWS, COLS);
    }

    #[test]
    fn a_session_tab_or_pane_row_shows_its_pane_live_and_plugins_or_exited_sessions_do_not() {
        let mut state = sessions_browser();
        assert_eq!(shown(&state), api(2), "the focused pane of the active tab");
        assert!(state.sessions_live_layout.is_some());
        expand(&mut state);
        select(&mut state, 1);
        assert_eq!(
            shown(&state),
            api(2),
            "the focused terminal pane of the tab"
        );
        select(&mut state, 2);
        assert_eq!(
            shown(&state),
            api(6),
            "the first terminal pane of a tab whose focused pane is a plugin"
        );
        expand(&mut state);
        select(&mut state, 3);
        assert_eq!(shown(&state), None, "a plugin pane keeps the snapshot");
        assert!(
            hidden(&state),
            "the view is kept without a target so the browsed panes keep their size"
        );
        assert!(state.sessions_live_layout.is_none());
        select(&mut state, 4);
        assert_eq!(shown(&state), api(6));
        select(&mut state, 5);
        assert_eq!(
            shown(&state),
            None,
            "a session whose focused pane is a plugin keeps the snapshot"
        );
        select(&mut state, 6);
        assert_eq!(
            state.tree_cursor(),
            Some(7),
            "the Recent heading is skipped"
        );
        assert_eq!(
            state.selected_session(),
            Some(("old".to_owned(), false, false))
        );
        assert_eq!(
            shown(&state),
            None,
            "an exited session keeps its saved preview"
        );
        assert!(hidden(&state));
        assert!(state.sessions_live_layout.is_none());
    }

    #[test]
    fn the_sessions_view_is_cleared_whenever_its_preview_is_not_drawn() {
        let mut state = sessions_browser();
        assert_eq!(shown(&state), api(2));
        state.show_all_keys = true;
        state.render_plugin(ROWS, COLS);
        assert_eq!(state.inbox.live_view_sent, None, "the all keys list");
        state.show_all_keys = false;
        state.render_plugin(ROWS, COLS);
        assert_eq!(shown(&state), api(2));
        state.single_screen_state.mode = SingleScreenMode::SelectingLayout;
        state.render_plugin(ROWS, COLS);
        assert_eq!(state.inbox.live_view_sent, None, "the layout picker");
        state.single_screen_state.mode = SingleScreenMode::SearchAndSelect;
        state.render_plugin(ROWS, 50);
        assert_eq!(state.inbox.live_view_sent, None, "the narrow layout");
        state.render_plugin(ROWS, COLS);
        assert_eq!(shown(&state), api(2));
        state.is_welcome_screen = true;
        state.render_plugin(ROWS, COLS);
        assert_eq!(state.inbox.live_view_sent, None, "the welcome screen");
        state.is_welcome_screen = false;
        state.render_plugin(ROWS, COLS);
        assert_eq!(shown(&state), api(2));
        state.update(Event::Visible(false));
        assert_eq!(shown(&state), None, "hidden");
        state.is_visible = true;
        state.render_plugin(ROWS, COLS);
        assert_eq!(shown(&state), api(2));
        state.panel.size = None;
        state.render_plugin(ROWS, COLS);
        assert_eq!(shown(&state), None, "closed");
        assert_eq!(state.inbox.live_view_sent, None);
    }

    #[test]
    fn switching_between_the_inbox_and_the_sessions_moves_the_same_view() {
        let mut state = sessions_browser();
        let mut attention = SessionAttention::default();
        let item = AttentionItem {
            pane_id: PaneId::Terminal(7),
            tab_id: 0,
            tab_name: "tab".to_owned(),
            pane_title: "pane7".to_owned(),
            attention: PaneAttention {
                status: AttentionStatus::NeedsYou,
                text: "question".to_owned(),
                since_ms: 100,
                ..Default::default()
            },
        };
        attention.counts.add(&item.attention);
        attention.items.push(item);
        state.inbox.update_global(GlobalAttention::build(
            "here",
            vec![
                ("here".to_owned(), Default::default()),
                ("web".to_owned(), attention),
            ],
        ));
        state.render_plugin(ROWS, COLS);
        assert_eq!(shown(&state), api(2));
        state.switch_browser_tab(BrowserTab::Inbox);
        state.render_plugin(ROWS, COLS);
        assert_eq!(
            shown(&state),
            Some(("web".to_owned(), PaneId::Terminal(7))),
            "the inbox moves the view to its item"
        );
        assert!(state.inbox.sessions_target.is_none());
        state.switch_browser_tab(BrowserTab::Sessions);
        state.render_plugin(ROWS, COLS);
        assert_eq!(shown(&state), api(2), "the sessions move it back");
        state.switch_browser_tab(BrowserTab::Inbox);
        state.render_plugin(ROWS, COLS);
        assert!(state.inbox.start_typing());
        state.inbox.apply_live_view_status(inbox::LiveViewStatus {
            target: Some(PaneAddress::local("web", PaneId::Terminal(7))),
            state: MirrorState::Live,
            input_captured: true,
            input_released_by: None,
            resize_allowed: true,
            pinned: true,
            source_size: None,
            release_key: "F9".to_owned(),
        });
        state.render_plugin(ROWS, COLS);
        assert_eq!(shown(&state), Some(("web".to_owned(), PaneId::Terminal(7))));
        state.switch_browser_tab(BrowserTab::Sessions);
        state.render_plugin(ROWS, COLS);
        assert_eq!(
            shown(&state),
            api(2),
            "typing into the inbox item ends with the tab and the sessions show their own pane"
        );
        assert_eq!(state.inbox.capture_target, None);
    }

    #[test]
    fn a_click_on_the_view_types_into_its_pane_and_while_typing_rows_only_select() {
        let mut state = sessions_browser();
        let layout = state.sessions_live_layout.expect("the view is drawn");
        assert_eq!(state.inbox.live_view_hint_text(), inbox::TYPE_INTO_IT_HINT);
        assert!(state.handle_mouse(Mouse::LeftClick(
            (layout.top + 2) as isize,
            layout.box_x + 2
        )));
        let api_pane = ItemKey {
            session: "api".to_owned(),
            pane_id: PaneId::Terminal(2),
        };
        assert_eq!(state.inbox.capture_target, Some(api_pane.clone()));
        state.inbox.apply_live_view_status(inbox::LiveViewStatus {
            target: Some(PaneAddress::local("api", PaneId::Terminal(2))),
            state: MirrorState::Live,
            input_captured: true,
            input_released_by: None,
            resize_allowed: true,
            pinned: true,
            source_size: None,
            release_key: "F9".to_owned(),
        });
        state.render_plugin(ROWS, COLS);
        assert!(state.inbox.live_view_hint_text().starts_with("typing into"));
        let (web_line, web_range, _) = state
            .panel
            .tree
            .row_targets
            .iter()
            .find(|(_, _, index)| *index == 1)
            .cloned()
            .expect("the web row is drawn");
        assert!(!state.handle_mouse(Mouse::Hover(web_line as isize, web_range.start)));
        assert!(!state.handle_mouse(Mouse::ScrollDown(1)));
        assert_eq!(state.tree_cursor(), Some(0));
        assert!(state.handle_mouse(Mouse::LeftClick(web_line as isize, web_range.start)));
        assert_eq!(state.tree_cursor(), Some(1), "the click selects the row");
        assert_eq!(
            state.panel.size,
            Some(PanelSize::Manager),
            "the click that ends typing does not open the session"
        );
        assert_eq!(state.inbox.capture_target, Some(api_pane));
    }

    fn tree_row_target(state: &State, index: usize) -> (isize, usize) {
        let (line, range, _) = state
            .panel
            .tree
            .row_targets
            .iter()
            .find(|(_, _, row)| *row == index)
            .cloned()
            .expect("the row is drawn");
        (line as isize, range.start)
    }

    #[test]
    fn clicking_a_row_selects_it_and_clicking_it_again_attaches() {
        let mut state = sessions_browser();
        state.tree_select(1);
        state.render_plugin(ROWS, COLS);
        let (api_line, api_column) = tree_row_target(&state, 0);
        assert!(state.handle_mouse(Mouse::LeftClick(api_line, api_column)));
        assert_eq!(
            state.tree_cursor(),
            Some(0),
            "the first click selects the row"
        );
        assert!(
            state.single_screen_state.selected_index.is_some(),
            "the first click does not attach"
        );
        state.render_plugin(ROWS, COLS);
        assert!(state.sessions_live_layout.is_some());
        assert!(state.handle_mouse(Mouse::LeftClick(api_line, api_column)));
        assert_eq!(state.inbox.capture_target, None, "a row click never types");
        assert_eq!(
            state.single_screen_state.selected_index, None,
            "the second click attaches"
        );
    }

    #[test]
    fn a_hover_only_marks_the_row_and_leaves_the_selection_and_preview_alone() {
        let mut state = sessions_browser();
        state.tree_select(1);
        state.render_plugin(ROWS, COLS);
        let before = state.inbox.live_view_sent.clone();
        let (api_line, api_column) = tree_row_target(&state, 0);
        assert!(state.handle_mouse(Mouse::Hover(api_line, api_column)));
        assert_eq!(state.tree_cursor(), Some(1), "the hover does not select");
        assert_eq!(
            state.panel.tree.hovered,
            Some(("api".to_owned(), session_tree::TreeNode::Session))
        );
        assert!(!state.handle_mouse(Mouse::Hover(api_line, api_column)));
        state.render_plugin(ROWS, COLS);
        assert_eq!(state.inbox.live_view_sent, before, "the preview stays");
        assert!(state.handle_mouse(Mouse::LeftClick(api_line, api_column)));
        assert!(
            state.single_screen_state.selected_index.is_some(),
            "a click on a hovered row that is not selected only selects"
        );
        assert_eq!(state.tree_cursor(), Some(0));
        state.render_plugin(ROWS, COLS);
        assert!(state.handle_mouse(Mouse::LeftClick(api_line, api_column)));
        assert_eq!(state.single_screen_state.selected_index, None);
    }

    #[test]
    fn a_click_on_a_row_selected_with_the_keys_opens_it() {
        let mut state = sessions_browser();
        state.tree_move(1);
        state.tree_move(-1);
        state.render_plugin(ROWS, COLS);
        let (api_line, api_column) = tree_row_target(&state, 0);
        assert!(state.handle_mouse(Mouse::LeftClick(api_line, api_column)));
        assert_eq!(state.single_screen_state.selected_index, None);
        assert_eq!(state.inbox.capture_target, None);
    }

    #[test]
    fn a_click_on_the_row_selected_when_the_panel_opens_opens_it() {
        let mut state = sessions_browser();
        let (api_line, api_column) = tree_row_target(&state, 0);
        assert_eq!(state.tree_cursor(), Some(0));
        assert!(state.handle_mouse(Mouse::LeftClick(api_line, api_column)));
        assert_eq!(state.single_screen_state.selected_index, None);
    }

    #[test]
    fn the_recent_heading_cannot_be_selected_or_clicked() {
        let mut state = sessions_browser();
        let heading = state
            .panel
            .tree
            .rows
            .iter()
            .position(|row| row.node == session_tree::TreeNode::Recent)
            .expect("the exited session is recent");
        select(&mut state, heading - 1);
        state.tree_move(1);
        state.render_plugin(ROWS, COLS);
        assert_eq!(state.tree_cursor(), Some(heading + 1));
        state.tree_move(-1);
        state.render_plugin(ROWS, COLS);
        assert_eq!(state.tree_cursor(), Some(heading - 1));
        let (line, column) = tree_row_target(&state, heading);
        assert!(!state.handle_mouse(Mouse::LeftClick(line, column)));
        assert!(!state.handle_mouse(Mouse::Hover(line, column)));
        assert_eq!(state.panel.tree.hovered, None);
    }

    #[test]
    fn alt_enter_types_into_the_shown_pane_and_enter_still_attaches() {
        let mut state = sessions_browser();
        let alt_enter = KeyWithModifier::new(BareKey::Enter).with_alt_modifier();
        assert_eq!(state.handle_manager_key(&alt_enter), Some(true));
        assert_eq!(
            state.inbox.capture_target,
            Some(ItemKey {
                session: "api".to_owned(),
                pane_id: PaneId::Terminal(2),
            })
        );
        assert_eq!(state.panel.size, Some(PanelSize::Manager));
        let mut exited = sessions_browser();
        select(&mut exited, 3);
        assert!(exited.selected_session_is_exited());
        assert_eq!(exited.handle_manager_key(&alt_enter), Some(false));
        assert_eq!(exited.inbox.capture_target, None);
    }

    #[test]
    fn the_all_keys_list_says_how_to_type_into_the_view_and_shift_keys_scroll_only_saved_previews()
    {
        let mut state = sessions_browser();
        let hints = state.sessions_hints(true);
        assert!(hints
            .iter()
            .any(|hint| hint.key == "<click the view>" && hint.label.contains("type")));
        assert!(hints
            .iter()
            .any(|hint| hint.key == inbox::TYPE_KEY && hint.label.contains("type")));
        assert!(hints.iter().any(|hint| hint.key == "<F9>"));
        assert!(!hints.iter().any(|hint| hint.key == "<Shift ←↓↑→>"));
        let shift_up = KeyWithModifier::new(BareKey::Up).with_shift_modifier();
        assert_eq!(state.handle_manager_key(&shift_up), None);
        assert_eq!(state.panel.preview_pan, panel::PreviewPan::default());
        select(&mut state, 3);
        assert!(state.selected_session_is_exited());
        let hints = state.sessions_hints(true);
        assert!(hints.iter().any(|hint| hint.key == "<Shift ←↓↑→>"));
        assert!(!hints.iter().any(|hint| hint.key == "<click the view>"));
        assert!(!hints.iter().any(|hint| hint.key == inbox::TYPE_KEY));
        assert_eq!(state.handle_manager_key(&shift_up), Some(true));
        assert!(state.panel.preview_pan.rows_up > 0);
    }

    fn with_current_session() -> (State, usize) {
        let mut state = sessions_browser();
        state.set_session_name("here");
        state.panel.tree.set_children_from_tabs(
            "here",
            &[tab("main", true, vec![pane(8, "editor", true, false)])],
        );
        state.render_plugin(ROWS, COLS);
        let index = state
            .panel
            .tree
            .rows
            .iter()
            .position(|row| row.session == "here")
            .expect("the current session is listed");
        (state, index)
    }

    fn sent_resize_allowed(state: &State) -> Option<bool> {
        state
            .inbox
            .live_view_sent
            .as_ref()
            .and_then(|view| view.resize_allowed)
    }

    #[test]
    fn the_current_session_is_listed_and_its_preview_does_not_resize_its_panes() {
        let (mut state, index) = with_current_session();
        let sessions: Vec<String> = state
            .panel
            .tree
            .rows
            .iter()
            .filter(|row| row.node == session_tree::TreeNode::Session)
            .map(|row| row.session.clone())
            .collect();
        assert_eq!(sessions, vec!["api", "web", "here", "old"]);
        select(&mut state, index);
        assert_eq!(
            shown(&state),
            Some(("here".to_owned(), PaneId::Terminal(8)))
        );
        assert_eq!(sent_resize_allowed(&state), Some(false));
        select(&mut state, 0);
        assert_eq!(shown(&state), api(2));
        assert_eq!(sent_resize_allowed(&state), None);
    }

    #[test]
    fn opening_the_current_session_closes_the_browser_and_it_cannot_be_stopped_from_here() {
        let (mut state, index) = with_current_session();
        select(&mut state, index);
        state.handle_key(KeyWithModifier::new(BareKey::Delete));
        assert_eq!(state.pending_session_stop, None);
        assert!(state.error.is_some());
        state.error = None;
        state.handle_key(KeyWithModifier::new(BareKey::Enter));
        assert_eq!(state.panel.size, None);
    }

    #[test]
    fn shift_delete_asks_before_deleting_every_exited_session() {
        let mut state = sessions_browser();
        assert_eq!(state.exited_session_count(), 1);
        let shift_delete = KeyWithModifier::new(BareKey::Delete).with_shift_modifier();
        state.handle_key(shift_delete.clone());
        assert!(state.pending_delete_exited);
        assert_eq!(
            delete_exited_warning_lines(1).0,
            "This will delete all 1 exited session"
        );
        assert!(state
            .sessions_hints(false)
            .iter()
            .any(|hint| hint.key == "<y>"));
        state.handle_key(KeyWithModifier::new(BareKey::Char('n')));
        assert!(!state.pending_delete_exited);
        assert_eq!(state.exited_session_count(), 1);
        let heading = state
            .panel
            .tree
            .rows
            .iter()
            .position(|row| row.node == session_tree::TreeNode::Recent)
            .expect("the exited session is listed");
        select(&mut state, heading + 1);
        state.handle_key(shift_delete);
        assert!(
            state.pending_delete_exited,
            "it also works from an exited row"
        );
    }

    #[test]
    fn the_current_session_is_not_listed_on_the_welcome_screen() {
        let mut state = sessions_browser();
        state.is_welcome_screen = true;
        state.set_session_name("here");
        assert!(!state
            .panel
            .tree
            .rows
            .iter()
            .any(|row| row.session == "here"));
    }
}

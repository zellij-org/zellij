mod main_screen;
mod token_management_screen;
mod token_screen;
mod ui_components;

use std::net::IpAddr;
use zellij_tile::prelude::*;

use std::collections::{BTreeMap, HashMap};

use main_screen::MainScreen;
use token_management_screen::TokenManagementScreen;
use token_screen::TokenScreen;

static WEB_SERVER_QUERY_DURATION: f64 = 0.4; // Doherty threshold

#[derive(Debug, Default)]
struct App {
    web_server: WebServerState,
    ui: UIState,
    tokens: TokenManager,
    state: AppState,
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        self.initialize();
    }

    fn update(&mut self, event: Event) -> bool {
        if !self.web_server.capability && !matches!(event, Event::ModeUpdate(_)) {
            return false;
        }

        match event {
            Event::Timer(_) => self.handle_timer(),
            Event::ModeUpdate(mode_info) => self.handle_mode_update(mode_info),
            Event::WebServerStatus(status) => self.handle_web_server_status(status),
            Event::Key(key) => self.handle_key_input(key),
            Event::Mouse(mouse_event) => self.handle_mouse_event(mouse_event),
            Event::RunCommandResult(exit_code, _stdout, _stderr, context) => {
                self.handle_command_result(exit_code, context)
            },
            Event::FailedToStartWebServer(error) => self.handle_web_server_error(error),
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if !self.web_server.capability {
            self.render_no_capability_message(rows, cols);
            return;
        }

        self.ui.reset_render_state();
        match &self.state.current_screen {
            Screen::Main => self.render_main_screen(rows, cols),
            Screen::Token(token) => self.render_token_screen(rows, cols, token),
            Screen::ManageTokens => self.render_manage_tokens_screen(rows, cols),
        }
    }
}

impl App {
    fn initialize(&mut self) {
        self.subscribe_to_events();
        self.state.own_plugin_id = Some(get_plugin_ids().plugin_id);
        self.retrieve_token_list();
        self.query_link_executable();
        self.set_plugin_title();
    }

    fn subscribe_to_events(&self) {
        subscribe(&[
            EventType::Key,
            EventType::ModeUpdate,
            EventType::WebServerStatus,
            EventType::Mouse,
            EventType::RunCommandResult,
            EventType::FailedToStartWebServer,
            EventType::Timer,
        ]);
    }

    fn set_plugin_title(&self) {
        if let Some(plugin_id) = self.state.own_plugin_id {
            rename_plugin_pane(plugin_id, "Share Session");
        }
    }

    fn handle_timer(&mut self) -> bool {
        query_web_server_status();
        self.retrieve_token_list();
        set_timeout(WEB_SERVER_QUERY_DURATION);
        false
    }

    fn handle_mode_update(&mut self, mode_info: ModeInfo) -> bool {
        let mut should_render = false;

        self.state.session_name = mode_info.session_name;

        if let Some(web_clients_allowed) = mode_info.web_clients_allowed {
            self.web_server.clients_allowed = web_clients_allowed;
            should_render = true;
        }

        if let Some(web_sharing) = mode_info.web_sharing {
            self.web_server.sharing = web_sharing;
            should_render = true;
        }

        if let Some(web_server_ip) = mode_info.web_server_ip {
            self.web_server.ip = Some(web_server_ip);
            should_render = true;
        }

        if let Some(web_server_port) = mode_info.web_server_port {
            self.web_server.port = Some(web_server_port);
            should_render = true;
        }

        if let Some(web_server_capability) = mode_info.web_server_capability {
            self.web_server.capability = web_server_capability;
            if self.web_server.capability && !self.state.timer_running {
                self.state.timer_running = true;
                set_timeout(WEB_SERVER_QUERY_DURATION);
            }
            should_render = true;
        }

        should_render
    }

    fn handle_web_server_status(&mut self, status: WebServerStatus) -> bool {
        match status {
            WebServerStatus::Online(base_url) => {
                self.web_server.base_url = base_url;
                self.web_server.started = true;
                self.web_server.different_version_error = None;
            },
            WebServerStatus::Offline => {
                self.web_server.started = false;
                self.web_server.different_version_error = None;
            },
            WebServerStatus::DifferentVersion(version) => {
                self.web_server.started = false;
                self.web_server.different_version_error = Some(version);
            },
        }
        true
    }

    fn handle_key_input(&mut self, key: KeyWithModifier) -> bool {
        if self.clear_error_or_info() {
            return true;
        }

        match self.state.current_screen {
            Screen::Main => self.handle_main_screen_keys(key),
            Screen::Token(_) => self.handle_token_screen_keys(key),
            Screen::ManageTokens => self.handle_manage_tokens_keys(key),
        }
    }

    fn clear_error_or_info(&mut self) -> bool {
        self.web_server.error.take().is_some() || self.state.info.take().is_some()
    }

    fn handle_main_screen_keys(&mut self, key: KeyWithModifier) -> bool {
        match key.bare_key {
            BareKey::Enter if key.has_no_modifiers() && !self.web_server.started => {
                start_web_server();
                false
            },
            BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                stop_web_server();
                false
            },
            BareKey::Char(' ') if key.has_no_modifiers() => {
                self.toggle_session_sharing();
                false
            },
            BareKey::Char('t') if key.has_no_modifiers() => {
                self.handle_token_action();
                true
            },
            BareKey::Esc if key.has_no_modifiers() => {
                close_self();
                false
            },
            _ => false,
        }
    }

    fn toggle_session_sharing(&self) {
        match self.web_server.sharing {
            WebSharing::Disabled => {},
            WebSharing::On => stop_sharing_current_session(),
            WebSharing::Off => share_current_session(),
        }
    }

    fn handle_token_action(&mut self) {
        if self.tokens.list.is_empty() {
            self.generate_new_token(None, false);
        } else {
            self.change_to_manage_tokens_screen();
        }
    }

    fn handle_token_screen_keys(&mut self, key: KeyWithModifier) -> bool {
        match key.bare_key {
            BareKey::Esc if key.has_no_modifiers() => {
                self.change_to_previous_screen();
                true
            },
            _ => false,
        }
    }

    fn handle_manage_tokens_keys(&mut self, key: KeyWithModifier) -> bool {
        if self.tokens.handle_text_input(&key) {
            return true;
        }

        match key.bare_key {
            BareKey::Esc if key.has_no_modifiers() => self.handle_escape_key(),
            BareKey::Down if key.has_no_modifiers() => self.tokens.navigate_down(),
            BareKey::Up if key.has_no_modifiers() => self.tokens.navigate_up(),
            BareKey::Char('n') if key.has_no_modifiers() => {
                self.tokens.start_new_token_input();
                true
            },
            BareKey::Char('o') if key.has_no_modifiers() => {
                self.tokens.start_new_read_only_token_input();
                true
            },
            BareKey::Enter if key.has_no_modifiers() => self.handle_enter_key(),
            BareKey::Char('r') if key.has_no_modifiers() => {
                self.tokens.start_rename_input();
                true
            },
            BareKey::Char('x') if key.has_no_modifiers() => self.revoke_selected_token(),
            BareKey::Char('x') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.revoke_all_tokens();
                true
            },
            _ => false,
        }
    }

    fn handle_escape_key(&mut self) -> bool {
        let was_editing = self.tokens.cancel_input();

        if !was_editing {
            self.change_to_main_screen();
        }
        true
    }

    fn handle_enter_key(&mut self) -> bool {
        if let Some(token_name) = self.tokens.finish_new_token_input() {
            self.generate_new_token(token_name, false);
            return true;
        }

        if let Some(token_name) = self.tokens.finish_new_read_only_token_input() {
            self.generate_new_token(token_name, true);
            return true;
        }

        if let Some(new_name) = self.tokens.finish_rename_input() {
            self.rename_current_token(new_name);
            return true;
        }

        false
    }

    fn generate_new_token(&mut self, name: Option<String>, read_only: bool) {
        match generate_web_login_token(name, read_only) {
            Ok(token) => self.change_to_token_screen(token),
            Err(e) => self.web_server.error = Some(e),
        }
    }

    fn rename_current_token(&mut self, new_name: String) {
        if let Some(current_token) = self.tokens.get_selected_token() {
            match rename_web_token(&current_token.0, &new_name) {
                Ok(_) => {
                    self.retrieve_token_list();
                    if self.tokens.adjust_selection_after_list_change() {
                        self.change_to_main_screen();
                    }
                },
                Err(e) => self.web_server.error = Some(e),
            }
        }
    }

    fn revoke_selected_token(&mut self) -> bool {
        if let Some(token) = self.tokens.get_selected_token() {
            match revoke_web_login_token(&token.0) {
                Ok(_) => {
                    self.retrieve_token_list();
                    if self.tokens.adjust_selection_after_list_change() {
                        self.change_to_main_screen();
                    }
                    self.state.info = Some("Revoked. Connected clients not affected.".to_owned());
                },
                Err(e) => self.web_server.error = Some(e),
            }
            return true;
        }
        false
    }

    fn revoke_all_tokens(&mut self) {
        match revoke_all_web_tokens() {
            Ok(_) => {
                self.retrieve_token_list();
                if self.tokens.adjust_selection_after_list_change() {
                    self.change_to_main_screen();
                }
                self.state.info = Some("Revoked. Connected clients not affected.".to_owned());
            },
            Err(e) => self.web_server.error = Some(e),
        }
    }

    fn handle_mouse_event(&mut self, event: Mouse) -> bool {
        match event {
            Mouse::LeftClick(line, column) => self.handle_link_click(line, column),
            Mouse::Hover(line, column) => {
                self.ui.hover_coordinates = Some((column, line as usize));
                true
            },
            _ => false,
        }
    }

    fn handle_link_click(&mut self, line: isize, column: usize) -> bool {
        for (coordinates, url) in &self.ui.clickable_urls {
            if coordinates.contains(column, line as usize) {
                if let Some(executable) = self.ui.link_executable {
                    run_command(&[executable, url], Default::default());
                }
                return true;
            }
        }
        false
    }

    fn handle_command_result(
        &mut self,
        exit_code: Option<i32>,
        context: BTreeMap<String, String>,
    ) -> bool {
        if context.contains_key("xdg_open_cli") && exit_code == Some(0) {
            self.ui.link_executable = Some("xdg-open");
        } else if context.contains_key("open_cli") && exit_code == Some(0) {
            self.ui.link_executable = Some("open");
        }
        false
    }

    fn handle_web_server_error(&mut self, error: String) -> bool {
        self.web_server.error = Some(error);
        true
    }

    fn query_link_executable(&self) {
        let mut xdg_context = BTreeMap::new();
        xdg_context.insert("xdg_open_cli".to_owned(), String::new());
        run_command(&["xdg-open", "--help"], xdg_context);

        let mut open_context = BTreeMap::new();
        open_context.insert("open_cli".to_owned(), String::new());
        run_command(&["open", "--help"], open_context);
    }

    fn render_no_capability_message(&self, rows: usize, cols: usize) {
        let full_text = "This version of Zellij was compiled without web sharing capabilities";
        let short_text = "No web server capabilities";
        let text = if cols >= full_text.chars().count() {
            full_text
        } else {
            short_text
        };

        let text_element = Text::new(text).color_range(3, ..);
        let text_x = cols.saturating_sub(text.chars().count()) / 2;
        let text_y = rows / 2;
        print_text_with_coordinates(text_element, text_x, text_y, None, None);
    }

    fn change_to_token_screen(&mut self, token: String) {
        self.retrieve_token_list();
        set_self_mouse_selection_support(true);
        self.state.previous_screen = Some(self.state.current_screen.clone());
        self.state.current_screen = Screen::Token(token);
    }

    fn change_to_manage_tokens_screen(&mut self) {
        self.retrieve_token_list();
        set_self_mouse_selection_support(false);
        self.tokens.selected_index = Some(0);
        self.state.previous_screen = None;
        self.state.current_screen = Screen::ManageTokens;
    }

    fn change_to_main_screen(&mut self) {
        self.retrieve_token_list();
        set_self_mouse_selection_support(false);
        self.state.previous_screen = None;
        self.state.current_screen = Screen::Main;
    }

    fn change_to_previous_screen(&mut self) {
        self.retrieve_token_list();
        match self.state.previous_screen.take() {
            Some(Screen::ManageTokens) => self.change_to_manage_tokens_screen(),
            _ => self.change_to_main_screen(),
        }
    }

    fn render_main_screen(&mut self, rows: usize, cols: usize) {
        let state_changes = MainScreen::new(
            self.tokens.list.is_empty(),
            self.web_server.started,
            &self.web_server.error,
            &self.web_server.different_version_error,
            &self.web_server.base_url,
            self.web_server.ip,
            self.web_server.port,
            &self.state.session_name,
            self.web_server.sharing,
            self.ui.hover_coordinates,
            &self.state.info,
            &self.ui.link_executable,
        )
        .render(rows, cols);

        self.ui.currently_hovering_over_link = state_changes.currently_hovering_over_link;
        self.ui.currently_hovering_over_unencrypted =
            state_changes.currently_hovering_over_unencrypted;
        self.ui.clickable_urls = state_changes.clickable_urls;
    }

    fn render_token_screen(&self, rows: usize, cols: usize, token: &str) {
        let token_screen =
            TokenScreen::new(token.to_string(), self.web_server.error.clone(), rows, cols);
        token_screen.render();
    }

    fn render_manage_tokens_screen(&self, rows: usize, cols: usize) {
        // Pass whichever token input field is active (normal or read-only)
        let entering_new_token_name = if self.tokens.entering_new_name.is_some() {
            &self.tokens.entering_new_name
        } else {
            &self.tokens.entering_new_read_only_name
        };

        TokenManagementScreen::new(
            &self.tokens.list,
            self.tokens.selected_index,
            &self.tokens.renaming_token,
            entering_new_token_name,
            &self.web_server.error,
            &self.state.info,
            rows,
            cols,
        )
        .render();
    }

    fn retrieve_token_list(&mut self) {
        if let Err(e) = self.tokens.retrieve_list() {
            self.web_server.error = Some(e);
        }
    }
}

#[derive(Debug, Default)]
struct WebServerState {
    started: bool,
    sharing: WebSharing,
    clients_allowed: bool,
    error: Option<String>,
    different_version_error: Option<String>,
    ip: Option<IpAddr>,
    port: Option<u16>,
    base_url: String,
    capability: bool,
}

#[derive(Debug, Default)]
struct UIState {
    hover_coordinates: Option<(usize, usize)>,
    clickable_urls: HashMap<CoordinatesInLine, String>,
    link_executable: Option<&'static str>,
    currently_hovering_over_link: bool,
    currently_hovering_over_unencrypted: bool,
}

impl UIState {
    fn reset_render_state(&mut self) {
        self.currently_hovering_over_link = false;
        self.clickable_urls.clear();
    }
}

#[derive(Debug, Default)]
struct TokenManager {
    list: Vec<(String, String, bool)>, // bool -> is_read_only
    selected_index: Option<usize>,
    entering_new_name: Option<String>,
    entering_new_read_only_name: Option<String>,
    renaming_token: Option<String>,
}

impl TokenManager {
    fn retrieve_list(&mut self) -> Result<(), String> {
        match list_web_login_tokens() {
            Ok(tokens) => {
                self.list = tokens;
                Ok(())
            },
            Err(e) => Err(format!("Failed to retrieve login tokens: {}", e)),
        }
    }

    fn get_selected_token(&self) -> Option<&(String, String, bool)> {
        self.selected_index.and_then(|i| self.list.get(i))
    }

    fn adjust_selection_after_list_change(&mut self) -> bool {
        if self.list.is_empty() {
            self.selected_index = None;
            true // indicates should change to main screen
        } else if self.selected_index >= Some(self.list.len()) {
            self.selected_index = Some(self.list.len().saturating_sub(1));
            false
        } else {
            false
        }
    }

    fn navigate_down(&mut self) -> bool {
        if let Some(ref mut index) = self.selected_index {
            *index = if *index < self.list.len().saturating_sub(1) {
                *index + 1
            } else {
                0
            };
            return true;
        }
        false
    }

    fn navigate_up(&mut self) -> bool {
        if let Some(ref mut index) = self.selected_index {
            *index = if *index == 0 {
                self.list.len().saturating_sub(1)
            } else {
                *index - 1
            };
            return true;
        }
        false
    }

    fn start_new_token_input(&mut self) {
        self.entering_new_name = Some(String::new());
    }

    fn start_new_read_only_token_input(&mut self) {
        self.entering_new_read_only_name = Some(String::new());
    }

    fn start_rename_input(&mut self) {
        self.renaming_token = Some(String::new());
    }

    fn handle_text_input(&mut self, key: &KeyWithModifier) -> bool {
        match key.bare_key {
            BareKey::Char(c) if key.has_no_modifiers() => {
                if let Some(ref mut name) = self.entering_new_name {
                    name.push(c);
                    return true;
                }
                if let Some(ref mut name) = self.entering_new_read_only_name {
                    name.push(c);
                    return true;
                }
                if let Some(ref mut name) = self.renaming_token {
                    name.push(c);
                    return true;
                }
            },
            BareKey::Backspace if key.has_no_modifiers() => {
                if let Some(ref mut name) = self.entering_new_name {
                    name.pop();
                    return true;
                }
                if let Some(ref mut name) = self.entering_new_read_only_name {
                    name.pop();
                    return true;
                }
                if let Some(ref mut name) = self.renaming_token {
                    name.pop();
                    return true;
                }
            },
            _ => {},
        }
        false
    }

    fn finish_new_token_input(&mut self) -> Option<Option<String>> {
        self.entering_new_name
            .take()
            .map(|name| if name.is_empty() { None } else { Some(name) })
    }

    fn finish_new_read_only_token_input(&mut self) -> Option<Option<String>> {
        self.entering_new_read_only_name
            .take()
            .map(|name| if name.is_empty() { None } else { Some(name) })
    }

    fn finish_rename_input(&mut self) -> Option<String> {
        self.renaming_token.take()
    }

    fn cancel_input(&mut self) -> bool {
        self.entering_new_name.take().is_some()
            || self.entering_new_read_only_name.take().is_some()
            || self.renaming_token.take().is_some()
    }
}

#[derive(Debug, Default)]
struct AppState {
    session_name: Option<String>,
    own_plugin_id: Option<u32>,
    timer_running: bool,
    current_screen: Screen,
    previous_screen: Option<Screen>,
    info: Option<String>,
}

#[derive(Debug, Clone)]
enum Screen {
    Main,
    Token(String),
    ManageTokens,
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Main
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct CoordinatesInLine {
    x: usize,
    y: usize,
    width: usize,
}

impl CoordinatesInLine {
    pub fn new(x: usize, y: usize, width: usize) -> Self {
        CoordinatesInLine { x, y, width }
    }

    pub fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x && x <= self.x + self.width && self.y == y
    }
}

mod main_screen;
mod token_screen;
mod token_management_screen;
mod ui_components;

use std::net::IpAddr;
use zellij_tile::prelude::*;

use std::collections::{BTreeMap, HashMap};

use token_management_screen::TokenManagementScreen;
use main_screen::MainScreen;
use token_screen::TokenScreen;

static WEB_SERVER_QUERY_DURATION: f64 = 0.4; // Doherty threshold

#[derive(Debug, Default)]
struct App {
    web_server_started: bool,
    web_sharing: WebSharing,
    web_clients_allowed: bool,
    session_name: Option<String>,
    web_server_error: Option<String>,
    web_server_different_version_error: Option<String>,
    web_server_ip: Option<IpAddr>,
    web_server_port: Option<u16>,
    web_server_base_url: String,
    hover_coordinates: Option<(usize, usize)>, // x, y
    clickable_urls: HashMap<CoordinatesInLine, String>,
    link_executable: Option<&'static str>,
    currently_hovering_over_link: bool,
    currently_hovering_over_unencrypted: bool,
    own_plugin_id: Option<u32>,
    web_server_capability: bool,
    timer_running: bool,
    current_screen: Screen,
    token_list: Vec<(String, String)>, // (name, created_at)
    selected_list_index: Option<usize>,
    entering_new_token_name: Option<String>,
    renaming_token: Option<String>,
    info: Option<String>,
    previous_screen: Option<Screen>,
}

#[derive(Debug, Clone)]
enum Screen {
    Main,
    Token(String), // String - the newly generated token for display
    ManageTokens,
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Main
    }
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::ModeUpdate,
            EventType::WebServerStatus,
            EventType::Mouse,
            EventType::RunCommandResult,
            EventType::FailedToStartWebServer,
            EventType::Timer,
        ]);
        self.own_plugin_id = Some(get_plugin_ids().plugin_id);
        self.retrieve_token_list();
        self.query_link_executable();
        self.change_own_title();
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::Timer(_) => {
                query_web_server_status();
                self.retrieve_token_list();
                set_timeout(WEB_SERVER_QUERY_DURATION);
            },
            Event::ModeUpdate(mode_info) => {
                self.session_name = mode_info.session_name;
                if let Some(web_clients_allowed) = mode_info.web_clients_allowed {
                    self.web_clients_allowed = web_clients_allowed;
                    should_render = true;
                }
                if let Some(web_sharing) = mode_info.web_sharing {
                    self.web_sharing = web_sharing;
                    should_render = true;
                }
                if let Some(web_server_ip) = mode_info.web_server_ip {
                    self.web_server_ip = Some(web_server_ip);
                    should_render = true;
                }
                if let Some(web_server_port) = mode_info.web_server_port {
                    self.web_server_port = Some(web_server_port);
                    should_render = true;
                }
                if let Some(web_server_capability) = mode_info.web_server_capability {
                    self.web_server_capability = web_server_capability;
                    if self.web_server_capability && !self.timer_running {
                        self.timer_running = true;
                        set_timeout(WEB_SERVER_QUERY_DURATION);
                    }
                    should_render = true;
                }
            },
            Event::WebServerStatus(web_server_status) => {
                if !self.web_server_capability {
                    return false;
                }
                match web_server_status {
                    WebServerStatus::Online(base_url) => {
                        self.web_server_base_url = base_url;
                        self.web_server_started = true;
                        self.web_server_different_version_error = None;
                    },
                    WebServerStatus::Offline => {
                        self.web_server_started = false;
                        self.web_server_different_version_error = None;
                    },
                    WebServerStatus::DifferentVersion(different_version) => {
                        self.web_server_started = false;
                        self.web_server_different_version_error = Some(different_version);
                    },
                }
                should_render = true;
            },
            Event::Key(key) => {
                if !self.web_server_capability {
                    return false;
                }
                if self.web_server_error.take().is_some() {
                    // clear the error with any key
                    return true;
                }
                if self.info.take().is_some() {
                    // clear info message with any key
                    return true;
                }
                match self.current_screen {
                    Screen::Main => {
                        match key.bare_key {
                            BareKey::Enter
                                if key.has_no_modifiers() && !self.web_server_started =>
                            {
                                start_web_server();
                            },
                            BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                                stop_web_server();
                            },
                            BareKey::Char(' ') if key.has_no_modifiers() => {
                                match self.web_sharing {
                                    WebSharing::Disabled => {
                                        // no-op
                                    },
                                    WebSharing::On => {
                                        stop_sharing_current_session();
                                    },
                                    WebSharing::Off => {
                                        share_current_session();
                                    },
                                }
                            },
                            BareKey::Char('t') if key.has_no_modifiers() => {
                                if self.token_list.is_empty() {
                                    match generate_web_login_token(None) {
                                        Ok(token) => {
                                            self.change_to_token_screen(token);
                                        },
                                        Err(e) => {
                                            self.web_server_error = Some(e);
                                        },
                                    }
                                } else {
                                    self.change_to_manage_tokens_screen();
                                }
                                should_render = true;
                            },
                            BareKey::Esc if key.has_no_modifiers() => {
                                close_self();
                            }
                            _ => {},
                        }
                    },
                    Screen::Token(..) => match key.bare_key {
                        BareKey::Esc if key.has_no_modifiers() => {
                            self.change_to_previous_screen();
                            should_render = true;
                        },
                        _ => {},
                    },
                    Screen::ManageTokens => {
                        match key.bare_key {
                            BareKey::Char(character)
                                if key.has_no_modifiers()
                                    && self.entering_new_token_name.is_some() =>
                            {
                                self.entering_new_token_name
                                    .as_mut()
                                    .map(|n| n.push(character));
                                should_render = true;
                            },
                            BareKey::Char(character)
                                if key.has_no_modifiers() && self.renaming_token.is_some() =>
                            {
                                self.renaming_token.as_mut().map(|n| n.push(character));
                                should_render = true;
                            },
                            BareKey::Backspace
                                if key.has_no_modifiers()
                                    && self.entering_new_token_name.is_some() =>
                            {
                                self.entering_new_token_name.as_mut().map(|n| n.pop());
                                should_render = true;
                            },
                            BareKey::Backspace
                                if key.has_no_modifiers() && self.renaming_token.is_some() =>
                            {
                                self.renaming_token.as_mut().map(|n| n.pop());
                                should_render = true;
                            },
                            BareKey::Esc if key.has_no_modifiers() => {
                                let entering_new_token_name = self.entering_new_token_name.take().is_some();
                                let renaming_token = self.renaming_token.take().is_some();
                                let editing_action_was_cancelled = entering_new_token_name || renaming_token;
                                if !editing_action_was_cancelled {
                                    self.change_to_main_screen();
                                }
                                should_render = true;
                            },
                            BareKey::Down if key.has_no_modifiers() => {
                                if let Some(selected_list_index) = self.selected_list_index.as_mut()
                                {
                                    if *selected_list_index
                                        < self.token_list.len().saturating_sub(1)
                                    {
                                        *selected_list_index += 1;
                                    } else {
                                        *selected_list_index = 0;
                                    }
                                    should_render = true;
                                }
                            },
                            BareKey::Up if key.has_no_modifiers() => {
                                if let Some(selected_list_index) = self.selected_list_index.as_mut()
                                {
                                    if *selected_list_index == 0 {
                                        *selected_list_index =
                                            self.token_list.len().saturating_sub(1)
                                    } else {
                                        *selected_list_index -= 1;
                                    }
                                    should_render = true;
                                }
                            },
                            BareKey::Char('n') if key.has_no_modifiers() => {
                                self.entering_new_token_name = Some(String::new());
                                should_render = true;
                            },
                            BareKey::Enter
                                if key.has_no_modifiers()
                                    && self.entering_new_token_name.is_some() =>
                            {
                                let new_token_name = self.entering_new_token_name.take().and_then(
                                    |new_token_name| {
                                        if new_token_name.is_empty() {
                                            None
                                        } else {
                                            Some(new_token_name)
                                        }
                                    },
                                );
                                match generate_web_login_token(new_token_name) {
                                    Ok(token) => {
                                        self.change_to_token_screen(token);
                                    },
                                    Err(e) => {
                                        self.web_server_error = Some(e);
                                    },
                                }
                                should_render = true;
                            },
                            BareKey::Char('r') if key.has_no_modifiers() => {
                                self.renaming_token = Some(String::new());
                                should_render = true;
                            },
                            BareKey::Enter
                                if key.has_no_modifiers() && self.renaming_token.is_some() =>
                            {
                                if let Some(currently_selected_token) = self
                                    .selected_list_index
                                    .and_then(|i| self.token_list.get(i))
                                {
                                    if let Some(new_token_name) = self.renaming_token.take() {
                                        match rename_web_token(
                                            &currently_selected_token.0,
                                            &new_token_name,
                                        ) {
                                            Ok(_) => {
                                                self.retrieve_token_list();
                                                if self.token_list.is_empty() {
                                                    self.selected_list_index = None;
                                                    self.change_to_main_screen();
                                                } else if self.selected_list_index
                                                    >= Some(self.token_list.len())
                                                {
                                                    self.selected_list_index = Some(
                                                        self.token_list.len().saturating_sub(1),
                                                    );
                                                }
                                            },
                                            Err(e) => {
                                                self.web_server_error = Some(e);
                                            },
                                        }
                                    }
                                }
                                should_render = true;
                            },
                            BareKey::Char('x') if key.has_no_modifiers() => {
                                if let Some(currently_selected_token) = self
                                    .selected_list_index
                                    .and_then(|i| self.token_list.get(i))
                                {
                                    match revoke_web_login_token(&currently_selected_token.0) {
                                        Ok(_) => {
                                            self.retrieve_token_list();
                                            if self.token_list.is_empty() {
                                                self.selected_list_index = None;
                                                self.change_to_main_screen();
                                            } else if self.selected_list_index
                                                >= Some(self.token_list.len())
                                            {
                                                self.selected_list_index =
                                                    Some(self.token_list.len().saturating_sub(1));
                                            }
                                            self.info = Some(
                                                "Revoked. Connected clients not affected."
                                                    .to_owned(),
                                            );
                                        },
                                        Err(e) => {
                                            self.web_server_error = Some(e);
                                        },
                                    }
                                }
                                should_render = true;
                            },
                            BareKey::Char('x') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                                match revoke_all_web_tokens() {
                                    Ok(_) => {
                                        self.retrieve_token_list();
                                        // TODO: move this outside to reduce duplication
                                        if self.token_list.is_empty() {
                                            self.selected_list_index = None;
                                            self.change_to_main_screen();
                                        } else if self.selected_list_index
                                            >= Some(self.token_list.len())
                                        {
                                            self.selected_list_index =
                                                Some(self.token_list.len().saturating_sub(1));
                                        }
                                        self.info = Some(
                                            "Revoked. Connected clients not affected.".to_owned(),
                                        );
                                    },
                                    Err(e) => {
                                        self.web_server_error = Some(e);
                                    },
                                }
                            },
                            _ => {},
                        }
                    },
                }
            },
            Event::Mouse(mouse_event) => {
                if !self.web_server_capability {
                    return false;
                }
                match mouse_event {
                    Mouse::LeftClick(line, column) => {
                        for (coordinates, url) in &self.clickable_urls {
                            if coordinates.contains(column, line as usize) {
                                if let Some(executable) = self.link_executable {
                                    run_command(&[&executable, &url], Default::default());
                                }
                                should_render = true;
                                break;
                            }
                        }
                    },
                    Mouse::Hover(line, column) => {
                        self.hover_coordinates = Some((column, line as usize));
                        should_render = true;
                    },
                    _ => {},
                }
            },
            Event::RunCommandResult(exit_code, _stdout, _stderr, context) => {
                if !self.web_server_capability {
                    return false;
                }
                let is_xdg_open = context.get("xdg_open_cli").is_some();
                let is_open = context.get("open_cli").is_some();
                if is_xdg_open {
                    if exit_code == Some(0) {
                        self.link_executable = Some("xdg-open");
                    }
                } else if is_open {
                    if exit_code == Some(0) {
                        self.link_executable = Some("open");
                    }
                }
            },
            Event::FailedToStartWebServer(error) => {
                self.web_server_error = Some(error);
                should_render = true;
            },
            _ => {},
        }
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        if !self.web_server_capability {
            self.render_no_web_server_capability(rows, cols);
            return;
        }
        // reset rendered state
        self.currently_hovering_over_link = false;
        self.clickable_urls.clear();
        match &self.current_screen {
            Screen::Main => {
                self.render_main_screen(rows, cols);
            },
            Screen::Token(generated_token) => {
                self.render_token_screen(rows, cols, generated_token);
            },
            Screen::ManageTokens => {
                self.render_manage_tokens_screen(rows, cols);
            },
        }
    }
}

impl App {
    pub fn query_link_executable(&self) {
        let mut xdg_open_context = BTreeMap::new();
        xdg_open_context.insert("xdg_open_cli".to_owned(), String::new());
        run_command(&["xdg-open", "--help"], xdg_open_context);
        let mut open_context = BTreeMap::new();
        open_context.insert("open_cli".to_owned(), String::new());
        run_command(&["open", "--help"], open_context);
    }
    pub fn change_own_title(&mut self) {
        if let Some(own_plugin_id) = self.own_plugin_id {
            rename_plugin_pane(own_plugin_id, "Share Session");
        }
    }
    pub fn render_no_web_server_capability(&self, rows: usize, cols: usize) {
        let text_full = "This version of Zellij was compiled without web sharing capabilities";
        let text_short = "No web server capabilities";
        let text = if cols >= text_full.chars().count() {
            text_full
        } else {
            text_short
        };
        let text_element = Text::new(text).color_range(3, ..);
        let text_x = cols.saturating_sub(text.chars().count()) / 2;
        let text_y = rows / 2;
        print_text_with_coordinates(text_element, text_x, text_y, None, None);
    }
    pub fn change_to_token_screen(&mut self, generated_token: String) {
        self.retrieve_token_list();
        set_self_mouse_selection_support(true);
        self.previous_screen = Some(self.current_screen.clone()); // so we can get back to it once
                                                                  // we've viewed the token
        self.current_screen = Screen::Token(generated_token);
    }
    pub fn change_to_manage_tokens_screen(&mut self) {
        self.retrieve_token_list();
        set_self_mouse_selection_support(false);
        self.selected_list_index = Some(0);
        self.previous_screen = None; // we don't want to go back to this screen
        self.current_screen = Screen::ManageTokens;
    }
    pub fn change_to_main_screen(&mut self) {
        self.retrieve_token_list();
        set_self_mouse_selection_support(false);
        self.previous_screen = None; // we don't want to go back to this screen
        self.current_screen = Screen::Main;
    }
    pub fn change_to_previous_screen(&mut self) {
        self.retrieve_token_list();
        match self.previous_screen.take() {
            Some(Screen::ManageTokens) => {
                self.change_to_manage_tokens_screen();
            },
            _ => {
                self.change_to_main_screen();
            }
        }

    }
    fn render_main_screen(&mut self, rows: usize, cols: usize) {
        let main_screen_state_changes = MainScreen::new(
            self.token_list.is_empty(),
            self.web_server_started,
            &self.web_server_error,
            &self.web_server_different_version_error,
            &self.web_server_base_url,
            self.web_server_ip,
            self.web_server_port,
            &self.session_name,
            self.web_sharing,
            self.hover_coordinates,
            &self.info,
            &self.link_executable,
        ).render(rows, cols);

        self.currently_hovering_over_link = main_screen_state_changes.currently_hovering_over_link;
        self.currently_hovering_over_unencrypted = main_screen_state_changes.currently_hovering_over_unencrypted;
        self.clickable_urls = main_screen_state_changes.clickable_urls;
    }
    fn render_token_screen(&self, rows: usize, cols: usize, generated_token: &str) {
        let token_screen = TokenScreen::new(
            generated_token.to_string(),
            self.web_server_error.clone(),
            rows,
            cols
        );
        token_screen.render();
    }

    fn render_manage_tokens_screen(&self, rows: usize, cols: usize) {
        TokenManagementScreen::new(
            &self.token_list,
            self.selected_list_index,
            &self.renaming_token,
            &self.entering_new_token_name,
            &self.web_server_error,
            &self.info,
            rows,
            cols,
        )
        .render();
    }
    fn retrieve_token_list(&mut self) {
        self.token_list = list_web_login_tokens().unwrap_or_else(|e| {
            self.web_server_error = Some(format!(
                "Failed to retrieve login tokens: {}",
                e.to_string()
            ));
            vec![]
        });
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

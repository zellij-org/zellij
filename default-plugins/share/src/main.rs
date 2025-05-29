mod ui_components;
use std::net::IpAddr;
use url::Url;
use zellij_tile::prelude::*;

use std::collections::{BTreeMap, HashMap};

use ui_components::{
    hovering_on_line, render_text_with_underline, CurrentSessionSection, Usage,
    WebServerStatusSection,
};

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
}

#[derive(Debug)]
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
                            BareKey::Enter if key.has_no_modifiers() && !self.web_server_started => {
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
                                        }
                                    }
                                } else {
                                    self.change_to_manage_tokens_screen();
                                }
                                should_render = true;
                            },
                            _ => {},
                        }
                    }
                    Screen::Token(..) => {
                        match key.bare_key {
                            BareKey::Esc if key.has_no_modifiers() => {
                                self.change_to_main_screen();
                                should_render = true;
                            },
                            _ => {}
                        }
                    }
                    Screen::ManageTokens => {
                        match key.bare_key {
                            BareKey::Char(character) if key.has_no_modifiers() && self.entering_new_token_name.is_some() => {
                                self.entering_new_token_name.as_mut().map(|n| n.push(character));
                                should_render = true;
                            }
                            BareKey::Char(character) if key.has_no_modifiers() && self.renaming_token.is_some() => {
                                self.renaming_token.as_mut().map(|n| n.push(character));
                                should_render = true;
                            }
                            BareKey::Backspace if key.has_no_modifiers() && self.entering_new_token_name.is_some() => {
                                self.entering_new_token_name.as_mut().map(|n| n.pop());
                                should_render = true;
                            }
                            BareKey::Backspace if key.has_no_modifiers() && self.renaming_token.is_some() => {
                                self.renaming_token.as_mut().map(|n| n.pop());
                                should_render = true;
                            }
                            BareKey::Esc if key.has_no_modifiers() => {
                                self.entering_new_token_name = None;
                                self.change_to_main_screen();
                                should_render = true;
                            },
                            BareKey::Down if key.has_no_modifiers() => {
                                if let Some(selected_list_index) = self.selected_list_index.as_mut() {
                                    if *selected_list_index < self.token_list.len().saturating_sub(1) {
                                        *selected_list_index += 1;
                                    } else {
                                        *selected_list_index = 0;
                                    }
                                    should_render = true;
                                }
                            }
                            BareKey::Up if key.has_no_modifiers() => {
                                if let Some(selected_list_index) = self.selected_list_index.as_mut() {
                                    if *selected_list_index == 0 {
                                        *selected_list_index = self.token_list.len().saturating_sub(1)
                                    } else {
                                        *selected_list_index -= 1;
                                    }
                                    should_render = true;
                                }
                            }
                            BareKey::Char('n') if key.has_no_modifiers() => {
                                self.entering_new_token_name = Some(String::new());
                                should_render = true;
                            }
                            BareKey::Enter if key.has_no_modifiers() && self.entering_new_token_name.is_some() => {
                                let new_token_name = self.entering_new_token_name.take().and_then(|new_token_name| if new_token_name.is_empty() { None } else { Some(new_token_name)});
                                match generate_web_login_token(new_token_name) {
                                    Ok(token) => {
                                        self.change_to_token_screen(token);
                                    },
                                    Err(e) => {
                                        self.web_server_error = Some(e);
                                    }
                                }
                                should_render = true;
                            },
                            BareKey::Char('r') if key.has_no_modifiers() => {
                                self.renaming_token = Some(String::new());
                                should_render = true;
                            }
                            BareKey::Enter if key.has_no_modifiers() && self.renaming_token.is_some() => {
                                if let Some(currently_selected_token) = self.selected_list_index.and_then(|i| self.token_list.get(i)) {
                                    if let Some(new_token_name) = self.renaming_token.take() {
                                        match rename_web_token(&currently_selected_token.0, &new_token_name) {
                                            Ok(_) => {
                                                self.retrieve_token_list();
                                                if self.token_list.is_empty() {
                                                    self.selected_list_index = None;
                                                    self.change_to_main_screen();
                                                } else if self.selected_list_index >= Some(self.token_list.len()) {
                                                    self.selected_list_index = Some(self.token_list.len().saturating_sub(1));
                                                }
                                            },
                                            Err(e) => {
                                                self.web_server_error = Some(e);
                                            }
                                        }
                                    }
                                }
                                should_render = true;
                            },
                            BareKey::Char('x') if key.has_no_modifiers() => {
                                if let Some(currently_selected_token) = self.selected_list_index.and_then(|i| self.token_list.get(i)) {
                                    match revoke_web_login_token(&currently_selected_token.0) {
                                        Ok(_) => {
                                            self.retrieve_token_list();
                                            if self.token_list.is_empty() {
                                                self.selected_list_index = None;
                                                self.change_to_main_screen();
                                            } else if self.selected_list_index >= Some(self.token_list.len()) {
                                                self.selected_list_index = Some(self.token_list.len().saturating_sub(1));
                                            }
                                            self.info = Some("Revoked. Connected clients not affected.".to_owned());
                                        },
                                        Err(e) => {
                                            self.web_server_error = Some(e);
                                        }
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
                                        } else if self.selected_list_index >= Some(self.token_list.len()) {
                                            self.selected_list_index = Some(self.token_list.len().saturating_sub(1));
                                        }
                                        self.info = Some("Revoked. Connected clients not affected.".to_owned());
                                    },
                                    Err(e) => {
                                        self.web_server_error = Some(e);
                                    }
                                }

                            }
                            _ => {}
                        }
                    }
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
            }
            Screen::ManageTokens => {
                self.render_manage_tokens_screen(rows, cols);
            }
        }
    }
}

impl App {
    pub fn render_link_help(&self, x: usize, y: usize) {
        let help_text = if self.link_executable.is_some() {
            let help_text = format!("Help: Click or Shift-Click to open in browser");
            Text::new(help_text)
                .color_range(3, 6..=10)
                .color_range(3, 15..=25)
        } else {
            let help_text = format!("Help: Shift-Click to open in browser");
            Text::new(help_text).color_range(3, 6..=16)
        };
        print_text_with_coordinates(help_text, x, y, None, None);
    }
    pub fn render_unencrypted_warning(&mut self, x: usize, y: usize) {
        let warning_text =
            format!("[*] Connection unencrypted. Consider using an SSL certificate.");
        let warning_text = Text::new(warning_text).color_range(1, ..3);
        let more_info_text = "More info: ";
        let url_text = "https://zellij.dev/documentation/web-server-ssl";
        let more_info_line = Text::new(format!("{}{}", more_info_text, url_text));
        let url_x = x + more_info_text.chars().count();
        let url_y = y + 1;
        let url_width = url_text.chars().count();
        self.clickable_urls.insert(
            CoordinatesInLine::new(url_x, url_y, url_width),
            url_text.to_owned(),
        );
        print_text_with_coordinates(warning_text, x, y, None, None);
        print_text_with_coordinates(more_info_line, x, y + 1, None, None);
        if hovering_on_line(url_x, url_y, url_width, self.hover_coordinates) {
            self.currently_hovering_over_link = true;
            render_text_with_underline(url_x, url_y, url_text);
        }
    }
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
    pub fn connection_is_unencrypted(&self) -> bool {
        Url::parse(&self.web_server_base_url)
            .ok()
            .map(|b| b.scheme() == "http")
            .unwrap_or(false)
    }
    pub fn change_to_token_screen(&mut self, generated_token: String) {
        self.retrieve_token_list();
        set_self_mouse_selection_support(true);
        self.current_screen = Screen::Token(generated_token);
    }
    pub fn change_to_manage_tokens_screen(&mut self) {
        self.retrieve_token_list();
        set_self_mouse_selection_support(false);
        self.selected_list_index = Some(0);
        self.current_screen = Screen::ManageTokens;
    }
    pub fn change_to_main_screen(&mut self) {
        self.retrieve_token_list();
        set_self_mouse_selection_support(false);
        self.current_screen = Screen::Main;
    }
    fn render_main_screen(&mut self, rows: usize, cols: usize) {
        let usage = Usage::new(!self.token_list.is_empty());
        let connection_is_unencrypted = self.connection_is_unencrypted();
        let mut web_server_status_section = WebServerStatusSection::new(
            self.web_server_started,
            self.web_server_error.clone(),
            self.web_server_different_version_error.clone(),
            self.web_server_base_url.clone(),
            connection_is_unencrypted,
        );
        let mut current_session_section = CurrentSessionSection::new(
            self.web_server_started,
            self.web_server_ip,
            self.web_server_port,
            self.session_name.clone(),
            self.web_sharing,
            connection_is_unencrypted,
        );

        let mut max_item_width = 0;
        let title_text = "Share Session Locally in the Browser";
        max_item_width = std::cmp::max(max_item_width, title_text.chars().count());

        let (web_server_items_width, web_server_items_height) =
            web_server_status_section.web_server_status_width_and_height();
        max_item_width = std::cmp::max(max_item_width, web_server_items_width);
        let (current_session_items_width, current_session_items_height) =
            current_session_section.current_session_status_width_and_height();
        max_item_width = std::cmp::max(max_item_width, current_session_items_width);
        let (usage_width, usage_height) = usage.usage_width_and_height(cols);
        max_item_width = std::cmp::max(max_item_width, usage_width);
        let mut line_count =
            2 + web_server_items_height + 1 + current_session_items_height + 1 + usage_height;

        if connection_is_unencrypted {
            line_count += 3; // space for the warning
        }

        let base_x = cols.saturating_sub(max_item_width) / 2;
        let base_y = rows.saturating_sub(line_count) / 2; // the + 2 are the line spaces

        let mut current_y = base_y;
        let title = Text::new(title_text).color_range(2, ..);
        print_text_with_coordinates(
            title,
            cols.saturating_sub(title_text.chars().count()) / 2,
            current_y,
            None,
            None,
        );
        current_y += 2;
        web_server_status_section.render_web_server_status(
            base_x,
            current_y,
            self.hover_coordinates,
        );
        self.currently_hovering_over_link = web_server_status_section.currently_hovering_over_link;
        self.currently_hovering_over_unencrypted = self.currently_hovering_over_unencrypted
            || web_server_status_section.currently_hovering_over_unencrypted;
        for (coordinates, url) in web_server_status_section.clickable_urls {
            self.clickable_urls.insert(coordinates, url);
        }
        current_y += web_server_items_height + 1;

        current_session_section.render_current_session_status(
            base_x,
            current_y,
            self.hover_coordinates,
        );
        self.currently_hovering_over_link = self.currently_hovering_over_link
            || current_session_section.currently_hovering_over_link;
        for (coordinates, url) in current_session_section.clickable_urls {
            self.clickable_urls.insert(coordinates, url);
        }

        current_y += web_server_items_height + 1;
        usage.render_usage(base_x, current_y, cols);
        current_y += usage_height + 1;

        if connection_is_unencrypted && self.web_server_started {
            self.render_unencrypted_warning(base_x, current_y);
            current_y += 3;
        }

        if self.currently_hovering_over_link {
            self.render_link_help(base_x, current_y);
            current_y += 3;
        }

        if let Some(info) = &self.info {
            print_text_with_coordinates(Text::new(info).color_range(1, ..), base_x, current_y, None, None);
        }
    }
    fn render_token_screen(&self, rows: usize, cols: usize, generated_token: &str) {
        let mut width = 0;
        let generated_token_text_long = format!("New log-in token: {}", generated_token);
        let generated_token_text_short = format!("Token: {}", generated_token);
        let (generated_token, generated_token_text) = if cols >= generated_token_text_long.chars().count() {
            let generated_token = Text::new(&generated_token_text_long).color_range(2, ..=16);
            (generated_token, generated_token_text_long)
        } else {
            let generated_token = Text::new(&generated_token_text_short).color_range(2, ..=5);
            (generated_token, generated_token_text_short)
        };
        width = std::cmp::max(width, generated_token_text.chars().count());

        let explanation_text_1_long = "Use this token to log-in from the browser.";
        let explanation_text_1_short = "Use to log-in from the browser.";
        let explanation_text_1 = if cols >= explanation_text_1_long.chars().count() {
            explanation_text_1_long
        } else {
            explanation_text_1_short
        };
        width = std::cmp::max(width, explanation_text_1.chars().count());
        let explanation_text_1 = Text::new(explanation_text_1).color_range(0, ..);
        
        let explanation_text_2_long = "Copy this token, because it will not be saved and can't be retrieved.";
        let explanation_text_2_short = "It will not be saved and can't be retrieved.";
        let explanation_text_2 = if cols >= explanation_text_2_long.chars().count() {
            explanation_text_2_long
        } else {
            explanation_text_2_short
        };
        width = std::cmp::max(width, explanation_text_2.chars().count());
        let explanation_text_2 = Text::new(explanation_text_2);

        let explanation_text_3_long = "If lost, it can always be revoked and a new one generated.";
        let explanation_text_3_short = "It can always be revoked and a regenerated.";
        let explanation_text_3 = if cols >= explanation_text_3_long.chars().count() {
            explanation_text_3_long
        } else {
            explanation_text_3_short
        };
        width = std::cmp::max(width, explanation_text_3.chars().count());
        let explanation_text_3 = Text::new(explanation_text_3);

        let esc_go_back = "<Esc> - go back";
        width = std::cmp::max(width, esc_go_back.chars().count());
        let esc_go_back = Text::new(esc_go_back).color_range(3, ..=4);

        let base_x = cols.saturating_sub(width) / 2;
        let base_y = rows.saturating_sub(7) / 2;
        print_text_with_coordinates(generated_token, base_x, base_y, None, None);
        print_text_with_coordinates(explanation_text_1, base_x, base_y + 2, None, None);
        print_text_with_coordinates(explanation_text_2, base_x, base_y + 4, None, None);
        print_text_with_coordinates(explanation_text_3, base_x, base_y + 5, None, None);
        print_text_with_coordinates(esc_go_back, base_x, base_y + 7, None, None);

        if let Some(error) = &self.web_server_error {
            print_text_with_coordinates(Text::new(error).color_range(3, ..), base_x, base_y + 8, None, None);
        }
    }
    fn render_manage_tokens_screen(&self, rows: usize, cols: usize) {
        // should include:
        // 1. a list of tokens (name, created_at)
        // 2. ability to revoke tokens
        // 3. ability to create new tokens
        // 4. ability to revoke all tokens
        // 5. ability to rename tokens
        //
        //                List of Authorized Login Tokens
        //
        // > token_1, issued on Jan 5th 2025 <x> - revoke, <n> - rename
        // > token_2, issued on Feb 17th 2025
        // > <n> - create new token
        //
        // Help: <Ctrl x> - revoke all tokens

        let mut width = 0;
        let title_text = "List of Login Tokens";
        let title = Text::new(title_text).color_range(2, ..);
        width = std::cmp::max(width, title_text.chars().count());
        let mut items = vec![];
        for (i, (token, created_at)) in self.token_list.iter().enumerate() {
            // let hard_coded_date = "Jan 5th 2025";
            let token_end_index = token.chars().count();
            let r_key_start_index = token_end_index + 10 + created_at.chars().count() + 3;
            let r_key_end_index = r_key_start_index + 2;
            let n_key_start_index = r_key_end_index + 10;
            let n_key_end_index = n_key_start_index + 2;
            let is_selected = Some(i) == self.selected_list_index;
            let (item_text, mut item) = if is_selected {
                if let Some(new_token_name) = &self.renaming_token {
                    let token_end_index = new_token_name.chars().count();
                    let item_text = format!("{}_ issued on {}", new_token_name, created_at);
                    let item = NestedListItem::new(&item_text)
                        .color_range(0, ..token_end_index + 1);
                    (item_text, item)
                } else {
                    let item_text = format!("{} issued on {} (<x> revoke, <r> rename)", token, created_at);
                    let item = NestedListItem::new(&item_text)
                        .color_range(0, ..token_end_index)
                        .color_range(3, r_key_start_index..=r_key_end_index)
                        .color_range(3, n_key_start_index..=n_key_end_index);
                    (item_text, item)
                }
            } else {
                let item_text = format!("{} issued on {}", token, created_at);
                let item = NestedListItem::new(&item_text)
                    .color_range(0, ..token_end_index);
                (item_text, item)
            };
            width = std::cmp::max(width, item_text.chars().count());
//             let mut item = NestedListItem::new(item_text)
//                 .color_range(0, ..token_end_index)
//                 .color_range(3, r_key_start_index..=r_key_end_index)
//                 .color_range(3, n_key_start_index..=n_key_end_index);
            if is_selected {
                item = item.selected();
            }
            items.push(item);
        }
        let (new_token_line_text, new_token_line) = if let Some(new_token_name) = &self.entering_new_token_name {
            let new_token_line_text = format!("{}_", new_token_name);
            let new_token_line = NestedListItem::new(&new_token_line_text).color_range(3, ..);
            (new_token_line_text, new_token_line)
        } else {
            let new_token_line_text = format!("<n> - create new token");
            let new_token_line = NestedListItem::new(&new_token_line_text).color_range(3, 0..=2);
            (new_token_line_text, new_token_line)
        };
        width = std::cmp::max(width, new_token_line_text.chars().count());
        items.push(new_token_line);
        let item_count = items.len();

        let (help_line_text, help_line) = if self.entering_new_token_name.is_some() {
            let help_line_text = "Help: Enter optional name for new token, <Enter> to submit";
            let help_line = Text::new(&help_line_text)
                .color_range(3, 41..=47);
            (help_line_text, help_line)
        } else if self.renaming_token.is_some() {
            let help_line_text = "Help: Enter new name for this token, <Enter> to submit";
            let help_line = Text::new(&help_line_text)
                .color_range(3, 39..=45);
            (help_line_text, help_line)
        } else {
            let help_line_text = "Help: <Ctrl x> - revoke all tokens, <Esc> - go back";
            let help_line = Text::new(&help_line_text)
                .color_range(3, 6..=13)
                .color_range(3, 36..=40);
            (help_line_text, help_line)
        };
        width = std::cmp::max(width, help_line_text.chars().count());

        let base_x = cols.saturating_sub(width) / 2;
        let base_y = rows.saturating_sub(4 + item_count) / 2;

        print_text_with_coordinates(title, cols.saturating_sub(title_text.chars().count()) / 2, base_y, None, None);
        print_nested_list_with_coordinates(items, base_x, base_y + 2, None, None);
        print_text_with_coordinates(help_line, base_x, base_y + item_count + 3, None, None);

        if let Some(error) = &self.web_server_error {
            print_text_with_coordinates(Text::new(error).color_range(3, ..), base_x, base_y + item_count + 5, None, None);
        } else  if let Some(info) = &self.info {
            print_text_with_coordinates(Text::new(info).color_range(1, ..), base_x, base_y + item_count + 5, None, None);
        }

    }
    fn retrieve_token_list(&mut self) {
        self.token_list = list_web_login_tokens().unwrap_or_else(|e| {
            self.web_server_error = Some(format!("Failed to retrieve login tokens: {}", e.to_string()));
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

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
}

#[derive(Debug)]
enum Screen {
    Main,
    Token,
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
        self.query_link_executable();
        self.change_own_title();
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::Timer(_) => {
                query_web_server_status();
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
                match self.current_screen {
                    Screen::Main => {
                        if self.web_server_error.take().is_some() {
                            // clear the error with any key
                            return true;
                        }
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
                                self.change_to_token_screen();
                                should_render = true;
                            },
                            _ => {},
                        }
                    }
                    Screen::Token => {
                        match key.bare_key {
                            BareKey::Esc if key.has_no_modifiers() => {
                                self.change_to_main_screen();
                                should_render = true;
                            },
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
//                   Share Session Locally in the Browser
// 
// Web server: RUNNING (<Ctrl c> - Stop)
// URL: https://127.0.0.1:8082
// <t> - generate authentication token / manage authentication tokens
// 
// Current session: NOT SHARING
// Press <SPACE> to share
// 
// How it works:
// - Visit base URL to start a new session
// - Follow base URL with a session name to attach to or create it
// - By default sessions not started from the web must be explicitly shared
//
//
// *******
//
// Generated token: <TOKEN>
//
// Press <c> to copy to clipboard (requires a supporting terminal)
//
// Or generate the token on the command line with:
// zellij web --generate-token
//
// Only token hashes are stored, tokens cannot be retrieved - only revoked.
//
//
// *******
//
//                    Active authentication tokens
//
//
        if !self.web_server_capability {
            self.render_no_web_server_capability(rows, cols);
            return;
        }
        match self.current_screen {
            Screen::Main => {
                self.render_main_screen(rows, cols);
            },
            Screen::Token => {
                self.render_token_screen(rows, cols);

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
    pub fn change_to_token_screen(&mut self) {
        self.current_screen = Screen::Token;
    }
    pub fn change_to_main_screen(&mut self) {
        // TODO: also delete token from state
        self.current_screen = Screen::Main;
    }
    fn render_main_screen(&mut self, rows: usize, cols: usize) {
        // reset rendered state
        self.currently_hovering_over_link = false;
        self.clickable_urls.clear();
        let usage = Usage::new();
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
        }
    }
    fn render_token_screen(&mut self, rows: usize, cols: usize) {

// Generated token: <TOKEN>
//
// Use this token to log-in from the browser.
// Tokens are not saved, so can't be retrieved - only revoked.
//
// <Esc> - go back
        let mut width = 0;
        let token_placeholder = "81eeb7cd-ca74-464d-b025-d9e3a57193bf";
        let generated_token_text = format!("New log-in token: {}", token_placeholder);
        width = std::cmp::max(width, generated_token_text.chars().count());
        let generated_token = Text::new(generated_token_text).color_range(2, ..=15);
        let explanation_text_1 = "Use this token to log-in from the browser.";
        width = std::cmp::max(width, explanation_text_1.chars().count());
        let explanation_text_2 = "Copy this token, because it will not be saved and can't be retrieved.";
        width = std::cmp::max(width, explanation_text_2.chars().count());
        let explanation_text_3 = "If lost, it can always be revoked and a new one generated.";
        width = std::cmp::max(width, explanation_text_3.chars().count());
        let esc_go_back = "<Esc> - go back";
        width = std::cmp::max(width, esc_go_back.chars().count());
        let explanation_text_1 = Text::new(explanation_text_1).color_range(0, ..);
        let explanation_text_2 = Text::new(explanation_text_2);
        let explanation_text_3 = Text::new(explanation_text_3);
        let esc_go_back = Text::new(esc_go_back).color_range(3, ..=4);
        let base_x = cols.saturating_sub(width) / 2;
        let base_y = rows.saturating_sub(7) / 2;
        print_text_with_coordinates(generated_token, base_x, base_y, None, None);
        print_text_with_coordinates(explanation_text_1, base_x, base_y + 2, None, None);
        print_text_with_coordinates(explanation_text_2, base_x, base_y + 4, None, None);
        print_text_with_coordinates(explanation_text_3, base_x, base_y + 5, None, None);
        print_text_with_coordinates(esc_go_back, base_x, base_y + 7, None, None);
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

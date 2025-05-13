use std::net::IpAddr;
use zellij_tile::prelude::*;

use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Default)]
struct App {
    web_server_started: bool,
    web_sharing: WebSharing,
    web_clients_allowed: bool,
    session_name: Option<String>,
    web_server_error: Option<String>,
    web_server_ip: Option<IpAddr>,
    web_server_port: Option<u16>,
    hover_coordinates: Option<(usize, usize)>, // x, y
    clickable_urls: HashMap<CoordinatesInLine, String>,
    link_executable: Option<&'static str>,
    currently_hovering_over_link: bool,
    own_plugin_id: Option<u32>,
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
        ]);
        self.own_plugin_id = Some(get_plugin_ids().plugin_id);
        self.query_link_executable();
        self.change_own_title();
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
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
            },
            Event::WebServerStatus(web_server_status)=> {
                match web_server_status {
                    WebServerStatus::Online => {
                        self.web_server_started = true;
                        self.web_server_error = None;
                    },
                    WebServerStatus::Offline => {
                        self.web_server_started = false;
                    },
                    WebServerStatus::DifferentVersion(different_version) => {
                        self.web_server_started = false;
                        self.web_server_error = Some(format!(
                            "Server online with an incompatible Zellij version: {}",
                            different_version
                        ));
                    }
                }
                should_render = true;
            },
            Event::Key(key) => {
                match key.bare_key {
                    BareKey::Enter if key.has_no_modifiers() => {
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
                    _ => {},
                }
            },
            Event::Mouse(mouse_event) => {
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
                    }
                    Mouse::Hover(line, column) => {
                        self.hover_coordinates = Some((column, line as usize));
                        should_render = true;
                    },
                    _ => {},
                }
            }
            Event::RunCommandResult(exit_code, _stdout, _stderr, context) => {
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
            _ => {},
        }
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        // reset rendered state
        self.currently_hovering_over_link = false;
        self.clickable_urls.clear();

        let mut max_item_width = 0;
        let title_text = "Share Session Locally in the Browser";
        max_item_width = std::cmp::max(max_item_width, title_text.chars().count());

        let (web_server_items_width, web_server_items_height) = self.web_server_status_width_and_height();
        max_item_width = std::cmp::max(max_item_width, web_server_items_width);
        let (current_session_items_width, current_session_items_height) = self.current_session_status_width_and_height();
        max_item_width = std::cmp::max(max_item_width, current_session_items_width);
        let (usage_width, usage_height) = self.usage_width_and_height(cols);
        max_item_width = std::cmp::max(max_item_width, usage_width);
        let line_count = 2 + web_server_items_height + 1 + current_session_items_height + 1 + usage_height;

        let base_x = cols.saturating_sub(max_item_width) / 2;
        let base_y = rows.saturating_sub(line_count) / 2; // the + 2 are the line spaces

        let mut current_y = base_y;
        let title = Text::new(title_text).color_range(2, ..);
        print_text_with_coordinates(title, cols.saturating_sub(title_text.chars().count()) / 2, current_y, None, None);
        current_y += 2;
        self.render_web_server_status(base_x, current_y);
        current_y += web_server_items_height + 1;
        self.render_current_session_status(base_x, current_y);
        current_y += web_server_items_height + 1;
        self.render_usage(base_x, current_y, cols);
        current_y += usage_height + 1;

        if self.currently_hovering_over_link {
            self.render_link_help(base_x, current_y);
        }
    }
}

// render methods, return UI components and the width of the widest one
impl App {
    pub fn web_server_status_width_and_height(&self) -> (usize, usize) {
        let mut max_len = 0;
        if self.web_server_started {
            let title = "Web server: ";
            let value = "RUNNING ";
            let shortcut = "(<Ctrl c> - Stop)";
            max_len = std::cmp::max(
                max_len,
                title.chars().count() + value.chars().count() + shortcut.chars().count(),
            );
        } else {
            let title = "Web server status: ";
            let value = "NOT RUNNING";
            max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
        };
        if self.web_server_started {
            let title = "URL: ";
            let web_server_ip = self
                .web_server_ip
                .map(|i| format!("{}", i))
                .unwrap_or("UNDEFINED".to_owned());
            let web_server_port = self
                .web_server_port
                .map(|p| format!("{}", p))
                .unwrap_or("UNDEFINED".to_owned());
            let value = format!("http://{}:{}/", web_server_ip, web_server_port);
            max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
        } else {
            let text = "Press <ENTER> to start";
            max_len = std::cmp::max(max_len, text.chars().count());
        };
        let width = max_len;
        let height = 2;
        (width, height)
    }
    pub fn render_web_server_status(&mut self, x: usize, y: usize) {
        let web_server_status_line = if self.web_server_started {
            let title = "Web server: ";
            let value = "RUNNING ";
            let shortcut = "(<Ctrl c> - Stop)";
            let value_start_position = title.chars().count();
            let value_end_position = value_start_position + value.chars().count();
            let ctrl_c_start_position = value_end_position + 1;
            let ctrl_c_end_position = ctrl_c_start_position + 8;
            Text::new(format!("{}{}{}", title, value, shortcut))
                .color_range(0, ..title.chars().count())
                .color_range(3, value_start_position..value_end_position)
                .color_range(3, ctrl_c_start_position..ctrl_c_end_position)
        } else {
            let title = "Web server status: ";
            let value = "NOT RUNNING";
            Text::new(format!("{}{}", title, value))
                .color_range(0, ..title.chars().count())
                .color_range(3, title.chars().count()..)
        };
        print_text_with_coordinates(web_server_status_line, x, y, None, None);
        if self.web_server_started {
            let title = "URL: ";
            let web_server_ip = self
                .web_server_ip
                .map(|i| format!("{}", i))
                .unwrap_or("UNDEFINED".to_owned());
            let web_server_port = self
                .web_server_port
                .map(|p| format!("{}", p))
                .unwrap_or("UNDEFINED".to_owned());
            let server_url = format!("http://{}:{}/", web_server_ip, web_server_port);
            let url_x = x + title.chars().count();
            let url_width = server_url.chars().count();
            let url_y = y + 1;
            self.clickable_urls.insert(CoordinatesInLine::new(url_x, url_y, url_width), server_url.clone());
            if self.hovering_on_line(url_x, url_y, url_width) {
                self.currently_hovering_over_link = true;
                let title = Text::new(title).color_range(0, ..title.chars().count());
                print_text_with_coordinates(title, x, y + 1, None, None);
                self.render_text_with_underline(url_x, url_y, server_url);
            } else {
                let info_line = Text::new(format!("{}{}", title, server_url)).color_range(0, ..title.chars().count());
                print_text_with_coordinates(info_line, x, y + 1, None, None);
            }
        } else {
            let text = "Press <ENTER> to start";
            let info_line = Text::new(text).color_range(3, 6..=12);
            print_text_with_coordinates(info_line, x, y + 1, None, None);
        };
    }
    pub fn current_session_status_width_and_height(&self) -> (usize, usize) {
        let mut max_len = 0;
        match self.web_sharing {
            WebSharing::On => {
                let title = "Current session: ";
                let value = "SHARING (<SPACE> - Stop Sharing)";
                max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
            },
            WebSharing::Disabled => {
                let title = "Current session: ";
                let value = "SHARING IS DISABLED";
                max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
            },
            WebSharing::Off => {
                let title = "Current session: ";
                let value = "NOT SHARING";
                max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
            },
        };
        if self.web_sharing.web_clients_allowed() && self.web_server_started {
            let title = "Session URL: ";
            let web_server_ip = self
                .web_server_ip
                .map(|i| format!("{}", i))
                .unwrap_or("UNDEFINED".to_owned());
            let web_server_port = self
                .web_server_port
                .map(|p| format!("{}", p))
                .unwrap_or("UNDEFINED".to_owned());
            let value = format!(
                "http://{}:{}/{}",
                web_server_ip,
                web_server_port,
                self.session_name.clone().unwrap_or_else(|| "".to_owned())
            );
            max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
        } else if self.web_sharing.web_clients_allowed() {
            let text = format!("...but web server is offline");
            max_len = std::cmp::max(max_len, text.chars().count());
        } else {
            let text = "Press <SPACE> to share";
            max_len = std::cmp::max(max_len, text.chars().count());
        };
        let width = max_len;
        let height = 2;
        (width, height)
    }
    pub fn render_current_session_status(&mut self, x: usize, y: usize) {
        let status_line = match self.web_sharing {
            WebSharing::On => {
                let title = "Current session: ";
                let value = "SHARING (<SPACE> - Stop Sharing)";
                let sharing_start_pos = title.chars().count();
                let sharing_end_pos = sharing_start_pos + 7;
                let space_start_position = sharing_end_pos + 2;
                let space_end_position = space_start_position + 6;
                Text::new(format!("{}{}", title, value))
                    .color_range(0, ..sharing_start_pos)
                    .color_range(3, sharing_start_pos..sharing_end_pos)
                    .color_range(3, space_start_position..=space_end_position)
            },
            WebSharing::Disabled => {
                let title = "Current session: ";
                let value = "SHARING IS DISABLED";
                Text::new(format!("{}{}", title, value))
                    .color_range(0, ..title.chars().count())
                    .color_range(3, title.chars().count()..)
            },
            WebSharing::Off => {
                let title = "Current session: ";
                let value = "NOT SHARING";
                Text::new(format!("{}{}", title, value))
                    .color_range(0, ..title.chars().count())
                    .color_range(3, title.chars().count()..)
            },
        };
        print_text_with_coordinates(status_line, x, y, None, None);
        if self.web_sharing.web_clients_allowed() && self.web_server_started {
            let title = "Session URL: ";
            let web_server_ip = self
                .web_server_ip
                .map(|i| format!("{}", i))
                .unwrap_or("UNDEFINED".to_owned());
            let web_server_port = self
                .web_server_port
                .map(|p| format!("{}", p))
                .unwrap_or("UNDEFINED".to_owned());
            let session_url = format!(
                "http://{}:{}/{}",
                web_server_ip,
                web_server_port,
                self.session_name.clone().unwrap_or_else(|| "".to_owned())
            );
            let url_x = x + title.chars().count();
            let url_width = session_url.chars().count();
            let url_y = y + 1;

            self.clickable_urls.insert(CoordinatesInLine::new(url_x, url_y, url_width), session_url.clone());

            if self.hovering_on_line(url_x, url_y, url_width) {
                self.currently_hovering_over_link = true;
                let title = Text::new(title).color_range(0, ..title.chars().count());
                print_text_with_coordinates(title, x, y + 1, None, None);
                self.render_text_with_underline(url_x, url_y, session_url);
            } else {
                let info_line = Text::new(format!("{}{}", title, session_url)).color_range(0, ..title.chars().count());
                print_text_with_coordinates(info_line, x, y + 1, None, None);
            }
        } else if self.web_sharing.web_clients_allowed() {
            let text = format!("...but web server is offline");
            let info_line = Text::new(text);
            print_text_with_coordinates(info_line, x, y + 1, None, None);
        } else {
            let text = "Press <SPACE> to share";
            let info_line = Text::new(text).color_range(3, 6..=12);
            print_text_with_coordinates(info_line, x, y + 1, None, None);
        }
    }
    pub fn usage_width_and_height(&self, max_width: usize) -> (usize, usize) {
        let mut max_len = 0;
        let usage_title = "How it works:";
        max_len = std::cmp::max(max_len, usage_title.chars().count());

        let bulletin_1_full = "- Visit base URL to start a new session";
        let bulletin_1_short = "- Base URL: new session";
        let bulletin_2_full = "- Follow base URL with a session name to attach to or create it";
        let bulletin_2_short = "- Base URL + session name: attach or create";
        let bulletin_3_full = "- Sessions must be explicitly shared unless specified otherwise in the config";
        let bulletin_3_short = "- Sessions must be explicitly shared";

        let bulletin_1 = if bulletin_1_full.chars().count() <= max_width {
            bulletin_1_full
        } else {
            bulletin_1_short
        };
        max_len = std::cmp::max(max_len, bulletin_1.chars().count());

        let bulletin_2 = if bulletin_2_full.chars().count() <= max_width {
            bulletin_2_full
        } else {
            bulletin_2_short
        };
        max_len = std::cmp::max(max_len, bulletin_2.chars().count());

        let bulletin_3 = if bulletin_3_full.chars().count() <= max_width {
            bulletin_3_full
        } else {
            bulletin_3_short
        };
        max_len = std::cmp::max(max_len, bulletin_3.chars().count());

        let width = max_len;
        let height = 4;
        (width, height)
    }
    pub fn render_usage(&self, x: usize, y: usize, max_width: usize) {
        let usage_title = "How it works:";
        let bulletin_1_full = "- Visit base URL to start a new session";
        let bulletin_1_short = "- Base URL: new session";
        let bulletin_2_full = "- Follow base URL with a session name to attach to or create it";
        let bulletin_2_short = "- Base URL + session name: attach or create";
        let bulletin_3_full = "- Sessions must be explicitly shared unless specified otherwise in the config";
        let bulletin_3_short = "- Sessions must be explicitly shared";

        let bulletin_1 = if bulletin_1_full.chars().count() <= max_width {
            bulletin_1_full
        } else {
            bulletin_1_short
        };
        let bulletin_2 = if bulletin_2_full.chars().count() <= max_width {
            bulletin_2_full
        } else {
            bulletin_2_short
        };
        let bulletin_3 = if bulletin_3_full.chars().count() <= max_width {
            bulletin_3_full
        } else {
            bulletin_3_short
        };

        let usage_title = Text::new(usage_title).color_range(2, ..);
        let bulletin_1 = Text::new(bulletin_1);
        let bulletin_2 = Text::new(bulletin_2);
        let bulletin_3 = Text::new(bulletin_3);

        print_text_with_coordinates(usage_title, x, y, None, None);
        print_text_with_coordinates(bulletin_1, x, y + 1, None, None);
        print_text_with_coordinates(bulletin_2, x, y + 2, None, None);
        print_text_with_coordinates(bulletin_3, x, y + 3, None, None);

    }
    pub fn render_link_help(&self, x: usize, y: usize) {
        let help_text = if self.link_executable.is_some() {
            let help_text = format!("Help: Click or Shift-Click to open in browser");
            Text::new(help_text)
                .color_range(3, 6..=10)
                .color_range(3, 15..=25)
        } else {
            let help_text = format!("Help: Shift-Click to open in browser");
            Text::new(help_text)
                .color_range(3, 6..=16)
        };
        print_text_with_coordinates(help_text, x, y, None, None);
    }
    fn render_text_with_underline(&self, url_x: usize, url_y: usize, url_text: String) {
        print!(
            "\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4m{}",
            url_y + 1,
            url_x + 1,
            url_text,
        );
    }
}

// utility methods
impl App {
    fn hovering_on_line(&self, x: usize, y: usize, width: usize) -> bool {
        match self.hover_coordinates {
            Some((hover_x, hover_y)) => {
                hover_y == y && hover_x <= x + width && hover_x > x
            },
            None => false
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
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct CoordinatesInLine {
    x: usize,
    y: usize,
    width: usize
}

impl CoordinatesInLine {
    pub fn new(x: usize, y: usize, width: usize) -> Self {
        CoordinatesInLine {
            x, y, width
        }
    }
    pub fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x && x <= self.x + self.width && self.y == y
    }
}

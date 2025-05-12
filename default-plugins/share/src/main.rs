use std::net::IpAddr;
use zellij_tile::prelude::*;

use std::collections::BTreeMap;

#[derive(Debug, Default)]
struct App {
    web_server_started: bool,
    web_sharing: WebSharing,
    web_clients_allowed: bool,
    session_name: Option<String>,
    web_server_error: Option<String>,
    web_server_ip: Option<IpAddr>,
    web_server_port: Option<u16>,
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::ModeUpdate,
            EventType::WebServerStatus,
        ]);
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
            _ => {},
        }
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        let (web_server_status_items, web_server_items_max_len) = self.render_web_server_status();
        let (current_session_status_items, current_session_items_max_len) =
            self.render_current_session_status();
        let max_item_width = std::cmp::max(
            web_server_items_max_len,
            current_session_items_max_len,
        );
        let item_count = web_server_status_items.len()
            + current_session_status_items.len()
            + 1;
        let base_x = cols.saturating_sub(max_item_width) / 2;
        let base_y = rows.saturating_sub(item_count) / 2; // the + 2 are the line spaces
                                                              // between items
        let mut current_y = base_y;
        for item in web_server_status_items {
            print_text_with_coordinates(item, base_x, current_y, None, None);
            current_y += 1;
        }
        current_y += 1; // space between items
        for item in current_session_status_items {
            print_text_with_coordinates(item, base_x, current_y, None, None);
            current_y += 1;
        }
    }
}

// render methods, return UI components and the width of the widest one
impl App {
    pub fn render_web_server_status(&self) -> (Vec<Text>, usize) {
        let mut max_len = 0;
        let web_server_status_line = if self.web_server_started {
            let title = "Web server: ";
            let value = "RUNNING ";
            let shortcut = "(<Ctrl c> - Stop)";
            max_len = std::cmp::max(
                max_len,
                title.chars().count() + value.chars().count() + shortcut.chars().count(),
            );
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
            max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
            Text::new(format!("{}{}", title, value))
                .color_range(0, ..title.chars().count())
                .color_range(3, title.chars().count()..)
        };
        let info_line = if self.web_server_started {
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
            Text::new(format!("{}{}", title, value)).color_range(0, ..title.chars().count())
        } else {
            let text = "Press <ENTER> to start";
            max_len = std::cmp::max(max_len, text.chars().count());
            Text::new(text).color_range(3, 6..=12)
        };
        (vec![web_server_status_line, info_line], max_len)
    }
    pub fn render_current_session_status(&self) -> (Vec<Text>, usize) {
        let mut max_len = 0;
        let status_line = match self.web_sharing {
            WebSharing::On => {
                let title = "Current session: ";
                let value = "SHARING";
                max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
                Text::new(format!("{}{}", title, value))
                    .color_range(0, ..title.chars().count())
                    .color_range(3, title.chars().count()..)
            },
            WebSharing::Disabled => {
                let title = "Current session: ";
                let value = "SHARING IS DISABLED";
                max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
                Text::new(format!("{}{}", title, value))
                    .color_range(0, ..title.chars().count())
                    .color_range(3, title.chars().count()..)
            },
            WebSharing::Off => {
                let title = "Current session: ";
                let value = "NOT SHARING";
                max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
                Text::new(format!("{}{}", title, value))
                    .color_range(0, ..title.chars().count())
                    .color_range(3, title.chars().count()..)
            },
        };
        let info_line = if self.web_sharing.web_clients_allowed() && self.web_server_started {
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
            Some(Text::new(format!("{}{}", title, value)).color_range(0, ..title.chars().count()))
        } else if self.web_sharing.web_clients_allowed() {
            let text = format!("...but web server is offline");
            max_len = std::cmp::max(max_len, text.chars().count());
            Some(Text::new(text))
        } else {
            let text = "Press <SPACE> to share";
            max_len = std::cmp::max(max_len, text.chars().count());
            Some(Text::new(text).color_range(3, 6..=12))
        };
        let mut text_elements = vec![status_line];
        if let Some(info_line) = info_line {
            text_elements.push(info_line);
        }
        (text_elements, max_len)
    }
}

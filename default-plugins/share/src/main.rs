use zellij_tile::prelude::*;

use std::collections::BTreeMap;

#[derive(Debug, Default)]
struct App {
    web_server_started: bool,
    web_sharing: WebSharing,
    web_clients_allowed: bool,
    session_name: Option<String>,
    web_server_error: Option<String>,
    web_session_info: Vec<WebSessionInfo>,
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::ModeUpdate,
            EventType::WebServerStarted,
            EventType::Timer,
            EventType::WebServerQueryResponse,
            EventType::SessionUpdate,
        ]);
        query_web_server();
        set_timeout(0.5);
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
            },
            Event::WebServerStarted => {
                self.web_server_started = true;
                should_render = true;
            }
            Event::Key(key) => {
                match key.bare_key {
                    BareKey::Enter if key.has_no_modifiers() => {
                        start_web_server();
                    }
                    BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                        stop_web_server();
                    }
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
                          }
                        }
                    }
                    _ => {}
                }
            },
            Event::Timer(_) => {
                query_web_server();
                set_timeout(0.5);
            }
            Event::WebServerQueryResponse(web_serer_status) => {
                match web_serer_status {
                    WebServerQueryResponse::Online => {
                        self.web_server_started = true;
                        self.web_server_error = None;
                    }
                    WebServerQueryResponse::DifferentVersion(version) => {
                        self.web_server_started = false;
                        self.web_server_error = Some(format!("Server online with an incompatible Zellij version: {}", version));
                    },
                    WebServerQueryResponse::RequestFailed(_error) => {
                        self.web_server_started = false;
                    }
                }
                should_render = true;
            }
            Event::SessionUpdate(session_infos, _) => {
                let mut web_session_info = vec![];
                for session_info in session_infos {
                    if session_info.web_clients_allowed {
                        let name = session_info.name;
                        let web_client_count = session_info.web_client_count;
                        let terminal_client_count = session_info.connected_clients.saturating_sub(web_client_count);
                        web_session_info.push(WebSessionInfo { name, web_client_count, terminal_client_count });
                    }
                }
                self.web_session_info = web_session_info;

            }
            _ => {},
        }
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        // MOCK:
        // 1. show server status
        // 2. show sessions shared on the web
        // 3. show connected clients to each session (?) web and terminal
        // 4. controls: start server, restart server
        //
        // Web Server Status: RUNNING/NOT-RUNNING <Ctrl c> > Stop, <Tab> > Start/Restart
        // URL: https://localhost:8082
        //
        // Current session: SHARING/NOT-SHARING
        // Session URL: https://localhost:8082/jumping-tomato
        //
        // All sessions:
        // > session_name (1 web, 2 terminal users)
        // > session_name (0 web, 1 terminal users)
        //  - Open in browser
        //  - Stop sharing
        //
        let (web_server_status_items, web_server_items_max_len) = self.render_web_server_status();
        let (current_session_status_items, current_session_items_max_len) = self.render_current_session_status();
        let (all_sessions_list_title, all_sessions_list_items, all_sessions_items_max_len) = self.render_all_sessions_list();
        let max_item_width = std::cmp::max(web_server_items_max_len, std::cmp::max(current_session_items_max_len, all_sessions_items_max_len));
        let item_count = web_server_status_items.len() + current_session_status_items.len() + 1 + all_sessions_list_items.len();
        let base_x = cols.saturating_sub(max_item_width) / 2;
        let base_y = rows.saturating_sub(item_count + 2) / 2; // the + 2 are the line spaces
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
        current_y += 1; // space between items
        print_text_with_coordinates(all_sessions_list_title, base_x, current_y, None, None);
        current_y += 1;
        print_nested_list_with_coordinates(all_sessions_list_items, base_x, current_y, None, None);
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
            max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count() + shortcut.chars().count());
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
            Text::new(format!("{}{}", title, value)).color_range(0, ..title.chars().count()).color_range(3, title.chars().count()..)
        };
        let info_line = if self.web_server_started {
            let title = "URL: ";
            let value = format!("http://localhost:8082/");
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
                Text::new(format!("{}{}", title, value)).color_range(0, ..title.chars().count()).color_range(3, title.chars().count()..)
            }
            WebSharing::Disabled => {
                let title = "Current session: ";
                let value = "SHARING IS DISABLED";
                max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
                Text::new(format!("{}{}", title, value)).color_range(0, ..title.chars().count()).color_range(3, title.chars().count()..)
            }
            WebSharing::Off => {
                let title = "Current session: ";
                let value = "NOT SHARING";
                max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
                Text::new(format!("{}{}", title, value)).color_range(0, ..title.chars().count()).color_range(3, title.chars().count()..)
            }
        };
        let info_line = if self.web_sharing.web_clients_allowed() && self.web_server_started {
            let title = "Session URL: ";
            let value = format!("http://localhost:8082/{}", self.session_name.clone().unwrap_or_else(|| "".to_owned()));
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
    pub fn render_all_sessions_list(&self) -> (Text, Vec<NestedListItem>, usize) {
        if self.web_session_info.is_empty() || !self.web_server_started {
            let all_sessions_title = "No active sessions.";
            let max_len = all_sessions_title.chars().count();
            let all_sessions = Text::new(all_sessions_title).color_range(1, ..);
            let nested_list = vec![];
            (all_sessions, nested_list, max_len)
        } else {
            let all_sessions_title = "All web sessions:";
            let mut max_len = all_sessions_title.chars().count();
            let all_sessions = Text::new(all_sessions_title).color_range(1, ..);
            let mut nested_list = vec![];
            for web_session_info in &self.web_session_info {
                let session_name = &web_session_info.name;
                let web_client_count = format!("{}", web_session_info.web_client_count);
                let terminal_client_count = format!("{}", web_session_info.terminal_client_count);
                let item_text = format!("{} [{} terminal clients, {} web clients]", session_name, terminal_client_count, web_client_count);
                max_len = std::cmp::max(item_text.chars().count() + 3, max_len); // 3 is the bulletin
                let terminal_client_count_start_pos = session_name.chars().count() + 2;
                let terminal_client_count_end_pos = terminal_client_count_start_pos + terminal_client_count.chars().count();
                let web_client_count_start_pos = terminal_client_count_end_pos + 18;
                let web_client_count_end_pos = web_client_count_start_pos + web_client_count.chars().count();
                nested_list.push(
                    NestedListItem::new(item_text)
                        .color_range(3, terminal_client_count_start_pos..terminal_client_count_end_pos)
                        .color_range(3, web_client_count_start_pos..=web_client_count_end_pos)
                );
            }
            (all_sessions, nested_list, max_len)
        }
    }
}

#[derive(Default, Clone, Eq, PartialEq, Debug)]
pub struct WebSessionInfo {
    pub name: String,
    pub web_client_count: usize,
    pub terminal_client_count: usize
}

impl WebSessionInfo {
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }
    pub fn with_web_client_count(mut self, count: usize) -> Self {
        self.web_client_count = count;
        self
    }
    pub fn with_terminal_client_count(mut self, count: usize) -> Self {
        self.terminal_client_count = count;
        self
    }
}

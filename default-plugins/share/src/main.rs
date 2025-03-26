use zellij_tile::prelude::*;

use std::collections::BTreeMap;

#[derive(Debug, Default)]
struct App {
    web_server_started: bool,
    session_is_shared: bool,
    session_name: Option<String>,
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        //  TODO CONTINUE HERE:
        //  1. when starting up, check the server status by trying to connect to its /info/version
        //     endpoint - if it's the same version, we assume it's up and running (account for
        //     error conditions later)
        //  2. Get the session list info from /info/sessions every 0.5 seconds and populate it in
        //     the UI
        //  3. Once this works, account for error conditions and see how it works when restarting
        //     the server manually
        //  4. Follow this up by populating the mock info on the server with real information and
        //     then build out the rest of the UI (stop sharing session, open in browser, etc.)
        subscribe(&[
            EventType::Key,
            EventType::ModeUpdate,
            EventType::WebServerStarted,
            EventType::Timer,
        ]);
        query_web_server();
        list_web_sessions();
        set_timeout(0.5);
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::ModeUpdate(mode_info) => {
                self.session_name = mode_info.session_name;
                if let Some(session_is_shared) = mode_info.session_is_shared {
                    self.session_is_shared = session_is_shared;
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
                    _ => {}
                }
            },
            Event::Timer(_) => {
                query_web_server();
                list_web_sessions();
                set_timeout(0.5);
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
            let title = "Web server status: ";
            let value = "RUNNING";
            max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
            Text::new(format!("{}{}", title, value)).color_range(0, ..title.chars().count()).color_range(3, title.chars().count()..)
        } else {
            let title = "Web server status: ";
            let value = "NOT RUNNING";
            max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
            Text::new(format!("{}{}", title, value)).color_range(0, ..title.chars().count()).color_range(3, title.chars().count()..)
        };
        let info_line = if self.web_server_started {
            let title = "URL:";
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
        let status_line = if self.session_is_shared {
            let title = "Current session: ";
            let value = "SHARING";
            max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
            Text::new(format!("{}{}", title, value)).color_range(0, ..title.chars().count()).color_range(3, title.chars().count()..)
        } else {
            let title = "Current session: ";
            let value = "NOT SHARING";
            max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
            Text::new(format!("{}{}", title, value)).color_range(0, ..title.chars().count()).color_range(3, title.chars().count()..)
        };
        let info_line = if self.session_is_shared {
            let title = "Session URL: ";
            let value = format!("http://localhost:8082/{}", self.session_name.clone().unwrap_or_else(|| "".to_owned()));
            max_len = std::cmp::max(max_len, title.chars().count() + value.chars().count());
            Text::new(format!("{}{}", title, value)).color_range(0, ..title.chars().count())
        } else {
            let text = "Press <SPACE> to share";
            max_len = std::cmp::max(max_len, text.chars().count());
            Text::new(text).color_range(3, 6..=12)
        };
        (vec![status_line, info_line], max_len)
    }
    pub fn render_all_sessions_list(&self) -> (Text, Vec<NestedListItem>, usize) {
        let all_sessions_title = "All web sessions:";
        let mock_sessions = vec![
            WebSessionInfo::default().with_name("session 1"),
            WebSessionInfo::default().with_name("session 2"),
            WebSessionInfo::default().with_name("session 3"),
        ];
        let mut max_len = all_sessions_title.chars().count();
        let all_sessions = Text::new(all_sessions_title).color_range(1, ..);
        let mut nested_list = vec![];
        for mock_session in mock_sessions {
            max_len = std::cmp::max(mock_session.name.chars().count() + 3, max_len); // 3 is the bulletin
            nested_list.push(NestedListItem::new(mock_session.name));
        }
        (all_sessions, nested_list, max_len)
    }
}

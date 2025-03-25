use zellij_tile::prelude::*;

use std::collections::BTreeMap;

#[derive(Debug, Default)]
struct App {
    web_server_started: bool,
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
        ]);
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::ModeUpdate(mode_info) => {
                self.session_name = mode_info.session_name;
                if let Some(session_is_shared) = mode_info.session_is_shared {
                    self.web_server_started = session_is_shared;
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
        let web_server_status_title = "Web server status:";
        let web_server_status_value = if self.web_server_started { "RUNNING" } else { "NOT RUNNING" };
        let url_title = "URL:";
        let url_value = format!("http://localhost:8082/");
        let current_session_title = "Current session:";
        let current_session_value = if self.web_server_started { "SHARING" } else { "NOT SHARING" }; // TODO: this is not accurate
        let session_url_title = "Session URL:";
        let session_url_value = match &self.session_name {
            Some(session_name) => {
                format!("http://localhost:8082/{}", session_name)
            },
            None => {
                format!("http://localhost:8082/")
            }
        };
        let all_sessions_title = "All web sessions:";
        let mock_sessions = vec![
            WebSessionInfo::default().with_name("session 1"),
            WebSessionInfo::default().with_name("session 2"),
            WebSessionInfo::default().with_name("session 3"),
        ];

        let row_count = 7 + mock_sessions.len();
        let base_y = rows.saturating_sub(row_count) / 2; // TODO: more accurate
        let base_x = cols.saturating_sub(session_url_title.chars().count() + session_url_value.chars().count()) / 2; // TODO: more
                                                                                 // accurate

        print_text_with_coordinates(Text::new(&web_server_status_title).color_range(0, ..), base_x, base_y, None, None);
        print_text_with_coordinates(Text::new(&web_server_status_value).color_range(3, ..), base_x + web_server_status_title.chars().count() + 1, base_y, None, None);
        print_text_with_coordinates(Text::new(&url_title).color_range(0, ..), base_x, base_y + 1, None, None);
        print_text_with_coordinates(Text::new(&url_value), base_x + url_title.chars().count() + 1, base_y + 1, None, None);

        print_text_with_coordinates(Text::new(&current_session_title).color_range(0, ..), base_x, base_y + 3, None, None);
        print_text_with_coordinates(Text::new(&current_session_value).color_range(3, ..), base_x + current_session_title.chars().count() + 1, base_y + 3, None, None);
        print_text_with_coordinates(Text::new(&session_url_title).color_range(0, ..), base_x, base_y + 4, None, None);
        print_text_with_coordinates(Text::new(&session_url_value), base_x + session_url_title.chars().count() + 1, base_y + 4, None, None);

        print_text_with_coordinates(Text::new(&all_sessions_title).color_range(1, ..), base_x, base_y + 6, None, None);
        let mut nested_list = vec![];
        for mock_session in mock_sessions {
            nested_list.push(NestedListItem::new(mock_session.name));
        }
        print_nested_list_with_coordinates(nested_list, base_x, base_y + 7, None, None);
    }
}

// #[derive(Default)]
// struct WebSessionInfo {
//     name: String,
//     web_client_count: usize,
//     terminal_client_count: usize
// }
// 
// impl WebSessionInfo {
//     pub fn with_name(mut self, name: &str) -> Self {
//         self.name = name.to_string();
//         self
//     }
// }

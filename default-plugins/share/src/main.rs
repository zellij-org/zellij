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
        // TODO:
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
        let title = "Share this session to the browser";
        let toggle_text = "<ENTER>";
        let toggle_ribbon_text = "Sharing";
        let url = match &self.session_name {
            Some(session_name) => {
                format!("http://localhost:8082/{}", session_name)
            },
            None => {
                format!("http://localhost:8082/")
            }
        };
        let base_y = rows.saturating_sub(3) / 2;
        let toggle_text_x = cols.saturating_sub(toggle_text.chars().count() + toggle_ribbon_text.chars().count() + 5) / 2;
        let title_x = cols.saturating_sub(title.chars().count()) / 2;
        let url_x = cols.saturating_sub(url.chars().count()) / 2;
        print_text_with_coordinates(Text::new(title).color_range(0, ..), title_x, base_y, None, None);
        print_text_with_coordinates(Text::new(toggle_text).color_range(3, ..), toggle_text_x, base_y + 2, None, None);
        if self.web_server_started {
            print_ribbon_with_coordinates(Text::new(toggle_ribbon_text).selected(), toggle_text_x + 8, base_y + 2, None, None);
            print_text_with_coordinates(Text::new(url), url_x, base_y + 4, None, None);
        } else {
            print_ribbon_with_coordinates(Text::new(toggle_ribbon_text), toggle_text_x + 8, base_y + 2, None, None);
        }
    }
}

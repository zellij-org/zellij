use std::collections::BTreeMap;
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    session_name: String,
    connected_clients: usize,
    tabs: usize,
    panes: usize,
    updates_received: usize,
    permission_prompts_seen: usize,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::SessionUpdate,
            EventType::PermissionRequestResult,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::SessionUpdate(sessions, _) => {
                if let Some(session) = sessions.iter().find(|s| s.is_current_session) {
                    self.session_name = session.name.clone();
                    self.connected_clients = session.connected_clients;
                    self.tabs = session.tabs.len();
                    self.panes = session.panes.panes.values().map(|p| p.len()).sum();
                }
                self.updates_received += 1;
                true
            },
            Event::PermissionRequestResult(_) => {
                self.permission_prompts_seen += 1;
                true
            },
            _ => false,
        }
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        let mut row = 0;
        let line = |text: Text, row: &mut usize| {
            print_text_with_coordinates(text, 0, *row, Some(cols), None);
            *row += 1;
        };

        line(
            Text::new("Example Distribution plugin").color_range(2, ..),
            &mut row,
        );
        row += 1;

        line(Text::new("bundled as: zellij:example-plugin"), &mut row);
        line(
            Text::new(format!(
                "permission prompts: {}",
                self.permission_prompts_seen
            ))
            .color_range(
                if self.permission_prompts_seen == 0 { 3 } else { 1 },
                ..,
            ),
            &mut row,
        );
        row += 1;

        if self.updates_received == 0 {
            line(Text::new("waiting for SessionUpdate..."), &mut row);
        } else {
            line(
                Text::new(format!("session: {}", self.session_name)),
                &mut row,
            );
            line(
                Text::new(format!(
                    "clients: {}  tabs: {}  panes: {}",
                    self.connected_clients, self.tabs, self.panes
                )),
                &mut row,
            );
            line(
                Text::new(format!("SessionUpdate events: {}", self.updates_received)),
                &mut row,
            );
        }
        row += 1;

        for note in [
            "SessionUpdate is gated behind the",
            "ReadApplicationState permission.",
            "This plugin never called",
            "request_permission() and was never",
            "prompted, because the distribution",
            "bundles it into the executable.",
        ] {
            line(Text::new(note), &mut row);
        }
    }
}

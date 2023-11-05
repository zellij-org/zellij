mod resurrectable_sessions;
mod session_list;
mod ui;
use zellij_tile::prelude::*;

use std::collections::BTreeMap;

use ui::{
    components::{
        render_controls_line, render_error, render_renaming_session_screen, render_new_session_line, render_prompt, render_resurrection_toggle,
        Colors,
    },
    SessionUiInfo,
};

use resurrectable_sessions::ResurrectableSessions;
use session_list::SessionList;

#[derive(Default)]
struct State {
    session_name: Option<String>,
    sessions: SessionList,
    resurrectable_sessions: ResurrectableSessions,
    search_term: String,
    new_session_name: Option<String>,
    renaming_session_name: Option<String>,
    error: Option<String>,
    browsing_resurrection_sessions: bool,
    colors: Colors,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::ModeUpdate,
            EventType::SessionUpdate,
            EventType::Key,
            EventType::RunCommandResult,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::ModeUpdate(mode_info) => {
                self.colors = Colors::new(mode_info.style.colors);
                should_render = true;
            },
            Event::Key(key) => {
                should_render = self.handle_key(key);
            },
            Event::PermissionRequestResult(_result) => {
                should_render = true;
            },
            Event::SessionUpdate(session_infos, resurrectable_session_list) => {
                self.resurrectable_sessions
                    .update(resurrectable_session_list);
                self.update_session_infos(session_infos);
                should_render = true;
            },
            _ => (),
        };
        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if self.browsing_resurrection_sessions {
            self.resurrectable_sessions.render(rows, cols);
            return;
        } else if let Some(new_session_name) = self.renaming_session_name.as_ref() {
            render_renaming_session_screen(&new_session_name, rows, cols);
            return;
        }
        render_resurrection_toggle(cols, false);
        render_prompt(
            self.new_session_name.is_some(),
            &self.search_term,
            self.colors,
        );
        let room_for_list = rows.saturating_sub(5); // search line and controls
        self.sessions.update_rows(room_for_list);
        let list = self
            .sessions
            .render(room_for_list, cols.saturating_sub(7), self.colors); // 7 for various ui
        for line in list {
            println!("{}", line.render());
        }
        render_new_session_line(
            &self.new_session_name,
            self.sessions.is_searching,
            self.colors,
        );
        if let Some(error) = self.error.as_ref() {
            render_error(&error, rows, cols);
        } else {
            render_controls_line(self.sessions.is_searching, rows, cols, self.colors);
        }
    }
}

impl State {
    fn reset_selected_index(&mut self) {
        self.sessions.reset_selected_index();
    }
    fn handle_key(&mut self, key: Key) -> bool {
        if self.error.is_some() {
            self.error = None;
            return true;
        }
        let mut should_render = false;
        if let Key::Right = key {
            if self.new_session_name.is_none() {
                self.sessions.result_expand();
            }
            should_render = true;
        } else if let Key::Left = key {
            if self.new_session_name.is_none() {
                self.sessions.result_shrink();
            }
            should_render = true;
        } else if let Key::Down = key {
            if self.browsing_resurrection_sessions {
                self.resurrectable_sessions.move_selection_down();
            } else if self.new_session_name.is_none() && self.renaming_session_name.is_none() {
                self.sessions.move_selection_down();
            }
            should_render = true;
        } else if let Key::Up = key {
            if self.browsing_resurrection_sessions {
                self.resurrectable_sessions.move_selection_up();
            } else if self.new_session_name.is_none() && self.renaming_session_name.is_none() {
                self.sessions.move_selection_up();
            }
            should_render = true;
        } else if let Key::Char(character) = key {
            if character == '\n' {
                self.handle_selection();
            } else if let Some(new_session_name) = self.new_session_name.as_mut() {
                new_session_name.push(character);
            } else if let Some(renaming_session_name) = self.renaming_session_name.as_mut() {
                renaming_session_name.push(character);
            } else if self.browsing_resurrection_sessions {
                self.resurrectable_sessions.handle_character(character);
            } else {
                self.search_term.push(character);
                self.sessions
                    .update_search_term(&self.search_term, &self.colors);
            }
            should_render = true;
        } else if let Key::Backspace = key {
            if let Some(new_session_name) = self.new_session_name.as_mut() {
                if new_session_name.is_empty() {
                    self.new_session_name = None;
                } else {
                    new_session_name.pop();
                }
            } else if let Some(renaming_session_name) = self.renaming_session_name.as_mut() {
                if renaming_session_name.is_empty() {
                    self.renaming_session_name = None;
                } else {
                    renaming_session_name.pop();
                }
            } else if self.browsing_resurrection_sessions {
                self.resurrectable_sessions.handle_backspace();
            } else {
                self.search_term.pop();
                self.sessions
                    .update_search_term(&self.search_term, &self.colors);
            }
            should_render = true;
        } else if let Key::Ctrl('w') = key {
            if self.sessions.is_searching || self.browsing_resurrection_sessions {
                // no-op
            } else if self.new_session_name.is_some() {
                self.new_session_name = None;
            } else {
                self.new_session_name = Some(String::new());
            }
            should_render = true;
        } else if let Key::Ctrl('r') = key {
            if self.sessions.is_searching || self.browsing_resurrection_sessions {
                // no-op
            } else if self.renaming_session_name.is_some() {
                self.renaming_session_name = None;
            } else {
                self.renaming_session_name = Some(String::new());
            }
            should_render = true;
        } else if let Key::Ctrl('c') = key {
            if let Some(new_session_name) = self.new_session_name.as_mut() {
                if new_session_name.is_empty() {
                    self.new_session_name = None;
                } else {
                    new_session_name.clear()
                }
            } else if let Some(renaming_session_name) = self.renaming_session_name.as_mut() {
                if renaming_session_name.is_empty() {
                    self.renaming_session_name = None;
                } else {
                    renaming_session_name.clear()
                }
            } else if !self.search_term.is_empty() {
                self.search_term.clear();
                self.sessions
                    .update_search_term(&self.search_term, &self.colors);
                self.reset_selected_index();
            } else {
                self.reset_selected_index();
                hide_self();
            }
            should_render = true;
        } else if let Key::BackTab = key {
            self.browsing_resurrection_sessions = !self.browsing_resurrection_sessions;
            should_render = true;
        } else if let Key::Delete = key {
            if self.browsing_resurrection_sessions {
                self.resurrectable_sessions.delete_selected_session();
                should_render = true;
            }
        } else if let Key::Ctrl('d') = key {
            if self.browsing_resurrection_sessions {
                self.resurrectable_sessions
                    .show_delete_all_sessions_warning();
                should_render = true;
            }
        } else if let Key::Esc = key {
            if self.renaming_session_name.is_some() {
                self.renaming_session_name = None;
                should_render = true;
            } else if self.new_session_name.is_some() {
                self.new_session_name = None;
                should_render = true;
            } else {
                hide_self();
            }
        }
        should_render
    }
    fn handle_selection(&mut self) {
        if self.browsing_resurrection_sessions {
            if let Some(session_name_to_resurrect) =
                self.resurrectable_sessions.get_selected_session_name()
            {
                switch_session(Some(&session_name_to_resurrect));
            }
        } else if let Some(new_session_name) = &self.new_session_name {
            if new_session_name.is_empty() {
                switch_session(None);
            } else if self.session_name.as_ref() == Some(new_session_name) {
                // noop - we're already here!
                self.new_session_name = None;
            } else {
                switch_session(Some(new_session_name));
            }
        } else if let Some(renaming_session_name) = &self.renaming_session_name.take() {
            if renaming_session_name.is_empty() {
                // TODO: implement these, then implement the error UI, then implement the renaming
                // session screen, then test it
                self.show_error("New name must not be empty.");
                return; // s that we don't hide self
            } else if self.session_name.as_ref() == Some(renaming_session_name) {
                // noop - we're already called that!
                return; // s that we don't hide self
            } else if self.sessions.has_session(&renaming_session_name) {
                self.show_error("A session by this name already exists.");
                return; // s that we don't hide self
            } else if self.resurrectable_sessions.has_session(&renaming_session_name) {
                self.show_error("A resurrectable session by this name already exists.");
                return; // s that we don't hide self
            } else {
                self.update_current_session_name_in_ui(&renaming_session_name);
                rename_session(&renaming_session_name);
                return; // s that we don't hide self
            }
        } else if let Some(selected_session_name) = self.sessions.get_selected_session_name() {
            let selected_tab = self.sessions.get_selected_tab_position();
            let selected_pane = self.sessions.get_selected_pane_id();
            let is_current_session = self.sessions.selected_is_current_session();
            if is_current_session {
                if let Some((pane_id, is_plugin)) = selected_pane {
                    if is_plugin {
                        focus_plugin_pane(pane_id, true);
                    } else {
                        focus_terminal_pane(pane_id, true);
                    }
                } else if let Some(tab_position) = selected_tab {
                    go_to_tab(tab_position as u32);
                }
            } else {
                switch_session_with_focus(&selected_session_name, selected_tab, selected_pane);
            }
        }
        self.reset_selected_index();
        self.new_session_name = None;
        self.search_term.clear();
        self.sessions
            .update_search_term(&self.search_term, &self.colors);
        hide_self();
    }
    fn show_error(&mut self, error_text: &str) {
        self.error = Some(error_text.to_owned());
    }
    fn update_current_session_name_in_ui(&mut self, new_name: &str) {
        if let Some(old_session_name) = self.session_name.as_ref() {
            self.sessions.update_session_name(&old_session_name, new_name);
        }
        self.session_name = Some(new_name.to_owned());

    }
    fn update_session_infos(&mut self, session_infos: Vec<SessionInfo>) {
        let session_infos: Vec<SessionUiInfo> = session_infos
            .iter()
            .map(|s| SessionUiInfo::from_session_info(s))
            .collect();
        let current_session_name = session_infos.iter().find_map(|s| {
            if s.is_current_session {
                Some(s.name.clone())
            } else {
                None
            }
        });
        if let Some(current_session_name) = current_session_name {
            self.session_name = Some(current_session_name);
        }
        self.sessions.set_sessions(session_infos);
    }
}

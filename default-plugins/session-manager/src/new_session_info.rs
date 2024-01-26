use zellij_tile::prelude::*;

#[derive(Default)]
pub struct NewSessionInfo {
    name: String,
    layout_info: LayoutInfo,
    entering_new_session_info: bool,
}

impl NewSessionInfo {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn entering_new_session_info(&self) -> bool {
        self.entering_new_session_info
    }
    pub fn add_char_to_name(&mut self, character: char) {
        self.name.push(character);
    }
    pub fn handle_backspace(&mut self) {
        self.name.pop(); // TODO: no crashy on empty, right?
    }
    pub fn toggle_entering_info(&mut self) {
        self.entering_new_session_info = !self.entering_new_session_info;
    }
    pub fn handle_break(&mut self) {
        self.name.clear();
        self.entering_new_session_info = false;
    }
    pub fn handle_key(&mut self, key: Key) {
        match key {
            Key::Backspace => {
                self.handle_backspace();
            },
            Key::Ctrl('c') | Key::Esc => {
                self.handle_break();
            },
            Key::Char(character) => {
                self.add_char_to_name(character);
            },
            Key::Up => {
                self.move_selection_up();
            }
            Key::Down => {
                self.move_selection_down();
            }
            _ => {}
        }
    }
    pub fn handle_selection(&mut self, current_session_name: &Option<String>) {
        let new_session_layout: Option<String> = self.layout_info.selected_layout_name();
        let new_session_name = if self.name.is_empty() { None } else { Some(self.name.as_str()) };
        if new_session_name != current_session_name.as_ref().map(|s| s.as_str()) {
            match new_session_layout {
                Some(new_session_layout) => {
                    switch_session_with_layout(new_session_name, new_session_layout)
                },
                None => {
                    switch_session(new_session_name);
                }
            }
        }
        self.name.clear();
        self.layout_info.clear_selection();
    }
    pub fn update_layout_list(&mut self, layout_list: Vec<impl Into<String>>) {
        self.layout_info.update_layout_list(layout_list);
    }
    pub fn layout_info(&self) -> Vec<(String, bool)> { // bool - is_selected
        self.layout_info.layout_list.iter().enumerate().map(|(i, l)| (l.clone(), Some(i) == self.layout_info.selected_layout_index)).collect()
    }
    pub fn layout_count(&self) -> usize {
        self.layout_info.layout_list.len()
    }
    pub fn selected_layout_name(&self) -> Option<String> {
        self.layout_info.selected_layout_name()
    }
    pub fn has_selection(&self) -> bool {
        self.layout_info.has_selection()
    }
    fn move_selection_up(&mut self) {
        self.layout_info.move_selection_up();
    }
    fn move_selection_down(&mut self) {
        self.layout_info.move_selection_down();

    }

}

#[derive(Default)]
struct LayoutInfo {
    layout_list: Vec<String>,
    selected_layout_index: Option<usize>,
}

impl LayoutInfo {
    pub fn update_layout_list(&mut self, layout_list: Vec<impl Into<String>>) {
        let old_layout_length = self.layout_list.len();
        self.layout_list = layout_list.into_iter().map(|l| l.into()).collect();
        if old_layout_length != self.layout_list.len() {
            // honestly, this is just the UX choice that sucks the least...
            self.selected_layout_index = None;
        }
    }
    pub fn selected_layout_name(&self) -> Option<String> {
        self.selected_layout_index.and_then(|i| self.layout_list.get(i).cloned())
    }
    pub fn clear_selection(&mut self) {
        self.selected_layout_index = None;
    }
    pub fn has_selection(&self) -> bool {
        self.selected_layout_index.is_some()
    }
    fn move_selection_up(&mut self) {
        if self.selected_layout_index.is_none() && !self.layout_list.is_empty() {
            self.selected_layout_index = Some(self.layout_list.len().saturating_sub(1));
        } else if let Some(selected_layout_index) = self.selected_layout_index.as_mut() {
            if *selected_layout_index == 0 {
                self.selected_layout_index = None;
            } else {
                *selected_layout_index = selected_layout_index.saturating_sub(1);
            }
        }
    }
    fn move_selection_down(&mut self) {
        if self.selected_layout_index.is_none() && !self.layout_list.is_empty() {
            self.selected_layout_index = Some(0);
        } else if let Some(selected_layout_index) = self.selected_layout_index.as_mut() {
            if *selected_layout_index == self.layout_list.len().saturating_sub(1) {
                self.selected_layout_index = None;
            } else {
                *selected_layout_index += 1;
            }
        }

    }
}

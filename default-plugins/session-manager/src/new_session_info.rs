use zellij_tile::prelude::*;

#[derive(Default)]
pub struct NewSessionInfo {
    name: String,
    layout_list: LayoutList,
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
        let new_session_layout: Option<LayoutInfo> = self.layout_list.selected_layout_info();
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
        self.layout_list.clear_selection();
    }
    pub fn update_layout_list(&mut self, layout_info: Vec<LayoutInfo>) {
        self.layout_list.update_layout_list(layout_info);
    }
    pub fn layout_list(&self) -> Vec<(LayoutInfo, bool)> { // bool - is_selected
        self.layout_list.layout_list.iter().enumerate().map(|(i, l)| (l.clone(), Some(i) == self.layout_list.selected_layout_index)).collect()
    }
    pub fn layout_count(&self) -> usize {
        self.layout_list.layout_list.len()
    }
    pub fn selected_layout_info(&self) -> Option<LayoutInfo> {
        self.layout_list.selected_layout_info()
    }
    pub fn has_selection(&self) -> bool {
        self.layout_list.has_selection()
    }
    fn move_selection_up(&mut self) {
        self.layout_list.move_selection_up();
    }
    fn move_selection_down(&mut self) {
        self.layout_list.move_selection_down();

    }

}

#[derive(Default)]
struct LayoutList {
    layout_list: Vec<LayoutInfo>,
    selected_layout_index: Option<usize>,
}

impl LayoutList {
    pub fn update_layout_list(&mut self, layout_list: Vec<LayoutInfo>) {
        let old_layout_length = self.layout_list.len();
        self.layout_list = layout_list;
        if old_layout_length != self.layout_list.len() {
            // honestly, this is just the UX choice that sucks the least...
            self.selected_layout_index = None;
        }
    }
    pub fn selected_layout_info(&self) -> Option<LayoutInfo> {
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

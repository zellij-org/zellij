use super::CommandEntry;

pub struct Selection {
    pub current_selected_command_index: Option<usize>,
    pub scroll_offset: usize,
}

impl Selection {
    pub fn new() -> Self {
        Self {
            current_selected_command_index: None,
            scroll_offset: 0,
        }
    }

    pub fn move_up(&mut self, all_commands: &[CommandEntry]) {
        match self.current_selected_command_index.as_mut() {
            Some(i) if *i > 0 => {
                *i -= 1;
            },
            None => {
                self.current_selected_command_index = Some(all_commands.len().saturating_sub(1));
            },
            _ => {
                self.current_selected_command_index = None;
            },
        }
    }

    pub fn move_down(&mut self, all_commands: &[CommandEntry]) {
        match self.current_selected_command_index.as_mut() {
            Some(i) if *i < all_commands.len().saturating_sub(1) => {
                *i += 1;
            },
            None => {
                self.current_selected_command_index = Some(0);
            },
            _ => {
                self.current_selected_command_index = None;
            },
        }
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::new()
    }
}

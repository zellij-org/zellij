use crate::state::{CommandEntry, Editing};
use std::path::PathBuf;
use zellij_tile::prelude::*;

pub struct Selection {
    pub current_selected_command_index: Option<usize>,
    pub scroll_offset: usize,
}

impl Selection {
    pub fn new() -> Self {
        Self {
            current_selected_command_index: Some(0),
            scroll_offset: 0,
        }
    }

    pub fn add_empty_command_after_current_selected(
        &mut self,
        all_commands: &mut Vec<CommandEntry>,
        editing: &mut Editing,
        cwd: &Option<PathBuf>,
    ) {
        self.save_editing_buffer_to_current_selected(all_commands, editing);
        editing.editing_input.as_mut().map(|i| i.clear());
        if !self.current_selected_command_is_empty(all_commands, editing) {
            self.current_selected_command_mut(all_commands)
                .map(|c| c.fill_chain_type_if_empty());

            if let Some(current_selected_command_index) = self.current_selected_command_index.take()
            {
                let new_command = CommandEntry::new("", cwd.clone());
                if current_selected_command_index == all_commands.len().saturating_sub(1) {
                    all_commands.push(new_command);
                } else {
                    all_commands.insert(current_selected_command_index + 1, new_command);
                }

                self.current_selected_command_index = Some(current_selected_command_index + 1);
            }
        }
    }

    pub fn current_selected_command_is_empty(
        &self,
        all_commands: &[CommandEntry],
        editing: &Editing,
    ) -> bool {
        let text_input_is_empty = editing
            .editing_input
            .as_ref()
            .map(|i| i.is_empty())
            .unwrap_or(true);
        let current_selected_command_is_empty = self
            .current_selected_command_index
            .and_then(|i| all_commands.get(i).map(|c| c.is_empty()))
            .unwrap_or(true);
        text_input_is_empty && current_selected_command_is_empty
    }

    pub fn remove_current_selected_command(
        &mut self,
        all_commands: &mut Vec<CommandEntry>,
        editing: &mut Editing,
    ) -> Option<PaneId> {
        // returns the pane_id of the removed command, if any
        let mut removed_pane_id = None;
        let Some(mut current_selected_command_index) = self.current_selected_command_index else {
            return removed_pane_id;
        };
        if all_commands.len() > 1 && all_commands.len() > current_selected_command_index {
            let command = all_commands.remove(current_selected_command_index);
            removed_pane_id = command.get_pane_id();
        } else {
            self.clear_current_selected_command(all_commands, editing);
        }
        if current_selected_command_index >= all_commands.len()
            && current_selected_command_index > 0
        {
            current_selected_command_index -= 1;
        }
        self.update_current_selected_command_index(
            current_selected_command_index,
            all_commands,
            editing,
        );
        if current_selected_command_index == all_commands.len().saturating_sub(1) {
            self.current_selected_command_mut(all_commands)
                .map(|c| c.clear_chain_type());
        }
        removed_pane_id
    }

    pub fn clear_current_selected_command(
        &mut self,
        all_commands: &mut [CommandEntry],
        editing: &mut Editing,
    ) {
        let Some(current_selected_command_index) = self.current_selected_command_index else {
            return;
        };
        self.current_selected_command_mut(all_commands)
            .map(|c| c.clear_text());
        self.clear_editing_buffer(editing);
        if current_selected_command_index == all_commands.len().saturating_sub(1) {
            self.current_selected_command_mut(all_commands)
                .map(|c| c.clear_chain_type());
        }
    }

    pub fn move_up(&mut self, all_commands: &mut [CommandEntry], editing: &mut Editing) {
        self.save_editing_buffer_to_current_selected(all_commands, editing);
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
        self.update_editing_buffer_to_current_selected(all_commands, editing);
    }

    pub fn move_down(&mut self, all_commands: &mut [CommandEntry], editing: &mut Editing) {
        self.save_editing_buffer_to_current_selected(all_commands, editing);
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
        self.update_editing_buffer_to_current_selected(all_commands, editing);
    }

    fn current_selected_command_mut<'a>(
        &self,
        all_commands: &'a mut [CommandEntry],
    ) -> Option<&'a mut CommandEntry> {
        let Some(i) = self.current_selected_command_index else {
            return None;
        };
        all_commands.get_mut(i)
    }

    fn current_selected_command<'a>(
        &self,
        all_commands: &'a [CommandEntry],
    ) -> Option<&'a CommandEntry> {
        let Some(i) = self.current_selected_command_index else {
            return None;
        };
        all_commands.get(i)
    }

    fn clear_editing_buffer(&self, editing: &mut Editing) {
        editing.editing_input.as_mut().map(|c| c.clear());
    }

    fn update_current_selected_command_index(
        &mut self,
        new_index: usize,
        all_commands: &[CommandEntry],
        editing: &mut Editing,
    ) {
        self.current_selected_command_index = Some(new_index);
        self.update_editing_buffer_to_current_selected(all_commands, editing);
    }

    fn get_text_of_current_selected_command(
        &self,
        all_commands: &[CommandEntry],
    ) -> Option<String> {
        self.current_selected_command(all_commands)
            .map(|c| c.get_text())
    }

    fn save_editing_buffer_to_current_selected(
        &self,
        all_commands: &mut [CommandEntry],
        editing: &mut Editing,
    ) {
        if let Some(text_input) = editing.editing_input.as_ref().map(|i| i.get_text()) {
            self.current_selected_command_mut(all_commands)
                .map(|c| c.set_text(text_input.to_owned()));
        }
    }

    fn update_editing_buffer_to_current_selected(
        &self,
        all_commands: &[CommandEntry],
        editing: &mut Editing,
    ) {
        let new_text_of_current_selected_command = self
            .get_text_of_current_selected_command(all_commands)
            .unwrap_or_else(|| String::new());
        if let Some(editing_input) = editing.editing_input.as_mut() {
            editing_input.set_text(new_text_of_current_selected_command);
        }
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::new()
    }
}

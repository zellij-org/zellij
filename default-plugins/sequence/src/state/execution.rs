use crate::path_formatting::format_cwd;
use crate::state::{CommandEntry, CommandStatus};
use std::collections::BTreeMap;
use std::path::PathBuf;
use zellij_tile::prelude::*;

pub struct Execution {
    pub all_commands: Vec<CommandEntry>,
    pub current_running_command_index: usize,
    pub is_running: bool,
    pub displayed_pane_id: Option<PaneId>,
}

impl Execution {
    pub fn new() -> Self {
        Self {
            all_commands: vec![CommandEntry::default()],
            current_running_command_index: 0,
            is_running: false,
            displayed_pane_id: None,
        }
    }

    pub fn longest_cwd_display(&self, global_cwd: &Option<PathBuf>) -> String {
        self.all_commands
            .iter()
            .map(|cmd| {
                let cwd = cmd.get_cwd().or_else(|| global_cwd.clone());
                if let Some(cwd) = &cwd {
                    format_cwd(cwd)
                } else {
                    "~".to_string()
                }
            })
            .max_by_key(|s| s.len())
            .unwrap_or_else(|| "~".to_string())
    }

    pub fn remove_empty_commands(&mut self) {
        if self.all_commands.iter().len() > 1 {
            self.all_commands.retain(|c| !c.get_text().is_empty());
        }
    }

    pub fn get_first_command(&self) -> Option<CommandEntry> {
        self.all_commands.iter().next().cloned()
    }

    pub fn set_command_status(&mut self, command_index: usize, status: CommandStatus) {
        self.all_commands
            .get_mut(command_index)
            .map(|c| c.set_status(status));
    }

    pub fn set_current_running_command_status(&mut self, status: CommandStatus) {
        self.all_commands
            .get_mut(self.current_running_command_index)
            .map(|c| c.set_status(status));
    }

    pub fn get_current_running_command_status(&self) -> Option<CommandStatus> {
        self.all_commands
            .get(self.current_running_command_index)
            .map(|c| c.get_status())
    }

    pub fn can_run_sequence(&self) -> bool {
        self.all_commands.iter().any(|command| !command.is_empty())
    }

    pub fn copy_to_clipboard(&self) {
        let text_to_copy = self
            .all_commands
            .iter()
            .map(|c| format!("{}", c.get_text()))
            .collect::<Vec<_>>()
            .join("\n");
        if !text_to_copy.is_empty() {
            copy_to_clipboard(text_to_copy);
        }
    }

    pub fn execute_command_sequence(
        &mut self,
        shell: &Option<PathBuf>,
        global_cwd: &Option<PathBuf>,
        plugin_id: Option<u32>,
    ) {
        self.all_commands.retain(|c| !c.is_empty());

        let Some(first_cmd) = self.get_first_command() else {
            return;
        };

        let shell = shell.clone().unwrap_or_else(|| PathBuf::from("/bin/bash"));
        let command_cwd = first_cmd.get_cwd().or_else(|| global_cwd.clone());

        let command = CommandToRun {
            path: shell,
            args: vec!["-ic".to_string(), first_cmd.get_text().trim().to_string()],
            cwd: command_cwd,
        };

        let (tab_id, pane_id) = open_command_pane_in_new_tab(command, BTreeMap::new());
        if let Some(pane_id) = pane_id {
            self.set_command_status(0, CommandStatus::Running(Some(pane_id)));
            self.current_running_command_index = 0;
            self.displayed_pane_id = Some(pane_id);
        }
        if let (Some(tab_id), Some(plugin_id)) = (tab_id, plugin_id) {
            break_panes_to_tab_with_id(&[PaneId::Plugin(plugin_id)], tab_id, true);
            focus_pane_with_id(PaneId::Plugin(plugin_id), false, false); // focus self
        }
    }
}

impl Default for Execution {
    fn default() -> Self {
        Self::new()
    }
}

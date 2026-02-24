mod chain_type;
mod command_entry;
mod command_parser;
mod command_status;
mod execution;
mod layout;
mod selection;
mod sequence_mode;

pub use chain_type::ChainType;
pub use command_entry::CommandEntry;
pub use command_parser::{parse_commands, serialize_sequence_to_editor};
pub use command_status::CommandStatus;
pub use execution::Execution;
pub use layout::Layout;
pub use selection::Selection;
pub use sequence_mode::SequenceMode;

use std::collections::BTreeMap;
use std::path::PathBuf;
use zellij_tile::prelude::*;

#[derive(Default)]
pub struct State {
    pub shell: Option<PathBuf>,
    pub plugin_id: Option<u32>,
    pub client_id: Option<u16>,
    pub own_rows: Option<usize>,
    pub own_columns: Option<usize>,
    pub cwd: Option<PathBuf>,
    pub total_viewport_columns: Option<usize>,
    pub total_viewport_rows: Option<usize>,
    pub selection: Selection,
    pub execution: Execution,
    pub layout: Layout,
    pub current_position: Option<FloatingPaneCoordinates>,
    pub mode: SequenceMode,
    pub editor_pane_id: Option<PaneId>,
    pub editor_temp_file: Option<PathBuf>,
    pub sequence_tab_id: Option<usize>,
    pub blocking_pipe_id: Option<String>,
}

impl State {
    pub fn set_plugin_id(&mut self, plugin_id: u32) {
        self.plugin_id = Some(plugin_id);
    }

    pub fn move_selection_up(&mut self) {
        self.selection.move_up(&self.execution.all_commands);
        self.show_selected_pane();
    }

    pub fn move_selection_down(&mut self) {
        self.selection.move_down(&self.execution.all_commands);
        self.show_selected_pane();
    }

    pub fn show_selected_pane(&mut self) {
        if self.mode == SequenceMode::Spread {
            let all_pane_ids: Vec<PaneId> = self
                .execution
                .all_commands
                .iter()
                .filter_map(|c| c.get_pane_id())
                .collect();
            let selected_pane_id = self
                .current_selected_command()
                .and_then(|c| c.get_pane_id());
            if let Some(selected) = selected_pane_id {
                let to_unhighlight: Vec<PaneId> = all_pane_ids
                    .into_iter()
                    .filter(|&p| p != selected)
                    .collect();
                highlight_and_unhighlight_panes(vec![selected], to_unhighlight);
            }
            return;
        }
        let target_pane_id = self
            .current_selected_command()
            .and_then(|c| c.get_pane_id());
        if let (Some(displayed), Some(target)) = (self.execution.displayed_pane_id, target_pane_id)
        {
            if displayed != target {
                replace_pane_with_existing_pane(displayed, target, true);
                self.execution.displayed_pane_id = Some(target);
            }
        }
    }

    pub fn remove_empty_commands(&mut self) {
        self.execution.remove_empty_commands();
    }

    pub fn clear_all_commands(&mut self) {
        self.execution.all_commands = vec![CommandEntry::new("", self.cwd.clone())];
        self.selection.current_selected_command_index = None;
        self.execution.current_running_command_index = 0;
    }

    pub fn load_commands(&mut self, command_string: &str, cwd_override: Option<PathBuf>) {
        let effective_cwd = cwd_override.or_else(|| self.cwd.clone());
        let commands = parse_commands(command_string, effective_cwd);
        if commands.is_empty() {
            return;
        }
        self.execution.all_commands = commands;
        self.execution.current_running_command_index = 0;
        self.selection.current_selected_command_index = None;
    }

    pub fn can_run_sequence(&self) -> bool {
        self.execution.can_run_sequence()
    }

    pub fn copy_to_clipboard(&mut self) {
        self.execution.copy_to_clipboard();
    }

    pub fn current_selected_command_mut(&mut self) -> Option<&mut CommandEntry> {
        let Some(i) = self.selection.current_selected_command_index else {
            return None;
        };
        self.execution.all_commands.get_mut(i)
    }

    /// Serialize current sequence to a temp file and open it in the user's $EDITOR.
    /// Stores the editor pane id and temp file path for later retrieval.
    pub fn open_editor(&mut self) {
        if self.execution.is_running {
            return;
        }
        let Some(plugin_id) = self.plugin_id else {
            return;
        };

        let serialized = serialize_sequence_to_editor(&self.execution.all_commands);
        let temp_path = PathBuf::from(format!("/tmp/zellij-sequence-{}.sh", plugin_id));

        if let Err(e) = std::fs::write(&temp_path, &serialized) {
            eprintln!("Failed to write sequence to temp file: {}", e);
            return;
        }

        let file_to_open = FileToOpen::new(temp_path.clone());
        let editor_pane_id = open_file_in_place_of_plugin(file_to_open, false, BTreeMap::new());
        self.editor_pane_id = editor_pane_id;
        self.editor_temp_file = Some(temp_path);
    }

    pub fn reposition_plugin(&mut self) -> bool {
        let mut repositioned = false;
        let Some(plugin_id) = self.plugin_id else {
            return repositioned;
        };
        let Some(total_viewport_rows) = self.total_viewport_rows else {
            return repositioned;
        };
        let Some(total_viewport_columns) = self.total_viewport_columns else {
            return repositioned;
        };
        if self.all_commands_are_pending() {
            return false;
        }

        let total_commands = std::cmp::max(self.execution.all_commands.iter().len(), 1);
        let height_padding = 7;

        // Calculate longest table row width
        let longest_cwd_display = self.execution.longest_cwd_display(&self.cwd);
        let longest_command = self
            .execution
            .all_commands
            .iter()
            .map(|cmd| cmd.get_text().chars().count())
            .max()
            .unwrap_or(1);

        let (max_chain_width, max_status_width) = crate::ui::components::calculate_max_widths(
            &self.execution.all_commands,
            self.layout.spinner_frame,
        );

        let longest_line = crate::ui::components::calculate_longest_line(
            &longest_cwd_display,
            longest_command,
            max_chain_width,
            max_status_width,
        );

        let ui_width = std::cmp::max(longest_line, 50) + 4; // 2 for ui-padding, 2 for pane frame
        let width = std::cmp::min(ui_width, total_viewport_columns.saturating_sub(2));

        let height = (height_padding + total_commands).min((total_viewport_rows * 50) / 100);

        // Position: top-right with 1-space margins
        let x = total_viewport_columns.saturating_sub(width);
        let y = 1;

        let coordinates = FloatingPaneCoordinates::new(
            Some(x.to_string()),
            Some(y.to_string()),
            Some(width.to_string()),
            Some(height.to_string()),
            Some(true), // should be pinned for sequence
            Some(false),
        );
        coordinates.map(|coordinates| {
            if Some(&coordinates) != self.current_position.as_ref() {
                self.current_position = Some(coordinates.clone());
                repositioned = true;
                change_floating_panes_coordinates(vec![(PaneId::Plugin(plugin_id), coordinates)]);
            }
        });
        repositioned
    }

    pub fn all_commands_are_pending(&self) -> bool {
        self.execution
            .all_commands
            .iter()
            .all(|command| matches!(command.get_status(), CommandStatus::Pending))
    }

    fn current_selected_command(&self) -> Option<&CommandEntry> {
        let Some(i) = self.selection.current_selected_command_index else {
            return None;
        };
        self.execution.all_commands.get(i)
    }
}

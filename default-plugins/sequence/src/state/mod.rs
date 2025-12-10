mod chain_type;
mod command_entry;
mod command_parser;
mod command_status;
mod editing;
mod execution;
mod layout;
pub mod positioning;
mod selection;

pub use chain_type::ChainType;
pub use command_entry::CommandEntry;
pub use command_parser::{
    detect_cd_command, detect_chain_operator_at_end, get_remaining_after_first_segment,
    split_by_chain_operators,
};
pub use command_status::CommandStatus;
pub use editing::Editing;
pub use execution::Execution;
pub use layout::Layout;
pub use selection::Selection;

use crate::ui::text_input::TextInput;
use std::path::PathBuf;
use zellij_tile::prelude::*;

pub struct State {
    pub shell: Option<PathBuf>,
    pub plugin_id: Option<u32>,
    pub client_id: Option<u16>,
    pub own_rows: Option<usize>,
    pub own_columns: Option<usize>,
    pub cwd: Option<PathBuf>,
    pub original_pane_id: Option<PaneId>,
    pub primary_pane_id: Option<PaneId>,
    pub is_first_run: bool,
    pub pane_manifest: Option<PaneManifest>,
    pub total_viewport_columns: Option<usize>,
    pub total_viewport_rows: Option<usize>,
    pub selection: Selection,
    pub editing: Editing,
    pub execution: Execution,
    pub layout: Layout,
    pub current_position: Option<FloatingPaneCoordinates>,
}

impl State {
    pub fn new() -> Self {
        Self {
            shell: None,
            plugin_id: None,
            client_id: None,
            own_rows: None,
            own_columns: None,
            cwd: None,
            original_pane_id: None,
            primary_pane_id: None,
            is_first_run: true,
            pane_manifest: None,
            total_viewport_columns: None,
            total_viewport_rows: None,
            selection: Selection::new(),
            editing: Editing::new(),
            execution: Execution::new(),
            layout: Layout::new(),
            current_position: None,
        }
    }

    pub fn set_plugin_id(&mut self, plugin_id: u32) {
        self.plugin_id = Some(plugin_id);
    }

    /// Check if the sequence has finished executing (all commands are done)
    pub fn has_finished(&self) -> bool {
        self.execution
            .all_commands
            .iter()
            .all(|command| matches!(command.status, CommandStatus::Exited(_, _)))
    }
    pub fn add_empty_command_after_current_selected(&mut self) {
        self.selection.add_empty_command_after_current_selected(
            &mut self.execution.all_commands,
            &mut self.editing,
            &self.cwd,
        );
    }

    pub fn current_selected_command_is_empty(&self) -> bool {
        self.selection
            .current_selected_command_is_empty(&self.execution.all_commands, &self.editing)
    }

    pub fn remove_current_selected_command(&mut self) {
        let removed_pane_id = self
            .selection
            .remove_current_selected_command(&mut self.execution.all_commands, &mut self.editing);
        if let Some(removed_pane_id) = removed_pane_id {
            close_pane_with_id(removed_pane_id);
        }
    }

    pub fn clear_current_selected_command(&mut self) {
        self.selection
            .clear_current_selected_command(&mut self.execution.all_commands, &mut self.editing);
    }

    pub fn move_selection_up(&mut self) {
        self.selection
            .move_up(&mut self.execution.all_commands, &mut self.editing);
        if let Some(pane_id) = self
            .current_selected_command()
            .and_then(|c| c.get_pane_id())
        {
            show_pane_with_id(pane_id, true, false);
        }
    }

    pub fn move_selection_down(&mut self) {
        self.selection
            .move_down(&mut self.execution.all_commands, &mut self.editing);
        if let Some(pane_id) = self
            .current_selected_command()
            .and_then(|c| c.get_pane_id())
        {
            show_pane_with_id(pane_id, true, false);
        }
    }
    pub fn start_editing_selected(&mut self) {
        if let Some(current_command_text) = self.current_selected_command().map(|c| c.get_text()) {
            self.editing.start_editing(current_command_text);
        }
    }

    pub fn cancel_editing_selected(&mut self) {
        self.editing.cancel_editing();
    }

    pub fn editing_input_text(&self) -> Option<String> {
        self.editing.input_text()
    }

    pub fn set_editing_input_text(&mut self, text: String) {
        self.editing.set_input_text(text);
    }
    fn handle_first_pasted_segment(
        &mut self,
        current_text: &str,
        segment_text: &str,
        chain_type_opt: &Option<ChainType>,
    ) {
        let new_text = if current_text.trim().is_empty() {
            segment_text.to_string()
        } else {
            format!("{}{}", current_text, segment_text)
        };

        if let Some(path) = detect_cd_command(&new_text) {
            use crate::path_formatting;
            if let Some(new_cwd) = path_formatting::resolve_path(self.cwd.as_ref(), &path) {
                self.current_selected_command_mut().map(|c| {
                    c.set_cwd(Some(new_cwd));
                    c.set_text("".to_owned());
                    if let Some(chain_type) = chain_type_opt {
                        c.set_chain_type(*chain_type);
                    }
                });
                return;
            }
        }

        self.current_selected_command_mut().map(|c| {
            c.set_text(new_text);
            if let Some(chain_type) = chain_type_opt {
                c.set_chain_type(*chain_type);
            }
        });
    }

    fn insert_new_pasted_segment(
        &mut self,
        segment_text: &str,
        chain_type_opt: &Option<ChainType>,
        is_last_line: bool,
    ) {
        use crate::path_formatting;

        let cd_path = detect_cd_command(segment_text);

        if let Some(path) = cd_path {
            if let Some(new_cwd) = path_formatting::resolve_path(self.cwd.as_ref(), &path) {
                self.current_selected_command_mut().map(|c| {
                    c.set_cwd(Some(new_cwd));
                    if let Some(chain_type) = chain_type_opt {
                        c.set_chain_type(*chain_type);
                    }
                });
                return;
            }
        }

        let Some(new_selected_index) = self.selection.current_selected_command_index.map(|i| i + 1)
        else {
            return;
        };

        let mut new_command = CommandEntry::new(segment_text, self.cwd.clone());
        if let Some(chain_type) = chain_type_opt {
            new_command.set_chain_type(*chain_type);
        } else if !is_last_line {
            new_command.set_chain_type(ChainType::And);
        }

        self.execution
            .all_commands
            .insert(new_selected_index, new_command);
        self.selection.current_selected_command_index = Some(new_selected_index);
    }

    fn ensure_line_end_chain_type(&mut self) {
        if let Some(last_cmd_index) = self.selection.current_selected_command_index {
            self.execution
                .all_commands
                .get_mut(last_cmd_index)
                .map(|c| {
                    if matches!(c.get_chain_type(), ChainType::None) {
                        c.set_chain_type(ChainType::And);
                    }
                });
        }
    }

    pub fn pasted_lines(&mut self, lines: Vec<&str>) {
        let Some(current_text) = self.editing_input_text() else {
            return;
        };

        for (line_index, line) in lines.iter().enumerate() {
            let segments = split_by_chain_operators(line);
            let is_last_line = line_index == lines.len().saturating_sub(1);

            for (seg_index, (segment_text, chain_type_opt)) in segments.iter().enumerate() {
                if line_index == 0 && seg_index == 0 {
                    self.handle_first_pasted_segment(&current_text, segment_text, chain_type_opt);
                } else {
                    self.insert_new_pasted_segment(segment_text, chain_type_opt, is_last_line);
                }
            }

            if !is_last_line {
                self.ensure_line_end_chain_type();
            }
        }

        self.start_editing_selected();
    }
    pub fn remove_empty_commands(&mut self) {
        self.execution.remove_empty_commands();
    }

    pub fn get_first_command(&self) -> Option<CommandEntry> {
        self.execution.get_first_command()
    }

    pub fn set_command_status(&mut self, command_index: usize, status: CommandStatus) {
        self.execution.set_command_status(command_index, status);
    }

    pub fn set_current_running_command_status(&mut self, status: CommandStatus) {
        self.execution.set_current_running_command_status(status);
    }

    pub fn get_current_running_command_status(&mut self) -> Option<CommandStatus> {
        self.execution.get_current_running_command_status()
    }

    pub fn handle_editing_submit(&mut self, current_cwd: &Option<PathBuf>) -> bool {
        let (handled_internally, new_selection_index) = self.editing.handle_submit(
            self.selection.current_selected_command_index,
            &mut self.execution.all_commands,
            current_cwd,
        );
        self.selection.current_selected_command_index = new_selection_index;
        handled_internally
    }

    pub fn update_pane_id_for_command(&mut self, pane_id: PaneId, command_text: &str) {
        self.execution
            .update_pane_id_for_command(pane_id, command_text);
    }
    pub fn update_exited_command_statuses(&mut self, pane_manifest: &PaneManifest) -> bool {
        self.execution.update_exited_command_statuses(pane_manifest)
    }
    pub fn update_sequence_stopped_state(&mut self) -> bool {
        self.execution.update_sequence_stopped_state()
    }

    pub fn cycle_chain_type(&mut self) {
        self.current_selected_command_mut()
            .map(|c| c.cycle_chain_type());
    }

    pub fn set_primary_pane_id_before_sequence(&mut self, pane_id: Option<PaneId>) {
        if pane_id.is_some() {
            self.execution.primary_pane_id_before_sequence = pane_id;
        }
    }
    pub fn clear_all_commands(&mut self) {
        self.execution.all_commands = vec![CommandEntry::new("", self.cwd.clone())];
        self.editing.editing_input = Some(TextInput::new("".to_owned()));
        self.selection.current_selected_command_index = Some(0);
        self.execution.current_running_command_index = 0;
    }
    pub fn update_running_state(&mut self, primary_pane_id: Option<PaneId>) {
        self.remove_empty_commands();
        self.set_primary_pane_id_before_sequence(primary_pane_id);
        self.layout.needs_reposition = true;
        self.execution.is_running = true;
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
        let total_commands = std::cmp::max(self.execution.all_commands.iter().len(), 1);
        let height_padding = 7;
        if self.all_commands_are_pending() {
            let initial_height = total_commands + height_padding;
            let y = total_viewport_rows.saturating_sub(initial_height) / 2;
            let coordinates = FloatingPaneCoordinates::new(
                Some(format!("25%")),
                Some(format!("{}", y)),
                Some(format!("50%")),
                Some(format!("{}", initial_height)),
                Some(false), // should not be pinned when running sequence
            );
            coordinates.map(|coordinates| {
                if Some(&coordinates) != self.current_position.as_ref() {
                    repositioned = true;
                    self.current_position = Some(coordinates.clone());
                    change_floating_panes_coordinates(vec![(
                        PaneId::Plugin(plugin_id),
                        coordinates,
                    )]);
                }
            });
        } else {
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
            let width = std::cmp::min(ui_width, total_viewport_columns / 2);

            let height = (height_padding + total_commands).min((total_viewport_rows * 25) / 100);

            // Position: top-right with 1-space margins
            let x = total_viewport_columns.saturating_sub(width);
            let y = 1;

            let coordinates = FloatingPaneCoordinates::new(
                Some(x.to_string()),
                Some(y.to_string()),
                Some(width.to_string()),
                Some(height.to_string()),
                Some(true), // should be pinned for sequence
            );
            coordinates.map(|coordinates| {
                if Some(&coordinates) != self.current_position.as_ref() {
                    self.current_position = Some(coordinates.clone());
                    repositioned = true;
                    change_floating_panes_coordinates(vec![(
                        PaneId::Plugin(plugin_id),
                        coordinates,
                    )]);
                }
            });
        }
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

    pub fn execute_command_sequence(&mut self) {
        self.execution
            .execute_command_sequence(&self.shell, &self.cwd, self.primary_pane_id);
    }
}

impl Default for State {
    fn default() -> Self {
        let mut state = Self::new();
        state.is_first_run = true;
        state
    }
}

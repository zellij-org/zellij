mod path_formatting;
mod state;
mod ui;

use crate::state::CommandStatus;
use crate::ui::components;
use crate::ui::fuzzy_complete;
use crate::ui::layout_calculations::calculate_viewport;
use crate::ui::text_input::InputAction;
use crate::ui::truncation::truncate_middle;
use state::State;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

use std::collections::BTreeMap;
use std::path::PathBuf;

use unicode_width::UnicodeWidthStr;

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::ModeUpdate,
            EventType::SessionUpdate,
            EventType::Key,
            EventType::PermissionRequestResult,
            EventType::HostFolderChanged,
            EventType::RunCommandResult,
            EventType::ActionComplete,
            EventType::PaneUpdate,
            EventType::Timer,
            EventType::PastedText,
            EventType::TabUpdate,
            EventType::CommandPaneOpened,
        ]);

        // Store our own plugin ID and client ID
        let plugin_ids = get_plugin_ids();
        self.plugin_id = Some(plugin_ids.plugin_id);
        self.client_id = Some(plugin_ids.client_id);
        self.cwd = Some(plugin_ids.initial_cwd);
        update_title(self);
    }

    fn update(&mut self, event: Event) -> bool {
        handle_event(self, event)
    }

    fn render(&mut self, rows: usize, cols: usize) {
        // Store dimensions for use in cursor calculations
        self.own_rows = Some(rows);
        self.own_columns = Some(cols);

        let max_visible_rows = rows.saturating_sub(5);

        let base_x = 1;
        let base_y = 0;

        let (offset, visible_count, hidden_above, hidden_below) = calculate_viewport(
            self.execution.all_commands.len(),
            max_visible_rows,
            self.selection.current_selected_command_index,
            self.execution.current_running_command_index,
        );

        let mut table = components::build_table_header(hidden_above > 0 || hidden_below > 0);

        for index in offset..offset + visible_count {
            table = components::add_command_row(
                table,
                self,
                index,
                offset,
                visible_count,
                hidden_above,
                hidden_below,
            );
        }

        print_table_with_coordinates(
            table,
            base_x,
            base_y,
            self.own_columns.map(|o| o.saturating_sub(base_x)),
            None,
        );

        let help_y = base_y + visible_count + 2;
        let (first_help, _, second_help) = components::render_help_lines(self, Some(cols));
        print_text_with_coordinates(first_help, base_x, help_y, None, None);

        if let Some((second_help_text, _)) = second_help {
            print_text_with_coordinates(second_help_text, base_x, help_y + 1, None, None);
        }
    }
}

pub fn handle_event(state: &mut State, event: Event) -> bool {
    use std::path::PathBuf;

    match event {
        Event::PermissionRequestResult(_) => {
            change_host_folder(PathBuf::from("/"));
            update_title(state);
            true
        },
        Event::ModeUpdate(mode_info) => {
            state.shell = mode_info.shell.clone();

            false
        },
        Event::Key(key) => {
            let mut should_render = handle_key_event(state, key);
            let repositioned = state.reposition_plugin();
            if repositioned {
                // we only want to render once we have repositioned, we will do this in TabUpdate
                should_render = false;
            }
            update_cursor(state);
            should_render
        },
        Event::SessionUpdate(session_infos, _resurrectable_sessions) => {
            if state.is_first_run {
                // Find the current session
                let current_session = session_infos.iter().find(|s| s.is_current_session);

                if let Some(session) = current_session {
                    // Get the pane history for this client
                    if let Some(client_id) = state.client_id {
                        if let Some(pane_history) = session.pane_history.get(&client_id) {
                            let own_pane_id = state.plugin_id.map(|id| PaneId::Plugin(id));
                            state.primary_pane_id = select_primary_pane_from_history(
                                pane_history,
                                &session.panes,
                                own_pane_id,
                            );
                            state.original_pane_id = state.primary_pane_id; // pane id focused
                                                                            // before plugin
                                                                            // launched
                            state.is_first_run = false;
                        }
                    }
                }
            }
            false
        },
        Event::Timer(_elapsed) => {
            // Timer events are used for lock backoff

            let should_render = update_spinner(state);

            should_render
        },
        Event::PastedText(pasted_text) => {
            // Split pasted text into lines
            let mut should_render = true;
            let lines: Vec<&str> = pasted_text
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect();

            if lines.is_empty() {
                return false;
            }
            state.pasted_lines(lines);
            let repositioned = state.reposition_plugin();
            if repositioned {
                // we only want to render once we have repositioned, we will do this in TabUpdate
                should_render = false;
            }
            update_cursor(state);

            should_render
        },
        Event::TabUpdate(tab_infos) => {
            if let Some(tab_info) = tab_infos.iter().find(|t| t.active) {
                let new_cols = Some(tab_info.viewport_columns);
                let new_rows = Some(tab_info.viewport_rows);

                // Check if dimensions changed
                let dimensions_changed = new_cols != state.total_viewport_columns
                    || new_rows != state.total_viewport_rows;

                state.total_viewport_columns = new_cols;
                state.total_viewport_rows = new_rows;

                if dimensions_changed {
                    state.reposition_plugin();
                }
                update_cursor(state);
            }

            false
        },
        Event::PaneUpdate(pane_manifest) => handle_pane_update(state, pane_manifest),
        Event::ActionComplete(_action, pane_id, context) => {
            let should_render = handle_action_complete(state, pane_id, context);
            update_title(state);
            should_render
        },
        Event::CommandPaneOpened(terminal_pane_id, context) => {
            // we get this event immediately as the pane opens, we use it to associate the
            // pane's id with our state

            if !is_running_sequence(&context, state.execution.sequence_id) {
                // action from previous sequence or unrelated
                return false;
            }
            if let Some(command_text) = context.get("command_text") {
                state.update_pane_id_for_command(PaneId::Terminal(terminal_pane_id), command_text);
                if !state.layout.spinner_timer_scheduled {
                    set_timeout(0.1);
                    state.layout.spinner_timer_scheduled = true;
                }
                update_title(state);
            }

            true
        },
        _ => false,
    }
}

fn handle_key_event(state: &mut State, key: KeyWithModifier) -> bool {
    // Ctrl+Enter - Insert command after current with AND chain type
    if key.has_modifiers(&[KeyModifier::Ctrl]) && matches!(key.bare_key, BareKey::Enter) {
        let mut is_cd = false;
        if let Some(current_text) = state.editing_input_text() {
            if let Some(path) = state::detect_cd_command(&current_text) {
                if let Some(new_cwd) = path_formatting::resolve_path(state.cwd.as_ref(), &path) {
                    state.current_selected_command_mut().map(|c| {
                        c.set_cwd(Some(new_cwd));
                        c.set_text("".to_owned());
                    });
                    state.editing.editing_input.as_mut().map(|i| i.clear());
                    is_cd = true
                }
            }
        };

        state.add_empty_command_after_current_selected();
        if is_cd {
            state.start_editing_selected();
        }
        return true;
    }

    // Ctrl+X - Cycle chain type
    if key.has_modifiers(&[KeyModifier::Ctrl]) && matches!(key.bare_key, BareKey::Char('x')) {
        // return state.ui.command_sequence.cycle_current_chain_type();
        state.cycle_chain_type();
        return true;
    }

    // Ctrl+Space, copy to clipboard
    if key.has_modifiers(&[KeyModifier::Ctrl]) && matches!(key.bare_key, BareKey::Char(' ')) {
        state.copy_to_clipboard();
        return false;
    }

    // Ctrl+w, close all panes and clear sequence
    if key.has_modifiers(&[KeyModifier::Ctrl]) && matches!(key.bare_key, BareKey::Char('w')) {
        if !state.execution.is_running && !state.all_commands_are_pending() {
            close_panes_and_return_to_shell(state);
        }
        return true;
    }

    if matches!(key.bare_key, BareKey::Up)
        && !key.has_modifiers(&[KeyModifier::Ctrl])
        && !key.has_modifiers(&[KeyModifier::Alt])
    {
        state.move_selection_up();
        return true;
    }

    if matches!(key.bare_key, BareKey::Down)
        && !key.has_modifiers(&[KeyModifier::Ctrl])
        && !key.has_modifiers(&[KeyModifier::Alt])
    {
        state.move_selection_down();
        return true;
    }

    // Del - delete current command
    if matches!(key.bare_key, BareKey::Delete)
        && !key.has_modifiers(&[KeyModifier::Ctrl])
        && !key.has_modifiers(&[KeyModifier::Alt])
    {
        state.remove_current_selected_command();
        return true;
    }

    // Ctrl+C - Clear currently focused command, or remove it if already empty
    if key.has_modifiers(&[KeyModifier::Ctrl]) && matches!(key.bare_key, BareKey::Char('c')) {
        if state.execution.is_running {
            interrupt_sequence(state);
            return true;
        } else if state.editing.editing_input.is_some() {
            let is_empty = state.current_selected_command_is_empty();
            let has_more_than_one_command = state.execution.all_commands.len() > 1;

            if is_empty && has_more_than_one_command {
                state.remove_current_selected_command();
            } else if is_empty {
                // last command, return to shell
                close_panes_and_return_to_shell(state);
            } else {
                // If not empty, clear it
                state.clear_current_selected_command();
            }
            return true;
        }
    }

    if state.editing.editing_input.is_none()
        && key.has_no_modifiers()
        && matches!(key.bare_key, BareKey::Char('e'))
    {
        state.start_editing_selected();
        return true;
    }

    if state.editing.editing_input.is_none()
        && key.has_no_modifiers()
        && matches!(key.bare_key, BareKey::Enter)
    {
        rerun_sequence(state);
        return true;
    }

    // Handle input actions from TextInput
    let is_backspace = matches!(key.bare_key, BareKey::Backspace);

    if let Some(action) = state
        .editing
        .editing_input
        .as_mut()
        .map(|i| i.handle_key(key))
    {
        match action {
            InputAction::Submit => {
                return handle_submit(state);
            },
            InputAction::Cancel => {
                state.cancel_editing_selected();
                if state.all_commands_are_pending() {
                    // we always want to stay in editing mode in main screen
                    state.start_editing_selected();
                }
                true
            },
            InputAction::Complete => {
                state
                    .editing_input_text()
                    .map(|current_text| {
                        if let Some(completed) = fuzzy_complete::fuzzy_complete(
                            &current_text,
                            &Default::default(), // TODO: get rid of this whoel thing?
                            state.cwd.as_ref(),
                        ) {
                            state.set_editing_input_text(completed);
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false)
            },
            InputAction::Continue => {
                if let Some(current_text) = state.editing_input_text() {
                    if let Some((cmd_text, chain_type)) =
                        state::detect_chain_operator_at_end(&current_text)
                    {
                        let mut should_add_empty_line = true;
                        if let Some(path) = state::detect_cd_command(&cmd_text) {
                            if let Some(new_cwd) =
                                path_formatting::resolve_path(state.cwd.as_ref(), &path)
                            {
                                let remaining_text =
                                    state::get_remaining_after_first_segment(&current_text)
                                        .unwrap_or_else(|| "".to_owned());
                                if remaining_text.len() > 0 {
                                    should_add_empty_line = false;
                                }
                                state.current_selected_command_mut().map(|c| {
                                    c.set_cwd(Some(new_cwd));
                                    c.set_text(remaining_text);
                                });
                            }
                        } else {
                            state.current_selected_command_mut().map(|c| {
                                c.set_text(cmd_text);
                                c.set_chain_type(chain_type);
                            });
                        };
                        state.cancel_editing_selected();
                        if should_add_empty_line {
                            state.add_empty_command_after_current_selected();
                        }
                        state.start_editing_selected();
                    }
                }

                // Handle backspace on empty command
                if is_backspace {
                    let is_empty = state.current_selected_command_is_empty()
                        || state
                            .editing_input_text()
                            .map(|i| i.is_empty())
                            .unwrap_or(false);
                    let has_more_than_one_command = state.execution.all_commands.len() > 1;

                    if is_empty && has_more_than_one_command {
                        let current_index =
                            state.selection.current_selected_command_index.unwrap_or(0);
                        if current_index > 0 {
                            state.remove_current_selected_command();
                            return true;
                        }
                    } else if is_empty {
                        // last command
                        close_panes_and_return_to_shell(state);
                    }
                }

                true
            },
            InputAction::NoAction => false,
        }
    } else {
        false
    }
}

/// Handle Enter key - submit and execute the command sequence
fn handle_submit(state: &mut State) -> bool {
    let cwd = state.cwd.clone();
    if state.handle_editing_submit(&cwd) {
        // handled the command internally (eg. cd) no need to run sequence
        return true;
    }

    if state.execution.all_commands.len() == 1 && state.can_run_sequence() {
        rerun_sequence(state);
    } else if state.execution.all_commands.len() == 1 {
        state.move_selection_down();
        state.start_editing_selected();
    }
    true
}

pub fn handle_pane_update(state: &mut State, pane_manifest: PaneManifest) -> bool {
    // Store the manifest for later use
    state.pane_manifest = Some(pane_manifest.clone());
    let mut needs_rerender = false;
    if state.update_exited_command_statuses(&pane_manifest) {
        needs_rerender = true;
    }
    if state.update_sequence_stopped_state() {
        needs_rerender = true;
    }
    state.reposition_plugin();
    update_primary_and_original_pane_ids(state, &pane_manifest);
    needs_rerender
}

pub fn calculate_cursor_position(state: &mut State) -> Option<(usize, usize)> {
    // Get pane dimensions from state (must be set by render)
    let Some(cols) = state.own_columns else {
        eprintln!("Warning: own_columns not set");
        return None;
    };
    let Some(rows) = state.own_rows else {
        eprintln!("Warning: own_rows not set");
        return None;
    };

    // Only show cursor if in edit mode
    let (text, cursor_pos) = if let Some(text_input) = &state.editing.editing_input {
        (
            text_input.get_text().to_string(),
            text_input.cursor_position(),
        )
    } else {
        return None;
    };

    let Some(edit_index) = state.selection.current_selected_command_index else {
        return None;
    };

    // Calculate folder column width using the longest cwd across all commands
    let longest_cwd_display = state.execution.longest_cwd_display(&state.cwd);
    let folder_display = format!("{} >", longest_cwd_display);
    let folder_col_width = folder_display.width().max(1);

    let base_x = 1;
    let base_y = 0;

    adjust_scroll_offset(state, rows);

    let max_visible_rows = state.own_rows.map(|r| r.saturating_sub(5)).unwrap_or(0);

    let (offset, visible_count, hidden_above, hidden_below) = calculate_viewport(
        state.execution.all_commands.len(),
        max_visible_rows,
        state.selection.current_selected_command_index,
        state.execution.current_running_command_index,
    );

    let (max_chain_width, max_status_width) =
        components::calculate_max_widths(&state.execution.all_commands, state.layout.spinner_frame);

    let Some((_, available_cmd_width)) = components::calculate_row_layout_info(
        edit_index,
        offset,
        visible_count,
        hidden_above,
        hidden_below,
        cols,
        folder_col_width,
        max_chain_width,
        max_status_width,
    ) else {
        return None;
    };

    // Get cursor position in truncated text
    let (_, cursor_in_truncated) = truncate_middle(&text, available_cmd_width, Some(cursor_pos));
    let Some(cursor_in_truncated) = cursor_in_truncated else {
        return None;
    };

    // Calculate relative coordinates within the visible table
    let relative_y = edit_index.saturating_sub(offset) + 1; // +1 for header row
    let relative_x = folder_col_width + 1 + cursor_in_truncated;

    let x = base_x + relative_x;
    let y = base_y + relative_y;

    Some((x, y))
}

pub fn handle_action_complete(
    state: &mut State,
    pane_id: Option<PaneId>,
    context: BTreeMap<String, String>,
) -> bool {
    // Update primary_pane_id to the pane that just completed the action
    // This ensures we always have a valid pane to target for InPlace launches
    // But only if it's not a floating pane
    if let (Some(pane_id), Some(manifest)) = (pane_id, &state.pane_manifest) {
        if !is_pane_floating(pane_id, manifest) {
            state.primary_pane_id = Some(pane_id);
        } else {
            eprintln!("Not setting primary_pane_id to floating pane {:?}", pane_id);
        }
    }

    if !is_running_sequence(&context, state.execution.sequence_id) {
        // action from previous sequence or unrelated
        return false;
    }

    // If the sequence has been stopped (e.g., via Ctrl+C), don't continue
    if !state.execution.is_running {
        return true; // Re-render to show the stopped state
    }

    // Move to the next command
    let next_index = state.execution.current_running_command_index + 1;

    if next_index < state.execution.all_commands.len() {
        // Execute the next command
        // let (next_command, next_chain_type) = &state.execution.all_commands[next_index];
        let Some(next_command) = state.execution.all_commands.get(next_index) else {
            // invalid state
            return true;
        };
        let next_chain_type = next_command.get_chain_type();
        let next_command_cwd = next_command.get_cwd();
        let next_command_text = next_command.get_text();

        let shell = state
            .shell
            .clone()
            .unwrap_or_else(|| PathBuf::from("/bin/bash"));

        let command = zellij_tile::prelude::actions::RunCommandAction {
            command: shell,
            args: vec!["-ic".to_string(), next_command_text.trim().to_string()],
            cwd: next_command_cwd,
            hold_on_close: true,
            ..Default::default()
        };

        // Determine placement based on layout mode
        let placement = NewPanePlacement::Stacked(pane_id);

        // Update status: mark current command as completed
        // Use the pane_id from ActionComplete if the status is still Pending
        let pane_id_to_mark = state
            .execution
            .all_commands
            .get(state.execution.current_running_command_index)
            .and_then(|c| match c.get_status() {
                CommandStatus::Running(pid) => pid,
                CommandStatus::Pending => pane_id, // Use the pane from ActionComplete
                CommandStatus::Exited(_, pid) => pid, // Already marked, keep it
                CommandStatus::Interrupted(pid) => pid, // Already marked, keep it
            });

        state
            .execution
            .all_commands
            .get_mut(state.execution.current_running_command_index)
            .map(|c| c.set_status(CommandStatus::Exited(None, pane_id_to_mark)));
        state.execution.current_running_command_index = next_index;

        // Determine unblock_condition based on whether this is the last command
        let unblock_condition = if next_index < state.execution.all_commands.len() - 1 {
            // Not the last command - use the chain type's unblock condition
            next_chain_type.to_unblock_condition()
        } else {
            // Last command - use UnblockCondition::OnAnyExit
            Some(UnblockCondition::OnAnyExit)
        };

        let action = Action::NewBlockingPane {
            placement,
            command: Some(command),
            pane_name: Some(next_command_text.trim().to_string()),
            unblock_condition,
            near_current_pane: true,
        };

        // Pass the sequence ID in the context
        let mut context = BTreeMap::new();
        context.insert(
            "sequence_id".to_string(),
            state.execution.sequence_id.to_string(),
        );
        context.insert("command_text".to_string(), next_command_text.to_string());

        // Put the sequence back with updated index
        run_action(action, context);
    } else {
        // Sequence complete - mark the last command as Exited

        let pane_id_to_mark = state
            .execution
            .all_commands
            .get(state.execution.current_running_command_index)
            .and_then(|c| match c.get_status() {
                CommandStatus::Running(pid) => pid,
                CommandStatus::Pending => pane_id, // Use the pane from ActionComplete
                CommandStatus::Exited(_, pid) => pid, // Already marked, keep it
                CommandStatus::Interrupted(pid) => pid, // Already marked, keep it
            });

        state
            .execution
            .all_commands
            .get_mut(state.execution.current_running_command_index)
            .map(|c| c.set_status(CommandStatus::Exited(None, pane_id_to_mark)));
    }
    true
}

/// Adjust scroll offset to keep selected or running command visible
pub fn adjust_scroll_offset(sequence: &mut crate::state::State, rows: usize) {
    let max_visible = rows.saturating_sub(6);
    let total_commands = sequence.execution.all_commands.len();

    if total_commands <= max_visible {
        sequence.selection.scroll_offset = 0;
        return;
    }

    let focus_index = sequence
        .selection
        .current_selected_command_index
        .unwrap_or(sequence.execution.current_running_command_index);
    let half_visible = max_visible / 2;

    if focus_index < half_visible {
        sequence.selection.scroll_offset = 0;
    } else if focus_index >= total_commands.saturating_sub(half_visible) {
        sequence.selection.scroll_offset = total_commands.saturating_sub(max_visible);
    } else {
        sequence.selection.scroll_offset = focus_index.saturating_sub(half_visible);
    }

    sequence.selection.scroll_offset = sequence
        .selection
        .scroll_offset
        .min(total_commands.saturating_sub(max_visible));
}

pub fn is_pane_floating(pane_id: PaneId, pane_manifest: &PaneManifest) -> bool {
    pane_manifest
        .panes
        .iter()
        .flat_map(|(_, panes)| panes.iter())
        .find(|pane_info| match pane_id {
            PaneId::Terminal(id) => !pane_info.is_plugin && pane_info.id == id,
            PaneId::Plugin(id) => pane_info.is_plugin && pane_info.id == id,
        })
        .map(|pane_info| pane_info.is_floating)
        .unwrap_or(false)
}

fn is_running_sequence(context: &BTreeMap<String, String>, running_sequence_id: u64) -> bool {
    if let Some(context_sequence_id) = context.get("sequence_id") {
        if let Ok(context_id) = context_sequence_id.parse::<u64>() {
            if context_id != running_sequence_id {
                // This action is from a different sequence, ignore it
                return false;
            } else {
                return true;
            }
        } else {
            // Failed to parse sequence_id, ignore this action
            return false;
        }
    } else {
        return false;
    }
}

fn update_primary_and_original_pane_ids(state: &mut State, pane_manifest: &PaneManifest) {
    if let Some(primary_pane_id) = state.primary_pane_id {
        let mut primary_pane_found = false;

        // Check if the primary pane still exists
        for (_tab_index, panes) in &pane_manifest.panes {
            for pane_info in panes {
                let pane_matches = match primary_pane_id {
                    PaneId::Terminal(id) => !pane_info.is_plugin && pane_info.id == id,
                    PaneId::Plugin(id) => pane_info.is_plugin && pane_info.id == id,
                };

                if pane_matches {
                    primary_pane_found = true;
                    break;
                }
            }
            if primary_pane_found {
                break;
            }
        }

        // If primary pane was not found, select a new one
        if !primary_pane_found {
            state.primary_pane_id = None;
        }
    }
    if let Some(original_pane_id) = state.original_pane_id {
        let mut original_pane_found = false;

        // Check if the original pane still exists
        for (_tab_index, panes) in &pane_manifest.panes {
            for pane_info in panes {
                let pane_matches = match original_pane_id {
                    PaneId::Terminal(id) => !pane_info.is_plugin && pane_info.id == id,
                    PaneId::Plugin(id) => pane_info.is_plugin && pane_info.id == id,
                };

                if pane_matches {
                    original_pane_found = true;
                    break;
                }
            }
            if original_pane_found {
                break;
            }
        }

        // If original pane was not found, select a new one
        if !original_pane_found {
            state.original_pane_id = None;
        }
    }
}

pub fn select_primary_pane_from_history(
    pane_history: &[PaneId],
    pane_manifest: &PaneManifest,
    own_pane_id: Option<PaneId>,
) -> Option<PaneId> {
    pane_history
        .iter()
        .rev() // Most recent first
        .find(|&id| {
            // Skip self
            if Some(*id) == own_pane_id {
                return false;
            }

            // Skip floating panes
            !is_pane_floating(*id, pane_manifest)
        })
        .copied()
}

fn close_panes_and_return_to_shell(state: &mut State) -> bool {
    // Close all panes in the sequence before clearing it
    // Handle the first pane: replace it with the primary_pane_id_before_sequence if available
    if let Some(first_pane_id) = state.execution.all_commands.iter().find_map(|command| {
        // we look for the first command in the sequence that has a pane that's
        // actually open
        match command.get_status() {
            CommandStatus::Running(pane_id) => pane_id,
            CommandStatus::Exited(_, pane_id) => pane_id,
            CommandStatus::Interrupted(pane_id) => pane_id,
            _ => None,
        }
    }) {
        if let Some(original_pane_id) = state.original_pane_id {
            replace_pane_with_existing_pane(
                first_pane_id,
                original_pane_id,
                true, // suppress_replaced_pane (closes the first pane)
            );
        } else {
            // Fallback: if we don't have a saved primary pane, just close it
            close_pane_with_id(first_pane_id);
        }
    }

    // Close all other panes (starting from index 1)
    for (idx, command) in state.execution.all_commands.iter().skip(1).enumerate() {
        let pane_id = match command.get_status() {
            CommandStatus::Running(pane_id) => pane_id,
            CommandStatus::Exited(_, pane_id) => pane_id,
            CommandStatus::Interrupted(pane_id) => pane_id,
            _ => None,
        };

        if let Some(pane_id) = pane_id {
            close_pane_with_id(pane_id);
        } else {
            eprintln!(
                "Warning: Cannot close pane at index {}: pane_id is None",
                idx + 1
            );
        }
    }

    state.clear_all_commands();
    // Transition back to shell screen
    if let Some(plugin_id) = state.plugin_id {
        change_floating_pane_coordinates(
            plugin_id,
            Some(25),
            Some(25),
            Some(50),
            Some(50),
            false, // pinned (this should unpin it)
        );
    }
    update_title(state);
    true
}

fn change_floating_pane_coordinates(
    own_plugin_id: u32,
    x: Option<usize>,
    y: Option<usize>,
    width: Option<usize>,
    height: Option<usize>,
    should_be_pinned: bool,
) {
    let coordinates = FloatingPaneCoordinates::new(
        x.map(|x| format!("{}%", x)),
        y.map(|y| format!("{}%", y)),
        width.map(|width| format!("{}%", width)),
        height.map(|height| format!("{}%", height)),
        Some(should_be_pinned),
    );
    if let Some(coordinates) = coordinates {
        // TODO: better
        // show_self(true);
        change_floating_panes_coordinates(vec![(PaneId::Plugin(own_plugin_id), coordinates)]);
    }
}

fn update_spinner(state: &mut State) -> bool {
    // Advance spinner frame for RUNNING animation
    state.layout.spinner_frame = (state.layout.spinner_frame + 1) % 8;

    // Reset timer scheduled flag since this timer just fired
    state.layout.spinner_timer_scheduled = false;

    // If we have a running command, schedule next timer event
    let has_running = state
        .execution
        .all_commands
        .iter()
        .any(|command| matches!(command.get_status(), CommandStatus::Running(_)));
    if has_running && !state.layout.spinner_timer_scheduled {
        set_timeout(0.1);
        state.layout.spinner_timer_scheduled = true;
    }
    has_running
}

fn rerun_sequence(state: &mut State) {
    if !state.can_run_sequence() {
        return;
    }

    state.execution.all_commands.retain(|c| !c.is_empty());

    if state.execution.all_commands.is_empty() {
        return;
    }

    let selected_index = state
        .selection
        .current_selected_command_index
        .unwrap_or(0)
        .min(state.execution.all_commands.len().saturating_sub(1));

    let mut close_replaced_pane = true;

    // Extract the selected pane's ID BEFORE resetting statuses (needed for in-place replacement)
    let selected_pane_id_for_replacement = &state
        .execution
        .all_commands
        .get(selected_index)
        .and_then(|c| match c.get_status() {
            CommandStatus::Running(pane_id) => pane_id,
            CommandStatus::Exited(_, pane_id) => pane_id,
            CommandStatus::Interrupted(pane_id) => pane_id,
            _ => None,
        })
        .or_else(|| {
            if state.all_commands_are_pending() {
                close_replaced_pane = false; // we should never close the original pane_id
                state.original_pane_id
            } else {
                None
            }
        });

    // Close all panes AFTER the selected one (selected will be replaced in-place)
    for i in (selected_index + 1)..state.execution.all_commands.len() {
        // Extract pane_id from the command status
        let pane_id = state
            .execution
            .all_commands
            .get(i)
            .and_then(|c| match c.get_status() {
                CommandStatus::Running(pane_id) => pane_id,
                CommandStatus::Exited(_, pane_id) => pane_id,
                CommandStatus::Interrupted(pane_id) => pane_id,
                _ => None,
            });

        // Close the pane if we have a pane_id
        if let Some(pane_id) = pane_id {
            close_pane_with_id(pane_id);
        } else {
            eprintln!(
                "Warning: Cannot close pane for command at index {}: pane_id is None",
                i
            );
        }

        // Reset the command status to Pending
        state
            .execution
            .all_commands
            .get_mut(i)
            .map(|c| c.set_status(CommandStatus::Pending));
    }

    // Reset the selected pane's status to Pending (it will be replaced in-place)
    state
        .execution
        .all_commands
        .get_mut(selected_index)
        .map(|c| c.set_status(CommandStatus::Pending));

    // Set the current command index to the selected command
    state.execution.current_running_command_index = selected_index;

    // Execute the command at the selected index
    if let Some((command_text, chain_type, command_cwd)) = state
        .execution
        .all_commands
        .get(selected_index)
        .map(|c| (c.get_text(), c.get_chain_type(), c.get_cwd()))
    {
        let shell = state
            .shell
            .clone()
            .unwrap_or_else(|| PathBuf::from("/bin/bash"));

        let command = zellij_tile::prelude::actions::RunCommandAction {
            command: shell,
            args: vec!["-ic".to_string(), command_text.trim().to_string()],
            cwd: command_cwd,
            hold_on_close: true,
            ..Default::default()
        };

        // Determine placement - always replace the selected pane in-place if we have its ID
        let placement = if let Some(pane_id) = selected_pane_id_for_replacement {
            NewPanePlacement::InPlace {
                pane_id_to_replace: Some(*pane_id),
                close_replaced_pane,
            }
        } else {
            let pane_id_to_stack_under = state
                .execution
                .all_commands
                .iter()
                .find_map(|c| c.get_pane_id());
            NewPanePlacement::Stacked(pane_id_to_stack_under)
        };

        // Determine unblock_condition based on whether this is the last command
        let unblock_condition = if selected_index < state.execution.all_commands.len() - 1 {
            // Not the last command - use the chain type's unblock condition
            chain_type.to_unblock_condition()
        } else {
            // Last command - use UnblockCondition::OnAnyExit
            Some(UnblockCondition::OnAnyExit)
        };

        let action = Action::NewBlockingPane {
            placement,
            command: Some(command),
            pane_name: Some(command_text.trim().to_string()),
            unblock_condition,
            near_current_pane: true,
        };

        // Generate a new sequence ID for the restart
        state.execution.sequence_id += 1;

        // Reset the stopped flag so the sequence can run
        state.execution.is_running = true;
        state.layout.needs_reposition = true;
        state.execution.primary_pane_id_before_sequence = state.primary_pane_id;

        // Pass the sequence ID in the context
        let mut context = BTreeMap::new();
        context.insert(
            "sequence_id".to_string(),
            state.execution.sequence_id.to_string(),
        );
        context.insert("command_text".to_string(), command_text.to_string());
        run_action(action, context);
    }
}

fn interrupt_sequence(state: &mut State) {
    if state.execution.is_running {
        // Ctrl-C press when running - interrupt the running command
        for command in state.execution.all_commands.iter_mut() {
            if let CommandStatus::Running(pane_id) = command.get_status() {
                if let Some(pane_id) = pane_id {
                    send_sigkill_to_pane_id(pane_id);
                }
                command.set_status(CommandStatus::Interrupted(pane_id));
            }
        }
        state.execution.is_running = false;
    } else {
        eprintln!("Cannot interrupt a sequence that is not running.");
    }
}

pub fn update_cursor(state: &mut State) {
    // Calculate new cursor position
    let new_coords = calculate_cursor_position(state);

    // Only update if the position changed
    if new_coords != state.layout.cached_cursor_position {
        match new_coords {
            Some(coords) => show_cursor(Some(coords)),
            None => show_cursor(None),
        }
        state.layout.cached_cursor_position = new_coords;
    }
}

pub fn update_title(state: &mut State) {
    let Some(plugin_id) = state.plugin_id else {
        return;
    };

    let title = if state.all_commands_are_pending() {
        "Run one or more commands in sequence"
    } else {
        let has_running_commands = state
            .execution
            .all_commands
            .iter()
            .any(|c| matches!(c.get_status(), CommandStatus::Running(..)));
        let has_interrupted_commands = state
            .execution
            .all_commands
            .iter()
            .any(|c| matches!(c.get_status(), CommandStatus::Interrupted(..)));
        let all_commands_complete = state
            .execution
            .all_commands
            .iter()
            .all(|c| matches!(c.get_status(), CommandStatus::Exited(..)));
        if has_running_commands {
            let current = state.execution.current_running_command_index + 1;
            let total = state.execution.all_commands.len();
            return rename_plugin_pane(plugin_id, &format!("Running {}/{}", current, total));
        } else if has_interrupted_commands {
            "Sequence Interrupted"
        } else if all_commands_complete {
            "Sequence Complete"
        } else {
            "Stopped"
        }
    };
    rename_plugin_pane(plugin_id, title);
}

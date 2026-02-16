mod path_formatting;
mod state;
mod ui;

use crate::state::CommandStatus;
use crate::ui::components;
use crate::ui::fuzzy_complete;
use crate::ui::layout_calculations::calculate_viewport;
use crate::ui::text_input::InputAction;
use crate::ui::truncation::truncate_middle;
use state::{ChainType, SequenceMode, State};
use zellij_tile::prelude::*;

use std::collections::BTreeMap;
use std::path::PathBuf;

use unicode_width::UnicodeWidthStr;

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::ModeUpdate,
            EventType::Key,
            EventType::PermissionRequestResult,
            EventType::HostFolderChanged,
            EventType::RunCommandResult,
            EventType::CommandPaneExited,
            EventType::CommandPaneOpened,
            EventType::Timer,
            EventType::PastedText,
            EventType::TabUpdate,
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

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        if !pipe_message.is_private {
            return false;
        }

        let Some(payload) = pipe_message.payload else {
            return false;
        };

        let payload = payload.trim().to_string();
        if payload.is_empty() {
            return false;
        }

        // If already running, emit error
        if self.execution.is_running {
            if let PipeSource::Cli(pipe_id) = &pipe_message.source {
                #[cfg(target_family = "wasm")]
                cli_pipe_output(pipe_id, "error: sequence already running\n");
            }
            return false;
        }

        let cwd_override = pipe_message.args.get("cwd").map(PathBuf::from);
        self.load_from_pipe(&payload, cwd_override);
        self.update_running_state();
        self.execute_command_sequence();

        self.reposition_plugin();

        true
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
                state.total_viewport_columns = Some(tab_info.viewport_columns);
                state.total_viewport_rows = Some(tab_info.viewport_rows);
                update_cursor(state);
            }
            state.reposition_plugin()
        },
        Event::CommandPaneExited(terminal_pane_id, exit_code, _context) => {
            let should_render = handle_command_pane_exited(state, terminal_pane_id, exit_code);
            let repositioned = state.reposition_plugin();
            update_title(state);
            should_render || repositioned
        },
        Event::CommandPaneOpened(_terminal_pane_id, _context) => {
            // Schedule spinner on first pane open
            if !state.layout.spinner_timer_scheduled {
                set_timeout(0.1);
                state.layout.spinner_timer_scheduled = true;
            }
            update_title(state);
            true
        },
        _ => false,
    }
}

fn handle_command_pane_exited(
    state: &mut State,
    terminal_pane_id: u32,
    exit_code: Option<i32>,
) -> bool {
    let pane_id = PaneId::Terminal(terminal_pane_id);

    // Find which command this pane belongs to — stale events produce no match
    let Some(cmd_index) = state
        .execution
        .all_commands
        .iter()
        .position(|c| matches!(c.get_status(), CommandStatus::Running(Some(id)) if id == pane_id))
    else {
        return false;
    };

    state.execution.all_commands[cmd_index]
        .set_status(CommandStatus::Exited(exit_code, Some(pane_id)));

    if !state.execution.is_running {
        return true;
    }

    let chain_type = state.execution.all_commands[cmd_index].get_chain_type();
    let should_continue = match chain_type {
        ChainType::And => exit_code.unwrap_or(0) == 0,
        ChainType::Or => exit_code.unwrap_or(0) != 0,
        ChainType::Then => true,
        ChainType::None => false,
    };

    let next_index = cmd_index + 1;
    if should_continue && next_index < state.execution.all_commands.len() {
        launch_command_at_index(state, next_index, Some(pane_id), false);
    } else {
        state.execution.is_running = false;
    }
    true
}

fn launch_command_at_index(
    state: &mut State,
    index: usize,
    replace_pane_id: Option<PaneId>,
    force_visible: bool,
) {
    let cmd = &state.execution.all_commands[index];
    let command = CommandToRun {
        path: state.shell.clone().unwrap_or_else(|| PathBuf::from("/bin/bash")),
        args: vec!["-ic".to_string(), cmd.get_text().trim().to_string()],
        cwd: cmd.get_cwd(),
    };
    let user_is_watching = replace_pane_id.is_some()
        && replace_pane_id == state.execution.displayed_pane_id;
    let new_pane_id = if state.mode == SequenceMode::Spread {
        open_command_pane_near_plugin(command, BTreeMap::new())
    } else if user_is_watching {
        let prev_pane_id = replace_pane_id.unwrap();
        let pane_id =
            open_command_pane_in_place_of_pane_id(prev_pane_id, command, false, BTreeMap::new());
        if let Some(pane_id) = pane_id {
            state.execution.displayed_pane_id = Some(pane_id);
        }
        pane_id
    } else if force_visible {
        let pane_id = open_command_pane_near_plugin(command, BTreeMap::new());
        if let Some(pane_id) = pane_id {
            state.execution.displayed_pane_id = Some(pane_id);
        }
        pane_id
    } else {
        // User navigated away — open in background, invisible until they navigate to it
        // Kept for future configurability: open_command_pane_near_plugin(command, BTreeMap::new())
        open_command_pane_background(command, BTreeMap::new())
    };
    if let Some(pane_id) = new_pane_id {
        state.execution.all_commands[index].set_status(CommandStatus::Running(Some(pane_id)));
        state.execution.current_running_command_index = index;
        state.selection.current_selected_command_index = Some(index);
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

    if state.editing.editing_input.is_none()
        && key.has_no_modifiers()
        && matches!(key.bare_key, BareKey::Tab)
        && !state.all_commands_are_pending()
    {
        toggle_spread_mode(state);
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

fn close_panes_and_return_to_shell(state: &mut State) -> bool {
    for command in &state.execution.all_commands {
        let pane_id = match command.get_status() {
            CommandStatus::Running(p) | CommandStatus::Exited(_, p) | CommandStatus::Interrupted(p) => p,
            _ => None,
        };
        if let Some(pane_id) = pane_id {
            close_pane_with_id(pane_id);
        }
    }
    state.clear_all_commands();
    // Reposition plugin back to center
    if let Some(plugin_id) = state.plugin_id {
        change_floating_pane_coordinates(plugin_id, Some(25), Some(25), Some(50), Some(50), false);
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
        Some(false),
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
    if has_running {
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


    // Close and reset commands after selected
    for i in (selected_index + 1)..state.execution.all_commands.len() {
        if let Some(pane_id) = state.execution.all_commands[i].get_pane_id() {
            close_pane_with_id(pane_id);
        }
        state.execution.all_commands[i].set_status(CommandStatus::Pending);
    }

    state.execution.is_running = true;
    state.execution.current_running_command_index = selected_index;
    match state.execution.all_commands[selected_index].get_pane_id() {
        Some(PaneId::Terminal(id)) => {
            let pane_id = PaneId::Terminal(id);

            if state.mode == SequenceMode::SinglePane {
                if let Some(displayed) = state.execution.displayed_pane_id {
                    if displayed != pane_id {
                        replace_pane_with_existing_pane(displayed, pane_id, true);
                    }
                }
                state.execution.displayed_pane_id = Some(pane_id);
            }
            state.execution.all_commands[selected_index]
                .set_status(CommandStatus::Running(Some(pane_id)));
            state.selection.current_selected_command_index = Some(selected_index);
            rerun_command_pane(id);
        },
        _ => {
            let replace_pane_id = state.execution.displayed_pane_id;
            launch_command_at_index(state, selected_index, replace_pane_id, true);
        },
    }
}

fn toggle_spread_mode(state: &mut State) {
    match state.mode {
        SequenceMode::SinglePane => enter_spread_mode(state),
        SequenceMode::Spread => exit_spread_mode(state),
    }
}

fn enter_spread_mode(state: &mut State) {
    state.mode = SequenceMode::Spread;

    let first_pane_id = state.execution.all_commands.first().and_then(|c| c.get_pane_id());
    let Some(first_pane_id) = first_pane_id else {
        return;
    };

    if let Some(displayed) = state.execution.displayed_pane_id {
        if displayed != first_pane_id {
            replace_pane_with_existing_pane(displayed, first_pane_id, true);
        }
    }
    state.execution.displayed_pane_id = Some(first_pane_id);

    for cmd in state.execution.all_commands.iter().skip(1) {
        if let Some(pane_id) = cmd.get_pane_id() {
            show_pane_with_id(pane_id, false, false);
        }
    }
}

fn exit_spread_mode(state: &mut State) {
    state.mode = SequenceMode::SinglePane;

    let first_pane_id = state.execution.all_commands.first().and_then(|c| c.get_pane_id());
    let selected_pane_id = state
        .selection
        .current_selected_command_index
        .and_then(|i| state.execution.all_commands.get(i))
        .and_then(|c| c.get_pane_id())
        .or(first_pane_id);

    let Some(selected_pane_id) = selected_pane_id else {
        return;
    };

    if let Some(first_pane_id) = first_pane_id {
        if first_pane_id != selected_pane_id {
            replace_pane_with_existing_pane(first_pane_id, selected_pane_id, true);
        }
    }

    for cmd in &state.execution.all_commands {
        if let Some(pane_id) = cmd.get_pane_id() {
            if pane_id != selected_pane_id {
                hide_pane_with_id(pane_id);
            }
        }
    }

    state.execution.displayed_pane_id = Some(selected_pane_id);
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

mod path_formatting;
mod state;
mod ui;

use crate::state::CommandStatus;
use crate::ui::components;
use crate::ui::layout_calculations::calculate_viewport;
use state::{load_from_editor_file, ChainType, CommandEntry, SequenceMode, State};
use zellij_tile::prelude::*;

use std::collections::BTreeMap;
use std::path::PathBuf;

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::ModeUpdate,
            EventType::Key,
            EventType::PermissionRequestResult,
            EventType::HostFolderChanged,
            EventType::CommandPaneExited,
            EventType::CommandPaneOpened,
            EventType::EditPaneExited,
            EventType::Timer,
            EventType::PastedText,
            EventType::TabUpdate,
        ]);

        // Store our own plugin ID and client ID
        let plugin_ids = get_plugin_ids();
        self.plugin_id = Some(plugin_ids.plugin_id);
        self.client_id = Some(plugin_ids.client_id);
        self.cwd = Some(plugin_ids.initial_cwd);
        rename_plugin_pane(plugin_ids.plugin_id, "Sequence");
        update_title(self);
    }

    fn update(&mut self, event: Event) -> bool {
        handle_event(self, event)
    }

    #[cfg(target_family = "wasm")]
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
        show_self(true);

        true
    }

    fn render(&mut self, rows: usize, cols: usize) {
        // Store dimensions for use in calculations
        self.own_rows = Some(rows);
        self.own_columns = Some(cols);

        // Main screen: plugin just opened, no commands entered yet
        if self.all_commands_are_pending() && !self.execution.can_run_sequence() {
            for (text, x, y) in components::render_intro_hint(rows, cols) {
                print_text_with_coordinates(text, x, y, None, None);
            }
            return;
        }

        let is_staging = self.all_commands_are_pending() && self.execution.can_run_sequence();

        let max_visible_rows = rows.saturating_sub(5);

        let (offset, visible_count, hidden_above, hidden_below) = calculate_viewport(
            self.execution.all_commands.len(),
            max_visible_rows,
            self.selection.current_selected_command_index,
            self.execution.current_running_command_index,
        );

        // For the staging screen, center the content both horizontally and vertically.
        // Content height: 1 header row + visible_count command rows + 1 gap + 1 help row = visible_count + 3
        let (base_x, base_y, table_max_width) = if is_staging {
            let longest_cwd = self.execution.longest_cwd_display(&self.cwd);
            let longest_cmd = self
                .execution
                .all_commands
                .iter()
                .map(|c| c.get_text().chars().count())
                .max()
                .unwrap_or(1);
            let (max_chain_w, max_status_w) = components::calculate_max_widths(
                &self.execution.all_commands,
                self.layout.spinner_frame,
            );
            let content_width = components::calculate_longest_line(
                &longest_cwd,
                longest_cmd,
                max_chain_w,
                max_status_w,
            )
            .min(cols);
            // header + commands + gap + help + gap + ribbon = visible_count + 5
            let content_height = visible_count + 5;
            let bx = cols.saturating_sub(content_width) / 2;
            let by = rows.saturating_sub(content_height) / 2;
            (bx, by, Some(content_width))
        } else {
            (1, 0, self.own_columns.map(|o| o.saturating_sub(1)))
        };

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

        print_table_with_coordinates(table, base_x, base_y, table_max_width, None);

        let help_y = base_y + visible_count + 2;
        let (first_help, first_help_len, second_help) =
            components::render_help_lines(self, Some(cols));
        let help_x = if is_staging {
            cols.saturating_sub(first_help_len) / 2
        } else {
            base_x
        };
        print_text_with_coordinates(first_help, help_x, help_y, None, None);

        if let Some((second_help_text, second_help_len)) = second_help {
            let second_help_x = if is_staging {
                cols.saturating_sub(second_help_len) / 2
            } else {
                base_x
            };
            print_text_with_coordinates(second_help_text, second_help_x, help_y + 1, None, None);
        }

        // Mode ribbon: "<Tab> [Single Pane] [Spread]" — only while running
        if self.execution.is_running {
            let ribbon_y = help_y + 2;
            print_text_with_coordinates(
                Text::new("<Tab>").color_all(3),
                base_x,
                ribbon_y,
                None,
                None,
            );
            let ribbon_x = base_x + 6; // "<Tab>" (5) + space (1)
            let (single_ribbon, spread_ribbon) = if self.mode == SequenceMode::SinglePane {
                (Text::new("Single Pane").selected(), Text::new("Spread"))
            } else {
                (Text::new("Single Pane"), Text::new("Spread").selected())
            };
            print_ribbon_with_coordinates(single_ribbon, ribbon_x, ribbon_y, Some(15), None);
            print_ribbon_with_coordinates(spread_ribbon, ribbon_x + 15, ribbon_y, Some(10), None);
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
            let should_render = handle_key_event(state, key);
            let repositioned = state.reposition_plugin();
            if repositioned {
                // we only want to render once we have repositioned, we will do this in TabUpdate
                return false;
            }
            show_cursor(None);
            should_render
        },
        Event::Timer(_elapsed) => {
            let should_render = update_spinner(state);
            update_title(state);
            should_render
        },
        Event::PastedText(pasted_text) => {
            if state.execution.is_running {
                return false;
            }
            let payload = pasted_text.trim().to_string();
            if payload.is_empty() {
                return false;
            }
            state.load_from_pipe(&payload, None);
            let repositioned = state.reposition_plugin();
            !repositioned
        },
        Event::TabUpdate(tab_infos) => {
            if let Some(tab_info) = tab_infos.iter().find(|t| t.active) {
                state.total_viewport_columns = Some(tab_info.viewport_columns);
                state.total_viewport_rows = Some(tab_info.viewport_rows);
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
        Event::EditPaneExited(terminal_pane_id, _exit_code, _context) => {
            let should_render = handle_editor_pane_exited(state, terminal_pane_id);
            let repositioned = state.reposition_plugin();
            should_render || repositioned
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
        state.selection.current_selected_command_index = None;
    }
    true
}

fn handle_editor_pane_exited(state: &mut State, terminal_pane_id: u32) -> bool {
    let pane_id = PaneId::Terminal(terminal_pane_id);

    // Ignore events from unrelated panes
    if state.editor_pane_id != Some(pane_id) {
        return false;
    }

    // Read the temp file contents
    let contents = if let Some(ref path) = state.editor_temp_file {
        std::fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };

    // Delete the temp file
    if let Some(ref path) = state.editor_temp_file {
        let _ = std::fs::remove_file(path);
    }

    // Clear editor state
    state.editor_pane_id = None;
    state.editor_temp_file = None;

    // Parse file into commands
    let commands = load_from_editor_file(&contents, state.cwd.clone());

    if !commands.is_empty() {
        state.execution.all_commands = commands;
        state.execution.current_running_command_index = 0;
        state.selection.current_selected_command_index = None;
        state.execution.is_running = false;
    } else {
        // Reset to empty pending state
        state.execution.all_commands = vec![CommandEntry::new("", state.cwd.clone())];
        state.execution.current_running_command_index = 0;
        state.selection.current_selected_command_index = None;
        state.execution.is_running = false;
    }

    show_self(true);

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
        let (tab_id, pane_id) = open_command_pane_in_new_tab(command, BTreeMap::new());
        if let Some(pane_id) = pane_id {
            state.execution.displayed_pane_id = Some(pane_id);
        }
        if let (Some(tab_id), Some(plugin_id)) = (tab_id, state.plugin_id) {
            state.sequence_tab_id = Some(tab_id);
            break_panes_to_tab_with_id(&[PaneId::Plugin(plugin_id)], tab_id, true);
            focus_pane_with_id(PaneId::Plugin(plugin_id), false, false);
        }
        pane_id
    } else {
        // User navigated away — open in background, invisible until they navigate to it
        open_command_pane_background(command, BTreeMap::new())
    };
    if let Some(pane_id) = new_pane_id {
        state.execution.all_commands[index].set_status(CommandStatus::Running(Some(pane_id)));
        state.execution.current_running_command_index = index;
        state.selection.current_selected_command_index = Some(index);
        state.show_selected_pane();
    }
}

fn handle_key_event(state: &mut State, key: KeyWithModifier) -> bool {
    // Ctrl+Space — copy to clipboard
    if key.has_modifiers(&[KeyModifier::Ctrl]) && matches!(key.bare_key, BareKey::Char(' ')) {
        state.copy_to_clipboard();
        return false;
    }

    // Ctrl+W — close tab if sequence has run; close plugin if staging; no-op if running
    if key.has_modifiers(&[KeyModifier::Ctrl]) && matches!(key.bare_key, BareKey::Char('w')) {
        if state.all_commands_are_pending() {
            if let Some(plugin_id) = state.plugin_id {
                close_pane_with_id(PaneId::Plugin(plugin_id));
            }
        } else if !state.execution.is_running {
            close_panes_and_return_to_shell(state);
        }
        return true;
    }

    // Ctrl+C — interrupt if running; clear commands if staging; close plugin if empty
    if key.has_modifiers(&[KeyModifier::Ctrl]) && matches!(key.bare_key, BareKey::Char('c')) {
        if state.execution.is_running {
            interrupt_sequence(state);
            return true;
        } else if state.all_commands_are_pending() {
            if state.execution.all_commands.iter().any(|c| !c.is_empty()) {
                state.clear_all_commands();
            } else if let Some(plugin_id) = state.plugin_id {
                close_pane_with_id(PaneId::Plugin(plugin_id));
            }
            return true;
        }
    }

    let is_staging = state.all_commands_are_pending() && state.execution.can_run_sequence();

    if matches!(key.bare_key, BareKey::Up)
        && !key.has_modifiers(&[KeyModifier::Ctrl])
        && !key.has_modifiers(&[KeyModifier::Alt])
    {
        if !is_staging {
            state.move_selection_up();
            return true;
        }
        return false;
    }

    if matches!(key.bare_key, BareKey::Down)
        && !key.has_modifiers(&[KeyModifier::Ctrl])
        && !key.has_modifiers(&[KeyModifier::Alt])
    {
        if !is_staging {
            state.move_selection_down();
            return true;
        }
        return false;
    }

    // e — open external editor (when not running)
    if !state.execution.is_running
        && key.has_no_modifiers()
        && matches!(key.bare_key, BareKey::Char('e'))
    {
        state.open_editor();
        return true;
    }

    // Enter — rerun sequence
    if key.has_no_modifiers() && matches!(key.bare_key, BareKey::Enter) {
        rerun_sequence(state);
        return true;
    }

    // Tab — toggle spread mode (when sequence has run)
    if key.has_no_modifiers()
        && matches!(key.bare_key, BareKey::Tab)
        && !state.all_commands_are_pending()
    {
        toggle_spread_mode(state);
        return true;
    }

    false
}

fn close_panes_and_return_to_shell(state: &mut State) -> bool {
    if let Some(tab_id) = state.sequence_tab_id {
        close_tab_with_id(tab_id as u64);
    }
    true
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
    ensure_spinner_running(state);
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
    state.show_selected_pane();
}

fn exit_spread_mode(state: &mut State) {
    // Remove all highlights before leaving spread mode
    let all_pane_ids: Vec<PaneId> = state
        .execution
        .all_commands
        .iter()
        .filter_map(|c| c.get_pane_id())
        .collect();
    if !all_pane_ids.is_empty() {
        highlight_and_unhighlight_panes(vec![], all_pane_ids);
    }
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
        for command in state.execution.all_commands.iter_mut() {
            if let CommandStatus::Running(pane_id) = command.get_status() {
                if let Some(pane_id) = pane_id {
                    send_sigkill_to_pane_id(pane_id);
                }
                command.set_status(CommandStatus::Interrupted(pane_id));
            }
        }
        state.execution.is_running = false;
        state.selection.current_selected_command_index = None;
    } else {
        eprintln!("Cannot interrupt a sequence that is not running.");
    }
}

pub fn update_title(state: &mut State) {
    let Some(tab_id) = state.sequence_tab_id else {
        return;
    };
    let tab_id_u64 = tab_id as u64;

    // All pending and not yet running
    if state.all_commands_are_pending() && !state.execution.is_running {
        rename_tab_with_id(tab_id_u64, "Run one or more commands in sequence");
        return;
    }

    // Currently running: "<cmd> <n>/<total> <spinner>"
    let has_running = state
        .execution
        .all_commands
        .iter()
        .any(|c| matches!(c.get_status(), CommandStatus::Running(..)));
    if has_running {
        let idx = state.execution.current_running_command_index;
        let total = state.execution.all_commands.len();
        let cmd_text = state
            .execution
            .all_commands
            .get(idx)
            .map(|c| c.get_text())
            .unwrap_or_default();
        let spinner = components::get_spinner_frame(state.layout.spinner_frame);
        rename_tab_with_id(
            tab_id_u64,
            &format!("{} {}/{} {}", cmd_text, idx + 1, total, spinner),
        );
        return;
    }

    // Interrupted: "<cmd> [INTERRUPTED]"
    if let Some(cmd) = state
        .execution
        .all_commands
        .iter()
        .find(|c| matches!(c.get_status(), CommandStatus::Interrupted(_)))
    {
        let status_str =
            components::format_status_text(&cmd.get_status(), state.layout.spinner_frame);
        rename_tab_with_id(tab_id_u64, &format!("{} {}", cmd.get_text(), status_str));
        return;
    }

    // All exited
    let all_exited = state
        .execution
        .all_commands
        .iter()
        .all(|c| matches!(c.get_status(), CommandStatus::Exited(..)));
    if all_exited {
        let all_success = state
            .execution
            .all_commands
            .iter()
            .all(|c| matches!(c.get_status(), CommandStatus::Exited(Some(0), _)));
        if all_success {
            rename_tab_with_id(tab_id_u64, "Sequence Complete");
        } else {
            // Show first command with non-zero or unknown exit
            if let Some(cmd) = state
                .execution
                .all_commands
                .iter()
                .find(|c| !matches!(c.get_status(), CommandStatus::Exited(Some(0), _)))
            {
                let status_str =
                    components::format_status_text(&cmd.get_status(), state.layout.spinner_frame);
                rename_tab_with_id(tab_id_u64, &format!("{} {}", cmd.get_text(), status_str));
            } else {
                rename_tab_with_id(tab_id_u64, "Sequence Complete");
            }
        }
        return;
    }

    // Partial (chain condition failed): show last exited command with its status
    if let Some(cmd) = state
        .execution
        .all_commands
        .iter()
        .rev()
        .find(|c| matches!(c.get_status(), CommandStatus::Exited(..)))
    {
        let status_str =
            components::format_status_text(&cmd.get_status(), state.layout.spinner_frame);
        rename_tab_with_id(tab_id_u64, &format!("{} {}", cmd.get_text(), status_str));
    } else {
        rename_tab_with_id(tab_id_u64, "Stopped");
    }
}

fn ensure_spinner_running(state: &mut State) {
    if !state.layout.spinner_timer_scheduled {
        set_timeout(0.1);
        state.layout.spinner_timer_scheduled = true;
    }
}

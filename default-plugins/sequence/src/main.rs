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
        launch_command_at_index(self, 0, None, true);

        self.reposition_plugin();

        true
    }

    fn render(&mut self, rows: usize, cols: usize) {
        // Store dimensions for use in calculations
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
            state.update_running_state();
            launch_command_at_index(state, 0, None, true);
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
        state.selection.current_selected_command_index = Some(0);
        state.update_running_state();
        launch_command_at_index(state, 0, None, true);
    } else {
        // Reset to empty pending state
        state.execution.all_commands = vec![CommandEntry::new("", state.cwd.clone())];
        state.execution.current_running_command_index = 0;
        state.selection.current_selected_command_index = Some(0);
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

    // Ctrl+W — close all panes and return to shell
    if key.has_modifiers(&[KeyModifier::Ctrl]) && matches!(key.bare_key, BareKey::Char('w')) {
        if !state.execution.is_running && !state.all_commands_are_pending() {
            close_panes_and_return_to_shell(state);
        }
        return true;
    }

    // Ctrl+C — interrupt sequence when running
    if key.has_modifiers(&[KeyModifier::Ctrl]) && matches!(key.bare_key, BareKey::Char('c')) {
        if state.execution.is_running {
            interrupt_sequence(state);
            return true;
        }
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
    } else {
        eprintln!("Cannot interrupt a sequence that is not running.");
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

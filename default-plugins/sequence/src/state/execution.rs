use crate::path_formatting::format_cwd;
use crate::state::{ChainType, CommandEntry, CommandStatus};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;
use zellij_tile::prelude::*;

pub struct Execution {
    pub all_commands: Vec<CommandEntry>,
    pub current_running_command_index: usize,
    pub is_running: bool,
    pub sequence_id: u64,
    pub primary_pane_id_before_sequence: Option<PaneId>,
}

impl Execution {
    pub fn new() -> Self {
        Self {
            all_commands: vec![CommandEntry::default()],
            current_running_command_index: 0,
            is_running: false,
            sequence_id: 0,
            primary_pane_id_before_sequence: None,
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

    pub fn update_pane_id_for_command(&mut self, pane_id: PaneId, command_text: &str) {
        for command in self.all_commands.iter_mut() {
            if let CommandStatus::Pending | CommandStatus::Running(None) = command.get_status() {
                let cmd_text = command.get_text();
                if cmd_text == command_text {
                    command.set_status(CommandStatus::Running(Some(pane_id)));
                    break;
                }
            }
        }
    }

    pub fn update_exited_command_statuses(&mut self, pane_manifest: &PaneManifest) -> bool {
        let mut updated = false;
        for command in self.all_commands.iter_mut() {
            let status = command.get_status();
            let pane_id_opt = match &status {
                CommandStatus::Running(pid) => *pid,
                CommandStatus::Exited(_, pid) => *pid,
                CommandStatus::Interrupted(pid) => *pid,
                _ => None,
            };

            if let Some(pane_id) = pane_id_opt {
                let mut pane_found = false;
                for (_tab_index, panes) in &pane_manifest.panes {
                    for pane_info in panes {
                        let pane_matches = match pane_id {
                            PaneId::Terminal(id) => !pane_info.is_plugin && pane_info.id == id,
                            PaneId::Plugin(id) => pane_info.is_plugin && pane_info.id == id,
                        };

                        if pane_matches {
                            pane_found = true;
                            if pane_info.exited {
                                command.set_status(CommandStatus::Exited(
                                    pane_info.exit_status,
                                    Some(pane_id),
                                ));
                                updated = true;
                            }
                            break;
                        }
                    }
                    if pane_found {
                        break;
                    }
                }

                if !pane_found && command.start_time.elapsed() > Duration::from_millis(400) {
                    match command.get_status() {
                        CommandStatus::Running(_) => {
                            eprintln!(
                                "Pane {:?} was closed while running, setting pane_id to None",
                                pane_id
                            );
                            command.set_status(CommandStatus::Running(None));
                            updated = true;
                        },
                        CommandStatus::Exited(exit_code, _) => {
                            eprintln!(
                                "Pane {:?} was closed after exiting, setting pane_id to None",
                                pane_id
                            );
                            command.set_status(CommandStatus::Exited(exit_code, None));
                            updated = true;
                        },
                        CommandStatus::Interrupted(_) => {
                            eprintln!("Pane {:?} was closed after being interrupted, setting pane_id to None", pane_id);
                            command.set_status(CommandStatus::Interrupted(None));
                            updated = true;
                        },
                        _ => {},
                    }
                }
            }
        }
        updated
    }

    pub fn update_sequence_stopped_state(&mut self) -> bool {
        let mut needs_rerender = false;
        if self.is_running {
            let current_idx = self.current_running_command_index;
            if let Some(command) = self.all_commands.get_mut(current_idx) {
                if let CommandStatus::Exited(exit_code, _) = command.get_status() {
                    let should_stop;
                    if current_idx >= self.all_commands.len().saturating_sub(1) {
                        should_stop = true;
                    } else {
                        if let Some(chain_type) = &self
                            .all_commands
                            .get(current_idx)
                            .map(|c| c.get_chain_type())
                        {
                            match chain_type {
                                ChainType::And => {
                                    should_stop = exit_code.unwrap_or(0) != 0;
                                },
                                ChainType::Or => {
                                    should_stop = exit_code.unwrap_or(0) == 0;
                                },
                                ChainType::Then => {
                                    should_stop = false;
                                },
                                ChainType::None => {
                                    should_stop = true;
                                },
                            }
                        } else {
                            should_stop = true;
                        }
                    };
                    if should_stop {
                        self.is_running = false;
                    }
                }
            }
            needs_rerender = true;
        }
        needs_rerender
    }

    pub fn execute_command_sequence(
        &mut self,
        shell: &Option<PathBuf>,
        global_cwd: &Option<PathBuf>,
        primary_pane_id: Option<PaneId>,
    ) {
        use zellij_tile::prelude::actions::{Action, RunCommandAction};

        self.all_commands.retain(|c| !c.is_empty());

        let Some(first_active_sequence_command) = self.get_first_command() else {
            return;
        };

        let shell = shell.clone().unwrap_or_else(|| PathBuf::from("/bin/bash"));

        let first_command = first_active_sequence_command.get_text();
        let first_chain_type = first_active_sequence_command.get_chain_type();
        let command_cwd = first_active_sequence_command
            .get_cwd()
            .or_else(|| global_cwd.clone());

        let command = RunCommandAction {
            command: shell.clone(),
            args: vec!["-ic".to_string(), first_command.trim().to_string()],
            cwd: command_cwd,
            hold_on_close: true,
            ..Default::default()
        };

        let placement = NewPanePlacement::InPlace {
            pane_id_to_replace: primary_pane_id,
            close_replaced_pane: false,
        };

        let action = Action::NewBlockingPane {
            placement,
            command: Some(command),
            pane_name: Some(first_command.trim().to_string()),
            unblock_condition: first_chain_type.to_unblock_condition(),
            near_current_pane: true,
        };

        self.sequence_id += 1;

        let mut context = BTreeMap::new();
        context.insert("sequence_id".to_string(), self.sequence_id.to_string());
        run_action(action, context);
        self.set_command_status(0, CommandStatus::Running(None));
    }
}

impl Default for Execution {
    fn default() -> Self {
        Self::new()
    }
}

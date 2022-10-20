use crate::os_input_output::SpawnTerminalError;
use crate::terminal_bytes::TerminalBytes;
use crate::{
    panes::PaneId,
    screen::ScreenInstruction,
    thread_bus::{Bus, ThreadSenders},
    wasm_vm::PluginInstruction,
    ClientId, ServerInstruction,
};
use async_std::task::{self, JoinHandle};
use std::{collections::HashMap, env, os::unix::io::RawFd, path::PathBuf};
use zellij_utils::nix::unistd::Pid;
use zellij_utils::{
    async_std,
    errors::{ContextType, PtyContext},
    input::{
        command::{RunCommand, TerminalAction},
        layout::{Layout, PaneLayout, Run},
    },
};

pub type VteBytes = Vec<u8>;
pub type TabIndex = u32;

#[derive(Clone, Copy, Debug)]
pub enum ClientOrTabIndex {
    ClientId(ClientId),
    TabIndex(usize),
}

/// Instructions related to PTYs (pseudoterminals).
#[derive(Clone, Debug)]
pub(crate) enum PtyInstruction {
    SpawnTerminal(
        Option<TerminalAction>,
        Option<bool>,
        Option<String>,
        ClientOrTabIndex,
    ), // bool (if Some) is
    // should_float, String is an optional pane name
    OpenInPlaceEditor(PathBuf, Option<usize>, ClientId), // Option<usize> is the optional line number
    SpawnTerminalVertically(Option<TerminalAction>, Option<String>, ClientId), // String is an
    // optional pane
    // name
    SpawnTerminalHorizontally(Option<TerminalAction>, Option<String>, ClientId), // String is an
    // optional pane
    // name
    UpdateActivePane(Option<PaneId>, ClientId),
    GoToTab(TabIndex, ClientId),
    NewTab(
        Option<TerminalAction>,
        Option<PaneLayout>,
        Option<String>,
        ClientId,
    ), // the String is the tab name
    ClosePane(PaneId),
    CloseTab(Vec<PaneId>),
    ReRunCommandInPane(PaneId, RunCommand),
    Exit,
}

impl From<&PtyInstruction> for PtyContext {
    fn from(pty_instruction: &PtyInstruction) -> Self {
        match *pty_instruction {
            PtyInstruction::SpawnTerminal(..) => PtyContext::SpawnTerminal,
            PtyInstruction::OpenInPlaceEditor(..) => PtyContext::OpenInPlaceEditor,
            PtyInstruction::SpawnTerminalVertically(..) => PtyContext::SpawnTerminalVertically,
            PtyInstruction::SpawnTerminalHorizontally(..) => PtyContext::SpawnTerminalHorizontally,
            PtyInstruction::UpdateActivePane(..) => PtyContext::UpdateActivePane,
            PtyInstruction::GoToTab(..) => PtyContext::GoToTab,
            PtyInstruction::ClosePane(_) => PtyContext::ClosePane,
            PtyInstruction::CloseTab(_) => PtyContext::CloseTab,
            PtyInstruction::NewTab(..) => PtyContext::NewTab,
            PtyInstruction::ReRunCommandInPane(..) => PtyContext::ReRunCommandInPane,
            PtyInstruction::Exit => PtyContext::Exit,
        }
    }
}

pub(crate) struct Pty {
    pub active_panes: HashMap<ClientId, PaneId>,
    pub bus: Bus<PtyInstruction>,
    pub id_to_child_pid: HashMap<u32, RawFd>, // terminal_id => child raw fd
    debug_to_file: bool,
    task_handles: HashMap<u32, JoinHandle<()>>, // terminal_id to join-handle
    default_editor: Option<PathBuf>,
}

pub(crate) fn pty_thread_main(mut pty: Pty, layout: Box<Layout>) {
    loop {
        let (event, mut err_ctx) = pty.bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Pty((&event).into()));
        match event {
            PtyInstruction::SpawnTerminal(
                terminal_action,
                should_float,
                name,
                client_or_tab_index,
            ) => {
                let (hold_on_close, run_command, pane_title) = match &terminal_action {
                    Some(TerminalAction::RunCommand(run_command)) => (
                        run_command.hold_on_close,
                        Some(run_command.clone()),
                        Some(name.unwrap_or_else(|| run_command.to_string())),
                    ),
                    _ => (false, None, name),
                };
                match pty.spawn_terminal(terminal_action, client_or_tab_index) {
                    Ok(pid) => {
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::NewPane(
                                PaneId::Terminal(pid),
                                pane_title,
                                should_float,
                                client_or_tab_index,
                            ))
                            .unwrap();
                    },
                    Err(SpawnTerminalError::CommandNotFound(pid)) => {
                        if hold_on_close {
                            pty.bus
                                .senders
                                .send_to_screen(ScreenInstruction::NewPane(
                                    PaneId::Terminal(pid),
                                    pane_title,
                                    should_float,
                                    client_or_tab_index,
                                ))
                                .unwrap();
                            if let Some(run_command) = run_command {
                                send_command_not_found_to_screen(
                                    pty.bus.senders.clone(),
                                    pid,
                                    run_command.clone(),
                                );
                            }
                        } else {
                            log::error!("Failed to spawn terminal: command not found");
                            pty.close_pane(PaneId::Terminal(pid));
                        }
                    },
                    Err(e) => {
                        log::error!("Failed to spawn terminal: {}", e);
                    },
                }
            },
            PtyInstruction::OpenInPlaceEditor(temp_file, line_number, client_id) => {
                match pty.spawn_terminal(
                    Some(TerminalAction::OpenFile(temp_file, line_number)),
                    ClientOrTabIndex::ClientId(client_id),
                ) {
                    Ok(pid) => {
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::OpenInPlaceEditor(
                                PaneId::Terminal(pid),
                                client_id,
                            ))
                            .unwrap();
                    },
                    Err(e) => {
                        log::error!("Failed to open editor: {}", e);
                    },
                }
            },
            PtyInstruction::SpawnTerminalVertically(terminal_action, name, client_id) => {
                let (hold_on_close, run_command, pane_title) = match &terminal_action {
                    Some(TerminalAction::RunCommand(run_command)) => (
                        run_command.hold_on_close,
                        Some(run_command.clone()),
                        Some(name.unwrap_or_else(|| run_command.to_string())),
                    ),
                    _ => (false, None, name),
                };
                match pty.spawn_terminal(terminal_action, ClientOrTabIndex::ClientId(client_id)) {
                    Ok(pid) => {
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::VerticalSplit(
                                PaneId::Terminal(pid),
                                pane_title,
                                client_id,
                            ))
                            .unwrap();
                    },
                    Err(SpawnTerminalError::CommandNotFound(pid)) => {
                        if hold_on_close {
                            pty.bus
                                .senders
                                .send_to_screen(ScreenInstruction::VerticalSplit(
                                    PaneId::Terminal(pid),
                                    pane_title,
                                    client_id,
                                ))
                                .unwrap();
                            if let Some(run_command) = run_command {
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::PtyBytes(
                                        pid,
                                        format!(
                                            "Command not found: {}",
                                            run_command.command.display()
                                        )
                                        .as_bytes()
                                        .to_vec(),
                                    ))
                                    .unwrap();
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::HoldPane(
                                        PaneId::Terminal(pid),
                                        Some(2), // exit status
                                        run_command,
                                        None,
                                    ))
                                    .unwrap();
                            }
                        }
                    },
                    Err(e) => {
                        log::error!("Failed to spawn terminal: {}", e);
                    },
                }
            },
            PtyInstruction::SpawnTerminalHorizontally(terminal_action, name, client_id) => {
                let (hold_on_close, run_command, pane_title) = match &terminal_action {
                    Some(TerminalAction::RunCommand(run_command)) => (
                        run_command.hold_on_close,
                        Some(run_command.clone()),
                        Some(name.unwrap_or_else(|| run_command.to_string())),
                    ),
                    _ => (false, None, name),
                };
                match pty.spawn_terminal(terminal_action, ClientOrTabIndex::ClientId(client_id)) {
                    Ok(pid) => {
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::HorizontalSplit(
                                PaneId::Terminal(pid),
                                pane_title,
                                client_id,
                            ))
                            .unwrap();
                    },
                    Err(SpawnTerminalError::CommandNotFound(pid)) => {
                        if hold_on_close {
                            pty.bus
                                .senders
                                .send_to_screen(ScreenInstruction::HorizontalSplit(
                                    PaneId::Terminal(pid),
                                    pane_title,
                                    client_id,
                                ))
                                .unwrap();
                            if let Some(run_command) = run_command {
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::PtyBytes(
                                        pid,
                                        format!(
                                            "Command not found: {}",
                                            run_command.command.display()
                                        )
                                        .as_bytes()
                                        .to_vec(),
                                    ))
                                    .unwrap();
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::HoldPane(
                                        PaneId::Terminal(pid),
                                        Some(2), // exit status
                                        run_command,
                                        None,
                                    ))
                                    .unwrap();
                            }
                        }
                    },
                    Err(e) => {
                        log::error!("Failed to spawn terminal: {}", e);
                    },
                }
            },
            PtyInstruction::UpdateActivePane(pane_id, client_id) => {
                pty.set_active_pane(pane_id, client_id);
            },
            PtyInstruction::GoToTab(tab_index, client_id) => {
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::GoToTab(tab_index, Some(client_id)))
                    .unwrap();
            },
            PtyInstruction::NewTab(terminal_action, tab_layout, tab_name, client_id) => {
                pty.spawn_terminals_for_layout(
                    tab_layout.unwrap_or_else(|| layout.new_tab()),
                    terminal_action.clone(),
                    client_id,
                );

                if let Some(tab_name) = tab_name {
                    // clear current name at first
                    pty.bus
                        .senders
                        .send_to_screen(ScreenInstruction::UpdateTabName(vec![0], client_id))
                        .unwrap();
                    pty.bus
                        .senders
                        .send_to_screen(ScreenInstruction::UpdateTabName(
                            tab_name.into_bytes(),
                            client_id,
                        ))
                        .unwrap();
                }
            },
            PtyInstruction::ClosePane(id) => {
                pty.close_pane(id);
                pty.bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            },
            PtyInstruction::CloseTab(ids) => {
                pty.close_tab(ids);
                pty.bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            },
            PtyInstruction::ReRunCommandInPane(pane_id, run_command) => {
                match pty.rerun_command_in_pane(pane_id, run_command.clone()) {
                    Ok(..) => {},
                    Err(SpawnTerminalError::CommandNotFound(pid)) => {
                        if run_command.hold_on_close {
                            pty.bus
                                .senders
                                .send_to_screen(ScreenInstruction::PtyBytes(
                                    pid,
                                    format!("Command not found: {}", run_command.command.display())
                                        .as_bytes()
                                        .to_vec(),
                                ))
                                .unwrap();
                            pty.bus
                                .senders
                                .send_to_screen(ScreenInstruction::HoldPane(
                                    PaneId::Terminal(pid),
                                    Some(2), // exit status
                                    run_command,
                                    None,
                                ))
                                .unwrap();
                        }
                    },
                    Err(e) => {
                        log::error!("Failed to spawn terminal: {}", e);
                    },
                }
            },
            PtyInstruction::Exit => break,
        }
    }
}

impl Pty {
    pub fn new(
        bus: Bus<PtyInstruction>,
        debug_to_file: bool,
        default_editor: Option<PathBuf>,
    ) -> Self {
        Pty {
            active_panes: HashMap::new(),
            bus,
            id_to_child_pid: HashMap::new(),
            debug_to_file,
            task_handles: HashMap::new(),
            default_editor,
        }
    }
    pub fn get_default_terminal(&self, cwd: Option<PathBuf>) -> TerminalAction {
        TerminalAction::RunCommand(RunCommand {
            args: vec![],
            command: PathBuf::from(env::var("SHELL").expect("Could not find the SHELL variable")),
            cwd, // note: this might also be filled by the calling function, eg. spawn_terminal
            hold_on_close: false,
        })
    }
    fn fill_cwd(&self, terminal_action: &mut TerminalAction, client_id: ClientId) {
        if let TerminalAction::RunCommand(run_command) = terminal_action {
            if run_command.cwd.is_none() {
                run_command.cwd = self
                    .active_panes
                    .get(&client_id)
                    .and_then(|pane| match pane {
                        PaneId::Plugin(..) => None,
                        PaneId::Terminal(id) => self.id_to_child_pid.get(id),
                    })
                    .and_then(|&id| {
                        self.bus
                            .os_input
                            .as_ref()
                            .and_then(|input| input.get_cwd(Pid::from_raw(id)))
                    });
            };
        };
    }
    pub fn spawn_terminal(
        &mut self,
        terminal_action: Option<TerminalAction>,
        client_or_tab_index: ClientOrTabIndex,
    ) -> Result<u32, SpawnTerminalError> {
        // returns the terminal id
        let terminal_action = match client_or_tab_index {
            ClientOrTabIndex::ClientId(client_id) => {
                let mut terminal_action =
                    terminal_action.unwrap_or_else(|| self.get_default_terminal(None));
                self.fill_cwd(&mut terminal_action, client_id);
                terminal_action
            },
            ClientOrTabIndex::TabIndex(_) => {
                terminal_action.unwrap_or_else(|| self.get_default_terminal(None))
            },
        };
        let hold_on_close = match &terminal_action {
            TerminalAction::RunCommand(run_command) => run_command.hold_on_close,
            _ => false,
        };
        let quit_cb = Box::new({
            let senders = self.bus.senders.clone();
            move |pane_id, exit_status, command| {
                if hold_on_close {
                    let _ = senders.send_to_screen(ScreenInstruction::HoldPane(
                        pane_id,
                        exit_status,
                        command,
                        None,
                    ));
                } else {
                    let _ = senders.send_to_screen(ScreenInstruction::ClosePane(pane_id, None));
                }
            }
        });
        let (terminal_id, pid_primary, child_fd): (u32, RawFd, RawFd) = self
            .bus
            .os_input
            .as_mut()
            .unwrap()
            .spawn_terminal(terminal_action, quit_cb, self.default_editor.clone())?;
        let terminal_bytes = task::spawn({
            let senders = self.bus.senders.clone();
            let os_input = self.bus.os_input.as_ref().unwrap().clone();
            let debug_to_file = self.debug_to_file;
            async move {
                TerminalBytes::new(pid_primary, senders, os_input, debug_to_file, terminal_id)
                    .listen()
                    .await;
            }
        });

        self.task_handles.insert(terminal_id, terminal_bytes);
        self.id_to_child_pid.insert(terminal_id, child_fd);
        Ok(terminal_id)
    }
    pub fn spawn_terminals_for_layout(
        &mut self,
        layout: PaneLayout,
        default_shell: Option<TerminalAction>,
        client_id: ClientId,
    ) {
        let mut default_shell = default_shell.unwrap_or_else(|| self.get_default_terminal(None));
        self.fill_cwd(&mut default_shell, client_id);
        let extracted_run_instructions = layout.extract_run_instructions();
        let mut new_pane_pids: Vec<(u32, Option<RunCommand>, Result<RawFd, SpawnTerminalError>)> =
            vec![]; // (terminal_id,
                    // run_command
                    // file_descriptor)
        for run_instruction in extracted_run_instructions {
            let quit_cb = Box::new({
                let senders = self.bus.senders.clone();
                move |pane_id, _exit_status, _command| {
                    let _ = senders.send_to_screen(ScreenInstruction::ClosePane(pane_id, None));
                }
            });
            match run_instruction {
                Some(Run::Command(command)) => {
                    let hold_on_close = command.hold_on_close;
                    let quit_cb = Box::new({
                        let senders = self.bus.senders.clone();
                        move |pane_id, exit_status, command| {
                            if hold_on_close {
                                let _ = senders.send_to_screen(ScreenInstruction::HoldPane(
                                    pane_id,
                                    exit_status,
                                    command,
                                    None,
                                ));
                            } else {
                                let _ = senders
                                    .send_to_screen(ScreenInstruction::ClosePane(pane_id, None));
                            }
                        }
                    });
                    let cmd = TerminalAction::RunCommand(command.clone());
                    match self.bus.os_input.as_mut().unwrap().spawn_terminal(
                        cmd,
                        quit_cb,
                        self.default_editor.clone(),
                    ) {
                        Ok((terminal_id, pid_primary, child_fd)) => {
                            self.id_to_child_pid.insert(terminal_id, child_fd);
                            new_pane_pids.push((
                                terminal_id,
                                Some(command.clone()),
                                Ok(pid_primary),
                            ));
                        },
                        Err(SpawnTerminalError::CommandNotFound(terminal_id)) => {
                            new_pane_pids.push((
                                terminal_id,
                                Some(command.clone()),
                                Err(SpawnTerminalError::CommandNotFound(terminal_id)),
                            ));
                        },
                        Err(e) => {
                            log::error!("Failed to spawn terminal: {}", e);
                        },
                    }
                },
                Some(Run::Cwd(cwd)) => {
                    let shell = self.get_default_terminal(Some(cwd));
                    match self.bus.os_input.as_mut().unwrap().spawn_terminal(
                        shell,
                        quit_cb,
                        self.default_editor.clone(),
                    ) {
                        Ok((terminal_id, pid_primary, child_fd)) => {
                            self.id_to_child_pid.insert(terminal_id, child_fd);
                            new_pane_pids.push((terminal_id, None, Ok(pid_primary)));
                        },
                        Err(SpawnTerminalError::CommandNotFound(terminal_id)) => {
                            new_pane_pids.push((
                                terminal_id,
                                None,
                                Err(SpawnTerminalError::CommandNotFound(terminal_id)),
                            ));
                        },
                        Err(e) => {
                            log::error!("Failed to spawn terminal: {}", e);
                        },
                    }
                },
                Some(Run::EditFile(path_to_file, line_number)) => {
                    match self.bus.os_input.as_mut().unwrap().spawn_terminal(
                        TerminalAction::OpenFile(path_to_file, line_number),
                        quit_cb,
                        self.default_editor.clone(),
                    ) {
                        Ok((terminal_id, pid_primary, child_fd)) => {
                            self.id_to_child_pid.insert(terminal_id, child_fd);
                            new_pane_pids.push((terminal_id, None, Ok(pid_primary)));
                        },
                        Err(SpawnTerminalError::CommandNotFound(terminal_id)) => {
                            new_pane_pids.push((
                                terminal_id,
                                None,
                                Err(SpawnTerminalError::CommandNotFound(terminal_id)),
                            ));
                        },
                        Err(e) => {
                            log::error!("Failed to spawn terminal: {}", e);
                        },
                    }
                },
                None => {
                    match self.bus.os_input.as_mut().unwrap().spawn_terminal(
                        default_shell.clone(),
                        quit_cb,
                        self.default_editor.clone(),
                    ) {
                        Ok((terminal_id, pid_primary, child_fd)) => {
                            self.id_to_child_pid.insert(terminal_id, child_fd);
                            new_pane_pids.push((terminal_id, None, Ok(pid_primary)));
                        },
                        Err(SpawnTerminalError::CommandNotFound(terminal_id)) => {
                            new_pane_pids.push((
                                terminal_id,
                                None,
                                Err(SpawnTerminalError::CommandNotFound(terminal_id)),
                            ));
                        },
                        Err(e) => {
                            log::error!("Failed to spawn terminal: {}", e);
                        },
                    }
                },
                // Investigate moving plugin loading to here.
                Some(Run::Plugin(_)) => {},
            }
        }
        let new_tab_pane_ids: Vec<u32> = new_pane_pids
            .iter()
            .map(|(terminal_id, _, _)| *terminal_id)
            .collect::<Vec<u32>>();
        self.bus
            .senders
            .send_to_screen(ScreenInstruction::NewTab(
                layout,
                new_tab_pane_ids,
                client_id,
            ))
            .unwrap();
        for (terminal_id, run_command, pid_primary) in new_pane_pids {
            match pid_primary {
                Ok(pid_primary) => {
                    let terminal_bytes = task::spawn({
                        let senders = self.bus.senders.clone();
                        let os_input = self.bus.os_input.as_ref().unwrap().clone();
                        let debug_to_file = self.debug_to_file;
                        async move {
                            TerminalBytes::new(
                                pid_primary,
                                senders,
                                os_input,
                                debug_to_file,
                                terminal_id,
                            )
                            .listen()
                            .await;
                        }
                    });
                    self.task_handles.insert(terminal_id, terminal_bytes);
                },
                _ => match run_command {
                    Some(run_command) => {
                        if run_command.hold_on_close {
                            send_command_not_found_to_screen(
                                self.bus.senders.clone(),
                                terminal_id,
                                run_command.clone(),
                            );
                        } else {
                            self.close_pane(PaneId::Terminal(terminal_id));
                        }
                    },
                    None => {
                        self.close_pane(PaneId::Terminal(terminal_id));
                    },
                },
            }
        }
    }
    pub fn close_pane(&mut self, id: PaneId) {
        match id {
            PaneId::Terminal(id) => {
                self.task_handles.remove(&id);
                if let Some(child_fd) = self.id_to_child_pid.remove(&id) {
                    task::block_on(async {
                        self.bus
                            .os_input
                            .as_mut()
                            .unwrap()
                            .kill(Pid::from_raw(child_fd))
                            .unwrap();
                    });
                }
                self.bus.os_input.as_ref().unwrap().clear_terminal_id(id);
            },
            PaneId::Plugin(pid) => drop(
                self.bus
                    .senders
                    .send_to_plugin(PluginInstruction::Unload(pid)),
            ),
        }
    }
    pub fn close_tab(&mut self, ids: Vec<PaneId>) {
        ids.iter().for_each(|&id| {
            self.close_pane(id);
        });
    }
    pub fn set_active_pane(&mut self, pane_id: Option<PaneId>, client_id: ClientId) {
        if let Some(pane_id) = pane_id {
            self.active_panes.insert(client_id, pane_id);
        }
    }
    pub fn rerun_command_in_pane(
        &mut self,
        pane_id: PaneId,
        run_command: RunCommand,
    ) -> Result<(), SpawnTerminalError> {
        match pane_id {
            PaneId::Terminal(id) => {
                let _ = self.task_handles.remove(&id); // if all is well, this shouldn't be here
                let _ = self.id_to_child_pid.remove(&id); // if all is wlel, this shouldn't be here

                let quit_cb = Box::new({
                    let senders = self.bus.senders.clone();
                    move |pane_id, exit_status, command| {
                        // we only re-run held panes, so we'll never close them from Pty
                        let _ = senders.send_to_screen(ScreenInstruction::HoldPane(
                            pane_id,
                            exit_status,
                            command,
                            None,
                        ));
                    }
                });
                let (pid_primary, child_fd): (RawFd, RawFd) =
                    self.bus
                        .os_input
                        .as_mut()
                        .unwrap()
                        .re_run_command_in_terminal(id, run_command, quit_cb)?;
                let terminal_bytes = task::spawn({
                    let senders = self.bus.senders.clone();
                    let os_input = self.bus.os_input.as_ref().unwrap().clone();
                    let debug_to_file = self.debug_to_file;
                    async move {
                        TerminalBytes::new(pid_primary, senders, os_input, debug_to_file, id)
                            .listen()
                            .await;
                    }
                });

                self.task_handles.insert(id, terminal_bytes);
                self.id_to_child_pid.insert(id, child_fd);
                Ok(())
            },
            _ => Err(SpawnTerminalError::GenericSpawnError(
                "Cannot respawn plugin panes",
            )),
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let child_ids: Vec<u32> = self.id_to_child_pid.keys().copied().collect();
        for id in child_ids {
            self.close_pane(PaneId::Terminal(id));
        }
    }
}

fn send_command_not_found_to_screen(
    senders: ThreadSenders,
    terminal_id: u32,
    run_command: RunCommand,
) {
    senders
        .send_to_screen(ScreenInstruction::PtyBytes(
            terminal_id,
            format!("Command not found: {}\n\rIf you were including arguments as part of the command, try including them as 'args' instead.", run_command.command.display())
                .as_bytes()
                .to_vec(),
        ))
        .unwrap();
    senders
        .send_to_screen(ScreenInstruction::HoldPane(
            PaneId::Terminal(terminal_id),
            Some(2),
            run_command.clone(),
            None,
        ))
        .unwrap();
}

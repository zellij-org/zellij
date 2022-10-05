use crate::terminal_bytes::TerminalBytes;
use crate::{
    panes::PaneId, screen::ScreenInstruction, thread_bus::Bus, wasm_vm::PluginInstruction,
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
    SpawnTerminal(Option<TerminalAction>, ClientOrTabIndex),
    OpenInPlaceEditor(PathBuf, Option<usize>, ClientId), // Option<usize> is the optional line number
    SpawnTerminalVertically(Option<TerminalAction>, ClientId),
    SpawnTerminalHorizontally(Option<TerminalAction>, ClientId),
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
            PtyInstruction::Exit => PtyContext::Exit,
        }
    }
}

pub(crate) struct Pty {
    pub active_panes: HashMap<ClientId, PaneId>,
    pub bus: Bus<PtyInstruction>,
    pub id_to_child_pid: HashMap<RawFd, RawFd>, // pty_primary => child raw fd
    debug_to_file: bool,
    task_handles: HashMap<RawFd, JoinHandle<()>>,
    default_editor: Option<PathBuf>,
}

pub(crate) fn pty_thread_main(mut pty: Pty, layout: Box<Layout>) {
    loop {
        let (event, mut err_ctx) = pty.bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Pty((&event).into()));
        match event {
            PtyInstruction::SpawnTerminal(terminal_action, client_or_tab_index) => {
                let pid = pty
                    .spawn_terminal(terminal_action, client_or_tab_index)
                    .unwrap(); // TODO: handle error here
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::NewPane(
                        PaneId::Terminal(pid),
                        client_or_tab_index,
                    ))
                    .unwrap();
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
            PtyInstruction::SpawnTerminalVertically(terminal_action, client_id) => {
                let pid = pty
                    .spawn_terminal(terminal_action, ClientOrTabIndex::ClientId(client_id))
                    .unwrap(); // TODO: handle error here
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::VerticalSplit(
                        PaneId::Terminal(pid),
                        client_id,
                    ))
                    .unwrap();
            },
            PtyInstruction::SpawnTerminalHorizontally(terminal_action, client_id) => {
                let pid = pty
                    .spawn_terminal(terminal_action, ClientOrTabIndex::ClientId(client_id))
                    .unwrap(); // TODO: handle error here
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::HorizontalSplit(
                        PaneId::Terminal(pid),
                        client_id,
                    ))
                    .unwrap();
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
    pub fn get_default_terminal(&self) -> TerminalAction {
        TerminalAction::RunCommand(RunCommand {
            args: vec![],
            command: PathBuf::from(env::var("SHELL").expect("Could not find the SHELL variable")),
            cwd: None, // this should be filled by the calling function, eg. spawn_terminal
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
    ) -> Result<RawFd, &'static str> {
        let terminal_action = match client_or_tab_index {
            ClientOrTabIndex::ClientId(client_id) => {
                let mut terminal_action =
                    terminal_action.unwrap_or_else(|| self.get_default_terminal());
                self.fill_cwd(&mut terminal_action, client_id);
                terminal_action
            },
            ClientOrTabIndex::TabIndex(_) => {
                terminal_action.unwrap_or_else(|| self.get_default_terminal())
            },
        };
        let hold_on_close = match &terminal_action {
            TerminalAction::RunCommand(run_command) => run_command.hold_on_close,
            _ => false
        };
        let quit_cb = Box::new({
            let senders = self.bus.senders.clone();
            move |pane_id| {
                if hold_on_close {
                    let _ = senders.send_to_screen(ScreenInstruction::HoldPane(pane_id, None));
                } else {
                    let _ = senders.send_to_screen(ScreenInstruction::ClosePane(pane_id, None));
                }
            }
        });
        let (pid_primary, child_fd): (RawFd, RawFd) = self
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
                TerminalBytes::new(pid_primary, senders, os_input, debug_to_file)
                    .listen()
                    .await;
            }
        });

        self.task_handles.insert(pid_primary, terminal_bytes);
        self.id_to_child_pid.insert(pid_primary, child_fd);
        Ok(pid_primary)
    }
    pub fn spawn_terminals_for_layout(
        &mut self,
        layout: PaneLayout,
        default_shell: Option<TerminalAction>,
        client_id: ClientId,
    ) {
        let mut default_shell = default_shell.unwrap_or_else(|| self.get_default_terminal());
        self.fill_cwd(&mut default_shell, client_id);
        let extracted_run_instructions = layout.extract_run_instructions();
        let mut new_pane_pids = vec![];
        for run_instruction in extracted_run_instructions {
            let quit_cb = Box::new({
                let senders = self.bus.senders.clone();
                move |pane_id| {
                    let _ = senders.send_to_screen(ScreenInstruction::ClosePane(pane_id, None));
                }
            });
            match run_instruction {
                Some(Run::Command(command)) => {
                    let cmd = TerminalAction::RunCommand(command);
                    let (pid_primary, child_fd): (RawFd, RawFd) = self
                        .bus
                        .os_input
                        .as_mut()
                        .unwrap()
                        .spawn_terminal(cmd, quit_cb, self.default_editor.clone())
                        .unwrap(); // TODO: handle error here
                    self.id_to_child_pid.insert(pid_primary, child_fd);
                    new_pane_pids.push(pid_primary);
                },
                None => {
                    let (pid_primary, child_fd): (RawFd, RawFd) = self
                        .bus
                        .os_input
                        .as_mut()
                        .unwrap()
                        .spawn_terminal(default_shell.clone(), quit_cb, self.default_editor.clone())
                        .unwrap(); // TODO: handle error here
                    self.id_to_child_pid.insert(pid_primary, child_fd);
                    new_pane_pids.push(pid_primary);
                },
                // Investigate moving plugin loading to here.
                Some(Run::Plugin(_)) => {},
            }
        }
        self.bus
            .senders
            .send_to_screen(ScreenInstruction::NewTab(
                layout,
                new_pane_pids.clone(),
                client_id,
            ))
            .unwrap();
        for id in new_pane_pids {
            let terminal_bytes = task::spawn({
                let senders = self.bus.senders.clone();
                let os_input = self.bus.os_input.as_ref().unwrap().clone();
                let debug_to_file = self.debug_to_file;
                async move {
                    TerminalBytes::new(id, senders, os_input, debug_to_file)
                        .listen()
                        .await;
                }
            });
            self.task_handles.insert(id, terminal_bytes);
        }
    }
    pub fn close_pane(&mut self, id: PaneId) {
        match id {
            PaneId::Terminal(id) => {
                let child_fd = self.id_to_child_pid.remove(&id).unwrap();
                self.task_handles.remove(&id).unwrap();
                task::block_on(async {
                    self.bus
                        .os_input
                        .as_mut()
                        .unwrap()
                        .kill(Pid::from_raw(child_fd))
                        .unwrap();
                });
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
}

impl Drop for Pty {
    fn drop(&mut self) {
        let child_ids: Vec<RawFd> = self.id_to_child_pid.keys().copied().collect();
        for id in child_ids {
            self.close_pane(PaneId::Terminal(id));
        }
    }
}

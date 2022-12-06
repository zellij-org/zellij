use crate::terminal_bytes::TerminalBytes;
use crate::{
    panes::PaneId,
    plugins::PluginInstruction,
    screen::ScreenInstruction,
    thread_bus::{Bus, ThreadSenders},
    ClientId, ServerInstruction,
};
use async_std::task::{self, JoinHandle};
use std::{collections::HashMap, env, os::unix::io::RawFd, path::PathBuf};
use zellij_utils::nix::unistd::Pid;
use zellij_utils::{
    async_std,
    errors::prelude::*,
    errors::{ContextType, PtyContext},
    input::{
        command::{RunCommand, TerminalAction},
        layout::{Layout, PaneLayout, Run, RunPluginLocation},
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
pub enum PtyInstruction {
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
        usize,                                // tab_index
        HashMap<RunPluginLocation, Vec<u32>>, // plugin_ids
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

pub(crate) fn pty_thread_main(mut pty: Pty, layout: Box<Layout>) -> Result<()> {
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
                let err_context =
                    || format!("failed to spawn terminal for {:?}", client_or_tab_index);

                let (hold_on_close, run_command, pane_title) = match &terminal_action {
                    Some(TerminalAction::RunCommand(run_command)) => (
                        run_command.hold_on_close,
                        Some(run_command.clone()),
                        Some(name.unwrap_or_else(|| run_command.to_string())),
                    ),
                    _ => (false, None, name),
                };
                match pty
                    .spawn_terminal(terminal_action, client_or_tab_index)
                    .with_context(err_context)
                {
                    Ok((pid, starts_held)) => {
                        let hold_for_command = if starts_held { run_command } else { None };
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::NewPane(
                                PaneId::Terminal(pid),
                                pane_title,
                                should_float,
                                hold_for_command,
                                client_or_tab_index,
                            ))
                            .with_context(err_context)?;
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            if hold_on_close {
                                let hold_for_command = None; // we do not hold an "error" pane
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::NewPane(
                                        PaneId::Terminal(*terminal_id),
                                        pane_title,
                                        should_float,
                                        hold_for_command,
                                        client_or_tab_index,
                                    ))
                                    .with_context(err_context)?;
                                if let Some(run_command) = run_command {
                                    send_command_not_found_to_screen(
                                        pty.bus.senders.clone(),
                                        *terminal_id,
                                        run_command.clone(),
                                    )
                                    .with_context(err_context)?;
                                }
                            } else {
                                log::error!("Failed to spawn terminal: {:?}", err);
                                pty.close_pane(PaneId::Terminal(*terminal_id))
                                    .with_context(err_context)?;
                            }
                        },
                        _ => Err::<(), _>(err).non_fatal(),
                    },
                }
            },
            PtyInstruction::OpenInPlaceEditor(temp_file, line_number, client_id) => {
                let err_context =
                    || format!("failed to open in-place editor for client {}", client_id);

                match pty.spawn_terminal(
                    Some(TerminalAction::OpenFile(temp_file, line_number)),
                    ClientOrTabIndex::ClientId(client_id),
                ) {
                    Ok((pid, _starts_held)) => {
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::OpenInPlaceEditor(
                                PaneId::Terminal(pid),
                                client_id,
                            ))
                            .with_context(err_context)?;
                    },
                    Err(e) => {
                        Err::<(), _>(e).with_context(err_context).non_fatal();
                    },
                }
            },
            PtyInstruction::SpawnTerminalVertically(terminal_action, name, client_id) => {
                let err_context =
                    || format!("failed to spawn terminal vertically for client {client_id}");

                let (hold_on_close, run_command, pane_title) = match &terminal_action {
                    Some(TerminalAction::RunCommand(run_command)) => (
                        run_command.hold_on_close,
                        Some(run_command.clone()),
                        Some(name.unwrap_or_else(|| run_command.to_string())),
                    ),
                    _ => (false, None, name),
                };
                match pty
                    .spawn_terminal(terminal_action, ClientOrTabIndex::ClientId(client_id))
                    .with_context(err_context)
                {
                    Ok((pid, starts_held)) => {
                        let hold_for_command = if starts_held { run_command } else { None };
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::VerticalSplit(
                                PaneId::Terminal(pid),
                                pane_title,
                                hold_for_command,
                                client_id,
                            ))
                            .with_context(err_context)?;
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            let hold_for_command = None; // we do not hold an "error" pane
                            if hold_on_close {
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::VerticalSplit(
                                        PaneId::Terminal(*terminal_id),
                                        pane_title,
                                        hold_for_command,
                                        client_id,
                                    ))
                                    .with_context(err_context)?;
                                if let Some(run_command) = run_command {
                                    pty.bus
                                        .senders
                                        .send_to_screen(ScreenInstruction::PtyBytes(
                                            *terminal_id,
                                            format!(
                                                "Command not found: {}",
                                                run_command.command.display()
                                            )
                                            .as_bytes()
                                            .to_vec(),
                                        ))
                                        .with_context(err_context)?;
                                    pty.bus
                                        .senders
                                        .send_to_screen(ScreenInstruction::HoldPane(
                                            PaneId::Terminal(*terminal_id),
                                            Some(2), // exit status
                                            run_command,
                                            None,
                                        ))
                                        .with_context(err_context)?;
                                }
                            }
                        },
                        _ => Err::<(), _>(err).non_fatal(),
                    },
                }
            },
            PtyInstruction::SpawnTerminalHorizontally(terminal_action, name, client_id) => {
                let err_context =
                    || format!("failed to spawn terminal horizontally for client {client_id}");

                let (hold_on_close, run_command, pane_title) = match &terminal_action {
                    Some(TerminalAction::RunCommand(run_command)) => (
                        run_command.hold_on_close,
                        Some(run_command.clone()),
                        Some(name.unwrap_or_else(|| run_command.to_string())),
                    ),
                    _ => (false, None, name),
                };
                match pty
                    .spawn_terminal(terminal_action, ClientOrTabIndex::ClientId(client_id))
                    .with_context(err_context)
                {
                    Ok((pid, starts_held)) => {
                        let hold_for_command = if starts_held { run_command } else { None };
                        pty.bus
                            .senders
                            .send_to_screen(ScreenInstruction::HorizontalSplit(
                                PaneId::Terminal(pid),
                                pane_title,
                                hold_for_command,
                                client_id,
                            ))
                            .with_context(err_context)?;
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            if hold_on_close {
                                let hold_for_command = None; // we do not hold an "error" pane
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::HorizontalSplit(
                                        PaneId::Terminal(*terminal_id),
                                        pane_title,
                                        hold_for_command,
                                        client_id,
                                    ))
                                    .with_context(err_context)?;
                                if let Some(run_command) = run_command {
                                    pty.bus
                                        .senders
                                        .send_to_screen(ScreenInstruction::PtyBytes(
                                            *terminal_id,
                                            format!(
                                                "Command not found: {}",
                                                run_command.command.display()
                                            )
                                            .as_bytes()
                                            .to_vec(),
                                        ))
                                        .with_context(err_context)?;
                                    pty.bus
                                        .senders
                                        .send_to_screen(ScreenInstruction::HoldPane(
                                            PaneId::Terminal(*terminal_id),
                                            Some(2), // exit status
                                            run_command,
                                            None,
                                        ))
                                        .with_context(err_context)?;
                                }
                            }
                        },
                        _ => Err::<(), _>(err).non_fatal(),
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
                    .with_context(|| {
                        format!("failed to move client {} to tab {}", client_id, tab_index)
                    })?;
            },
            PtyInstruction::NewTab(
                terminal_action,
                tab_layout,
                tab_name,
                tab_index,
                plugin_ids,
                client_id,
            ) => {
                let err_context = || format!("failed to open new tab for client {}", client_id);

                pty.spawn_terminals_for_layout(
                    tab_layout.unwrap_or_else(|| layout.new_tab()),
                    terminal_action.clone(),
                    plugin_ids,
                    tab_index,
                    client_id,
                )
                .with_context(err_context)?;

                if let Some(tab_name) = tab_name {
                    // clear current name at first
                    pty.bus
                        .senders
                        .send_to_screen(ScreenInstruction::UpdateTabName(vec![0], client_id))
                        .with_context(err_context)?;
                    pty.bus
                        .senders
                        .send_to_screen(ScreenInstruction::UpdateTabName(
                            tab_name.into_bytes(),
                            client_id,
                        ))
                        .with_context(err_context)?;
                }
            },
            PtyInstruction::ClosePane(id) => {
                pty.close_pane(id)
                    .and_then(|_| {
                        pty.bus
                            .senders
                            .send_to_server(ServerInstruction::UnblockInputThread)
                    })
                    .with_context(|| format!("failed to close pane {:?}", id))?;
            },
            PtyInstruction::CloseTab(ids) => {
                pty.close_tab(ids)
                    .and_then(|_| {
                        pty.bus
                            .senders
                            .send_to_server(ServerInstruction::UnblockInputThread)
                    })
                    .context("failed to close tabs")?;
            },
            PtyInstruction::ReRunCommandInPane(pane_id, run_command) => {
                let err_context = || format!("failed to rerun command in pane {:?}", pane_id);

                match pty
                    .rerun_command_in_pane(pane_id, run_command.clone())
                    .with_context(err_context)
                {
                    Ok(..) => {},
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                            if run_command.hold_on_close {
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::PtyBytes(
                                        *terminal_id,
                                        format!(
                                            "Command not found: {}",
                                            run_command.command.display()
                                        )
                                        .as_bytes()
                                        .to_vec(),
                                    ))
                                    .with_context(err_context)?;
                                pty.bus
                                    .senders
                                    .send_to_screen(ScreenInstruction::HoldPane(
                                        PaneId::Terminal(*terminal_id),
                                        Some(2), // exit status
                                        run_command,
                                        None,
                                    ))
                                    .with_context(err_context)?;
                            }
                        },
                        _ => Err::<(), _>(err).non_fatal(),
                    },
                }
            },
            PtyInstruction::Exit => break,
        }
    }
    Ok(())
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
        let shell = PathBuf::from(env::var("SHELL").unwrap_or_else(|_| {
            log::warn!("Cannot read SHELL env, falling back to use /bin/sh");
            "/bin/sh".to_string()
        }));
        if !shell.exists() {
            panic!("Cannot find shell {}", shell.display());
        }
        TerminalAction::RunCommand(RunCommand {
            args: vec![],
            command: shell,
            cwd, // note: this might also be filled by the calling function, eg. spawn_terminal
            hold_on_close: false,
            hold_on_start: false,
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
    ) -> Result<(u32, bool)> {
        // bool is starts_held
        let err_context = || format!("failed to spawn terminal for {:?}", client_or_tab_index);

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
        let (hold_on_start, hold_on_close) = match &terminal_action {
            TerminalAction::RunCommand(run_command) => {
                (run_command.hold_on_start, run_command.hold_on_close)
            },
            _ => (false, false),
        };

        if hold_on_start {
            // we don't actually open a terminal in this case, just wait for the user to run it
            let starts_held = hold_on_start;
            let terminal_id = self.bus.os_input.as_mut().unwrap().reserve_terminal_id()?;
            return Ok((terminal_id, starts_held));
        }

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
            .context("no OS I/O interface found")
            .and_then(|os_input| {
                os_input.spawn_terminal(terminal_action, quit_cb, self.default_editor.clone())
            })
            .with_context(err_context)?;
        let terminal_bytes = task::spawn({
            let err_context =
                |terminal_id: u32| format!("failed to run async task for terminal {terminal_id}");
            let senders = self.bus.senders.clone();
            let os_input = self
                .bus
                .os_input
                .as_ref()
                .with_context(|| err_context(terminal_id))
                .fatal()
                .clone();
            let debug_to_file = self.debug_to_file;
            async move {
                TerminalBytes::new(pid_primary, senders, os_input, debug_to_file, terminal_id)
                    .listen()
                    .await
                    .with_context(|| err_context(terminal_id))
                    .fatal();
            }
        });

        self.task_handles.insert(terminal_id, terminal_bytes);
        self.id_to_child_pid.insert(terminal_id, child_fd);
        let starts_held = false;
        Ok((terminal_id, starts_held))
    }
    pub fn spawn_terminals_for_layout(
        &mut self,
        layout: PaneLayout,
        default_shell: Option<TerminalAction>,
        plugin_ids: HashMap<RunPluginLocation, Vec<u32>>,
        tab_index: usize,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || format!("failed to spawn terminals for layout for client {client_id}");

        let mut default_shell = default_shell.unwrap_or_else(|| self.get_default_terminal(None));
        self.fill_cwd(&mut default_shell, client_id);
        let extracted_run_instructions = layout.extract_run_instructions();
        let mut new_pane_pids: Vec<(u32, bool, Option<RunCommand>, Result<RawFd>)> = vec![]; // (terminal_id,
                                                                                             // starts_held,
                                                                                             // run_command,
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
                    let starts_held = command.hold_on_start;
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
                    if starts_held {
                        // we don't actually open a terminal in this case, just wait for the user to run it
                        match self
                            .bus
                            .os_input
                            .as_mut()
                            .context("no OS I/O interface found")
                            .with_context(err_context)?
                            .reserve_terminal_id()
                        {
                            Ok(terminal_id) => {
                                new_pane_pids.push((
                                    terminal_id,
                                    starts_held,
                                    Some(command.clone()),
                                    Ok(terminal_id as i32), // this is not actually correct but gets
                                                            // stripped later
                                ));
                            },
                            Err(e) => Err::<(), _>(e).with_context(err_context).non_fatal(),
                        }
                    } else {
                        match self
                            .bus
                            .os_input
                            .as_mut()
                            .context("no OS I/O interface found")
                            .with_context(err_context)?
                            .spawn_terminal(cmd, quit_cb, self.default_editor.clone())
                            .with_context(err_context)
                        {
                            Ok((terminal_id, pid_primary, child_fd)) => {
                                self.id_to_child_pid.insert(terminal_id, child_fd);
                                new_pane_pids.push((
                                    terminal_id,
                                    starts_held,
                                    Some(command.clone()),
                                    Ok(pid_primary),
                                ));
                            },
                            Err(err) => match err.downcast_ref::<ZellijError>() {
                                Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                                    new_pane_pids.push((
                                        *terminal_id,
                                        starts_held,
                                        Some(command.clone()),
                                        Err(err),
                                    ));
                                },
                                _ => {
                                    Err::<(), _>(err).non_fatal();
                                },
                            },
                        }
                    }
                },
                Some(Run::Cwd(cwd)) => {
                    let starts_held = false; // we do not hold Cwd panes
                    let shell = self.get_default_terminal(Some(cwd));
                    match self
                        .bus
                        .os_input
                        .as_mut()
                        .context("no OS I/O interface found")
                        .with_context(err_context)?
                        .spawn_terminal(shell, quit_cb, self.default_editor.clone())
                        .with_context(err_context)
                    {
                        Ok((terminal_id, pid_primary, child_fd)) => {
                            self.id_to_child_pid.insert(terminal_id, child_fd);
                            new_pane_pids.push((terminal_id, starts_held, None, Ok(pid_primary)));
                        },
                        Err(err) => match err.downcast_ref::<ZellijError>() {
                            Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                                new_pane_pids.push((*terminal_id, starts_held, None, Err(err)));
                            },
                            _ => {
                                Err::<(), _>(err).non_fatal();
                            },
                        },
                    }
                },
                Some(Run::EditFile(path_to_file, line_number)) => {
                    let starts_held = false; // we do not hold edit panes (for now?)
                    match self
                        .bus
                        .os_input
                        .as_mut()
                        .context("no OS I/O interface found")
                        .with_context(err_context)?
                        .spawn_terminal(
                            TerminalAction::OpenFile(path_to_file, line_number),
                            quit_cb,
                            self.default_editor.clone(),
                        )
                        .with_context(err_context)
                    {
                        Ok((terminal_id, pid_primary, child_fd)) => {
                            self.id_to_child_pid.insert(terminal_id, child_fd);
                            new_pane_pids.push((terminal_id, starts_held, None, Ok(pid_primary)));
                        },
                        Err(err) => match err.downcast_ref::<ZellijError>() {
                            Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                                new_pane_pids.push((*terminal_id, starts_held, None, Err(err)));
                            },
                            _ => {
                                Err::<(), _>(err).non_fatal();
                            },
                        },
                    }
                },
                None => {
                    let starts_held = false;
                    match self
                        .bus
                        .os_input
                        .as_mut()
                        .context("no OS I/O interface found")
                        .with_context(err_context)?
                        .spawn_terminal(default_shell.clone(), quit_cb, self.default_editor.clone())
                        .with_context(err_context)
                    {
                        Ok((terminal_id, pid_primary, child_fd)) => {
                            self.id_to_child_pid.insert(terminal_id, child_fd);
                            new_pane_pids.push((terminal_id, starts_held, None, Ok(pid_primary)));
                        },
                        Err(err) => match err.downcast_ref::<ZellijError>() {
                            Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
                                new_pane_pids.push((*terminal_id, starts_held, None, Err(err)));
                            },
                            _ => {
                                Err::<(), _>(err).non_fatal();
                            },
                        },
                    }
                },
                // Investigate moving plugin loading to here.
                Some(Run::Plugin(_)) => {},
            }
        }
        // Option<RunCommand> should only be Some if the pane starts held
        let new_tab_pane_ids: Vec<(u32, Option<RunCommand>)> = new_pane_pids
            .iter()
            .map(|(terminal_id, starts_held, run_command, _)| {
                if *starts_held {
                    (*terminal_id, run_command.clone())
                } else {
                    (*terminal_id, None)
                }
            })
            .collect();
        self.bus
            .senders
            .send_to_screen(ScreenInstruction::ApplyLayout(
                layout,
                new_tab_pane_ids,
                plugin_ids,
                tab_index,
                client_id,
            ))
            .with_context(err_context)?;
        for (terminal_id, starts_held, run_command, pid_primary) in new_pane_pids {
            if starts_held {
                // we do not run a command or start listening for bytes on held panes
                continue;
            }
            match pid_primary {
                Ok(pid_primary) => {
                    let terminal_bytes = task::spawn({
                        let senders = self.bus.senders.clone();
                        let os_input = self
                            .bus
                            .os_input
                            .as_ref()
                            .with_context(err_context)?
                            .clone();
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
                            .await
                            .context("failed to spawn terminals for layout")
                            .fatal();
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
                            )
                            .with_context(err_context)?;
                        } else {
                            self.close_pane(PaneId::Terminal(terminal_id))
                                .with_context(err_context)?;
                        }
                    },
                    None => {
                        self.close_pane(PaneId::Terminal(terminal_id))
                            .with_context(err_context)?;
                    },
                },
            }
        }
        Ok(())
    }
    pub fn close_pane(&mut self, id: PaneId) -> Result<()> {
        let err_context = || format!("failed to close for pane {id:?}");
        match id {
            PaneId::Terminal(id) => {
                self.task_handles.remove(&id);
                if let Some(child_fd) = self.id_to_child_pid.remove(&id) {
                    task::block_on(async {
                        let err_context = || format!("failed to run async task for pane {id}");
                        self.bus
                            .os_input
                            .as_mut()
                            .with_context(err_context)
                            .fatal()
                            .kill(Pid::from_raw(child_fd))
                            .with_context(err_context)
                            .fatal();
                    });
                }
                self.bus
                    .os_input
                    .as_ref()
                    .context("no OS I/O interface found")
                    .and_then(|os_input| os_input.clear_terminal_id(id))
                    .with_context(err_context)?;
            },
            PaneId::Plugin(pid) => drop(
                self.bus
                    .senders
                    .send_to_plugin(PluginInstruction::Unload(pid)),
            ),
        }
        Ok(())
    }
    pub fn close_tab(&mut self, ids: Vec<PaneId>) -> Result<()> {
        for id in ids {
            self.close_pane(id)
                .with_context(|| format!("failed to close tab for pane {id:?}"))?;
        }
        Ok(())
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
    ) -> Result<()> {
        let err_context = || format!("failed to rerun command in pane {:?}", pane_id);

        match pane_id {
            PaneId::Terminal(id) => {
                let _ = self.task_handles.remove(&id); // if all is well, this shouldn't be here
                let _ = self.id_to_child_pid.remove(&id); // if all is wlel, this shouldn't be here

                let hold_on_close = run_command.hold_on_close;
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
                            let _ =
                                senders.send_to_screen(ScreenInstruction::ClosePane(pane_id, None));
                        }
                    }
                });
                let (pid_primary, child_fd): (RawFd, RawFd) = self
                    .bus
                    .os_input
                    .as_mut()
                    .context("no OS I/O interface found")
                    .and_then(|os_input| {
                        os_input.re_run_command_in_terminal(id, run_command, quit_cb)
                    })
                    .with_context(err_context)?;
                let terminal_bytes = task::spawn({
                    let err_context =
                        |pane_id| format!("failed to run async task for pane {pane_id:?}");
                    let senders = self.bus.senders.clone();
                    let os_input = self
                        .bus
                        .os_input
                        .as_ref()
                        .with_context(|| err_context(pane_id))
                        .fatal()
                        .clone();
                    let debug_to_file = self.debug_to_file;
                    async move {
                        TerminalBytes::new(pid_primary, senders, os_input, debug_to_file, id)
                            .listen()
                            .await
                            .with_context(|| err_context(pane_id))
                            .fatal();
                    }
                });

                self.task_handles.insert(id, terminal_bytes);
                self.id_to_child_pid.insert(id, child_fd);
                Ok(())
            },
            _ => Err(anyhow!("cannot respawn plugin panes")).with_context(err_context),
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let child_ids: Vec<u32> = self.id_to_child_pid.keys().copied().collect();
        for id in child_ids {
            self.close_pane(PaneId::Terminal(id))
                .with_context(|| format!("failed to close pane for pid {id}"))
                .fatal();
        }
    }
}

fn send_command_not_found_to_screen(
    senders: ThreadSenders,
    terminal_id: u32,
    run_command: RunCommand,
) -> Result<()> {
    let err_context = || format!("failed to send command_not_fount for terminal {terminal_id}");
    senders
        .send_to_screen(ScreenInstruction::PtyBytes(
            terminal_id,
            format!("Command not found: {}\n\rIf you were including arguments as part of the command, try including them as 'args' instead.", run_command.command.display())
                .as_bytes()
                .to_vec(),
        ))
        .with_context(err_context)?;
    senders
        .send_to_screen(ScreenInstruction::HoldPane(
            PaneId::Terminal(terminal_id),
            Some(2),
            run_command.clone(),
            None,
        ))
        .with_context(err_context)?;
    Ok(())
}

use crate::{
    os_input_output::{AsyncReader, ServerOsApi},
    panes::PaneId,
    screen::ScreenInstruction,
    thread_bus::{Bus, ThreadSenders},
    wasm_vm::PluginInstruction,
    ClientId, ServerInstruction,
};
use async_std::{
    future::timeout as async_timeout,
    task::{self, JoinHandle},
};
use std::{
    collections::HashMap,
    env,
    os::unix::io::RawFd,
    path::PathBuf,
    time::{Duration, Instant},
};
use zellij_utils::nix::unistd::Pid;
use zellij_utils::{
    async_std,
    errors::{get_current_ctx, ContextType, PtyContext},
    input::{
        command::{RunCommand, TerminalAction},
        layout::{Layout, LayoutFromYaml, Run, TabLayout},
    },
    logging::debug_to_file,
};

pub type VteBytes = Vec<u8>;

#[derive(Clone, Copy, Debug)]
pub enum ClientOrTabIndex {
    ClientId(ClientId),
    TabIndex(usize),
}

/// Instructions related to PTYs (pseudoterminals).
#[derive(Clone, Debug)]
pub(crate) enum PtyInstruction {
    SpawnTerminal(Option<TerminalAction>, ClientOrTabIndex),
    SpawnTerminalVertically(Option<TerminalAction>, ClientId),
    SpawnTerminalHorizontally(Option<TerminalAction>, ClientId),
    UpdateActivePane(Option<PaneId>, ClientId),
    NewTab(Option<TerminalAction>, Option<TabLayout>, ClientId),
    ClosePane(PaneId),
    CloseTab(Vec<PaneId>),
    Exit,
}

impl From<&PtyInstruction> for PtyContext {
    fn from(pty_instruction: &PtyInstruction) -> Self {
        match *pty_instruction {
            PtyInstruction::SpawnTerminal(..) => PtyContext::SpawnTerminal,
            PtyInstruction::SpawnTerminalVertically(..) => PtyContext::SpawnTerminalVertically,
            PtyInstruction::SpawnTerminalHorizontally(..) => PtyContext::SpawnTerminalHorizontally,
            PtyInstruction::UpdateActivePane(..) => PtyContext::UpdateActivePane,
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
}

use std::convert::TryFrom;

pub(crate) fn pty_thread_main(mut pty: Pty, layout: Box<LayoutFromYaml>) {
    loop {
        let (event, mut err_ctx) = pty.bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Pty((&event).into()));
        match event {
            PtyInstruction::SpawnTerminal(terminal_action, client_or_tab_index) => {
                let pid = pty.spawn_terminal(terminal_action, client_or_tab_index);
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::NewPane(
                        PaneId::Terminal(pid),
                        client_or_tab_index,
                    ))
                    .unwrap();
            }
            PtyInstruction::SpawnTerminalVertically(terminal_action, client_id) => {
                let pid =
                    pty.spawn_terminal(terminal_action, ClientOrTabIndex::ClientId(client_id));
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::VerticalSplit(
                        PaneId::Terminal(pid),
                        client_id,
                    ))
                    .unwrap();
            }
            PtyInstruction::SpawnTerminalHorizontally(terminal_action, client_id) => {
                let pid =
                    pty.spawn_terminal(terminal_action, ClientOrTabIndex::ClientId(client_id));
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::HorizontalSplit(
                        PaneId::Terminal(pid),
                        client_id,
                    ))
                    .unwrap();
            }
            PtyInstruction::UpdateActivePane(pane_id, client_id) => {
                pty.set_active_pane(pane_id, client_id);
            }
            PtyInstruction::NewTab(terminal_action, tab_layout, client_id) => {
                let tab_name = tab_layout.as_ref().and_then(|layout| {
                    if layout.name.is_empty() {
                        None
                    } else {
                        Some(layout.name.clone())
                    }
                });

                let merged_layout = layout.template.clone().insert_tab_layout(tab_layout);
                let layout: Layout =
                    Layout::try_from(merged_layout).unwrap_or_else(|err| panic!("{}", err));

                pty.spawn_terminals_for_layout(layout, terminal_action.clone(), client_id);

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
            }
            PtyInstruction::ClosePane(id) => {
                pty.close_pane(id);
                pty.bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            PtyInstruction::CloseTab(ids) => {
                pty.close_tab(ids);
                pty.bus
                    .senders
                    .send_to_server(ServerInstruction::UnblockInputThread)
                    .unwrap();
            }
            PtyInstruction::Exit => break,
        }
    }
}

enum ReadResult {
    Ok(usize),
    Timeout,
    Err(std::io::Error),
}

impl From<std::io::Result<usize>> for ReadResult {
    fn from(e: std::io::Result<usize>) -> ReadResult {
        match e {
            Err(e) => ReadResult::Err(e),
            Ok(n) => ReadResult::Ok(n),
        }
    }
}

async fn deadline_read(
    reader: &mut dyn AsyncReader,
    deadline: Option<Instant>,
    buf: &mut [u8],
) -> ReadResult {
    if let Some(deadline) = deadline {
        let timeout = deadline.checked_duration_since(Instant::now());
        if let Some(timeout) = timeout {
            match async_timeout(timeout, reader.read(buf)).await {
                Ok(res) => res.into(),
                _ => ReadResult::Timeout,
            }
        } else {
            // deadline has already elapsed
            ReadResult::Timeout
        }
    } else {
        reader.read(buf).await.into()
    }
}

async fn async_send_to_screen(senders: ThreadSenders, screen_instruction: ScreenInstruction) {
    task::spawn_blocking(move || senders.send_to_screen(screen_instruction))
        .await
        .unwrap()
}

fn stream_terminal_bytes(
    pid: RawFd,
    senders: ThreadSenders,
    os_input: Box<dyn ServerOsApi>,
    debug: bool,
) -> JoinHandle<()> {
    let mut err_ctx = get_current_ctx();
    task::spawn({
        async move {
            err_ctx.add_call(ContextType::AsyncTask);

            // After a successful read, we keep on reading additional data up to a duration of
            // `RENDER_PAUSE`. This is in order to batch up PtyBytes before rendering them.
            // Once `render_deadline` has elapsed, we send Render.
            const RENDER_PAUSE: Duration = Duration::from_millis(30);
            let mut render_deadline = None;
            // Keep track of the last render time so we can render immediately if something shows
            // up after a period of inactivity. This reduces input latency perception.
            let mut last_render = Instant::now();

            let mut buf = [0u8; 65536];
            let mut async_reader = os_input.async_file_reader(pid);
            loop {
                match deadline_read(async_reader.as_mut(), render_deadline, &mut buf).await {
                    ReadResult::Ok(0) | ReadResult::Err(_) => break, // EOF or error
                    ReadResult::Timeout => {
                        async_send_to_screen(senders.clone(), ScreenInstruction::Render).await;
                        // next read does not need a deadline as we just rendered everything
                        render_deadline = None;
                        last_render = Instant::now();
                    }
                    ReadResult::Ok(n_bytes) => {
                        let bytes = &buf[..n_bytes];
                        if debug {
                            let _ = debug_to_file(bytes, pid);
                        }
                        async_send_to_screen(
                            senders.clone(),
                            ScreenInstruction::PtyBytes(pid, bytes.to_vec()),
                        )
                        .await;
                        // if we already have a render_deadline we keep it, otherwise we set it
                        // to RENDER_PAUSE since the last time we rendered.
                        render_deadline.get_or_insert(last_render + RENDER_PAUSE);
                    }
                }
            }
            async_send_to_screen(senders.clone(), ScreenInstruction::Render).await;
        }
    })
}

impl Pty {
    pub fn new(bus: Bus<PtyInstruction>, debug_to_file: bool) -> Self {
        Pty {
            active_panes: HashMap::new(),
            bus,
            id_to_child_pid: HashMap::new(),
            debug_to_file,
            task_handles: HashMap::new(),
        }
    }
    pub fn get_default_terminal(&self) -> TerminalAction {
        TerminalAction::RunCommand(RunCommand {
            args: vec![],
            command: PathBuf::from(env::var("SHELL").expect("Could not find the SHELL variable")),
            cwd: None, // this should be filled by the calling function, eg. spawn_terminal
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
    ) -> RawFd {
        let terminal_action = match client_or_tab_index {
            ClientOrTabIndex::ClientId(client_id) => {
                let mut terminal_action =
                    terminal_action.unwrap_or_else(|| self.get_default_terminal());
                self.fill_cwd(&mut terminal_action, client_id);
                terminal_action
            }
            ClientOrTabIndex::TabIndex(_) => {
                terminal_action.unwrap_or_else(|| self.get_default_terminal())
            }
        };
        let quit_cb = Box::new({
            let senders = self.bus.senders.clone();
            move |pane_id| {
                let _ = senders.send_to_screen(ScreenInstruction::ClosePane(pane_id, None));
            }
        });
        let (pid_primary, child_fd): (RawFd, RawFd) = self
            .bus
            .os_input
            .as_mut()
            .unwrap()
            .spawn_terminal(terminal_action, quit_cb);
        let task_handle = stream_terminal_bytes(
            pid_primary,
            self.bus.senders.clone(),
            self.bus.os_input.as_ref().unwrap().clone(),
            self.debug_to_file,
        );
        self.task_handles.insert(pid_primary, task_handle);
        self.id_to_child_pid.insert(pid_primary, child_fd);
        pid_primary
    }
    pub fn spawn_terminals_for_layout(
        &mut self,
        layout: Layout,
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
                        .spawn_terminal(cmd, quit_cb);
                    self.id_to_child_pid.insert(pid_primary, child_fd);
                    new_pane_pids.push(pid_primary);
                }
                None => {
                    let (pid_primary, child_fd): (RawFd, RawFd) = self
                        .bus
                        .os_input
                        .as_mut()
                        .unwrap()
                        .spawn_terminal(default_shell.clone(), quit_cb);
                    self.id_to_child_pid.insert(pid_primary, child_fd);
                    new_pane_pids.push(pid_primary);
                }
                // Investigate moving plugin loading to here.
                Some(Run::Plugin(_)) => {}
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
            let task_handle = stream_terminal_bytes(
                id,
                self.bus.senders.clone(),
                self.bus.os_input.as_ref().unwrap().clone(),
                self.debug_to_file,
            );
            self.task_handles.insert(id, task_handle);
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
            }
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

use crate::{
    os_input_output::{AsyncReader, ChildId, ServerOsApi},
    panes::PaneId,
    screen::ScreenInstruction,
    thread_bus::{Bus, ThreadSenders},
    wasm_vm::PluginInstruction,
    ServerInstruction,
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

/// Instructions related to PTYs (pseudoterminals).
#[derive(Clone, Debug)]
pub(crate) enum PtyInstruction {
    SpawnTerminal(Option<TerminalAction>),
    SpawnTerminalVertically(Option<TerminalAction>),
    SpawnTerminalHorizontally(Option<TerminalAction>),
    UpdateActivePane(Option<PaneId>),
    NewTab(Option<TerminalAction>, Option<TabLayout>),
    ClosePane(PaneId),
    CloseTab(Vec<PaneId>),
    Exit,
}

impl From<&PtyInstruction> for PtyContext {
    fn from(pty_instruction: &PtyInstruction) -> Self {
        match *pty_instruction {
            PtyInstruction::SpawnTerminal(_) => PtyContext::SpawnTerminal,
            PtyInstruction::SpawnTerminalVertically(_) => PtyContext::SpawnTerminalVertically,
            PtyInstruction::SpawnTerminalHorizontally(_) => PtyContext::SpawnTerminalHorizontally,
            PtyInstruction::UpdateActivePane(_) => PtyContext::UpdateActivePane,
            PtyInstruction::ClosePane(_) => PtyContext::ClosePane,
            PtyInstruction::CloseTab(_) => PtyContext::CloseTab,
            PtyInstruction::NewTab(..) => PtyContext::NewTab,
            PtyInstruction::Exit => PtyContext::Exit,
        }
    }
}

pub(crate) struct Pty {
    pub active_pane: Option<PaneId>,
    pub bus: Bus<PtyInstruction>,
    pub id_to_child_pid: HashMap<RawFd, ChildId>,
    debug_to_file: bool,
    task_handles: HashMap<RawFd, JoinHandle<()>>,
}

use std::convert::TryFrom;

pub(crate) fn pty_thread_main(mut pty: Pty, layout: LayoutFromYaml) {
    loop {
        let (event, mut err_ctx) = pty.bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Pty((&event).into()));
        match event {
            PtyInstruction::SpawnTerminal(terminal_action) => {
                let pid = pty.spawn_terminal(terminal_action);
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::NewPane(PaneId::Terminal(pid)))
                    .unwrap();
            }
            PtyInstruction::SpawnTerminalVertically(terminal_action) => {
                let pid = pty.spawn_terminal(terminal_action);
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::VerticalSplit(PaneId::Terminal(pid)))
                    .unwrap();
            }
            PtyInstruction::SpawnTerminalHorizontally(terminal_action) => {
                let pid = pty.spawn_terminal(terminal_action);
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::HorizontalSplit(PaneId::Terminal(pid)))
                    .unwrap();
            }
            PtyInstruction::UpdateActivePane(pane_id) => {
                pty.set_active_pane(pane_id);
            }
            PtyInstruction::NewTab(terminal_action, tab_layout) => {
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

                pty.spawn_terminals_for_layout(layout, terminal_action.clone());

                if let Some(tab_name) = tab_name {
                    // clear current name at first
                    pty.bus
                        .senders
                        .send_to_screen(ScreenInstruction::UpdateTabName(vec![0]))
                        .unwrap();
                    pty.bus
                        .senders
                        .send_to_screen(ScreenInstruction::UpdateTabName(tab_name.into_bytes()))
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
            // `render_pause`. This is in order to batch up PtyBytes before rendering them.
            // Once `render_deadline` has elapsed, we send Render.
            let render_pause = Duration::from_millis(30);
            let mut render_deadline = None;

            let mut buf = [0u8; 65536];
            let mut async_reader = os_input.async_file_reader(pid);
            loop {
                match deadline_read(async_reader.as_mut(), render_deadline, &mut buf).await {
                    ReadResult::Ok(0) | ReadResult::Err(_) => break, // EOF or error
                    ReadResult::Timeout => {
                        async_send_to_screen(senders.clone(), ScreenInstruction::Render).await;
                        // next read does not need a deadline as we just rendered everything
                        render_deadline = None;
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
                        // to the duration of `render_pause`.
                        render_deadline.get_or_insert(Instant::now() + render_pause);
                    }
                }
            }
            async_send_to_screen(senders.clone(), ScreenInstruction::Render).await;

            // this is a little hacky, and is because the tests end the file as soon as
            // we read everything, rather than hanging until there is new data
            // a better solution would be to fix the test fakes, but this will do for now
            async_send_to_screen(senders, ScreenInstruction::ClosePane(PaneId::Terminal(pid)))
                .await;
        }
    })
}

impl Pty {
    pub fn new(bus: Bus<PtyInstruction>, debug_to_file: bool) -> Self {
        Pty {
            active_pane: None,
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
            cwd: self
                .active_pane
                .and_then(|pane| match pane {
                    PaneId::Plugin(..) => None,
                    PaneId::Terminal(id) => self.id_to_child_pid.get(&id).and_then(|id| id.shell),
                })
                .and_then(|id| self.bus.os_input.as_ref().map(|input| input.get_cwd(id)))
                .flatten(),
        })
    }
    pub fn spawn_terminal(&mut self, terminal_action: Option<TerminalAction>) -> RawFd {
        let terminal_action = terminal_action.unwrap_or_else(|| self.get_default_terminal());
        let (pid_primary, child_id): (RawFd, ChildId) = self
            .bus
            .os_input
            .as_mut()
            .unwrap()
            .spawn_terminal(terminal_action);
        let task_handle = stream_terminal_bytes(
            pid_primary,
            self.bus.senders.clone(),
            self.bus.os_input.as_ref().unwrap().clone(),
            self.debug_to_file,
        );
        self.task_handles.insert(pid_primary, task_handle);
        self.id_to_child_pid.insert(pid_primary, child_id);
        pid_primary
    }
    pub fn spawn_terminals_for_layout(
        &mut self,
        layout: Layout,
        default_shell: Option<TerminalAction>,
    ) {
        let default_shell = default_shell.unwrap_or_else(|| self.get_default_terminal());
        let extracted_run_instructions = layout.extract_run_instructions();
        let mut new_pane_pids = vec![];
        for run_instruction in extracted_run_instructions {
            match run_instruction {
                Some(Run::Command(command)) => {
                    let cmd = TerminalAction::RunCommand(command);
                    let (pid_primary, child_id): (RawFd, ChildId) =
                        self.bus.os_input.as_mut().unwrap().spawn_terminal(cmd);
                    self.id_to_child_pid.insert(pid_primary, child_id);
                    new_pane_pids.push(pid_primary);
                }
                None => {
                    let (pid_primary, child_id): (RawFd, ChildId) = self
                        .bus
                        .os_input
                        .as_mut()
                        .unwrap()
                        .spawn_terminal(default_shell.clone());
                    self.id_to_child_pid.insert(pid_primary, child_id);
                    new_pane_pids.push(pid_primary);
                }
                // Investigate moving plugin loading to here.
                Some(Run::Plugin(_)) => {}
            }
        }
        self.bus
            .senders
            .send_to_screen(ScreenInstruction::ApplyLayout(
                layout,
                new_pane_pids.clone(),
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
                let pids = self.id_to_child_pid.remove(&id).unwrap();
                let handle = self.task_handles.remove(&id).unwrap();
                task::block_on(async {
                    self.bus
                        .os_input
                        .as_mut()
                        .unwrap()
                        .kill(pids.primary)
                        .unwrap();
                    let timeout = Duration::from_millis(100);
                    match async_timeout(timeout, handle.cancel()).await {
                        Ok(_) => {}
                        _ => {
                            self.bus
                                .os_input
                                .as_mut()
                                .unwrap()
                                .force_kill(pids.primary)
                                .unwrap();
                        }
                    };
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
    pub fn set_active_pane(&mut self, pane_id: Option<PaneId>) {
        self.active_pane = pane_id;
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

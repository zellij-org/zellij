use zellij_utils::async_std;

use async_std::future::timeout as async_timeout;
use async_std::task::{self, JoinHandle};
use std::collections::HashMap;
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::{
    os_input_output::{AsyncReader, Pid, ServerOsApi},
    panes::PaneId,
    screen::ScreenInstruction,
    thread_bus::{Bus, ThreadSenders},
    ui::layout::Layout,
    wasm_vm::PluginInstruction,
    ServerInstruction,
};
use zellij_utils::{
    errors::{get_current_ctx, ContextType, PtyContext},
    logging::debug_to_file,
};

pub type VteBytes = Vec<u8>;

/// Instructions related to PTYs (pseudoterminals).
#[derive(Clone, Debug)]
pub(crate) enum PtyInstruction {
    SpawnTerminal(Option<PathBuf>),
    SpawnTerminalVertically(Option<PathBuf>),
    SpawnTerminalHorizontally(Option<PathBuf>),
    NewTab,
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
            PtyInstruction::ClosePane(_) => PtyContext::ClosePane,
            PtyInstruction::CloseTab(_) => PtyContext::CloseTab,
            PtyInstruction::NewTab => PtyContext::NewTab,
            PtyInstruction::Exit => PtyContext::Exit,
        }
    }
}

pub(crate) struct Pty {
    pub bus: Bus<PtyInstruction>,
    pub id_to_child_pid: HashMap<RawFd, Pid>,
    debug_to_file: bool,
    task_handles: HashMap<RawFd, JoinHandle<()>>,
}

pub(crate) fn pty_thread_main(mut pty: Pty, maybe_layout: Option<Layout>) {
    loop {
        let (event, mut err_ctx) = pty.bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Pty((&event).into()));
        match event {
            PtyInstruction::SpawnTerminal(file_to_open) => {
                let pid = pty.spawn_terminal(file_to_open);
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::NewPane(PaneId::Terminal(pid)))
                    .unwrap();
            }
            PtyInstruction::SpawnTerminalVertically(file_to_open) => {
                let pid = pty.spawn_terminal(file_to_open);
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::VerticalSplit(PaneId::Terminal(pid)))
                    .unwrap();
            }
            PtyInstruction::SpawnTerminalHorizontally(file_to_open) => {
                let pid = pty.spawn_terminal(file_to_open);
                pty.bus
                    .senders
                    .send_to_screen(ScreenInstruction::HorizontalSplit(PaneId::Terminal(pid)))
                    .unwrap();
            }
            PtyInstruction::NewTab => {
                if let Some(layout) = maybe_layout.clone() {
                    pty.spawn_terminals_for_layout(layout);
                } else {
                    let pid = pty.spawn_terminal(None);
                    pty.bus
                        .senders
                        .send_to_screen(ScreenInstruction::NewTab(pid))
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
                        let _ = senders.send_to_screen(ScreenInstruction::Render);
                        // next read does not need a deadline as we just rendered everything
                        render_deadline = None;

                        // yield so Screen thread has some time to render before send additional
                        // PtyBytes.
                        task::sleep(Duration::from_millis(10)).await;
                    }
                    ReadResult::Ok(n_bytes) => {
                        let bytes = &buf[..n_bytes];
                        if debug {
                            let _ = debug_to_file(bytes, pid);
                        }
                        let _ = senders
                            .send_to_screen(ScreenInstruction::PtyBytes(pid, bytes.to_vec()));
                        // if we already have a render_deadline we keep it, otherwise we set it
                        // to the duration of `render_pause`.
                        render_deadline.get_or_insert(Instant::now() + render_pause);
                    }
                }
            }
            let _ = senders.send_to_screen(ScreenInstruction::Render);

            #[cfg(not(any(feature = "test", test)))]
            // this is a little hacky, and is because the tests end the file as soon as
            // we read everything, rather than hanging until there is new data
            // a better solution would be to fix the test fakes, but this will do for now
            senders
                .send_to_screen(ScreenInstruction::ClosePane(PaneId::Terminal(pid)))
                .unwrap();
        }
    })
}

impl Pty {
    pub fn new(bus: Bus<PtyInstruction>, debug_to_file: bool) -> Self {
        Pty {
            bus,
            id_to_child_pid: HashMap::new(),
            debug_to_file,
            task_handles: HashMap::new(),
        }
    }
    pub fn spawn_terminal(&mut self, file_to_open: Option<PathBuf>) -> RawFd {
        let (pid_primary, pid_secondary): (RawFd, Pid) = self
            .bus
            .os_input
            .as_mut()
            .unwrap()
            .spawn_terminal(file_to_open);
        let task_handle = stream_terminal_bytes(
            pid_primary,
            self.bus.senders.clone(),
            self.bus.os_input.as_ref().unwrap().clone(),
            self.debug_to_file,
        );
        self.task_handles.insert(pid_primary, task_handle);
        self.id_to_child_pid.insert(pid_primary, pid_secondary);
        pid_primary
    }
    pub fn spawn_terminals_for_layout(&mut self, layout: Layout) {
        let total_panes = layout.total_terminal_panes();
        let mut new_pane_pids = vec![];
        for _ in 0..total_panes {
            let (pid_primary, pid_secondary): (RawFd, Pid) =
                self.bus.os_input.as_mut().unwrap().spawn_terminal(None);
            self.id_to_child_pid.insert(pid_primary, pid_secondary);
            new_pane_pids.push(pid_primary);
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
                let child_pid = self.id_to_child_pid.remove(&id).unwrap();
                let handle = self.task_handles.remove(&id).unwrap();
                self.bus.os_input.as_mut().unwrap().kill(child_pid).unwrap();
                task::block_on(async {
                    handle.cancel().await;
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
}

impl Drop for Pty {
    fn drop(&mut self) {
        let child_ids: Vec<RawFd> = self.id_to_child_pid.keys().copied().collect();
        for id in child_ids {
            self.close_pane(PaneId::Terminal(id));
        }
    }
}

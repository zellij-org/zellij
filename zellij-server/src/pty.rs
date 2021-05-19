use zellij_utils::{async_std, nix};

use async_std::stream::*;
use async_std::task;
use async_std::task::*;
use std::collections::HashMap;
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::pin::*;
use std::time::{Duration, Instant};

use crate::{
    os_input_output::{Pid, ServerOsApi},
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

pub struct ReadFromPid {
    pid: RawFd,
    os_input: Box<dyn ServerOsApi>,
}

impl ReadFromPid {
    pub fn new(pid: &RawFd, os_input: Box<dyn ServerOsApi>) -> ReadFromPid {
        ReadFromPid {
            pid: *pid,
            os_input,
        }
    }
}

impl Stream for ReadFromPid {
    type Item = Vec<u8>;
    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut read_buffer = [0; 65535];
        let pid = self.pid;
        let read_result = &self.os_input.read_from_tty_stdout(pid, &mut read_buffer);
        match read_result {
            Ok(res) => {
                if *res == 0 {
                    // indicates end of file
                    Poll::Ready(None)
                } else {
                    let res = Some(read_buffer[..*res].to_vec());
                    Poll::Ready(res)
                }
            }
            Err(e) => {
                match e {
                    nix::Error::Sys(errno) => {
                        if *errno == nix::errno::Errno::EAGAIN {
                            Poll::Ready(Some(vec![])) // TODO: better with timeout waker somehow
                        } else {
                            Poll::Ready(None)
                        }
                    }
                    _ => Poll::Ready(None),
                }
            }
        }
    }
}

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
            let mut terminal_bytes = ReadFromPid::new(&pid, os_input);

            let mut last_byte_receive_time: Option<Instant> = None;
            let mut pending_render = false;
            let max_render_pause = Duration::from_millis(30);

            while let Some(bytes) = terminal_bytes.next().await {
                let bytes_is_empty = bytes.is_empty();
                if debug {
                    debug_to_file(&bytes, pid).unwrap();
                }
                if !bytes_is_empty {
                    let _ = senders.send_to_screen(ScreenInstruction::PtyBytes(pid, bytes));
                    // for UX reasons, if we got something on the wire, we only send the render notice if:
                    // 1. there aren't any more bytes on the wire afterwards
                    // 2. a certain period (currently 30ms) has elapsed since the last render
                    //    (otherwise if we get a large amount of data, the display would hang
                    //    until it's done)
                    // 3. the stream has ended, and so we render 1 last time
                    match last_byte_receive_time.as_mut() {
                        Some(receive_time) => {
                            if receive_time.elapsed() > max_render_pause {
                                pending_render = false;
                                let _ = senders.send_to_screen(ScreenInstruction::Render);
                                last_byte_receive_time = Some(Instant::now());
                            } else {
                                pending_render = true;
                            }
                        }
                        None => {
                            last_byte_receive_time = Some(Instant::now());
                            pending_render = true;
                        }
                    };
                } else {
                    if pending_render {
                        pending_render = false;
                        let _ = senders.send_to_screen(ScreenInstruction::Render);
                    }
                    last_byte_receive_time = None;
                    task::sleep(::std::time::Duration::from_millis(10)).await;
                }
            }
            senders.send_to_screen(ScreenInstruction::Render).unwrap();
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

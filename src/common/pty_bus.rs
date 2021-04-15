use ::async_std::stream::*;
use ::async_std::task;
use ::async_std::task::*;
use ::std::collections::HashMap;
use ::std::os::unix::io::RawFd;
use ::std::pin::*;
use ::std::sync::mpsc::Receiver;
use ::std::time::{Duration, Instant};
use std::path::PathBuf;

use super::SenderWithContext;
use crate::layout::Layout;
use crate::os_input_output::ServerOsApi;
use crate::utils::logging::debug_to_file;
use crate::{
    errors::{get_current_ctx, ContextType, ErrorContext},
    panes::PaneId,
    screen::ScreenInstruction,
    wasm_vm::PluginInstruction,
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
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut read_buffer = [0; 65535];
        let pid = self.pid;
        let read_result = &self.os_input.read_from_tty_stdout(pid, &mut read_buffer);
        match read_result {
            Ok(res) => {
                if *res == 0 {
                    // indicates end of file
                    Poll::Ready(None)
                } else {
                    let res = Some(read_buffer[..=*res].to_vec());
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
pub enum PtyInstruction {
    SpawnTerminal(Option<PathBuf>),
    SpawnTerminalVertically(Option<PathBuf>),
    SpawnTerminalHorizontally(Option<PathBuf>),
    NewTab,
    ClosePane(PaneId),
    CloseTab(Vec<PaneId>),
    Exit,
}

pub struct PtyBus {
    pub receive_pty_instructions: Receiver<(PtyInstruction, ErrorContext)>,
    pub id_to_child_pid: HashMap<RawFd, RawFd>,
    pub send_screen_instructions: SenderWithContext<ScreenInstruction>,
    send_plugin_instructions: SenderWithContext<PluginInstruction>,
    os_input: Box<dyn ServerOsApi>,
    debug_to_file: bool,
    task_handles: HashMap<RawFd, JoinHandle<()>>,
}

fn stream_terminal_bytes(
    pid: RawFd,
    send_screen_instructions: SenderWithContext<ScreenInstruction>,
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
                    for byte in bytes.iter() {
                        debug_to_file(*byte, pid).unwrap();
                    }
                }
                if !bytes_is_empty {
                    let _ = send_screen_instructions.send(ScreenInstruction::PtyBytes(pid, bytes));
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
                                send_screen_instructions
                                    .send(ScreenInstruction::Render)
                                    .unwrap();
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
                        send_screen_instructions
                            .send(ScreenInstruction::Render)
                            .unwrap();
                    }
                    last_byte_receive_time = None;
                    task::sleep(::std::time::Duration::from_millis(10)).await;
                }
            }
            send_screen_instructions
                .send(ScreenInstruction::Render)
                .unwrap();
            #[cfg(not(test))]
            // this is a little hacky, and is because the tests end the file as soon as
            // we read everything, rather than hanging until there is new data
            // a better solution would be to fix the test fakes, but this will do for now
            send_screen_instructions
                .send(ScreenInstruction::ClosePane(PaneId::Terminal(pid)))
                .unwrap();
        }
    })
}

impl PtyBus {
    pub fn new(
        receive_pty_instructions: Receiver<(PtyInstruction, ErrorContext)>,
        os_input: Box<dyn ServerOsApi>,
        send_screen_instructions: SenderWithContext<ScreenInstruction>,
        send_plugin_instructions: SenderWithContext<PluginInstruction>,
        debug_to_file: bool,
    ) -> Self {
        PtyBus {
            receive_pty_instructions,
            os_input,
            id_to_child_pid: HashMap::new(),
            send_screen_instructions,
            send_plugin_instructions,
            debug_to_file,
            task_handles: HashMap::new(),
        }
    }
    pub fn spawn_terminal(&mut self, file_to_open: Option<PathBuf>) -> RawFd {
        let (pid_primary, pid_secondary): (RawFd, RawFd) =
            self.os_input.spawn_terminal(file_to_open);
        let task_handle = stream_terminal_bytes(
            pid_primary,
            self.send_screen_instructions.clone(),
            self.os_input.clone(),
            self.debug_to_file,
        );
        self.task_handles.insert(pid_primary, task_handle);
        self.id_to_child_pid.insert(pid_primary, pid_secondary);
        pid_primary
    }
    pub fn spawn_terminals_for_layout(&mut self, layout_path: PathBuf) {
        let layout = Layout::new(layout_path.clone());
        let total_panes = layout.total_terminal_panes();
        let mut new_pane_pids = vec![];
        for _ in 0..total_panes {
            let (pid_primary, pid_secondary): (RawFd, RawFd) = self.os_input.spawn_terminal(None);
            self.id_to_child_pid.insert(pid_primary, pid_secondary);
            new_pane_pids.push(pid_primary);
        }
        self.send_screen_instructions
            .send(ScreenInstruction::ApplyLayout(
                layout_path,
                new_pane_pids.clone(),
            ))
            .unwrap();
        for id in new_pane_pids {
            let task_handle = stream_terminal_bytes(
                id,
                self.send_screen_instructions.clone(),
                self.os_input.clone(),
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
                self.os_input.kill(child_pid).unwrap();
                task::block_on(async {
                    handle.cancel().await;
                });
            }
            PaneId::Plugin(pid) => self
                .send_plugin_instructions
                .send(PluginInstruction::Unload(pid))
                .unwrap(),
        }
    }
    pub fn close_tab(&mut self, ids: Vec<PaneId>) {
        ids.iter().for_each(|&id| {
            self.close_pane(id);
        });
    }
}

impl Drop for PtyBus {
    fn drop(&mut self) {
        let child_ids: Vec<RawFd> = self.id_to_child_pid.keys().copied().collect();
        for id in child_ids {
            self.close_pane(PaneId::Terminal(id));
        }
    }
}

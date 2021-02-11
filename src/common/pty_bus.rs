use ::async_std::stream::*;
use ::async_std::task;
use ::async_std::task::*;
use ::std::collections::HashMap;
use ::std::os::unix::io::RawFd;
use ::std::pin::*;
use ::std::sync::mpsc::Receiver;
use ::std::time::{Duration, Instant};
use ::vte;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use super::{ScreenInstruction, OPENCALLS};
use crate::layout::Layout;
use crate::os_input_output::OsApi;
use crate::utils::{consts::ZELLIJ_IPC_PIPE, logging::debug_to_file};
use crate::{
    common::ApiCommand,
    errors::{ContextType, ErrorContext},
    panes::PaneId,
};

pub struct ReadFromPid {
    pid: RawFd,
    os_input: Box<dyn OsApi>,
}

impl ReadFromPid {
    pub fn new(pid: &RawFd, os_input: Box<dyn OsApi>) -> ReadFromPid {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VteEvent {
    // TODO: try not to allocate Vecs
    Print(char),
    Execute(u8),                         // byte
    Hook(Vec<i64>, Vec<u8>, bool, char), // params, intermediates, ignore, char
    Put(u8),                             // byte
    Unhook,
    OscDispatch(Vec<Vec<u8>>, bool), // params, bell_terminated
    CsiDispatch(Vec<i64>, Vec<u8>, bool, char), // params, intermediates, ignore, char
    EscDispatch(Vec<u8>, bool, u8),  // intermediates, ignore, byte
}

struct VteEventSender {
    id: RawFd,
    err_ctx: ErrorContext,
    server_stream: UnixStream,
}

impl VteEventSender {
    pub fn new(id: RawFd, err_ctx: ErrorContext) -> Self {
        VteEventSender {
            id,
            err_ctx,
            server_stream: UnixStream::connect(ZELLIJ_IPC_PIPE).unwrap(),
        }
    }
}

impl vte::Perform for VteEventSender {
    fn print(&mut self, c: char) {
        let api_command = bincode::serialize(&(
            self.err_ctx,
            ApiCommand::ToScreen(ScreenInstruction::Pty(self.id, VteEvent::Print(c))),
        ))
        .unwrap();
        self.server_stream.write_all(&api_command).unwrap();
    }
    fn execute(&mut self, byte: u8) {
        let api_command = bincode::serialize(&(
            self.err_ctx,
            ApiCommand::ToScreen(ScreenInstruction::Pty(self.id, VteEvent::Execute(byte))),
        ))
        .unwrap();
        self.server_stream.write_all(&api_command).unwrap();
    }

    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        let params = params.iter().copied().collect();
        let intermediates = intermediates.iter().copied().collect();
        let instruction =
            ScreenInstruction::Pty(self.id, VteEvent::Hook(params, intermediates, ignore, c));
        let api_command =
            bincode::serialize(&(self.err_ctx, ApiCommand::ToScreen(instruction))).unwrap();
        self.server_stream.write_all(&api_command).unwrap();
    }

    fn put(&mut self, byte: u8) {
        let api_command = bincode::serialize(&(
            self.err_ctx,
            ApiCommand::ToScreen(ScreenInstruction::Pty(self.id, VteEvent::Put(byte))),
        ))
        .unwrap();
        self.server_stream.write_all(&api_command).unwrap();
    }

    fn unhook(&mut self) {
        let api_command = bincode::serialize(&(
            self.err_ctx,
            ApiCommand::ToScreen(ScreenInstruction::Pty(self.id, VteEvent::Unhook)),
        ))
        .unwrap();
        self.server_stream.write_all(&api_command).unwrap();
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let params = params.iter().map(|p| p.to_vec()).collect();
        let instruction =
            ScreenInstruction::Pty(self.id, VteEvent::OscDispatch(params, bell_terminated));
        let api_command =
            bincode::serialize(&(self.err_ctx, ApiCommand::ToScreen(instruction))).unwrap();
        self.server_stream.write_all(&api_command).unwrap();
    }

    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        let params = params.iter().copied().collect();
        let intermediates = intermediates.iter().copied().collect();
        let instruction = ScreenInstruction::Pty(
            self.id,
            VteEvent::CsiDispatch(params, intermediates, ignore, c),
        );
        let api_command =
            bincode::serialize(&(self.err_ctx, ApiCommand::ToScreen(instruction))).unwrap();
        self.server_stream.write_all(&api_command).unwrap();
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        let intermediates = intermediates.iter().copied().collect();
        let instruction =
            ScreenInstruction::Pty(self.id, VteEvent::EscDispatch(intermediates, ignore, byte));
        let api_command =
            bincode::serialize(&(self.err_ctx, ApiCommand::ToScreen(instruction))).unwrap();
        self.server_stream.write_all(&api_command).unwrap();
    }
}

/// Instructions related to PTYs (pseudoterminals).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PtyInstruction {
    SpawnTerminal(Option<PathBuf>),
    SpawnTerminalVertically(Option<PathBuf>),
    SpawnTerminalHorizontally(Option<PathBuf>),
    NewTab,
    ClosePane(PaneId),
    CloseTab(Vec<PaneId>),
    Quit,
}

pub struct PtyBus {
    pub receive_pty_instructions: Receiver<(PtyInstruction, ErrorContext)>,
    pub id_to_child_pid: HashMap<RawFd, RawFd>,
    os_input: Box<dyn OsApi>,
    debug_to_file: bool,
    task_handles: HashMap<RawFd, JoinHandle<()>>,
    pub server_stream: UnixStream,
}

fn stream_terminal_bytes(pid: RawFd, os_input: Box<dyn OsApi>, debug: bool) -> JoinHandle<()> {
    let mut err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());
    task::spawn({
        async move {
            err_ctx.add_call(ContextType::AsyncTask);
            let mut server_stream = UnixStream::connect(ZELLIJ_IPC_PIPE).unwrap();
            let mut vte_parser = vte::Parser::new();
            let mut vte_event_sender = VteEventSender::new(pid, err_ctx);
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
                                let api_command = bincode::serialize(&(
                                    err_ctx,
                                    ApiCommand::ToScreen(ScreenInstruction::Render),
                                ))
                                .unwrap();
                                server_stream.write_all(&api_command).unwrap();
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
                        let api_command = bincode::serialize(&(
                            err_ctx,
                            ApiCommand::ToScreen(ScreenInstruction::Render),
                        ))
                        .unwrap();
                        server_stream.write_all(&api_command).unwrap();
                    }
                    last_byte_receive_time = None;
                    task::sleep(::std::time::Duration::from_millis(10)).await;
                }
            }
            let api_command =
                bincode::serialize(&(err_ctx, ApiCommand::ToScreen(ScreenInstruction::Render)))
                    .unwrap();
            server_stream.write_all(&api_command).unwrap();
            #[cfg(not(test))]
            // this is a little hacky, and is because the tests end the file as soon as
            // we read everything, rather than hanging until there is new data
            // a better solution would be to fix the test fakes, but this will do for now
            let api_command = bincode::serialize(&(
                err_ctx,
                ApiCommand::ToScreen(ScreenInstruction::ClosePane(PaneId::Terminal(pid))),
            ))
            .unwrap();
            server_stream.write_all(&api_command).unwrap();
        }
    })
}

impl PtyBus {
    pub fn new(
        receive_pty_instructions: Receiver<(PtyInstruction, ErrorContext)>,
        os_input: Box<dyn OsApi>,
        server_stream: UnixStream,
        debug_to_file: bool,
    ) -> Self {
        PtyBus {
            receive_pty_instructions,
            os_input,
            id_to_child_pid: HashMap::new(),
            debug_to_file,
            task_handles: HashMap::new(),
            server_stream,
        }
    }
    pub fn spawn_terminal(&mut self, file_to_open: Option<PathBuf>) -> RawFd {
        let (pid_primary, pid_secondary): (RawFd, RawFd) =
            self.os_input.spawn_terminal(file_to_open);
        let task_handle =
            stream_terminal_bytes(pid_primary, self.os_input.clone(), self.debug_to_file);
        self.task_handles.insert(pid_primary, task_handle);
        self.id_to_child_pid.insert(pid_primary, pid_secondary);
        pid_primary
    }
    pub fn spawn_terminals_for_layout(&mut self, layout: Layout, err_ctx: ErrorContext) {
        let total_panes = layout.total_terminal_panes();
        let mut new_pane_pids = vec![];
        for _ in 0..total_panes {
            let (pid_primary, pid_secondary): (RawFd, RawFd) = self.os_input.spawn_terminal(None);
            self.id_to_child_pid.insert(pid_primary, pid_secondary);
            new_pane_pids.push(pid_primary);
        }
        let api_command = bincode::serialize(&(
            err_ctx,
            ApiCommand::ToScreen(ScreenInstruction::ApplyLayout((
                layout,
                new_pane_pids.clone(),
            ))),
        ))
        .unwrap();
        self.server_stream.write_all(&api_command).unwrap();
        for id in new_pane_pids {
            let task_handle = stream_terminal_bytes(id, self.os_input.clone(), self.debug_to_file);
            self.task_handles.insert(id, task_handle);
        }
    }
    pub fn close_pane(&mut self, id: PaneId, err_ctx: ErrorContext) {
        match id {
            PaneId::Terminal(id) => {
                let child_pid = self.id_to_child_pid.remove(&id).unwrap();
                let handle = self.task_handles.remove(&id).unwrap();
                self.os_input.kill(child_pid).unwrap();
                task::block_on(async {
                    handle.cancel().await;
                });
            }
            PaneId::Plugin(pid) => {
                let api_command =
                    bincode::serialize(&(err_ctx, ApiCommand::ClosePluginPane(pid))).unwrap();
                self.server_stream.write_all(&api_command).unwrap();
            }
        }
    }
    pub fn close_tab(&mut self, ids: Vec<PaneId>, err_ctx: ErrorContext) {
        ids.iter().for_each(|&id| {
            self.close_pane(id, err_ctx);
        });
    }
}

impl Drop for PtyBus {
    fn drop(&mut self) {
        let child_ids: Vec<RawFd> = self.id_to_child_pid.keys().copied().collect();
        for id in child_ids {
            self.close_pane(PaneId::Terminal(id), ErrorContext::new());
        }
    }
}

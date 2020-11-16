use ::async_std::stream::*;
use ::async_std::task;
use ::async_std::task::*;
use ::std::collections::HashMap;
use ::std::os::unix::io::RawFd;
use ::std::pin::*;
use ::std::sync::mpsc::{Receiver, Sender};
use ::std::time::{Duration, Instant};
use ::vte;
use std::path::PathBuf;

use crate::layout::Layout;
use crate::os_input_output::OsApi;
use crate::ScreenInstruction;

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

fn _debug_log_to_file(message: String) {
    use std::fs::OpenOptions;
    use std::io::prelude::*;
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open("/tmp/mosaic-log.txt")
        .unwrap();
    file.write_all(message.as_bytes()).unwrap();
    file.write_all("\n".as_bytes()).unwrap();
}

fn debug_to_file(message: u8, pid: RawFd) {
    use std::fs::OpenOptions;
    use std::io::prelude::*;
    let mut path = PathBuf::new();
    path.push(
        [
            String::from("/tmp/mosaic-logs/mosaic-"),
            pid.to_string(),
            String::from(".log"),
        ]
        .concat(),
    );
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .unwrap();
    file.write_all(&[message]).unwrap();
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
                    return Poll::Ready(None);
                } else {
                    let res = Some(read_buffer[..=*res].to_vec());
                    return Poll::Ready(res);
                }
            }
            Err(e) => {
                match e {
                    nix::Error::Sys(errno) => {
                        if *errno == nix::errno::Errno::EAGAIN {
                            return Poll::Ready(Some(vec![])); // TODO: better with timeout waker somehow
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

#[derive(Debug)]
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
    sender: Sender<ScreenInstruction>,
}

impl VteEventSender {
    pub fn new(id: RawFd, sender: Sender<ScreenInstruction>) -> Self {
        VteEventSender { id, sender }
    }
}

impl vte::Perform for VteEventSender {
    fn print(&mut self, c: char) {
        self.sender
            .send(ScreenInstruction::Pty(self.id, VteEvent::Print(c)))
            .unwrap();
    }
    fn execute(&mut self, byte: u8) {
        self.sender
            .send(ScreenInstruction::Pty(self.id, VteEvent::Execute(byte)))
            .unwrap();
    }

    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        let params = params.iter().copied().collect();
        let intermediates = intermediates.iter().copied().collect();
        let instruction =
            ScreenInstruction::Pty(self.id, VteEvent::Hook(params, intermediates, ignore, c));
        self.sender.send(instruction).unwrap();
    }

    fn put(&mut self, byte: u8) {
        self.sender
            .send(ScreenInstruction::Pty(self.id, VteEvent::Put(byte)))
            .unwrap();
    }

    fn unhook(&mut self) {
        self.sender
            .send(ScreenInstruction::Pty(self.id, VteEvent::Unhook))
            .unwrap();
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let params = params.iter().map(|p| p.to_vec()).collect();
        let instruction =
            ScreenInstruction::Pty(self.id, VteEvent::OscDispatch(params, bell_terminated));
        self.sender.send(instruction).unwrap();
    }

    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        let params = params.iter().copied().collect();
        let intermediates = intermediates.iter().copied().collect();
        let instruction = ScreenInstruction::Pty(
            self.id,
            VteEvent::CsiDispatch(params, intermediates, ignore, c),
        );
        self.sender.send(instruction).unwrap();
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        let intermediates = intermediates.iter().copied().collect();
        let instruction =
            ScreenInstruction::Pty(self.id, VteEvent::EscDispatch(intermediates, ignore, byte));
        self.sender.send(instruction).unwrap();
    }
}

pub enum PtyInstruction {
    SpawnTerminal(Option<PathBuf>),
    SpawnTerminalVertically(Option<PathBuf>),
    SpawnTerminalHorizontally(Option<PathBuf>),
    ClosePane(RawFd),
    Quit,
}

pub struct PtyBus {
    pub send_screen_instructions: Sender<ScreenInstruction>,
    pub receive_pty_instructions: Receiver<PtyInstruction>,
    pub id_to_child_pid: HashMap<RawFd, RawFd>,
    os_input: Box<dyn OsApi>,
    debug_to_file: bool,
}

fn stream_terminal_bytes(
    pid: RawFd,
    send_screen_instructions: Sender<ScreenInstruction>,
    os_input: Box<dyn OsApi>,
    debug: bool,
) {
    task::spawn({
        async move {
            let mut vte_parser = vte::Parser::new();
            let mut vte_event_sender = VteEventSender::new(pid, send_screen_instructions.clone());
            let mut terminal_bytes = ReadFromPid::new(&pid, os_input);

            let mut last_byte_receive_time: Option<Instant> = None;
            let mut pending_render = false;
            let max_render_pause = Duration::from_millis(30);

            while let Some(bytes) = terminal_bytes.next().await {
                let bytes_is_empty = bytes.is_empty();
                for byte in bytes {
                    if debug {
                        debug_to_file(byte, pid);
                    }
                    vte_parser.advance(&mut vte_event_sender, byte);
                }
                if !bytes_is_empty {
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
                .send(ScreenInstruction::ClosePane(pid))
                .unwrap();
        }
    });
}

impl PtyBus {
    pub fn new(
        receive_pty_instructions: Receiver<PtyInstruction>,
        send_screen_instructions: Sender<ScreenInstruction>,
        os_input: Box<dyn OsApi>,
        debug_to_file: bool,
    ) -> Self {
        PtyBus {
            send_screen_instructions,
            receive_pty_instructions,
            os_input,
            id_to_child_pid: HashMap::new(),
            debug_to_file,
        }
    }
    pub fn spawn_terminal(&mut self, file_to_open: Option<PathBuf>) {
        let (pid_primary, pid_secondary): (RawFd, RawFd) =
            self.os_input.spawn_terminal(file_to_open);
        stream_terminal_bytes(
            pid_primary,
            self.send_screen_instructions.clone(),
            self.os_input.clone(),
            self.debug_to_file,
        );
        self.id_to_child_pid.insert(pid_primary, pid_secondary);
        self.send_screen_instructions
            .send(ScreenInstruction::NewPane(pid_primary))
            .unwrap();
    }
    pub fn spawn_terminal_vertically(&mut self, file_to_open: Option<PathBuf>) {
        let (pid_primary, pid_secondary): (RawFd, RawFd) =
            self.os_input.spawn_terminal(file_to_open);
        stream_terminal_bytes(
            pid_primary,
            self.send_screen_instructions.clone(),
            self.os_input.clone(),
            self.debug_to_file,
        );
        self.id_to_child_pid.insert(pid_primary, pid_secondary);
        self.send_screen_instructions
            .send(ScreenInstruction::VerticalSplit(pid_primary))
            .unwrap();
    }
    pub fn spawn_terminal_horizontally(&mut self, file_to_open: Option<PathBuf>) {
        let (pid_primary, pid_secondary): (RawFd, RawFd) =
            self.os_input.spawn_terminal(file_to_open);
        stream_terminal_bytes(
            pid_primary,
            self.send_screen_instructions.clone(),
            self.os_input.clone(),
            self.debug_to_file,
        );
        self.id_to_child_pid.insert(pid_primary, pid_secondary);
        self.send_screen_instructions
            .send(ScreenInstruction::HorizontalSplit(pid_primary))
            .unwrap();
    }
    pub fn spawn_terminals_for_layout(&mut self, layout: Layout) {
        let total_panes = layout.total_panes();
        let mut new_pane_pids = vec![];
        for _ in 0..total_panes {
            let (pid_primary, pid_secondary): (RawFd, RawFd) = self.os_input.spawn_terminal(None);
            self.id_to_child_pid.insert(pid_primary, pid_secondary);
            new_pane_pids.push(pid_primary);
        }
        &self
            .send_screen_instructions
            .send(ScreenInstruction::ApplyLayout((
                layout,
                new_pane_pids.clone(),
            )));
        for id in new_pane_pids {
            stream_terminal_bytes(
                id,
                self.send_screen_instructions.clone(),
                self.os_input.clone(),
                self.debug_to_file,
            );
        }
    }
    pub fn close_pane(&mut self, id: RawFd) {
        let child_pid = self.id_to_child_pid.get(&id).unwrap();
        self.os_input.kill(*child_pid).unwrap();
    }
}

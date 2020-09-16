use ::std::os::unix::io::RawFd;
use ::async_std::stream::*;
use ::async_std::task;
use ::async_std::task::*;
use ::std::pin::*;
use ::std::sync::mpsc::{channel, Sender, Receiver};
use ::vte;

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

impl Stream for ReadFromPid {
    type Item = Vec<u8>;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut read_buffer = [0; 115200];
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
            },
            Err(e) => {
                match e {
                    nix::Error::Sys(errno) => {
                        if *errno == nix::errno::Errno::EAGAIN {
                            return Poll::Ready(Some(vec![])) // TODO: better with timeout waker somehow
                        } else {
                            Poll::Ready(None)
                        }
                    },
                    _ => Poll::Ready(None)
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum VteEvent { // TODO: try not to allocate Vecs
    Print(char),
    Execute(u8), // byte
    Hook(Vec<i64>, Vec<u8>, bool, char), // params, intermediates, ignore, char
    Put(u8), // byte
    Unhook,
    OscDispatch(Vec<Vec<u8>>, bool), // params, bell_terminated
    CsiDispatch(Vec<i64>, Vec<u8>, bool, char), // params, intermediates, ignore, char
    EscDispatch(Vec<u8>, bool, u8), // intermediates, ignore, byte
}

struct VteEventSender {
    id: RawFd,
    sender: Sender<ScreenInstruction>,
}

impl VteEventSender {
    pub fn new (id: RawFd, sender: Sender<ScreenInstruction>) -> Self {
        VteEventSender { id, sender }
    }
}

impl vte::Perform for VteEventSender {
    fn print(&mut self, c: char) {
        self.sender.send(
            ScreenInstruction::Pty(self.id, VteEvent::Print(c))
        ).unwrap();
    }
    fn execute(&mut self, byte: u8) {
        self.sender.send(ScreenInstruction::Pty(self.id, VteEvent::Execute(byte))).unwrap();
    }

    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        let params = params.iter().copied().collect();
        let intermediates = intermediates.iter().copied().collect();
        let instruction = ScreenInstruction::Pty(self.id, VteEvent::Hook(params, intermediates, ignore, c));
        self.sender.send(instruction).unwrap();
    }

    fn put(&mut self, byte: u8) {
        self.sender.send(ScreenInstruction::Pty(self.id, VteEvent::Put(byte))).unwrap();
    }

    fn unhook(&mut self) {
        self.sender.send(ScreenInstruction::Pty(self.id, VteEvent::Unhook)).unwrap();
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let params = params.iter().map(|p| p.to_vec()).collect();
        let instruction = ScreenInstruction::Pty(self.id, VteEvent::OscDispatch(params, bell_terminated));
        self.sender.send(instruction).unwrap();
    }

    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        let params = params.iter().copied().collect();
        let intermediates = intermediates.iter().copied().collect();
        let instruction = ScreenInstruction::Pty(self.id, VteEvent::CsiDispatch(params, intermediates, ignore, c));
        self.sender.send(instruction).unwrap();
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        let intermediates = intermediates.iter().copied().collect();
        let instruction = ScreenInstruction::Pty(self.id, VteEvent::EscDispatch(intermediates, ignore, byte));
        self.sender.send(instruction).unwrap();
    }
}

pub enum PtyInstruction {
    SpawnTerminalVertically,
    SpawnTerminalHorizontally,
    Quit
}

pub struct PtyBus {
    pub send_pty_instructions: Sender<PtyInstruction>,
    pub send_screen_instructions: Sender<ScreenInstruction>,
    pub receive_pty_instructions: Receiver<PtyInstruction>,
    os_input: Box<dyn OsApi>,
}

impl PtyBus {
    pub fn new (send_screen_instructions: Sender<ScreenInstruction>, os_input: Box<dyn OsApi>) -> Self {
        let (send_pty_instructions, receive_pty_instructions): (Sender<PtyInstruction>, Receiver<PtyInstruction>) = channel();
        PtyBus {
            send_pty_instructions,
            send_screen_instructions,
            receive_pty_instructions,
            os_input,
        }
    }
    pub fn spawn_terminal_vertically(&mut self) {
        let (pid_primary, _pid_secondary): (RawFd, RawFd) = self.os_input.spawn_terminal();
        task::spawn({
            let send_screen_instructions = self.send_screen_instructions.clone();
            let os_input = self.os_input.clone();
            async move {
                let mut vte_parser = vte::Parser::new();
                let mut vte_event_sender = VteEventSender::new(pid_primary, send_screen_instructions.clone());
                let mut first_terminal_bytes = ReadFromPid::new(&pid_primary, os_input);
                while let Some(bytes) = first_terminal_bytes.next().await {
                    let bytes_is_empty = bytes.is_empty();
                    for byte in bytes {
                        vte_parser.advance(&mut vte_event_sender, byte);
                    }
                    if !bytes_is_empty {
                        send_screen_instructions.send(ScreenInstruction::Render).unwrap();
                    } else {
                        task::sleep(::std::time::Duration::from_millis(10)).await;
                    }
                }
            }
        });
        self.send_screen_instructions.send(ScreenInstruction::VerticalSplit(pid_primary)).unwrap();
    }
    pub fn spawn_terminal_horizontally(&mut self) {
        let (pid_primary, _pid_secondary): (RawFd, RawFd) = self.os_input.spawn_terminal();
        task::spawn({
            let send_screen_instructions = self.send_screen_instructions.clone();
            let os_input = self.os_input.clone();
            async move {
                let mut vte_parser = vte::Parser::new();
                let mut vte_event_sender = VteEventSender::new(pid_primary, send_screen_instructions.clone());
                let mut first_terminal_bytes = ReadFromPid::new(&pid_primary, os_input);
                while let Some(bytes) = first_terminal_bytes.next().await {
                    let bytes_is_empty = bytes.is_empty();
                    for byte in bytes {
                        vte_parser.advance(&mut vte_event_sender, byte);
                    }
                    if !bytes_is_empty {
                        send_screen_instructions.send(ScreenInstruction::Render).unwrap();
                    } else {
                        task::sleep(::std::time::Duration::from_millis(10)).await;
                    }
                }
            }
        });
        self.send_screen_instructions.send(ScreenInstruction::HorizontalSplit(pid_primary)).unwrap();
    }
}

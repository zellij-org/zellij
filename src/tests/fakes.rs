use crate::panes::PositionAndSize;
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::common::{
    ChannelWithContext, ClientInstruction, SenderType, SenderWithContext, ServerInstruction,
};
use crate::errors::ErrorContext;
use crate::os_input_output::{ClientOsApi, ServerOsApi};
use crate::tests::possible_tty_inputs::{get_possible_tty_inputs, Bytes};

const MIN_TIME_BETWEEN_SNAPSHOTS: Duration = Duration::from_millis(150);

#[derive(Clone)]
pub enum IoEvent {
    Kill(RawFd),
    SetTerminalSizeUsingFd(RawFd, u16, u16),
    IntoRawMode(RawFd),
    UnsetRawMode(RawFd),
    TcDrain(RawFd),
}

#[derive(Clone)]
pub struct FakeStdoutWriter {
    output_buffer: Arc<Mutex<Vec<u8>>>,
    pub output_frames: Arc<Mutex<Vec<Vec<u8>>>>,
    last_snapshot_time: Arc<Mutex<Instant>>,
}

impl FakeStdoutWriter {
    pub fn new(last_snapshot_time: Arc<Mutex<Instant>>) -> Self {
        FakeStdoutWriter {
            output_buffer: Arc::new(Mutex::new(Vec::new())),
            output_frames: Arc::new(Mutex::new(Vec::new())),
            last_snapshot_time,
        }
    }
}

impl Write for FakeStdoutWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
        let mut bytes_written = 0;
        let mut output_buffer = self.output_buffer.lock().unwrap();
        for byte in buf {
            bytes_written += 1;
            output_buffer.push(*byte);
        }
        Ok(bytes_written)
    }
    fn flush(&mut self) -> Result<(), std::io::Error> {
        let mut output_buffer = self.output_buffer.lock().unwrap();
        let mut output_frames = self.output_frames.lock().unwrap();
        let new_frame = output_buffer.drain(..).collect();
        output_frames.push(new_frame);
        let mut last_snapshot_time = self.last_snapshot_time.lock().unwrap();
        *last_snapshot_time = Instant::now();
        Ok(())
    }
}

#[derive(Clone)]
pub struct FakeInputOutput {
    read_buffers: Arc<Mutex<HashMap<RawFd, Bytes>>>,
    input_to_add: Arc<Mutex<Option<Vec<[u8; 10]>>>>,
    stdin_commands: Arc<Mutex<VecDeque<Vec<u8>>>>,
    stdin_writes: Arc<Mutex<HashMap<RawFd, Vec<u8>>>>,
    pub stdout_writer: FakeStdoutWriter, // stdout_writer.output is already an arc/mutex
    io_events: Arc<Mutex<Vec<IoEvent>>>,
    win_sizes: Arc<Mutex<HashMap<RawFd, PositionAndSize>>>,
    possible_tty_inputs: HashMap<u16, Bytes>,
    last_snapshot_time: Arc<Mutex<Instant>>,
    started_reading_from_pty: Arc<AtomicBool>,
    client_sender: SenderWithContext<ClientInstruction>,
    client_receiver: Arc<Mutex<mpsc::Receiver<(ClientInstruction, ErrorContext)>>>,
    server_sender: SenderWithContext<ServerInstruction>,
    server_receiver: Arc<Mutex<mpsc::Receiver<(ServerInstruction, ErrorContext)>>>,
}

impl FakeInputOutput {
    pub fn new(winsize: PositionAndSize) -> Self {
        let mut win_sizes = HashMap::new();
        let last_snapshot_time = Arc::new(Mutex::new(Instant::now()));
        let stdout_writer = FakeStdoutWriter::new(last_snapshot_time.clone());
        let (client_sender, client_receiver): ChannelWithContext<ClientInstruction> =
            mpsc::channel();
        let client_sender =
            SenderWithContext::new(ErrorContext::new(), SenderType::Sender(client_sender));
        let (server_sender, server_receiver): ChannelWithContext<ServerInstruction> =
            mpsc::channel();
        let server_sender =
            SenderWithContext::new(ErrorContext::new(), SenderType::Sender(server_sender));
        win_sizes.insert(0, winsize); // 0 is the current terminal

        FakeInputOutput {
            read_buffers: Arc::new(Mutex::new(HashMap::new())),
            stdin_writes: Arc::new(Mutex::new(HashMap::new())),
            input_to_add: Arc::new(Mutex::new(None)),
            stdin_commands: Arc::new(Mutex::new(VecDeque::new())),
            stdout_writer,
            last_snapshot_time,
            io_events: Arc::new(Mutex::new(vec![])),
            win_sizes: Arc::new(Mutex::new(win_sizes)),
            possible_tty_inputs: get_possible_tty_inputs(),
            started_reading_from_pty: Arc::new(AtomicBool::new(false)),
            server_receiver: Arc::new(Mutex::new(server_receiver)),
            server_sender,
            client_receiver: Arc::new(Mutex::new(client_receiver)),
            client_sender,
        }
    }
    pub fn with_tty_inputs(mut self, tty_inputs: HashMap<u16, Bytes>) -> Self {
        self.possible_tty_inputs = tty_inputs;
        self
    }
    pub fn add_terminal_input(&mut self, input: &[&[u8]]) {
        let mut stdin_commands: VecDeque<Vec<u8>> = VecDeque::new();
        for command in input.iter() {
            stdin_commands.push_back(command.iter().copied().collect())
        }
        self.stdin_commands = Arc::new(Mutex::new(stdin_commands));
    }
    pub fn add_terminal(&mut self, fd: RawFd) {
        self.stdin_writes.lock().unwrap().insert(fd, vec![]);
    }
    pub fn add_sigwinch_event(&mut self, new_position_and_size: PositionAndSize) {
        self.sigwinch_event = Some(new_position_and_size);
    }
}

impl ClientOsApi for FakeInputOutput {
    fn get_terminal_size_using_fd(&self, pid: RawFd) -> PositionAndSize {
        if let Some(new_position_and_size) = self.sigwinch_event {
            let (lock, _cvar) = &*self.should_trigger_sigwinch;
            let should_trigger_sigwinch = lock.lock().unwrap();
            if *should_trigger_sigwinch && pid == 0 {
                return new_position_and_size;
            }
        }
        let win_sizes = self.win_sizes.lock().unwrap();
        let winsize = win_sizes.get(&pid).unwrap();
        *winsize
    }
    fn set_raw_mode(&mut self, pid: RawFd) {
        self.io_events
            .lock()
            .unwrap()
            .push(IoEvent::IntoRawMode(pid));
    }
    fn unset_raw_mode(&mut self, pid: RawFd) {
        self.io_events
            .lock()
            .unwrap()
            .push(IoEvent::UnsetRawMode(pid));
    }
    fn box_clone(&self) -> Box<dyn ClientOsApi> {
        Box::new((*self).clone())
    }
    fn read_from_stdin(&self) -> Vec<u8> {
        loop {
            let last_snapshot_time = { *self.last_snapshot_time.lock().unwrap() };
            if last_snapshot_time.elapsed() > MIN_TIME_BETWEEN_SNAPSHOTS {
                break;
            } else {
                ::std::thread::sleep(MIN_TIME_BETWEEN_SNAPSHOTS - last_snapshot_time.elapsed());
            }
        }
        if self.stdin_commands.lock().unwrap().len() == 1 {
            std::thread::sleep_ms(100);
        }
        self.stdin_commands
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(vec![])
    }
    fn get_stdout_writer(&self) -> Box<dyn Write> {
        Box::new(self.stdout_writer.clone())
    }
    fn send_to_server(&mut self, msg: ServerInstruction) {
        self.server_sender.send(msg).unwrap();
    }
    fn update_senders(&mut self, new_ctx: ErrorContext) {
        self.server_sender.update(new_ctx);
        self.client_sender.update(new_ctx);
    }
    fn notify_server(&mut self) {
        ClientOsApi::send_to_server(self, ServerInstruction::NewClient("zellij".into()));
    }
    fn client_recv(&self) -> (ClientInstruction, ErrorContext) {
        self.client_receiver.lock().unwrap().recv().unwrap()
    }
}

impl ServerOsApi for FakeInputOutput {
    fn set_terminal_size_using_fd(&mut self, pid: RawFd, cols: u16, rows: u16) {
        let terminal_input = self
            .possible_tty_inputs
            .get(&cols)
            .expect(&format!("could not find input for size {:?}", cols));
        self.read_buffers
            .lock()
            .unwrap()
            .insert(pid, terminal_input.clone());
        self.io_events
            .lock()
            .unwrap()
            .push(IoEvent::SetTerminalSizeUsingFd(pid, cols, rows));
    }
    fn spawn_terminal(&mut self, _file_to_open: Option<PathBuf>) -> (RawFd, RawFd) {
        let next_terminal_id = self.stdin_writes.lock().unwrap().keys().len() as RawFd + 1;
        self.add_terminal(next_terminal_id);
        (next_terminal_id as i32, next_terminal_id + 1000) // secondary number is arbitrary here
    }
    fn write_to_tty_stdin(&mut self, pid: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error> {
        let mut stdin_writes = self.stdin_writes.lock().unwrap();
        let write_buffer = stdin_writes.get_mut(&pid).unwrap();
        let mut bytes_written = 0;
        for byte in buf {
            bytes_written += 1;
            write_buffer.push(*byte);
        }
        Ok(bytes_written)
    }
    fn read_from_tty_stdout(&mut self, pid: RawFd, buf: &mut [u8]) -> Result<usize, nix::Error> {
        let mut read_buffers = self.read_buffers.lock().unwrap();
        let mut bytes_read = 0;
        match read_buffers.get_mut(&pid) {
            Some(bytes) => {
                for i in bytes.read_position..bytes.content.len() {
                    bytes_read += 1;
                    buf[i] = bytes.content[i];
                }
                if bytes_read > bytes.read_position {
                    bytes.set_read_position(bytes_read);
                }
                return Ok(bytes_read);
            }
            None => Err(nix::Error::Sys(nix::errno::Errno::EAGAIN)),
        }
    }
    fn tcdrain(&mut self, pid: RawFd) -> Result<(), nix::Error> {
        self.io_events.lock().unwrap().push(IoEvent::TcDrain(pid));
        Ok(())
    }
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }
    fn kill(&mut self, fd: RawFd) -> Result<(), nix::Error> {
        self.io_events.lock().unwrap().push(IoEvent::Kill(fd));
        Ok(())
    }
    fn send_to_server(&mut self, msg: ServerInstruction) {
        self.server_sender.send(msg).unwrap();
    }
    fn server_recv(&self) -> (ServerInstruction, ErrorContext) {
        self.server_receiver.lock().unwrap().recv().unwrap()
    }
    fn send_to_client(&mut self, msg: ClientInstruction) {
        self.client_sender.send(msg).unwrap();
    }
    fn add_client_sender(&mut self, _buffer_path: String) {}
    fn update_senders(&mut self, new_ctx: ErrorContext) {
        self.server_sender.update(new_ctx);
        self.client_sender.update(new_ctx);
    }
}

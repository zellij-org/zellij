use crate::tests::possible_tty_inputs::{get_possible_tty_inputs, Bytes};
use crate::tests::utils::commands::{QUIT, SLEEP};
use interprocess::local_socket::LocalSocketStream;
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use zellij_client::os_input_output::ClientOsApi;
use zellij_server::os_input_output::ServerOsApi;
use zellij_tile::data::Palette;
use zellij_utils::{
    channels::{ChannelWithContext, SenderType, SenderWithContext},
    errors::ErrorContext,
    ipc::{ClientToServerMsg, ServerToClientMsg},
    pane_size::PositionAndSize,
    shared::default_palette,
};

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
    send_instructions_to_client: SenderWithContext<ServerToClientMsg>,
    receive_instructions_from_server: Arc<Mutex<mpsc::Receiver<(ServerToClientMsg, ErrorContext)>>>,
    send_instructions_to_server: SenderWithContext<ClientToServerMsg>,
    receive_instructions_from_client: Arc<Mutex<mpsc::Receiver<(ClientToServerMsg, ErrorContext)>>>,
    should_trigger_sigwinch: Arc<(Mutex<bool>, Condvar)>,
    sigwinch_event: Option<PositionAndSize>,
}

impl FakeInputOutput {
    pub fn new(winsize: PositionAndSize) -> Self {
        let mut win_sizes = HashMap::new();
        let last_snapshot_time = Arc::new(Mutex::new(Instant::now()));
        let stdout_writer = FakeStdoutWriter::new(last_snapshot_time.clone());
        let (client_sender, client_receiver): ChannelWithContext<ServerToClientMsg> =
            mpsc::channel();
        let send_instructions_to_client = SenderWithContext::new(SenderType::Sender(client_sender));
        let (server_sender, server_receiver): ChannelWithContext<ClientToServerMsg> =
            mpsc::channel();
        let send_instructions_to_server = SenderWithContext::new(SenderType::Sender(server_sender));
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
            receive_instructions_from_client: Arc::new(Mutex::new(server_receiver)),
            send_instructions_to_server,
            receive_instructions_from_server: Arc::new(Mutex::new(client_receiver)),
            send_instructions_to_client,
            should_trigger_sigwinch: Arc::new((Mutex::new(false), Condvar::new())),
            sigwinch_event: None,
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
    fn unset_raw_mode(&self, pid: RawFd) {
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
        let command = self
            .stdin_commands
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(vec![]);
        if command == SLEEP {
            std::thread::sleep(std::time::Duration::from_millis(200));
        } else if command == QUIT && self.sigwinch_event.is_some() {
            let (lock, cvar) = &*self.should_trigger_sigwinch;
            {
                let mut should_trigger_sigwinch = lock.lock().unwrap();
                *should_trigger_sigwinch = true;
            }
            cvar.notify_one();
            ::std::thread::sleep(MIN_TIME_BETWEEN_SNAPSHOTS); // give some time for the app to resize before quitting
        } else if command == QUIT {
            ::std::thread::sleep(MIN_TIME_BETWEEN_SNAPSHOTS);
        }
        command
    }
    fn get_stdout_writer(&self) -> Box<dyn Write> {
        Box::new(self.stdout_writer.clone())
    }
    fn send_to_server(&self, msg: ClientToServerMsg) {
        self.send_instructions_to_server.send(msg).unwrap();
    }
    fn recv_from_server(&self) -> (ServerToClientMsg, ErrorContext) {
        self.receive_instructions_from_server
            .lock()
            .unwrap()
            .recv()
            .unwrap()
    }
    fn handle_signals(&self, sigwinch_cb: Box<dyn Fn()>, _quit_cb: Box<dyn Fn()>) {
        if self.sigwinch_event.is_some() {
            let (lock, cvar) = &*self.should_trigger_sigwinch;
            {
                let mut should_trigger_sigwinch = lock.lock().unwrap();
                while !*should_trigger_sigwinch {
                    should_trigger_sigwinch = cvar.wait(should_trigger_sigwinch).unwrap();
                }
            }
            sigwinch_cb();
        }
    }
    fn connect_to_server(&self, _path: &std::path::Path) {}
    fn load_palette(&self) -> Palette {
        default_palette()
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
    fn recv_from_client(&self) -> (ClientToServerMsg, ErrorContext) {
        self.receive_instructions_from_client
            .lock()
            .unwrap()
            .recv()
            .unwrap()
    }
    fn send_to_client(&self, msg: ServerToClientMsg) {
        self.send_instructions_to_client.send(msg).unwrap();
    }
    fn add_client_sender(&mut self) {}
    fn update_receiver(&mut self, _stream: LocalSocketStream) {}
    fn load_palette(&self) -> Palette {
        default_palette()
    }
}

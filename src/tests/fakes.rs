use crate::panes::PositionAndSize;
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::os_input_output::OsApi;
use crate::tests::possible_tty_inputs::{get_possible_tty_inputs, Bytes};

const MIN_TIME_BETWEEN_SNAPSHOTS: Duration = Duration::from_millis(50);

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
}

impl FakeInputOutput {
    pub fn new(winsize: PositionAndSize) -> Self {
        let mut win_sizes = HashMap::new();
        let last_snapshot_time = Arc::new(Mutex::new(Instant::now()));
        let stdout_writer = FakeStdoutWriter::new(last_snapshot_time.clone());
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
}

impl OsApi for FakeInputOutput {
    fn get_terminal_size_using_fd(&self, pid: RawFd) -> PositionAndSize {
        let win_sizes = self.win_sizes.lock().unwrap();
        let winsize = win_sizes.get(&pid).unwrap();
        *winsize
    }
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
    fn spawn_terminal(&mut self, _file_to_open: Option<PathBuf>) -> (RawFd, RawFd) {
        let next_terminal_id = self.stdin_writes.lock().unwrap().keys().len() as RawFd + 1;
        self.add_terminal(next_terminal_id);
        (next_terminal_id as i32, next_terminal_id + 1000) // secondary number is arbitrary here
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
                self.started_reading_from_pty.store(true, Ordering::Release);
                return Ok(bytes_read);
            }
            None => Err(nix::Error::Sys(nix::errno::Errno::EAGAIN)),
        }
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
    fn tcdrain(&mut self, pid: RawFd) -> Result<(), nix::Error> {
        self.io_events.lock().unwrap().push(IoEvent::TcDrain(pid));
        Ok(())
    }
    fn box_clone(&self) -> Box<dyn OsApi> {
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
        self.stdin_commands
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(vec![])
    }
    fn get_stdout_writer(&self) -> Box<dyn Write> {
        Box::new(self.stdout_writer.clone())
    }
    fn kill(&mut self, fd: RawFd) -> Result<(), nix::Error> {
        self.io_events.lock().unwrap().push(IoEvent::Kill(fd));
        Ok(())
    }
}

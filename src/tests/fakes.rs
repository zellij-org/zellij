use ::nix::pty::Winsize;
use ::std::collections::HashMap;
use ::std::io::{Read, Write};
use ::std::os::unix::io::RawFd;
use ::std::path::PathBuf;
use ::std::sync::{Arc, Mutex};
use ::std::time::Duration;

use crate::os_input_output::OsApi;
use crate::tests::possible_tty_inputs::{get_possible_tty_inputs, Bytes};

#[derive(Clone)]
pub enum IoEvent {
    Kill(RawFd),
    SetTerminalSizeUsingFd(RawFd, u16, u16),
    IntoRawMode(RawFd),
    UnsetRawMode(RawFd),
    TcDrain(RawFd),
}

pub struct FakeStdinReader {
    pub input_chars: Vec<[u8; 10]>,
    pub read_position: usize,
}

impl FakeStdinReader {
    pub fn new(input_chars: Vec<[u8; 10]>) -> Self {
        FakeStdinReader {
            input_chars,
            read_position: 0,
        }
    }
}

impl Read for FakeStdinReader {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        // ideally, we shouldn't have to sleep here
        // stdin should be buffered and handled in the app itself
        ::std::thread::sleep(Duration::from_millis(50));
        // ::std::thread::sleep(Duration::from_millis(100));
        let read_position = self.read_position;
        let bytes_to_read = self.input_chars.get(read_position).unwrap();
        for (i, byte) in bytes_to_read.iter().enumerate() {
            buf[i] = *byte;
        }
        self.read_position += 1;
        Ok(bytes_to_read.len())
    }
}

#[derive(Clone, Default)]
pub struct FakeStdoutWriter {
    output_buffer: Arc<Mutex<Vec<u8>>>,
    pub output_frames: Arc<Mutex<Vec<Vec<u8>>>>,
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
        Ok(())
    }
}

#[derive(Clone)]
pub struct FakeInputOutput {
    read_buffers: Arc<Mutex<HashMap<RawFd, Bytes>>>,
    input_to_add: Arc<Mutex<Option<Vec<[u8; 10]>>>>,
    stdin_writes: Arc<Mutex<HashMap<RawFd, Vec<u8>>>>,
    pub stdout_writer: FakeStdoutWriter, // stdout_writer.output is already an arc/mutex
    io_events: Arc<Mutex<Vec<IoEvent>>>,
    win_sizes: Arc<Mutex<HashMap<RawFd, Winsize>>>,
    possible_tty_inputs: HashMap<u16, Bytes>,
}

impl FakeInputOutput {
    pub fn new(winsize: Winsize) -> Self {
        let mut win_sizes = HashMap::new();
        win_sizes.insert(0, winsize); // 0 is the current terminal
        FakeInputOutput {
            read_buffers: Arc::new(Mutex::new(HashMap::new())),
            stdin_writes: Arc::new(Mutex::new(HashMap::new())),
            input_to_add: Arc::new(Mutex::new(None)),
            stdout_writer: FakeStdoutWriter::default(),
            io_events: Arc::new(Mutex::new(vec![])),
            win_sizes: Arc::new(Mutex::new(win_sizes)),
            possible_tty_inputs: get_possible_tty_inputs(),
        }
    }
    pub fn with_tty_inputs(mut self, tty_inputs: HashMap<u16, Bytes>) -> Self {
        self.possible_tty_inputs = tty_inputs;
        self
    }
    pub fn add_terminal_input(&mut self, input: &[[u8; 10]]) {
        self.input_to_add = Arc::new(Mutex::new(Some(input.to_vec())));
    }
    pub fn add_terminal(&mut self, fd: RawFd) {
        self.stdin_writes.lock().unwrap().insert(fd, vec![]);
    }
}

impl OsApi for FakeInputOutput {
    fn get_terminal_size_using_fd(&self, pid: RawFd) -> Winsize {
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
    fn into_raw_mode(&mut self, pid: RawFd) {
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
        let mut attempts_left = 3;
        loop {
            if attempts_left < 3 {
                // this sometimes happens because in the context of the tests,
                // the read_buffers are set in set_terminal_size_using_fd
                // which sometimes happens after the first read and then the tests get messed up
                // in a real world application this doesn't matter, but here the snapshots taken
                // in the tests are asserted against exact copies, and so a slight variation makes
                // them fail
                ::std::thread::sleep(::std::time::Duration::from_millis(25));
            } else if attempts_left == 0 {
                return Ok(0);
            }
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
                None => {
                    attempts_left -= 1;
                }
            }
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
    fn get_stdin_reader(&self) -> Box<dyn Read> {
        let mut input_chars = vec![[0; 10]];
        if let Some(input_to_add) = self.input_to_add.lock().unwrap().as_ref() {
            for bytes in input_to_add {
                input_chars.push(*bytes);
            }
        }
        input_chars.push([17, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // ctrl-q (quit)
        let reader = FakeStdinReader::new(input_chars);
        Box::new(reader)
    }
    fn get_stdout_writer(&self) -> Box<dyn Write> {
        Box::new(self.stdout_writer.clone())
    }
    fn kill(&mut self, fd: RawFd) -> Result<(), nix::Error> {
        self.io_events.lock().unwrap().push(IoEvent::Kill(fd));
        Ok(())
    }
}

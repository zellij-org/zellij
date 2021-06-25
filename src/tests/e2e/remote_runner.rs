use zellij_tile::data::Palette;
use zellij_utils::pane_size::PositionAndSize;

use zellij_server::panes::TerminalPane;
use zellij_utils::{vte, zellij_tile};

use ssh2::Session;
use std::io::prelude::*;
use std::net::TcpStream;

use std::path::{Path, PathBuf};
use std::fs::File;

const ZELLIJ_EXECUTABLE_LOCATION: &str = "/usr/src/zellij/x86_64-unknown-linux-musl/debug/zellij";
const CONNECTION_STRING: &str = "127.0.0.1:2222";
const CONNECTION_USERNAME: &str = "test";
const CONNECTION_PASSWORD: &str = "test";

fn ssh_connect() -> ssh2::Session {
    let tcp = TcpStream::connect(CONNECTION_STRING).unwrap();
    let mut sess = Session::new().unwrap();
    sess.set_tcp_stream(tcp);
    sess.handshake().unwrap();
    sess.userauth_password(CONNECTION_USERNAME, CONNECTION_PASSWORD)
        .unwrap();
    sess.set_timeout(20000);
    sess
}

fn send_local_file_to_remote(local_layout_path: &Path, remote_path: &Path) {
    let mut file = File::open(local_layout_path).unwrap();
    let mut file_buffer = Vec::new();
    file.read_to_end(&mut file_buffer).unwrap();
    let file_size = file_buffer.len() as u64;
    let sess = ssh_connect();
    let mut channel = sess.scp_send(&remote_path, 0o644, file_size, None).unwrap();
    channel.write_all(&file_buffer).unwrap();
    channel.close().unwrap();
}


fn setup_remote_environment(channel: &mut ssh2::Channel, win_size: PositionAndSize) {
    let (columns, rows) = (win_size.cols as u32, win_size.rows as u32);
    channel
        .request_pty("xterm", None, Some((columns, rows, 0, 0)))
        .unwrap();
    channel.shell().unwrap();
    channel
        .write_all(format!("export PS1=\"$ \"\n").as_bytes())
        .unwrap();
    channel.flush().unwrap();
}

fn start_zellij(channel: &mut ssh2::Channel, session_name: Option<&String>) {
    match session_name.as_ref() {
        Some(name) => {
            channel
                .write_all(
                    format!("{} --session {}\n", ZELLIJ_EXECUTABLE_LOCATION, name).as_bytes(),
                )
                .unwrap();
        }
        None => {
            channel
                .write_all(format!("{}\n", ZELLIJ_EXECUTABLE_LOCATION).as_bytes())
                .unwrap();
        }
    };
    channel.flush().unwrap();
}

fn start_zellij_with_layout(channel: &mut ssh2::Channel, layout_path: &str, session_name: Option<&String>) {
    match session_name.as_ref() {
        Some(name) => {
            channel
                .write_all(
                    format!("{} --layout-path {} --session {}\n", ZELLIJ_EXECUTABLE_LOCATION, layout_path, name).as_bytes(),
                )
                .unwrap();
        }
        None => {
            channel
                .write_all(format!("{} --layout-path {}\n", ZELLIJ_EXECUTABLE_LOCATION, layout_path).as_bytes())
                .unwrap();
        }
    };
    channel.flush().unwrap();
}

pub fn take_snapshot(terminal_output: &mut TerminalPane) -> String {
    let output_lines = terminal_output.read_buffer_as_lines();
    let cursor_coordinates = terminal_output.cursor_coordinates();
    let mut snapshot = String::new();
    for (line_index, line) in output_lines.iter().enumerate() {
        for (character_index, terminal_character) in line.iter().enumerate() {
            if let Some((cursor_x, cursor_y)) = cursor_coordinates {
                if line_index == cursor_y && character_index == cursor_x {
                    snapshot.push('█');
                    continue;
                }
            }
            snapshot.push(terminal_character.character);
        }
        if line_index != output_lines.len() - 1 {
            snapshot.push('\n');
        }
    }
    snapshot
}

pub struct RemoteTerminal<'a> {
    channel: &'a mut ssh2::Channel,
    session_name: Option<&'a String>,
    cursor_x: usize,
    cursor_y: usize,
    current_snapshot: String,
}

impl<'a> std::fmt::Debug for RemoteTerminal<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "cursor x: {}\ncursor_y: {}\ncurrent_snapshot:\n{}",
            self.cursor_x, self.cursor_y, self.current_snapshot
        )
    }
}

impl<'a> RemoteTerminal<'a> {
    pub fn cursor_position_is(&self, x: usize, y: usize) -> bool {
        x == self.cursor_x && y == self.cursor_y
    }
    pub fn tip_appears(&self) -> bool {
        self.current_snapshot.contains("Tip:")
    }
    pub fn status_bar_appears(&self) -> bool {
        self.current_snapshot.contains("Ctrl +") && !self.current_snapshot.contains("─────")
        // this is a bug that happens because the app draws borders around the status bar momentarily on first render
    }
    pub fn snapshot_contains(&self, text: &str) -> bool {
        self.current_snapshot.contains(text)
    }
    pub fn current_snapshot(&self) -> String {
        // convenience method for writing tests,
        // this should only be used when developing,
        // please prefer "snapsht_contains" instead
        self.current_snapshot.clone()
    }
    pub fn current_cursor_position(&self) -> String {
        // convenience method for writing tests,
        // this should only be used when developing,
        // please prefer "cursor_position_is" instead
        format!("x: {}, y: {}", self.cursor_x, self.cursor_y)
    }
    pub fn send_key(&mut self, key: &[u8]) {
        self.channel.write(key).unwrap();
        self.channel.flush().unwrap();
    }
    pub fn change_size(&mut self, cols: u32, rows: u32) {
        self.channel
            .request_pty_size(cols, rows, Some(cols), Some(rows))
            .unwrap();
    }
    pub fn attach_to_original_session(&mut self) {
        self.channel
            .write_all(
                format!(
                    "{} attach {}\n",
                    ZELLIJ_EXECUTABLE_LOCATION,
                    self.session_name.unwrap()
                )
                .as_bytes(),
            )
            .unwrap();
        self.channel.flush().unwrap();
    }
}

#[derive(Clone)]
pub struct Step {
    pub instruction: fn(RemoteTerminal) -> bool,
    pub name: &'static str,
}

pub struct RemoteRunner {
    steps: Vec<Step>,
    current_step_index: usize,
    vte_parser: vte::Parser,
    terminal_output: TerminalPane,
    channel: ssh2::Channel,
    session_name: Option<String>,
    test_name: &'static str,
    currently_running_step: Option<String>,
    retries_left: usize,
    win_size: PositionAndSize,
    local_layout_path: Option<PathBuf>,
}

impl RemoteRunner {
    pub fn new(
        test_name: &'static str,
        win_size: PositionAndSize,
        session_name: Option<String>,
    ) -> Self {
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        let vte_parser = vte::Parser::new();
        let terminal_output = TerminalPane::new(0, win_size, Palette::default());
        setup_remote_environment(&mut channel, win_size);
        start_zellij(&mut channel, session_name.as_ref());
        RemoteRunner {
            steps: vec![],
            channel,
            terminal_output,
            vte_parser,
            session_name,
            test_name,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: 3,
            win_size,
            local_layout_path: None,
        }
    }
    pub fn new_with_layout(
        test_name: &'static str,
        win_size: PositionAndSize,
        local_layout_path: &Path,
        session_name: Option<String>,
    ) -> Self {
        let layout_file_name = local_layout_path.file_name().unwrap();
        let remote_path = Path::new("/usr/src/zellij").join(layout_file_name); // TODO: not hardcoded
        send_local_file_to_remote(local_layout_path, remote_path.as_path());
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        let vte_parser = vte::Parser::new();
        let terminal_output = TerminalPane::new(0, win_size, Palette::default());
        setup_remote_environment(&mut channel, win_size);
        start_zellij_with_layout(&mut channel, &remote_path.to_string_lossy(), session_name.as_ref());
        RemoteRunner {
            steps: vec![],
            channel,
            terminal_output,
            vte_parser,
            session_name,
            test_name,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: 3,
            win_size,
            local_layout_path: Some(PathBuf::from(local_layout_path))
        }
    }
    pub fn add_step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }
    pub fn replace_steps(&mut self, steps: Vec<Step>) {
        self.steps = steps;
    }
    fn current_remote_terminal_state(&mut self) -> RemoteTerminal {
        let current_snapshot = self.get_current_snapshot();
        let (cursor_x, cursor_y) = self.terminal_output.cursor_coordinates().unwrap_or((0, 0));
        RemoteTerminal {
            cursor_x,
            cursor_y,
            current_snapshot,
            channel: &mut self.channel,
            session_name: self.session_name.as_ref(),
        }
    }
    pub fn run_next_step(&mut self) {
        if let Some(next_step) = self.steps.get(self.current_step_index) {
            let current_snapshot = take_snapshot(&mut self.terminal_output);
            let (cursor_x, cursor_y) = self.terminal_output.cursor_coordinates().unwrap_or((0, 0));
            let remote_terminal = RemoteTerminal {
                cursor_x,
                cursor_y,
                current_snapshot,
                channel: &mut self.channel,
                session_name: self.session_name.as_ref(),
            };
            let instruction = next_step.instruction;
            self.currently_running_step = Some(String::from(next_step.name));
            if instruction(remote_terminal) {
                self.current_step_index += 1;
            }
        }
    }
    pub fn steps_left(&self) -> bool {
        self.steps.get(self.current_step_index).is_some()
    }
    fn restart_test(&mut self) -> String {
        let session_name = self.session_name.as_ref().map(|name| {
            // this is so that we don't try to connect to the previous session if it's still stuck
            // inside the container
            format!("{}_{}", name, self.retries_left)
        });
        if let Some(local_layout_path) = self.local_layout_path.as_ref() {
            let mut new_runner = RemoteRunner::new_with_layout(self.test_name, self.win_size, Path::new(&local_layout_path), session_name);
            new_runner.retries_left = self.retries_left - 1;
            new_runner.replace_steps(self.steps.clone());
            drop(std::mem::replace(self, new_runner));
            self.run_all_steps()
        } else {
            let mut new_runner = RemoteRunner::new(self.test_name, self.win_size, session_name);
            new_runner.retries_left = self.retries_left - 1;
            new_runner.replace_steps(self.steps.clone());
            drop(std::mem::replace(self, new_runner));
            self.run_all_steps()
        }
    }
    fn display_informative_error(&mut self) {
        let test_name = self.test_name;
        let current_step_name = self.currently_running_step.as_ref().cloned();
        match current_step_name {
            Some(current_step) => {
                let remote_terminal = self.current_remote_terminal_state();
                eprintln!("Timed out waiting for data on the SSH channel for test {}. Was waiting for step: {}", test_name, current_step);
                eprintln!("{:?}", remote_terminal);
            }
            None => {
                let remote_terminal = self.current_remote_terminal_state();
                eprintln!("Timed out waiting for data on the SSH channel for test {}. Haven't begun running steps yet.", test_name);
                eprintln!("{:?}", remote_terminal);
            }
        }
    }
    pub fn run_all_steps(&mut self) -> String {
        // returns the last snapshot
        loop {
            let mut buf = [0u8; 1024];
            match self.channel.read(&mut buf) {
                Ok(0) => break,
                Ok(_count) => {
                    for byte in buf.iter() {
                        self.vte_parser
                            .advance(&mut self.terminal_output.grid, *byte);
                    }
                    self.run_next_step();
                    if !self.steps_left() {
                        break;
                    }
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::TimedOut {
                        if self.retries_left > 0 {
                            return self.restart_test();
                        }
                        self.display_informative_error();
                        panic!("Timed out waiting for test");
                    }
                    panic!("Error while reading remote session: {}", e);
                }
            }
        }
        take_snapshot(&mut self.terminal_output)
    }
    pub fn get_current_snapshot(&mut self) -> String {
        take_snapshot(&mut self.terminal_output)
    }
}

impl Drop for RemoteRunner {
    fn drop(&mut self) {
        self.channel.close().unwrap();
    }
}

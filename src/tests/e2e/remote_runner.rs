use zellij_tile::data::Palette;

use zellij_server::panes::TerminalPane;
use zellij_utils::pane_size::{Dimension, PaneGeom, Size};
use zellij_utils::{vte, zellij_tile};

use ssh2::Session;
use std::io::prelude::*;
use std::net::TcpStream;

use std::path::Path;

const ZELLIJ_EXECUTABLE_LOCATION: &str = "/usr/src/zellij/x86_64-unknown-linux-musl/debug/zellij";
const ZELLIJ_LAYOUT_PATH: &str = "/usr/src/zellij/fixtures/layouts";
const CONNECTION_STRING: &str = "127.0.0.1:2222";
const CONNECTION_USERNAME: &str = "test";
const CONNECTION_PASSWORD: &str = "test";
const SESSION_NAME: &str = "e2e-test";

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

fn setup_remote_environment(channel: &mut ssh2::Channel, win_size: Size) {
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

fn stop_zellij(channel: &mut ssh2::Channel) {
    channel
        .write_all("killall -KILL zellij\n".as_bytes())
        .unwrap();
}

fn start_zellij(channel: &mut ssh2::Channel) {
    stop_zellij(channel);
    channel
        .write_all(
            format!(
                "{} --session {}\n",
                ZELLIJ_EXECUTABLE_LOCATION, SESSION_NAME
            )
            .as_bytes(),
        )
        .unwrap();
    channel.flush().unwrap();
}

fn start_zellij_without_frames(channel: &mut ssh2::Channel) {
    stop_zellij(channel);
    channel
        .write_all(
            format!(
                "{} --session {} options --no-pane-frames\n",
                ZELLIJ_EXECUTABLE_LOCATION, SESSION_NAME
            )
            .as_bytes(),
        )
        .unwrap();
    channel.flush().unwrap();
}

fn start_zellij_with_layout(channel: &mut ssh2::Channel, layout_path: &str) {
    stop_zellij(channel);
    channel
        .write_all(
            format!(
                "{} --layout-path {} --session {}\n",
                ZELLIJ_EXECUTABLE_LOCATION, layout_path, SESSION_NAME
            )
            .as_bytes(),
        )
        .unwrap();
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
        self.current_snapshot.contains("Ctrl +")
    }
    pub fn snapshot_contains(&self, text: &str) -> bool {
        self.current_snapshot.contains(text)
    }
    #[allow(unused)]
    pub fn current_snapshot(&self) -> String {
        // convenience method for writing tests,
        // this should only be used when developing,
        // please prefer "snapsht_contains" instead
        self.current_snapshot.clone()
    }
    #[allow(unused)]
    pub fn current_cursor_position(&self) -> String {
        // convenience method for writing tests,
        // this should only be used when developing,
        // please prefer "cursor_position_is" instead
        format!("x: {}, y: {}", self.cursor_x, self.cursor_y)
    }
    pub fn send_key(&mut self, key: &[u8]) {
        self.channel.write_all(key).unwrap();
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
                format!("{} attach {}\n", ZELLIJ_EXECUTABLE_LOCATION, SESSION_NAME).as_bytes(),
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
    test_name: &'static str,
    currently_running_step: Option<String>,
    retries_left: usize,
    win_size: Size,
    layout_file_name: Option<&'static str>,
    without_frames: bool,
}

impl RemoteRunner {
    pub fn new(test_name: &'static str, win_size: Size) -> Self {
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        let vte_parser = vte::Parser::new();
        let mut rows = Dimension::fixed(win_size.rows);
        let mut cols = Dimension::fixed(win_size.cols);
        rows.set_inner(win_size.rows);
        cols.set_inner(win_size.cols);
        let pane_geom = PaneGeom {
            x: 0,
            y: 0,
            rows,
            cols,
        };
        let terminal_output = TerminalPane::new(0, pane_geom, Palette::default(), 0); // 0 is the pane index
        setup_remote_environment(&mut channel, win_size);
        start_zellij(&mut channel);
        RemoteRunner {
            steps: vec![],
            channel,
            terminal_output,
            vte_parser,
            test_name,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: 3,
            win_size,
            layout_file_name: None,
            without_frames: false,
        }
    }
    pub fn new_without_frames(test_name: &'static str, win_size: Size) -> Self {
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        let vte_parser = vte::Parser::new();
        let mut rows = Dimension::fixed(win_size.rows);
        let mut cols = Dimension::fixed(win_size.cols);
        rows.set_inner(win_size.rows);
        cols.set_inner(win_size.cols);
        let pane_geom = PaneGeom {
            x: 0,
            y: 0,
            rows,
            cols,
        };
        let terminal_output = TerminalPane::new(0, pane_geom, Palette::default(), 0); // 0 is the pane index
        setup_remote_environment(&mut channel, win_size);
        start_zellij_without_frames(&mut channel);
        RemoteRunner {
            steps: vec![],
            channel,
            terminal_output,
            vte_parser,
            test_name,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: 3,
            win_size,
            layout_file_name: None,
            without_frames: true,
        }
    }
    pub fn new_with_layout(
        test_name: &'static str,
        win_size: Size,
        layout_file_name: &'static str,
    ) -> Self {
        let remote_path = Path::new(ZELLIJ_LAYOUT_PATH).join(layout_file_name);
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        let vte_parser = vte::Parser::new();
        let mut rows = Dimension::fixed(win_size.rows);
        let mut cols = Dimension::fixed(win_size.cols);
        rows.set_inner(win_size.rows);
        cols.set_inner(win_size.cols);
        let pane_geom = PaneGeom {
            x: 0,
            y: 0,
            rows,
            cols,
        };
        let terminal_output = TerminalPane::new(0, pane_geom, Palette::default(), 0); // 0 is the pane index
        setup_remote_environment(&mut channel, win_size);
        start_zellij_with_layout(&mut channel, &remote_path.to_string_lossy());
        RemoteRunner {
            steps: vec![],
            channel,
            terminal_output,
            vte_parser,
            test_name,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: 3,
            win_size,
            layout_file_name: Some(layout_file_name),
            without_frames: false,
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
        if let Some(layout_file_name) = self.layout_file_name.as_ref() {
            // let mut new_runner = RemoteRunner::new_with_layout(self.test_name, self.win_size, Path::new(&local_layout_path), session_name);
            let mut new_runner =
                RemoteRunner::new_with_layout(self.test_name, self.win_size, layout_file_name);
            new_runner.retries_left = self.retries_left - 1;
            new_runner.replace_steps(self.steps.clone());
            drop(std::mem::replace(self, new_runner));
            self.run_all_steps()
        } else if self.without_frames {
            let mut new_runner = RemoteRunner::new_without_frames(self.test_name, self.win_size);
            new_runner.retries_left = self.retries_left - 1;
            new_runner.replace_steps(self.steps.clone());
            drop(std::mem::replace(self, new_runner));
            self.run_all_steps()
        } else {
            let mut new_runner = RemoteRunner::new(self.test_name, self.win_size);
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

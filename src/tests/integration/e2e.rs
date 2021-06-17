use ::insta::assert_snapshot;
use zellij_utils::pane_size::PositionAndSize;
use zellij_tile::data::Palette;

use rand::Rng;

use crate::tests::fakes::FakeInputOutput;
use crate::tests::start;
use crate::tests::utils::commands::{
    BRACKETED_PASTE_END, BRACKETED_PASTE_START, PANE_MODE, QUIT, SCROLL_DOWN_IN_SCROLL_MODE,
    SCROLL_MODE, SCROLL_PAGE_DOWN_IN_SCROLL_MODE, SCROLL_PAGE_UP_IN_SCROLL_MODE,
    SCROLL_UP_IN_SCROLL_MODE, SPAWN_TERMINAL_IN_PANE_MODE, SPLIT_DOWN_IN_PANE_MODE,
    SPLIT_RIGHT_IN_PANE_MODE, TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE, ESC, ENTER,
    TAB_MODE, NEW_TAB_IN_TAB_MODE, CLOSE_PANE_IN_PANE_MODE, CLOSE_TAB_IN_TAB_MODE, RESIZE_MODE, RESIZE_LEFT_IN_RESIZE_MODE, LOCK_MODE,
    SESSION_MODE, DETACH_IN_SESSION_MODE
};
use crate::tests::utils::{get_next_to_last_snapshot, get_output_frame_snapshots};
use crate::CliArgs;
use zellij_utils::input::config::Config;
use zellij_utils::{vte, zellij_tile};
use zellij_server::{panes::TerminalPane, tab::Pane};

use std::net::TcpStream;
use ssh2::Session;
use std::io::prelude::*;

const ZELLIJ_EXECUTABLE_LOCATION: &str = "/usr/src/zellij/x86_64-unknown-linux-musl/debug/zellij";
// const ZELLIJ_EXECUTABLE_LOCATION: &str = "/usr/src/zellij/zellij";
const CONNECTION_STRING: &str = "127.0.0.1:2222";
const CONNECTION_USERNAME: &str = "test";
const CONNECTION_PASSWORD: &str = "test";

fn ssh_connect() -> ssh2::Session {
    let tcp = TcpStream::connect(CONNECTION_STRING).unwrap();
    let mut sess = Session::new().unwrap();
    sess.set_tcp_stream(tcp);
    sess.handshake().unwrap();
    sess.userauth_password(CONNECTION_USERNAME, CONNECTION_PASSWORD).unwrap();
    sess
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

fn status_bar_appears(current_snapshot: &str) -> bool {
    current_snapshot.contains("Ctrl +") &&
        !current_snapshot.contains("─────") // this is a bug that happens because the app draws borders around the status bar momentarily on first render
}

fn tip_appears(current_snapshot: &str) -> bool {
    current_snapshot.contains("Tip:")
}


struct RemoteTerminal <'a>{
    channel: &'a mut ssh2::Channel,
    session_name: Option<&'a String>,
    cursor_x: usize,
    cursor_y: usize,
    current_snapshot: String,
}

impl<'a> std::fmt::Debug for RemoteTerminal <'a>{
    fn fmt (&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "cursor x: {}\ncursor_y: {}\ncurrent_snapshot:\n{}",
            self.cursor_x,
            self.cursor_y,
            self.current_snapshot
        )
    }
}

// impl fmt::Debug for RemoteTerminal {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//             f.debug_struct("Point")
//              .field("x", &self.x)
//              .field("y", &self.y)
//              .finish()
//         }
// }')}

impl<'a> RemoteTerminal <'a>{
    pub fn cursor_position_is(&self, x: usize, y: usize) -> bool {
        x == self.cursor_x && y == self.cursor_y
    }
    pub fn tip_appears(&self) -> bool {
        self.current_snapshot.contains("Tip:")
    }
    pub fn status_bar_appears(&self) -> bool {
        self.current_snapshot.contains("Ctrl +") &&
            !self.current_snapshot.contains("─────") // this is a bug that happens because the app draws borders around the status bar momentarily on first render
    }
    pub fn snapshot_contains(&self, text: &str) -> bool {
        self.current_snapshot.contains(text)
    }
    pub fn send_key (&mut self, key: &[u8]) {
        self.channel.write(key).unwrap();
        self.channel.flush().unwrap();
    }
    pub fn change_size (&mut self, cols: u32, rows: u32) {
        self.channel.request_pty_size(
            cols,
            rows,
            Some(cols),
            Some(rows),
        ).unwrap();
    }
    pub fn attach_to_original_session(&mut self) {
        self.channel.write_all(format!("{} attach {}\n", ZELLIJ_EXECUTABLE_LOCATION, self.session_name.unwrap()).as_bytes()).unwrap();
        self.channel.flush().unwrap();
    }
}

// struct SessionActions <'a>{
//     channel: &'a mut ssh2::Channel,
//     session_name: Option<&'a String>,
// }
// 
// impl<'a> SessionActions <'a>{
//     pub fn send_key (&mut self, key: &[u8]) {
//         self.channel.write(key).unwrap();
//         self.channel.flush().unwrap();
//     }
//     pub fn change_size (&mut self, cols: u32, rows: u32) {
//         self.channel.request_pty_size(
//             cols,
//             rows,
//             Some(cols),
//             Some(rows),
//         ).unwrap();
//     }
//     pub fn attach_to_original_session(&mut self) {
//         self.channel.write_all(format!("{} attach {}\n", ZELLIJ_EXECUTABLE_LOCATION, self.session_name.unwrap()).as_bytes()).unwrap();
//         self.channel.flush().unwrap();
//     }
// }

struct Step {
    // pub instruction: fn(SessionActions, SessionInfo) -> bool, // TODO: separate this to condition and Option<instruction> to make it clearer and not have ifs in the middle of everything
    pub instruction: fn(RemoteTerminal) -> bool, // TODO: separate this to condition and Option<instruction> to make it clearer and not have ifs in the middle of everything
    pub name: &'static str,
}

struct RemoteRunner {
    // remaining_steps: Vec<fn(SessionActions, SessionInfo) -> bool>,
    remaining_steps: Vec<Step>,
    vte_parser: vte::Parser,
    terminal_output: TerminalPane,
    channel: ssh2::Channel,
    session_name: Option<String>,
    test_name: &'static str,
    currently_running_step: Option<String>,
}

impl RemoteRunner {
    pub fn new(test_name: &'static str, win_size: PositionAndSize, session_name: Option<String>) -> Self {
        let sess = ssh_connect();
        sess.set_timeout(20000);
        let mut channel = sess.channel_session().unwrap();
        let (columns, rows) = (win_size.cols as u32, win_size.rows as u32);
		channel.request_pty(
		   "xterm",
			None,
			Some((columns, rows, 0, 0)),
		).unwrap();
        channel.shell().unwrap();
        channel.write_all(format!("export PS1=\"$ \"\n").as_bytes()).unwrap();
        channel.flush().unwrap();
        match session_name.as_ref() {
            Some(name) => {
                channel.write_all(format!("{} --session {}\n", ZELLIJ_EXECUTABLE_LOCATION, name).as_bytes()).unwrap();
            },
            None => {
                channel.write_all(format!("{}\n", ZELLIJ_EXECUTABLE_LOCATION).as_bytes()).unwrap();
            }
        };

        channel.flush().unwrap();

        let vte_parser = vte::Parser::new();
        let terminal_output = TerminalPane::new(0, win_size, Palette::default());
        RemoteRunner {
            remaining_steps: vec![],
            channel,
            terminal_output,
            vte_parser,
            session_name,
            test_name,
            currently_running_step: None,
        }
    }
    // pub fn add_step(&mut self, step: fn(SessionActions, SessionInfo) -> bool) {
    pub fn add_step(mut self, step: Step) -> Self {
        self.remaining_steps.push(step);
        self
    }
    fn current_remote_terminal_state (&mut self) -> RemoteTerminal {
        // let current_snapshot = take_snapshot(&mut self.terminal_output);
        let current_snapshot = self.get_current_snapshot();
        let (cursor_x, cursor_y) = self.terminal_output.cursor_coordinates().unwrap_or((0, 0));
        RemoteTerminal { cursor_x, cursor_y, current_snapshot, channel: &mut self.channel, session_name: self.session_name.as_ref() }
    }
    pub fn run_next_step(&mut self) {
        let current_snapshot = take_snapshot(&mut self.terminal_output);
        let (cursor_x, cursor_y) = self.terminal_output.cursor_coordinates().unwrap_or((0, 0));
        let next_step = self.remaining_steps.remove(0);
        let session_info = RemoteTerminal { cursor_x, cursor_y, current_snapshot, channel: &mut self.channel, session_name: self.session_name.as_ref() };
        // let session_actions = SessionActions { channel: &mut self.channel, session_name: self.session_name.as_ref() };
        // if !next_step(session_actions, session_info) {
        let instruction = next_step.instruction;
        self.currently_running_step = Some(String::from(next_step.name));
        // if !instruction(session_actions, session_info) {
        if !instruction(session_info) {
            self.remaining_steps.insert(0, next_step);
        }
    }
    pub fn steps_left(&self) -> bool {
        !self.remaining_steps.is_empty()
    }
    pub fn run_all_steps(&mut self) -> String { // returns the last snapshot
        let mut retries = 3;
        loop {
            let mut buf = [0u8; 1024];
            match self.channel.read(&mut buf) {
                Ok(0) => break,
                Ok(_count) => {
                    retries = 3;
                    for byte in buf.iter() {
                        self.vte_parser.advance(&mut self.terminal_output.grid, *byte);
                    }
                    self.run_next_step();
                    if !self.steps_left() {
                        break;
                    }
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::TimedOut {
                        if retries > 0 {
                            retries -= 1;
                            let remote_terminal = self.current_remote_terminal_state();
                            eprintln!("retrying: {:?}", remote_terminal);
                            // self.run_next_step();
                            continue;
                        }
                        let test_name = self.test_name;
                        let current_step_name = self.currently_running_step.as_ref().cloned();
                        // match self.currently_running_step.as_ref() {
                        match current_step_name {
                            Some(current_step) => {
                                let remote_terminal = self.current_remote_terminal_state();
                                eprintln!("Timed out waiting for data on the SSH channel for test {}. Was waiting for step: {}", test_name, current_step);
                                eprintln!("{:?}", remote_terminal);
                            },
                            None => {
                                let remote_terminal = self.current_remote_terminal_state();
                                eprintln!("Timed out waiting for data on the SSH channel for test {}. Haven't begun running steps yet.", test_name);
                                eprintln!("{:?}", remote_terminal);
                            }
                        }
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

#[test]
#[ignore]
pub fn starts_with_one_terminal() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("starts_with_one_terminal", fake_win_size, None)
        .add_step(Step {
            name: "Wait for app to load",
            // instruction: |_session_actions: SessionActions, session_info: SessionInfo| -> bool {
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn split_terminals_vertically() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };

    let last_snapshot = RemoteRunner::new("split_terminals_vertically", fake_win_size, None)
    .add_step(Step {
        name: "Split pane to the right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
            }
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .add_step(Step {
        name: "Wait for new pane to appear",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                // cursor is in the newly opened second pane
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn cannot_split_terminals_vertically_when_active_terminal_is_too_small() {
    let fake_win_size = PositionAndSize {
        cols: 8,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("cannot_split_terminals_vertically_when_active_terminal_is_too_small", fake_win_size, None)
    .add_step(Step {
        name: "Split pane to the right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .add_step(Step {
        name: "Send text to terminal",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            // this is just normal input that should be sent into the one terminal so that we can make
            // sure we silently failed to split in the previous step
            remote_terminal.send_key(&"Hi!".as_bytes());
            true
        }
    })
    .add_step(Step {
        name: "Wait for text to appear",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(5, 2) && remote_terminal.snapshot_contains("Hi!") {
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn scrolling_inside_a_pane() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("scrolling_inside_a_pane", fake_win_size, None)
    .add_step(Step {
        name: "Split pane to the right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
            }
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .add_step(Step {
        name: "Fill terminal with text",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                // cursor is in the newly opened second pane
                remote_terminal.send_key(&format!("{:0<57}", "line1 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line2 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line3 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line4 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line5 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line6 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line7 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line8 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line9 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line10 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line11 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line12 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line13 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line14 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line15 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line16 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line17 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line18 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<59}", "line19 ").as_bytes());
                remote_terminal.send_key(&format!("{:0<58}", "line20 ").as_bytes());
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Scroll up inside pane",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(119, 20) {
                // all lines have been written to the pane
                remote_terminal.send_key(&SCROLL_MODE);
                remote_terminal.send_key(&SCROLL_UP_IN_SCROLL_MODE);
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Wait for scroll to finish",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(119, 20) && remote_terminal.snapshot_contains("line1 ") {
                // scrolled up one line
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn toggle_pane_fullscreen() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("toggle_pane_fullscreen", fake_win_size, None)
    .add_step(Step {
        name: "Split pane to the right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
            }
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .add_step(Step {
        name: "Change newly opened pane to be fullscreen",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                // cursor is in the newly opened second pane
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE);
                // back to normal mode after toggling fullscreen
                remote_terminal.send_key(&ENTER);
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Wait for pane to become fullscreen",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(2, 0) {
                // cursor is in full screen pane now
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}
 
#[test]
#[ignore]
pub fn open_new_tab() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("open_new_tab", fake_win_size, None)
    .add_step(Step {
        name: "Split pane to the right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
            }
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .add_step(Step {
        name: "Open new tab",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                // cursor is in the newly opened second pane
                remote_terminal.send_key(&TAB_MODE);
                remote_terminal.send_key(&NEW_TAB_IN_TAB_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Wait for new tab to open",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(2, 2) &&
                remote_terminal.tip_appears() &&
                remote_terminal.snapshot_contains("Tab #2") &&
                remote_terminal.status_bar_appears() {
                // cursor is in the newly opened second tab
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn close_pane() {
    // TODO: CONTINUE HERE - make this past when running alone with the retries and stuff
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("close_pane", fake_win_size, None)
    .add_step(Step {
        name: "Split pane to the right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
            }
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .add_step(Step {
        name: "Close pane",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                // cursor is in the newly opened second pane
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&CLOSE_PANE_IN_PANE_MODE);
                // back to normal mode after close
                remote_terminal.send_key(&ENTER);
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Wait for pane to close",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(2, 2) && remote_terminal.tip_appears() {
                // cursor is in the original pane
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn exit_zellij() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("exit_zellij", fake_win_size, None)
    .add_step(Step {
        name: "Wait for app to load",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&QUIT);
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Wait for app to exit",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if !remote_terminal.status_bar_appears() && remote_terminal.snapshot_contains("Bye from Zellij!") {
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert!(last_snapshot.contains("Bye from Zellij!"));
}

#[test]
#[ignore]
pub fn closing_last_pane_exits_zellij() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("closing_last_pane_exits_zellij", fake_win_size, None)
    .add_step(Step {
        name: "Close pane",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&CLOSE_PANE_IN_PANE_MODE);
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Wait for app to exit",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.snapshot_contains("Bye from Zellij!") {
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert!(last_snapshot.contains("Bye from Zellij!"));
}

#[test]
#[ignore]
pub fn resize_pane() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("resize_pane", fake_win_size, None)
    .add_step(Step {
        name: "Split pane to the right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
            }
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .add_step(Step {
        name: "Resize pane",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                // cursor is in the newly opened second pane
                remote_terminal.send_key(&RESIZE_MODE);
                remote_terminal.send_key(&RESIZE_LEFT_IN_RESIZE_MODE);
                // back to normal mode after resizing
                remote_terminal.send_key(&ENTER);
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Wait for pane to be resized",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(53, 2) && remote_terminal.tip_appears() {
                // pane has been resized
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn lock_mode() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("lock_mode", fake_win_size, None)
    .add_step(Step {
        name: "Enter lock mode",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&LOCK_MODE);
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Send keys that should not be intercepted by the app",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.snapshot_contains("INTERFACE LOCKED") {
                remote_terminal.send_key(&TAB_MODE);
                remote_terminal.send_key(&NEW_TAB_IN_TAB_MODE);
                remote_terminal.send_key(&"abc".as_bytes());
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Wait for terminal to render sent keys",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(6, 2) {
                // text has been entered into the only terminal pane
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn resize_terminal_window() {
    // this checks the resizing of the whole terminal window (reaction to SIGWINCH) and not just one pane
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("resize_terminal_window", fake_win_size, None)
    .add_step(Step {
        name: "Split pane to the right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
            }
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .add_step(Step {
        name: "Change terminal window size",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                // new pane has been opened and focused
                remote_terminal.change_size(100, 24);
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "wait for terminal to be resized and app to be re-rendered",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(43, 2) && remote_terminal.tip_appears() {
                // size has been changed
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn detach_and_attach_session() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let session_id = rand::thread_rng().gen_range(0..10000);
    let session_name = format!("session_{}", session_id);
    let last_snapshot = RemoteRunner::new("detach_and_attach_session", fake_win_size, Some(session_name))
    .add_step(Step {
        name: "Split pane to the right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
            }
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .add_step(Step {
        name: "Send some text to the active pane",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                // new pane has been opened and focused
                remote_terminal.send_key(&"I am some text".as_bytes());
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Detach session",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(77, 2) {
                remote_terminal.send_key(&SESSION_MODE);
                remote_terminal.send_key(&DETACH_IN_SESSION_MODE);
                // text has been entered
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Reattach session",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if !remote_terminal.status_bar_appears() {
                // we don't see the toolbar, so we can assume we've already detached
                remote_terminal.attach_to_original_session();
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .add_step(Step {
        name: "Wait for session to be attached",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(77, 2) {
                // we're back inside the session
                step_is_complete = true;
            }
            step_is_complete
        }
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}

// // #[test]
// // pub fn split_terminals_vertically() {
// //     let fake_win_size = PositionAndSize {
// //         cols: 121,
// //         rows: 20,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[&PANE_MODE, &SPLIT_RIGHT_IN_PANE_MODE, &QUIT]);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         CliArgs::default(),
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn split_terminals_horizontally() {
// //     let fake_win_size = PositionAndSize {
// //         cols: 121,
// //         rows: 20,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[&PANE_MODE, &SPLIT_DOWN_IN_PANE_MODE, &QUIT]);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         CliArgs::default(),
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn split_largest_terminal() {
// //     // this finds the largest pane and splits along its longest edge (vertically or horizontally)
// //     let fake_win_size = PositionAndSize {
// //         cols: 121,
// //         rows: 20,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[
// //         &PANE_MODE,
// //         &SPAWN_TERMINAL_IN_PANE_MODE,
// //         &SPAWN_TERMINAL_IN_PANE_MODE,
// //         &SPAWN_TERMINAL_IN_PANE_MODE,
// //         &QUIT,
// //     ]);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         CliArgs::default(),
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn cannot_split_terminals_vertically_when_active_terminal_is_too_small() {
// //     let fake_win_size = PositionAndSize {
// //         cols: 8,
// //         rows: 20,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[&PANE_MODE, &SPLIT_RIGHT_IN_PANE_MODE, &QUIT]);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         CliArgs::default(),
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn cannot_split_terminals_horizontally_when_active_terminal_is_too_small() {
// //     let fake_win_size = PositionAndSize {
// //         cols: 121,
// //         rows: 4,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[&PANE_MODE, &SPLIT_DOWN_IN_PANE_MODE, &QUIT]);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         CliArgs::default(),
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn cannot_split_largest_terminal_when_there_is_no_room() {
// //     let fake_win_size = PositionAndSize {
// //         cols: 8,
// //         rows: 4,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[&PANE_MODE, &SPAWN_TERMINAL_IN_PANE_MODE, &QUIT]);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         CliArgs::default(),
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn scrolling_up_inside_a_pane() {
// //     let fake_win_size = PositionAndSize {
// //         cols: 121,
// //         rows: 20,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[
// //         &PANE_MODE,
// //         &SPLIT_DOWN_IN_PANE_MODE,
// //         &SPLIT_RIGHT_IN_PANE_MODE,
// //         &SCROLL_MODE,
// //         &SCROLL_UP_IN_SCROLL_MODE,
// //         &SCROLL_UP_IN_SCROLL_MODE,
// //         &QUIT,
// //     ]);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         CliArgs::default(),
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn scrolling_down_inside_a_pane() {
// //     let fake_win_size = PositionAndSize {
// //         cols: 121,
// //         rows: 20,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[
// //         &PANE_MODE,
// //         &SPLIT_DOWN_IN_PANE_MODE,
// //         &SPLIT_RIGHT_IN_PANE_MODE,
// //         &SCROLL_MODE,
// //         &SCROLL_UP_IN_SCROLL_MODE,
// //         &SCROLL_UP_IN_SCROLL_MODE,
// //         &SCROLL_DOWN_IN_SCROLL_MODE,
// //         &SCROLL_DOWN_IN_SCROLL_MODE,
// //         &QUIT,
// //     ]);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         CliArgs::default(),
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn scrolling_page_up_inside_a_pane() {
// //     let fake_win_size = PositionAndSize {
// //         cols: 121,
// //         rows: 20,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[
// //         &PANE_MODE,
// //         &SPLIT_DOWN_IN_PANE_MODE,
// //         &SPLIT_RIGHT_IN_PANE_MODE,
// //         &SCROLL_MODE,
// //         &SCROLL_PAGE_UP_IN_SCROLL_MODE,
// //         &QUIT,
// //     ]);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         CliArgs::default(),
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn scrolling_page_down_inside_a_pane() {
// //     let fake_win_size = PositionAndSize {
// //         cols: 121,
// //         rows: 20,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[
// //         &PANE_MODE,
// //         &SPLIT_DOWN_IN_PANE_MODE,
// //         &SPLIT_RIGHT_IN_PANE_MODE,
// //         &SCROLL_MODE,
// //         &SCROLL_PAGE_UP_IN_SCROLL_MODE,
// //         &SCROLL_PAGE_UP_IN_SCROLL_MODE,
// //         &SCROLL_PAGE_DOWN_IN_SCROLL_MODE,
// //         &SCROLL_PAGE_DOWN_IN_SCROLL_MODE,
// //         &QUIT,
// //     ]);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         CliArgs::default(),
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn max_panes() {
// //     // with the --max-panes option, we only allow a certain amount of panes on screen
// //     // simultaneously, new panes beyond this limit will close older panes on screen
// //     let fake_win_size = PositionAndSize {
// //         cols: 121,
// //         rows: 20,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[
// //         &PANE_MODE,
// //         &SPAWN_TERMINAL_IN_PANE_MODE,
// //         &SPAWN_TERMINAL_IN_PANE_MODE,
// //         &SPAWN_TERMINAL_IN_PANE_MODE,
// //         &SPAWN_TERMINAL_IN_PANE_MODE,
// //         &QUIT,
// //     ]);
// //     let mut opts = CliArgs::default();
// //     opts.max_panes = Some(4);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         opts,
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn toggle_focused_pane_fullscreen() {
// //     let fake_win_size = PositionAndSize {
// //         cols: 121,
// //         rows: 20,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[
// //         &PANE_MODE,
// //         &SPAWN_TERMINAL_IN_PANE_MODE,
// //         &SPAWN_TERMINAL_IN_PANE_MODE,
// //         &SPAWN_TERMINAL_IN_PANE_MODE,
// //         &TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE,
// //         &QUIT,
// //     ]);
// //     let mut opts = CliArgs::default();
// //     opts.max_panes = Some(4);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         opts,
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }
// // 
// // #[test]
// // pub fn bracketed_paste() {
// //     // bracketed paste (https://xfree86.org/current/ctlseqs.html#Bracketed%20Paste%20Mode)
// //     // makes sure that text the user pastes is not interpreted as commands by the running program
// //     // (zellij in this case)
// //     // this tests makes sure the "SPLIT_RIGHT_IN_PANE_MODE" command is not interpreted as Zellij,
// //     // since it's inside a bracketed paste block, while the "QUIT" command is, since it is already
// //     // past the block
// //     let fake_win_size = PositionAndSize {
// //         cols: 121,
// //         rows: 20,
// //         x: 0,
// //         y: 0,
// //         ..Default::default()
// //     };
// //     let mut fake_input_output = get_fake_os_input(&fake_win_size);
// //     fake_input_output.add_terminal_input(&[
// //         &PANE_MODE,
// //         &BRACKETED_PASTE_START,
// //         &SPLIT_RIGHT_IN_PANE_MODE,
// //         &BRACKETED_PASTE_END,
// //         &QUIT,
// //     ]);
// //     start(
// //         Box::new(fake_input_output.clone()),
// //         CliArgs::default(),
// //         Box::new(fake_input_output.clone()),
// //         Config::default(),
// //     );
// //     let output_frames = fake_input_output
// //         .stdout_writer
// //         .output_frames
// //         .lock()
// //         .unwrap();
// //     let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
// //     let snapshot_before_quit =
// //         get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
// //     assert_snapshot!(snapshot_before_quit);
// // }

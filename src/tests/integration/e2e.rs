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


struct SessionInfo {
    cursor_x: usize,
    cursor_y: usize,
    current_snapshot: String,
}

struct SessionActions <'a>{
    channel: &'a mut ssh2::Channel,
    session_name: Option<&'a String>,
}

impl<'a> SessionActions <'a>{
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

struct RemoteRunner {
    // remaining_steps: Vec<fn(&mut dyn FnMut (&[u8]), &mut dyn FnMut (u32, u32), SessionInfo) -> bool>,
    remaining_steps: Vec<fn(SessionActions, SessionInfo) -> bool>,
    vte_parser: vte::Parser,
    terminal_output: TerminalPane,
    channel: ssh2::Channel,
    session_name: Option<String>,
}

impl RemoteRunner {
    pub fn new(win_size: PositionAndSize, session_name: Option<String>) -> Self {
        let sess = ssh_connect();
        sess.set_timeout(10000);
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
            session_name
        }
    }
    // pub fn add_step(&mut self, step: fn(&mut dyn FnMut (&[u8]), &mut dyn FnMut(u32, u32), SessionInfo) -> bool) {
    pub fn add_step(&mut self, step: fn(SessionActions, SessionInfo) -> bool) {
        self.remaining_steps.push(step);
    }
    pub fn run_next_step(&mut self) {
        let current_snapshot = take_snapshot(&mut self.terminal_output);
        let (cursor_x, cursor_y) = self.terminal_output.cursor_coordinates().unwrap_or((0, 0));
        let next_step = self.remaining_steps.remove(0);
        let session_info = SessionInfo { cursor_x, cursor_y, current_snapshot };
        let session_actions = SessionActions { channel: &mut self.channel, session_name: self.session_name.as_ref() };
        if !next_step(session_actions, session_info) {
            self.remaining_steps.insert(0, next_step);
        }
    }
    pub fn steps_left(&self) -> bool {
        !self.remaining_steps.is_empty()
    }
    pub fn run_all_steps(&mut self) {
        loop {
            let mut buf = [0u8; 1024];
            match self.channel.read(&mut buf) {
                Ok(0) => break,
                Ok(_count) => {
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
                        let current_snapshot = self.get_current_snapshot();
                        eprintln!("Timed out waiting for data on the SSH channel. Current snapshot:\n{}", current_snapshot);
                        std::process::exit(1);
                    }
                    panic!("Error while reading remote session: {}", e);
                }
            }
        }
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
    let mut remote_runner = RemoteRunner::new(fake_win_size, None);

    remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
        let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
        let mut step_is_complete = false;
        // if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
        if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 222 {
            step_is_complete = true;
        }
        step_is_complete
    });
    remote_runner.run_all_steps();
    let last_snapshot = remote_runner.get_current_snapshot();
    assert_snapshot!(last_snapshot);
}

// #[test]
// #[ignore]
// pub fn split_terminals_vertically() {
//     let fake_win_size = PositionAndSize {
//         cols: 120,
//         rows: 24,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let mut remote_runner = RemoteRunner::new(fake_win_size, None);
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
//             // back to normal mode after split
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 63 && cursor_y == 2 && tip_appears(&current_snapshot) {
//             // cursor is in the newly opened second pane
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert_snapshot!(last_snapshot);
// }
// 
// #[test]
// #[ignore]
// pub fn cannot_split_terminals_vertically_when_active_terminal_is_too_small() {
//     let fake_win_size = PositionAndSize {
//         cols: 8,
//         rows: 20,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let mut remote_runner = RemoteRunner::new(fake_win_size, None);
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
//             // back to normal mode after failing to split
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, _session_info: SessionInfo| -> bool {
//         // this is just normal input that should be sent into the one terminal so that we can make
//         // sure we silently failed to split in the previous step
//         session_actions.send_key(&"Hi!".as_bytes());
//         true
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 5 && cursor_y == 2 && current_snapshot.contains("Hi!") {
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert_snapshot!(last_snapshot);
// }
// 
// #[test]
// #[ignore]
// pub fn scrolling_inside_a_pane() {
//     let fake_win_size = PositionAndSize {
//         cols: 120,
//         rows: 24,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let mut remote_runner = RemoteRunner::new(fake_win_size, None);
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
//             // back to normal mode after splitting
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 63 && cursor_y == 2 && tip_appears(&current_snapshot){
//             // cursor is in the newly opened second pane
//             session_actions.send_key(&format!("{:0<57}", "line1 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line2 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line3 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line4 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line5 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line6 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line7 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line8 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line9 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line10 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line11 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line12 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line13 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line14 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line15 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line16 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line17 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line18 ").as_bytes());
//             session_actions.send_key(&format!("{:0<59}", "line19 ").as_bytes());
//             session_actions.send_key(&format!("{:0<58}", "line20 ").as_bytes());
//          step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y) = (session_info.cursor_x, session_info.cursor_y);
//         let mut step_is_complete = false;
//         if cursor_x == 119 && cursor_y == 20 {
//             // all lines have been written to the pane
//             session_actions.send_key(&SCROLL_MODE);
//             session_actions.send_key(&SCROLL_UP_IN_SCROLL_MODE);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 119 && cursor_y == 20 && current_snapshot.contains("line1 ") {
//             // scrolled up one line
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert_snapshot!(last_snapshot);
// }
// 
// #[test]
// #[ignore]
// pub fn toggle_pane_fullscreen() {
//     let fake_win_size = PositionAndSize {
//         cols: 120,
//         rows: 24,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let mut remote_runner = RemoteRunner::new(fake_win_size, None);
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
//             // back to normal mode after failing to split
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 63 && cursor_y == 2 && tip_appears(&current_snapshot) {
//             // cursor is in the newly opened second pane
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE);
//             // back to normal mode after toggling fullscreen
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y) = (session_info.cursor_x, session_info.cursor_y);
//         let mut step_is_complete = false;
//         if cursor_x == 2 && cursor_y == 0 {
//             // cursor is in full screen pane now
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert_snapshot!(last_snapshot);
// }
// 
// #[test]
// #[ignore]
// pub fn open_new_tab() {
//     let fake_win_size = PositionAndSize {
//         cols: 120,
//         rows: 24,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let mut remote_runner = RemoteRunner::new(fake_win_size, None);
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
//             // back to normal mode after split
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 63 && cursor_y == 2 && tip_appears(&current_snapshot) {
//             // cursor is in the newly opened second pane
//             session_actions.send_key(&TAB_MODE);
//             session_actions.send_key(&NEW_TAB_IN_TAB_MODE);
//             // back to normal mode after split
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 2 && cursor_y == 2 && tip_appears(&current_snapshot) && current_snapshot.contains("Tab #2") && status_bar_appears(&current_snapshot) {
//             // cursor is in the newly opened second tab
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert_snapshot!(last_snapshot);
// }
// 
// #[test]
// #[ignore]
// pub fn close_pane() {
//     let fake_win_size = PositionAndSize {
//         cols: 120,
//         rows: 24,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let mut remote_runner = RemoteRunner::new(fake_win_size, None);
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
//             // back to normal mode after split
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 63 && cursor_y == 2 && tip_appears(&current_snapshot) {
//             // cursor is in the newly opened second pane
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&CLOSE_PANE_IN_PANE_MODE);
//             // back to normal mode after close
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 2 && cursor_y == 2 && tip_appears(&current_snapshot) {
//             // cursor is in the newly opened second tab
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert_snapshot!(last_snapshot);
// }
// 
// #[test]
// #[ignore]
// pub fn exit_zellij() {
//     let fake_win_size = PositionAndSize {
//         cols: 120,
//         rows: 24,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let mut remote_runner = RemoteRunner::new(fake_win_size, None);
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&QUIT);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let mut step_is_complete = false;
//         if session_info.current_snapshot.contains("Bye from Zellij!") {
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert!(last_snapshot.contains("Bye from Zellij!"));
// }
// 
// #[test]
// #[ignore]
// pub fn closing_last_pane_exits_zellij() {
//     let fake_win_size = PositionAndSize {
//         cols: 120,
//         rows: 24,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let mut remote_runner = RemoteRunner::new(fake_win_size, None);
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&CLOSE_PANE_IN_PANE_MODE);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let mut step_is_complete = false;
//         if session_info.current_snapshot.contains("Bye from Zellij!") {
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert!(last_snapshot.contains("Bye from Zellij!"));
// }
// 
// #[test]
// #[ignore]
// pub fn resize_pane() {
//     let fake_win_size = PositionAndSize {
//         cols: 120,
//         rows: 24,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let mut remote_runner = RemoteRunner::new(fake_win_size, None);
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
//             // back to normal mode after failing to split
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 63 && cursor_y == 2 && tip_appears(&current_snapshot) {
//             // cursor is in the newly opened second pane
//             session_actions.send_key(&RESIZE_MODE);
//             session_actions.send_key(&RESIZE_LEFT_IN_RESIZE_MODE);
//             // back to normal mode after resizing
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 53 && cursor_y == 2 && tip_appears(&current_snapshot) {
//             // pane has been resized
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert_snapshot!(last_snapshot);
// }
// 
// #[test]
// #[ignore]
// pub fn lock_mode() {
//     let fake_win_size = PositionAndSize {
//         cols: 120,
//         rows: 24,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let mut remote_runner = RemoteRunner::new(fake_win_size, None);
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&LOCK_MODE);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if current_snapshot.contains("INTERFACE LOCKED") {
//             session_actions.send_key(&TAB_MODE);
//             session_actions.send_key(&NEW_TAB_IN_TAB_MODE);
//             session_actions.send_key(&"abc".as_bytes());
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y) = (session_info.cursor_x, session_info.cursor_y);
//         let mut step_is_complete = false;
//         if cursor_x == 6 && cursor_y == 2 {
//             // text has been entered into the only terminal pane
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert_snapshot!(last_snapshot);
// }
// 
// #[test]
// #[ignore]
// pub fn resize_terminal_window() {
//     // this checks the resizing of the whole terminal window (reaction to SIGWINCH) and not just one pane
//     let fake_win_size = PositionAndSize {
//         cols: 120,
//         rows: 24,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let mut remote_runner = RemoteRunner::new(fake_win_size, None);
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 63 && cursor_y == 2 && tip_appears(&current_snapshot) {
//             // new pane has been opened and focused
//             session_actions.change_size(100, 24);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 43 && cursor_y == 2 && tip_appears(&current_snapshot) {
//             // size has been changed
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert_snapshot!(last_snapshot);
// }
// 
// #[test]
// #[ignore]
// pub fn detach_and_attach_session() {
//     let fake_win_size = PositionAndSize {
//         cols: 120,
//         rows: 24,
//         x: 0,
//         y: 0,
//         ..Default::default()
//     };
//     let session_id = rand::thread_rng().gen_range(0..10000);
//     let session_name = format!("session_{}", session_id);
//     let mut remote_runner = RemoteRunner::new(fake_win_size, Some(session_name));
// 
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if status_bar_appears(&current_snapshot) && cursor_x == 2 && cursor_y == 2 {
//             session_actions.send_key(&PANE_MODE);
//             session_actions.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
//             session_actions.send_key(&ENTER);
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y, current_snapshot) = (session_info.cursor_x, session_info.cursor_y, session_info.current_snapshot);
//         let mut step_is_complete = false;
//         if cursor_x == 63 && cursor_y == 2 && tip_appears(&current_snapshot) {
//             // new pane has been opened and focused
//             session_actions.send_key(&"I am some text".as_bytes());
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y) = (session_info.cursor_x, session_info.cursor_y);
//         let mut step_is_complete = false;
//         if cursor_x == 77 && cursor_y == 2 {
//             session_actions.send_key(&SESSION_MODE);
//             session_actions.send_key(&DETACH_IN_SESSION_MODE);
//             // text has been entered
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|mut session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let mut step_is_complete = false;
//         if !status_bar_appears(&session_info.current_snapshot) {
//             // we don't see the toolbar, so we can assume we've already detached
//             session_actions.attach_to_original_session();
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.add_step(|_session_actions: SessionActions, session_info: SessionInfo| -> bool {
//         let (cursor_x, cursor_y) = (session_info.cursor_x, session_info.cursor_y);
//         let mut step_is_complete = false;
//         if cursor_x == 77 && cursor_y == 2 {
//             // we're back inside the session
//             step_is_complete = true;
//         }
//         step_is_complete
//     });
//     remote_runner.run_all_steps();
//     let last_snapshot = remote_runner.get_current_snapshot();
//     assert_snapshot!(last_snapshot);
//     assert!(1 == 1);
// }
// 
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

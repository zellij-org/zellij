use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use zellij_server::panes::sixel::SixelImageStore;
use zellij_server::panes::{LinkHandler, TerminalPane};
use zellij_utils::data::{Palette, Style};
use zellij_utils::pane_size::{Dimension, PaneGeom, Size, SizeInPixels};
use zellij_utils::vte;

use ssh2::Session;
use std::io::prelude::*;
use std::net::TcpStream;

use std::path::Path;

use std::cell::RefCell;
use std::rc::Rc;

const ZELLIJ_EXECUTABLE_LOCATION: &str = "/usr/src/zellij/x86_64-unknown-linux-musl/release/zellij";
const SET_ENV_VARIABLES: &str = "EDITOR=/usr/bin/vi";
const ZELLIJ_CONFIG_PATH: &str = "/usr/src/zellij/fixtures/configs";
const ZELLIJ_DATA_DIR: &str = "/usr/src/zellij/e2e-data";
const ZELLIJ_FIXTURE_PATH: &str = "/usr/src/zellij/fixtures";
const CONNECTION_STRING: &str = "127.0.0.1:2222";
const CONNECTION_USERNAME: &str = "test";
const CONNECTION_PASSWORD: &str = "test";
const SESSION_NAME: &str = "e2e-test";
const RETRIES: usize = 10;

fn ssh_connect() -> ssh2::Session {
    let tcp = TcpStream::connect(CONNECTION_STRING).unwrap();
    let mut sess = Session::new().unwrap();
    sess.set_tcp_stream(tcp);
    sess.handshake().unwrap();
    sess.userauth_password(CONNECTION_USERNAME, CONNECTION_PASSWORD)
        .unwrap();
    sess
}

fn ssh_connect_without_timeout() -> ssh2::Session {
    let tcp = TcpStream::connect(CONNECTION_STRING).unwrap();
    let mut sess = Session::new().unwrap();
    sess.set_tcp_stream(tcp);
    sess.handshake().unwrap();
    sess.userauth_password(CONNECTION_USERNAME, CONNECTION_PASSWORD)
        .unwrap();
    sess
}

fn setup_remote_environment(channel: &mut ssh2::Channel, win_size: Size) {
    let (columns, rows) = (win_size.cols as u32, win_size.rows as u32);
    channel
        .request_pty("xterm", None, Some((columns, rows, 0, 0)))
        .unwrap();
    channel.shell().unwrap();
    channel.write_all(b"export PS1=\"$ \"\n").unwrap();
    channel.flush().unwrap();
}

fn stop_zellij(channel: &mut ssh2::Channel) {
    // here we remove the status-bar-tips cache to make sure only the quicknav tip is loaded
    channel
        .write_all(b"find /tmp | grep status-bar-tips | xargs rm\n")
        .unwrap();
    channel.write_all(b"killall -KILL zellij\n").unwrap();
    channel.write_all(b"rm -rf /tmp/*\n").unwrap(); // remove temporary artifacts from previous
                                                    // tests
    channel.write_all(b"rm -rf /tmp/*\n").unwrap(); // remove temporary artifacts from previous
    channel
        .write_all(b"rm -rf ~/.cache/zellij/*/session_info\n")
        .unwrap();
}

fn start_zellij(channel: &mut ssh2::Channel) {
    stop_zellij(channel);
    channel
        .write_all(
            format!(
                "{} {} --session {} --data-dir {}\n",
                SET_ENV_VARIABLES, ZELLIJ_EXECUTABLE_LOCATION, SESSION_NAME, ZELLIJ_DATA_DIR
            )
            .as_bytes(),
        )
        .unwrap();
    channel.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1)); // wait until Zellij stops parsing startup ANSI codes from the terminal STDIN
}

fn start_zellij_mirrored_session(channel: &mut ssh2::Channel) {
    stop_zellij(channel);
    channel
        .write_all(
            format!(
                "{} {} --session {} --data-dir {} options --mirror-session true --serialization-interval 1\n",
                SET_ENV_VARIABLES, ZELLIJ_EXECUTABLE_LOCATION, SESSION_NAME, ZELLIJ_DATA_DIR
            )
            .as_bytes(),
        )
        .unwrap();
    channel.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1)); // wait until Zellij stops parsing startup ANSI codes from the terminal STDIN
}

fn start_zellij_mirrored_session_with_layout(channel: &mut ssh2::Channel, layout_file_name: &str) {
    stop_zellij(channel);
    channel
        .write_all(
            format!(
                "{} {} --session {} --data-dir {} --layout {} options --mirror-session true --serialization-interval 1\n",
                SET_ENV_VARIABLES,
                ZELLIJ_EXECUTABLE_LOCATION,
                SESSION_NAME,
                ZELLIJ_DATA_DIR,
                format!("{}/{}", ZELLIJ_FIXTURE_PATH, layout_file_name)
            )
            .as_bytes(),
        )
        .unwrap();
    channel.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1)); // wait until Zellij stops parsing startup ANSI codes from the terminal STDIN
}

fn start_zellij_mirrored_session_with_layout_and_viewport_serialization(
    channel: &mut ssh2::Channel,
    layout_file_name: &str,
) {
    stop_zellij(channel);
    channel
        .write_all(
            format!(
                "{} {} --session {} --data-dir {} --layout {} options --mirror-session true --serialize-pane-viewport true --serialization-interval 1\n",
                SET_ENV_VARIABLES,
                ZELLIJ_EXECUTABLE_LOCATION,
                SESSION_NAME,
                ZELLIJ_DATA_DIR,
                format!("{}/{}", ZELLIJ_FIXTURE_PATH, layout_file_name)
            )
            .as_bytes(),
        )
        .unwrap();
    channel.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1)); // wait until Zellij stops parsing startup ANSI codes from the terminal STDIN
}

fn start_zellij_in_session(channel: &mut ssh2::Channel, session_name: &str, mirrored: bool) {
    stop_zellij(channel);
    channel
        .write_all(
            format!(
                "{} {} --session {} --data-dir {} options --mirror-session {}\n",
                SET_ENV_VARIABLES,
                ZELLIJ_EXECUTABLE_LOCATION,
                session_name,
                ZELLIJ_DATA_DIR,
                mirrored
            )
            .as_bytes(),
        )
        .unwrap();
    channel.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1)); // wait until Zellij stops parsing startup ANSI codes from the terminal STDIN
}

fn attach_to_existing_session(channel: &mut ssh2::Channel, session_name: &str) {
    channel
        .write_all(
            format!(
                "{} {} attach {}\n",
                SET_ENV_VARIABLES, ZELLIJ_EXECUTABLE_LOCATION, session_name
            )
            .as_bytes(),
        )
        .unwrap();
    channel.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1)); // wait until Zellij stops parsing startup ANSI codes from the terminal STDIN
}

fn start_zellij_without_frames(channel: &mut ssh2::Channel) {
    stop_zellij(channel);
    channel
        .write_all(
            format!(
                "{} {} --session {} --data-dir {} options --no-pane-frames\n",
                SET_ENV_VARIABLES, ZELLIJ_EXECUTABLE_LOCATION, SESSION_NAME, ZELLIJ_DATA_DIR
            )
            .as_bytes(),
        )
        .unwrap();
    channel.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1)); // wait until Zellij stops parsing startup ANSI codes from the terminal STDIN
}

fn start_zellij_with_config(channel: &mut ssh2::Channel, config_path: &str) {
    stop_zellij(channel);
    channel
        .write_all(
            format!(
                "{} {} --config {} --session {} --data-dir {}\n",
                SET_ENV_VARIABLES,
                ZELLIJ_EXECUTABLE_LOCATION,
                config_path,
                SESSION_NAME,
                ZELLIJ_DATA_DIR
            )
            .as_bytes(),
        )
        .unwrap();
    channel.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1)); // wait until Zellij stops parsing startup ANSI codes from the terminal STDIN
}

fn read_from_channel(
    channel: &Arc<Mutex<ssh2::Channel>>,
    last_snapshot: &Arc<Mutex<String>>,
    cursor_coordinates: &Arc<Mutex<(usize, usize)>>,
    pane_geom: &PaneGeom,
) -> (Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let should_keep_running = Arc::new(AtomicBool::new(true));
    let thread = std::thread::Builder::new()
        .name("read_thread".into())
        .spawn({
            let pane_geom = *pane_geom;
            let should_keep_running = should_keep_running.clone();
            let channel = channel.clone();
            let last_snapshot = last_snapshot.clone();
            let cursor_coordinates = cursor_coordinates.clone();
            move || {
                let mut retries_left = 3;
                let mut should_sleep = false;
                let mut vte_parser = vte::Parser::new();
                let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
                    height: 21,
                    width: 8,
                })));
                let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
                let debug = false;
                let arrow_fonts = true;
                let styled_underlines = true;
                let explicitly_disable_kitty_keyboard_protocol = false;
                let mut terminal_output = TerminalPane::new(
                    0,
                    pane_geom,
                    Style::default(),
                    0,
                    String::new(),
                    Rc::new(RefCell::new(LinkHandler::new())),
                    character_cell_size,
                    sixel_image_store,
                    Rc::new(RefCell::new(Palette::default())),
                    Rc::new(RefCell::new(HashMap::new())),
                    None,
                    None,
                    debug,
                    arrow_fonts,
                    styled_underlines,
                    explicitly_disable_kitty_keyboard_protocol,
                ); // 0 is the pane index
                loop {
                    if !should_keep_running.load(Ordering::SeqCst) {
                        break;
                    }
                    if should_sleep {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        should_sleep = false;
                    }
                    let mut buf = [0u8; 1280000];
                    match channel.lock().unwrap().read(&mut buf) {
                        Ok(0) => {
                            let current_snapshot = take_snapshot(&mut terminal_output);
                            let mut last_snapshot = last_snapshot.lock().unwrap();
                            *cursor_coordinates.lock().unwrap() =
                                terminal_output.cursor_coordinates().unwrap_or((0, 0));
                            *last_snapshot = current_snapshot;
                            should_sleep = true;
                        },
                        Ok(count) => {
                            for byte in buf.iter().take(count) {
                                vte_parser.advance(&mut terminal_output.grid, *byte);
                            }
                            let current_snapshot = take_snapshot(&mut terminal_output);
                            let mut last_snapshot = last_snapshot.lock().unwrap();
                            *cursor_coordinates.lock().unwrap() =
                                terminal_output.grid.cursor_coordinates().unwrap_or((0, 0));
                            *last_snapshot = current_snapshot;
                            should_sleep = true;
                        },
                        Err(e) => {
                            if e.kind() == std::io::ErrorKind::WouldBlock {
                                let current_snapshot = take_snapshot(&mut terminal_output);
                                let mut last_snapshot = last_snapshot.lock().unwrap();
                                *cursor_coordinates.lock().unwrap() =
                                    terminal_output.cursor_coordinates().unwrap_or((0, 0));
                                *last_snapshot = current_snapshot;
                                should_sleep = true;
                            } else if retries_left > 0 {
                                retries_left -= 1;
                            } else {
                                break;
                            }
                        },
                    }
                }
            }
        })
        .unwrap();
    (should_keep_running, thread)
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

pub struct RemoteTerminal {
    channel: Arc<Mutex<ssh2::Channel>>,
    cursor_x: usize,
    cursor_y: usize,
    last_snapshot: Arc<Mutex<String>>,
}

impl std::fmt::Debug for RemoteTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "cursor x: {}\ncursor_y: {}\ncurrent_snapshot:\n{}",
            self.cursor_x,
            self.cursor_y,
            *self.last_snapshot.lock().unwrap()
        )
    }
}

impl RemoteTerminal {
    pub fn cursor_position_is(&self, x: usize, y: usize) -> bool {
        x == self.cursor_x && y == self.cursor_y
    }
    pub fn tip_appears(&self) -> bool {
        let snapshot = self.last_snapshot.lock().unwrap();
        snapshot.contains("Tip:") || snapshot.contains("QuickNav:")
    }
    pub fn status_bar_appears(&self) -> bool {
        self.last_snapshot.lock().unwrap().contains("Ctrl +")
    }
    pub fn tab_bar_appears(&self) -> bool {
        self.last_snapshot.lock().unwrap().contains("Tab #1")
    }
    pub fn snapshot_contains(&self, text: &str) -> bool {
        self.last_snapshot.lock().unwrap().contains(text)
    }
    #[allow(unused)]
    pub fn current_snapshot(&self) -> String {
        // convenience method for writing tests,
        // this should only be used when developing,
        // please prefer "snapsht_contains" instead
        self.last_snapshot.lock().unwrap().clone()
    }
    #[allow(unused)]
    pub fn current_cursor_position(&self) -> String {
        // convenience method for writing tests,
        // this should only be used when developing,
        // please prefer "cursor_position_is" instead
        format!("x: {}, y: {}", self.cursor_x, self.cursor_y)
    }
    pub fn send_key(&mut self, key: &[u8]) {
        let mut channel = self.channel.lock().unwrap();
        channel.write_all(key).unwrap();
        channel.flush().unwrap();
    }
    pub fn change_size(&mut self, cols: u32, rows: u32) {
        self.channel
            .lock()
            .unwrap()
            .request_pty_size(cols, rows, Some(cols), Some(rows))
            .unwrap();
    }
    pub fn attach_to_original_session(&mut self) {
        let mut channel = self.channel.lock().unwrap();
        channel
            .write_all(
                format!("{} attach {}\n", ZELLIJ_EXECUTABLE_LOCATION, SESSION_NAME).as_bytes(),
            )
            .unwrap();
        channel.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1)); // wait until Zellij stops parsing startup ANSI codes from the terminal STDIN
    }
    pub fn send_command_through_the_cli(&mut self, command: &str) {
        let mut channel = self.channel.lock().unwrap();
        channel
            .write_all(
                // note that this is run with the -s flag that suspends the command on startup
                format!("{} run -s -- \"{}\"\n", ZELLIJ_EXECUTABLE_LOCATION, command).as_bytes(),
            )
            .unwrap();
        channel.flush().unwrap();
    }
    pub fn path_to_fixture_folder(&self) -> String {
        ZELLIJ_FIXTURE_PATH.to_string()
    }
    pub fn load_fixture(&mut self, name: &str) {
        let mut channel = self.channel.lock().unwrap();
        channel
            .write_all(format!("cat {ZELLIJ_FIXTURE_PATH}/{name}\n").as_bytes())
            .unwrap();
        channel.flush().unwrap();
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
    channel: Arc<Mutex<ssh2::Channel>>,
    currently_running_step: Option<String>,
    retries_left: usize,
    retry_pause_ms: usize,
    panic_on_no_retries_left: bool,
    last_snapshot: Arc<Mutex<String>>,
    cursor_coordinates: Arc<Mutex<(usize, usize)>>, // x, y
    reader_thread: (Arc<AtomicBool>, std::thread::JoinHandle<()>),
    pub test_timed_out: bool,
}

impl RemoteRunner {
    pub fn new(win_size: Size) -> Self {
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        let mut rows = Dimension::fixed(win_size.rows);
        let mut cols = Dimension::fixed(win_size.cols);
        rows.set_inner(win_size.rows);
        cols.set_inner(win_size.cols);
        let pane_geom = PaneGeom {
            x: 0,
            y: 0,
            rows,
            cols,
            is_stacked: false,
        };
        setup_remote_environment(&mut channel, win_size);
        start_zellij(&mut channel);
        let channel = Arc::new(Mutex::new(channel));
        let last_snapshot = Arc::new(Mutex::new(String::new()));
        let cursor_coordinates = Arc::new(Mutex::new((0, 0)));
        sess.set_blocking(false);
        let reader_thread =
            read_from_channel(&channel, &last_snapshot, &cursor_coordinates, &pane_geom);
        RemoteRunner {
            steps: vec![],
            channel,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: RETRIES,
            retry_pause_ms: 100,
            test_timed_out: false,
            panic_on_no_retries_left: true,
            last_snapshot,
            cursor_coordinates,
            reader_thread,
        }
    }
    pub fn new_mirrored_session(win_size: Size) -> Self {
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        let mut rows = Dimension::fixed(win_size.rows);
        let mut cols = Dimension::fixed(win_size.cols);
        rows.set_inner(win_size.rows);
        cols.set_inner(win_size.cols);
        let pane_geom = PaneGeom {
            x: 0,
            y: 0,
            rows,
            cols,
            is_stacked: false,
        };
        setup_remote_environment(&mut channel, win_size);
        start_zellij_mirrored_session(&mut channel);
        let channel = Arc::new(Mutex::new(channel));
        let last_snapshot = Arc::new(Mutex::new(String::new()));
        let cursor_coordinates = Arc::new(Mutex::new((0, 0)));
        sess.set_blocking(false);
        let reader_thread =
            read_from_channel(&channel, &last_snapshot, &cursor_coordinates, &pane_geom);
        RemoteRunner {
            steps: vec![],
            channel,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: RETRIES,
            retry_pause_ms: 100,
            test_timed_out: false,
            panic_on_no_retries_left: true,
            last_snapshot,
            cursor_coordinates,
            reader_thread,
        }
    }
    pub fn new_mirrored_session_with_layout(win_size: Size, layout_file_name: &str) -> Self {
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        let mut rows = Dimension::fixed(win_size.rows);
        let mut cols = Dimension::fixed(win_size.cols);
        rows.set_inner(win_size.rows);
        cols.set_inner(win_size.cols);
        let pane_geom = PaneGeom {
            x: 0,
            y: 0,
            rows,
            cols,
            is_stacked: false,
        };
        setup_remote_environment(&mut channel, win_size);
        start_zellij_mirrored_session_with_layout(&mut channel, layout_file_name);
        let channel = Arc::new(Mutex::new(channel));
        let last_snapshot = Arc::new(Mutex::new(String::new()));
        let cursor_coordinates = Arc::new(Mutex::new((0, 0)));
        sess.set_blocking(false);
        let reader_thread =
            read_from_channel(&channel, &last_snapshot, &cursor_coordinates, &pane_geom);
        RemoteRunner {
            steps: vec![],
            channel,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: RETRIES,
            retry_pause_ms: 100,
            test_timed_out: false,
            panic_on_no_retries_left: true,
            last_snapshot,
            cursor_coordinates,
            reader_thread,
        }
    }
    pub fn new_mirrored_session_with_layout_and_viewport_serialization(
        win_size: Size,
        layout_file_name: &str,
    ) -> Self {
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        let mut rows = Dimension::fixed(win_size.rows);
        let mut cols = Dimension::fixed(win_size.cols);
        rows.set_inner(win_size.rows);
        cols.set_inner(win_size.cols);
        let pane_geom = PaneGeom {
            x: 0,
            y: 0,
            rows,
            cols,
            is_stacked: false,
        };
        setup_remote_environment(&mut channel, win_size);
        start_zellij_mirrored_session_with_layout_and_viewport_serialization(
            &mut channel,
            layout_file_name,
        );
        let channel = Arc::new(Mutex::new(channel));
        let last_snapshot = Arc::new(Mutex::new(String::new()));
        let cursor_coordinates = Arc::new(Mutex::new((0, 0)));
        sess.set_blocking(false);
        let reader_thread =
            read_from_channel(&channel, &last_snapshot, &cursor_coordinates, &pane_geom);
        RemoteRunner {
            steps: vec![],
            channel,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: RETRIES,
            retry_pause_ms: 100,
            test_timed_out: false,
            panic_on_no_retries_left: true,
            last_snapshot,
            cursor_coordinates,
            reader_thread,
        }
    }
    pub fn kill_running_sessions(win_size: Size) {
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        setup_remote_environment(&mut channel, win_size);
        start_zellij(&mut channel);
    }
    pub fn new_with_session_name(win_size: Size, session_name: &str, mirrored: bool) -> Self {
        // notice that this method does not have a timeout, so use with caution!
        let sess = ssh_connect_without_timeout();
        let mut channel = sess.channel_session().unwrap();
        let mut rows = Dimension::fixed(win_size.rows);
        let mut cols = Dimension::fixed(win_size.cols);
        rows.set_inner(win_size.rows);
        cols.set_inner(win_size.cols);
        let pane_geom = PaneGeom {
            x: 0,
            y: 0,
            rows,
            cols,
            is_stacked: false,
        };
        setup_remote_environment(&mut channel, win_size);
        start_zellij_in_session(&mut channel, session_name, mirrored);
        let channel = Arc::new(Mutex::new(channel));
        let last_snapshot = Arc::new(Mutex::new(String::new()));
        let cursor_coordinates = Arc::new(Mutex::new((0, 0)));
        sess.set_blocking(false);
        let reader_thread =
            read_from_channel(&channel, &last_snapshot, &cursor_coordinates, &pane_geom);
        RemoteRunner {
            steps: vec![],
            channel,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: RETRIES,
            retry_pause_ms: 100,
            test_timed_out: false,
            panic_on_no_retries_left: true,
            last_snapshot,
            cursor_coordinates,
            reader_thread,
        }
    }
    pub fn new_existing_session(win_size: Size, session_name: &str) -> Self {
        let sess = ssh_connect_without_timeout();
        let mut channel = sess.channel_session().unwrap();
        let mut rows = Dimension::fixed(win_size.rows);
        let mut cols = Dimension::fixed(win_size.cols);
        rows.set_inner(win_size.rows);
        cols.set_inner(win_size.cols);
        let pane_geom = PaneGeom {
            x: 0,
            y: 0,
            rows,
            cols,
            is_stacked: false,
        };
        setup_remote_environment(&mut channel, win_size);
        attach_to_existing_session(&mut channel, session_name);
        let channel = Arc::new(Mutex::new(channel));
        let last_snapshot = Arc::new(Mutex::new(String::new()));
        let cursor_coordinates = Arc::new(Mutex::new((0, 0)));
        sess.set_blocking(false);
        let reader_thread =
            read_from_channel(&channel, &last_snapshot, &cursor_coordinates, &pane_geom);
        RemoteRunner {
            steps: vec![],
            channel,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: RETRIES,
            retry_pause_ms: 100,
            test_timed_out: false,
            panic_on_no_retries_left: true,
            last_snapshot,
            cursor_coordinates,
            reader_thread,
        }
    }
    pub fn new_without_frames(win_size: Size) -> Self {
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        let mut rows = Dimension::fixed(win_size.rows);
        let mut cols = Dimension::fixed(win_size.cols);
        rows.set_inner(win_size.rows);
        cols.set_inner(win_size.cols);
        let pane_geom = PaneGeom {
            x: 0,
            y: 0,
            rows,
            cols,
            is_stacked: false,
        };
        setup_remote_environment(&mut channel, win_size);
        start_zellij_without_frames(&mut channel);
        let channel = Arc::new(Mutex::new(channel));
        let last_snapshot = Arc::new(Mutex::new(String::new()));
        let cursor_coordinates = Arc::new(Mutex::new((0, 0)));
        sess.set_blocking(false);
        let reader_thread =
            read_from_channel(&channel, &last_snapshot, &cursor_coordinates, &pane_geom);
        RemoteRunner {
            steps: vec![],
            channel,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: RETRIES,
            retry_pause_ms: 100,
            test_timed_out: false,
            panic_on_no_retries_left: true,
            last_snapshot,
            cursor_coordinates,
            reader_thread,
        }
    }
    pub fn new_with_config(win_size: Size, config_file_name: &'static str) -> Self {
        let remote_path = Path::new(ZELLIJ_CONFIG_PATH).join(config_file_name);
        let sess = ssh_connect();
        let mut channel = sess.channel_session().unwrap();
        let mut rows = Dimension::fixed(win_size.rows);
        let mut cols = Dimension::fixed(win_size.cols);
        rows.set_inner(win_size.rows);
        cols.set_inner(win_size.cols);
        let pane_geom = PaneGeom {
            x: 0,
            y: 0,
            rows,
            cols,
            is_stacked: false,
        };
        setup_remote_environment(&mut channel, win_size);
        start_zellij_with_config(&mut channel, &remote_path.to_string_lossy());
        let channel = Arc::new(Mutex::new(channel));
        let last_snapshot = Arc::new(Mutex::new(String::new()));
        let cursor_coordinates = Arc::new(Mutex::new((0, 0)));
        sess.set_blocking(false);
        let reader_thread =
            read_from_channel(&channel, &last_snapshot, &cursor_coordinates, &pane_geom);
        RemoteRunner {
            steps: vec![],
            channel,
            currently_running_step: None,
            current_step_index: 0,
            retries_left: RETRIES,
            retry_pause_ms: 100,
            test_timed_out: false,
            panic_on_no_retries_left: true,
            last_snapshot,
            cursor_coordinates,
            reader_thread,
        }
    }
    pub fn dont_panic(mut self) -> Self {
        self.panic_on_no_retries_left = false;
        self
    }
    #[allow(unused)]
    pub fn retry_pause_ms(mut self, retry_pause_ms: usize) -> Self {
        self.retry_pause_ms = retry_pause_ms;
        self
    }
    pub fn add_step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }
    pub fn run_next_step(&mut self) {
        if let Some(next_step) = self.steps.get(self.current_step_index) {
            println!(
                "running step: {}, retries left: {}",
                next_step.name, self.retries_left
            );
            let (cursor_x, cursor_y) = *self.cursor_coordinates.lock().unwrap();
            let remote_terminal = RemoteTerminal {
                cursor_x,
                cursor_y,
                last_snapshot: self.last_snapshot.clone(),
                channel: self.channel.clone(),
            };
            let instruction = next_step.instruction;
            self.currently_running_step = Some(String::from(next_step.name));
            if instruction(remote_terminal) {
                self.retries_left = RETRIES;
                self.current_step_index += 1;
            } else {
                self.retries_left -= 1;
                std::thread::sleep(std::time::Duration::from_millis(self.retry_pause_ms as u64));
            }
        }
    }
    pub fn steps_left(&self) -> bool {
        self.steps.get(self.current_step_index).is_some()
    }
    pub fn take_snapshot_after(&mut self, step: Step) -> String {
        let mut retries_left = RETRIES;
        let instruction = step.instruction;
        loop {
            println!(
                "taking snapshot: {}, retries left: {}",
                step.name, retries_left
            );
            if retries_left == 0 {
                self.test_timed_out = true;
                return self.last_snapshot.lock().unwrap().clone();
            }
            let (cursor_x, cursor_y) = *self.cursor_coordinates.lock().unwrap();
            let remote_terminal = RemoteTerminal {
                cursor_x,
                cursor_y,
                last_snapshot: self.last_snapshot.clone(),
                channel: self.channel.clone(),
            };
            if instruction(remote_terminal) {
                return self.last_snapshot.lock().unwrap().clone();
            } else {
                retries_left -= 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
        }
    }
    pub fn run_all_steps(&mut self) {
        println!();
        loop {
            self.run_next_step();
            if !self.steps_left() {
                break;
            } else if self.retries_left == 0 {
                self.test_timed_out = true;
                break;
            }
        }
    }
}

impl Drop for RemoteRunner {
    fn drop(&mut self) {
        let _ = self.channel.lock().unwrap().close();
        let reader_thread_running = &mut self.reader_thread.0;
        reader_thread_running.store(false, Ordering::SeqCst);
    }
}

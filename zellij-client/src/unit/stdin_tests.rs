use super::input_loop;
use crate::os_input_output::HandleType;
use crate::stdin_ansi_parser::StdinAnsiParser;
use crate::stdin_loop;
use zellij_utils::anyhow::Result;
use zellij_utils::data::{Direction, InputMode, Palette};
use zellij_utils::input::actions::Action;
use zellij_utils::input::config::Config;
use zellij_utils::input::options::Options;
use zellij_utils::nix;
use zellij_utils::pane_size::Size;
use zellij_utils::termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers};

use crate::InputInstruction;
use crate::{
    os_input_output::{ClientOsApi, StdinPoller},
    ClientInstruction, CommandIsExecuting,
};

use ::insta::assert_snapshot;
use std::path::Path;

use std::io;
use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};
use std::thread;
use zellij_utils::{
    errors::ErrorContext,
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

use zellij_utils::channels::{self, ChannelWithContext, SenderWithContext};

fn read_fixture(fixture_name: &str) -> Vec<u8> {
    let mut path_to_file = std::path::PathBuf::new();
    path_to_file.push("../src");
    path_to_file.push("tests");
    path_to_file.push("fixtures");
    path_to_file.push(fixture_name);
    std::fs::read(path_to_file)
        .unwrap_or_else(|_| panic!("could not read fixture {:?}", &fixture_name))
}

#[allow(unused)]
pub mod commands {
    pub const QUIT: [u8; 1] = [17]; // ctrl-q
    pub const ESC: [u8; 1] = [27];
    pub const ENTER: [u8; 1] = [10]; // char '\n'

    pub const MOVE_FOCUS_LEFT_IN_NORMAL_MODE: [u8; 2] = [27, 104]; // alt-h
    pub const MOVE_FOCUS_RIGHT_IN_NORMAL_MODE: [u8; 2] = [27, 108]; // alt-l

    pub const PANE_MODE: [u8; 1] = [16]; // ctrl-p
    pub const SPAWN_TERMINAL_IN_PANE_MODE: [u8; 1] = [110]; // n
    pub const MOVE_FOCUS_IN_PANE_MODE: [u8; 1] = [112]; // p
    pub const SPLIT_DOWN_IN_PANE_MODE: [u8; 1] = [100]; // d
    pub const SPLIT_RIGHT_IN_PANE_MODE: [u8; 1] = [114]; // r
    pub const TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE: [u8; 1] = [102]; // f
    pub const CLOSE_PANE_IN_PANE_MODE: [u8; 1] = [120]; // x
    pub const MOVE_FOCUS_DOWN_IN_PANE_MODE: [u8; 1] = [106]; // j
    pub const MOVE_FOCUS_UP_IN_PANE_MODE: [u8; 1] = [107]; // k
    pub const MOVE_FOCUS_LEFT_IN_PANE_MODE: [u8; 1] = [104]; // h
    pub const MOVE_FOCUS_RIGHT_IN_PANE_MODE: [u8; 1] = [108]; // l

    pub const SCROLL_MODE: [u8; 1] = [19]; // ctrl-s
    pub const SCROLL_UP_IN_SCROLL_MODE: [u8; 1] = [107]; // k
    pub const SCROLL_DOWN_IN_SCROLL_MODE: [u8; 1] = [106]; // j
    pub const SCROLL_PAGE_UP_IN_SCROLL_MODE: [u8; 1] = [2]; // ctrl-b
    pub const SCROLL_PAGE_DOWN_IN_SCROLL_MODE: [u8; 1] = [6]; // ctrl-f

    pub const RESIZE_MODE: [u8; 1] = [18]; // ctrl-r
    pub const RESIZE_DOWN_IN_RESIZE_MODE: [u8; 1] = [106]; // j
    pub const RESIZE_UP_IN_RESIZE_MODE: [u8; 1] = [107]; // k
    pub const RESIZE_LEFT_IN_RESIZE_MODE: [u8; 1] = [104]; // h
    pub const RESIZE_RIGHT_IN_RESIZE_MODE: [u8; 1] = [108]; // l

    pub const TAB_MODE: [u8; 1] = [20]; // ctrl-t
    pub const NEW_TAB_IN_TAB_MODE: [u8; 1] = [110]; // n
    pub const SWITCH_NEXT_TAB_IN_TAB_MODE: [u8; 1] = [108]; // l
    pub const SWITCH_PREV_TAB_IN_TAB_MODE: [u8; 1] = [104]; // h
    pub const CLOSE_TAB_IN_TAB_MODE: [u8; 1] = [120]; // x

    pub const BRACKETED_PASTE_START: [u8; 6] = [27, 91, 50, 48, 48, 126]; // \u{1b}[200~
    pub const BRACKETED_PASTE_END: [u8; 6] = [27, 91, 50, 48, 49, 126]; // \u{1b}[201
    pub const SLEEP: [u8; 0] = [];
}

#[derive(Default, Clone)]
struct FakeStdoutWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}
impl FakeStdoutWriter {
    pub fn new(buffer: Arc<Mutex<Vec<u8>>>) -> Self {
        FakeStdoutWriter { buffer }
    }
}
impl io::Write for FakeStdoutWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.buffer.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), io::Error> {
        Ok(())
    }
}

#[derive(Clone)]
struct FakeClientOsApi {
    events_sent_to_server: Arc<Mutex<Vec<ClientToServerMsg>>>,
    command_is_executing: Arc<Mutex<CommandIsExecuting>>,
    stdout_buffer: Arc<Mutex<Vec<u8>>>,
    stdin_buffer: Vec<u8>,
}

impl FakeClientOsApi {
    pub fn new(
        events_sent_to_server: Arc<Mutex<Vec<ClientToServerMsg>>>,
        command_is_executing: CommandIsExecuting,
    ) -> Self {
        // while command_is_executing itself is implemented with an Arc<Mutex>, we have to have an
        // Arc<Mutex> here because we need interior mutability, otherwise we'll have to change the
        // ClientOsApi trait, and that will cause a lot of havoc
        let command_is_executing = Arc::new(Mutex::new(command_is_executing));
        let stdout_buffer = Arc::new(Mutex::new(vec![]));
        FakeClientOsApi {
            events_sent_to_server,
            command_is_executing,
            stdout_buffer,
            stdin_buffer: vec![],
        }
    }
    pub fn with_stdin_buffer(mut self, stdin_buffer: Vec<u8>) -> Self {
        self.stdin_buffer = stdin_buffer;
        self
    }
    pub fn stdout_buffer(&self) -> Vec<u8> {
        self.stdout_buffer.lock().unwrap().drain(..).collect()
    }
}

impl ClientOsApi for FakeClientOsApi {
    fn get_terminal_size(&self, _handle_type: HandleType) -> Size {
        unimplemented!()
    }
    fn set_raw_mode(&mut self, _fd: RawFd) {
        unimplemented!()
    }
    fn unset_raw_mode(&self, _fd: RawFd) -> Result<(), nix::Error> {
        unimplemented!()
    }
    fn get_stdout_writer(&self) -> Box<dyn io::Write> {
        let fake_stdout_writer = FakeStdoutWriter::new(self.stdout_buffer.clone());
        Box::new(fake_stdout_writer)
    }
    fn get_stdin_reader(&self) -> Box<dyn io::Read> {
        unimplemented!()
    }
    fn update_session_name(&mut self, new_session_name: String) {}
    fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
        Ok(self.stdin_buffer.drain(..).collect())
    }
    fn box_clone(&self) -> Box<dyn ClientOsApi> {
        unimplemented!()
    }
    fn send_to_server(&self, msg: ClientToServerMsg) {
        {
            let mut events_sent_to_server = self.events_sent_to_server.lock().unwrap();
            events_sent_to_server.push(msg);
        }
        {
            let mut command_is_executing = self.command_is_executing.lock().unwrap();
            command_is_executing.unblock_input_thread();
        }
    }
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
        unimplemented!()
    }
    fn handle_signals(&self, _sigwinch_cb: Box<dyn Fn()>, _quit_cb: Box<dyn Fn()>) {
        unimplemented!()
    }
    fn connect_to_server(&self, _path: &Path) {
        unimplemented!()
    }
    fn load_palette(&self) -> Palette {
        unimplemented!()
    }
    fn enable_mouse(&self) -> Result<()> {
        Ok(())
    }
    fn disable_mouse(&self) -> Result<()> {
        Ok(())
    }
    fn stdin_poller(&self) -> StdinPoller {
        unimplemented!()
    }
}

fn extract_actions_sent_to_server(
    events_sent_to_server: Arc<Mutex<Vec<ClientToServerMsg>>>,
) -> Vec<Action> {
    let events_sent_to_server = events_sent_to_server.lock().unwrap();
    events_sent_to_server.iter().fold(vec![], |mut acc, event| {
        if let ClientToServerMsg::Action(action, None, None) = event {
            acc.push(action.clone());
        }
        acc
    })
}

#[test]
pub fn quit_breaks_input_loop() {
    let stdin_events = vec![(
        commands::QUIT.to_vec(),
        InputEvent::Key(KeyEvent {
            key: KeyCode::Char('q'),
            modifiers: Modifiers::CTRL,
        }),
    )];
    let events_sent_to_server = Arc::new(Mutex::new(vec![]));
    let command_is_executing = CommandIsExecuting::new();
    let client_os_api = Box::new(FakeClientOsApi::new(
        events_sent_to_server.clone(),
        command_is_executing.clone(),
    ));
    let config = Config::from_default_assets().unwrap();
    let options = Options::default();

    let (send_client_instructions, _receive_client_instructions): ChannelWithContext<
        ClientInstruction,
    > = channels::bounded(50);
    let send_client_instructions = SenderWithContext::new(send_client_instructions);

    let (send_input_instructions, receive_input_instructions): ChannelWithContext<
        InputInstruction,
    > = channels::bounded(50);
    let send_input_instructions = SenderWithContext::new(send_input_instructions);
    for event in stdin_events {
        send_input_instructions
            .send(InputInstruction::KeyEvent(event.1, event.0))
            .unwrap();
    }

    let default_mode = InputMode::Normal;
    input_loop(
        client_os_api,
        config,
        options,
        command_is_executing,
        send_client_instructions,
        default_mode,
        receive_input_instructions,
    );
    let expected_actions_sent_to_server = vec![Action::Quit];
    let received_actions = extract_actions_sent_to_server(events_sent_to_server);
    assert_eq!(
        expected_actions_sent_to_server, received_actions,
        "All actions sent to server properly"
    );
}

#[test]
pub fn move_focus_left_in_normal_mode() {
    let stdin_events = vec![
        (
            commands::MOVE_FOCUS_LEFT_IN_NORMAL_MODE.to_vec(),
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('h'),
                modifiers: Modifiers::ALT,
            }),
        ),
        (
            commands::QUIT.to_vec(),
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('q'),
                modifiers: Modifiers::CTRL,
            }),
        ),
    ];

    let events_sent_to_server = Arc::new(Mutex::new(vec![]));
    let command_is_executing = CommandIsExecuting::new();
    let client_os_api = Box::new(FakeClientOsApi::new(
        events_sent_to_server.clone(),
        command_is_executing.clone(),
    ));
    let config = Config::from_default_assets().unwrap();
    let options = Options::default();

    let (send_client_instructions, _receive_client_instructions): ChannelWithContext<
        ClientInstruction,
    > = channels::bounded(50);
    let send_client_instructions = SenderWithContext::new(send_client_instructions);

    let (send_input_instructions, receive_input_instructions): ChannelWithContext<
        InputInstruction,
    > = channels::bounded(50);
    let send_input_instructions = SenderWithContext::new(send_input_instructions);
    for event in stdin_events {
        send_input_instructions
            .send(InputInstruction::KeyEvent(event.1, event.0))
            .unwrap();
    }

    let default_mode = InputMode::Normal;
    input_loop(
        client_os_api,
        config,
        options,
        command_is_executing,
        send_client_instructions,
        default_mode,
        receive_input_instructions,
    );
    let expected_actions_sent_to_server =
        vec![Action::MoveFocusOrTab(Direction::Left), Action::Quit];
    let received_actions = extract_actions_sent_to_server(events_sent_to_server);
    assert_eq!(
        expected_actions_sent_to_server, received_actions,
        "All actions sent to server properly"
    );
}

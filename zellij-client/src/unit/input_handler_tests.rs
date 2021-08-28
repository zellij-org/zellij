use super::input_loop;
use zellij_utils::input::actions::{Action, Direction};
use zellij_utils::input::config::Config;
use zellij_utils::input::options::Options;
use zellij_utils::pane_size::Size;
use zellij_utils::zellij_tile::data::Palette;

use crate::{os_input_output::ClientOsApi, ClientInstruction, CommandIsExecuting};

use std::path::Path;

use zellij_utils::zellij_tile;

use std::io;
use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};
use zellij_tile::data::InputMode;
use zellij_utils::{
    errors::ErrorContext,
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

use zellij_utils::channels::{self, ChannelWithContext, SenderWithContext};

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

struct FakeClientOsApi {
    stdin_events: Arc<Mutex<Vec<Vec<u8>>>>,
    events_sent_to_server: Arc<Mutex<Vec<ClientToServerMsg>>>,
    command_is_executing: Arc<Mutex<CommandIsExecuting>>,
}

impl FakeClientOsApi {
    pub fn new(
        mut stdin_events: Vec<Vec<u8>>,
        events_sent_to_server: Arc<Mutex<Vec<ClientToServerMsg>>>,
        command_is_executing: CommandIsExecuting,
    ) -> Self {
        // while command_is_executing itself is implemented with an Arc<Mutex>, we have to have an
        // Arc<Mutex> here because we need interior mutability, otherwise we'll have to change the
        // ClientOsApi trait, and that will cause a lot of havoc
        let command_is_executing = Arc::new(Mutex::new(command_is_executing));
        stdin_events.push(commands::QUIT.to_vec());
        let stdin_events = Arc::new(Mutex::new(stdin_events)); // this is also done for interior mutability
        FakeClientOsApi {
            stdin_events,
            events_sent_to_server,
            command_is_executing,
        }
    }
}

impl ClientOsApi for FakeClientOsApi {
    fn get_terminal_size_using_fd(&self, _fd: RawFd) -> Size {
        unimplemented!()
    }
    fn set_raw_mode(&mut self, _fd: RawFd) {
        unimplemented!()
    }
    fn unset_raw_mode(&self, _fd: RawFd) {
        unimplemented!()
    }
    fn get_stdout_writer(&self) -> Box<dyn io::Write> {
        unimplemented!()
    }
    fn read_from_stdin(&self) -> Vec<u8> {
        let mut stdin_events = self.stdin_events.lock().unwrap();
        if stdin_events.is_empty() {
            panic!("ran out of stdin events!");
        }
        let next_event = stdin_events.remove(0);
        next_event
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
    fn recv_from_server(&self) -> (ServerToClientMsg, ErrorContext) {
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
    fn enable_mouse(&self) {}
    fn disable_mouse(&self) {}
    fn start_action_repeater(&mut self, _action: Action) {}
}

fn extract_actions_sent_to_server(
    events_sent_to_server: Arc<Mutex<Vec<ClientToServerMsg>>>,
) -> Vec<Action> {
    let events_sent_to_server = events_sent_to_server.lock().unwrap();
    events_sent_to_server.iter().fold(vec![], |mut acc, event| {
        if let ClientToServerMsg::Action(action) = event {
            acc.push(action.clone());
        }
        acc
    })
}

#[test]
pub fn quit_breaks_input_loop() {
    let stdin_events = vec![];
    let events_sent_to_server = Arc::new(Mutex::new(vec![]));
    let command_is_executing = CommandIsExecuting::new();
    let client_os_api = Box::new(FakeClientOsApi::new(
        stdin_events,
        events_sent_to_server.clone(),
        command_is_executing.clone(),
    ));
    let config = Config::from_default_assets().unwrap();
    let options = Options::default();

    let (send_client_instructions, _receive_client_instructions): ChannelWithContext<
        ClientInstruction,
    > = channels::bounded(50);
    let send_client_instructions = SenderWithContext::new(send_client_instructions);

    let default_mode = InputMode::Normal;
    drop(input_loop(
        client_os_api,
        config,
        options,
        command_is_executing,
        send_client_instructions,
        default_mode,
    ));
    let expected_actions_sent_to_server = vec![Action::Quit];
    let received_actions = extract_actions_sent_to_server(events_sent_to_server);
    assert_eq!(
        expected_actions_sent_to_server, received_actions,
        "All actions sent to server properly"
    );
}

#[test]
pub fn move_focus_left_in_pane_mode() {
    let mut stdin_events = vec![];
    stdin_events.push(commands::MOVE_FOCUS_LEFT_IN_NORMAL_MODE.to_vec());
    let events_sent_to_server = Arc::new(Mutex::new(vec![]));
    let command_is_executing = CommandIsExecuting::new();
    let client_os_api = Box::new(FakeClientOsApi::new(
        stdin_events,
        events_sent_to_server.clone(),
        command_is_executing.clone(),
    ));
    let config = Config::from_default_assets().unwrap();
    let options = Options::default();

    let (send_client_instructions, _receive_client_instructions): ChannelWithContext<
        ClientInstruction,
    > = channels::bounded(50);
    let send_client_instructions = SenderWithContext::new(send_client_instructions);

    let default_mode = InputMode::Normal;
    drop(input_loop(
        client_os_api,
        config,
        options,
        command_is_executing,
        send_client_instructions,
        default_mode,
    ));
    let expected_actions_sent_to_server =
        vec![Action::MoveFocusOrTab(Direction::Left), Action::Quit];
    let received_actions = extract_actions_sent_to_server(events_sent_to_server);
    assert_eq!(
        expected_actions_sent_to_server, received_actions,
        "All actions sent to server properly"
    );
}

#[test]
pub fn bracketed_paste() {
    let stdin_events = vec![
        commands::BRACKETED_PASTE_START.to_vec(),
        commands::MOVE_FOCUS_LEFT_IN_NORMAL_MODE.to_vec(),
        commands::BRACKETED_PASTE_END.to_vec(),
    ];
    let events_sent_to_server = Arc::new(Mutex::new(vec![]));
    let command_is_executing = CommandIsExecuting::new();
    let client_os_api = Box::new(FakeClientOsApi::new(
        stdin_events,
        events_sent_to_server.clone(),
        command_is_executing.clone(),
    ));
    let config = Config::from_default_assets().unwrap();
    let options = Options::default();

    let (send_client_instructions, _receive_client_instructions): ChannelWithContext<
        ClientInstruction,
    > = channels::bounded(50);
    let send_client_instructions = SenderWithContext::new(send_client_instructions);

    let default_mode = InputMode::Normal;
    drop(input_loop(
        client_os_api,
        config,
        options,
        command_is_executing,
        send_client_instructions,
        default_mode,
    ));
    let expected_actions_sent_to_server = vec![
        Action::Write(commands::BRACKETED_PASTE_START.to_vec()),
        Action::Write(commands::MOVE_FOCUS_LEFT_IN_NORMAL_MODE.to_vec()), // keys were directly written to server and not interpreted
        Action::Write(commands::BRACKETED_PASTE_END.to_vec()),
        Action::Quit,
    ];
    let received_actions = extract_actions_sent_to_server(events_sent_to_server);
    assert_eq!(
        expected_actions_sent_to_server, received_actions,
        "All actions sent to server properly"
    );
}

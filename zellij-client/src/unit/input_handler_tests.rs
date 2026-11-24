use crate::input_handler::{from_termwiz, input_loop};
use crate::nested_reannounce::NestedReannounce;
use crate::os_input_output::ClientOsApi;
use crate::{ClientInstruction, CommandIsExecuting, InputInstruction};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use zellij_utils::channels::{self, ChannelWithContext, SenderWithContext};
use zellij_utils::data::{InputMode, Palette};
use zellij_utils::errors::ErrorContext;
use zellij_utils::input::actions::Action;
use zellij_utils::input::config::Config;
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::input::options::{OnForceClose, Options};
use zellij_utils::ipc::{ClientToServerMsg, ExitReason, ServerToClientMsg};
use zellij_utils::pane_size::Size;
use zellij_utils::position::Position;
use zellij_utils::vendored::termwiz::input::{InputEvent, InputParser};

#[derive(Clone, Debug, Default)]
struct RecordingOsApi {
    sent_to_server: Arc<Mutex<Vec<ClientToServerMsg>>>,
}

impl ClientOsApi for RecordingOsApi {
    fn get_terminal_size(&self) -> Size {
        Size { rows: 24, cols: 80 }
    }
    fn set_raw_mode(&mut self) {}
    fn unset_raw_mode(&self) -> Result<(), io::Error> {
        Ok(())
    }
    fn get_stdout_writer(&self) -> Box<dyn Write> {
        Box::new(io::sink())
    }
    fn get_stdin_reader(&self) -> Box<dyn io::BufRead> {
        Box::new(io::Cursor::new(Vec::new()))
    }
    fn update_session_name(&mut self, _new_session_name: String) {}
    fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
        Ok(vec![])
    }
    fn box_clone(&self) -> Box<dyn ClientOsApi> {
        Box::new(self.clone())
    }
    fn send_to_server(&self, msg: ClientToServerMsg) {
        self.sent_to_server.lock().unwrap().push(msg);
    }
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
        None
    }
    fn handle_signals(
        &self,
        _sigwinch_cb: Box<dyn Fn()>,
        _quit_cb: Box<dyn Fn()>,
        _resize_receiver: Option<std::sync::mpsc::Receiver<()>>,
    ) {
    }
    fn connect_to_server(&self, _path: &std::path::Path) -> std::io::Result<()> {
        Ok(())
    }
    fn load_palette(&self) -> Palette {
        Palette::default()
    }
    fn enable_mouse(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn disable_mouse(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn close_host_terminal(options: Options) -> (Vec<Action>, Vec<ClientInstruction>) {
    let os_api = RecordingOsApi::default();
    let (send_client_instructions, receive_client_instructions): ChannelWithContext<
        ClientInstruction,
    > = channels::unbounded();
    let (send_input_instructions, receive_input_instructions): ChannelWithContext<
        InputInstruction,
    > = channels::unbounded();
    let send_input_instructions = SenderWithContext::new(send_input_instructions);
    let nested_reannounce = NestedReannounce::spawn(os_api.box_clone(), "test".to_string());
    send_input_instructions
        .send(InputInstruction::HostTerminalClosed)
        .unwrap();
    let input_thread = std::thread::spawn({
        let os_api = os_api.box_clone();
        let nested_reannounce = nested_reannounce.clone();
        move || {
            input_loop(
                os_api,
                Config::default(),
                options,
                CommandIsExecuting::new(),
                SenderWithContext::new(send_client_instructions),
                InputMode::Normal,
                receive_input_instructions,
                nested_reannounce,
            )
        }
    });
    input_thread.join().unwrap();
    nested_reannounce.stop();
    let actions = os_api
        .sent_to_server
        .lock()
        .unwrap()
        .drain(..)
        .filter_map(|msg| match msg {
            ClientToServerMsg::Action { action, .. } => Some(action),
            _ => None,
        })
        .collect();
    let client_instructions = receive_client_instructions
        .try_iter()
        .map(|(instruction, _)| instruction)
        .collect();
    (actions, client_instructions)
}

#[test]
fn a_closed_host_terminal_detaches_by_default() {
    let (actions, client_instructions) = close_host_terminal(Options::default());
    assert_eq!(actions, vec![Action::Detach]);
    assert!(
        client_instructions.iter().any(|instruction| matches!(
            instruction,
            ClientInstruction::Exit(ExitReason::NormalDetached)
        )),
        "the client must exit after detaching: {:?}",
        client_instructions
    );
}

#[test]
fn a_closed_host_terminal_quits_when_on_force_close_is_quit() {
    let options = Options {
        on_force_close: Some(OnForceClose::Quit),
        ..Default::default()
    };
    let (actions, _) = close_host_terminal(options);
    assert_eq!(
        actions,
        vec![Action::SkipConfirm {
            action: Box::new(Action::Quit)
        }]
    );
}

fn mouse_event_from_host_terminal(bytes: &[u8]) -> MouseEvent {
    let mut events = InputParser::new().parse_as_vec(bytes, false);
    assert_eq!(events.len(), 1, "expected a single event, got {:?}", events);
    match events.remove(0) {
        InputEvent::Mouse(event) => from_termwiz(&mut MouseEvent::new(), event),
        other => panic!("expected a mouse event, got {:?}", other),
    }
}

#[test]
fn horizontal_scroll_left_from_host_terminal_is_read_as_left() {
    let event = mouse_event_from_host_terminal(b"\x1b[<66;42;12M");
    assert!(event.wheel_left);
    assert!(!event.wheel_right);
    assert!(!event.wheel_up && !event.wheel_down);
    assert_eq!(event.position, Position::new(11, 41));
}

#[test]
fn horizontal_scroll_right_from_host_terminal_is_read_as_right() {
    let event = mouse_event_from_host_terminal(b"\x1b[<67;42;12M");
    assert!(event.wheel_right);
    assert!(!event.wheel_left);
    assert!(!event.wheel_up && !event.wheel_down);
    assert_eq!(event.position, Position::new(11, 41));
}

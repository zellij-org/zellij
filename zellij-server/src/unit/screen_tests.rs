use super::{Screen, ScreenInstruction};
use crate::zellij_tile::data::{ModeInfo, Palette};
use crate::{
    os_input_output::{AsyncReader, Pid, ServerOsApi},
    thread_bus::Bus,
    SessionState,
};
use std::sync::{Arc, RwLock};
use zellij_utils::input::command::TerminalAction;
use zellij_utils::pane_size::Size;

use std::os::unix::io::RawFd;

use zellij_utils::ipc::ClientAttributes;
use zellij_utils::nix;

use zellij_utils::{
    errors::ErrorContext,
    interprocess::local_socket::LocalSocketStream,
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

#[derive(Clone)]
struct FakeInputOutput {}

impl ServerOsApi for FakeInputOutput {
    fn set_terminal_size_using_fd(&self, _fd: RawFd, _cols: u16, _rows: u16) {
        // noop
    }
    fn spawn_terminal(&self, _file_to_open: Option<TerminalAction>) -> (RawFd, Pid) {
        unimplemented!()
    }
    fn read_from_tty_stdout(&self, _fd: RawFd, _buf: &mut [u8]) -> Result<usize, nix::Error> {
        unimplemented!()
    }
    fn async_file_reader(&self, _fd: RawFd) -> Box<dyn AsyncReader> {
        unimplemented!()
    }
    fn write_to_tty_stdin(&self, _fd: RawFd, _buf: &[u8]) -> Result<usize, nix::Error> {
        unimplemented!()
    }
    fn tcdrain(&self, _fd: RawFd) -> Result<(), nix::Error> {
        unimplemented!()
    }
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }
    fn force_kill(&self, _pid: Pid) -> Result<(), nix::Error> {
        unimplemented!()
    }
    fn kill(&self, _pid: Pid) -> Result<(), nix::Error> {
        unimplemented!()
    }
    fn recv_from_client(&self) -> (ClientToServerMsg, ErrorContext) {
        unimplemented!()
    }
    fn send_to_client(&self, _msg: ServerToClientMsg) {
        unimplemented!()
    }
    fn add_client_sender(&self) {
        unimplemented!()
    }
    fn send_to_temp_client(&self, _msg: ServerToClientMsg) {
        unimplemented!()
    }
    fn remove_client_sender(&self) {
        unimplemented!()
    }
    fn update_receiver(&mut self, _stream: LocalSocketStream) {
        unimplemented!()
    }
    fn load_palette(&self) -> Palette {
        unimplemented!()
    }
}

fn create_new_screen(size: Size) -> Screen {
    let mut bus: Bus<ScreenInstruction> = Bus::empty();
    let fake_os_input = FakeInputOutput {};
    bus.os_input = Some(Box::new(fake_os_input));
    let mut client_attributes = ClientAttributes::default();
    client_attributes.size = size;
    let max_panes = None;
    let mode_info = ModeInfo::default();
    let session_state = Arc::new(RwLock::new(SessionState::Attached));
    Screen::new(
        bus,
        &client_attributes,
        max_panes,
        mode_info,
        session_state,
        false, // draw_pane_frames
    )
}

#[test]
fn open_new_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    screen.new_tab(1);
    screen.new_tab(2);

    assert_eq!(screen.tabs.len(), 2, "Screen now has two tabs");
    assert_eq!(
        screen.get_active_tab().unwrap().position,
        1,
        "Active tab switched to new tab"
    );
}

#[test]
pub fn switch_to_prev_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    screen.new_tab(1);
    screen.new_tab(2);
    screen.switch_tab_prev();

    assert_eq!(
        screen.get_active_tab().unwrap().position,
        0,
        "Active tab switched to previous tab"
    );
}

#[test]
pub fn switch_to_next_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    screen.new_tab(1);
    screen.new_tab(2);
    screen.switch_tab_prev();
    screen.switch_tab_next();

    assert_eq!(
        screen.get_active_tab().unwrap().position,
        1,
        "Active tab switched to next tab"
    );
}

#[test]
pub fn close_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    screen.new_tab(1);
    screen.new_tab(2);
    screen.close_tab();

    assert_eq!(screen.tabs.len(), 1, "Only one tab left");
    assert_eq!(
        screen.get_active_tab().unwrap().position,
        0,
        "Active tab switched to previous tab"
    );
}

#[test]
pub fn close_the_middle_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    screen.new_tab(1);
    screen.new_tab(2);
    screen.new_tab(3);
    screen.switch_tab_prev();
    screen.close_tab();

    assert_eq!(screen.tabs.len(), 2, "Two tabs left");
    assert_eq!(
        screen.get_active_tab().unwrap().position,
        0,
        "Active tab switched to previous tab"
    );
}

#[test]
fn move_focus_left_at_left_screen_edge_changes_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    screen.new_tab(1);
    screen.new_tab(2);
    screen.new_tab(3);
    screen.switch_tab_prev();
    screen.move_focus_left_or_previous_tab();

    assert_eq!(
        screen.get_active_tab().unwrap().position,
        0,
        "Active tab switched to previous"
    );
}

#[test]
fn move_focus_right_at_right_screen_edge_changes_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    screen.new_tab(1);
    screen.new_tab(2);
    screen.new_tab(3);
    screen.switch_tab_prev();
    screen.move_focus_right_or_next_tab();

    assert_eq!(
        screen.get_active_tab().unwrap().position,
        2,
        "Active tab switched to next"
    );
}

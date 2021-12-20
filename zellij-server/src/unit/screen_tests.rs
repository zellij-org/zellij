use super::{Screen, ScreenInstruction};
use crate::panes::PaneId;
use crate::zellij_tile::data::{ModeInfo, Palette};
use crate::{
    os_input_output::{AsyncReader, Pid, ServerOsApi},
    thread_bus::Bus,
    ClientId,
};
use std::convert::TryInto;
use std::path::PathBuf;
use zellij_utils::input::command::TerminalAction;
use zellij_utils::input::layout::LayoutTemplate;
use zellij_utils::ipc::IpcReceiverWithContext;
use zellij_utils::pane_size::Size;

use std::os::unix::io::RawFd;

use zellij_utils::ipc::ClientAttributes;
use zellij_utils::nix;

use zellij_utils::{
    interprocess::local_socket::LocalSocketStream,
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

#[derive(Clone)]
struct FakeInputOutput {}

impl ServerOsApi for FakeInputOutput {
    fn set_terminal_size_using_fd(&self, _fd: RawFd, _cols: u16, _rows: u16) {
        // noop
    }
    fn spawn_terminal(
        &self,
        _file_to_open: TerminalAction,
        _quit_db: Box<dyn Fn(PaneId) + Send>,
    ) -> (RawFd, RawFd) {
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
    fn kill(&self, _pid: Pid) -> Result<(), nix::Error> {
        unimplemented!()
    }
    fn force_kill(&self, _pid: Pid) -> Result<(), nix::Error> {
        unimplemented!()
    }
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }
    fn send_to_client(&self, _client_id: ClientId, _msg: ServerToClientMsg) {
        unimplemented!()
    }
    fn new_client(
        &mut self,
        _client_id: ClientId,
        _stream: LocalSocketStream,
    ) -> IpcReceiverWithContext<ClientToServerMsg> {
        unimplemented!()
    }
    fn remove_client(&mut self, _client_id: ClientId) {
        unimplemented!()
    }
    fn load_palette(&self) -> Palette {
        unimplemented!()
    }
    fn get_cwd(&self, _pid: Pid) -> Option<PathBuf> {
        unimplemented!()
    }
}

fn create_new_screen(size: Size) -> Screen {
    let mut bus: Bus<ScreenInstruction> = Bus::empty();
    let fake_os_input = FakeInputOutput {};
    bus.os_input = Some(Box::new(fake_os_input));
    let client_attributes = ClientAttributes {
        size,
        ..Default::default()
    };
    let max_panes = None;
    let mode_info = ModeInfo::default();
    let draw_pane_frames = false;
    let session_is_mirrored = true;
    Screen::new(
        bus,
        &client_attributes,
        max_panes,
        mode_info,
        draw_pane_frames,
        session_is_mirrored,
    )
}

fn new_tab(screen: &mut Screen, pid: i32) {
    let client_id = 1;
    screen.new_tab(
        LayoutTemplate::default().try_into().unwrap(),
        vec![pid],
        client_id,
    );
}

#[test]
fn open_new_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);

    assert_eq!(screen.tabs.len(), 2, "Screen now has two tabs");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
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

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    screen.switch_tab_prev(1);

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
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

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    screen.switch_tab_prev(1);
    screen.switch_tab_next(1);

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
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

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    screen.close_tab(1);

    assert_eq!(screen.tabs.len(), 1, "Only one tab left");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
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

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    new_tab(&mut screen, 3);
    dbg!(screen
        .tabs
        .values()
        .map(|t| (t.index, t.position, t.name.clone(), t.get_pane_ids()))
        .collect::<Vec<_>>());
    screen.switch_tab_prev(1);
    dbg!(screen
        .tabs
        .values()
        .map(|t| (t.index, t.position, t.name.clone(), t.get_pane_ids()))
        .collect::<Vec<_>>());
    screen.close_tab(1);
    dbg!(screen
        .tabs
        .values()
        .map(|t| (t.index, t.position, t.name.clone(), t.get_pane_ids()))
        .collect::<Vec<_>>());

    assert_eq!(screen.tabs.len(), 2, "Two tabs left");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
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

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    new_tab(&mut screen, 3);
    screen.switch_tab_prev(1);
    screen.move_focus_left_or_previous_tab(1);

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
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

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    new_tab(&mut screen, 3);
    screen.switch_tab_prev(1);
    screen.move_focus_right_or_next_tab(1);

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab switched to next"
    );
}

#[test]
pub fn toggle_to_previous_tab_simple() {
    let position_and_size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(position_and_size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    screen.go_to_tab(1, 1);
    screen.go_to_tab(2, 1);

    screen.toggle_tab(1);
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab toggler to previous tab"
    );

    screen.toggle_tab(1);
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab toggler to previous tab"
    );
}

#[test]
pub fn toggle_to_previous_tab_create_tabs_only() {
    let position_and_size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(position_and_size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    new_tab(&mut screen, 3);

    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1],
        "Tab history is invalid"
    );

    screen.toggle_tab(1);
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab toggler to previous tab"
    );
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 2],
        "Tab history is invalid"
    );

    screen.toggle_tab(1);
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab toggler to previous tab"
    );
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1],
        "Tab history is invalid"
    );

    screen.toggle_tab(1);
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab toggler to previous tab"
    );
}

#[test]
pub fn toggle_to_previous_tab_delete() {
    let position_and_size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(position_and_size);

    new_tab(&mut screen, 1); // 0
    new_tab(&mut screen, 2); // 1
    new_tab(&mut screen, 3); // 2
    new_tab(&mut screen, 4); // 3

    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1, 2],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        3,
        "Active tab toggler to previous tab"
    );

    screen.toggle_tab(1);
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1, 3],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab toggler to previous tab"
    );

    screen.toggle_tab(1);
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1, 2],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        3,
        "Active tab toggler to previous tab"
    );

    screen.switch_tab_prev(1);
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1, 3],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab toggler to previous tab"
    );
    screen.switch_tab_prev(1);
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 3, 2],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab toggler to previous tab"
    );

    screen.close_tab(1);
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 3],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab toggler to previous tab"
    );

    screen.toggle_tab(1);
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab toggler to previous tab"
    );
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 2],
        "Tab history is invalid"
    );
}

#[test]
fn switch_to_tab_with_fullscreen() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1);
    {
        let active_tab = screen.get_active_tab_mut(1).unwrap();
        active_tab.new_pane(PaneId::Terminal(2), Some(1));
        active_tab.toggle_active_pane_fullscreen(1);
    }
    new_tab(&mut screen, 2);

    screen.switch_tab_prev(1);

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab switched to previous"
    );
    assert_eq!(
        screen
            .get_active_tab(1)
            .unwrap()
            .active_panes
            .get(&1)
            .unwrap(),
        &PaneId::Terminal(2),
        "Active pane is still the fullscreen pane"
    );
}

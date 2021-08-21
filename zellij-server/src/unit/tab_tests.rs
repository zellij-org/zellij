use super::Tab;
use crate::zellij_tile::data::{ModeInfo, Palette};
use crate::{
    os_input_output::{AsyncReader, Pid, ServerOsApi},
    panes::PaneId,
    thread_bus::ThreadSenders,
    SessionState,
};
use std::sync::{Arc, RwLock};
use zellij_utils::pane_size::PositionAndSize;

use std::os::unix::io::RawFd;

use zellij_utils::nix;

use zellij_utils::{
    errors::ErrorContext,
    input::command::TerminalAction,
    interprocess::local_socket::LocalSocketStream,
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

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
        unimplemented!()
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

fn create_new_tab(position_and_size: PositionAndSize) -> Tab {
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput {});
    let senders = ThreadSenders::default().silently_fail_on_send();
    let max_panes = None;
    let first_pane_id = Some(PaneId::Terminal(1));
    let mode_info = ModeInfo::default();
    let colors = Palette::default();
    let session_state = Arc::new(RwLock::new(SessionState::Attached));
    Tab::new(
        index,
        position,
        name,
        &position_and_size,
        os_api,
        senders,
        max_panes,
        first_pane_id,
        mode_info,
        colors,
        session_state,
        true, // draw pane frames
    )
}

#[test]
fn split_panes_vertically() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);
    tab.vertical_split(new_pane_id);
    assert_eq!(tab.panes.len(), 2, "The tab has two panes");
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "first pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "first pane row count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "second pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "second pane row count"
    );
}

#[test]
fn split_panes_horizontally() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id);
    assert_eq!(tab.panes.len(), 2, "The tab has two panes");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "first pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "first pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "second pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "second pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "second pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "second pane row count"
    );
}

#[test]
fn split_largest_pane() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    for i in 2..5 {
        let new_pane_id = PaneId::Terminal(i);
        tab.new_pane(new_pane_id);
    }
    assert_eq!(tab.panes.len(), 4, "The tab has four panes");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "first pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "first pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "second pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "second pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "third pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "third pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "third pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "third pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "fourth pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "fourth pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "fourth pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "fourth pane row count"
    );
}

#[test]
pub fn cannot_split_panes_vertically_when_active_pane_is_too_small() {
    let position_and_size = PositionAndSize {
        cols: 8,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    assert_eq!(tab.panes.len(), 1, "Tab still has only one pane");
}

#[test]
pub fn cannot_split_panes_horizontally_when_active_pane_is_too_small() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 4,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    assert_eq!(tab.panes.len(), 1, "Tab still has only one pane");
}

#[test]
pub fn cannot_split_largest_pane_when_there_is_no_room() {
    let position_and_size = PositionAndSize {
        cols: 8,
        rows: 4,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.new_pane(PaneId::Terminal(2));
    assert_eq!(tab.panes.len(), 1, "Tab still has only one pane");
}

#[test]
pub fn toggle_focused_pane_fullscreen() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    for i in 2..5 {
        let new_pane_id = PaneId::Terminal(i);
        tab.new_pane(new_pane_id);
    }
    tab.toggle_active_pane_fullscreen();
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().x(),
        0,
        "Pane x is on screen edge"
    );
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().y(),
        0,
        "Pane y is on screen edge"
    );
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().columns(),
        121,
        "Pane cols match fullscreen cols"
    );
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().rows(),
        20,
        "Pane rows match fullscreen rows"
    );
    tab.toggle_active_pane_fullscreen();
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().x(),
        61,
        "Pane x is on screen edge"
    );
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().y(),
        10,
        "Pane y is on screen edge"
    );
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().columns(),
        60,
        "Pane cols match fullscreen cols"
    );
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().rows(),
        10,
        "Pane rows match fullscreen rows"
    );
    // we don't test if all other panes are hidden because this logic is done in the render
    // function and we already test that in the e2e tests
}

#[test]
pub fn move_focus_is_disabled_in_fullscreen() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    for i in 2..5 {
        let new_pane_id = PaneId::Terminal(i);
        tab.new_pane(new_pane_id);
    }
    tab.toggle_active_pane_fullscreen();
    tab.move_focus_left();
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().x(),
        0,
        "Pane x is on screen edge"
    );
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().y(),
        0,
        "Pane y is on screen edge"
    );
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().columns(),
        121,
        "Pane cols match fullscreen cols"
    );
    assert_eq!(
        tab.panes.get(&PaneId::Terminal(4)).unwrap().rows(),
        20,
        "Pane rows match fullscreen rows"
    );
}

#[test]
pub fn close_pane_with_another_pane_above_it() {
    // ┌───────────┐            ┌───────────┐
    // │xxxxxxxxxxx│            │xxxxxxxxxxx│
    // │xxxxxxxxxxx│            │xxxxxxxxxxx│
    // ├───────────┤ ==close==> │xxxxxxxxxxx│
    // │███████████│            │xxxxxxxxxxx│
    // │███████████│            │xxxxxxxxxxx│
    // └───────────┘            └───────────┘
    // █ == pane being closed

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id);
    tab.close_focused_pane();
    assert_eq!(tab.panes.len(), 1, "One pane left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "remaining pane row count"
    );
}

#[test]
pub fn close_pane_with_another_pane_below_it() {
    // ┌───────────┐            ┌───────────┐
    // │███████████│            │xxxxxxxxxxx│
    // │███████████│            │xxxxxxxxxxx│
    // ├───────────┤ ==close==> │xxxxxxxxxxx│
    // │xxxxxxxxxxx│            │xxxxxxxxxxx│
    // │xxxxxxxxxxx│            │xxxxxxxxxxx│
    // └───────────┘            └───────────┘
    // █ == pane being closed

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id);
    tab.move_focus_up();
    tab.close_focused_pane();
    assert_eq!(tab.panes.len(), 1, "One pane left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "remaining pane row count"
    );
}

#[test]
pub fn close_pane_with_another_pane_to_the_left() {
    // ┌─────┬─────┐            ┌──────────┐
    // │xxxxx│█████│            │xxxxxxxxxx│
    // │xxxxx│█████│ ==close==> │xxxxxxxxxx│
    // │xxxxx│█████│            │xxxxxxxxxx│
    // └─────┴─────┘            └──────────┘
    // █ == pane being closed
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);
    tab.vertical_split(new_pane_id);
    tab.close_focused_pane();
    assert_eq!(tab.panes.len(), 1, "One pane left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "remaining pane row count"
    );
}

#[test]
pub fn close_pane_with_another_pane_to_the_right() {
    // ┌─────┬─────┐            ┌──────────┐
    // │█████│xxxxx│            │xxxxxxxxxx│
    // │█████│xxxxx│ ==close==> │xxxxxxxxxx│
    // │█████│xxxxx│            │xxxxxxxxxx│
    // └─────┴─────┘            └──────────┘
    // █ == pane being closed
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);
    tab.vertical_split(new_pane_id);
    tab.move_focus_left();
    tab.close_focused_pane();
    assert_eq!(tab.panes.len(), 1, "One pane left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "remaining pane row count"
    );
}

#[test]
pub fn close_pane_with_multiple_panes_above_it() {
    // ┌─────┬─────┐            ┌─────┬─────┐
    // │xxxxx│xxxxx│            │xxxxx│xxxxx│
    // │xxxxx│xxxxx│            │xxxxx│xxxxx│
    // ├─────┴─────┤ ==close==> │xxxxx│xxxxx│
    // │███████████│            │xxxxx│xxxxx│
    // │███████████│            │xxxxx│xxxxx│
    // └───────────┘            └─────┴─────┘
    // █ == pane being closed
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.horizontal_split(new_pane_id_1);
    tab.move_focus_up();
    tab.vertical_split(new_pane_id_2);
    tab.move_focus_down();
    tab.close_focused_pane();
    assert_eq!(tab.panes.len(), 2, "Two panes left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "second remaining pane row count"
    );
}

#[test]
pub fn close_pane_with_multiple_panes_below_it() {
    // ┌───────────┐            ┌─────┬─────┐
    // │███████████│            │xxxxx│xxxxx│
    // │███████████│            │xxxxx│xxxxx│
    // ├─────┬─────┤ ==close==> │xxxxx│xxxxx│
    // │xxxxx│xxxxx│            │xxxxx│xxxxx│
    // │xxxxx│xxxxx│            │xxxxx│xxxxx│
    // └─────┴─────┘            └─────┴─────┘
    // █ == pane being closed
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.horizontal_split(new_pane_id_1);
    tab.vertical_split(new_pane_id_2);
    tab.move_focus_up();
    tab.close_focused_pane();
    assert_eq!(tab.panes.len(), 2, "Two panes left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "second remaining pane row count"
    );
}

#[test]
pub fn close_pane_with_multiple_panes_to_the_left() {
    // ┌─────┬─────┐            ┌──────────┐
    // │xxxxx│█████│            │xxxxxxxxxx│
    // │xxxxx│█████│            │xxxxxxxxxx│
    // ├─────┤█████│ ==close==> ├──────────┤
    // │xxxxx│█████│            │xxxxxxxxxx│
    // │xxxxx│█████│            │xxxxxxxxxx│
    // └─────┴─────┘            └──────────┘
    // █ == pane being closed
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.vertical_split(new_pane_id_1);
    tab.move_focus_left();
    tab.horizontal_split(new_pane_id_2);
    tab.move_focus_right();
    tab.close_focused_pane();
    assert_eq!(tab.panes.len(), 2, "Two panes left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "second remaining pane row count"
    );
}

#[test]
pub fn close_pane_with_multiple_panes_to_the_right() {
    // ┌─────┬─────┐            ┌──────────┐
    // │█████│xxxxx│            │xxxxxxxxxx│
    // │█████│xxxxx│            │xxxxxxxxxx│
    // │█████├─────┤ ==close==> ├──────────┤
    // │█████│xxxxx│            │xxxxxxxxxx│
    // │█████│xxxxx│            │xxxxxxxxxx│
    // └─────┴─────┘            └──────────┘
    // █ == pane being closed
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.vertical_split(new_pane_id_1);
    tab.horizontal_split(new_pane_id_2);
    tab.move_focus_left();
    tab.close_focused_pane();
    assert_eq!(tab.panes.len(), 2, "Two panes left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "second remaining pane row count"
    );
}

#[test]
pub fn close_pane_with_multiple_panes_above_it_away_from_screen_edges() {
    // ┌───┬───┬───┬───┐            ┌───┬───┬───┬───┐
    // │xxx│xxx│xxx│xxx│            │xxx│xxx│xxx│xxx│
    // ├───┤xxx│xxx├───┤            ├───┤xxx│xxx├───┤
    // │xxx├───┴───┤xxx│ ==close==> │xxx│xxx│xxx│xxx│
    // │xxx│███████│xxx│            │xxx│xxx│xxx│xxx│
    // │xxx│███████│xxx│            │xxx│xxx│xxx│xxx│
    // └───┴───────┴───┘            └───┴───┴───┴───┘
    // █ == pane being closed
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let new_pane_id_6 = PaneId::Terminal(7);

    tab.vertical_split(new_pane_id_1);
    tab.vertical_split(new_pane_id_2);
    tab.move_focus_left();
    tab.move_focus_left();
    tab.horizontal_split(new_pane_id_3);
    tab.move_focus_right();
    tab.horizontal_split(new_pane_id_4);
    tab.move_focus_right();
    tab.horizontal_split(new_pane_id_5);
    tab.move_focus_left();
    tab.move_focus_up();
    tab.resize_down();
    tab.vertical_split(new_pane_id_6);
    tab.move_focus_down();
    tab.close_focused_pane();

    assert_eq!(tab.panes.len(), 6, "Six panes left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        15,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "second remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "third remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "third remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "third remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "third remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "fourth remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "fourth remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "fourth remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "fourth remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "sixths remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "sixths remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "sixths remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "sixths remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        76,
        "seventh remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "seventh remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        15,
        "seventh remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "seventh remaining pane row count"
    );
}

#[test]
pub fn close_pane_with_multiple_panes_below_it_away_from_screen_edges() {
    // ┌───┬───────┬───┐            ┌───┬───┬───┬───┐
    // │xxx│███████│xxx│            │xxx│xxx│xxx│xxx│
    // │xxx│███████│xxx│            │xxx│xxx│xxx│xxx│
    // │xxx├───┬───┤xxx│ ==close==> │xxx│xxx│xxx│xxx│
    // ├───┤xxx│xxx├───┤            ├───┤xxx│xxx├───┤
    // │xxx│xxx│xxx│xxx│            │xxx│xxx│xxx│xxx│
    // └───┴───┴───┴───┘            └───┴───┴───┴───┘
    // █ == pane being closed

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let new_pane_id_6 = PaneId::Terminal(7);

    tab.vertical_split(new_pane_id_1);
    tab.vertical_split(new_pane_id_2);
    tab.move_focus_left();
    tab.move_focus_left();
    tab.horizontal_split(new_pane_id_3);
    tab.move_focus_right();
    tab.horizontal_split(new_pane_id_4);
    tab.move_focus_right();
    tab.horizontal_split(new_pane_id_5);
    tab.move_focus_left();
    tab.resize_up();
    tab.vertical_split(new_pane_id_6);
    tab.move_focus_up();
    tab.close_focused_pane();

    assert_eq!(tab.panes.len(), 6, "Six panes left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "third remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "third remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "third remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "third remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "fourth remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "fourth remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "fourth remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "fourth remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        15,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "second remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "sixths remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "sixths remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "sixths remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "sixths remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        76,
        "seventh remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "seventh remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        15,
        "seventh remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "seventh remaining pane row count"
    );
}

#[test]
pub fn close_pane_with_multiple_panes_to_the_left_away_from_screen_edges() {
    // ┌────┬──────┐            ┌────┬──────┐
    // │xxxx│xxxxxx│            │xxxx│xxxxxx│
    // ├────┴┬─────┤            ├────┴──────┤
    // │xxxxx│█████│            │xxxxxxxxxxx│
    // ├─────┤█████│ ==close==> ├───────────┤
    // │xxxxx│█████│            │xxxxxxxxxxx│
    // ├────┬┴─────┤            ├────┬──────┤
    // │xxxx│xxxxxx│            │xxxx│xxxxxx│
    // └────┴──────┘            └────┴──────┘
    // █ == pane being closed

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let new_pane_id_6 = PaneId::Terminal(7);

    tab.horizontal_split(new_pane_id_1);
    tab.horizontal_split(new_pane_id_2);
    tab.move_focus_up();
    tab.move_focus_up();
    tab.vertical_split(new_pane_id_3);
    tab.move_focus_down();
    tab.vertical_split(new_pane_id_4);
    tab.move_focus_down();
    tab.vertical_split(new_pane_id_5);
    tab.move_focus_up();
    tab.move_focus_left();
    tab.resize_right();
    tab.resize_up();
    tab.horizontal_split(new_pane_id_6);
    tab.move_focus_right();
    tab.close_focused_pane();

    assert_eq!(tab.panes.len(), 6, "Six panes left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "third remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "third remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "third remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "third remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "fourth remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        23,
        "fourth remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "fourth remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        7,
        "fourth remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "second remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "sixths remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        23,
        "sixths remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "sixths remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        7,
        "sixths remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "seventh remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        18,
        "seventh remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "seventh remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "seventh remaining pane row count"
    );
}

#[test]
pub fn close_pane_with_multiple_panes_to_the_right_away_from_screen_edges() {
    // ┌────┬──────┐            ┌────┬──────┐
    // │xxxx│xxxxxx│            │xxxx│xxxxxx│
    // ├────┴┬─────┤            ├────┴──────┤
    // │█████│xxxxx│            │xxxxxxxxxxx│
    // │█████├─────┤ ==close==> ├───────────┤
    // │█████│xxxxx│            │xxxxxxxxxxx│
    // ├────┬┴─────┤            ├────┬──────┤
    // │xxxx│xxxxxx│            │xxxx│xxxxxx│
    // └────┴──────┘            └────┴──────┘
    // █ == pane being closed

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let new_pane_id_6 = PaneId::Terminal(7);

    tab.horizontal_split(new_pane_id_1);
    tab.horizontal_split(new_pane_id_2);
    tab.move_focus_up();
    tab.move_focus_up();
    tab.vertical_split(new_pane_id_3);
    tab.move_focus_down();
    tab.vertical_split(new_pane_id_4);
    tab.move_focus_down();
    tab.vertical_split(new_pane_id_5);
    tab.move_focus_up();
    tab.resize_left();
    tab.resize_up();
    tab.horizontal_split(new_pane_id_6);
    tab.move_focus_left();
    tab.close_focused_pane();

    assert_eq!(tab.panes.len(), 6, "Six panes left in tab");

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "fourth remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        23,
        "fourth remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "fourth remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        7,
        "fourth remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "second remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "third remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "third remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "third remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "third remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "sixths remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        23,
        "sixths remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "sixths remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        7,
        "sixths remaining pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "seventh remaining pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        18,
        "seventh remaining pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "seventh remaining pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "seventh remaining pane row count"
    );
}

#[test]
pub fn move_focus_down() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);

    tab.horizontal_split(new_pane_id);
    tab.move_focus_up();
    tab.move_focus_down();

    assert_eq!(
        tab.get_active_pane().unwrap().y(),
        10,
        "Active pane is the bottom one"
    );
}

#[test]
pub fn move_focus_down_to_the_most_recently_used_pane() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.horizontal_split(new_pane_id_1);
    tab.vertical_split(new_pane_id_2);
    tab.vertical_split(new_pane_id_3);
    tab.move_focus_up();
    tab.move_focus_down();

    assert_eq!(
        tab.get_active_pane().unwrap().y(),
        10,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane().unwrap().x(),
        91,
        "Active pane x position"
    );
}

#[test]
pub fn move_focus_up() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);

    tab.horizontal_split(new_pane_id);
    tab.move_focus_up();

    assert_eq!(
        tab.get_active_pane().unwrap().y(),
        0,
        "Active pane is the top one"
    );
}

#[test]
pub fn move_focus_up_to_the_most_recently_used_pane() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.horizontal_split(new_pane_id_1);
    tab.move_focus_up();
    tab.vertical_split(new_pane_id_2);
    tab.vertical_split(new_pane_id_3);
    tab.move_focus_down();
    tab.move_focus_up();

    assert_eq!(
        tab.get_active_pane().unwrap().y(),
        0,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane().unwrap().x(),
        91,
        "Active pane x position"
    );
}

#[test]
pub fn move_focus_left() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);

    tab.vertical_split(new_pane_id);
    tab.move_focus_left();

    assert_eq!(
        tab.get_active_pane().unwrap().x(),
        0,
        "Active pane is the left one"
    );
}

#[test]
pub fn move_focus_left_to_the_most_recently_used_pane() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.vertical_split(new_pane_id_1);
    tab.move_focus_left();
    tab.horizontal_split(new_pane_id_2);
    tab.horizontal_split(new_pane_id_3);
    tab.move_focus_right();
    tab.move_focus_left();

    assert_eq!(
        tab.get_active_pane().unwrap().y(),
        15,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane().unwrap().x(),
        0,
        "Active pane x position"
    );
}

#[test]
pub fn move_focus_right() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);

    tab.vertical_split(new_pane_id);
    tab.move_focus_left();
    tab.move_focus_right();

    assert_eq!(
        tab.get_active_pane().unwrap().x(),
        61,
        "Active pane is the right one"
    );
}

#[test]
pub fn move_focus_right_to_the_most_recently_used_pane() {
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.vertical_split(new_pane_id_1);
    tab.horizontal_split(new_pane_id_2);
    tab.horizontal_split(new_pane_id_3);
    tab.move_focus_left();
    tab.move_focus_right();

    assert_eq!(
        tab.get_active_pane().unwrap().y(),
        15,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane().unwrap().x(),
        61,
        "Active pane x position"
    );
}

#[test]
pub fn resize_down_with_pane_above() {
    // ┌───────────┐                  ┌───────────┐
    // │           │                  │           │
    // │           │                  │           │
    // ├───────────┤ ==resize=down==> │           │
    // │███████████│                  ├───────────┤
    // │███████████│                  │███████████│
    // │███████████│                  │███████████│
    // └───────────┘                  └───────────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id);
    tab.resize_down();

    assert_eq!(
        tab.panes.get(&new_pane_id).unwrap().position_and_size().x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.panes.get(&new_pane_id).unwrap().position_and_size().y,
        12,
        "focused pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "focused pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .rows,
        8,
        "focused pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane above x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane above y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane above column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        12,
        "pane above row count"
    );
}

#[test]
pub fn resize_down_with_pane_below() {
    // ┌───────────┐                  ┌───────────┐
    // │███████████│                  │███████████│
    // │███████████│                  │███████████│
    // ├───────────┤ ==resize=down==> │███████████│
    // │           │                  ├───────────┤
    // │           │                  │           │
    // └───────────┘                  └───────────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id);
    tab.move_focus_up();
    tab.resize_down();

    assert_eq!(
        tab.panes.get(&new_pane_id).unwrap().position_and_size().x,
        0,
        "pane below x position"
    );
    assert_eq!(
        tab.panes.get(&new_pane_id).unwrap().position_and_size().y,
        12,
        "pane below y position"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane below column count"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .rows,
        8,
        "pane below row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "focused pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "focused pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        12,
        "focused pane row count"
    );
}

#[test]
pub fn resize_down_with_panes_above_and_below() {
    // ┌───────────┐                  ┌───────────┐
    // │           │                  │           │
    // │           │                  │           │
    // ├───────────┤                  ├───────────┤
    // │███████████│ ==resize=down==> │███████████│
    // │███████████│                  │███████████│
    // │███████████│                  │███████████│
    // ├───────────┤                  │███████████│
    // │           │                  ├───────────┤
    // │           │                  │           │
    // └───────────┘                  └───────────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let first_pane_id = PaneId::Terminal(1);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.horizontal_split(new_pane_id_1);
    tab.horizontal_split(new_pane_id_2);
    tab.move_focus_up();
    tab.resize_down();

    assert_eq!(
        tab.panes.get(&new_pane_id_1).unwrap().position_and_size().x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.panes.get(&new_pane_id_1).unwrap().position_and_size().y,
        15,
        "focused pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "focused pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "focused pane row count"
    );

    assert_eq!(
        tab.panes.get(&new_pane_id_2).unwrap().position_and_size().x,
        0,
        "pane below x position"
    );
    assert_eq!(
        tab.panes.get(&new_pane_id_2).unwrap().position_and_size().y,
        25,
        "pane below y position"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane below column count"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane below row count"
    );

    assert_eq!(
        tab.panes.get(&first_pane_id).unwrap().position_and_size().x,
        0,
        "pane above x position"
    );
    assert_eq!(
        tab.panes.get(&first_pane_id).unwrap().position_and_size().y,
        0,
        "pane above y position"
    );
    assert_eq!(
        tab.panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane above column count"
    );
    assert_eq!(
        tab.panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane above row count"
    );
}

#[test]
pub fn resize_down_with_multiple_panes_above() {
    //
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┴─────┤  ==resize=down==>  │     │     │
    // │███████████│                    ├─────┴─────┤
    // │███████████│                    │███████████│
    // └───────────┘                    └───────────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let first_pane_id = PaneId::Terminal(1);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.horizontal_split(new_pane_id_1);
    tab.move_focus_up();
    tab.vertical_split(new_pane_id_2);
    tab.move_focus_down();
    tab.resize_down();

    assert_eq!(
        tab.panes.get(&new_pane_id_1).unwrap().position_and_size().x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.panes.get(&new_pane_id_1).unwrap().position_and_size().y,
        17,
        "focused pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "focused pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "focused pane row count"
    );

    assert_eq!(
        tab.panes.get(&new_pane_id_2).unwrap().position_and_size().x,
        61,
        "first pane above x position"
    );
    assert_eq!(
        tab.panes.get(&new_pane_id_2).unwrap().position_and_size().y,
        0,
        "first pane above y position"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "first pane above column count"
    );
    assert_eq!(
        tab.panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "first pane above row count"
    );

    assert_eq!(
        tab.panes.get(&first_pane_id).unwrap().position_and_size().x,
        0,
        "second pane above x position"
    );
    assert_eq!(
        tab.panes.get(&first_pane_id).unwrap().position_and_size().y,
        0,
        "second pane above y position"
    );
    assert_eq!(
        tab.panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "second pane above column count"
    );
    assert_eq!(
        tab.panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "second pane above row count"
    );
}

#[test]
pub fn resize_down_with_panes_above_aligned_left_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=down==>  ├─────┤     │
    // │     │█████│                    │     ├─────┤
    // │     │█████│                    │     │█████│
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let pane_above_and_left = PaneId::Terminal(1);
    let pane_to_the_left = PaneId::Terminal(2);
    let focused_pane = PaneId::Terminal(3);
    let pane_above = PaneId::Terminal(4);
    tab.horizontal_split(pane_to_the_left);
    tab.vertical_split(focused_pane);
    tab.move_focus_up();
    tab.vertical_split(pane_above);
    tab.move_focus_down();
    tab.resize_down();

    assert_eq!(
        tab.panes.get(&focused_pane).unwrap().position_and_size().x,
        61,
        "focused pane x position"
    );
    assert_eq!(
        tab.panes.get(&focused_pane).unwrap().position_and_size().y,
        17,
        "focused pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "focused pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "focused pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&pane_above_and_left)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane above and to the left x position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_above_and_left)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane above and to the left y position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_above_and_left)
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane above and to the left column count"
    );
    assert_eq!(
        tab.panes
            .get(&pane_above_and_left)
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane above and to the left row count"
    );

    assert_eq!(
        tab.panes.get(&pane_above).unwrap().position_and_size().x,
        61,
        "pane above x position"
    );
    assert_eq!(
        tab.panes.get(&pane_above).unwrap().position_and_size().y,
        0,
        "pane above y position"
    );
    assert_eq!(
        tab.panes.get(&pane_above).unwrap().position_and_size().cols,
        60,
        "pane above column count"
    );
    assert_eq!(
        tab.panes.get(&pane_above).unwrap().position_and_size().rows,
        17,
        "pane above row count"
    );

    assert_eq!(
        tab.panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane to the left x position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane to the left y position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane to the left column count"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane to the left row count"
    );
}

#[test]
pub fn resize_down_with_panes_below_aligned_left_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │█████│                    │     │█████│
    // │     │█████│                    │     │█████│
    // ├─────┼─────┤  ==resize=down==>  ├─────┤█████│
    // │     │     │                    │     ├─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let pane_to_the_left = PaneId::Terminal(1);
    let pane_below_and_left = PaneId::Terminal(2);
    let pane_below = PaneId::Terminal(3);
    let focused_pane = PaneId::Terminal(4);
    tab.horizontal_split(pane_below_and_left);
    tab.vertical_split(pane_below);
    tab.move_focus_up();
    tab.vertical_split(focused_pane);
    tab.resize_down();

    assert_eq!(
        tab.panes.get(&focused_pane).unwrap().position_and_size().x,
        61,
        "focused pane x position"
    );
    assert_eq!(
        tab.panes.get(&focused_pane).unwrap().position_and_size().y,
        0,
        "focused pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "focused pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "focused pane row count"
    );

    assert_eq!(
        tab.panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane above and to the left x position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane above and to the left y position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane above and to the left column count"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane above and to the left row count"
    );

    assert_eq!(
        tab.panes.get(&pane_below).unwrap().position_and_size().x,
        61,
        "pane above x position"
    );
    assert_eq!(
        tab.panes.get(&pane_below).unwrap().position_and_size().y,
        17,
        "pane above y position"
    );
    assert_eq!(
        tab.panes.get(&pane_below).unwrap().position_and_size().cols,
        60,
        "pane above column count"
    );
    assert_eq!(
        tab.panes.get(&pane_below).unwrap().position_and_size().rows,
        13,
        "pane above row count"
    );

    assert_eq!(
        tab.panes
            .get(&pane_below_and_left)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane to the left x position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_below_and_left)
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane to the left y position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_below_and_left)
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane to the left column count"
    );
    assert_eq!(
        tab.panes
            .get(&pane_below_and_left)
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane to the left row count"
    );
}

#[test]
pub fn resize_down_with_panes_above_aligned_right_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=down==>  │     ├─────┤
    // │█████│     │                    ├─────┤     │
    // │█████│     │                    │█████│     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let pane_above = PaneId::Terminal(1);
    let focused_pane = PaneId::Terminal(2);
    let pane_to_the_right = PaneId::Terminal(3);
    let pane_above_and_right = PaneId::Terminal(4);
    tab.horizontal_split(focused_pane);
    tab.vertical_split(pane_to_the_right);
    tab.move_focus_up();
    tab.vertical_split(pane_above_and_right);
    tab.move_focus_down();
    tab.move_focus_left();
    tab.resize_down();

    assert_eq!(
        tab.panes.get(&focused_pane).unwrap().position_and_size().x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.panes.get(&focused_pane).unwrap().position_and_size().y,
        17,
        "focused pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "focused pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "focused pane row count"
    );

    assert_eq!(
        tab.panes.get(&pane_above).unwrap().position_and_size().x,
        0,
        "pane above x position"
    );
    assert_eq!(
        tab.panes.get(&pane_above).unwrap().position_and_size().y,
        0,
        "pane above y position"
    );
    assert_eq!(
        tab.panes.get(&pane_above).unwrap().position_and_size().cols,
        61,
        "pane above column count"
    );
    assert_eq!(
        tab.panes.get(&pane_above).unwrap().position_and_size().rows,
        17,
        "pane above row count"
    );

    assert_eq!(
        tab.panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane to the right x position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane to the right y position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane to the right column count"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane to the right row count"
    );

    assert_eq!(
        tab.panes
            .get(&pane_above_and_right)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane above and to the right x position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_above_and_right)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane above and to the right y position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_above_and_right)
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane above and to the right column count"
    );
    assert_eq!(
        tab.panes
            .get(&pane_above_and_right)
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane above and to the right row count"
    );
}

#[test]
pub fn resize_down_with_panes_below_aligned_right_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │█████│     │                    │█████│     │
    // │█████│     │                    │█████│     │
    // ├─────┼─────┤  ==resize=down==>  │█████├─────┤
    // │     │     │                    ├─────┤     │
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    let focused_pane = PaneId::Terminal(1);
    let pane_below = PaneId::Terminal(2);
    let pane_below_and_right = PaneId::Terminal(3);
    let pane_to_the_right = PaneId::Terminal(4);
    tab.horizontal_split(pane_below);
    tab.vertical_split(pane_below_and_right);
    tab.move_focus_up();
    tab.vertical_split(pane_to_the_right);
    tab.move_focus_left();
    tab.resize_down();

    assert_eq!(
        tab.panes.get(&focused_pane).unwrap().position_and_size().x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.panes.get(&focused_pane).unwrap().position_and_size().y,
        0,
        "focused pane y position"
    );
    assert_eq!(
        tab.panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "focused pane column count"
    );
    assert_eq!(
        tab.panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "focused pane row count"
    );

    assert_eq!(
        tab.panes.get(&pane_below).unwrap().position_and_size().x,
        0,
        "pane below x position"
    );
    assert_eq!(
        tab.panes.get(&pane_below).unwrap().position_and_size().y,
        17,
        "pane below y position"
    );
    assert_eq!(
        tab.panes.get(&pane_below).unwrap().position_and_size().cols,
        61,
        "pane below column count"
    );
    assert_eq!(
        tab.panes.get(&pane_below).unwrap().position_and_size().rows,
        13,
        "pane below row count"
    );

    assert_eq!(
        tab.panes
            .get(&pane_below_and_right)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane below and to the right x position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_below_and_right)
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane below and to the right y position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_below_and_right)
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane below and to the right column count"
    );
    assert_eq!(
        tab.panes
            .get(&pane_below_and_right)
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane below and to the right row count"
    );

    assert_eq!(
        tab.panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane to the right x position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane to the right y position"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane to the right column count"
    );
    assert_eq!(
        tab.panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane to the right row count"
    );
}

#[test]
pub fn resize_down_with_panes_above_aligned_left_and_right_with_current_pane() {
    // ┌───┬───┬───┐                    ┌───┬───┬───┐
    // │   │   │   │                    │   │   │   │
    // │   │   │   │                    │   │   │   │
    // ├───┼───┼───┤  ==resize=down==>  ├───┤   ├───┤
    // │   │███│   │                    │   ├───┤   │
    // │   │███│   │                    │   │███│   │
    // └───┴───┴───┘                    └───┴───┴───┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.vertical_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_left();
    tab.move_focus_down();
    tab.resize_down();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        17,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 6 row count"
    );
}

#[test]
pub fn resize_down_with_panes_below_aligned_left_and_right_with_current_pane() {
    // ┌───┬───┬───┐                    ┌───┬───┬───┐
    // │   │███│   │                    │   │███│   │
    // │   │███│   │                    │   │███│   │
    // ├───┼───┼───┤  ==resize=down==>  ├───┤███├───┤
    // │   │   │   │                    │   ├───┤   │
    // │   │   │   │                    │   │   │   │
    // └───┴───┴───┘                    └───┴───┴───┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.vertical_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_left();
    tab.resize_down();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        17,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 6 row count"
    );
}

#[test]
pub fn resize_down_with_panes_above_aligned_left_and_right_with_panes_to_the_left_and_right() {
    // ┌─┬───────┬─┐                    ┌─┬───────┬─┐
    // │ │       │ │                    │ │       │ │
    // │ │       │ │                    │ │       │ │
    // ├─┼─┬───┬─┼─┤  ==resize=down==>  ├─┤       ├─┤
    // │ │ │███│ │ │                    │ ├─┬───┬─┤ │
    // │ │ │███│ │ │                    │ │ │███│ │ │
    // └─┴─┴───┴─┴─┘                    └─┴─┴───┴─┴─┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_down();
    tab.vertical_split(PaneId::Terminal(5));
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_left();
    tab.vertical_split(PaneId::Terminal(7));
    tab.vertical_split(PaneId::Terminal(8));
    tab.move_focus_left();
    tab.resize_down();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        17,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        15,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 6 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        76,
        "pane 7 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        17,
        "pane 7 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        8,
        "pane 7 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 7 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        84,
        "pane 8 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        17,
        "pane 8 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols,
        7,
        "pane 8 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 8 row count"
    );
}

#[test]
pub fn resize_down_with_panes_below_aligned_left_and_right_with_to_the_left_and_right() {
    // ┌─┬─┬───┬─┬─┐                    ┌─┬─┬───┬─┬─┐
    // │ │ │███│ │ │                    │ │ │███│ │ │
    // │ │ │███│ │ │                    │ │ │███│ │ │
    // ├─┼─┴───┴─┼─┤  ==resize=down==>  ├─┤ │███│ ├─┤
    // │ │       │ │                    │ ├─┴───┴─┤ │
    // │ │       │ │                    │ │       │ │
    // └─┴───────┴─┘                    └─┴───────┴─┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_left();
    tab.vertical_split(PaneId::Terminal(5));
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_down();
    tab.vertical_split(PaneId::Terminal(7));
    tab.vertical_split(PaneId::Terminal(8));
    tab.move_focus_left();
    tab.move_focus_up();
    tab.move_focus_left();
    tab.resize_down();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        15,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        76,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        8,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        84,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        7,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 6 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 7 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        17,
        "pane 7 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 7 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 7 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 8 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 8 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 8 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 8 row count"
    );
}

#[test]
pub fn cannot_resize_down_when_pane_below_is_at_minimum_height() {
    // ┌───────────┐                  ┌───────────┐
    // │███████████│                  │███████████│
    // ├───────────┤ ==resize=down==> ├───────────┤
    // │           │                  │           │
    // └───────────┘                  └───────────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 10,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.move_focus_up();
    tab.resize_down();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 1 height stayed the same"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 2 height stayed the same"
    );
}

#[test]
pub fn resize_left_with_pane_to_the_left() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // │     │█████│  ==resize=left==>  │   │███████│
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );
}

#[test]
pub fn resize_left_with_pane_to_the_right() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │█████│     │                    │███│       │
    // │█████│     │  ==resize=left==>  │███│       │
    // │█████│     │                    │███│       │
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.move_focus_left();
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );
}

#[test]
pub fn resize_left_with_panes_to_the_left_and_right() {
    // ┌─────┬─────┬─────┐                    ┌─────┬───┬───────┐
    // │     │█████│     │                    │     │███│       │
    // │     │█████│     │  ==resize=left==>  │     │███│       │
    // │     │█████│     │                    │     │███│       │
    // └─────┴─────┴─────┘                    └─────┴───┴───────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.vertical_split(PaneId::Terminal(3));
    tab.move_focus_left();
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        40,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );
}

#[test]
pub fn resize_left_with_multiple_panes_to_the_left() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // ├─────┤█████│  ==resize=left==>  ├───┤███████│
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.move_focus_left();
    tab.horizontal_split(PaneId::Terminal(3));
    tab.move_focus_right();
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 2 row count"
    );
}

#[test]
pub fn resize_left_with_panes_to_the_left_aligned_top_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=left==>  ├───┬─┴─────┤
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.vertical_split(PaneId::Terminal(3));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_down();
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_left_with_panes_to_the_right_aligned_top_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=left==>  ├───┬─┴─────┤
    // │█████│     │                    │███│       │
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.vertical_split(PaneId::Terminal(3));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_down();
    tab.move_focus_left();
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_left_with_panes_to_the_left_aligned_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // ├─────┼─────┤  ==resize=left==>  ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.vertical_split(PaneId::Terminal(3));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(4));
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_left_with_panes_to_the_right_aligned_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │█████│     │                    │███│       │
    // ├─────┼─────┤  ==resize=left==>  ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.vertical_split(PaneId::Terminal(3));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_left();
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_left_with_panes_to_the_left_aligned_top_and_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │     │█████│  ==resize=left==>  │   │███████│
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.horizontal_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_down();
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        8,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        23,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        7,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        23,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        7,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        8,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 6 row count"
    );
}

#[test]
pub fn resize_left_with_panes_to_the_right_aligned_top_and_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │█████│     │  ==resize=left==>  │███│       │
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.horizontal_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_down();
    tab.move_focus_left();
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        8,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        23,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        7,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        23,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        7,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        8,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 6 row count"
    );
}

#[test]
pub fn resize_left_with_panes_to_the_left_aligned_top_and_bottom_with_panes_above_and_below() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │     ├─────┤                    │   ├───────┤
    // │     │█████│  ==resize=left==>  │   │███████│
    // │     ├─────┤                    │   ├───────┤
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 70,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.horizontal_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.move_focus_down();
    tab.resize_down();
    tab.vertical_split(PaneId::Terminal(6));
    tab.horizontal_split(PaneId::Terminal(7));
    tab.horizontal_split(PaneId::Terminal(8));
    tab.move_focus_up();
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        35,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        35,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        55,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        55,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        35,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        35,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 6 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 7 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        45,
        "pane 7 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 7 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 7 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 8 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        50,
        "pane 8 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 8 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 8 row count"
    );
}

#[test]
pub fn resize_left_with_panes_to_the_right_aligned_top_and_bottom_with_panes_above_and_below() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // ├─────┤     │                    ├───┤       │
    // │█████│     │  ==resize=left==>  │███│       │
    // ├─────┤     │                    ├───┤       │
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 70,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.horizontal_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.move_focus_down();
    tab.resize_down();
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_left();
    tab.horizontal_split(PaneId::Terminal(7));
    tab.horizontal_split(PaneId::Terminal(8));
    tab.move_focus_up();
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        35,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        35,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        55,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        55,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        35,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        51,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        35,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        70,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 6 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 7 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        45,
        "pane 7 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 7 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 7 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 8 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        50,
        "pane 8 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols,
        51,
        "pane 8 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 8 row count"
    );
}

#[test]
pub fn cannot_resize_left_when_pane_to_the_left_is_at_minimum_width() {
    // ┌─┬─┐                    ┌─┬─┐
    // │ │█│                    │ │█│
    // │ │█│  ==resize=left==>  │ │█│
    // │ │█│                    │ │█│
    // └─┴─┘                    └─┴─┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 10,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.resize_left();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        5,
        "pane 1 columns stayed the same"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        5,
        "pane 2 columns stayed the same"
    );
}

#[test]
pub fn resize_right_with_pane_to_the_left() {
    // ┌─────┬─────┐                   ┌───────┬───┐
    // │     │█████│                   │       │███│
    // │     │█████│ ==resize=right==> │       │███│
    // │     │█████│                   │       │███│
    // └─────┴─────┘                   └───────┴───┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );
}

#[test]
pub fn resize_right_with_pane_to_the_right() {
    // ┌─────┬─────┐                   ┌───────┬───┐
    // │█████│     │                   │███████│   │
    // │█████│     │ ==resize=right==> │███████│   │
    // │█████│     │                   │███████│   │
    // └─────┴─────┘                   └───────┴───┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.move_focus_left();
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );
}

#[test]
pub fn resize_right_with_panes_to_the_left_and_right() {
    // ┌─────┬─────┬─────┐                   ┌─────┬───────┬───┐
    // │     │█████│     │                   │     │███████│   │
    // │     │█████│     │ ==resize=right==> │     │███████│   │
    // │     │█████│     │                   │     │███████│   │
    // └─────┴─────┴─────┘                   └─────┴───────┴───┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.vertical_split(PaneId::Terminal(3));
    tab.move_focus_left();
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        40,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        101,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        20,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );
}

#[test]
pub fn resize_right_with_multiple_panes_to_the_left() {
    // ┌─────┬─────┐                   ┌───────┬───┐
    // │     │█████│                   │       │███│
    // ├─────┤█████│ ==resize=right==> ├───────┤███│
    // │     │█████│                   │       │███│
    // └─────┴─────┘                   └───────┴───┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.move_focus_left();
    tab.horizontal_split(PaneId::Terminal(3));
    tab.move_focus_right();
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 3 row count"
    );
}

#[test]
pub fn resize_right_with_panes_to_the_left_aligned_top_with_current_pane() {
    // ┌─────┬─────┐                   ┌─────┬─────┐
    // │     │     │                   │     │     │
    // ├─────┼─────┤ ==resize=right==> ├─────┴─┬───┤
    // │     │█████│                   │       │███│
    // └─────┴─────┘                   └───────┴───┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.move_focus_left();
    tab.horizontal_split(PaneId::Terminal(3));
    tab.move_focus_right();
    tab.horizontal_split(PaneId::Terminal(4));
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_right_with_panes_to_the_right_aligned_top_with_current_pane() {
    // ┌─────┬─────┐                   ┌─────┬─────┐
    // │     │     │                   │     │     │
    // ├─────┼─────┤ ==resize=right==> ├─────┴─┬───┤
    // │█████│     │                   │███████│   │
    // └─────┴─────┘                   └───────┴───┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.move_focus_left();
    tab.horizontal_split(PaneId::Terminal(3));
    tab.move_focus_right();
    tab.horizontal_split(PaneId::Terminal(4));
    tab.move_focus_left();
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_right_with_panes_to_the_left_aligned_bottom_with_current_pane() {
    // ┌─────┬─────┐                   ┌───────┬───┐
    // │     │█████│                   │       │███│
    // ├─────┼─────┤ ==resize=right==> ├─────┬─┴───┤
    // │     │     │                   │     │     │
    // └─────┴─────┘                   └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.move_focus_left();
    tab.horizontal_split(PaneId::Terminal(3));
    tab.move_focus_right();
    tab.horizontal_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_right_with_panes_to_the_right_aligned_bottom_with_current_pane() {
    // ┌─────┬─────┐                   ┌───────┬───┐
    // │█████│     │                   │███████│   │
    // ├─────┼─────┤ ==resize=right==> ├─────┬─┴───┤
    // │     │     │                   │     │     │
    // └─────┴─────┘                   └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.move_focus_left();
    tab.horizontal_split(PaneId::Terminal(3));
    tab.move_focus_right();
    tab.horizontal_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.move_focus_left();
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_right_with_panes_to_the_left_aligned_top_and_bottom_with_current_pane() {
    // ┌─────┬─────┐                   ┌─────┬─────┐
    // │     │     │                   │     │     │
    // ├─────┼─────┤                   ├─────┴─┬───┤
    // │     │█████│ ==resize=right==> │       │███│
    // ├─────┼─────┤                   ├─────┬─┴───┤
    // │     │     │                   │     │     │
    // └─────┴─────┘                   └─────┴─────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.horizontal_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_down();
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 6 row count"
    );
}

#[test]
pub fn resize_right_with_panes_to_the_right_aligned_top_and_bottom_with_current_pane() {
    // ┌─────┬─────┐                   ┌─────┬─────┐
    // │     │     │                   │     │     │
    // ├─────┼─────┤                   ├─────┴─┬───┤
    // │█████│     │ ==resize=right==> │███████│   │
    // ├─────┼─────┤                   ├─────┬─┴───┤
    // │     │     │                   │     │     │
    // └─────┴─────┘                   └─────┴─────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.horizontal_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_down();
    tab.move_focus_left();
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 6 row count"
    );
}

#[test]
pub fn resize_right_with_panes_to_the_left_aligned_top_and_bottom_with_panes_above_and_below() {
    // ┌─────┬─────┐                   ┌─────┬─────┐
    // ├─────┼─────┤                   ├─────┴─┬───┤
    // │     ├─────┤                   │       ├───┤
    // │     │█████│ ==resize=right==> │       │███│
    // │     ├─────┤                   │       ├───┤
    // ├─────┼─────┤                   ├─────┬─┴───┤
    // └─────┴─────┘                   └─────┴─────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 70,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.horizontal_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.move_focus_down();
    tab.resize_up();
    tab.vertical_split(PaneId::Terminal(6));
    tab.horizontal_split(PaneId::Terminal(7));
    tab.horizontal_split(PaneId::Terminal(8));
    tab.move_focus_up();
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        33,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        33,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        53,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        53,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        33,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        33,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 6 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 7 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        43,
        "pane 7 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 7 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 7 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 8 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        48,
        "pane 8 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 8 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 8 row count"
    );
}

#[test]
pub fn resize_right_with_panes_to_the_right_aligned_top_and_bottom_with_panes_above_and_below() {
    // ┌─────┬─────┐                   ┌─────┬─────┐
    // ├─────┼─────┤                   ├─────┴─┬───┤
    // ├─────┤     │                   ├───────┤   │
    // │█████│     │ ==resize=right==> │███████│   │
    // ├─────┤     │                   ├───────┤   │
    // ├─────┼─────┤                   ├─────┬─┴───┤
    // └─────┴─────┘                   └─────┴─────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 70,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.horizontal_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.move_focus_down();
    tab.resize_up();
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_left();
    tab.horizontal_split(PaneId::Terminal(7));
    tab.horizontal_split(PaneId::Terminal(8));
    tab.move_focus_up();
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        33,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        33,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        53,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        53,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        33,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        71,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        33,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        50,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        20,
        "pane 6 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 7 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        43,
        "pane 7 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 7 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 7 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 8 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        48,
        "pane 8 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols,
        71,
        "pane 8 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 8 row count"
    );
}

#[test]
pub fn cannot_resize_right_when_pane_to_the_left_is_at_minimum_width() {
    // ┌─┬─┐                   ┌─┬─┐
    // │ │█│                   │ │█│
    // │ │█│ ==resize=right==> │ │█│
    // │ │█│                   │ │█│
    // └─┴─┘                   └─┴─┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 10,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.vertical_split(PaneId::Terminal(2));
    tab.resize_right();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        5,
        "pane 1 columns stayed the same"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        5,
        "pane 2 columns stayed the same"
    );
}

#[test]
pub fn resize_up_with_pane_above() {
    // ┌───────────┐                ┌───────────┐
    // │           │                │           │
    // │           │                ├───────────┤
    // ├───────────┤ ==resize=up==> │███████████│
    // │███████████│                │███████████│
    // │███████████│                │███████████│
    // └───────────┘                └───────────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        8,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        8,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        12,
        "pane 2 row count"
    );
}

#[test]
pub fn resize_up_with_pane_below() {
    // ┌───────────┐                ┌───────────┐
    // │███████████│                │███████████│
    // │███████████│                ├───────────┤
    // ├───────────┤ ==resize=up==> │           │
    // │           │                │           │
    // │           │                │           │
    // └───────────┘                └───────────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.move_focus_up();
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        8,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        8,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        12,
        "pane 2 row count"
    );
}

#[test]
pub fn resize_up_with_panes_above_and_below() {
    // ┌───────────┐                ┌───────────┐
    // │           │                │           │
    // │           │                ├───────────┤
    // ├───────────┤                │███████████│
    // │███████████│ ==resize=up==> │███████████│
    // │███████████│                │███████████│
    // ├───────────┤                ├───────────┤
    // │           │                │           │
    // │           │                │           │
    // └───────────┘                └───────────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.horizontal_split(PaneId::Terminal(3));
    tab.move_focus_up();
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        10,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        23,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        7,
        "pane 3 row count"
    );
}

#[test]
pub fn resize_up_with_multiple_panes_above() {
    //
    // ┌─────┬─────┐                 ┌─────┬─────┐
    // │     │     │                 ├─────┴─────┤
    // ├─────┴─────┤  ==resize=up==> │███████████│
    // │███████████│                 │███████████│
    // └───────────┘                 └───────────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(3));
    tab.move_focus_down();
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        121,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 3 row count"
    );
}

#[test]
pub fn resize_up_with_panes_above_aligned_left_with_current_pane() {
    // ┌─────┬─────┐                  ┌─────┬─────┐
    // │     │     │                  │     ├─────┤
    // ├─────┼─────┤  ==resize=up==>  ├─────┤█████│
    // │     │█████│                  │     │█████│
    // └─────┴─────┘                  └─────┴─────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(3));
    tab.move_focus_down();
    tab.vertical_split(PaneId::Terminal(4));
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_up_with_panes_below_aligned_left_with_current_pane() {
    // ┌─────┬─────┐                  ┌─────┬─────┐
    // │     │█████│                  │     │█████│
    // │     │█████│                  │     ├─────┤
    // ├─────┼─────┤  ==resize=up==>  ├─────┤     │
    // │     │     │                  │     │     │
    // │     │     │                  │     │     │
    // └─────┴─────┘                  └─────┴─────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(3));
    tab.move_focus_down();
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_up_with_panes_above_aligned_right_with_current_pane() {
    // ┌─────┬─────┐                  ┌─────┬─────┐
    // │     │     │                  │     │     │
    // │     │     │                  ├─────┤     │
    // ├─────┼─────┤  ==resize=up==>  │█████├─────┤
    // │█████│     │                  │█████│     │
    // │█████│     │                  │█████│     │
    // └─────┴─────┘                  └─────┴─────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(3));
    tab.move_focus_down();
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_left();
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_up_with_panes_below_aligned_right_with_current_pane() {
    // ┌─────┬─────┐                  ┌─────┬─────┐
    // │█████│     │                  │█████│     │
    // │█████│     │                  ├─────┤     │
    // ├─────┼─────┤  ==resize=up==>  │     ├─────┤
    // │     │     │                  │     │     │
    // │     │     │                  │     │     │
    // └─────┴─────┘                  └─────┴─────┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(3));
    tab.move_focus_down();
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_left();
    tab.move_focus_up();
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );
}

#[test]
pub fn resize_up_with_panes_above_aligned_left_and_right_with_current_pane() {
    // ┌───┬───┬───┐                  ┌───┬───┬───┐
    // │   │   │   │                  │   │   │   │
    // │   │   │   │                  │   ├───┤   │
    // ├───┼───┼───┤  ==resize=up==>  ├───┤███├───┤
    // │   │███│   │                  │   │███│   │
    // │   │███│   │                  │   │███│   │
    // └───┴───┴───┘                  └───┴───┴───┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.vertical_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_left();
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 6 row count"
    );
}

#[test]
pub fn resize_up_with_panes_below_aligned_left_and_right_with_current_pane() {
    // ┌───┬───┬───┐                  ┌───┬───┬───┐
    // │   │███│   │                  │   │███│   │
    // │   │███│   │                  │   ├───┤   │
    // ├───┼───┼───┤  ==resize=up==>  ├───┤   ├───┤
    // │   │   │   │                  │   │   │   │
    // │   │   │   │                  │   │   │   │
    // └───┴───┴───┘                  └───┴───┴───┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.vertical_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(5));
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_left();
    tab.move_focus_up();
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 6 row count"
    );
}

#[test]
pub fn resize_up_with_panes_above_aligned_left_and_right_with_panes_to_the_left_and_right() {
    // ┌─┬───────┬─┐                  ┌─┬───────┬─┐
    // │ │       │ │                  │ │       │ │
    // │ │       │ │                  │ ├─┬───┬─┤ │
    // ├─┼─┬───┬─┼─┤  ==resize=up==>  ├─┤ │███│ ├─┤
    // │ │ │███│ │ │                  │ │ │███│ │ │
    // │ │ │███│ │ │                  │ │ │███│ │ │
    // └─┴─┴───┴─┴─┘                  └─┴─┴───┴─┴─┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_down();
    tab.vertical_split(PaneId::Terminal(5));
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_left();
    tab.vertical_split(PaneId::Terminal(7));
    tab.vertical_split(PaneId::Terminal(8));
    tab.move_focus_left();
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        15,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 6 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        76,
        "pane 7 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 7 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        8,
        "pane 7 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 7 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        84,
        "pane 8 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 8 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols,
        7,
        "pane 8 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 8 row count"
    );
}

#[test]
pub fn resize_up_with_panes_below_aligned_left_and_right_with_to_the_left_and_right() {
    // ┌─┬─┬───┬─┬─┐                  ┌─┬─┬───┬─┬─┐
    // │ │ │███│ │ │                  │ │ │███│ │ │
    // │ │ │███│ │ │                  │ ├─┴───┴─┤ │
    // ├─┼─┴───┴─┼─┤  ==resize=up==>  ├─┤       ├─┤
    // │ │       │ │                  │ │       │ │
    // │ │       │ │                  │ │       │ │
    // └─┴───────┴─┘                  └─┴───────┴─┘
    // █ == focused pane
    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.move_focus_up();
    tab.vertical_split(PaneId::Terminal(3));
    tab.vertical_split(PaneId::Terminal(4));
    tab.move_focus_down();
    tab.vertical_split(PaneId::Terminal(5));
    tab.vertical_split(PaneId::Terminal(6));
    tab.move_focus_up();
    tab.move_focus_left();
    tab.vertical_split(PaneId::Terminal(7));
    tab.vertical_split(PaneId::Terminal(8));
    tab.move_focus_left();
    tab.resize_up();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols,
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols,
        15,
        "pane 3 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 3 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 5 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 5 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows,
        17,
        "pane 5 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 6 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols,
        30,
        "pane 6 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows,
        15,
        "pane 6 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        76,
        "pane 7 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 7 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols,
        8,
        "pane 7 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 7 row count"
    );

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        84,
        "pane 8 x position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 8 y position"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols,
        7,
        "pane 8 column count"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows,
        13,
        "pane 8 row count"
    );
}

#[test]
pub fn cannot_resize_up_when_pane_above_is_at_minimum_height() {
    // ┌───────────┐                ┌───────────┐
    // │           │                │           │
    // ├───────────┤ ==resize=up==> ├───────────┤
    // │███████████│                │███████████│
    // └───────────┘                └───────────┘
    // █ == focused pane

    let position_and_size = PositionAndSize {
        cols: 121,
        rows: 10,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut tab = create_new_tab(position_and_size);
    tab.horizontal_split(PaneId::Terminal(2));
    tab.resize_down();

    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 1 height stayed the same"
    );
    assert_eq!(
        tab.panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows,
        5,
        "pane 2 height stayed the same"
    );
}

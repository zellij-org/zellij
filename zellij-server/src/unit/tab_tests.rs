use super::Tab;
use crate::zellij_tile::data::{InputMode, ModeInfo, Palette};
use zellij_utils::{
    pane_size::PositionAndSize,
};
use crate::{
    os_input_output::{ServerOsApi, Pid, AsyncReader},
    panes::PaneId,
    thread_bus::ThreadSenders,
    SessionState,
};
use std::sync::{Arc, RwLock};

use std::os::unix::io::RawFd;
use std::path::PathBuf;

use zellij_utils::nix;

use zellij_utils::{
    errors::ErrorContext,
    interprocess::local_socket::LocalSocketStream,
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

struct FakeInputOutput {}

impl ServerOsApi for FakeInputOutput {
    fn set_terminal_size_using_fd(&self, _fd: RawFd, _cols: u16, _rows: u16) {

    }
    fn spawn_terminal(&self, _file_to_open: Option<PathBuf>) -> (RawFd, Pid) {
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
    let input_mode = InputMode::Normal;
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
        input_mode,
        colors,
        session_state,
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
    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().x, 0, "first pane x position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().y, 0, "first pane y position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().cols, 60, "first pane column count");
    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().rows, 20, "first pane row count");
    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().x, 61, "second pane x position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().y, 0, "second pane y position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().cols, 60, "second pane column count");
    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().rows, 20, "second pane row count");
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

    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().x, 0, "first pane x position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().y, 0, "first pane y position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().cols, 121, "first pane column count");
    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().rows, 10, "first pane row count");

    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().x, 0, "second pane x position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().y, 11, "second pane y position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().cols, 121, "second pane column count");
    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().rows, 9, "second pane row count");
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

    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().x, 0, "first pane x position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().y, 0, "first pane y position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().cols, 60, "first pane column count");
    assert_eq!(tab.panes.get(&PaneId::Terminal(1)).unwrap().position_and_size().rows, 10, "first pane row count");

    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().x, 61, "second pane x position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().y, 0, "second pane y position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().cols, 60, "second pane column count");
    assert_eq!(tab.panes.get(&PaneId::Terminal(2)).unwrap().position_and_size().rows, 10, "second pane row count");

    assert_eq!(tab.panes.get(&PaneId::Terminal(3)).unwrap().position_and_size().x, 0, "third pane x position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(3)).unwrap().position_and_size().y, 11, "third pane y position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(3)).unwrap().position_and_size().cols, 60, "third pane column count");
    assert_eq!(tab.panes.get(&PaneId::Terminal(3)).unwrap().position_and_size().rows, 9, "third pane row count");

    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().position_and_size().x, 61, "fourth pane x position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().position_and_size().y, 11, "fourth pane y position");
    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().position_and_size().cols, 60, "fourth pane column count");
    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().position_and_size().rows, 9, "fourth pane row count");
}

#[test]
pub fn cannot_split_panes_vertically_when_active_terminal_is_too_small() {
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
    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().x(), 0, "Pane x is on screen edge");
    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().y(), 0, "Pane y is on screen edge");
    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().columns(), 121, "Pane cols match fullscreen cols");
    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().rows(), 20, "Pane rows match fullscreen rows");
    tab.toggle_active_pane_fullscreen();
    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().x(), 61, "Pane x is on screen edge");
    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().y(), 11, "Pane y is on screen edge");
    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().columns(), 60, "Pane cols match fullscreen cols");
    assert_eq!(tab.panes.get(&PaneId::Terminal(4)).unwrap().rows(), 9, "Pane rows match fullscreen rows");
    // we don't test if all other panes are hidden because this logic is done in the render
    // function and we already test that in the e2e tests
}

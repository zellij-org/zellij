use super::{Output, Tab};
use crate::screen::CopyOptions;
use crate::zellij_tile::data::{ModeInfo, Palette};
use crate::{
    os_input_output::{AsyncReader, Pid, ServerOsApi},
    panes::PaneId,
    thread_bus::ThreadSenders,
    ClientId,
};
use std::convert::TryInto;
use std::path::PathBuf;
use zellij_tile::prelude::Style;
use zellij_utils::envs::set_session_name;
use zellij_utils::input::layout::LayoutTemplate;
use zellij_utils::ipc::IpcReceiverWithContext;
use zellij_utils::pane_size::Size;
use zellij_utils::position::Position;

use std::cell::RefCell;
use std::collections::HashSet;
use std::os::unix::io::RawFd;
use std::rc::Rc;

use zellij_utils::nix;

use zellij_utils::{
    input::command::TerminalAction,
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
        _quit_cb: Box<dyn Fn(PaneId) + Send>,
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

// TODO: move to shared thingy with other test file
fn create_new_tab(size: Size) -> Tab {
    set_session_name("test".into());
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput {});
    let senders = ThreadSenders::default().silently_fail_on_send();
    let max_panes = None;
    let mode_info = ModeInfo::default();
    let style = Style::default();
    let draw_pane_frames = true;
    let client_id = 1;
    let session_is_mirrored = true;
    let mut connected_clients = HashSet::new();
    connected_clients.insert(client_id);
    let connected_clients = Rc::new(RefCell::new(connected_clients));
    let character_cell_info = Rc::new(RefCell::new(None));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let copy_options = CopyOptions::default();
    let mut tab = Tab::new(
        index,
        position,
        name,
        size,
        character_cell_info,
        os_api,
        senders,
        max_panes,
        style,
        mode_info,
        draw_pane_frames,
        connected_clients,
        session_is_mirrored,
        client_id,
        copy_options,
        terminal_emulator_colors,
    );
    tab.apply_layout(
        LayoutTemplate::default().try_into().unwrap(),
        vec![1],
        index,
        client_id,
    );
    tab
}

fn read_fixture(fixture_name: &str) -> Vec<u8> {
    let mut path_to_file = std::path::PathBuf::new();
    path_to_file.push("../src");
    path_to_file.push("tests");
    path_to_file.push("fixtures");
    path_to_file.push(fixture_name);
    std::fs::read(path_to_file)
        .unwrap_or_else(|_| panic!("could not read fixture {:?}", &fixture_name))
}

use crate::panes::grid::Grid;
use crate::panes::link_handler::LinkHandler;
use ::insta::assert_snapshot;
use zellij_utils::vte;

fn take_snapshot(ansi_instructions: &str, rows: usize, columns: usize, palette: Palette) -> String {
    let mut grid = Grid::new(
        rows,
        columns,
        Rc::new(RefCell::new(palette)),
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
    );
    let mut vte_parser = vte::Parser::new();
    for &byte in ansi_instructions.as_bytes() {
        vte_parser.advance(&mut grid, byte);
    }
    format!("{:?}", grid)
}

fn take_snapshot_and_cursor_position(
    ansi_instructions: &str,
    rows: usize,
    columns: usize,
    palette: Palette,
) -> (String, Option<(usize, usize)>) {
    // snapshot, x_coordinates, y_coordinates
    let mut grid = Grid::new(
        rows,
        columns,
        Rc::new(RefCell::new(palette)),
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
    );
    let mut vte_parser = vte::Parser::new();
    for &byte in ansi_instructions.as_bytes() {
        vte_parser.advance(&mut grid, byte);
    }
    (format!("{:?}", grid), grid.cursor_coordinates())
}

#[test]
fn new_floating_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn floating_panes_persist_across_toggles() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));
    tab.toggle_floating_panes(client_id, None);
    // here we send bytes to the pane when it's not visible to make sure they're still handled and
    // we see them once we toggle the panes back
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.toggle_floating_panes(client_id, None);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn toggle_floating_panes_off() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.toggle_floating_panes(client_id, None);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn toggle_floating_panes_on() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.toggle_floating_panes(client_id, None);
    tab.toggle_floating_panes(client_id, None);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn five_new_floating_panes() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn increase_floating_pane_size() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.resize_increase(client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn decrease_floating_pane_size() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.resize_decrease(client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn resize_floating_pane_left() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.resize_left(client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn resize_floating_pane_right() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.resize_right(client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn resize_floating_pane_up() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.resize_up(client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn resize_floating_pane_down() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.resize_down(client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn move_floating_pane_focus_left() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.move_focus_left(client_id);
    tab.render(&mut output, None);
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((71, 9)),
        "cursor coordinates moved to the pane on the left"
    );

    assert_snapshot!(snapshot);
}

#[test]
fn move_floating_pane_focus_right() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.move_focus_left(client_id);
    tab.move_focus_right(client_id);
    tab.render(&mut output, None);
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((80, 3)),
        "cursor coordinates moved to the pane on the right"
    );

    assert_snapshot!(snapshot);
}

#[test]
fn move_floating_pane_focus_up() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.move_focus_up(client_id);
    tab.render(&mut output, None);
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((71, 9)),
        "cursor coordinates moved to the pane above"
    );

    assert_snapshot!(snapshot);
}

#[test]
fn move_floating_pane_focus_down() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.move_focus_up(client_id);
    tab.move_focus_down(client_id);
    tab.render(&mut output, None);
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((80, 13)),
        "cursor coordinates moved to the pane below"
    );

    assert_snapshot!(snapshot);
}

#[test]
fn move_floating_pane_focus_with_mouse() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_left_click(&Position::new(9, 71), client_id);
    tab.handle_mouse_release(&Position::new(9, 71), client_id);
    tab.render(&mut output, None);
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((71, 9)),
        "cursor coordinates moved to the clicked pane"
    );

    assert_snapshot!(snapshot);
}

#[test]
fn move_pane_focus_with_mouse_to_non_floating_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_left_click(&Position::new(4, 71), client_id);
    tab.handle_mouse_release(&Position::new(4, 71), client_id);
    tab.render(&mut output, None);
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((1, 1)),
        "cursor coordinates moved to the clicked pane"
    );

    assert_snapshot!(snapshot);
}

#[test]
fn drag_pane_with_mouse() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_left_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_release(&Position::new(7, 75), client_id);
    tab.render(&mut output, None);
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((75, 11)),
        "cursor coordinates moved to the clicked pane"
    );

    assert_snapshot!(snapshot);
}

#[test]
fn mark_text_inside_floating_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_left_click(&Position::new(9, 71), client_id);
    assert!(
        tab.selecting_with_mouse,
        "started selecting with mouse on click"
    );
    tab.handle_mouse_release(&Position::new(8, 50), client_id);
    assert!(
        !tab.selecting_with_mouse,
        "stopped selecting with mouse on release"
    );
    tab.render(&mut output, None);
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((71, 9)),
        "cursor coordinates stayed in clicked pane"
    );

    assert_snapshot!(snapshot);
}

#[test]
fn resize_tab_with_floating_panes() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.resize_whole_tab(Size {
        cols: 100,
        rows: 10,
    });
    tab.render(&mut output, None);
    let (snapshot, _cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn shrink_whole_tab_with_floating_panes_horizontally_and_vertically() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.resize_whole_tab(Size { cols: 50, rows: 10 });
    tab.render(&mut output, None);
    let (snapshot, _cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn shrink_whole_tab_with_floating_panes_horizontally_and_vertically_and_expand_back() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id_1, Some(client_id));
    tab.new_pane(new_pane_id_2, Some(client_id));
    tab.new_pane(new_pane_id_3, Some(client_id));
    tab.new_pane(new_pane_id_4, Some(client_id));
    tab.new_pane(new_pane_id_5, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()));
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()));
    tab.resize_whole_tab(Size { cols: 50, rows: 10 });
    tab.resize_whole_tab(Size {
        cols: 121,
        rows: 20,
    });
    tab.render(&mut output, None);
    let (snapshot, _cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn embed_floating_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    );
    tab.toggle_pane_embed_or_floating(client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn float_embedded_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.new_pane(new_pane_id, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am an embedded pane".as_bytes()),
    );
    tab.toggle_pane_embed_or_floating(client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn cannot_float_only_embedded_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let mut output = Output::default();
    tab.handle_pty_bytes(
        1,
        Vec::from("\n\n\n                   I am an embedded pane".as_bytes()),
    );
    tab.toggle_pane_embed_or_floating(client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn replacing_existing_wide_characters() {
    // this is a real world use case using ncmpcpp with wide characters and scrolling
    // the reason we don't break it down is that it exposes quite a few edge cases with wide
    // characters that we should handle properly
    let size = Size {
        cols: 238,
        rows: 48,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size);
    let mut output = Output::default();
    let pane_content = read_fixture("ncmpcpp-wide-chars");
    tab.handle_pty_bytes(1, pane_content);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

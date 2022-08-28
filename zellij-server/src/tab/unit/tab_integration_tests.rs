use super::{Output, Tab};
use crate::panes::sixel::SixelImageStore;
use crate::screen::CopyOptions;
use crate::Arc;
use crate::Mutex;
use crate::{
    os_input_output::{AsyncReader, Pid, ServerOsApi},
    panes::PaneId,
    thread_bus::ThreadSenders,
    ClientId,
};
use std::convert::TryInto;
use std::path::PathBuf;
use zellij_utils::envs::set_session_name;
use zellij_utils::input::layout::LayoutTemplate;
use zellij_utils::ipc::IpcReceiverWithContext;
use zellij_utils::pane_size::{Size, SizeInPixels};
use zellij_utils::position::Position;

use crate::pty_writer::PtyWriteInstruction;
use zellij_utils::channels::{self, ChannelWithContext, SenderWithContext};

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::os::unix::io::RawFd;
use std::rc::Rc;

use zellij_utils::nix;

use zellij_utils::{
    data::{InputMode, ModeInfo, Palette, Style},
    input::command::TerminalAction,
    interprocess::local_socket::LocalSocketStream,
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

#[derive(Clone)]
struct FakeInputOutput {
    file_dumps: Arc<Mutex<HashMap<String, String>>>,
}

impl ServerOsApi for FakeInputOutput {
    fn set_terminal_size_using_fd(&self, _fd: RawFd, _cols: u16, _rows: u16) {
        // noop
    }
    fn spawn_terminal(
        &self,
        _file_to_open: TerminalAction,
        _quit_cb: Box<dyn Fn(PaneId) + Send>,
        _default_editor: Option<PathBuf>,
    ) -> Result<(RawFd, RawFd), &'static str> {
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
    fn write_to_file(&mut self, buf: String, name: Option<String>) {
        let f: String = match name {
            Some(x) => x,
            None => "tmp-name".to_owned(),
        };
        self.file_dumps.lock().unwrap().insert(f, buf);
    }
}

// TODO: move to shared thingy with other test file
fn create_new_tab(size: Size, default_mode: ModeInfo) -> Tab {
    set_session_name("test".into());
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput {
        file_dumps: Arc::new(Mutex::new(HashMap::new())),
    });
    let senders = ThreadSenders::default().silently_fail_on_send();
    let max_panes = None;
    let mode_info = default_mode;
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
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let mut tab = Tab::new(
        index,
        position,
        name,
        size,
        character_cell_info,
        sixel_image_store,
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
        terminal_emulator_color_codes,
    );
    tab.apply_layout(
        LayoutTemplate::default().try_into().unwrap(),
        vec![1],
        index,
        client_id,
    );
    tab
}

fn create_new_tab_with_mock_pty_writer(
    size: Size,
    default_mode: ModeInfo,
    mock_pty_writer: SenderWithContext<PtyWriteInstruction>,
) -> Tab {
    set_session_name("test".into());
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput {
        file_dumps: Arc::new(Mutex::new(HashMap::new())),
    });
    let mut senders = ThreadSenders::default().silently_fail_on_send();
    senders.replace_to_pty_writer(mock_pty_writer);
    let max_panes = None;
    let mode_info = default_mode;
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
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let mut tab = Tab::new(
        index,
        position,
        name,
        size,
        character_cell_info,
        sixel_image_store,
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
        terminal_emulator_color_codes,
    );
    tab.apply_layout(
        LayoutTemplate::default().try_into().unwrap(),
        vec![1],
        index,
        client_id,
    );
    tab
}

fn create_new_tab_with_sixel_support(
    size: Size,
    sixel_image_store: Rc<RefCell<SixelImageStore>>,
) -> Tab {
    // this is like the create_new_tab function but includes stuff needed for sixel,
    // eg. character_cell_size
    set_session_name("test".into());
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput {
        file_dumps: Arc::new(Mutex::new(HashMap::new())),
    });
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
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let copy_options = CopyOptions::default();
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let mut tab = Tab::new(
        index,
        position,
        name,
        size,
        character_cell_size,
        sixel_image_store,
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
        terminal_emulator_color_codes,
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
use insta::assert_snapshot;
use zellij_utils::vte;

fn take_snapshot(ansi_instructions: &str, rows: usize, columns: usize, palette: Palette) -> String {
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let mut grid = Grid::new(
        rows,
        columns,
        Rc::new(RefCell::new(palette)),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
    );
    let mut vte_parser = vte::Parser::new();
    for &byte in ansi_instructions.as_bytes() {
        vte_parser.advance(&mut grid, byte);
    }
    format!("{:?}", grid)
}

fn take_snapshot_with_sixel(
    ansi_instructions: &str,
    rows: usize,
    columns: usize,
    palette: Palette,
    sixel_image_store: Rc<RefCell<SixelImageStore>>,
) -> String {
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let mut grid = Grid::new(
        rows,
        columns,
        Rc::new(RefCell::new(palette)),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
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
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let mut grid = Grid::new(
        rows,
        columns,
        Rc::new(RefCell::new(palette)),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
    );
    let mut vte_parser = vte::Parser::new();
    for &byte in ansi_instructions.as_bytes() {
        vte_parser.advance(&mut grid, byte);
    }
    (format!("{:?}", grid), grid.cursor_coordinates())
}

#[test]
fn dump_screen() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let map = Arc::new(Mutex::new(HashMap::new()));
    tab.os_api = Box::new(FakeInputOutput {
        file_dumps: map.clone(),
    });
    let new_pane_id = PaneId::Terminal(2);
    tab.new_pane(new_pane_id, Some(client_id));
    tab.handle_pty_bytes(2, Vec::from("scratch".as_bytes()));
    let file = "/tmp/log.sh";
    tab.dump_active_terminal_screen(Some(file.to_string()), client_id);
    assert_eq!(
        map.lock().unwrap().get(file).unwrap(),
        "scratch",
        "screen was dumped properly"
    );
}

#[test]
fn new_floating_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    tab.handle_left_mouse_release(&Position::new(9, 71), client_id);
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    tab.handle_left_mouse_release(&Position::new(4, 71), client_id);
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id);
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    tab.handle_left_mouse_release(&Position::new(8, 50), client_id);
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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
    let mut tab = create_new_tab(size, ModeInfo::default());
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

#[test]
fn rename_embedded_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let mut output = Output::default();
    tab.handle_pty_bytes(
        1,
        Vec::from("\n\n\n                   I am an embedded pane".as_bytes()),
    );
    tab.update_active_pane_name("Renamed empedded pane".as_bytes().to_vec(), client_id);
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
fn rename_floating_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.new_pane(new_pane_id, Some(client_id));
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am a floating pane".as_bytes()),
    );
    tab.toggle_pane_embed_or_floating(client_id);
    tab.update_active_pane_name("Renamed floating pane".as_bytes().to_vec(), client_id);
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
fn wide_characters_in_left_title_side() {
    // this test makes sure the title doesn't overflow when it has wide characters
    let size = Size {
        cols: 238,
        rows: 48,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let mut output = Output::default();
    let pane_content = read_fixture("title-wide-chars");
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

#[test]
fn save_cursor_position_across_resizes() {
    // the save cursor position ANSI instruction (CSI s) needs to point to the same character after we
    // resize the pane
    let size = Size { cols: 100, rows: 5 };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let mut output = Output::default();

    tab.handle_pty_bytes(
        1,
        Vec::from("\n\nI am some text\nI am another line of text\nLet's save the cursor position here \u{1b}[sI should be ovewritten".as_bytes()),
    );
    tab.resize_whole_tab(Size { cols: 100, rows: 3 });
    tab.handle_pty_bytes(1, Vec::from("\u{1b}[uthis overwrote me!".as_bytes()));

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
fn move_floating_pane_with_sixel_image() {
    let new_pane_id = PaneId::Terminal(2);
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let mut tab = create_new_tab_with_sixel_support(size, sixel_image_store.clone());
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let mut output = Output::new(sixel_image_store.clone(), character_cell_size);

    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));
    let fixture = read_fixture("sixel-image-500px.six");
    tab.handle_pty_bytes(2, fixture);
    tab.handle_left_click(&Position::new(5, 71), client_id);
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id);

    tab.render(&mut output, None);
    let snapshot = take_snapshot_with_sixel(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
        sixel_image_store,
    );

    assert_snapshot!(snapshot);
}

#[test]
fn floating_pane_above_sixel_image() {
    let new_pane_id = PaneId::Terminal(2);
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let mut tab = create_new_tab_with_sixel_support(size, sixel_image_store.clone());
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 8,
        height: 21,
    })));
    let mut output = Output::new(sixel_image_store.clone(), character_cell_size);

    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));
    let fixture = read_fixture("sixel-image-500px.six");
    tab.handle_pty_bytes(1, fixture);
    tab.handle_left_click(&Position::new(5, 71), client_id);
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id);

    tab.render(&mut output, None);
    let snapshot = take_snapshot_with_sixel(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
        sixel_image_store,
    );

    assert_snapshot!(snapshot);
}

#[test]
fn suppress_tiled_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.suppress_active_pane(new_pane_id, client_id);
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()));
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
fn suppress_floating_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let new_pane_id = PaneId::Terminal(2);
    let editor_pane_id = PaneId::Terminal(3);
    let mut output = Output::default();

    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));
    tab.suppress_active_pane(editor_pane_id, client_id);
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()));
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
fn close_suppressing_tiled_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.suppress_active_pane(new_pane_id, client_id);
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()));
    tab.handle_pty_bytes(1, Vec::from("\n\n\nI am the original pane".as_bytes()));
    tab.close_pane(new_pane_id, false);
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
fn close_suppressing_floating_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let new_pane_id = PaneId::Terminal(2);
    let editor_pane_id = PaneId::Terminal(3);
    let mut output = Output::default();

    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));
    tab.suppress_active_pane(editor_pane_id, client_id);
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()));
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am the original pane".as_bytes()));
    tab.close_pane(editor_pane_id, false);
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
fn suppress_tiled_pane_float_it_and_close() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.suppress_active_pane(new_pane_id, client_id);
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()));
    tab.handle_pty_bytes(1, Vec::from("\n\n\nI am the original pane".as_bytes()));
    tab.toggle_pane_embed_or_floating(client_id);
    tab.close_pane(new_pane_id, false);
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
fn suppress_floating_pane_embed_it_and_close_it() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let new_pane_id = PaneId::Terminal(2);
    let editor_pane_id = PaneId::Terminal(3);
    let mut output = Output::default();

    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));
    tab.suppress_active_pane(editor_pane_id, client_id);
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()));
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am the original pane".as_bytes()));
    tab.toggle_pane_embed_or_floating(client_id);
    tab.close_pane(editor_pane_id, false);
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
fn resize_whole_tab_while_tiled_pane_is_suppressed() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.suppress_active_pane(new_pane_id, client_id);
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()));
    tab.resize_whole_tab(Size {
        cols: 100,
        rows: 10,
    });
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
fn resize_whole_tab_while_floting_pane_is_suppressed() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let new_pane_id = PaneId::Terminal(2);
    let editor_pane_id = PaneId::Terminal(3);
    let mut output = Output::default();

    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));
    tab.suppress_active_pane(editor_pane_id, client_id);
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()));
    tab.resize_whole_tab(Size {
        cols: 100,
        rows: 10,
    });
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
fn enter_search_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mode_info = ModeInfo {
        mode: InputMode::Search,
        ..Default::default()
    };
    let mut tab = create_new_tab(size, mode_info);
    let mut output = Output::default();
    let pane_content = read_fixture("grid_copy");
    tab.handle_pty_bytes(1, pane_content);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!("search_tab_nothing_highlighted", snapshot);

    // Pane title should show 'tortor' as search term
    // Only lines containing 'tortor' get marked as render-targets, so
    // only those are updated (search-styling is not visible here).
    tab.update_search_term("tortor".as_bytes().to_vec(), client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!("search_tab_highlight_tortor", snapshot);

    // Pane title should show search modifiers
    tab.toggle_search_wrap(client_id);
    tab.toggle_search_whole_words(client_id);
    tab.toggle_search_case_sensitivity(client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!("search_tab_highlight_tortor_modified", snapshot);

    // And only the search term again
    tab.toggle_search_wrap(client_id);
    tab.toggle_search_whole_words(client_id);
    tab.toggle_search_case_sensitivity(client_id);

    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!("search_tab_highlight_tortor", snapshot);
}

#[test]
fn enter_search_floating_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mode_info = ModeInfo {
        mode: InputMode::Search,
        ..Default::default()
    };
    let mut tab = create_new_tab(size, mode_info);
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.toggle_floating_panes(client_id, None);
    tab.new_pane(new_pane_id, Some(client_id));

    let pane_content = read_fixture("grid_copy");
    tab.handle_pty_bytes(2, pane_content);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!("search_floating_tab_nothing_highlighted", snapshot);

    // Only the line inside the floating tab which contain 'fring' should be in the new snapshot
    tab.update_search_term("fring".as_bytes().to_vec(), client_id);
    tab.render(&mut output, None);
    let snapshot = take_snapshot(
        output.serialize().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!("search_floating_tab_highlight_fring", snapshot);
}

#[test]
fn pane_in_sgr_button_event_tracking_mouse_mode() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;

    let messages_to_pty_writer = Arc::new(Mutex::new(vec![]));
    let (to_pty_writer, pty_writer_receiver): ChannelWithContext<PtyWriteInstruction> =
        channels::unbounded();
    let to_pty_writer = SenderWithContext::new(to_pty_writer);
    let mut tab = create_new_tab_with_mock_pty_writer(size, ModeInfo::default(), to_pty_writer);

    // TODO: note that this thread does not die when the test dies
    // it only dies once all the test process exits... not a biggy if we have only a handful of
    // these, but otherwise we might want to think of a better way to handle this
    let _pty_writer_thread = std::thread::Builder::new()
        .name("pty_writer".to_string())
        .spawn({
            // TODO: kill this thread
            let messages_to_pty_writer = messages_to_pty_writer.clone();
            move || loop {
                let (event, _err_ctx) = pty_writer_receiver
                    .recv()
                    .expect("failed to receive event on channel");
                if let PtyWriteInstruction::Write(msg, _) = event {
                    messages_to_pty_writer
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&msg).to_string());
                }
            }
        });
    let sgr_mouse_mode_any_button = String::from("\u{1b}[?1002;1006h"); // button event tracking (1002) with SGR encoding (1006)
    tab.handle_pty_bytes(1, sgr_mouse_mode_any_button.as_bytes().to_vec());
    tab.handle_left_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_left(&Position::new(9, 72), client_id);
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_right_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_right(&Position::new(9, 72), client_id);
    tab.handle_right_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_middle_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_middle(&Position::new(9, 72), client_id);
    tab.handle_middle_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_scrollwheel_up(&Position::new(5, 71), 1, client_id);
    tab.handle_scrollwheel_down(&Position::new(5, 71), 1, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for messages to arrive
    assert_eq!(
        *messages_to_pty_writer.lock().unwrap(),
        vec![
            "\u{1b}[<0;71;5M".to_string(),  // SGR left click
            "\u{1b}[<32;72;9M".to_string(), // SGR left click (hold)
            "\u{1b}[<0;75;7m".to_string(),  // SGR left button release
            "\u{1b}[<2;71;5M".to_string(),  // SGR right click
            "\u{1b}[<34;72;9M".to_string(), // SGR right click (hold)
            "\u{1b}[<2;75;7m".to_string(),  // SGR right button release
            "\u{1b}[<1;71;5M".to_string(),  // SGR middle click
            "\u{1b}[<33;72;9M".to_string(), // SGR middle click (hold)
            "\u{1b}[<1;75;7m".to_string(),  // SGR middle button release
            "\u{1b}[<64;71;5M".to_string(), // SGR scroll up
            "\u{1b}[<65;71;5M".to_string(), // SGR scroll down
        ]
    );
}

#[test]
fn pane_in_sgr_normal_event_tracking_mouse_mode() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;

    let messages_to_pty_writer = Arc::new(Mutex::new(vec![]));
    let (to_pty_writer, pty_writer_receiver): ChannelWithContext<PtyWriteInstruction> =
        channels::unbounded();
    let to_pty_writer = SenderWithContext::new(to_pty_writer);
    let mut tab = create_new_tab_with_mock_pty_writer(size, ModeInfo::default(), to_pty_writer);

    // TODO: note that this thread does not die when the test dies
    // it only dies once all the test process exits... not a biggy if we have only a handful of
    // these, but otherwise we might want to think of a better way to handle this
    let _pty_writer_thread = std::thread::Builder::new()
        .name("pty_writer".to_string())
        .spawn({
            // TODO: kill this thread
            let messages_to_pty_writer = messages_to_pty_writer.clone();
            move || loop {
                let (event, _err_ctx) = pty_writer_receiver
                    .recv()
                    .expect("failed to receive event on channel");
                if let PtyWriteInstruction::Write(msg, _) = event {
                    messages_to_pty_writer
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&msg).to_string());
                }
            }
        });
    let sgr_mouse_mode_any_button = String::from("\u{1b}[?1000;1006h"); // normal event tracking (1000) with sgr encoding (1006)
    tab.handle_pty_bytes(1, sgr_mouse_mode_any_button.as_bytes().to_vec());
    tab.handle_left_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_left(&Position::new(9, 72), client_id);
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_right_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_right(&Position::new(9, 72), client_id);
    tab.handle_right_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_middle_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_middle(&Position::new(9, 72), client_id);
    tab.handle_middle_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_scrollwheel_up(&Position::new(5, 71), 1, client_id);
    tab.handle_scrollwheel_down(&Position::new(5, 71), 1, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for messages to arrive
    assert_eq!(
        *messages_to_pty_writer.lock().unwrap(),
        vec![
            "\u{1b}[<0;71;5M".to_string(), // SGR left click
            // no hold event here, as hold events are not reported in normal mode
            "\u{1b}[<0;75;7m".to_string(), // SGR left button release
            "\u{1b}[<2;71;5M".to_string(), // SGR right click
            // no hold event here, as hold events are not reported in normal mode
            "\u{1b}[<2;75;7m".to_string(), // SGR right button release
            "\u{1b}[<1;71;5M".to_string(), // SGR middle click
            // no hold event here, as hold events are not reported in normal mode
            "\u{1b}[<1;75;7m".to_string(),  // SGR middle button release
            "\u{1b}[<64;71;5M".to_string(), // SGR scroll up
            "\u{1b}[<65;71;5M".to_string(), // SGR scroll down
        ]
    );
}

#[test]
fn pane_in_utf8_button_event_tracking_mouse_mode() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;

    let messages_to_pty_writer = Arc::new(Mutex::new(vec![]));
    let (to_pty_writer, pty_writer_receiver): ChannelWithContext<PtyWriteInstruction> =
        channels::unbounded();
    let to_pty_writer = SenderWithContext::new(to_pty_writer);
    let mut tab = create_new_tab_with_mock_pty_writer(size, ModeInfo::default(), to_pty_writer);

    // TODO: note that this thread does not die when the test dies
    // it only dies once all the test process exits... not a biggy if we have only a handful of
    // these, but otherwise we might want to think of a better way to handle this
    let _pty_writer_thread = std::thread::Builder::new()
        .name("pty_writer".to_string())
        .spawn({
            // TODO: kill this thread
            let messages_to_pty_writer = messages_to_pty_writer.clone();
            move || loop {
                let (event, _err_ctx) = pty_writer_receiver
                    .recv()
                    .expect("failed to receive event on channel");
                if let PtyWriteInstruction::Write(msg, _) = event {
                    messages_to_pty_writer
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&msg).to_string());
                }
            }
        });
    let sgr_mouse_mode_any_button = String::from("\u{1b}[?1002;1005h"); // button event tracking (1002) with utf8 encoding (1005)
    tab.handle_pty_bytes(1, sgr_mouse_mode_any_button.as_bytes().to_vec());
    tab.handle_left_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_left(&Position::new(9, 72), client_id);
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_right_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_right(&Position::new(9, 72), client_id);
    tab.handle_right_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_middle_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_middle(&Position::new(9, 72), client_id);
    tab.handle_middle_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_scrollwheel_up(&Position::new(5, 71), 1, client_id);
    tab.handle_scrollwheel_down(&Position::new(5, 71), 1, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for messages to arrive
    assert_eq!(
        *messages_to_pty_writer.lock().unwrap(),
        vec![
            "\u{1b}[M g%".to_string(),  // utf8 left click
            "\u{1b}[M@h)".to_string(),  // utf8 left click (hold)
            "\u{1b}[M#k'".to_string(),  // utf8 left button release
            "\u{1b}[M\"g%".to_string(), // utf8 right click
            "\u{1b}[MBh)".to_string(),  // utf8 right click (hold)
            "\u{1b}[M#k'".to_string(),  // utf8 right button release
            "\u{1b}[M!g%".to_string(),  // utf8 middle click
            "\u{1b}[MAh)".to_string(),  // utf8 middle click (hold)
            "\u{1b}[M#k'".to_string(),  // utf8 middle click release
            "\u{1b}[M`g%".to_string(),  // utf8 scroll up
            "\u{1b}[Mag%".to_string(),  // utf8 scroll down
        ]
    );
}

#[test]
fn pane_in_utf8_normal_event_tracking_mouse_mode() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;

    let messages_to_pty_writer = Arc::new(Mutex::new(vec![]));
    let (to_pty_writer, pty_writer_receiver): ChannelWithContext<PtyWriteInstruction> =
        channels::unbounded();
    let to_pty_writer = SenderWithContext::new(to_pty_writer);
    let mut tab = create_new_tab_with_mock_pty_writer(size, ModeInfo::default(), to_pty_writer);

    // TODO: note that this thread does not die when the test dies
    // it only dies once all the test process exits... not a biggy if we have only a handful of
    // these, but otherwise we might want to think of a better way to handle this
    let _pty_writer_thread = std::thread::Builder::new()
        .name("pty_writer".to_string())
        .spawn({
            // TODO: kill this thread
            let messages_to_pty_writer = messages_to_pty_writer.clone();
            move || loop {
                let (event, _err_ctx) = pty_writer_receiver
                    .recv()
                    .expect("failed to receive event on channel");
                if let PtyWriteInstruction::Write(msg, _) = event {
                    messages_to_pty_writer
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&msg).to_string());
                }
            }
        });
    let sgr_mouse_mode_any_button = String::from("\u{1b}[?1000;1005h"); // normal event tracking (1000) with sgr encoding (1006)
    tab.handle_pty_bytes(1, sgr_mouse_mode_any_button.as_bytes().to_vec());
    tab.handle_left_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_left(&Position::new(9, 72), client_id);
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_right_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_right(&Position::new(9, 72), client_id);
    tab.handle_right_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_middle_click(&Position::new(5, 71), client_id);
    tab.handle_mouse_hold_middle(&Position::new(9, 72), client_id);
    tab.handle_middle_mouse_release(&Position::new(7, 75), client_id);
    tab.handle_scrollwheel_up(&Position::new(5, 71), 1, client_id);
    tab.handle_scrollwheel_down(&Position::new(5, 71), 1, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for messages to arrive
    assert_eq!(
        *messages_to_pty_writer.lock().unwrap(),
        vec![
            "\u{1b}[M g%".to_string(), // utf8 left click
            // no hold event here, as hold events are not reported in normal mode
            "\u{1b}[M#k'".to_string(),  // utf8 left button release
            "\u{1b}[M\"g%".to_string(), // utf8 right click
            // no hold event here, as hold events are not reported in normal mode
            "\u{1b}[M#k'".to_string(), // utf8 right button release
            "\u{1b}[M!g%".to_string(), // utf8 middle click
            // no hold event here, as hold events are not reported in normal mode
            "\u{1b}[M#k'".to_string(), // utf8 middle click release
            "\u{1b}[M`g%".to_string(), // utf8 scroll up
            "\u{1b}[Mag%".to_string(), // utf8 scroll down
        ]
    );
}

#[test]
fn pane_bracketed_paste_ignored_when_not_in_bracketed_paste_mode() {
    // regression test for: https://github.com/zellij-org/zellij/issues/1687
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id: u16 = 1;

    let messages_to_pty_writer = Arc::new(Mutex::new(vec![]));
    let (to_pty_writer, pty_writer_receiver): ChannelWithContext<PtyWriteInstruction> =
        channels::unbounded();
    let to_pty_writer = SenderWithContext::new(to_pty_writer);
    let mut tab =
        create_new_tab_with_mock_pty_writer(size, ModeInfo::default(), to_pty_writer.clone());

    let _pty_writer_thread = std::thread::Builder::new()
        .name("pty_writer".to_string())
        .spawn({
            let messages_to_pty_writer = messages_to_pty_writer.clone();
            move || loop {
                let (event, _err_ctx) = pty_writer_receiver
                    .recv()
                    .expect("failed to receive event on channel");
                match event {
                    PtyWriteInstruction::Write(msg, _) => messages_to_pty_writer
                        .lock()
                        .unwrap()
                        .push(String::from_utf8_lossy(&msg).to_string()),
                    PtyWriteInstruction::Exit => break,
                }
            }
        });
    let bracketed_paste_start = vec![27, 91, 50, 48, 48, 126]; // \u{1b}[200~
    let bracketed_paste_end = vec![27, 91, 50, 48, 49, 126]; // \u{1b}[201
    tab.write_to_active_terminal(bracketed_paste_start, client_id);
    tab.write_to_active_terminal("test".as_bytes().to_vec(), client_id);
    tab.write_to_active_terminal(bracketed_paste_end, client_id);

    to_pty_writer.send(PtyWriteInstruction::Exit).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for messages to arrive
    assert_eq!(
        *messages_to_pty_writer.lock().unwrap(),
        vec!["", "test", ""]
    );
}

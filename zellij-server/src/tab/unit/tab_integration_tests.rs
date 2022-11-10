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
use std::path::PathBuf;
use zellij_utils::channels::Receiver;
use zellij_utils::envs::set_session_name;
use zellij_utils::errors::{prelude::*, ErrorContext};
use zellij_utils::input::layout::{Layout, PaneLayout};
use zellij_utils::ipc::IpcReceiverWithContext;
use zellij_utils::pane_size::{Size, SizeInPixels};
use zellij_utils::position::Position;

use crate::pty_writer::PtyWriteInstruction;
use zellij_utils::channels::{self, ChannelWithContext, SenderWithContext};

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::os::unix::io::RawFd;
use std::rc::Rc;

use zellij_utils::{
    data::{InputMode, ModeInfo, Palette, Style},
    input::command::{RunCommand, TerminalAction},
    interprocess::local_socket::LocalSocketStream,
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

#[derive(Clone, Default)]
struct FakeInputOutput {
    file_dumps: Arc<Mutex<HashMap<String, String>>>,
    pub tty_stdin_bytes: Arc<Mutex<BTreeMap<u32, Vec<u8>>>>,
}

impl ServerOsApi for FakeInputOutput {
    fn set_terminal_size_using_terminal_id(&self, _id: u32, _cols: u16, _rows: u16) -> Result<()> {
        // noop
        Ok(())
    }
    fn spawn_terminal(
        &self,
        _file_to_open: TerminalAction,
        _quit_db: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _default_editor: Option<PathBuf>,
    ) -> Result<(u32, RawFd, RawFd)> {
        unimplemented!()
    }
    fn read_from_tty_stdout(&self, _fd: RawFd, _buf: &mut [u8]) -> Result<usize> {
        unimplemented!()
    }
    fn async_file_reader(&self, _fd: RawFd) -> Box<dyn AsyncReader> {
        unimplemented!()
    }
    fn write_to_tty_stdin(&self, id: u32, buf: &[u8]) -> Result<usize> {
        self.tty_stdin_bytes
            .lock()
            .unwrap()
            .entry(id)
            .or_insert_with(std::vec::Vec::new)
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn tcdrain(&self, _id: u32) -> Result<()> {
        unimplemented!()
    }
    fn kill(&self, _pid: Pid) -> Result<()> {
        unimplemented!()
    }
    fn force_kill(&self, _pid: Pid) -> Result<()> {
        unimplemented!()
    }
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }
    fn send_to_client(&self, _client_id: ClientId, _msg: ServerToClientMsg) -> Result<()> {
        unimplemented!()
    }
    fn new_client(
        &mut self,
        _client_id: ClientId,
        _stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>> {
        unimplemented!()
    }
    fn remove_client(&mut self, _client_id: ClientId) -> Result<()> {
        unimplemented!()
    }
    fn load_palette(&self) -> Palette {
        unimplemented!()
    }
    fn get_cwd(&self, _pid: Pid) -> Option<PathBuf> {
        unimplemented!()
    }
    fn write_to_file(&mut self, buf: String, name: Option<String>) -> Result<()> {
        let f: String = match name {
            Some(x) => x,
            None => "tmp-name".to_owned(),
        };
        self.file_dumps.lock().to_anyhow()?.insert(f, buf);
        Ok(())
    }
    fn re_run_command_in_terminal(
        &self,
        _terminal_id: u32,
        _run_command: RunCommand,
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>, // u32 is the exit status
    ) -> Result<(RawFd, RawFd)> {
        unimplemented!()
    }
    fn clear_terminal_id(&self, _terminal_id: u32) -> Result<()> {
        unimplemented!()
    }
}

struct MockPtyInstructionBus {
    output: Arc<Mutex<Vec<String>>>,
    pty_writer_sender: SenderWithContext<PtyWriteInstruction>,
    pty_writer_receiver: Arc<Receiver<(PtyWriteInstruction, ErrorContext)>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl MockPtyInstructionBus {
    fn new() -> Self {
        let output = Arc::new(Mutex::new(vec![]));
        let (pty_writer_sender, pty_writer_receiver): ChannelWithContext<PtyWriteInstruction> =
            channels::unbounded();
        let pty_writer_sender = SenderWithContext::new(pty_writer_sender);
        let pty_writer_receiver = Arc::new(pty_writer_receiver);

        Self {
            output,
            pty_writer_sender,
            pty_writer_receiver,
            handle: None,
        }
    }

    fn start(&mut self) {
        let output = self.output.clone();
        let pty_writer_receiver = self.pty_writer_receiver.clone();
        let handle = std::thread::Builder::new()
            .name("pty_writer".to_string())
            .spawn({
                move || loop {
                    let (event, _err_ctx) = pty_writer_receiver
                        .recv()
                        .expect("failed to receive event on channel");
                    match event {
                        PtyWriteInstruction::Write(msg, _) => output
                            .lock()
                            .unwrap()
                            .push(String::from_utf8_lossy(&msg).to_string()),
                        PtyWriteInstruction::Exit => break,
                    }
                }
            })
            .unwrap();
        self.handle = Some(handle);
    }

    fn exit(&mut self) {
        self.pty_writer_sender
            .send(PtyWriteInstruction::Exit)
            .unwrap();
        let handle = self.handle.take().unwrap();
        handle.join().unwrap();
    }

    fn pty_write_sender(&self) -> SenderWithContext<PtyWriteInstruction> {
        self.pty_writer_sender.clone()
    }

    fn clone_output(&self) -> Vec<String> {
        self.output.lock().unwrap().clone()
    }
}

// TODO: move to shared thingy with other test file
fn create_new_tab(size: Size, default_mode: ModeInfo) -> Tab {
    set_session_name("test".into());
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput::default());
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
    tab.apply_layout(PaneLayout::default(), vec![(1, None)], index, client_id)
        .unwrap();
    tab
}

fn create_new_tab_with_os_api(
    size: Size,
    default_mode: ModeInfo,
    os_api: &Box<FakeInputOutput>,
) -> Tab {
    set_session_name("test".into());
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = os_api.clone();
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
    tab.apply_layout(PaneLayout::default(), vec![(1, None)], index, client_id)
        .unwrap();
    tab
}

fn create_new_tab_with_layout(size: Size, default_mode: ModeInfo, layout: &str) -> Tab {
    set_session_name("test".into());
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput::default());
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
    let layout = Layout::from_str(layout, "layout_file_name".into(), None).unwrap();
    let tab_layout = layout.new_tab();
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
    let pane_ids = tab_layout
        .extract_run_instructions()
        .iter()
        .enumerate()
        .map(|(i, _)| (i as u32, None))
        .collect();
    tab.apply_layout(tab_layout, pane_ids, index, client_id)
        .unwrap();
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
    let os_api = Box::new(FakeInputOutput::default());
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
    tab.apply_layout(PaneLayout::default(), vec![(1, None)], index, client_id)
        .unwrap();
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
    let os_api = Box::new(FakeInputOutput::default());
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
    tab.apply_layout(PaneLayout::default(), vec![(1, None)], index, client_id)
        .unwrap();
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
        ..Default::default()
    });
    let new_pane_id = PaneId::Terminal(2);
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(2, Vec::from("scratch".as_bytes()))
        .unwrap();
    let file = "/tmp/log.sh";
    tab.dump_active_terminal_screen(Some(file.to_string()), client_id, false)
        .unwrap();
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.toggle_floating_panes(client_id, None).unwrap();
    // here we send bytes to the pane when it's not visible to make sure they're still handled and
    // we see them once we toggle the panes back
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize_increase(client_id);
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize_decrease(client_id);
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize_left(client_id);
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize_right(client_id);
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize_up(client_id);
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize_down(client_id);
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.move_focus_left(client_id);
    tab.render(&mut output, None).unwrap();
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((3, 3)),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.move_focus_left(client_id);
    tab.move_focus_right(client_id);
    tab.render(&mut output, None).unwrap();
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((5, 5)),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.move_focus_up(client_id);
    tab.render(&mut output, None).unwrap();
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((3, 3)),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.move_focus_up(client_id);
    tab.move_focus_down(client_id);
    tab.render(&mut output, None).unwrap();
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((5, 5)),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_left_click(&Position::new(9, 71), client_id)
        .unwrap();
    tab.handle_left_mouse_release(&Position::new(9, 71), client_id)
        .unwrap();
    tab.render(&mut output, None).unwrap();
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((35, 10)),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_left_click(&Position::new(4, 71), client_id)
        .unwrap();
    tab.handle_left_mouse_release(&Position::new(4, 71), client_id)
        .unwrap();
    tab.render(&mut output, None).unwrap();
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_left_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.render(&mut output, None).unwrap();
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_left_click(&Position::new(6, 30), client_id)
        .unwrap();
    assert!(
        tab.selecting_with_mouse,
        "started selecting with mouse on click"
    );
    tab.handle_left_mouse_release(&Position::new(5, 15), client_id)
        .unwrap();
    assert!(
        !tab.selecting_with_mouse,
        "stopped selecting with mouse on release"
    );
    tab.render(&mut output, None).unwrap();
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((5, 5)),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.resize_whole_tab(Size {
        cols: 100,
        rows: 10,
    });
    tab.render(&mut output, None).unwrap();
    let (snapshot, _cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.resize_whole_tab(Size { cols: 50, rows: 10 });
    tab.render(&mut output, None).unwrap();
    let (snapshot, _cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(4, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(5, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(6, Vec::from("\u{1b}#8".as_bytes()))
        .unwrap();
    tab.resize_whole_tab(Size { cols: 50, rows: 10 });
    tab.resize_whole_tab(Size {
        cols: 121,
        rows: 20,
    });
    tab.render(&mut output, None).unwrap();
    let (snapshot, _cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am an embedded pane".as_bytes()),
    )
    .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn embed_floating_pane_without_pane_frames() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.set_pane_frames(false);
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn float_embedded_pane_without_pane_frames() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab(size, ModeInfo::default());
    let new_pane_id = PaneId::Terminal(2);
    let mut output = Output::default();
    tab.set_pane_frames(false);
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am an embedded pane".as_bytes()),
    )
    .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    )
    .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.handle_pty_bytes(1, pane_content).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    )
    .unwrap();
    tab.update_active_pane_name("Renamed empedded pane".as_bytes().to_vec(), client_id)
        .unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am a floating pane".as_bytes()),
    )
    .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.update_active_pane_name("Renamed floating pane".as_bytes().to_vec(), client_id)
        .unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.handle_pty_bytes(1, pane_content).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    ).unwrap();
    tab.resize_whole_tab(Size { cols: 100, rows: 3 });
    tab.handle_pty_bytes(1, Vec::from("\u{1b}[uthis overwrote me!".as_bytes()))
        .unwrap();

    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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

    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    let fixture = read_fixture("sixel-image-500px.six");
    tab.handle_pty_bytes(2, fixture).unwrap();
    tab.handle_left_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();

    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot_with_sixel(
        output.serialize().unwrap().get(&client_id).unwrap(),
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

    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    let fixture = read_fixture("sixel-image-500px.six");
    tab.handle_pty_bytes(1, fixture).unwrap();
    tab.handle_left_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();

    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot_with_sixel(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.suppress_active_pane(new_pane_id, client_id).unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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

    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.suppress_active_pane(editor_pane_id, client_id).unwrap();
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.suppress_active_pane(new_pane_id, client_id).unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(1, Vec::from("\n\n\nI am the original pane".as_bytes()))
        .unwrap();
    tab.close_pane(new_pane_id, false);
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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

    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.suppress_active_pane(editor_pane_id, client_id).unwrap();
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am the original pane".as_bytes()))
        .unwrap();
    tab.close_pane(editor_pane_id, false);
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.suppress_active_pane(new_pane_id, client_id).unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(1, Vec::from("\n\n\nI am the original pane".as_bytes()))
        .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.close_pane(new_pane_id, false);
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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

    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.suppress_active_pane(editor_pane_id, client_id).unwrap();
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am the original pane".as_bytes()))
        .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.close_pane(editor_pane_id, false);
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.suppress_active_pane(new_pane_id, client_id).unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.resize_whole_tab(Size {
        cols: 100,
        rows: 10,
    });
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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

    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();
    tab.suppress_active_pane(editor_pane_id, client_id).unwrap();
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.resize_whole_tab(Size {
        cols: 100,
        rows: 10,
    });
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.handle_pty_bytes(1, pane_content).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!("search_tab_nothing_highlighted", snapshot);

    // Pane title should show 'tortor' as search term
    // Only lines containing 'tortor' get marked as render-targets, so
    // only those are updated (search-styling is not visible here).
    tab.update_search_term("tortor".as_bytes().to_vec(), client_id)
        .unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!("search_tab_highlight_tortor", snapshot);

    // Pane title should show search modifiers
    tab.toggle_search_wrap(client_id);
    tab.toggle_search_whole_words(client_id);
    tab.toggle_search_case_sensitivity(client_id);
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!("search_tab_highlight_tortor_modified", snapshot);

    // And only the search term again
    tab.toggle_search_wrap(client_id);
    tab.toggle_search_whole_words(client_id);
    tab.toggle_search_case_sensitivity(client_id);

    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id, None, None, Some(client_id))
        .unwrap();

    let pane_content = read_fixture("grid_copy");
    tab.handle_pty_bytes(2, pane_content).unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!("search_floating_tab_nothing_highlighted", snapshot);

    // Only the line inside the floating tab which contain 'fring' should be in the new snapshot
    tab.update_search_term("fring".as_bytes().to_vec(), client_id)
        .unwrap();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
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

    let mut pty_instruction_bus = MockPtyInstructionBus::new();
    let mut tab = create_new_tab_with_mock_pty_writer(
        size,
        ModeInfo::default(),
        pty_instruction_bus.pty_write_sender(),
    );
    pty_instruction_bus.start();

    let sgr_mouse_mode_any_button = String::from("\u{1b}[?1002;1006h"); // button event tracking (1002) with SGR encoding (1006)
    tab.handle_pty_bytes(1, sgr_mouse_mode_any_button.as_bytes().to_vec())
        .unwrap();
    tab.handle_left_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_left(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_right_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_right(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_right_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_middle_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_middle(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_middle_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_scrollwheel_up(&Position::new(5, 71), 1, client_id)
        .unwrap();
    tab.handle_scrollwheel_down(&Position::new(5, 71), 1, client_id)
        .unwrap();

    pty_instruction_bus.exit();

    assert_eq!(
        pty_instruction_bus.clone_output(),
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

    let mut pty_instruction_bus = MockPtyInstructionBus::new();
    let mut tab = create_new_tab_with_mock_pty_writer(
        size,
        ModeInfo::default(),
        pty_instruction_bus.pty_write_sender(),
    );
    pty_instruction_bus.start();

    let sgr_mouse_mode_any_button = String::from("\u{1b}[?1000;1006h"); // normal event tracking (1000) with sgr encoding (1006)
    tab.handle_pty_bytes(1, sgr_mouse_mode_any_button.as_bytes().to_vec())
        .unwrap();
    tab.handle_left_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_left(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_right_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_right(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_right_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_middle_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_middle(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_middle_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_scrollwheel_up(&Position::new(5, 71), 1, client_id)
        .unwrap();
    tab.handle_scrollwheel_down(&Position::new(5, 71), 1, client_id)
        .unwrap();

    pty_instruction_bus.exit();

    assert_eq!(
        pty_instruction_bus.clone_output(),
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

    let mut pty_instruction_bus = MockPtyInstructionBus::new();
    let mut tab = create_new_tab_with_mock_pty_writer(
        size,
        ModeInfo::default(),
        pty_instruction_bus.pty_write_sender(),
    );
    pty_instruction_bus.start();

    let sgr_mouse_mode_any_button = String::from("\u{1b}[?1002;1005h"); // button event tracking (1002) with utf8 encoding (1005)
    tab.handle_pty_bytes(1, sgr_mouse_mode_any_button.as_bytes().to_vec())
        .unwrap();
    tab.handle_left_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_left(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_right_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_right(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_right_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_middle_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_middle(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_middle_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_scrollwheel_up(&Position::new(5, 71), 1, client_id)
        .unwrap();
    tab.handle_scrollwheel_down(&Position::new(5, 71), 1, client_id)
        .unwrap();

    pty_instruction_bus.exit();

    assert_eq!(
        pty_instruction_bus.clone_output(),
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

    let mut pty_instruction_bus = MockPtyInstructionBus::new();
    let mut tab = create_new_tab_with_mock_pty_writer(
        size,
        ModeInfo::default(),
        pty_instruction_bus.pty_write_sender(),
    );
    pty_instruction_bus.start();

    let sgr_mouse_mode_any_button = String::from("\u{1b}[?1000;1005h"); // normal event tracking (1000) with sgr encoding (1006)
    tab.handle_pty_bytes(1, sgr_mouse_mode_any_button.as_bytes().to_vec())
        .unwrap();
    tab.handle_left_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_left(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_right_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_right(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_right_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_middle_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_mouse_hold_middle(&Position::new(9, 72), client_id)
        .unwrap();
    tab.handle_middle_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();
    tab.handle_scrollwheel_up(&Position::new(5, 71), 1, client_id)
        .unwrap();
    tab.handle_scrollwheel_down(&Position::new(5, 71), 1, client_id)
        .unwrap();

    pty_instruction_bus.exit();

    assert_eq!(
        pty_instruction_bus.clone_output(),
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
fn tab_with_basic_layout() {
    let layout = r#"
        layout {
            pane split_direction="Vertical" {
                pane
                pane split_direction="Horizontal" {
                    pane
                    pane
                }
            }
        }
    "#;
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab_with_layout(size, ModeInfo::default(), layout);
    let mut output = Output::default();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn tab_with_nested_layout() {
    let layout = r#"
        layout {
            pane_template name="top-and-vertical-sandwich" {
                pane
                vertical-sandwich {
                    pane
                }
            }
            pane_template name="vertical-sandwich" split_direction="vertical" {
                pane
                children
                pane
            }
            pane_template name="nested-vertical-sandwich" split_direction="vertical" {
                pane
                top-and-vertical-sandwich
                pane
            }
            nested-vertical-sandwich
        }
    "#;
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab_with_layout(size, ModeInfo::default(), layout);
    let mut output = Output::default();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn tab_with_nested_uneven_layout() {
    let layout = r#"
        layout {
            pane_template name="horizontal-with-vertical-top" {
                pane split_direction="Vertical" {
                    pane
                    children
                }
                pane
            }
            horizontal-with-vertical-top name="my tab" {
                pane
                pane
            }
        }
    "#;
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut tab = create_new_tab_with_layout(size, ModeInfo::default(), layout);
    let mut output = Output::default();
    tab.render(&mut output, None).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn pane_bracketed_paste_ignored_when_not_in_bracketed_paste_mode() {
    // regression test for: https://github.com/zellij-org/zellij/issues/1687
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id: u16 = 1;

    let mut pty_instruction_bus = MockPtyInstructionBus::new();
    let mut tab = create_new_tab_with_mock_pty_writer(
        size,
        ModeInfo::default(),
        pty_instruction_bus.pty_write_sender(),
    );
    pty_instruction_bus.start();

    let bracketed_paste_start = vec![27, 91, 50, 48, 48, 126]; // \u{1b}[200~
    let bracketed_paste_end = vec![27, 91, 50, 48, 49, 126]; // \u{1b}[201
    tab.write_to_active_terminal(bracketed_paste_start, client_id)
        .unwrap();
    tab.write_to_active_terminal("test".as_bytes().to_vec(), client_id)
        .unwrap();
    tab.write_to_active_terminal(bracketed_paste_end, client_id)
        .unwrap();

    pty_instruction_bus.exit();

    assert_eq!(pty_instruction_bus.clone_output(), vec!["", "test", ""]);
}

#[test]
fn pane_faux_scrolling_in_alternate_mode() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id: u16 = 1;
    let lines_to_scroll = 3;

    let mut pty_instruction_bus = MockPtyInstructionBus::new();
    let mut tab = create_new_tab_with_mock_pty_writer(
        size,
        ModeInfo::default(),
        pty_instruction_bus.pty_write_sender(),
    );
    pty_instruction_bus.start();

    let enable_alternate_screen = String::from("\u{1b}[?1049h"); // CSI ? 1049 h -> switch to the Alternate Screen Buffer
    let set_application_mode = String::from("\u{1b}[?1h");

    // no output since alternate scren not active yet
    tab.handle_scrollwheel_up(&Position::new(1, 1), lines_to_scroll, client_id)
        .unwrap();
    tab.handle_scrollwheel_down(&Position::new(1, 1), lines_to_scroll, client_id)
        .unwrap();

    tab.handle_pty_bytes(1, enable_alternate_screen.as_bytes().to_vec())
        .unwrap();
    // CSI A * lines_to_scroll, CSI B * lines_to_scroll
    tab.handle_scrollwheel_up(&Position::new(1, 1), lines_to_scroll, client_id)
        .unwrap();
    tab.handle_scrollwheel_down(&Position::new(1, 1), lines_to_scroll, client_id)
        .unwrap();

    tab.handle_pty_bytes(1, set_application_mode.as_bytes().to_vec())
        .unwrap();
    // SS3 A * lines_to_scroll, SS3 B * lines_to_scroll
    tab.handle_scrollwheel_up(&Position::new(1, 1), lines_to_scroll, client_id)
        .unwrap();
    tab.handle_scrollwheel_down(&Position::new(1, 1), lines_to_scroll, client_id)
        .unwrap();

    pty_instruction_bus.exit();

    let mut expected: Vec<&str> = Vec::new();
    expected.append(&mut vec!["\u{1b}[A"; lines_to_scroll]);
    expected.append(&mut vec!["\u{1b}[B"; lines_to_scroll]);
    expected.append(&mut vec!["\u{1b}OA"; lines_to_scroll]);
    expected.append(&mut vec!["\u{1b}OB"; lines_to_scroll]);

    assert_eq!(pty_instruction_bus.clone_output(), expected);
}

#[test]
fn move_pane_focus_sends_tty_csi_event() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let tty_stdin_bytes = Arc::new(Mutex::new(BTreeMap::new()));
    let os_api = Box::new(FakeInputOutput {
        tty_stdin_bytes: tty_stdin_bytes.clone(),
        ..Default::default()
    });
    let mut tab = create_new_tab_with_os_api(size, ModeInfo::default(), &os_api);
    let new_pane_id_1 = PaneId::Terminal(2);
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        1,
        // subscribe to focus events
        Vec::from("\u{1b}[?1004h".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(
        2,
        // subscribe to focus events
        Vec::from("\u{1b}[?1004h".as_bytes()),
    )
    .unwrap();
    tab.move_focus_left(client_id);
    assert_snapshot!(format!("{:?}", *tty_stdin_bytes.lock().unwrap()));
}

#[test]
fn move_floating_pane_focus_sends_tty_csi_event() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let tty_stdin_bytes = Arc::new(Mutex::new(BTreeMap::new()));
    let os_api = Box::new(FakeInputOutput {
        tty_stdin_bytes: tty_stdin_bytes.clone(),
        ..Default::default()
    });
    let mut tab = create_new_tab_with_os_api(size, ModeInfo::default(), &os_api);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);

    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        1,
        // subscribe to focus events
        Vec::from("\u{1b}[?1004h".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(
        2,
        // subscribe to focus events
        Vec::from("\u{1b}[?1004h".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(
        3,
        // subscribe to focus events
        Vec::from("\u{1b}[?1004h".as_bytes()),
    )
    .unwrap();
    tab.move_focus_left(client_id);
    assert_snapshot!(format!("{:?}", *tty_stdin_bytes.lock().unwrap()));
}

#[test]
fn toggle_floating_panes_on_sends_tty_csi_event() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let tty_stdin_bytes = Arc::new(Mutex::new(BTreeMap::new()));
    let os_api = Box::new(FakeInputOutput {
        tty_stdin_bytes: tty_stdin_bytes.clone(),
        ..Default::default()
    });
    let mut tab = create_new_tab_with_os_api(size, ModeInfo::default(), &os_api);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);

    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.handle_pty_bytes(
        1,
        // subscribe to focus events
        Vec::from("\u{1b}[?1004h".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(
        2,
        // subscribe to focus events
        Vec::from("\u{1b}[?1004h".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(
        3,
        // subscribe to focus events
        Vec::from("\u{1b}[?1004h".as_bytes()),
    )
    .unwrap();
    tab.toggle_floating_panes(client_id, None).unwrap();
    assert_snapshot!(format!("{:?}", *tty_stdin_bytes.lock().unwrap()));
}

#[test]
fn toggle_floating_panes_off_sends_tty_csi_event() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let tty_stdin_bytes = Arc::new(Mutex::new(BTreeMap::new()));
    let os_api = Box::new(FakeInputOutput {
        tty_stdin_bytes: tty_stdin_bytes.clone(),
        ..Default::default()
    });
    let mut tab = create_new_tab_with_os_api(size, ModeInfo::default(), &os_api);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);

    tab.toggle_floating_panes(client_id, None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        1,
        // subscribe to focus events
        Vec::from("\u{1b}[?1004h".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(
        2,
        // subscribe to focus events
        Vec::from("\u{1b}[?1004h".as_bytes()),
    )
    .unwrap();
    tab.handle_pty_bytes(
        3,
        // subscribe to focus events
        Vec::from("\u{1b}[?1004h".as_bytes()),
    )
    .unwrap();
    tab.toggle_floating_panes(client_id, None).unwrap();
    assert_snapshot!(format!("{:?}", *tty_stdin_bytes.lock().unwrap()));
}

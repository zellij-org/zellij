use super::{Output, Tab};
use crate::panes::sixel::SixelImageStore;
use crate::screen::CopyOptions;
use crate::Arc;

use crate::{
    os_input_output::{AsyncReader, Pid, ServerOsApi},
    panes::PaneId,
    plugins::PluginInstruction,
    thread_bus::ThreadSenders,
    ClientId,
};
use std::path::PathBuf;
use std::sync::Mutex;

use zellij_utils::channels::Receiver;
use zellij_utils::data::Direction;
use zellij_utils::data::Resize;
use zellij_utils::data::ResizeStrategy;
use zellij_utils::envs::set_session_name;
use zellij_utils::errors::{prelude::*, ErrorContext};
use zellij_utils::input::layout::{
    FloatingPaneLayout, Layout, PluginUserConfiguration, RunPluginLocation, RunPluginOrAlias,
    SwapFloatingLayout, SwapTiledLayout, TiledPaneLayout,
};
use zellij_utils::input::plugins::PluginTag;
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
    fn set_terminal_size_using_terminal_id(
        &self,
        _id: u32,
        _cols: u16,
        _rows: u16,
        _width_in_pixels: Option<u16>,
        _height_in_pixels: Option<u16>,
    ) -> Result<()> {
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
            .or_insert_with(|| vec![])
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
                        _ => {},
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
    let auto_layout = true;
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
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
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
        auto_layout,
        connected_clients,
        session_is_mirrored,
        client_id,
        copy_options,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        (vec![], vec![]),
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    );
    tab.apply_layout(
        TiledPaneLayout::default(),
        vec![],
        vec![(1, None)],
        vec![],
        HashMap::new(),
        client_id,
    )
    .unwrap();
    tab
}

fn create_new_tab_with_swap_layouts(
    size: Size,
    default_mode: ModeInfo,
    swap_layouts: (Vec<SwapTiledLayout>, Vec<SwapFloatingLayout>),
    base_layout_and_ids: Option<(
        TiledPaneLayout,
        Vec<FloatingPaneLayout>,
        Vec<(u32, Option<RunCommand>)>,
        Vec<(u32, Option<RunCommand>)>,
        HashMap<RunPluginOrAlias, Vec<u32>>,
    )>,
    draw_pane_frames: bool,
) -> Tab {
    set_session_name("test".into());
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput::default());
    let mut senders = ThreadSenders::default().silently_fail_on_send();
    let (mock_plugin_sender, _mock_plugin_receiver): ChannelWithContext<PluginInstruction> =
        channels::unbounded();
    senders.replace_to_plugin(SenderWithContext::new(mock_plugin_sender));
    let max_panes = None;
    let mode_info = default_mode;
    let style = Style::default();
    let auto_layout = true;
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
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
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
        auto_layout,
        connected_clients,
        session_is_mirrored,
        client_id,
        copy_options,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        swap_layouts,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let (
        base_layout,
        base_floating_layout,
        new_terminal_ids,
        new_floating_terminal_ids,
        new_plugin_ids,
    ) = base_layout_and_ids.unwrap_or_default();
    let new_terminal_ids = if new_terminal_ids.is_empty() {
        vec![(1, None)]
    } else {
        new_terminal_ids
    };
    tab.apply_layout(
        base_layout,
        base_floating_layout,
        new_terminal_ids,
        new_floating_terminal_ids,
        new_plugin_ids,
        client_id,
    )
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
    let auto_layout = true;
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
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
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
        auto_layout,
        connected_clients,
        session_is_mirrored,
        client_id,
        copy_options,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        (vec![], vec![]), // swap layouts
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    );
    tab.apply_layout(
        TiledPaneLayout::default(),
        vec![],
        vec![(1, None)],
        vec![],
        HashMap::new(),
        client_id,
    )
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
    let auto_layout = true;
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
    let layout = Layout::from_str(layout, "layout_file_name".into(), None, None).unwrap();
    let (tab_layout, floating_panes_layout) = layout.new_tab();
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
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
        auto_layout,
        connected_clients,
        session_is_mirrored,
        client_id,
        copy_options,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        (vec![], vec![]), // swap layouts
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    );
    let pane_ids = tab_layout
        .extract_run_instructions()
        .iter()
        .enumerate()
        .map(|(i, _)| (i as u32, None))
        .collect();
    let floating_pane_ids = floating_panes_layout
        .iter()
        .enumerate()
        .map(|(i, _)| (i as u32, None))
        .collect();
    tab.apply_layout(
        tab_layout,
        floating_panes_layout,
        pane_ids,
        floating_pane_ids,
        HashMap::new(),
        client_id,
    )
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
    let auto_layout = true;
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
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
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
        auto_layout,
        connected_clients,
        session_is_mirrored,
        client_id,
        copy_options,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        (vec![], vec![]), // swap layouts
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    );
    tab.apply_layout(
        TiledPaneLayout::default(),
        vec![],
        vec![(1, None)],
        vec![],
        HashMap::new(),
        client_id,
    )
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
    let auto_layout = true;
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
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
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
        auto_layout,
        connected_clients,
        session_is_mirrored,
        client_id,
        copy_options,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        (vec![], vec![]), // swap layouts
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    );
    tab.apply_layout(
        TiledPaneLayout::default(),
        vec![],
        vec![(1, None)],
        vec![],
        HashMap::new(),
        client_id,
    )
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
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        rows,
        columns,
        Rc::new(RefCell::new(palette)),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        rows,
        columns,
        Rc::new(RefCell::new(palette)),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        character_cell_size,
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut grid = Grid::new(
        rows,
        columns,
        Rc::new(RefCell::new(palette)),
        terminal_emulator_color_codes,
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Style::default(),
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
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
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
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
fn clear_screen() {
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
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(2, Vec::from("scratch".as_bytes()))
        .unwrap();
    let file = "/tmp/log-clear-screen.sh";
    tab.clear_active_terminal_screen(client_id).unwrap();
    tab.dump_active_terminal_screen(Some(file.to_string()), client_id, false)
        .unwrap();
    assert_eq!(
        map.lock().unwrap().get(file).unwrap(),
        "",
        "screen was cleared properly"
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    // here we send bytes to the pane when it's not visible to make sure they're still handled and
    // we see them once we toggle the panes back
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize(client_id, ResizeStrategy::new(Resize::Increase, None))
        .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize(client_id, ResizeStrategy::new(Resize::Decrease, None))
        .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize(
        client_id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Left)),
    )
    .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize(
        client_id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Right)),
    )
    .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize(
        client_id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Up)),
    )
    .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.resize(
        client_id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Down)),
    )
    .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    tab.move_focus_left(client_id).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    tab.move_focus_left(client_id).unwrap();
    tab.move_focus_right(client_id).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    tab.move_focus_up(client_id).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    tab.move_focus_up(client_id).unwrap();
    tab.move_focus_down(client_id).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    })
    .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    tab.resize_whole_tab(Size { cols: 50, rows: 10 }).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
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
    tab.resize_whole_tab(Size { cols: 50, rows: 10 }).unwrap();
    tab.resize_whole_tab(Size {
        cols: 121,
        rows: 20,
    })
    .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am an embedded pane".as_bytes()),
    )
    .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am scratch terminal".as_bytes()),
    )
    .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am an embedded pane".as_bytes()),
    )
    .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.render(&mut output).unwrap();
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
    tab.render(&mut output).unwrap();
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
    tab.render(&mut output).unwrap();
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
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_pty_bytes(
        2,
        Vec::from("\n\n\n                   I am a floating pane".as_bytes()),
    )
    .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.update_active_pane_name("Renamed floating pane".as_bytes().to_vec(), client_id)
        .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.render(&mut output).unwrap();
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
        Vec::from("\n\n\rI am some text\n\rI am another line of text\n\rLet's save the cursor position here \u{1b}[sI should be ovewritten".as_bytes()),
    ).unwrap();

    // We check cursor and saved cursor are handled separately by:
    // 1. moving real cursor up two lines
    tab.handle_pty_bytes(1, Vec::from("\u{1b}[2A".as_bytes()));
    // 2. resizing so real cursor gets lost above the viewport, which resets it to row 0
    // The saved cursor ends up on row 1, allowing detection if it (incorrectly) gets reset too
    tab.resize_whole_tab(Size { cols: 35, rows: 4 }).unwrap();

    // Now overwrite
    tab.handle_pty_bytes(1, Vec::from("\u{1b}[uthis overwrote me!".as_bytes()))
        .unwrap();

    tab.resize_whole_tab(Size { cols: 100, rows: 3 }).unwrap();

    tab.render(&mut output).unwrap();
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
    let mut output = Output::new(sixel_image_store.clone(), character_cell_size, true);

    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    let fixture = read_fixture("sixel-image-500px.six");
    tab.handle_pty_bytes(2, fixture).unwrap();
    tab.handle_left_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();

    tab.render(&mut output).unwrap();
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
    let mut output = Output::new(sixel_image_store.clone(), character_cell_size, true);

    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    let fixture = read_fixture("sixel-image-500px.six");
    tab.handle_pty_bytes(1, fixture).unwrap();
    tab.handle_left_click(&Position::new(5, 71), client_id)
        .unwrap();
    tab.handle_left_mouse_release(&Position::new(7, 75), client_id)
        .unwrap();

    tab.render(&mut output).unwrap();
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
    tab.replace_active_pane_with_editor_pane(new_pane_id, client_id)
        .unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.render(&mut output).unwrap();
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

    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.replace_active_pane_with_editor_pane(editor_pane_id, client_id)
        .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.replace_active_pane_with_editor_pane(new_pane_id, client_id)
        .unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(1, Vec::from("\n\n\nI am the original pane".as_bytes()))
        .unwrap();
    tab.close_pane(new_pane_id, false, None);
    tab.render(&mut output).unwrap();
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

    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.replace_active_pane_with_editor_pane(editor_pane_id, client_id)
        .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am the original pane".as_bytes()))
        .unwrap();
    tab.close_pane(editor_pane_id, false, None);
    tab.render(&mut output).unwrap();
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
    tab.replace_active_pane_with_editor_pane(new_pane_id, client_id)
        .unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(1, Vec::from("\n\n\nI am the original pane".as_bytes()))
        .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.close_pane(new_pane_id, false, None);
    tab.render(&mut output).unwrap();
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

    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.replace_active_pane_with_editor_pane(editor_pane_id, client_id)
        .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am the original pane".as_bytes()))
        .unwrap();
    tab.toggle_pane_embed_or_floating(client_id).unwrap();
    tab.close_pane(editor_pane_id, false, None);
    tab.render(&mut output).unwrap();
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
    tab.replace_active_pane_with_editor_pane(new_pane_id, client_id)
        .unwrap();
    tab.handle_pty_bytes(2, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.resize_whole_tab(Size {
        cols: 100,
        rows: 10,
    })
    .unwrap();
    tab.render(&mut output).unwrap();
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

    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();
    tab.replace_active_pane_with_editor_pane(editor_pane_id, client_id)
        .unwrap();
    tab.handle_pty_bytes(3, Vec::from("\n\n\nI am an editor pane".as_bytes()))
        .unwrap();
    tab.resize_whole_tab(Size {
        cols: 100,
        rows: 10,
    })
    .unwrap();
    tab.render(&mut output).unwrap();
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
    tab.render(&mut output).unwrap();
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
    tab.render(&mut output).unwrap();
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
    tab.render(&mut output).unwrap();
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

    tab.render(&mut output).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id, None, None, None, None, Some(client_id))
        .unwrap();

    let pane_content = read_fixture("grid_copy");
    tab.handle_pty_bytes(2, pane_content).unwrap();
    tab.render(&mut output).unwrap();
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
    tab.render(&mut output).unwrap();
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
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn tab_with_layout_that_has_floating_panes() {
    let layout = r#"
        layout {
            pane
            floating_panes {
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
    tab.render(&mut output).unwrap();
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
    tab.render(&mut output).unwrap();
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
    tab.render(&mut output).unwrap();
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
    tab.write_to_active_terminal(&None, bracketed_paste_start, false, client_id)
        .unwrap();
    tab.write_to_active_terminal(&None, "test".as_bytes().to_vec(), false, client_id)
        .unwrap();
    tab.write_to_active_terminal(&None, bracketed_paste_end, false, client_id)
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
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
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
    tab.move_focus_left(client_id).unwrap();
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

    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
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
    tab.move_focus_left(client_id).unwrap();
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

    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
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

    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
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
    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    assert_snapshot!(format!("{:?}", *tty_stdin_bytes.lock().unwrap()));
}

#[test]
fn can_swap_tiled_layout_at_runtime() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab max_panes=2 split_direction="vertical" {
                    pane
                    pane
                }
            }
            swap_tiled_layout {
                tab max_panes=2 {
                    pane
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn can_swap_floating_layout_at_runtime() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_floating_layout {
                floating_panes max_panes=2 {
                    pane
                    pane
                }
            }
            swap_floating_layout {
                floating_panes max_panes=2 {
                    pane {
                        x "0%"
                    }
                    pane {
                        x "100%"
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);

    tab.toggle_floating_panes(Some(client_id), None).unwrap();
    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn swapping_layouts_after_resize_snaps_to_current_layout() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane
                    pane
                }
            }
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane
                        pane
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.resize(client_id, ResizeStrategy::new(Resize::Increase, None))
        .unwrap();
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn swap_tiled_layout_with_stacked_children() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn swap_tiled_layout_with_only_stacked_children() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane stacked=true { children; }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn swap_tiled_layout_with_stacked_children_and_no_pane_frames() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        false,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn move_focus_up_with_stacked_panes() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.move_focus_right(client_id);
    tab.move_focus_up(client_id);
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn move_focus_down_with_stacked_panes() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.move_focus_right(client_id);
    tab.move_focus_up(client_id);
    tab.move_focus_down(client_id);
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn move_focus_right_into_stacked_panes() {
    // here we make sure that when we focus right into a stack,
    // we will always focus on the "main" pane of the stack
    // and not on one of its folds
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    for i in 0..12 {
        let new_pane_id = i + 2;
        tab.new_pane(
            PaneId::Terminal(new_pane_id),
            None,
            None,
            None,
            None,
            Some(client_id),
        )
        .unwrap();
    }
    tab.move_focus_left(client_id);
    tab.horizontal_split(PaneId::Terminal(16), None, client_id)
        .unwrap();

    tab.move_focus_up(client_id);
    tab.move_focus_right(client_id);
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((62, 12)),
        "cursor coordinates moved to the main pane of the stack",
    );

    assert_snapshot!(snapshot);
}

#[test]
fn move_focus_left_into_stacked_panes() {
    // here we make sure that when we focus left into a stack,
    // we will always focus on the "main" pane of the stack
    // and not on one of its folds
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane stacked=true { children; }
                        pane focus=true
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    for i in 0..13 {
        let new_pane_id = i + 2;
        tab.new_pane(
            PaneId::Terminal(new_pane_id),
            None,
            None,
            None,
            None,
            Some(client_id),
        )
        .unwrap();
    }
    tab.move_focus_right(client_id);
    tab.horizontal_split(PaneId::Terminal(1), None, client_id)
        .unwrap();

    tab.move_focus_up(client_id);
    tab.move_focus_left(client_id);
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((1, 12)),
        "cursor coordinates moved to the main pane of the stack",
    );

    assert_snapshot!(snapshot);
}

#[test]
fn move_focus_up_into_stacked_panes() {
    // here we make sure that when we focus up into a stack,
    // the main pane will become the lowest pane and the sizes
    // in the stack will be adjusted accordingly
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    for i in 0..4 {
        let new_pane_id = i + 3;
        tab.new_pane(
            PaneId::Terminal(new_pane_id),
            None,
            None,
            None,
            None,
            Some(client_id),
        )
        .unwrap();
    }
    tab.move_focus_right(client_id);
    tab.move_focus_up(client_id);
    tab.move_focus_left(client_id);
    tab.move_focus_down(client_id);
    tab.vertical_split(PaneId::Terminal(7), None, client_id)
        .unwrap();

    tab.move_focus_up(client_id);
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((62, 9)),
        "cursor coordinates moved to the main pane of the stack",
    );
    assert_snapshot!(snapshot);
}

#[test]
fn move_focus_down_into_stacked_panes() {
    // here we make sure that when we focus down into a stack,
    // the main pane will become the highest pane and the sizes
    // in the stack will be adjusted accordingly
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    for i in 0..4 {
        let new_pane_id = i + 3;
        tab.new_pane(
            PaneId::Terminal(new_pane_id),
            None,
            None,
            None,
            None,
            Some(client_id),
        )
        .unwrap();
    }
    tab.move_focus_left(client_id);
    tab.move_focus_up(client_id);
    tab.vertical_split(PaneId::Terminal(7), None, client_id)
        .unwrap();

    tab.move_focus_down(client_id);
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((62, 8)),
        "cursor coordinates moved to the main pane of the stack",
    );

    assert_snapshot!(snapshot);
}

#[test]
fn close_main_stacked_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.close_pane(new_pane_id_2, false, None);
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn close_main_stacked_pane_in_mid_stack() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.move_focus_right(client_id);
    tab.move_focus_up(client_id);
    tab.move_focus_up(client_id);
    tab.close_pane(new_pane_id_3, false, None);
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn close_one_liner_stacked_pane_below_main_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.move_focus_left(client_id);
    tab.move_focus_right(client_id);
    tab.move_focus_up(client_id);
    tab.move_focus_up(client_id);
    tab.close_pane(new_pane_id_2, false, None);
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn close_one_liner_stacked_pane_above_main_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.move_focus_right(client_id);
    tab.move_focus_up(client_id);
    tab.move_focus_up(client_id);
    tab.close_pane(new_pane_id_1, false, None);
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn can_increase_size_of_main_pane_in_stack_horizontally() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.move_focus_right(client_id);
    tab.resize(
        client_id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Left)),
    )
    .unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn can_increase_size_of_main_pane_in_stack_vertically() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.move_focus_right(client_id);
    tab.resize(
        client_id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Down)),
    )
    .unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn can_increase_size_of_main_pane_in_stack_non_directionally() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    let _ = tab.move_focus_up(client_id);
    let _ = tab.move_focus_right(client_id);
    tab.resize(client_id, ResizeStrategy::new(Resize::Increase, None))
        .unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn can_increase_size_into_pane_stack_horizontally() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.resize(
        client_id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Right)),
    )
    .unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn can_increase_size_into_pane_stack_vertically() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.move_focus_right(client_id);
    tab.move_focus_down(client_id);
    tab.resize(
        client_id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Up)),
    )
    .unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn can_increase_size_into_pane_stack_non_directionally() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    let _ = tab.move_focus_up(client_id);
    tab.resize(client_id, ResizeStrategy::new(Resize::Increase, None))
        .unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn decreasing_size_of_whole_tab_treats_stacked_panes_properly() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.resize_whole_tab(Size {
        cols: 100,
        rows: 10,
    });
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn increasing_size_of_whole_tab_treats_stacked_panes_properly() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.resize_whole_tab(Size {
        cols: 100,
        rows: 10,
    });
    tab.resize_whole_tab(Size {
        cols: 121,
        rows: 20,
    });
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn cannot_decrease_stack_size_beyond_minimum_height() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.move_focus_down(client_id);
    for _ in 0..6 {
        tab.resize(
            client_id,
            ResizeStrategy::new(Resize::Increase, Some(Direction::Up)),
        )
        .unwrap();
    }
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn focus_stacked_pane_over_flexible_pane_with_the_mouse() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_left_click(&Position::new(1, 71), client_id)
        .unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn focus_stacked_pane_under_flexible_pane_with_the_mouse() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_left_click(&Position::new(1, 71), client_id)
        .unwrap();
    tab.handle_left_click(&Position::new(9, 71), client_id)
        .unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn close_stacked_pane_with_previously_focused_other_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.handle_left_click(&Position::new(2, 71), client_id)
        .unwrap();
    tab.handle_left_click(&Position::new(1, 71), client_id)
        .unwrap();
    tab.close_pane(PaneId::Terminal(4), false, None);
    tab.render(&mut output).unwrap();
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((62, 2)),
        "cursor coordinates moved to the main pane of the stack",
    );

    assert_snapshot!(snapshot);
}

#[test]
fn close_pane_near_stacked_panes() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane
                        pane stacked=true { children; }
                    }
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.close_pane(PaneId::Terminal(6), false, None);
    tab.render(&mut output).unwrap();
    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((62, 4)),
        "cursor coordinates moved to the main pane of the stack",
    );

    assert_snapshot!(snapshot);
}

#[test]
fn focus_next_pane_expands_stacked_panes() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.move_focus_left(client_id);
    tab.focus_next_pane(client_id);
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn stacked_panes_can_become_fullscreen() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane stacked=true { children; }
                    }
                    pane
                }
            }
        }
    "#;
    let layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        None,
        true,
    );
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);

    tab.new_pane(new_pane_id_1, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_2, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_3, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_4, None, None, None, None, Some(client_id))
        .unwrap();
    tab.new_pane(new_pane_id_5, None, None, None, None, Some(client_id))
        .unwrap();
    tab.move_focus_up(client_id);
    tab.toggle_active_pane_fullscreen(client_id);
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn layout_with_plugins_and_commands_swaped_properly() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            pane size=1 borderless=true {
                plugin location="zellij:tab-bar"
            }
            pane split_direction="vertical" {
                pane command="command1"
                pane
                pane command="command2"
            }
            pane size=2 borderless=true {
                plugin location="zellij:status-bar"
            }
        }
    "#;
    // this swap layout changes both the split direction of the two command panes and the location
    // of the plugins - we want to make sure that they are all placed properly and not switched
    // around
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane size=2 borderless=true {
                        plugin location="zellij:status-bar"
                    }
                    pane command="command2"
                    pane command="command1"
                    pane
                    pane size=1 borderless=true {
                        plugin location="zellij:tab-bar"
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let mut command_1 = RunCommand::default();
    command_1.command = PathBuf::from("command1");
    let mut command_2 = RunCommand::default();
    command_2.command = PathBuf::from("command2");
    let new_terminal_ids = vec![(1, Some(command_1)), (2, None), (3, Some(command_2))];
    let new_floating_terminal_ids = vec![];
    let mut new_plugin_ids = HashMap::new();
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap(),
        vec![1],
    );
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap(),
        vec![2],
    );

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let _ = tab.handle_plugin_bytes(1, 1, "I am a tab bar".as_bytes().to_vec());
    let _ = tab.handle_plugin_bytes(2, 1, "I am a\n\rstatus bar".as_bytes().to_vec());
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn base_layout_is_included_in_swap_layouts() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            pane size=1 borderless=true {
                plugin location="zellij:tab-bar"
            }
            pane split_direction="vertical" {
                pane command="command1"
                pane
                pane command="command2"
            }
            pane size=2 borderless=true {
                plugin location="zellij:status-bar"
            }
        }
    "#;
    // this swap layout changes both the split direction of the two command panes and the location
    // of the plugins - we want to make sure that they are all placed properly and not switched
    // around
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane size=2 borderless=true {
                        plugin location="zellij:status-bar"
                    }
                    pane command="command2"
                    pane command="command1"
                    pane
                    pane size=1 borderless=true {
                        plugin location="zellij:tab-bar"
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let mut command_1 = RunCommand::default();
    command_1.command = PathBuf::from("command1");
    let mut command_2 = RunCommand::default();
    command_2.command = PathBuf::from("command2");
    let new_terminal_ids = vec![(1, Some(command_1)), (2, None), (3, Some(command_2))];
    let new_floating_terminal_ids = vec![];
    let mut new_plugin_ids = HashMap::new();
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap(),
        vec![1],
    );
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap(),
        vec![2],
    );

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let _ = tab.handle_plugin_bytes(1, 1, "I am a tab bar".as_bytes().to_vec());
    let _ = tab.handle_plugin_bytes(2, 1, "I am a\n\rstatus bar".as_bytes().to_vec());
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.previous_swap_layout(Some(client_id)).unwrap(); // move back to the base layout
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn swap_layouts_including_command_panes_absent_from_existing_layout() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            pane size=1 borderless=true {
                plugin location="zellij:tab-bar"
            }
            pane split_direction="vertical" {
                pane
                pane
                pane
            }
            pane size=2 borderless=true {
                plugin location="zellij:status-bar"
            }
        }
    "#;
    // this swap layout changes both the split direction of the two command panes and the location
    // of the plugins - we want to make sure that they are all placed properly and not switched
    // around
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane size=2 borderless=true {
                        plugin location="zellij:status-bar"
                    }
                    pane command="command2"
                    pane command="command1"
                    pane
                    pane size=1 borderless=true {
                        plugin location="zellij:tab-bar"
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_floating_terminal_ids = vec![];
    let mut new_plugin_ids = HashMap::new();
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap(),
        vec![1],
    );
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap(),
        vec![2],
    );

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let _ = tab.handle_plugin_bytes(1, 1, "I am a tab bar".as_bytes().to_vec());
    let _ = tab.handle_plugin_bytes(2, 1, "I am a\n\rstatus bar".as_bytes().to_vec());
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn swap_layouts_not_including_command_panes_present_in_existing_layout() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            pane size=1 borderless=true {
                plugin location="zellij:tab-bar"
            }
            pane split_direction="vertical" {
                pane command="command1"
                pane
                pane command="command2"
            }
            pane size=2 borderless=true {
                plugin location="zellij:status-bar"
            }
        }
    "#;
    // this swap layout changes both the split direction of the two command panes and the location
    // of the plugins - we want to make sure that they are all placed properly and not switched
    // around
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane size=2 borderless=true {
                        plugin location="zellij:status-bar"
                    }
                    pane
                    pane
                    pane
                    pane size=1 borderless=true {
                        plugin location="zellij:tab-bar"
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let mut command_1 = RunCommand::default();
    command_1.command = PathBuf::from("command1");
    let mut command_2 = RunCommand::default();
    command_2.command = PathBuf::from("command2");
    let new_terminal_ids = vec![(1, Some(command_1)), (2, None), (3, Some(command_2))];
    let new_floating_terminal_ids = vec![];
    let mut new_plugin_ids = HashMap::new();
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap(),
        vec![1],
    );
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap(),
        vec![2],
    );

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let _ = tab.handle_plugin_bytes(1, 1, "I am a tab bar".as_bytes().to_vec());
    let _ = tab.handle_plugin_bytes(2, 1, "I am a\n\rstatus bar".as_bytes().to_vec());
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn swap_layouts_including_plugin_panes_absent_from_existing_layout() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            pane size=2 borderless=true
            pane split_direction="vertical" {
                pane
                pane
                pane
            }
            pane size=1 borderless=true
        }
    "#;
    // this swap layout changes both the split direction of the two command panes and the location
    // of the plugins - we want to make sure that they are all placed properly and not switched
    // around
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane size=2 borderless=true {
                        plugin location="zellij:status-bar"
                    }
                    pane
                    pane
                    pane
                    pane size=1 borderless=true {
                        plugin location="zellij:tab-bar"
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_floating_terminal_ids = vec![];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn swap_layouts_not_including_plugin_panes_present_in_existing_layout() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            pane size=1 borderless=true {
                plugin location="zellij:tab-bar"
            }
            pane split_direction="vertical" {
                pane command="command1"
                pane
                pane command="command2"
            }
            pane size=2 borderless=true {
                plugin location="zellij:status-bar"
            }
        }
    "#;
    // this swap layout changes both the split direction of the two command panes and the location
    // of the plugins - we want to make sure that they are all placed properly and not switched
    // around
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane size=2
                    pane
                    pane
                    pane
                    pane size=1
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let mut command_1 = RunCommand::default();
    command_1.command = PathBuf::from("command1");
    let mut command_2 = RunCommand::default();
    command_2.command = PathBuf::from("command2");
    let new_terminal_ids = vec![(1, Some(command_1)), (2, None), (3, Some(command_2))];
    let new_floating_terminal_ids = vec![];
    let mut new_plugin_ids = HashMap::new();
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap(),
        vec![1],
    );
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap(),
        vec![2],
    );

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let _ = tab.handle_plugin_bytes(1, 1, "I am a tab bar".as_bytes().to_vec());
    let _ = tab.handle_plugin_bytes(2, 1, "I am a\n\rstatus bar".as_bytes().to_vec());
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn new_pane_in_auto_layout() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout
    "#;
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab max_panes=5 {
                    pane split_direction="vertical" {
                        pane
                        pane { children; }
                    }
                }
                tab max_panes=8 {
                    pane split_direction="vertical" {
                        pane { children; }
                        pane { pane; pane; pane; pane; }
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_floating_terminal_ids = vec![];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );

    let mut expected_cursor_coordinates = vec![
        (62, 1),
        (62, 11),
        (62, 15),
        (62, 16),
        (1, 11),
        (1, 15),
        (1, 16),
    ];
    for i in 0..7 {
        let new_pane_id = i + 2;
        tab.new_pane(
            PaneId::Terminal(new_pane_id),
            None,
            None,
            None,
            None,
            Some(client_id),
        )
        .unwrap();
        tab.render(&mut output).unwrap();

        let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
            output.serialize().unwrap().get(&client_id).unwrap(),
            size.rows,
            size.cols,
            Palette::default(),
        );
        let (expected_x, expected_y) = expected_cursor_coordinates.remove(0);
        assert_eq!(
            cursor_coordinates,
            Some((expected_x, expected_y)),
            "cursor coordinates moved to the new pane",
        );
        assert_snapshot!(snapshot);
    }
}

#[test]
fn when_swapping_tiled_layouts_in_a_damaged_state_layout_and_pane_focus_are_unchanged() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            pane
            pane
            pane
        }
    "#;
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane
                        pane
                        pane
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_floating_terminal_ids = vec![];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    tab.move_focus_down(client_id);
    tab.resize(
        client_id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Down)),
    )
    .unwrap();
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((1, 8)),
        "cursor coordinates moved to the new pane",
    );

    assert_snapshot!(snapshot);
}

#[test]
fn when_swapping_tiled_layouts_in_an_undamaged_state_pane_focuses_on_focused_node() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            pane
            pane
            pane
        }
    "#;
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane focus=true
                        pane
                        pane
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_floating_terminal_ids = vec![];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    tab.move_focus_down(client_id);
    tab.next_swap_layout(Some(client_id), true).unwrap();
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((1, 1)),
        "cursor coordinates moved to the new pane",
    );

    assert_snapshot!(snapshot);
}

#[test]
fn when_swapping_tiled_layouts_in_an_undamaged_state_with_no_focus_node_pane_focuses_on_deepest_node(
) {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            pane
            pane
            pane
        }
    "#;
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane
                        pane
                        pane
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_floating_terminal_ids = vec![];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    tab.move_focus_down(client_id);
    tab.next_swap_layout(Some(client_id), true).unwrap();
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((82, 1)),
        "cursor coordinates moved to the new pane",
    );

    assert_snapshot!(snapshot);
}

#[test]
fn when_closing_a_pane_in_auto_layout_the_focus_goes_to_last_focused_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            pane
            pane
            pane
        }
    "#;
    let swap_layouts = r#"
        layout {
            swap_tiled_layout {
                tab {
                    pane split_direction="vertical" {
                        pane
                        pane
                        pane
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_floating_terminal_ids = vec![];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let _ = tab.move_focus_down(client_id);
    let _ = tab.move_focus_down(client_id);
    tab.close_pane(PaneId::Terminal(3), false, Some(client_id));
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((62, 1)),
        "cursor coordinates moved to the new pane",
    );
    assert_snapshot!(snapshot);
}

#[test]
fn floating_layout_with_plugins_and_commands_swaped_properly() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            floating_panes {
                pane x=0 y=0 {
                    plugin location="zellij:tab-bar"
                }
                pane x=0 y=10 command="command1"
                pane
                pane x=50 y=10 command="command2"
                pane x=50 y=0 {
                    plugin location="zellij:status-bar"
                }
            }
        }
    "#;
    // this swap layout swaps between the location of the plugins and the commands
    let swap_layouts = r#"
        layout {
            swap_floating_layout {
                floating_panes {
                    pane x=0 y=0 {
                        plugin location="zellij:status-bar"
                    }
                    pane x=0 y=10 command="command2"
                    pane
                    pane x=50 y=10 command="command1"
                    pane x=50 y=0 {
                        plugin location="zellij:tab-bar"
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let mut command_1 = RunCommand::default();
    command_1.command = PathBuf::from("command1");
    let mut command_2 = RunCommand::default();
    command_2.command = PathBuf::from("command2");
    let new_floating_terminal_ids = vec![(1, Some(command_1)), (2, None), (3, Some(command_2))];
    let new_terminal_ids = vec![(4, None)];
    let mut new_plugin_ids = HashMap::new();
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap(),
        vec![1],
    );
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap(),
        vec![2],
    );

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let _ = tab.handle_plugin_bytes(1, 1, "I am a tab bar".as_bytes().to_vec());
    let _ = tab.handle_plugin_bytes(2, 1, "I am a\n\rstatus bar".as_bytes().to_vec());
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn base_floating_layout_is_included_in_swap_layouts() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            floating_panes {
                pane x=0 y=0 {
                    plugin location="zellij:tab-bar"
                }
                pane x=0 y=10 command="command1"
                pane
                pane x=50 y=10 command="command2"
                pane x=50 y=0 {
                    plugin location="zellij:status-bar"
                }
            }
        }
    "#;
    // this swap layout swaps between the location of the plugins and the commands
    let swap_layouts = r#"
        layout {
            swap_floating_layout {
                floating_panes {
                    pane x=0 y=0 {
                        plugin location="zellij:status-bar"
                    }
                    pane x=0 y=10 command="command2"
                    pane
                    pane x=50 y=10 command="command1"
                    pane x=50 y=0 {
                        plugin location="zellij:tab-bar"
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let mut command_1 = RunCommand::default();
    command_1.command = PathBuf::from("command1");
    let mut command_2 = RunCommand::default();
    command_2.command = PathBuf::from("command2");
    let new_floating_terminal_ids = vec![(1, Some(command_1)), (2, None), (3, Some(command_2))];
    let new_terminal_ids = vec![(4, None)];
    let mut new_plugin_ids = HashMap::new();
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap(),
        vec![1],
    );
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap(),
        vec![2],
    );

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let _ = tab.handle_plugin_bytes(1, 1, "I am a tab bar".as_bytes().to_vec());
    let _ = tab.handle_plugin_bytes(2, 1, "I am a\n\rstatus bar".as_bytes().to_vec());
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.previous_swap_layout(Some(client_id)).unwrap(); // move back to the base layout
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn swap_floating_layouts_including_command_panes_absent_from_existing_layout() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            floating_panes {
                pane {
                    plugin location="zellij:tab-bar"
                }
                pane
                pane
                pane
                pane {
                    plugin location="zellij:status-bar"
                }
            }
        }
    "#;
    // this swap layout changes both the split direction of the two command panes and the location
    // of the plugins - we want to make sure that they are all placed properly and not switched
    // around
    let swap_layouts = r#"
        layout {
            swap_floating_layout {
                floating_panes {
                    pane {
                        plugin location="zellij:status-bar"
                    }
                    pane x=0 y=0 command="command1"
                    pane x=10 y=10 command="command2"
                    pane
                    pane {
                        plugin location="zellij:tab-bar"
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_floating_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_terminal_ids = vec![(4, None)];
    let mut new_plugin_ids = HashMap::new();
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap(),
        vec![1],
    );
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap(),
        vec![2],
    );

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let _ = tab.handle_plugin_bytes(1, 1, "I am a tab bar".as_bytes().to_vec());
    let _ = tab.handle_plugin_bytes(2, 1, "I am a\n\rstatus bar".as_bytes().to_vec());
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn swap_floating_layouts_not_including_command_panes_present_in_existing_layout() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            floating_panes {
                pane {
                    plugin location="zellij:tab-bar"
                }
                pane command="command1"
                pane
                pane command="command2"
                pane {
                    plugin location="zellij:status-bar"
                }
            }
        }
    "#;
    // this swap layout changes both the split direction of the two command panes and the location
    // of the plugins - we want to make sure that they are all placed properly and not switched
    // around
    let swap_layouts = r#"
        layout {
            swap_floating_layout {
                floating_panes {
                    pane {
                        plugin location="zellij:status-bar"
                    }
                    pane
                    pane
                    pane
                    pane {
                        plugin location="zellij:tab-bar"
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let mut command_1 = RunCommand::default();
    command_1.command = PathBuf::from("command1");
    let mut command_2 = RunCommand::default();
    command_2.command = PathBuf::from("command2");
    let new_floating_terminal_ids = vec![(1, Some(command_1)), (2, None), (3, Some(command_2))];
    let new_terminal_ids = vec![(4, None)];
    let mut new_plugin_ids = HashMap::new();
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap(),
        vec![1],
    );
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap(),
        vec![2],
    );

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let _ = tab.handle_plugin_bytes(1, 1, "I am a tab bar".as_bytes().to_vec());
    let _ = tab.handle_plugin_bytes(2, 1, "I am a\n\rstatus bar".as_bytes().to_vec());
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn swap_floating_layouts_including_plugin_panes_absent_from_existing_layout() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            floating_panes {
                pane
                pane
                pane
            }
        }
    "#;
    let swap_layouts = r#"
        layout {
            swap_floating_layout {
                floating_panes {
                    pane {
                        plugin location="zellij:status-bar"
                    }
                    pane
                    pane {
                        plugin location="zellij:tab-bar"
                    }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_floating_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_terminal_ids = vec![(4, None)];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn swap_floating_layouts_not_including_plugin_panes_present_in_existing_layout() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            floating_panes {
                pane {
                    plugin location="zellij:tab-bar"
                }
                pane
                pane {
                    plugin location="zellij:status-bar"
                }
            }
        }
    "#;
    // this swap layout changes both the split direction of the two command panes and the location
    // of the plugins - we want to make sure that they are all placed properly and not switched
    // around
    let swap_layouts = r#"
        layout {
            swap_floating_layout {
                floating_panes {
                    pane
                    pane
                    pane
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let mut command_1 = RunCommand::default();
    command_1.command = PathBuf::from("command1");
    let mut command_2 = RunCommand::default();
    command_2.command = PathBuf::from("command2");
    let new_floating_terminal_ids = vec![(1, Some(command_1)), (2, None), (3, Some(command_2))];
    let new_terminal_ids = vec![(4, None)];
    let mut new_plugin_ids = HashMap::new();
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap(),
        vec![1],
    );
    new_plugin_ids.insert(
        RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap(),
        vec![2],
    );

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let _ = tab.handle_plugin_bytes(1, 1, "I am a tab bar".as_bytes().to_vec());
    let _ = tab.handle_plugin_bytes(2, 1, "I am a\n\rstatus bar".as_bytes().to_vec());
    tab.next_swap_layout(Some(client_id), false).unwrap();
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );

    assert_snapshot!(snapshot);
}

#[test]
fn new_floating_pane_in_auto_layout() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout
    "#;
    let swap_layouts = r#"
        layout {
            swap_floating_layout name="spread" {
                floating_panes max_panes=1 {
                    pane {y "50%"; x "50%"; }
                }
                floating_panes max_panes=2 {
                    pane { x "1%"; y "25%"; width "45%"; }
                    pane { x "50%"; y "25%"; width "45%"; }
                }
                floating_panes max_panes=3 {
                    pane focus=true { y "55%"; width "45%"; height "45%"; }
                    pane { x "1%"; y "1%"; width "45%"; }
                    pane { x "50%"; y "1%"; width "45%"; }
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_terminal_ids = vec![(1, None)];
    let new_floating_terminal_ids = vec![];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );

    let mut expected_cursor_coordinates = vec![(62, 11), (62, 6), (31, 12)];
    for i in 0..3 {
        let new_pane_id = i + 2;
        let should_float = true;
        tab.new_pane(
            PaneId::Terminal(new_pane_id),
            None,
            Some(should_float),
            None,
            None,
            Some(client_id),
        )
        .unwrap();
        tab.render(&mut output).unwrap();

        let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
            output.serialize().unwrap().get(&client_id).unwrap(),
            size.rows,
            size.cols,
            Palette::default(),
        );
        let (expected_x, expected_y) = expected_cursor_coordinates.remove(0);
        assert_eq!(
            cursor_coordinates,
            Some((expected_x, expected_y)),
            "cursor coordinates moved to the new pane",
        );
        assert_snapshot!(snapshot);
    }
}

#[test]
fn when_swapping_floating_layouts_in_a_damaged_state_layout_and_pane_focus_are_unchanged() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            floating_panes {
                pane x=0 y=0
                pane
                pane
            }
        }
    "#;
    let swap_layouts = r#"
        layout {
            swap_floating_layout {
                floating_panes {
                    pane
                    pane
                    pane
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_floating_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_terminal_ids = vec![(4, None)];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    tab.resize(
        client_id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Down)),
    )
    .unwrap();
    tab.next_swap_layout(Some(client_id), true).unwrap();
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((33, 8)),
        "cursor coordinates moved to the new pane",
    );

    assert_snapshot!(snapshot);
}

#[test]
fn when_swapping_floating_layouts_in_an_undamaged_state_pane_focuses_on_focused_node() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            floating_panes {
                pane x=0 y=0
                pane
                pane
            }
        }
    "#;
    let swap_layouts = r#"
        layout {
            swap_floating_layout {
                floating_panes {
                    pane focus=true
                    pane
                    pane
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_floating_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_terminal_ids = vec![(4, None)];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    tab.next_swap_layout(Some(client_id), true).unwrap();
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((31, 6)),
        "cursor coordinates moved to the new pane",
    );

    assert_snapshot!(snapshot);
}

#[test]
fn when_swapping_floating_layouts_in_an_undamaged_state_with_no_focus_node_pane_focuses_on_deepest_node(
) {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            floating_panes {
                pane focus=true
                pane
                pane
            }
        }
    "#;
    let swap_layouts = r#"
        layout {
            swap_floating_layout {
                floating_panes {
                    pane
                    pane
                    pane
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_floating_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_terminal_ids = vec![(4, None)];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    tab.next_swap_layout(Some(client_id), true).unwrap();
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((35, 10)),
        "cursor coordinates moved to the new pane",
    );

    assert_snapshot!(snapshot);
}

#[test]
fn when_closing_a_floating_pane_in_auto_layout_the_focus_goes_to_last_focused_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            floating_panes {
                pane
                pane
                pane
            }
        }
    "#;
    let swap_layouts = r#"
        layout {
            swap_floating_layout {
                floating_panes {
                    pane
                    pane
                    pane
                }
            }
        }
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_floating_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_terminal_ids = vec![(4, None)];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    tab.move_focus_up(client_id);
    tab.move_focus_up(client_id);
    tab.close_pane(PaneId::Terminal(1), false, Some(client_id));
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((31, 6)),
        "cursor coordinates moved to the new pane",
    );
    assert_snapshot!(snapshot);
}

#[test]
fn when_resizing_whole_tab_with_auto_layout_and_floating_panes_the_layout_is_maintained() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 1;
    let mut output = Output::default();
    let base_layout = r#"
        layout {
            floating_panes {
                pane
                pane
                pane
            }
        }
    "#;
    let swap_layouts = r#"
        layout
    "#;
    let (base_layout, base_floating_layout) =
        Layout::from_kdl(base_layout, "file_name.kdl".into(), None, None)
            .unwrap()
            .template
            .unwrap();

    let new_floating_terminal_ids = vec![(1, None), (2, None), (3, None)];
    let new_terminal_ids = vec![(4, None)];
    let new_plugin_ids = HashMap::new();

    let swap_layout = Layout::from_kdl(swap_layouts, "file_name.kdl".into(), None, None).unwrap();
    let swap_tiled_layouts = swap_layout.swap_tiled_layouts.clone();
    let swap_floating_layouts = swap_layout.swap_floating_layouts.clone();
    let mut tab = create_new_tab_with_swap_layouts(
        size,
        ModeInfo::default(),
        (swap_tiled_layouts, swap_floating_layouts),
        Some((
            base_layout,
            base_floating_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
        )),
        true,
    );
    let new_size = Size {
        cols: 150,
        rows: 30,
    };
    tab.resize_whole_tab(new_size);
    tab.render(&mut output).unwrap();

    let (snapshot, cursor_coordinates) = take_snapshot_and_cursor_position(
        output.serialize().unwrap().get(&client_id).unwrap(),
        new_size.rows,
        new_size.cols,
        Palette::default(),
    );
    assert_eq!(
        cursor_coordinates,
        Some((43, 13)),
        "cursor coordinates moved to the new pane",
    );
    assert_snapshot!(snapshot);
}

#[test]
fn when_applying_a_truncated_swap_layout_child_attributes_are_not_ignored() {
    // here we want to make sure that the nested borderless is preserved on resize (when the layout
    // is reapplied, and thus is truncated to just one pane rather than a logical container pane
    // and an actual pane as it is described here)
    let layout = r#"
        layout {
            pane {
                pane borderless=true
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
    let new_size = Size {
        cols: 122,
        rows: 20,
    };
    let _ = tab.resize_whole_tab(new_size);
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        new_size.rows,
        new_size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

#[test]
fn can_define_expanded_pane_in_stack() {
    let layout = r#"
        layout {
            pane split_direction="Vertical" {
                pane
                pane stacked=true {
                    pane
                    pane expanded=true
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
    tab.render(&mut output).unwrap();
    let snapshot = take_snapshot(
        output.serialize().unwrap().get(&client_id).unwrap(),
        size.rows,
        size.cols,
        Palette::default(),
    );
    assert_snapshot!(snapshot);
}

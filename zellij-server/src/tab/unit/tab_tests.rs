use super::Tab;
use crate::pane_groups::PaneGroups;
use crate::panes::sixel::SixelImageStore;
use crate::screen::CopyOptions;
use crate::{os_input_output::ServerOsApi, panes::PaneId, thread_bus::ThreadSenders, ClientId};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use zellij_utils::data::{Direction, NewPanePlacement, Resize, ResizeStrategy, WebSharing};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::layout::{SplitDirection, SplitSize, TiledPaneLayout};
use zellij_utils::ipc::IpcReceiverWithContext;
use zellij_utils::pane_size::{Size, SizeInPixels};

use crate::os_input_output::AsyncReader;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use interprocess::local_socket::Stream as LocalSocketStream;
use zellij_utils::{
    data::{ModeInfo, Palette, Style},
    input::command::{RunCommand, TerminalAction},
    ipc::{ClientToServerMsg, ServerToClientMsg},
};

#[derive(Clone)]
struct FakeInputOutput {}

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
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _default_editor: Option<PathBuf>,
    ) -> Result<(u32, Box<dyn AsyncReader>, Option<u32>)> {
        unimplemented!()
    }
    fn write_to_tty_stdin(&self, _id: u32, _buf: &[u8]) -> Result<usize> {
        unimplemented!()
    }
    fn tcdrain(&self, _id: u32) -> Result<()> {
        unimplemented!()
    }
    fn kill(&self, _pid: u32) -> Result<()> {
        unimplemented!()
    }
    fn force_kill(&self, _pid: u32) -> Result<()> {
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
    fn new_client_with_reply(
        &mut self,
        _client_id: ClientId,
        _stream: LocalSocketStream,
        _reply_stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>> {
        unimplemented!()
    }
    fn remove_client(&mut self, _client_id: ClientId) -> Result<()> {
        unimplemented!()
    }
    fn load_palette(&self) -> Palette {
        unimplemented!()
    }
    fn get_cwd(&self, _pid: u32) -> Option<PathBuf> {
        unimplemented!()
    }

    fn write_to_file(&mut self, _buf: String, _name: Option<String>) -> Result<()> {
        unimplemented!()
    }
    fn re_run_command_in_terminal(
        &self,
        _terminal_id: u32,
        _run_command: RunCommand,
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    ) -> Result<(Box<dyn AsyncReader>, Option<u32>)> {
        unimplemented!()
    }
    fn clear_terminal_id(&self, _terminal_id: u32) -> Result<()> {
        unimplemented!()
    }
    fn send_sigint(&self, _pid: u32) -> Result<()> {
        unimplemented!()
    }
}

fn tab_resize_increase(tab: &mut Tab, id: ClientId) {
    tab.resize(id, ResizeStrategy::new(Resize::Increase, None))
        .unwrap();
}

fn tab_resize_left(tab: &mut Tab, id: ClientId) {
    tab.resize(
        id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Left)),
    )
    .unwrap();
}

fn tab_resize_down(tab: &mut Tab, id: ClientId) {
    tab.resize(
        id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Down)),
    )
    .unwrap();
}

fn tab_resize_up(tab: &mut Tab, id: ClientId) {
    tab.resize(
        id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Up)),
    )
    .unwrap();
}

fn tab_resize_right(tab: &mut Tab, id: ClientId) {
    tab.resize(
        id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Right)),
    )
    .unwrap();
}

fn create_new_tab(size: Size, stacked_resize: bool) -> Tab {
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput {});
    let senders = ThreadSenders::default().silently_fail_on_send();
    let max_panes = None;
    let mode_info = ModeInfo::default();
    let style = Style::default();
    let draw_pane_frames = true;
    let auto_layout = true;
    let client_id = 1;
    let session_is_mirrored = true;
    let mut connected_clients = HashMap::new();
    let character_cell_info = Rc::new(RefCell::new(None));
    let stacked_resize = Rc::new(RefCell::new(stacked_resize));
    connected_clients.insert(client_id, false);
    let connected_clients = Rc::new(RefCell::new(connected_clients));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let copy_options = CopyOptions::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let current_pane_group = Rc::new(RefCell::new(PaneGroups::new(ThreadSenders::default())));
    let currently_marking_pane_group = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let advanced_mouse_actions = true;
    let web_sharing = WebSharing::Off;
    let web_server_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let web_server_port = 8080;
    let mut tab = Tab::new(
        index,
        position,
        name,
        size,
        character_cell_info,
        stacked_resize,
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
        Some(client_id),
        copy_options,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        (vec![], vec![]), // swap layouts
        PathBuf::from("my_default_shell"),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
        false,
        web_sharing,
        current_pane_group,
        currently_marking_pane_group,
        advanced_mouse_actions,
        true,  // mouse_hover_effects
        false, // focus_follows_mouse
        false, // mouse_click_through
        web_server_ip,
        web_server_port,
    );
    tab.apply_layout(
        TiledPaneLayout::default(),
        vec![],
        vec![(1, None)],
        vec![],
        HashMap::new(),
        client_id,
        None,
    )
    .unwrap();
    tab
}

fn create_new_tab_with_layout(size: Size, layout: TiledPaneLayout) -> Tab {
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput {});
    let senders = ThreadSenders::default().silently_fail_on_send();
    let max_panes = None;
    let mode_info = ModeInfo::default();
    let style = Style::default();
    let draw_pane_frames = true;
    let auto_layout = true;
    let client_id = 1;
    let session_is_mirrored = true;
    let mut connected_clients = HashMap::new();
    let character_cell_info = Rc::new(RefCell::new(None));
    let stacked_resize = Rc::new(RefCell::new(true));
    connected_clients.insert(client_id, false);
    let connected_clients = Rc::new(RefCell::new(connected_clients));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let copy_options = CopyOptions::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let current_pane_group = Rc::new(RefCell::new(PaneGroups::new(ThreadSenders::default())));
    let currently_marking_pane_group = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let advanced_mouse_actions = true;
    let web_sharing = WebSharing::Off;
    let web_server_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let web_server_port = 8080;
    let mut tab = Tab::new(
        index,
        position,
        name,
        size,
        character_cell_info,
        stacked_resize,
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
        Some(client_id),
        copy_options,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        (vec![], vec![]), // swap layouts
        PathBuf::from("my_default_shell"),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
        false,
        web_sharing,
        current_pane_group,
        currently_marking_pane_group,
        advanced_mouse_actions,
        true,  // mouse_hover_effects
        false, // focus_follows_mouse
        false, // mouse_click_through
        web_server_ip,
        web_server_port,
    );
    let mut new_terminal_ids = vec![];
    for i in 0..layout.extract_run_instructions().len() {
        new_terminal_ids.push((i as u32, None));
    }
    tab.apply_layout(
        layout,
        vec![],
        new_terminal_ids,
        vec![],
        HashMap::new(),
        client_id,
        None,
    )
    .unwrap();
    tab
}

fn create_new_tab_with_cell_size(
    size: Size,
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
) -> Tab {
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput {});
    let senders = ThreadSenders::default().silently_fail_on_send();
    let max_panes = None;
    let mode_info = ModeInfo::default();
    let style = Style::default();
    let draw_pane_frames = true;
    let auto_layout = true;
    let client_id = 1;
    let session_is_mirrored = true;
    let mut connected_clients = HashMap::new();
    connected_clients.insert(client_id, false);
    let connected_clients = Rc::new(RefCell::new(connected_clients));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let copy_options = CopyOptions::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let stacked_resize = Rc::new(RefCell::new(true));
    let current_pane_group = Rc::new(RefCell::new(PaneGroups::new(ThreadSenders::default())));
    let currently_marking_pane_group = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let advanced_mouse_actions = true;
    let web_sharing = WebSharing::Off;
    let web_server_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let web_server_port = 8080;
    let mut tab = Tab::new(
        index,
        position,
        name,
        size,
        character_cell_size,
        stacked_resize,
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
        Some(client_id),
        copy_options,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        (vec![], vec![]), // swap layouts
        PathBuf::from("my_default_shell"),
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
        false,
        web_sharing,
        current_pane_group,
        currently_marking_pane_group,
        advanced_mouse_actions,
        true,  // mouse_hover_effects
        false, // focus_follows_mouse
        false, // mouse_click_through
        web_server_ip,
        web_server_port,
    );
    tab.apply_layout(
        TiledPaneLayout::default(),
        vec![],
        vec![(1, None)],
        vec![],
        HashMap::new(),
        client_id,
        None,
    )
    .unwrap();
    tab
}

#[test]
fn write_to_suppressed_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();

    // Suppress pane 2 and remove it from active panes
    tab.replace_active_pane_with_editor_pane(PaneId::Terminal(2), 1)
        .unwrap();
    tab.tiled_panes.remove_pane(PaneId::Terminal(2));

    // Make sure it's suppressed now
    tab.suppressed_panes.get(&PaneId::Terminal(2)).unwrap();
    // Write content to it
    tab.write_to_pane_id(
        &None,
        vec![34, 127, 31, 82, 17, 182],
        false,
        PaneId::Terminal(2),
        None,
        None,
    )
    .unwrap();
}

#[test]
fn split_panes_vertically() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);
    tab.vertical_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2, "The tab has two panes");
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "first pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "first pane row count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "second pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "second pane row count"
    );
}

#[test]
fn split_panes_horizontally() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2, "The tab has two panes");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "first pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "first pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "second pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "second pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "second pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "second pane row count"
    );
}

#[test]
fn split_largest_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false; // note - this is not the default
    let mut tab = create_new_tab(size, stacked_resize);
    for i in 2..5 {
        let new_pane_id = PaneId::Terminal(i);
        tab.new_pane(
            new_pane_id,
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    }
    assert_eq!(tab.tiled_panes.panes.len(), 4, "The tab has four panes");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "first pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "first pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "second pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "second pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "third pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "third pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "third pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "third pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "fourth pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "fourth pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "fourth pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "fourth pane row count"
    );
}

#[test]
pub fn cannot_split_panes_vertically_when_active_pane_is_too_small() {
    let size = Size { cols: 8, rows: 20 };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    assert_eq!(
        tab.tiled_panes.panes.len(),
        1,
        "Tab still has only one pane"
    );
}

#[test]
pub fn cannot_split_panes_horizontally_when_active_pane_is_too_small() {
    let size = Size { cols: 121, rows: 4 };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    assert_eq!(
        tab.tiled_panes.panes.len(),
        1,
        "Tab still has only one pane"
    );
}

#[test]
pub fn cannot_split_largest_pane_when_there_is_no_room() {
    let size = Size { cols: 8, rows: 4 };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::default(),
        Some(1),
        None,
    )
    .unwrap();
    assert_eq!(
        tab.tiled_panes.panes.len(),
        1,
        "Tab still has only one pane"
    );
}

#[test]
pub fn cannot_split_panes_vertically_when_active_pane_has_fixed_columns() {
    let size = Size { cols: 50, rows: 20 };
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    let mut fixed_child = TiledPaneLayout::default();
    fixed_child.split_size = Some(SplitSize::Fixed(30));
    initial_layout.children = vec![fixed_child, TiledPaneLayout::default()];
    let mut tab = create_new_tab_with_layout(size, initial_layout);
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2, "Tab still has two panes");
}

#[test]
pub fn cannot_split_panes_horizontally_when_active_pane_has_fixed_rows() {
    let size = Size { cols: 50, rows: 20 };
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Horizontal;
    let mut fixed_child = TiledPaneLayout::default();
    fixed_child.split_size = Some(SplitSize::Fixed(12));
    initial_layout.children = vec![fixed_child, TiledPaneLayout::default()];
    let mut tab = create_new_tab_with_layout(size, initial_layout);
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2, "Tab still has two panes");
}

#[test]
pub fn toggle_focused_pane_fullscreen() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false; // note - this is not the default
    let mut tab = create_new_tab(size, stacked_resize);
    for i in 2..5 {
        let new_pane_id = PaneId::Terminal(i);
        tab.new_pane(
            new_pane_id,
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    }
    tab.toggle_active_pane_fullscreen(1);
    assert_eq!(
        tab.tiled_panes.panes.get(&PaneId::Terminal(4)).unwrap().x(),
        0,
        "Pane x is on screen edge"
    );
    assert_eq!(
        tab.tiled_panes.panes.get(&PaneId::Terminal(4)).unwrap().y(),
        0,
        "Pane y is on screen edge"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .cols(),
        121,
        "Pane cols match fullscreen cols"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .rows(),
        20,
        "Pane rows match fullscreen rows"
    );
    tab.toggle_active_pane_fullscreen(1);
    assert_eq!(
        tab.tiled_panes.panes.get(&PaneId::Terminal(4)).unwrap().x(),
        61,
        "Pane x is on screen edge"
    );
    assert_eq!(
        tab.tiled_panes.panes.get(&PaneId::Terminal(4)).unwrap().y(),
        10,
        "Pane y is on screen edge"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .cols(),
        60,
        "Pane cols match fullscreen cols"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .rows(),
        10,
        "Pane rows match fullscreen rows"
    );
    // we don't test if all other panes are hidden because this logic is done in the render
    // function and we already test that in the e2e tests
}

#[test]
pub fn toggle_focused_pane_fullscreen_with_stacked_resizes() {
    // note - this is the default
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    for i in 2..5 {
        let new_pane_id = PaneId::Terminal(i);
        tab.new_pane(
            new_pane_id,
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    }
    tab.toggle_active_pane_fullscreen(1);
    assert_eq!(
        tab.tiled_panes.panes.get(&PaneId::Terminal(4)).unwrap().x(),
        0,
        "Pane x is on screen edge"
    );
    assert_eq!(
        tab.tiled_panes.panes.get(&PaneId::Terminal(4)).unwrap().y(),
        0,
        "Pane y is on screen edge"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .cols(),
        121,
        "Pane cols match fullscreen cols"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .rows(),
        20,
        "Pane rows match fullscreen rows"
    );
    tab.toggle_active_pane_fullscreen(1);
    assert_eq!(
        tab.tiled_panes.panes.get(&PaneId::Terminal(4)).unwrap().x(),
        61,
        "Pane x is back to its original position"
    );
    assert_eq!(
        tab.tiled_panes.panes.get(&PaneId::Terminal(4)).unwrap().y(),
        2,
        "Pane y is back to its original position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .cols(),
        60,
        "Pane cols are back at their original position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .rows(),
        18,
        "Pane rows are back at their original position"
    );
    // we don't test if all other panes are hidden because this logic is done in the render
    // function and we already test that in the e2e tests
}

#[test]
pub fn resize_whole_tab_while_fullscreen_preserves_fullscreen() {
    // A host-terminal resize (e.g. a font size change) that arrives while a
    // pane is fullscreen must keep the active pane fullscreened, sized to the
    // new display dimensions.
    let initial_size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
    let mut tab = create_new_tab(initial_size, stacked_resize);
    for i in 2..5 {
        let new_pane_id = PaneId::Terminal(i);
        tab.new_pane(
            new_pane_id,
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    }
    tab.toggle_active_pane_fullscreen(1);
    assert!(
        tab.is_fullscreen_active(),
        "Tab is fullscreen before the resize"
    );

    let new_size = Size { cols: 80, rows: 30 };
    tab.resize_whole_tab(new_size).unwrap();

    assert!(
        tab.is_fullscreen_active(),
        "Fullscreen is preserved across a host-terminal resize"
    );
    let active_pane = tab
        .tiled_panes
        .panes
        .get(&PaneId::Terminal(4))
        .expect("Active fullscreen pane is still present");
    assert_eq!(
        active_pane.cols(),
        new_size.cols,
        "Fullscreen pane cols match the new display cols"
    );
    assert_eq!(
        active_pane.rows(),
        new_size.rows,
        "Fullscreen pane rows match the new display rows"
    );
    assert_eq!(active_pane.x(), 0, "Fullscreen pane x is at viewport edge");
    assert_eq!(active_pane.y(), 0, "Fullscreen pane y is at viewport edge");
}

#[test]
pub fn resize_while_fullscreen_updates_hidden_pane_geometry() {
    // When a host-terminal resize arrives while a pane is fullscreen, every
    // hidden pane's geometry must be updated to match the new display area.
    // Otherwise their `inner` cell counts stay sized for the old display and
    // toggling fullscreen off hands the cassowary solver coordinates that
    // fall outside the viewport, producing layout-solve failures and a
    // corrupt render.
    //
    // The assertion here is the direct invariant: after the resize, every
    // pane that is currently hidden behind the fullscreen pane fits inside
    // the new display area.
    let initial_size = Size {
        cols: 200,
        rows: 60,
    };
    let new_size = Size { cols: 60, rows: 18 };
    let stacked_resize = false;
    let mut tab = create_new_tab(initial_size, stacked_resize);
    for i in 2..6 {
        tab.new_pane(
            PaneId::Terminal(i),
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    }

    let active_pane_id = tab
        .get_active_pane_id(1)
        .expect("an active pane exists before fullscreen");
    tab.toggle_active_pane_fullscreen(1);
    assert!(tab.is_fullscreen_active(), "Fullscreen is active");

    tab.resize_whole_tab(new_size).unwrap();

    // Collect the panes hidden by the fullscreen state and verify each one
    // already fits the new display area; if any extends beyond it, exiting
    // fullscreen would hand the cassowary solver an unsatisfiable layout.
    let hidden_pane_ids: Vec<PaneId> = tab
        .tiled_panes
        .panes
        .keys()
        .copied()
        .filter(|id| *id != active_pane_id && tab.tiled_panes.panes_to_hide_contains(*id))
        .collect();
    assert!(
        !hidden_pane_ids.is_empty(),
        "the test setup actually produced hidden panes"
    );
    for pane_id in hidden_pane_ids {
        let pane = tab.tiled_panes.panes.get(&pane_id).unwrap();
        let geom = pane.position_and_size();
        assert!(
            geom.x + geom.cols.as_usize() <= new_size.cols,
            "hidden pane {pane_id:?} fits horizontally after resize: \
             x={}, cols={}, display_cols={}",
            geom.x,
            geom.cols.as_usize(),
            new_size.cols,
        );
        assert!(
            geom.y + geom.rows.as_usize() <= new_size.rows,
            "hidden pane {pane_id:?} fits vertically after resize: \
             y={}, rows={}, display_rows={}",
            geom.y,
            geom.rows.as_usize(),
            new_size.rows,
        );
    }
}

#[test]
pub fn closing_fullscreen_scrollback_editor_restores_consistent_layout() {
    // Replacing a pane (e.g. opening or closing a scrollback editor) swaps
    // the pane id occupying its tiled slot. If the replaced pane was the
    // fullscreen pane, the fullscreen bookkeeping must follow the swap so
    // that toggling fullscreen off later resets the geom_override on the
    // pane that actually carries it. Otherwise the restored pane keeps the
    // 100% override, the previously-hidden panes come back into a layout
    // that overlaps it, and the screen renders incorrectly.
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
    let client_id = 1;
    let mut tab = create_new_tab(size, stacked_resize);
    for i in 2..5 {
        tab.new_pane(
            PaneId::Terminal(i),
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(client_id),
            None,
        )
        .unwrap();
    }
    let active_pane_id = tab
        .get_active_pane_id(client_id)
        .expect("active pane exists before opening editor");

    let editor_pane_id = PaneId::Terminal(99);
    tab.replace_active_pane_with_editor_pane(editor_pane_id, client_id)
        .unwrap();
    assert_eq!(
        tab.get_active_pane_id(client_id),
        Some(editor_pane_id),
        "editor pane is now active",
    );

    tab.toggle_active_pane_fullscreen(client_id);
    assert!(
        tab.is_fullscreen_active(),
        "fullscreen is active on the editor pane",
    );
    assert_eq!(
        tab.tiled_panes.fullscreen_pane_id(),
        Some(editor_pane_id),
        "fullscreen tracks the editor pane id",
    );

    // Close the editor: this restores the originally-suppressed pane in the
    // editor's slot. Fullscreen bookkeeping must retarget to the restored
    // pane id so subsequent fullscreen-off cleanup hits the right pane.
    tab.close_pane(editor_pane_id, false, None);
    assert!(
        tab.is_fullscreen_active(),
        "fullscreen is preserved after the editor is closed",
    );
    assert_eq!(
        tab.tiled_panes.fullscreen_pane_id(),
        Some(active_pane_id),
        "fullscreen now tracks the restored suppressed pane",
    );

    tab.toggle_active_pane_fullscreen(client_id);
    assert!(
        !tab.is_fullscreen_active(),
        "fullscreen is cleared after the second toggle",
    );
    assert_eq!(
        tab.tiled_panes.panes_to_hide_count(),
        0,
        "no panes remain hidden after exiting fullscreen",
    );
    let restored_pane = tab
        .tiled_panes
        .panes
        .get(&active_pane_id)
        .expect("restored pane is present");
    assert!(
        restored_pane.geom_override().is_none(),
        "restored pane no longer carries the fullscreen geom_override",
    );
    for pane in tab.tiled_panes.panes.values() {
        let geom = pane.position_and_size();
        assert!(
            geom.x + geom.cols.as_usize() <= size.cols,
            "pane fits horizontally after exiting fullscreen: \
             x={}, cols={}, display_cols={}",
            geom.x,
            geom.cols.as_usize(),
            size.cols,
        );
        assert!(
            geom.y + geom.rows.as_usize() <= size.rows,
            "pane fits vertically after exiting fullscreen: \
             y={}, rows={}, display_rows={}",
            geom.y,
            geom.rows.as_usize(),
            size.rows,
        );
    }
}

#[test]
pub fn opening_scrollback_editor_on_fullscreen_pane_retargets_fullscreen() {
    // Reverse-direction variant: fullscreen the pane *first*, then open the
    // scrollback editor on it. The editor takes the fullscreen pane's slot
    // and inherits its 100% geom_override, so the fullscreen bookkeeping
    // must follow the swap onto the editor's pane id. Otherwise toggling
    // fullscreen off later cannot reset the override on the editor.
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
    let client_id = 1;
    let mut tab = create_new_tab(size, stacked_resize);
    for i in 2..5 {
        tab.new_pane(
            PaneId::Terminal(i),
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(client_id),
            None,
        )
        .unwrap();
    }
    let active_pane_id = tab
        .get_active_pane_id(client_id)
        .expect("active pane exists");

    tab.toggle_active_pane_fullscreen(client_id);
    assert_eq!(
        tab.tiled_panes.fullscreen_pane_id(),
        Some(active_pane_id),
        "fullscreen tracks the original pane",
    );

    let editor_pane_id = PaneId::Terminal(99);
    tab.replace_active_pane_with_editor_pane(editor_pane_id, client_id)
        .unwrap();
    assert!(
        tab.is_fullscreen_active(),
        "fullscreen state survives the editor swap",
    );
    assert_eq!(
        tab.tiled_panes.fullscreen_pane_id(),
        Some(editor_pane_id),
        "fullscreen now tracks the editor pane id, not the suppressed one",
    );

    tab.toggle_active_pane_fullscreen(client_id);
    assert!(
        !tab.is_fullscreen_active(),
        "fullscreen is cleared after the second toggle",
    );
    let editor_pane = tab
        .tiled_panes
        .panes
        .get(&editor_pane_id)
        .expect("editor pane is present in tiled panes");
    assert!(
        editor_pane.geom_override().is_none(),
        "editor pane no longer carries the fullscreen geom_override",
    );
    assert_eq!(
        tab.tiled_panes.panes_to_hide_count(),
        0,
        "no panes remain hidden after exiting fullscreen",
    );
    for pane in tab.tiled_panes.panes.values() {
        let geom = pane.position_and_size();
        assert!(
            geom.x + geom.cols.as_usize() <= size.cols
                && geom.y + geom.rows.as_usize() <= size.rows,
            "pane fits inside the display area: \
             x={}, y={}, cols={}, rows={}, display={}x{}",
            geom.x,
            geom.y,
            geom.cols.as_usize(),
            geom.rows.as_usize(),
            size.cols,
            size.rows,
        );
    }
}

#[test]
fn switch_to_next_pane_fullscreen() {
    let size = Size {
        cols: 121,
        rows: 20,
    };

    let stacked_resize = true;
    let mut active_tab = create_new_tab(size, stacked_resize);

    active_tab
        .new_pane(
            PaneId::Terminal(1),
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    active_tab
        .new_pane(
            PaneId::Terminal(2),
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    active_tab
        .new_pane(
            PaneId::Terminal(3),
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    active_tab
        .new_pane(
            PaneId::Terminal(4),
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    active_tab.toggle_active_pane_fullscreen(1);

    // order is now 1 ->2 -> 3 -> 4 due to how new panes are inserted

    active_tab.switch_next_pane_fullscreen(1);
    active_tab.switch_next_pane_fullscreen(1);
    active_tab.switch_next_pane_fullscreen(1);
    active_tab.switch_next_pane_fullscreen(1);

    // position should now be back in terminal 4.

    assert_eq!(
        active_tab.get_active_pane_id(1).unwrap(),
        PaneId::Terminal(4),
        "Active pane did not switch in fullscreen mode"
    );
}

#[test]
fn switch_to_prev_pane_fullscreen() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut active_tab = create_new_tab(size, stacked_resize);

    //testing four consecutive switches in fullscreen mode

    active_tab
        .new_pane(
            PaneId::Terminal(1),
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    active_tab
        .new_pane(
            PaneId::Terminal(2),
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    active_tab
        .new_pane(
            PaneId::Terminal(3),
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    active_tab
        .new_pane(
            PaneId::Terminal(4),
            None,
            None,
            false,
            true,
            NewPanePlacement::default(),
            Some(1),
            None,
        )
        .unwrap();
    active_tab.toggle_active_pane_fullscreen(1);
    // order is now 1 2 3 4

    active_tab.switch_prev_pane_fullscreen(1);
    active_tab.switch_prev_pane_fullscreen(1);
    active_tab.switch_prev_pane_fullscreen(1);
    active_tab.switch_prev_pane_fullscreen(1);

    // the position should now be in Terminal 4.

    assert_eq!(
        active_tab.get_active_pane_id(1).unwrap(),
        PaneId::Terminal(4),
        "Active pane did not switch in fullscreen mode"
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.close_focused_pane(1, None).unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 1, "One pane left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.close_focused_pane(1, None).unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 1, "One pane left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);
    tab.vertical_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.close_focused_pane(1, None).unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 1, "One pane left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);
    tab.vertical_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.close_focused_pane(1, None).unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 1, "One pane left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.horizontal_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.close_focused_pane(1, None).unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2, "Two panes left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.horizontal_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.close_focused_pane(1, None).unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2, "Two panes left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.vertical_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.close_focused_pane(1, None).unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2, "Two panes left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.vertical_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.close_focused_pane(1, None).unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2, "Two panes left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let new_pane_id_6 = PaneId::Terminal(7);

    tab.vertical_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(new_pane_id_4, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(new_pane_id_5, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_down(&mut tab, 1);
    tab.vertical_split(new_pane_id_6, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.close_focused_pane(1, None).unwrap();

    assert_eq!(tab.tiled_panes.panes.len(), 6, "Six panes left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        15,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "second remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "third remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "third remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "third remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "third remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "fourth remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "fourth remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "fourth remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "fourth remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "sixths remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "sixths remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "sixths remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "sixths remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        76,
        "seventh remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "seventh remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        15,
        "seventh remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let new_pane_id_6 = PaneId::Terminal(7);

    tab.vertical_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(new_pane_id_4, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(new_pane_id_5, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_up(&mut tab, 1);
    tab.vertical_split(new_pane_id_6, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.close_focused_pane(1, None).unwrap();

    assert_eq!(tab.tiled_panes.panes.len(), 6, "Six panes left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "third remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "third remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "third remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "third remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "fourth remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "fourth remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "fourth remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "fourth remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        15,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "second remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "sixths remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "sixths remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "sixths remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "sixths remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        76,
        "seventh remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "seventh remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        15,
        "seventh remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let new_pane_id_6 = PaneId::Terminal(7);

    tab.horizontal_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(new_pane_id_4, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(new_pane_id_5, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_right(&mut tab, 1);
    tab_resize_up(&mut tab, 1);
    tab_resize_up(&mut tab, 1);
    tab.horizontal_split(new_pane_id_6, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.close_focused_pane(1, None).unwrap();

    assert_eq!(tab.tiled_panes.panes.len(), 6, "Six panes left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        12,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "third remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        12,
        "third remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "third remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "third remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "fourth remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        22,
        "fourth remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "fourth remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "fourth remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        12,
        "second remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "sixths remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        22,
        "sixths remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "sixths remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "sixths remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "seventh remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        17,
        "seventh remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "seventh remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);
    let new_pane_id_4 = PaneId::Terminal(5);
    let new_pane_id_5 = PaneId::Terminal(6);
    let new_pane_id_6 = PaneId::Terminal(7);

    tab.horizontal_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(new_pane_id_4, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(new_pane_id_5, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_left(&mut tab, 1);
    tab_resize_up(&mut tab, 1);
    tab_resize_up(&mut tab, 1);
    tab.horizontal_split(new_pane_id_6, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.close_focused_pane(1, None).unwrap();

    assert_eq!(tab.tiled_panes.panes.len(), 6, "Six panes left in tab");

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "first remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "first remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        11,
        "first remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "fourth remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        22,
        "fourth remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "fourth remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "fourth remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "second remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "second remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        11,
        "second remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "third remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        11,
        "third remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "third remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        6,
        "third remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "sixths remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        22,
        "sixths remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "sixths remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "sixths remaining pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "seventh remaining pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        17,
        "seventh remaining pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "seventh remaining pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "seventh remaining pane row count"
    );
}

#[test]
pub fn move_focus_down() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);

    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_down(1).unwrap();

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        10,
        "Active pane is the bottom one"
    );
}

#[test]
pub fn move_focus_down_to_the_most_recently_used_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.horizontal_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_down(1).unwrap();

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        10,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        91,
        "Active pane x position"
    );
}

#[test]
pub fn move_focus_up() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);

    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        0,
        "Active pane is the top one"
    );
}

#[test]
pub fn move_focus_up_to_the_most_recently_used_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.horizontal_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.move_focus_up(1).unwrap();

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        0,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        91,
        "Active pane x position"
    );
}

#[test]
pub fn move_focus_left() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);

    tab.vertical_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();

    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        0,
        "Active pane is the left one"
    );
}

#[test]
pub fn move_focus_left_to_the_most_recently_used_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.vertical_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.move_focus_left(1).unwrap();

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        15,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        0,
        "Active pane x position"
    );
}

#[test]
pub fn move_focus_right() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);

    tab.vertical_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_right(1).unwrap();

    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        61,
        "Active pane is the right one"
    );
}

#[test]
pub fn move_focus_right_to_the_most_recently_used_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.vertical_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_right(1).unwrap();

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        15,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        61,
        "Active pane x position"
    );
}

#[test]
pub fn move_active_pane_down() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);

    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_active_pane_down(1);

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        10,
        "Active pane is the bottom one"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().pid(),
        PaneId::Terminal(1),
        "Active pane is the bottom one"
    );
}

#[test]
pub fn move_active_pane_down_to_the_most_recently_used_position() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.horizontal_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_active_pane_down(1);

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        10,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        91,
        "Active pane x position"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().pid(),
        PaneId::Terminal(1),
        "Active pane PaneId"
    );
}

#[test]
pub fn move_active_pane_up() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);

    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.move_active_pane_up(1);

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        0,
        "Active pane is the top one"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().pid(),
        PaneId::Terminal(2),
        "Active pane is the top one"
    );
}

#[test]
pub fn move_active_pane_up_to_the_most_recently_used_position() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.horizontal_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.move_active_pane_up(1);

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        0,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        91,
        "Active pane x position"
    );

    assert_eq!(
        tab.get_active_pane(1).unwrap().pid(),
        PaneId::Terminal(2),
        "Active pane PaneId"
    );
}

#[test]
pub fn move_active_pane_left() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);

    tab.vertical_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.move_active_pane_left(1);

    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        0,
        "Active pane is the left one"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().pid(),
        PaneId::Terminal(2),
        "Active pane is the left one"
    );
}

#[test]
pub fn move_active_pane_left_to_the_most_recently_used_position() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.vertical_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.move_active_pane_left(1);

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        15,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        0,
        "Active pane x position"
    );

    assert_eq!(
        tab.get_active_pane(1).unwrap().pid(),
        PaneId::Terminal(2),
        "Active pane PaneId"
    );
}

#[test]
pub fn move_active_pane_right() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);

    tab.vertical_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_active_pane_right(1);

    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        61,
        "Active pane is the right one"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().pid(),
        PaneId::Terminal(1),
        "Active pane is the right one"
    );
}

#[test]
pub fn move_active_pane_right_to_the_most_recently_used_position() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    let new_pane_id_3 = PaneId::Terminal(4);

    tab.vertical_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_active_pane_right(1);

    assert_eq!(
        tab.get_active_pane(1).unwrap().y(),
        15,
        "Active pane y position"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().x(),
        61,
        "Active pane x position"
    );
    assert_eq!(
        tab.get_active_pane(1).unwrap().pid(),
        PaneId::Terminal(1),
        "Active pane Paneid"
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .y,
        11,
        "focused pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "focused pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        9,
        "focused pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane above x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane above y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane above column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        11,
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane below x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .y,
        11,
        "pane below y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane below column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        9,
        "pane below row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "focused pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "focused pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        11,
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
    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let first_pane_id = PaneId::Terminal(1);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.horizontal_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .y,
        15,
        "focused pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "focused pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        9,
        "focused pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane below x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .y,
        24,
        "pane below y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane below column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        6,
        "pane below row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane above x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane above y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane above column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let first_pane_id = PaneId::Terminal(1);
    let new_pane_id_1 = PaneId::Terminal(2);
    let new_pane_id_2 = PaneId::Terminal(3);
    tab.horizontal_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .y,
        16,
        "focused pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "focused pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_1)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "focused pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "first pane above x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "first pane above y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "first pane above column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&new_pane_id_2)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "first pane above row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "second pane above x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "second pane above y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "second pane above column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&first_pane_id)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
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
    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let pane_above_and_left = PaneId::Terminal(1);
    let pane_to_the_left = PaneId::Terminal(2);
    let focused_pane = PaneId::Terminal(3);
    let pane_above = PaneId::Terminal(4);
    tab.horizontal_split(pane_to_the_left, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(focused_pane, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(pane_above, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "focused pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .y,
        16,
        "focused pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "focused pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "focused pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above_and_left)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane above and to the left x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above_and_left)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane above and to the left y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above_and_left)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane above and to the left column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above_and_left)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane above and to the left row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane above x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane above y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane above column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane above row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane to the left x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane to the left y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane to the left column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let pane_to_the_left = PaneId::Terminal(1);
    let pane_below_and_left = PaneId::Terminal(2);
    let pane_below = PaneId::Terminal(3);
    let focused_pane = PaneId::Terminal(4);
    tab.horizontal_split(pane_below_and_left, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(pane_below, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(focused_pane, None, 1, None, None, None)
        .unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "focused pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "focused pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "focused pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "focused pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane above and to the left x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane above and to the left y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane above and to the left column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_left)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane above and to the left row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane above x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below)
            .unwrap()
            .position_and_size()
            .y,
        16,
        "pane above y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane above column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane above row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below_and_left)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane to the left x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below_and_left)
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane to the left y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below_and_left)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane to the left column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below_and_left)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let pane_above = PaneId::Terminal(1);
    let focused_pane = PaneId::Terminal(2);
    let pane_to_the_right = PaneId::Terminal(3);
    let pane_above_and_right = PaneId::Terminal(4);
    tab.horizontal_split(focused_pane, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(pane_to_the_right, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(pane_above_and_right, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .y,
        16,
        "focused pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "focused pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "focused pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane above x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane above y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane above column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane above row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane to the right x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane to the right y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane to the right column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane to the right row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above_and_right)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane above and to the right x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above_and_right)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane above and to the right y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above_and_right)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane above and to the right column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_above_and_right)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let focused_pane = PaneId::Terminal(1);
    let pane_below = PaneId::Terminal(2);
    let pane_below_and_right = PaneId::Terminal(3);
    let pane_to_the_right = PaneId::Terminal(4);
    tab.horizontal_split(pane_below, None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(pane_below_and_right, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(pane_to_the_right, None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "focused pane x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "focused pane y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "focused pane column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&focused_pane)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "focused pane row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below)
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane below x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below)
            .unwrap()
            .position_and_size()
            .y,
        16,
        "pane below y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane below column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane below row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below_and_right)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane below and to the right x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below_and_right)
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane below and to the right y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below_and_right)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane below and to the right column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_below_and_right)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane below and to the right row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane to the right x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane to the right y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane to the right column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&pane_to_the_right)
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        16,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        16,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 122,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.vertical_split(PaneId::Terminal(7), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(8), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        60,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        60,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        16,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        15,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 6 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        75,
        "pane 7 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        16,
        "pane 7 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        8,
        "pane 7 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 7 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        83,
        "pane 8 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        16,
        "pane 8 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        8,
        "pane 8 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
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

    let size = Size {
        cols: 122,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(7), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(8), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        60,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        15,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        75,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        8,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        83,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        8,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 6 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        60,
        "pane 7 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        16,
        "pane 7 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 7 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 7 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 8 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 8 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 8 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 10,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 1 height stayed the same"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 2 height stayed the same"
    );
}

#[test]
pub fn cannot_resize_down_when_pane_has_fixed_rows() {
    // ┌───────────┐                  ┌───────────┐
    // │███████████│                  │███████████│
    // ├───────────┤ ==resize=down==> ├───────────┤
    // │           │                  │           │
    // └───────────┘                  └───────────┘
    // █ == focused pane

    let size = Size {
        cols: 121,
        rows: 20,
    };

    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Horizontal;
    let mut fixed_child = TiledPaneLayout::default();
    fixed_child.split_size = Some(SplitSize::Fixed(10));
    initial_layout.children = vec![fixed_child, TiledPaneLayout::default()];
    let mut tab = create_new_tab_with_layout(size, initial_layout);
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(0))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 1 height stayed the same"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 2 height stayed the same"
    );
}

#[test]
pub fn cannot_resize_down_when_pane_below_has_fixed_rows() {
    // ┌───────────┐                  ┌───────────┐
    // │███████████│                  │███████████│
    // ├───────────┤ ==resize=down==> ├───────────┤
    // │           │                  │           │
    // └───────────┘                  └───────────┘
    // █ == focused pane

    let size = Size {
        cols: 121,
        rows: 20,
    };

    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Horizontal;
    let mut fixed_child = TiledPaneLayout::default();
    fixed_child.split_size = Some(SplitSize::Fixed(10));
    initial_layout.children = vec![TiledPaneLayout::default(), fixed_child];
    let mut tab = create_new_tab_with_layout(size, initial_layout);
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(0))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 1 height stayed the same"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 2 height stayed the same"
    );
}

#[test]
pub fn cannot_resize_up_when_pane_below_has_fixed_rows() {
    // ┌───────────┐                  ┌───────────┐
    // │███████████│                  │███████████│
    // ├───────────┤ ==resize=down==> ├───────────┤
    // │           │                  │           │
    // └───────────┘                  └───────────┘
    // █ == focused pane

    let size = Size {
        cols: 121,
        rows: 20,
    };

    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Horizontal;
    let mut fixed_child = TiledPaneLayout::default();
    fixed_child.split_size = Some(SplitSize::Fixed(10));
    initial_layout.children = vec![TiledPaneLayout::default(), fixed_child];
    let mut tab = create_new_tab_with_layout(size, initial_layout);
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(0))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 1 height stayed the same"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        36,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        90,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        22,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        22,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
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

    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        22,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        22,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
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

    let size = Size {
        cols: 121,
        rows: 70,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_down(&mut tab, 1);
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(7), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(8), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        35,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        35,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        21,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        56,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        56,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        35,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        35,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        11,
        "pane 6 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 7 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        46,
        "pane 7 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 7 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 7 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 8 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        51,
        "pane 8 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 8 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 70,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_down(&mut tab, 1);
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(7), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(8), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        35,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        35,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        11,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        56,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        56,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        35,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        35,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        21,
        "pane 6 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 7 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        46,
        "pane 7 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 7 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 7 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 8 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        51,
        "pane 8 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 8 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size { cols: 10, rows: 20 };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab_resize_left(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        5,
        "pane 1 columns stayed the same"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        36,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        97,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        24,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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

    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        10,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        10,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 70,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_up(&mut tab, 1);
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(7), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(8), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        31,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        31,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        21,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        52,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        18,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        52,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        18,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        31,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        31,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        11,
        "pane 6 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 7 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        42,
        "pane 7 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 7 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 7 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 8 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        47,
        "pane 8 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 8 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 70,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_up(&mut tab, 1);
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(7), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(8), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        31,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        31,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        11,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        52,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        18,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        52,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        18,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        31,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        67,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        31,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        54,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        21,
        "pane 6 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 7 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        42,
        "pane 7 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 7 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 7 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 8 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        47,
        "pane 8 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 8 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size { cols: 10, rows: 20 };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab_resize_right(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        5,
        "pane 1 columns stayed the same"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        5,
        "pane 2 columns stayed the same"
    );
}

#[test]
pub fn cannot_resize_right_when_pane_has_fixed_columns() {
    // ┌──┬──┐                   ┌──┬──┐
    // │██│  │                   │██│  │
    // │██│  │ ==resize=right==> │██│  │
    // │██│  │                   │██│  │
    // └──┴──┘                   └──┴──┘
    // █ == focused pane

    let size = Size {
        cols: 121,
        rows: 20,
    };

    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    let mut fixed_child = TiledPaneLayout::default();
    fixed_child.split_size = Some(SplitSize::Fixed(60));
    initial_layout.children = vec![fixed_child, TiledPaneLayout::default()];
    let mut tab = create_new_tab_with_layout(size, initial_layout);
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(0))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 1 height stayed the same"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 2 height stayed the same"
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        9,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        9,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        11,
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
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        9,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        9,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        11,
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
    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        13,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        13,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        9,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        22,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        8,
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
    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        121,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
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
    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
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
    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
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
    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 121,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        61,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        30,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
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
    let size = Size {
        cols: 122,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.vertical_split(PaneId::Terminal(7), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(8), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        60,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        60,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        15,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 6 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        75,
        "pane 7 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 7 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        8,
        "pane 7 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 7 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        83,
        "pane 8 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 8 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        8,
        "pane 8 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
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
    let size = Size {
        cols: 122,
        rows: 30,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab.vertical_split(PaneId::Terminal(7), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(8), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_up(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 1 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 1 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 1 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 1 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        60,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 2 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        60,
        "pane 3 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 3 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        15,
        "pane 3 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 3 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 4 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 4 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 4 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(4))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 4 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .x,
        60,
        "pane 5 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .y,
        14,
        "pane 5 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 5 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(5))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        16,
        "pane 5 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .x,
        91,
        "pane 6 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .y,
        15,
        "pane 6 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        31,
        "pane 6 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(6))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        15,
        "pane 6 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .x,
        75,
        "pane 7 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 7 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        8,
        "pane 7 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(7))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
        "pane 7 row count"
    );

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .x,
        83,
        "pane 8 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 8 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        8,
        "pane 8 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(8))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        14,
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

    let size = Size {
        cols: 121,
        rows: 10,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab_resize_down(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(1))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 1 height stayed the same"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        5,
        "pane 2 height stayed the same"
    );
}

#[test]
pub fn nondirectional_resize_increase_with_1_pane() {
    let size = Size {
        cols: 121,
        rows: 10,
    };
    let stacked_resize = false; // note - this is not the default
    let mut tab = create_new_tab(size, stacked_resize);
    tab_resize_increase(&mut tab, 1);

    assert_eq!(
        tab.get_active_pane(1).unwrap().position_and_size().y,
        0,
        "There is only 1 pane so both coordinates should be 0"
    );

    assert_eq!(
        tab.get_active_pane(1).unwrap().position_and_size().x,
        0,
        "There is only 1 pane so both coordinates should be 0"
    );
}

#[test]
pub fn nondirectional_resize_increase_with_1_pane_with_stacked_resizes() {
    let size = Size {
        cols: 121,
        rows: 10,
    };
    let stacked_resize = true; // note - this is not the default
    let mut tab = create_new_tab(size, stacked_resize);
    tab_resize_increase(&mut tab, 1);

    assert_eq!(
        tab.get_active_pane(1).unwrap().position_and_size().y,
        0,
        "There is only 1 pane so both coordinates should be 0"
    );

    assert_eq!(
        tab.get_active_pane(1).unwrap().position_and_size().x,
        0,
        "There is only 1 pane so both coordinates should be 0"
    );
}

#[test]
pub fn nondirectional_resize_increase_with_1_pane_to_left() {
    let size = Size {
        cols: 121,
        rows: 10,
    };
    let stacked_resize = false; // note - this is not the default
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id_1 = PaneId::Terminal(2);
    tab.vertical_split(new_pane_id_1, None, 1, None, None, None)
        .unwrap();
    tab_resize_increase(&mut tab, 1);

    // should behave like `resize_left_with_pane_to_the_left`
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
}

#[test]
pub fn nondirectional_resize_increase_with_2_panes_to_left() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false; // note - this is not the default
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab_resize_increase(&mut tab, 1);

    // should behave like `resize_left_with_multiple_panes_to_the_left`
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        54,
        "pane 2 x position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "pane 2 y position"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "pane 2 column count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "pane 2 row count"
    );
}

#[test]
pub fn nondirectional_resize_increase_with_1_pane_to_right_1_pane_above() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false; // note - this is not the default
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab_resize_increase(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .y,
        9,
        "Pane 3 y coordinate"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .x,
        0,
        "Pane 3 x coordinate"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        11,
        "Pane 3 row count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(3))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        67,
        "Pane 3 col count"
    );
}

#[test]
pub fn nondirectional_resize_increase_with_1_pane_to_right_1_pane_to_left() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false; // note - this is not the default
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_increase(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "Pane 3 y coordinate"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "Pane 3 x coordinate"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "Pane 3 row count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        36,
        "Pane 3 col count"
    );
}

#[test]
pub fn nondirectional_resize_increase_with_pane_above_aligned_right_with_current_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false; // note - this is not the default
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_increase(&mut tab, 1);

    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .y,
        0,
        "Pane 3 y coordinate"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .x,
        61,
        "Pane 3 x coordinate"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .rows
            .as_usize(),
        20,
        "Pane 3 row count"
    );
    assert_eq!(
        tab.tiled_panes
            .panes
            .get(&PaneId::Terminal(2))
            .unwrap()
            .position_and_size()
            .cols
            .as_usize(),
        36,
        "Pane 3 col count"
    );
}

#[test]
pub fn custom_cursor_height_width_ratio() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let character_cell_size = Rc::new(RefCell::new(None));
    let tab = create_new_tab_with_cell_size(size, character_cell_size.clone());
    let initial_cursor_height_width_ratio = tab.tiled_panes.cursor_height_width_ratio();
    *character_cell_size.borrow_mut() = Some(SizeInPixels {
        height: 10,
        width: 4,
    });
    let cursor_height_width_ratio_after_update = tab.tiled_panes.cursor_height_width_ratio();
    assert_eq!(
        initial_cursor_height_width_ratio, None,
        "initially no ratio "
    );
    assert_eq!(
        cursor_height_width_ratio_after_update,
        Some(3),
        "ratio updated successfully"
    ); // 10 / 4 == 2.5, rounded: 3
}

#[test]
fn correctly_resize_frameless_panes_on_pane_close() {
    // check that https://github.com/zellij-org/zellij/issues/1773 is fixed
    let cols = 60;
    let rows = 20;
    let size = Size { cols, rows };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.set_pane_frames(false);

    // a single frameless pane should take up all available space
    let pane = tab.tiled_panes.panes.get(&PaneId::Terminal(1)).unwrap();
    let content_size = (pane.get_content_columns(), pane.get_content_rows());
    assert_eq!(content_size, (cols, rows));

    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::default(),
        Some(1),
        None,
    )
    .unwrap();
    tab.close_pane(PaneId::Terminal(2), true, None);

    // the size should be the same after adding and then removing a pane
    let pane = tab.tiled_panes.panes.get(&PaneId::Terminal(1)).unwrap();
    let content_size = (pane.get_content_columns(), pane.get_content_rows());
    assert_eq!(content_size, (cols, rows));
}

#[test]
fn floating_pane_z_index_is_tracked() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
    let mut tab = create_new_tab(size, stacked_resize);
    let _client_id = 1;

    // Create first floating pane (should_float = true means it will be a floating pane)
    tab.new_floating_pane(PaneId::Terminal(2), None, None, false, true, None, None)
        .unwrap();

    // Create second floating pane
    tab.new_floating_pane(PaneId::Terminal(3), None, None, false, true, None, None)
        .unwrap();

    // Verify z-indices exist and are different
    let z_index_pane2 = tab.floating_panes.get_pane_z_index(PaneId::Terminal(2));
    let z_index_pane3 = tab.floating_panes.get_pane_z_index(PaneId::Terminal(3));

    assert!(
        z_index_pane2.is_some(),
        "First floating pane should have a z-index"
    );
    assert!(
        z_index_pane3.is_some(),
        "Second floating pane should have a z-index"
    );
    assert_ne!(
        z_index_pane2, z_index_pane3,
        "Different panes should have different z-indices"
    );
}

#[test]
fn pinned_floating_pane_has_higher_z_index() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
    let mut tab = create_new_tab(size, stacked_resize);
    let _client_id = 1;

    // Create first floating pane (will be unpinned)
    tab.new_floating_pane(PaneId::Terminal(2), None, None, false, true, None, None)
        .unwrap();

    // Create second floating pane and pin it
    tab.new_floating_pane(PaneId::Terminal(3), None, None, false, true, None, None)
        .unwrap();
    tab.set_floating_pane_pinned(PaneId::Terminal(3), true);

    // Get z-indices
    let z_index_unpinned = tab
        .floating_panes
        .get_pane_z_index(PaneId::Terminal(2))
        .expect("Unpinned pane should have z-index");
    let z_index_pinned = tab
        .floating_panes
        .get_pane_z_index(PaneId::Terminal(3))
        .expect("Pinned pane should have z-index");

    assert!(
        z_index_pinned > z_index_unpinned,
        "Pinned pane should have higher z-index than unpinned pane (pinned: {}, unpinned: {})",
        z_index_pinned,
        z_index_unpinned
    );
}

#[test]
fn pinned_pane_z_index_higher_than_regular_floating_panes() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
    let mut tab = create_new_tab(size, stacked_resize);
    let _client_id = 1;

    // Create first floating pane
    tab.new_floating_pane(PaneId::Terminal(2), None, None, false, true, None, None)
        .unwrap();

    // Create second floating pane
    tab.new_floating_pane(PaneId::Terminal(3), None, None, false, true, None, None)
        .unwrap();

    // Pin the second pane so it's on top
    tab.set_floating_pane_pinned(PaneId::Terminal(3), true);

    // Verify that get_pane_z_index returns correct values for both panes
    let z_index_bottom = tab.floating_panes.get_pane_z_index(PaneId::Terminal(2));
    let z_index_top = tab.floating_panes.get_pane_z_index(PaneId::Terminal(3));

    assert!(
        z_index_bottom.is_some(),
        "Regular floating pane should have z-index"
    );
    assert!(
        z_index_top.is_some(),
        "Pinned floating pane should have z-index"
    );
    assert!(
        z_index_top.unwrap() > z_index_bottom.unwrap(),
        "Pinned pane should have higher z-index than regular floating pane (pinned: {}, regular: {})",
        z_index_top.unwrap(),
        z_index_bottom.unwrap()
    );
}

#[test]
fn active_pane_z_index_retrieved_for_cursor_visibility() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
    let mut tab = create_new_tab(size, stacked_resize);
    let client_id = 1;

    // Start with tiled panes - active pane should not have z-index
    let active_pane_id_tiled = tab.get_active_pane_id(client_id).unwrap();
    let z_index_tiled = tab.floating_panes.get_pane_z_index(active_pane_id_tiled);
    assert!(
        z_index_tiled.is_none(),
        "Tiled pane should not have z-index in floating panes"
    );

    // Create a floating pane
    tab.new_floating_pane(PaneId::Terminal(2), None, None, false, true, None, None)
        .unwrap();

    // Active pane should now have a z-index
    let active_pane_id_floating = tab.get_active_pane_id(client_id).unwrap();
    let z_index_floating = tab.floating_panes.get_pane_z_index(active_pane_id_floating);
    assert!(
        z_index_floating.is_some(),
        "Active floating pane should have a z-index"
    );
}

#[test]
fn get_pane_z_index_returns_none_for_nonexistent_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
    let mut tab = create_new_tab(size, stacked_resize);
    let _client_id = 1;

    // Create two floating panes
    tab.new_floating_pane(PaneId::Terminal(2), None, None, false, true, None, None)
        .unwrap();
    tab.new_floating_pane(PaneId::Terminal(3), None, None, false, true, None, None)
        .unwrap();

    // Query for a pane that doesn't exist
    let z_index_nonexistent = tab.floating_panes.get_pane_z_index(PaneId::Terminal(999));

    assert!(
        z_index_nonexistent.is_none(),
        "Non-existent pane should return None for z-index"
    );

    // Query for existing panes
    let z_index_2 = tab.floating_panes.get_pane_z_index(PaneId::Terminal(2));
    let z_index_3 = tab.floating_panes.get_pane_z_index(PaneId::Terminal(3));

    assert!(
        z_index_2.is_some(),
        "Existing floating pane 2 should return Some for z-index"
    );
    assert!(
        z_index_3.is_some(),
        "Existing floating pane 3 should return Some for z-index"
    );
    assert_ne!(
        z_index_2, z_index_3,
        "Different floating panes should have different z-indices"
    );
}

#[test]
pub fn bell_in_unfocused_pane_sets_notification() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);
    let client_id = 1;

    // Create a second pane; client is focused on pane 1 (PaneId::Terminal(1))
    tab.horizontal_split(new_pane_id, None, client_id, None, None, None)
        .unwrap();
    // Move focus back to pane 1
    tab.move_focus_up(client_id).unwrap();

    // Simulate bell in pane 2 via pty bytes (\x07)
    tab.handle_pty_bytes(2, vec![7u8]).unwrap();

    // Now call check_and_handle_bell_notifications as non-active tab
    let (new_panes, tab_newly_set) = tab.check_and_handle_bell_notifications(false);

    assert!(
        new_panes.contains(&new_pane_id),
        "Pane 2 should be in new_panes"
    );
    assert!(
        tab.panes_with_pending_bell.contains(&new_pane_id),
        "Pane 2 should be in panes_with_pending_bell"
    );
    assert!(
        tab.tab_has_pending_bell,
        "tab_has_pending_bell should be true"
    );
    assert!(tab_newly_set, "tab_bell_newly_set should be true");
    assert!(
        tab.get_pane_with_id(new_pane_id)
            .map(|p| p.get_bell_notification())
            .unwrap_or(false),
        "Pane 2 should have bell notification"
    );
}

#[test]
pub fn clearing_last_pane_bell_clears_tab_bell() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);
    let client_id = 1;

    tab.horizontal_split(new_pane_id, None, client_id, None, None, None)
        .unwrap();
    tab.move_focus_up(client_id).unwrap();

    // Set bell on unfocused pane 2 via check_and_handle_bell_notifications
    tab.handle_pty_bytes(2, vec![7u8]).unwrap();
    tab.check_and_handle_bell_notifications(false);

    assert!(
        tab.tab_has_pending_bell,
        "tab_has_pending_bell should be set before clearing"
    );

    // Clear bell for pane 2
    tab.clear_bell_notification_for_pane(new_pane_id);

    assert!(
        tab.panes_with_pending_bell.is_empty(),
        "panes_with_pending_bell should be empty after clearing"
    );
    assert!(
        !tab.tab_has_pending_bell,
        "tab_has_pending_bell should be false after last pane bell cleared"
    );
}

// Category 5: pane-id-based operations

#[test]
pub fn scroll_up_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    tab.scroll_up_by_pane_id(pane_id);
}

#[test]
pub fn scroll_down_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    let _ = tab.scroll_down_by_pane_id(pane_id);
}

#[test]
pub fn scroll_to_top_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    let _ = tab.scroll_to_top_by_pane_id(pane_id);
}

#[test]
pub fn scroll_to_bottom_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    let _ = tab.scroll_to_bottom_by_pane_id(pane_id);
}

#[test]
pub fn page_scroll_up_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    tab.page_scroll_up_by_pane_id(pane_id);
}

#[test]
pub fn page_scroll_down_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    let _ = tab.page_scroll_down_by_pane_id(pane_id);
}

#[test]
pub fn half_page_scroll_up_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    tab.half_page_scroll_up_by_pane_id(pane_id);
}

#[test]
pub fn half_page_scroll_down_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    let _ = tab.half_page_scroll_down_by_pane_id(pane_id);
}

#[test]
pub fn rename_pane_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    let _ = tab.rename_pane_by_pane_id(pane_id, "new-name".as_bytes().to_vec());
}

#[test]
pub fn undo_rename_pane_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    let _ = tab.rename_pane_by_pane_id(pane_id, "new-name".as_bytes().to_vec());
    tab.undo_rename_pane_by_pane_id(pane_id);
}

#[test]
pub fn close_pane_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2);
    tab.close_pane_by_pane_id(new_pane_id, None).unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 1);
    assert!(!tab.has_pane_with_pid(&new_pane_id));
}

#[test]
pub fn resize_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2);
    tab.resize_by_pane_id(
        new_pane_id,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Down)),
    );
}

#[test]
pub fn toggle_fullscreen_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2);
    tab.toggle_fullscreen_by_pane_id(new_pane_id);
}

#[test]
pub fn move_pane_by_pane_id_down() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2);
    tab.move_pane_by_pane_id(new_pane_id, Some(Direction::Down));
}

#[test]
pub fn move_pane_backwards_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let new_pane_id = PaneId::Terminal(2);
    tab.horizontal_split(new_pane_id, None, 1, None, None, None)
        .unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2);
    tab.move_pane_backwards_by_pane_id(new_pane_id);
}

#[test]
pub fn clear_screen_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    let _ = tab.clear_screen_by_pane_id(pane_id);
}

#[test]
pub fn toggle_pane_embed_or_floating_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    let _ = tab.toggle_pane_embed_or_floating_for_pane_id(pane_id, None);
}

#[test]
pub fn toggle_pane_pinned_by_pane_id() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    assert!(tab.has_pane_with_pid(&pane_id));
    tab.toggle_pane_pinned_by_pane_id(pane_id);
}

#[test]
pub fn scroll_up_nonexistent_pane_id_does_not_panic() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(999);
    assert!(!tab.has_pane_with_pid(&pane_id));
    tab.scroll_up_by_pane_id(pane_id);
}

#[test]
pub fn rename_pane_sets_current_title() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "flame");
}

#[test]
pub fn rename_pane_replaces_existing_name() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    let _ = tab.rename_pane_by_pane_id(pane_id, "spark".as_bytes().to_vec());
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "spark");
}

#[test]
pub fn rename_pane_to_empty_clears_name() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    let _ = tab.rename_pane_by_pane_id(pane_id, "".as_bytes().to_vec());
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    // Empty name should fall through to the fallback title
    assert_ne!(pane.current_title(), "flame");
}

#[test]
pub fn rename_pane_single_char() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "x".as_bytes().to_vec());
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "x");
}

#[test]
pub fn rename_pane_with_spaces() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "my pane".as_bytes().to_vec());
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "my pane");
}

#[test]
pub fn rename_pane_with_special_chars() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "pane#1 (dev)".as_bytes().to_vec());
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "pane#1 (dev)");
}

#[test]
pub fn named_pane_not_overridden_by_osc_title() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    // Simulate shell sending OSC 0 title
    let osc_title = b"\x1b]0;user@host: ~/code\x07";
    let _ = tab.handle_pty_bytes(1, osc_title.to_vec());
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "flame");
}

#[test]
pub fn unnamed_pane_shows_osc_title() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    // Send OSC 0 title without renaming the pane
    let osc_title = b"\x1b]0;user@host: ~/code\x07";
    let _ = tab.handle_pty_bytes(1, osc_title.to_vec());
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "user@host: ~/code");
}

#[test]
pub fn undo_rename_clears_name() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    let title_before = tab.get_pane_with_id(pane_id).unwrap().current_title();
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    tab.undo_rename_pane_by_pane_id(pane_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), title_before);
}

#[test]
pub fn undo_rename_on_unnamed_pane_is_noop() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    let title_before = tab.get_pane_with_id(pane_id).unwrap().current_title();
    tab.undo_rename_pane_by_pane_id(pane_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), title_before);
}

#[test]
pub fn interactive_rename_appends_to_empty_name() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let _ = tab.update_active_pane_name(vec![b's'], client_id);
    let _ = tab.update_active_pane_name(vec![b'p'], client_id);
    let _ = tab.update_active_pane_name(vec![b'a'], client_id);
    let _ = tab.update_active_pane_name(vec![b'r'], client_id);
    let _ = tab.update_active_pane_name(vec![b'k'], client_id);
    let pane = tab.get_pane_with_id(PaneId::Terminal(1)).unwrap();
    assert_eq!(pane.current_title(), "spark");
}

#[test]
pub fn interactive_rename_appends_to_existing_name() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    // Simulate entering rename mode
    if let Some(pane) = tab.get_active_pane_or_floating_pane_mut(client_id) {
        pane.store_pane_name();
    }
    let _ = tab.update_active_pane_name(vec![b's'], client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "flames");
}

#[test]
pub fn interactive_rename_backspace_removes_chars() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    if let Some(pane) = tab.get_active_pane_or_floating_pane_mut(client_id) {
        pane.store_pane_name();
    }
    // Backspace 3 times (DEL = 0x7F)
    let _ = tab.update_active_pane_name(vec![0x7f], client_id);
    let _ = tab.update_active_pane_name(vec![0x7f], client_id);
    let _ = tab.update_active_pane_name(vec![0x7f], client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "fl");
}

#[test]
pub fn interactive_rename_backspace_all_then_retype() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    if let Some(pane) = tab.get_active_pane_or_floating_pane_mut(client_id) {
        pane.store_pane_name();
    }
    // Backspace 5 times to clear "flame"
    for _ in 0..5 {
        let _ = tab.update_active_pane_name(vec![0x7f], client_id);
    }
    // Type "new"
    let _ = tab.update_active_pane_name(vec![b'n'], client_id);
    let _ = tab.update_active_pane_name(vec![b'e'], client_id);
    let _ = tab.update_active_pane_name(vec![b'w'], client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "new");
}

#[test]
pub fn interactive_rename_esc_reverts() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    // Enter rename mode
    if let Some(pane) = tab.get_active_pane_or_floating_pane_mut(client_id) {
        pane.store_pane_name();
    }
    // Type some chars
    let _ = tab.update_active_pane_name(vec![b'x'], client_id);
    let _ = tab.update_active_pane_name(vec![b'y'], client_id);
    // Esc — undo
    let _ = tab.undo_active_rename_pane(client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "flame");
}

#[test]
pub fn interactive_rename_esc_on_unnamed_stays_unnamed() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    let title_before = tab.get_pane_with_id(pane_id).unwrap().current_title();
    // Enter rename mode on unnamed pane
    if let Some(pane) = tab.get_active_pane_or_floating_pane_mut(client_id) {
        pane.store_pane_name();
    }
    let _ = tab.update_active_pane_name(vec![b'a'], client_id);
    let _ = tab.update_active_pane_name(vec![b'b'], client_id);
    // Esc
    let _ = tab.undo_active_rename_pane(client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), title_before);
}

#[test]
pub fn cli_rename_then_interactive_esc_restores_cli_name() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    // CLI rename
    let _ = tab.rename_pane_by_pane_id(pane_id, "spark".as_bytes().to_vec());
    // Enter interactive rename mode
    if let Some(pane) = tab.get_active_pane_or_floating_pane_mut(client_id) {
        pane.store_pane_name();
    }
    let _ = tab.update_active_pane_name(vec![b'!'], client_id);
    // Esc — should restore "spark"
    let _ = tab.undo_active_rename_pane(client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "spark");
}

#[test]
pub fn cli_rename_then_undo_clears_to_fallback() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let pane_id = PaneId::Terminal(1);
    let fallback_title = tab.get_pane_with_id(pane_id).unwrap().current_title();
    let _ = tab.rename_pane_by_pane_id(pane_id, "spark".as_bytes().to_vec());
    tab.undo_rename_pane_by_pane_id(pane_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), fallback_title);
}

#[test]
pub fn cli_rename_active_pane_replaces_name() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    // Set initial name
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    // CLI rename (focused pane) — full replacement
    let _ = tab.rename_active_pane("spark".as_bytes().to_vec(), client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "spark");
}

#[test]
pub fn cli_rename_active_pane_single_char() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    // CLI rename with single character — should replace, not append
    let _ = tab.rename_active_pane("x".as_bytes().to_vec(), client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "x");
}

#[test]
pub fn cli_rename_active_pane_on_unnamed_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    // Pane has no name — CLI rename should set it
    let _ = tab.rename_active_pane("spark".as_bytes().to_vec(), client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "spark");
}

#[test]
pub fn cli_rename_active_pane_to_empty_clears_name() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    let fallback = tab.get_pane_with_id(pane_id).unwrap().current_title();
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    // CLI rename to empty — should clear name
    let _ = tab.rename_active_pane("".as_bytes().to_vec(), client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), fallback);
}

#[test]
pub fn cli_rename_active_pane_then_interactive_esc_restores() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    // CLI rename
    let _ = tab.rename_active_pane("spark".as_bytes().to_vec(), client_id);
    // Enter interactive rename, type something
    if let Some(pane) = tab.get_active_pane_or_floating_pane_mut(client_id) {
        pane.store_pane_name();
    }
    let _ = tab.update_active_pane_name(vec![b'!'], client_id);
    // Esc — should restore "spark"
    let _ = tab.undo_active_rename_pane(client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.current_title(), "spark");
}

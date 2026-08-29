use super::Tab;
use crate::pane_groups::PaneGroups;
use crate::panes::kitty_graphics::KittyImageStore;
use crate::panes::sixel::SixelImageStore;
use crate::plugins::PluginInstruction;
use crate::pty_writer::PtyWriteInstruction;
use crate::screen::CopyOptions;
use crate::{os_input_output::ServerOsApi, panes::PaneId, thread_bus::ThreadSenders, ClientId};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use zellij_utils::channels::{unbounded, ChannelWithContext, Receiver, SenderWithContext};
use zellij_utils::data::{Direction, Event, NewPanePlacement, Resize, ResizeStrategy, WebSharing};
use zellij_utils::errors::prelude::*;
use zellij_utils::errors::ErrorContext;
use zellij_utils::input::layout::{SplitDirection, SplitSize, TiledPaneLayout};
use zellij_utils::input::options::PaneFrameStyle;
use zellij_utils::ipc::IpcReceiverWithContext;
use zellij_utils::pane_size::{PaneGeom, Size, SizeInPixels};

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
    create_new_tab_with_plugin_receiver(size, stacked_resize).0
}

fn create_new_tab_with_plugin_receiver(
    size: Size,
    stacked_resize: bool,
) -> (Tab, Receiver<(PluginInstruction, ErrorContext)>) {
    let index = 0;
    let position = 0;
    let name = String::new();
    let os_api = Box::new(FakeInputOutput {});
    let mut senders = ThreadSenders::default().silently_fail_on_send();
    let (to_plugin, plugin_receiver): ChannelWithContext<PluginInstruction> = unbounded();
    senders.replace_to_plugin(SenderWithContext::new(to_plugin));
    let max_panes = None;
    let mode_info = ModeInfo::default();
    let style = Style::default();
    let draw_pane_frames = PaneFrameStyle::Full;
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
    let mouse_scroll_resize = true;
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
        Rc::new(RefCell::new(false)),
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
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
        mouse_scroll_resize,
        true, // mouse_hover_effects
        true,
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
    (tab, plugin_receiver)
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
    let draw_pane_frames = PaneFrameStyle::Full;
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
    let mouse_scroll_resize = true;
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
        Rc::new(RefCell::new(false)),
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
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
        mouse_scroll_resize,
        true, // mouse_hover_effects
        true,
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
    let draw_pane_frames = PaneFrameStyle::Full;
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
    let mouse_scroll_resize = true;
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
        Rc::new(RefCell::new(false)),
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
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
        mouse_scroll_resize,
        true, // mouse_hover_effects
        true,
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.vertical_split(new_pane_id, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id, None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    assert_eq!(
        tab.tiled_panes.panes.len(),
        1,
        "Tab still has only one pane"
    );
}

#[test]
pub fn split_in_direction_without_a_connected_client_still_creates_the_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.remove_client(1);
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::Tiled {
            direction: Some(Direction::Down),
            borderless: None,
        },
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        tab.tiled_panes.panes.len(),
        2,
        "the pane is created even though no client is attached"
    );
}

#[test]
pub fn split_in_direction_without_a_connected_client_splits_the_existing_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.remove_client(1);
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::Tiled {
            direction: Some(Direction::Right),
            borderless: None,
        },
        None,
        None,
    )
    .unwrap();
    let first_pane_geom = tab
        .tiled_panes
        .panes
        .get(&PaneId::Terminal(1))
        .unwrap()
        .position_and_size();
    let new_pane_geom = tab
        .tiled_panes
        .panes
        .get(&PaneId::Terminal(2))
        .unwrap()
        .position_and_size();
    assert_eq!(first_pane_geom.x, 0, "the existing pane keeps its position");
    assert!(
        new_pane_geom.x > first_pane_geom.x,
        "the new pane is placed to the right of the existing one"
    );
    assert_eq!(
        first_pane_geom.y, new_pane_geom.y,
        "both panes share the same row"
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
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
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
    // toggling fullscreen off hands the layout solver coordinates that
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
    // fullscreen would hand the layout solver an unsatisfiable layout.
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
pub fn toggle_no_ui_fullscreen_covers_whole_display_and_restores() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
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
    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(tab.is_fullscreen_active(), "NoUi fullscreen is active");
    assert!(
        tab.fullscreen_covers_ui(),
        "NoUi fullscreen covers the UI rows"
    );
    let active_pane = tab.tiled_panes.panes.get(&PaneId::Terminal(4)).unwrap();
    assert_eq!(active_pane.x(), 0, "Pane x is on display edge");
    assert_eq!(active_pane.y(), 0, "Pane y is on display edge");
    assert_eq!(active_pane.cols(), 121, "Pane cols match display cols");
    assert_eq!(active_pane.rows(), 20, "Pane rows match display rows");
    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(!tab.is_fullscreen_active(), "NoUi fullscreen toggled off");
    assert!(
        !tab.fullscreen_covers_ui(),
        "NoUi flag cleared after toggle off"
    );
    let active_pane = tab.tiled_panes.panes.get(&PaneId::Terminal(4)).unwrap();
    assert_eq!(active_pane.x(), 61, "Pane x restored");
    assert_eq!(active_pane.y(), 10, "Pane y restored");
    assert_eq!(active_pane.cols(), 60, "Pane cols restored");
    assert_eq!(active_pane.rows(), 10, "Pane rows restored");
}

#[test]
pub fn regular_fullscreen_switches_to_no_ui_fullscreen() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
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
    assert!(tab.is_fullscreen_active(), "Regular fullscreen is active");
    assert!(
        !tab.fullscreen_covers_ui(),
        "Regular fullscreen leaves the UI rows"
    );
    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(
        tab.is_fullscreen_active(),
        "Fullscreen stays active after switching kinds"
    );
    assert!(
        tab.fullscreen_covers_ui(),
        "Fullscreen switched to covering the UI rows"
    );
    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(
        !tab.is_fullscreen_active(),
        "Fullscreen toggled off entirely"
    );
}

#[test]
pub fn no_ui_fullscreen_downgrades_to_regular_fullscreen() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
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
    let viewport = *tab.viewport.borrow();
    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(
        tab.is_fullscreen_active() && tab.fullscreen_covers_ui(),
        "NoUi fullscreen is active"
    );

    tab.toggle_active_pane_fullscreen(1);

    assert!(
        tab.is_fullscreen_active(),
        "Fullscreen stays active after the downgrade"
    );
    assert!(
        !tab.fullscreen_covers_ui(),
        "The downgrade restores the UI rows"
    );
    assert_eq!(
        tab.fullscreen_pane_id(),
        Some(PaneId::Terminal(4)),
        "The same pane remains fullscreen"
    );
    let active_pane = tab.tiled_panes.panes.get(&PaneId::Terminal(4)).unwrap();
    assert_eq!(active_pane.x(), viewport.x, "Pane x matches the viewport");
    assert_eq!(active_pane.y(), viewport.y, "Pane y matches the viewport");
    assert_eq!(
        active_pane.cols(),
        viewport.cols,
        "Pane cols match the viewport cols"
    );
    assert_eq!(
        active_pane.rows(),
        viewport.rows,
        "Pane rows match the viewport rows"
    );

    tab.toggle_active_pane_fullscreen(1);
    assert!(
        !tab.is_fullscreen_active(),
        "The next regular toggle leaves fullscreen entirely"
    );
}

#[test]
pub fn resize_whole_tab_while_no_ui_fullscreen_preserves_no_ui() {
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
    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(
        tab.fullscreen_covers_ui(),
        "NoUi fullscreen is active before the resize"
    );

    let new_size = Size { cols: 80, rows: 30 };
    tab.resize_whole_tab(new_size).unwrap();

    assert!(
        tab.is_fullscreen_active(),
        "Fullscreen is preserved across a host-terminal resize"
    );
    assert!(
        tab.fullscreen_covers_ui(),
        "NoUi kind is preserved across a host-terminal resize"
    );
    let active_pane = tab
        .tiled_panes
        .panes
        .get(&PaneId::Terminal(4))
        .expect("Active fullscreen pane is still present");
    assert_eq!(
        active_pane.cols(),
        new_size.cols,
        "NoUi fullscreen pane cols match the new display cols"
    );
    assert_eq!(
        active_pane.rows(),
        new_size.rows,
        "NoUi fullscreen pane rows match the new display rows"
    );
    assert_eq!(
        active_pane.x(),
        0,
        "NoUi fullscreen pane x is at display edge"
    );
    assert_eq!(
        active_pane.y(),
        0,
        "NoUi fullscreen pane y is at display edge"
    );
}

fn create_tab_with_two_floating_panes() -> Tab {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.new_floating_pane(PaneId::Terminal(2), None, None, false, true, None, None)
        .unwrap();
    tab.new_floating_pane(PaneId::Terminal(3), None, None, false, true, None, None)
        .unwrap();
    tab
}

fn floating_pane_geom(tab: &Tab, pane_id: PaneId) -> PaneGeom {
    tab.floating_panes
        .get(&pane_id)
        .expect("floating pane exists")
        .current_geom()
}

#[test]
pub fn toggle_floating_pane_fullscreen_expands_over_viewport() {
    let mut tab = create_tab_with_two_floating_panes();
    let viewport = *tab.viewport.borrow();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");

    tab.toggle_active_pane_fullscreen(1);

    assert!(tab.is_fullscreen_active(), "Floating fullscreen is active");
    assert!(
        !tab.fullscreen_covers_ui(),
        "Regular floating fullscreen leaves the UI rows"
    );
    assert_eq!(
        tab.fullscreen_pane_id(),
        Some(active_pane_id),
        "Fullscreen tracks the active floating pane"
    );
    let geom = floating_pane_geom(&tab, active_pane_id);
    assert_eq!(geom.x, viewport.x, "Fullscreen pane x matches viewport");
    assert_eq!(geom.y, viewport.y, "Fullscreen pane y matches viewport");
    assert_eq!(
        geom.cols.as_usize(),
        viewport.cols,
        "Fullscreen pane cols match viewport cols"
    );
    assert_eq!(
        geom.rows.as_usize(),
        viewport.rows,
        "Fullscreen pane rows match viewport rows"
    );
}

#[test]
pub fn toggle_floating_pane_no_ui_fullscreen_covers_whole_display() {
    let mut tab = create_tab_with_two_floating_panes();
    let display_area = *tab.display_area.borrow();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");

    tab.toggle_active_pane_no_ui_fullscreen(1);

    assert!(
        tab.is_fullscreen_active(),
        "Floating no-ui fullscreen is active"
    );
    assert!(
        tab.fullscreen_covers_ui(),
        "Floating no-ui fullscreen covers the UI rows"
    );
    let geom = floating_pane_geom(&tab, active_pane_id);
    assert_eq!(geom.x, 0, "No-ui fullscreen pane x is on display edge");
    assert_eq!(geom.y, 0, "No-ui fullscreen pane y is on display edge");
    assert_eq!(
        geom.cols.as_usize(),
        display_area.cols,
        "No-ui fullscreen pane cols match display cols"
    );
    assert_eq!(
        geom.rows.as_usize(),
        display_area.rows,
        "No-ui fullscreen pane rows match display rows"
    );
}

#[test]
pub fn toggle_floating_pane_fullscreen_off_restores_geometry() {
    let mut tab = create_tab_with_two_floating_panes();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");
    let geom_before = floating_pane_geom(&tab, active_pane_id);

    tab.toggle_active_pane_fullscreen(1);
    assert!(tab.is_fullscreen_active(), "Fullscreen active after toggle");
    tab.toggle_active_pane_fullscreen(1);

    assert!(
        !tab.is_fullscreen_active(),
        "Fullscreen cleared after toggling off"
    );
    assert_eq!(
        floating_pane_geom(&tab, active_pane_id),
        geom_before,
        "Floating pane geometry restored after fullscreen off"
    );
}

#[test]
pub fn regular_floating_fullscreen_switches_to_no_ui() {
    let mut tab = create_tab_with_two_floating_panes();
    tab.toggle_active_pane_fullscreen(1);
    assert!(
        tab.is_fullscreen_active() && !tab.fullscreen_covers_ui(),
        "Regular floating fullscreen active"
    );

    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(
        tab.is_fullscreen_active(),
        "Fullscreen stays active when switching kinds"
    );
    assert!(
        tab.fullscreen_covers_ui(),
        "Fullscreen switched to covering the UI rows"
    );

    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(
        !tab.is_fullscreen_active(),
        "Fullscreen toggled off entirely"
    );
}

#[test]
pub fn floating_no_ui_fullscreen_downgrades_to_regular_fullscreen() {
    let mut tab = create_tab_with_two_floating_panes();
    let viewport = *tab.viewport.borrow();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");
    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(
        tab.is_fullscreen_active() && tab.fullscreen_covers_ui(),
        "Floating no-ui fullscreen is active"
    );

    tab.toggle_active_pane_fullscreen(1);

    assert!(
        tab.is_fullscreen_active(),
        "Fullscreen stays active after the downgrade"
    );
    assert!(
        !tab.fullscreen_covers_ui(),
        "The downgrade restores the UI rows"
    );
    let geom = floating_pane_geom(&tab, active_pane_id);
    assert_eq!(geom.x, viewport.x, "Pane x matches the viewport");
    assert_eq!(geom.y, viewport.y, "Pane y matches the viewport");
    assert_eq!(
        geom.cols.as_usize(),
        viewport.cols,
        "Pane cols match the viewport cols"
    );
    assert_eq!(
        geom.rows.as_usize(),
        viewport.rows,
        "Pane rows match the viewport rows"
    );

    tab.toggle_active_pane_fullscreen(1);
    assert!(
        !tab.is_fullscreen_active(),
        "The next regular toggle leaves fullscreen entirely"
    );
}

#[test]
pub fn floating_fullscreen_on_lone_floating_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.new_floating_pane(PaneId::Terminal(2), None, None, false, true, None, None)
        .unwrap();
    let viewport = *tab.viewport.borrow();

    tab.toggle_active_pane_fullscreen(1);

    assert!(
        tab.is_fullscreen_active(),
        "A lone floating pane still enters fullscreen"
    );
    let geom = floating_pane_geom(&tab, PaneId::Terminal(2));
    assert_eq!(
        geom.cols.as_usize(),
        viewport.cols,
        "Lone floating fullscreen pane covers the viewport cols"
    );
    assert_eq!(
        geom.rows.as_usize(),
        viewport.rows,
        "Lone floating fullscreen pane covers the viewport rows"
    );
}

#[test]
pub fn floating_and_tiled_fullscreen_are_mutually_exclusive() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
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
    tab.new_floating_pane(PaneId::Terminal(3), None, None, false, true, None, None)
        .unwrap();

    tab.toggle_active_pane_fullscreen(1);
    assert_eq!(
        tab.fullscreen_pane_id(),
        Some(PaneId::Terminal(3)),
        "The focused floating pane is fullscreen"
    );
    assert!(
        !tab.tiled_panes.fullscreen_is_active(),
        "No tiled fullscreen while a floating pane is fullscreen"
    );

    tab.hide_floating_panes();
    tab.toggle_active_pane_fullscreen(1);
    assert!(
        tab.tiled_panes.fullscreen_is_active(),
        "A tiled pane can be fullscreen after floating is hidden"
    );
    assert!(
        !tab.floating_panes.fullscreen_is_active(),
        "No floating fullscreen while a tiled pane is fullscreen"
    );
}

#[test]
pub fn move_focus_while_floating_fullscreen_transfers_fullscreen() {
    let mut tab = create_tab_with_two_floating_panes();
    let first = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");

    tab.toggle_active_pane_fullscreen(1);
    assert_eq!(
        tab.fullscreen_pane_id(),
        Some(first),
        "Fullscreen starts on the focused floating pane"
    );

    let _ = tab.move_focus_left(1);

    assert!(
        tab.is_fullscreen_active(),
        "Fullscreen stays active after moving focus"
    );
    let new_active = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused after the move");
    assert_eq!(
        tab.fullscreen_pane_id(),
        Some(new_active),
        "Fullscreen transferred to the newly focused floating pane"
    );
}

#[test]
pub fn resize_while_floating_fullscreen_is_noop() {
    let mut tab = create_tab_with_two_floating_panes();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");
    tab.toggle_active_pane_fullscreen(1);
    let geom_before = floating_pane_geom(&tab, active_pane_id);

    tab.resize(1, ResizeStrategy::new(Resize::Increase, None))
        .unwrap();

    assert_eq!(
        floating_pane_geom(&tab, active_pane_id),
        geom_before,
        "Resizing a floating fullscreen pane does not change its geometry"
    );
}

#[test]
pub fn move_pane_while_floating_fullscreen_is_noop() {
    let mut tab = create_tab_with_two_floating_panes();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");
    tab.toggle_active_pane_fullscreen(1);
    let geom_before = floating_pane_geom(&tab, active_pane_id);

    tab.move_active_pane_down(1);
    tab.move_active_pane_up(1);
    tab.move_active_pane_left(1);
    tab.move_active_pane_right(1);

    assert_eq!(
        floating_pane_geom(&tab, active_pane_id),
        geom_before,
        "Moving a floating fullscreen pane does not change its geometry"
    );
}

#[test]
pub fn closing_floating_fullscreen_pane_clears_state() {
    let mut tab = create_tab_with_two_floating_panes();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");
    tab.toggle_active_pane_fullscreen(1);
    assert!(tab.is_fullscreen_active(), "Fullscreen active before close");

    tab.close_pane(active_pane_id, false, None);

    assert!(
        !tab.floating_panes.fullscreen_is_active(),
        "Closing the fullscreen floating pane clears fullscreen state"
    );
}

#[test]
pub fn adding_floating_pane_while_fullscreen_unsets_fullscreen() {
    let mut tab = create_tab_with_two_floating_panes();
    tab.toggle_active_pane_fullscreen(1);
    assert!(tab.is_fullscreen_active(), "Fullscreen active before add");

    tab.new_floating_pane(PaneId::Terminal(4), None, None, false, true, None, None)
        .unwrap();

    assert!(
        !tab.floating_panes.fullscreen_is_active(),
        "Adding a floating pane breaks out of floating fullscreen"
    );
}

#[test]
pub fn hiding_floating_layer_while_fullscreen_unsets_fullscreen() {
    let mut tab = create_tab_with_two_floating_panes();
    tab.toggle_active_pane_fullscreen(1);
    assert!(tab.is_fullscreen_active(), "Fullscreen active before hide");

    tab.hide_floating_panes();

    assert!(
        !tab.floating_panes.fullscreen_is_active(),
        "Hiding the floating layer clears floating fullscreen"
    );
}

#[test]
pub fn embed_floating_fullscreen_pane_unsets_and_embeds() {
    let mut tab = create_tab_with_two_floating_panes();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");
    tab.toggle_active_pane_fullscreen(1);
    assert!(tab.is_fullscreen_active(), "Fullscreen active before embed");

    tab.toggle_pane_embed_or_floating(1).unwrap();

    assert!(
        !tab.floating_panes.fullscreen_is_active(),
        "Embedding clears floating fullscreen state"
    );
    assert!(
        tab.tiled_panes.panes_contain(&active_pane_id),
        "The embedded pane is now a tiled pane"
    );
}

#[test]
pub fn resize_whole_tab_while_floating_fullscreen_preserves_regular() {
    let mut tab = create_tab_with_two_floating_panes();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");
    tab.toggle_active_pane_fullscreen(1);

    let new_size = Size { cols: 80, rows: 30 };
    tab.resize_whole_tab(new_size).unwrap();

    assert!(
        tab.is_fullscreen_active() && !tab.fullscreen_covers_ui(),
        "Regular floating fullscreen preserved across resize"
    );
    let viewport = *tab.viewport.borrow();
    let geom = floating_pane_geom(&tab, active_pane_id);
    assert_eq!(
        geom.cols.as_usize(),
        viewport.cols,
        "Floating fullscreen pane re-expands to the new viewport cols"
    );
    assert_eq!(
        geom.rows.as_usize(),
        viewport.rows,
        "Floating fullscreen pane re-expands to the new viewport rows"
    );
}

#[test]
pub fn resize_whole_tab_while_floating_no_ui_fullscreen_preserves_no_ui() {
    let mut tab = create_tab_with_two_floating_panes();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");
    tab.toggle_active_pane_no_ui_fullscreen(1);

    let new_size = Size { cols: 80, rows: 30 };
    tab.resize_whole_tab(new_size).unwrap();

    assert!(
        tab.fullscreen_covers_ui(),
        "Floating no-ui fullscreen preserved across resize"
    );
    let geom = floating_pane_geom(&tab, active_pane_id);
    assert_eq!(
        geom.cols.as_usize(),
        new_size.cols,
        "Floating no-ui fullscreen pane re-expands to the new display cols"
    );
    assert_eq!(
        geom.rows.as_usize(),
        new_size.rows,
        "Floating no-ui fullscreen pane re-expands to the new display rows"
    );
}

#[test]
pub fn pane_info_reports_floating_fullscreen() {
    let mut tab = create_tab_with_two_floating_panes();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(1)
        .expect("a floating pane is focused");
    tab.toggle_active_pane_fullscreen(1);

    let fullscreen_info = tab
        .get_pane_info(active_pane_id)
        .expect("pane info for the fullscreen floating pane");
    assert!(
        fullscreen_info.is_fullscreen,
        "The fullscreen floating pane reports is_fullscreen"
    );
    assert!(
        fullscreen_info.is_floating,
        "The fullscreen pane still reports is_floating"
    );

    let other_pane_id = if active_pane_id == PaneId::Terminal(2) {
        PaneId::Terminal(3)
    } else {
        PaneId::Terminal(2)
    };
    let other_info = tab
        .get_pane_info(other_pane_id)
        .expect("pane info for the other floating pane");
    assert!(
        !other_info.is_fullscreen,
        "A non-fullscreen floating pane does not report is_fullscreen"
    );
}

#[test]
pub fn toggle_floating_fullscreen_by_pane_id() {
    let mut tab = create_tab_with_two_floating_panes();

    tab.toggle_pane_fullscreen(PaneId::Terminal(2));
    assert_eq!(
        tab.fullscreen_pane_id(),
        Some(PaneId::Terminal(2)),
        "By-id fullscreen targets the requested floating pane"
    );

    tab.toggle_pane_fullscreen(PaneId::Terminal(2));
    assert!(
        !tab.is_fullscreen_active(),
        "By-id toggle off clears floating fullscreen"
    );
}

fn create_tab_with_four_panes() -> Tab {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
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
    tab
}

fn tiled_pane_geoms(tab: &Tab) -> Vec<(PaneId, PaneGeom)> {
    tab.tiled_panes
        .panes
        .iter()
        .map(|(pane_id, pane)| (*pane_id, pane.position_and_size()))
        .collect()
}

fn open_pane_five(tab: &mut Tab) {
    tab.new_pane(
        PaneId::Terminal(5),
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

fn assert_panes_tile_the_whole_display(tab: &Tab, display: Size) {
    let geoms = tiled_pane_geoms(tab);
    let mut total_pane_area = 0;
    for (pane_id, geom) in &geoms {
        let right_edge = geom.x + geom.cols.as_usize();
        let bottom_edge = geom.y + geom.rows.as_usize();
        assert!(
            right_edge <= display.cols && bottom_edge <= display.rows,
            "{pane_id:?} fits inside the display: {geom:?}"
        );
        total_pane_area += geom.rows.as_usize() * geom.cols.as_usize();
    }
    for (pane_id, geom) in &geoms {
        for (other_pane_id, other_geom) in &geoms {
            if pane_id == other_pane_id {
                continue;
            }
            let overlap_horizontally = geom.x < other_geom.x + other_geom.cols.as_usize()
                && other_geom.x < geom.x + geom.cols.as_usize();
            let overlap_vertically = geom.y < other_geom.y + other_geom.rows.as_usize()
                && other_geom.y < geom.y + geom.rows.as_usize();
            assert!(
                !(overlap_horizontally && overlap_vertically),
                "{pane_id:?} and {other_pane_id:?} do not overlap"
            );
        }
    }
    assert_eq!(
        total_pane_area,
        display.rows * display.cols,
        "Panes cover the whole display with no gaps"
    );
}

#[test]
pub fn opening_new_pane_while_fullscreen_unsets_fullscreen() {
    let mut tab = create_tab_with_four_panes();
    tab.toggle_active_pane_fullscreen(1);
    assert!(tab.is_fullscreen_active(), "Fullscreen active before split");
    open_pane_five(&mut tab);

    assert!(
        !tab.is_fullscreen_active(),
        "Opening a new pane breaks out of fullscreen"
    );
    assert_eq!(
        tiled_pane_geoms(&tab).len(),
        5,
        "All panes including the new one are tiled"
    );
    assert_panes_tile_the_whole_display(
        &tab,
        Size {
            cols: 121,
            rows: 20,
        },
    );
}

#[test]
pub fn opening_new_pane_while_no_ui_fullscreen_unsets_fullscreen() {
    let mut tab = create_tab_with_four_panes();
    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(
        tab.fullscreen_covers_ui(),
        "NoUi fullscreen active before split"
    );
    open_pane_five(&mut tab);

    assert!(
        !tab.is_fullscreen_active(),
        "Opening a new pane breaks out of no-ui fullscreen"
    );
    assert!(
        !tab.fullscreen_covers_ui(),
        "NoUi flag cleared after breaking out"
    );
    assert_eq!(
        tiled_pane_geoms(&tab).len(),
        5,
        "All panes including the new one are tiled"
    );
    assert_panes_tile_the_whole_display(
        &tab,
        Size {
            cols: 121,
            rows: 20,
        },
    );
}

#[test]
pub fn closing_the_fullscreen_pane_restores_remaining_layout() {
    let mut tab_without_fullscreen = create_tab_with_four_panes();
    tab_without_fullscreen.close_pane(PaneId::Terminal(4), false, None);

    let mut tab = create_tab_with_four_panes();
    tab.toggle_active_pane_fullscreen(1);
    assert!(tab.is_fullscreen_active(), "Fullscreen active before close");
    tab.close_pane(PaneId::Terminal(4), false, None);

    assert!(
        !tab.is_fullscreen_active(),
        "Closing the fullscreen pane leaves fullscreen"
    );
    assert_eq!(
        tiled_pane_geoms(&tab),
        tiled_pane_geoms(&tab_without_fullscreen),
        "Remaining panes match a close that never went through fullscreen"
    );
}

#[test]
pub fn closing_the_no_ui_fullscreen_pane_restores_remaining_layout() {
    let mut tab_without_fullscreen = create_tab_with_four_panes();
    tab_without_fullscreen.close_pane(PaneId::Terminal(4), false, None);

    let mut tab = create_tab_with_four_panes();
    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(
        tab.fullscreen_covers_ui(),
        "NoUi fullscreen active before close"
    );
    tab.close_pane(PaneId::Terminal(4), false, None);

    assert!(
        !tab.is_fullscreen_active(),
        "Closing the no-ui fullscreen pane leaves fullscreen"
    );
    assert!(
        !tab.fullscreen_covers_ui(),
        "NoUi flag cleared after the close"
    );
    assert_eq!(
        tiled_pane_geoms(&tab),
        tiled_pane_geoms(&tab_without_fullscreen),
        "Remaining panes match a close that never went through no-ui fullscreen"
    );
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
pub fn opening_scrollback_editor_on_fullscreen_floating_pane_retargets_fullscreen() {
    let client_id = 1;
    let mut tab = create_tab_with_two_floating_panes();
    let viewport = *tab.viewport.borrow();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(client_id)
        .expect("a floating pane is focused");
    let original_geom = tab
        .floating_panes
        .get(&active_pane_id)
        .expect("focused floating pane exists")
        .position_and_size();

    tab.toggle_active_pane_fullscreen(client_id);
    assert_eq!(
        tab.floating_panes.fullscreen_pane_id(),
        Some(active_pane_id),
        "fullscreen tracks the original floating pane",
    );

    let editor_pane_id = PaneId::Terminal(99);
    tab.replace_active_pane_with_editor_pane(editor_pane_id, client_id)
        .unwrap();
    assert_eq!(
        tab.floating_panes.fullscreen_pane_id(),
        Some(editor_pane_id),
        "fullscreen now tracks the editor pane id, not the suppressed one",
    );
    let editor_geom = floating_pane_geom(&tab, editor_pane_id);
    assert_eq!(
        editor_geom.cols.as_usize(),
        viewport.cols,
        "the editor pane covers the viewport cols",
    );
    assert_eq!(
        editor_geom.rows.as_usize(),
        viewport.rows,
        "the editor pane covers the viewport rows",
    );

    tab.toggle_active_pane_fullscreen(client_id);
    assert!(
        !tab.floating_panes.fullscreen_is_active(),
        "fullscreen is cleared after the second toggle",
    );
    let editor_pane = tab
        .floating_panes
        .get(&editor_pane_id)
        .expect("editor pane is present in floating panes");
    assert!(
        editor_pane.geom_override().is_none(),
        "editor pane no longer carries the fullscreen geom_override",
    );
    assert_eq!(
        editor_pane.position_and_size(),
        original_geom,
        "editor pane falls back to the replaced pane's original geometry",
    );
}

#[test]
pub fn closing_fullscreen_floating_scrollback_editor_restores_geometry() {
    let client_id = 1;
    let mut tab = create_tab_with_two_floating_panes();
    let active_pane_id = tab
        .floating_panes
        .active_pane_id(client_id)
        .expect("a floating pane is focused");
    let original_geom = tab
        .floating_panes
        .get(&active_pane_id)
        .expect("focused floating pane exists")
        .position_and_size();

    let editor_pane_id = PaneId::Terminal(99);
    tab.replace_active_pane_with_editor_pane(editor_pane_id, client_id)
        .unwrap();
    tab.toggle_active_pane_fullscreen(client_id);
    assert_eq!(
        tab.floating_panes.fullscreen_pane_id(),
        Some(editor_pane_id),
        "fullscreen tracks the editor pane",
    );

    tab.close_pane(editor_pane_id, false, None);
    assert_eq!(
        tab.floating_panes.fullscreen_pane_id(),
        Some(active_pane_id),
        "fullscreen now tracks the restored suppressed pane",
    );

    tab.toggle_active_pane_fullscreen(client_id);
    assert!(
        !tab.floating_panes.fullscreen_is_active(),
        "fullscreen is cleared after the second toggle",
    );
    let restored_pane = tab
        .floating_panes
        .get(&active_pane_id)
        .expect("restored pane is present");
    assert!(
        restored_pane.geom_override().is_none(),
        "restored pane no longer carries the fullscreen geom_override",
    );
    assert_eq!(
        restored_pane.position_and_size(),
        original_geom,
        "restored pane is back to its original geometry",
    );
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
fn switch_to_last_pane_fullscreen() {
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

    // order is now 1 2 3 4, current active is Terminal 4

    active_tab.switch_last_pane_fullscreen(1);

    // the position should now be in Terminal 3

    assert_eq!(
        active_tab.get_active_pane_id(1).unwrap(),
        PaneId::Terminal(3),
        "Active pane did not switch to the last pane in fullscreen mode"
    );

    active_tab.switch_last_pane_fullscreen(1);

    // the position should now be back in Terminal 4

    assert_eq!(
        active_tab.get_active_pane_id(1).unwrap(),
        PaneId::Terminal(4),
        "Active pane did not switch to the last pane in fullscreen mode"
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
    tab.horizontal_split(new_pane_id, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id, None, 1, None, None)
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
    tab.vertical_split(new_pane_id, None, 1, None, None)
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
    tab.vertical_split(new_pane_id, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None)
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
    tab.vertical_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None)
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
    tab.vertical_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None)
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

    tab.vertical_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(new_pane_id_4, None, 1, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(new_pane_id_5, None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_down(&mut tab, 1);
    tab.vertical_split(new_pane_id_6, None, 1, None, None)
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

    tab.vertical_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(new_pane_id_4, None, 1, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(new_pane_id_5, None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_up(&mut tab, 1);
    tab.vertical_split(new_pane_id_6, None, 1, None, None)
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

    tab.horizontal_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(new_pane_id_4, None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(new_pane_id_5, None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab_resize_right(&mut tab, 1);
    tab_resize_up(&mut tab, 1);
    tab_resize_up(&mut tab, 1);
    tab.horizontal_split(new_pane_id_6, None, 1, None, None)
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

    tab.horizontal_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(new_pane_id_4, None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(new_pane_id_5, None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab_resize_left(&mut tab, 1);
    tab_resize_up(&mut tab, 1);
    tab_resize_up(&mut tab, 1);
    tab.horizontal_split(new_pane_id_6, None, 1, None, None)
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

    tab.horizontal_split(new_pane_id, None, 1, None, None)
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

    tab.horizontal_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None)
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

    tab.horizontal_split(new_pane_id, None, 1, None, None)
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

    tab.horizontal_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None)
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

    tab.vertical_split(new_pane_id, None, 1, None, None)
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

    tab.vertical_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None)
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

    tab.vertical_split(new_pane_id, None, 1, None, None)
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

    tab.vertical_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None)
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

    tab.horizontal_split(new_pane_id, None, 1, None, None)
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

    tab.horizontal_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None)
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

    tab.horizontal_split(new_pane_id, None, 1, None, None)
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

    tab.horizontal_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.vertical_split(new_pane_id_3, None, 1, None, None)
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

    tab.vertical_split(new_pane_id, None, 1, None, None)
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

    tab.vertical_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None)
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

    tab.vertical_split(new_pane_id, None, 1, None, None)
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

    tab.vertical_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_3, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.horizontal_split(new_pane_id_2, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id_1, None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(new_pane_id_2, None, 1, None, None)
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
    tab.horizontal_split(pane_to_the_left, None, 1, None, None)
        .unwrap();
    tab.vertical_split(focused_pane, None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(pane_above, None, 1, None, None).unwrap();
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
    tab.horizontal_split(pane_below_and_left, None, 1, None, None)
        .unwrap();
    tab.vertical_split(pane_below, None, 1, None, None).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(focused_pane, None, 1, None, None)
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
    tab.horizontal_split(focused_pane, None, 1, None, None)
        .unwrap();
    tab.vertical_split(pane_to_the_right, None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(pane_above_and_right, None, 1, None, None)
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
    tab.horizontal_split(pane_below, None, 1, None, None)
        .unwrap();
    tab.vertical_split(pane_below_and_right, None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(pane_to_the_right, None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.vertical_split(PaneId::Terminal(7), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(8), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(7), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(8), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_down(&mut tab, 1);
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(7), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(8), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_down(&mut tab, 1);
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(7), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(8), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_right(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_up(&mut tab, 1);
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(7), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(8), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab_resize_up(&mut tab, 1);
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(7), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(8), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.vertical_split(PaneId::Terminal(7), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(8), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(4), None, 1, None, None)
        .unwrap();
    tab.move_focus_down(1).unwrap();
    tab.vertical_split(PaneId::Terminal(5), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(6), None, 1, None, None)
        .unwrap();
    tab.move_focus_up(1).unwrap();
    tab.move_focus_left(1).unwrap();
    tab.vertical_split(PaneId::Terminal(7), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(8), None, 1, None, None)
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
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
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
    tab.vertical_split(new_pane_id_1, None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.move_focus_left(1).unwrap();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
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
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.vertical_split(PaneId::Terminal(3), None, 1, None, None)
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
    tab.set_pane_frames(PaneFrameStyle::None);

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
    tab.horizontal_split(new_pane_id, None, client_id, None, None)
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

    tab.horizontal_split(new_pane_id, None, client_id, None, None)
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
    tab.horizontal_split(new_pane_id, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id, None, 1, None, None)
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
    tab.horizontal_split(new_pane_id, None, 1, None, None)
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
pub fn interactive_rename_nul_clears_existing_name() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    let _ = tab.update_active_pane_name(vec![0], client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.custom_title(), None);

    let _ = tab.update_active_pane_name(b"spark".to_vec(), client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.custom_title(), Some("spark".to_owned()));
}

#[test]
pub fn interactive_rename_sanitizes_other_control_characters() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let client_id = 1;
    let pane_id = PaneId::Terminal(1);
    let _ = tab.rename_pane_by_pane_id(pane_id, "flame".as_bytes().to_vec());
    let _ = tab.update_active_pane_name(vec![0x01, 0x07, b'\n', b'\r', 0x1b], client_id);
    let pane = tab.get_pane_with_id(pane_id).unwrap();
    assert_eq!(pane.custom_title(), Some("flame".to_owned()));
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

fn install_pty_writer_capture(
    tab: &mut Tab,
) -> Receiver<(PtyWriteInstruction, zellij_utils::errors::ErrorContext)> {
    let (tx, rx) = unbounded();
    tab.senders
        .replace_to_pty_writer(SenderWithContext::new(tx));
    rx
}

fn drain_pty_writer(
    rx: &Receiver<(PtyWriteInstruction, zellij_utils::errors::ErrorContext)>,
) -> Vec<PtyWriteInstruction> {
    let mut out = Vec::new();
    while let Ok((msg, _ctx)) = rx.try_recv() {
        out.push(msg);
    }
    out
}

#[test]
pub fn scroll_terminal_up_forwards_sgr_when_pane_tracks_mouse() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let rx = install_pty_writer_capture(&mut tab);
    tab.handle_pty_bytes(1, b"\x1b[?1000h\x1b[?1006h".to_vec())
        .unwrap();

    tab.scroll_terminal_up(1).unwrap();

    let writes = drain_pty_writer(&rx);
    assert!(!writes.is_empty(), "expected at least one pty write");
    match &writes[0] {
        PtyWriteInstruction::Write(bytes, terminal_id, _) => {
            assert_eq!(*terminal_id, 1);
            let s = String::from_utf8_lossy(bytes);
            assert!(
                s.starts_with("\u{1b}[<64;"),
                "expected SGR wheel-up (button 64) prefix, got {s:?}"
            );
        },
        other => panic!("expected PtyWriteInstruction::Write, got {other:?}"),
    }
}

#[test]
pub fn scroll_terminal_down_forwards_sgr_when_pane_tracks_mouse() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let rx = install_pty_writer_capture(&mut tab);
    tab.handle_pty_bytes(1, b"\x1b[?1000h\x1b[?1006h".to_vec())
        .unwrap();

    tab.scroll_terminal_down(1).unwrap();

    let writes = drain_pty_writer(&rx);
    assert!(!writes.is_empty(), "expected at least one pty write");
    match &writes[0] {
        PtyWriteInstruction::Write(bytes, terminal_id, _) => {
            assert_eq!(*terminal_id, 1);
            let s = String::from_utf8_lossy(bytes);
            assert!(
                s.starts_with("\u{1b}[<65;"),
                "expected SGR wheel-down (button 65) prefix, got {s:?}"
            );
        },
        other => panic!("expected PtyWriteInstruction::Write, got {other:?}"),
    }
}

#[test]
pub fn scroll_terminal_up_emits_arrow_up_in_alt_mode_without_mouse() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let rx = install_pty_writer_capture(&mut tab);
    tab.handle_pty_bytes(1, b"\x1b[?1049h".to_vec()).unwrap();

    tab.scroll_terminal_up(1).unwrap();

    let writes = drain_pty_writer(&rx);
    assert!(!writes.is_empty(), "expected at least one pty write");
    match &writes[0] {
        PtyWriteInstruction::Write(bytes, terminal_id, _) => {
            assert_eq!(*terminal_id, 1);
            assert_eq!(bytes.as_slice(), b"\x1b[A");
        },
        other => panic!("expected PtyWriteInstruction::Write, got {other:?}"),
    }
}

#[test]
pub fn scroll_terminal_down_emits_arrow_down_in_alt_mode_without_mouse() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let rx = install_pty_writer_capture(&mut tab);
    tab.handle_pty_bytes(1, b"\x1b[?1049h".to_vec()).unwrap();

    tab.scroll_terminal_down(1).unwrap();

    let writes = drain_pty_writer(&rx);
    assert!(!writes.is_empty(), "expected at least one pty write");
    match &writes[0] {
        PtyWriteInstruction::Write(bytes, terminal_id, _) => {
            assert_eq!(*terminal_id, 1);
            assert_eq!(bytes.as_slice(), b"\x1b[B");
        },
        other => panic!("expected PtyWriteInstruction::Write, got {other:?}"),
    }
}

#[test]
pub fn scroll_terminal_up_walks_scrollback_in_regular_mode_no_pty_write() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let rx = install_pty_writer_capture(&mut tab);

    tab.scroll_terminal_up(1).unwrap();

    let writes = drain_pty_writer(&rx);
    assert!(
        writes.is_empty(),
        "expected no pty writes in regular mode, got {writes:?}"
    );
}

#[test]
pub fn scroll_terminal_down_walks_scrollback_in_regular_mode_no_pty_write() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let rx = install_pty_writer_capture(&mut tab);

    tab.scroll_terminal_down(1).unwrap();

    let writes = drain_pty_writer(&rx);
    assert!(
        writes.is_empty(),
        "expected no pty writes in regular mode, got {writes:?}"
    );
}

#[test]
pub fn scroll_terminal_up_nonexistent_pane_id_is_a_noop() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let rx = install_pty_writer_capture(&mut tab);

    tab.scroll_terminal_up(999).unwrap();

    let writes = drain_pty_writer(&rx);
    assert!(writes.is_empty());
}

#[test]
pub fn scroll_terminal_down_nonexistent_pane_id_is_a_noop() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let rx = install_pty_writer_capture(&mut tab);

    tab.scroll_terminal_down(999).unwrap();

    let writes = drain_pty_writer(&rx);
    assert!(writes.is_empty());
}

#[test]
fn floating_plugin_panes_are_notified_when_their_tab_is_hidden() {
    // Regression test: Tab::visible() used to only walk the tiled panes, so a plugin living in a
    // floating pane never learned that its tab had gone away. Plugins that idle on a timer (the
    // session-manager re-reads the whole session list once a second) went on doing that work
    // forever, even with no client attached to the session at all.
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, plugin_receiver) = create_new_tab_with_plugin_receiver(size, true);
    let plugin_pane_id = PaneId::Plugin(1);
    tab.new_pane(
        plugin_pane_id,
        None,
        None,
        false,
        true,
        NewPanePlacement::Floating(None),
        Some(1),
        None,
    )
    .unwrap();

    tab.visible(false).unwrap();

    let mut told_the_floating_plugin = false;
    while let Ok((instruction, _)) = plugin_receiver.try_recv() {
        if let PluginInstruction::Update(updates) = instruction {
            for (pid, _client_id, event) in updates {
                if pid == Some(1) && matches!(event, Event::Visible(false)) {
                    told_the_floating_plugin = true;
                }
            }
        }
    }
    assert!(
        told_the_floating_plugin,
        "a plugin in a floating pane should be sent Event::Visible(false) when its tab is hidden"
    );
}

fn drain_visible_events(
    plugin_receiver: &Receiver<(PluginInstruction, ErrorContext)>,
) -> Vec<(Option<u32>, bool)> {
    let mut visible_events = vec![];
    while let Ok((instruction, _)) = plugin_receiver.try_recv() {
        if let PluginInstruction::Update(updates) = instruction {
            for (pid, _client_id, event) in updates {
                if let Event::Visible(is_visible) = event {
                    visible_events.push((pid, is_visible));
                }
            }
        }
    }
    visible_events
}

fn tab_with_floating_plugin_pane(
    plugin_pid: u32,
) -> (Tab, Receiver<(PluginInstruction, ErrorContext)>) {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, plugin_receiver) = create_new_tab_with_plugin_receiver(size, true);
    tab.new_pane(
        PaneId::Plugin(plugin_pid),
        None,
        None,
        false,
        true,
        NewPanePlacement::Floating(None),
        Some(1),
        None,
    )
    .unwrap();
    (tab, plugin_receiver)
}

#[test]
fn floating_plugin_panes_are_notified_when_the_floating_surface_is_hidden() {
    let (mut tab, plugin_receiver) = tab_with_floating_plugin_pane(1);
    drain_visible_events(&plugin_receiver);

    tab.hide_floating_panes();

    assert_eq!(
        drain_visible_events(&plugin_receiver),
        vec![(Some(1), false)],
        "a plugin in a floating pane should be sent Event::Visible(false) when the floating surface is hidden"
    );
}

#[test]
fn floating_plugin_panes_are_notified_when_the_floating_surface_is_shown() {
    let (mut tab, plugin_receiver) = tab_with_floating_plugin_pane(1);
    tab.hide_floating_panes();
    drain_visible_events(&plugin_receiver);

    tab.show_floating_panes();

    assert_eq!(
        drain_visible_events(&plugin_receiver),
        vec![(Some(1), true)],
        "a plugin in a floating pane should be sent Event::Visible(true) when the floating surface is shown"
    );
}

#[test]
fn toggling_the_floating_surface_twice_does_not_repeat_the_visibility_event() {
    let (mut tab, plugin_receiver) = tab_with_floating_plugin_pane(1);
    drain_visible_events(&plugin_receiver);

    tab.hide_floating_panes();
    tab.hide_floating_panes();
    tab.show_floating_panes();
    tab.show_floating_panes();

    assert_eq!(
        drain_visible_events(&plugin_receiver),
        vec![(Some(1), false), (Some(1), true)],
        "only actual visibility transitions of the floating surface should be reported"
    );
}

#[test]
fn floating_surface_toggles_in_a_hidden_tab_do_not_notify_plugins() {
    let (mut tab, plugin_receiver) = tab_with_floating_plugin_pane(1);
    tab.visible(false).unwrap();
    drain_visible_events(&plugin_receiver);

    tab.hide_floating_panes();
    tab.show_floating_panes();

    assert!(
        drain_visible_events(&plugin_receiver).is_empty(),
        "a plugin in a hidden tab should not be told it became visible because the floating surface was toggled"
    );
}

#[test]
fn floating_plugin_panes_are_not_shown_again_when_their_tab_returns_with_the_surface_hidden() {
    let (mut tab, plugin_receiver) = tab_with_floating_plugin_pane(1);
    tab.hide_floating_panes();
    tab.visible(false).unwrap();
    drain_visible_events(&plugin_receiver);

    tab.visible(true).unwrap();

    assert!(
        !drain_visible_events(&plugin_receiver).contains(&(Some(1), true)),
        "a plugin whose floating surface is hidden should not be told it is visible when its tab returns"
    );
}

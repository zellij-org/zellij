use super::{Pane, PaneNotCreatedReason, Tab};
use crate::background_jobs::BackgroundJob;
use crate::pane_groups::PaneGroups;
use crate::panes::kitty_graphics::KittyImageStore;
use crate::panes::sixel::SixelImageStore;
use crate::plugins::PluginInstruction;
use crate::pty::PtyInstruction;
use crate::pty_writer::PtyWriteInstruction;
use crate::route::{ActionCompletionResult, NotificationEnd};
use crate::screen::CopyOptions;
use crate::screen::ScreenInstruction;
use crate::ServerInstruction;
use crate::{os_input_output::ServerOsApi, panes::PaneId, thread_bus::ThreadSenders, ClientId};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use tokio::sync::oneshot;
use zellij_utils::channels::{unbounded, ChannelWithContext, Receiver, SenderWithContext};
use zellij_utils::data::{Direction, Event, NewPanePlacement, Resize, ResizeStrategy, WebSharing};
use zellij_utils::errors::prelude::*;
use zellij_utils::errors::ErrorContext;
use zellij_utils::input::layout::{SplitDirection, SplitSize, TiledPaneLayout};
use zellij_utils::input::options::PaneFrameStyle;
use zellij_utils::ipc::IpcReceiverWithContext;
use zellij_utils::pane_size::{PaneGeom, Size, SizeInPixels};
use zellij_utils::position::Position;

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
        _pane_env: &crate::os_input_output::PaneEnv,
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
    fn register_client(
        &mut self,
        _client_id: ClientId,
        _receiver: &IpcReceiverWithContext<ClientToServerMsg>,
    ) -> Result<()> {
        unimplemented!()
    }
    fn register_client_with_reply(
        &mut self,
        _client_id: ClientId,
        _reply_stream: LocalSocketStream,
    ) -> Result<()> {
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
        _pane_env: &crate::os_input_output::PaneEnv,
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
        Rc::new(RefCell::new(HashMap::new())),
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
        Rc::new(RefCell::new(HashMap::new())),
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
        Rc::new(RefCell::new(HashMap::new())),
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
pub fn removing_a_client_not_connected_to_the_tab_does_not_force_render() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    for pane in tab.tiled_panes.panes.values_mut() {
        pane.set_should_render(false);
    }
    tab.remove_client(99);
    assert!(
        tab.tiled_panes.panes.values().all(|p| !p.should_render()),
        "removing a client that never viewed this tab must not trigger a full re-render"
    );
}

#[test]
pub fn removing_a_client_not_connected_to_the_tab_clears_its_dimming() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.set_client_dimmed(99, true);
    for pane in tab.tiled_panes.panes.values_mut() {
        pane.set_should_render(false);
    }
    tab.remove_client(99);
    assert!(!tab.dimmed_clients.contains(&99));
    assert!(
        tab.tiled_panes.panes.values().all(|p| p.should_render()),
        "removing a dimmed client must re-render to clear the dimming"
    );
}

#[test]
pub fn removing_a_client_connected_to_the_tab_forces_render() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    for pane in tab.tiled_panes.panes.values_mut() {
        pane.set_should_render(false);
    }
    tab.remove_client(1);
    assert!(
        tab.tiled_panes.panes.values().all(|p| p.should_render()),
        "removing a client that was viewing this tab must re-render it for the remaining clients"
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
            border_style: None,
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
            border_style: None,
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

fn create_tab_with_small_fixed_top_left_pane() -> Tab {
    let size = Size {
        cols: 120,
        rows: 40,
    };
    let mut fixed_cols_child = TiledPaneLayout::default();
    fixed_cols_child.split_size = Some(SplitSize::Fixed(24));
    let mut top_row = TiledPaneLayout::default();
    top_row.split_size = Some(SplitSize::Fixed(7));
    top_row.children_split_direction = SplitDirection::Vertical;
    top_row.children = vec![fixed_cols_child, TiledPaneLayout::default()];
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Horizontal;
    initial_layout.children = vec![top_row, TiledPaneLayout::default()];
    let mut tab = create_new_tab_with_layout(size, initial_layout);
    tab.focus_pane_with_id(PaneId::Terminal(0), false, false, 1)
        .unwrap();
    tab
}

fn tiled_pane_is_stacked(tab: &Tab, pane_id: PaneId) -> bool {
    tab.tiled_panes
        .panes
        .get(&pane_id)
        .unwrap()
        .position_and_size()
        .is_stacked()
}

#[test]
pub fn new_pane_next_to_small_fixed_pane_keeps_it_unstacked() {
    let mut tab = create_tab_with_small_fixed_top_left_pane();
    tab.new_pane(
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
    assert!(
        tab.tiled_panes.panes.contains_key(&PaneId::Terminal(3)),
        "new pane was added"
    );
    assert!(
        !tiled_pane_is_stacked(&tab, PaneId::Terminal(0)),
        "small fixed pane is not left in a stack"
    );
    tab.close_pane(PaneId::Terminal(3), false, None);
    assert!(
        !tab.tiled_panes.panes.contains_key(&PaneId::Terminal(3)),
        "new pane was closed"
    );
    assert_eq!(tab.tiled_panes.panes.len(), 3, "original panes remain");
}

#[test]
pub fn failed_stacked_pane_on_small_fixed_pane_keeps_it_unstacked() {
    let mut tab = create_tab_with_small_fixed_top_left_pane();
    tab.new_pane(
        PaneId::Terminal(3),
        None,
        None,
        false,
        true,
        NewPanePlacement::Stacked {
            pane_id_to_stack_under: None,
            borderless: None,
            border_style: None,
        },
        Some(1),
        None,
    )
    .unwrap();
    assert!(
        !tiled_pane_is_stacked(&tab, PaneId::Terminal(0)),
        "small fixed pane is not left in a stack"
    );
    tab.close_pane(PaneId::Terminal(1), false, None);
    assert!(
        tab.tiled_panes.panes.contains_key(&PaneId::Terminal(0)),
        "small fixed pane remains"
    );
    assert!(
        !tab.tiled_panes.panes.contains_key(&PaneId::Terminal(1)),
        "neighbouring pane was closed"
    );
}

#[test]
pub fn moving_suppressed_pane_leaves_tiled_panes_in_place() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = true;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.replace_active_pane_with_editor_pane(PaneId::Terminal(3), 1)
        .unwrap();
    assert!(
        tab.suppressed_panes
            .values()
            .any(|(_, p)| p.pid() == PaneId::Terminal(2)),
        "pane is suppressed"
    );
    assert!(
        !tab.tiled_panes.panes.contains_key(&PaneId::Terminal(2)),
        "suppressed pane is not tiled"
    );
    let geoms_before: Vec<_> = tab
        .tiled_panes
        .panes
        .iter()
        .map(|(id, p)| (*id, p.position_and_size()))
        .collect();
    tab.move_pane(PaneId::Terminal(2));
    let geoms_after: Vec<_> = tab
        .tiled_panes
        .panes
        .iter()
        .map(|(id, p)| (*id, p.position_and_size()))
        .collect();
    assert_eq!(geoms_before, geoms_after, "tiled panes did not move");
}

#[test]
pub fn moving_pane_hidden_by_fullscreen_keeps_both_panes() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let stacked_resize = false;
    let mut tab = create_new_tab(size, stacked_resize);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab.toggle_active_pane_fullscreen(1);
    let search_backwards = false;
    tab.tiled_panes
        .move_pane(search_backwards, PaneId::Terminal(1));
    tab.tiled_panes
        .move_pane(!search_backwards, PaneId::Terminal(1));
    assert_eq!(tab.tiled_panes.panes.len(), 2, "both panes remain");
}

#[test]
pub fn closing_pane_next_to_stack_without_flexible_pane_removes_it() {
    let mut tab = create_tab_with_small_fixed_top_left_pane();
    tab.focus_pane_with_id(PaneId::Terminal(2), false, false, 1)
        .unwrap();
    tab.new_pane(
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
    {
        let small_fixed_pane = tab.tiled_panes.panes.get_mut(&PaneId::Terminal(0)).unwrap();
        let mut geom = small_fixed_pane.position_and_size();
        geom.stacked = Some(999);
        small_fixed_pane.set_geom(geom);
    }
    tab.close_pane(PaneId::Terminal(3), false, None);
    assert!(
        !tab.tiled_panes.panes.contains_key(&PaneId::Terminal(3)),
        "pane was closed"
    );
    assert_eq!(tab.tiled_panes.panes.len(), 3, "original panes remain");
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

#[test]
fn suppressed_plugin_panes_get_mode_updates_but_are_not_visible() {
    let (mut tab, plugin_receiver) = tab_with_floating_plugin_pane(1);
    tab.suppress_pane(PaneId::Plugin(1), Some(1));
    while plugin_receiver.try_recv().is_ok() {}

    tab.update_input_modes().unwrap();

    let mut got_mode_update = false;
    while let Ok((instruction, _)) = plugin_receiver.try_recv() {
        if let PluginInstruction::Update(updates) = instruction {
            for (pid, _client_id, event) in updates {
                if pid == Some(1) && matches!(event, Event::ModeUpdate(..)) {
                    got_mode_update = true;
                }
            }
        }
    }
    assert!(
        got_mode_update,
        "a suppressed plugin should still receive ModeUpdate so it can show itself again"
    );
    assert!(
        !tab.get_plugin_ids().contains(&1),
        "a suppressed plugin should not count as visible, since render targeting uses get_plugin_ids"
    );
    assert!(tab.get_plugin_ids_including_suppressed().contains(&1));
}

struct TabReceivers {
    pty: Receiver<(PtyInstruction, ErrorContext)>,
    plugin: Receiver<(PluginInstruction, ErrorContext)>,
    pty_writer: Receiver<(PtyWriteInstruction, ErrorContext)>,
    _screen: Receiver<(ScreenInstruction, ErrorContext)>,
    _server: Receiver<(ServerInstruction, ErrorContext)>,
    _background_jobs: Receiver<(BackgroundJob, ErrorContext)>,
}

fn create_new_tab_with_receivers(
    size: Size,
    stacked_pane_list: bool,
    should_silently_fail: bool,
) -> (Tab, TabReceivers) {
    let (mut tab, receivers) =
        create_tab_with_receivers_without_layout(size, stacked_pane_list, should_silently_fail);
    tab.apply_layout(
        TiledPaneLayout::default(),
        vec![],
        vec![(1, None)],
        vec![],
        HashMap::new(),
        1,
        None,
    )
    .unwrap();
    (tab, receivers)
}

fn create_tab_with_receivers_without_layout(
    size: Size,
    stacked_pane_list: bool,
    should_silently_fail: bool,
) -> (Tab, TabReceivers) {
    let (to_pty, pty): ChannelWithContext<PtyInstruction> = unbounded();
    let (to_plugin, plugin): ChannelWithContext<PluginInstruction> = unbounded();
    let (to_pty_writer, pty_writer): ChannelWithContext<PtyWriteInstruction> = unbounded();
    let (to_screen, screen): ChannelWithContext<ScreenInstruction> = unbounded();
    let (to_server, server): ChannelWithContext<ServerInstruction> = unbounded();
    let (to_background_jobs, background_jobs): ChannelWithContext<BackgroundJob> = unbounded();
    let senders = ThreadSenders {
        to_screen: Some(SenderWithContext::new(to_screen)),
        to_pty: Some(SenderWithContext::new(to_pty)),
        to_plugin: Some(SenderWithContext::new(to_plugin)),
        to_server: Some(SenderWithContext::new(to_server)),
        to_pty_writer: Some(SenderWithContext::new(to_pty_writer)),
        to_background_jobs: Some(SenderWithContext::new(to_background_jobs)),
        should_silently_fail,
    };
    let client_id = 1;
    let mut connected_clients = HashMap::new();
    connected_clients.insert(client_id, false);
    let tab = Tab::new(
        0,
        0,
        String::new(),
        size,
        Rc::new(RefCell::new(None)),
        Rc::new(RefCell::new(true)),
        Rc::new(RefCell::new(stacked_pane_list)),
        Rc::new(RefCell::new(SixelImageStore::default())),
        Rc::new(RefCell::new(KittyImageStore::default())),
        Box::new(FakeInputOutput {}),
        senders,
        None,
        Style::default(),
        ModeInfo::default(),
        PaneFrameStyle::Full,
        true,
        Rc::new(RefCell::new(connected_clients)),
        Rc::new(RefCell::new(HashMap::new())),
        true,
        Some(client_id),
        CopyOptions::default(),
        Rc::new(RefCell::new(Palette::default())),
        Rc::new(RefCell::new(HashMap::new())),
        (vec![], vec![]),
        PathBuf::from("my_default_shell"),
        false,
        true,
        true,
        true,
        false,
        None,
        false,
        WebSharing::Off,
        Rc::new(RefCell::new(PaneGroups::new(ThreadSenders::default()))),
        Rc::new(RefCell::new(HashMap::new())),
        true,
        true,
        true,
        true,
        false,
        false,
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        8080,
    );
    (
        tab,
        TabReceivers {
            pty,
            plugin,
            pty_writer,
            _screen: screen,
            _server: server,
            _background_jobs: background_jobs,
        },
    )
}

fn closed_pane_ids(receivers: &TabReceivers) -> Vec<PaneId> {
    receivers
        .pty
        .try_iter()
        .filter_map(|(instruction, _)| match instruction {
            PtyInstruction::ClosePane(pane_id, _)
            | PtyInstruction::ClosePaneThatWasNotCreated(pane_id, _) => Some(pane_id),
            _ => None,
        })
        .collect()
}

fn pane_closed_events(receivers: &TabReceivers) -> Vec<zellij_utils::data::PaneId> {
    receivers
        .plugin
        .try_iter()
        .flat_map(|(instruction, _)| match instruction {
            PluginInstruction::Update(events) => events
                .into_iter()
                .filter_map(|(_, _, event)| match event {
                    Event::PaneClosed(pane_id) => Some(pane_id),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect()
}

fn completion_and_receiver() -> (NotificationEnd, oneshot::Receiver<ActionCompletionResult>) {
    let (sender, receiver) = oneshot::channel();
    (NotificationEnd::new(sender), receiver)
}

fn assert_reported_as_not_created(
    mut receiver: oneshot::Receiver<ActionCompletionResult>,
    reason: PaneNotCreatedReason,
) {
    let result = receiver
        .try_recv()
        .expect("the caller is notified that the action ended");
    assert_eq!(
        result.exit_status,
        Some(1),
        "the action is reported as failed"
    );
    assert_eq!(
        result.affected_pane_id, None,
        "the id of the pane that was not created is not reported"
    );
    assert_eq!(result.error_message.as_deref(), Some(reason.message()));
}

fn new_tiled_pane_in_tab(tab: &mut Tab, id: u32) {
    tab.new_pane(
        PaneId::Terminal(id),
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

fn detached_terminal_pane(id: u32) -> Box<dyn Pane> {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut donor_tab = create_new_tab(size, true);
    new_tiled_pane_in_tab(&mut donor_tab, id);
    donor_tab
        .tiled_panes
        .extract_pane(PaneId::Terminal(id))
        .unwrap()
}

fn tab_with_three_panes() -> Tab {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    new_tiled_pane_in_tab(&mut tab, 2);
    new_tiled_pane_in_tab(&mut tab, 3);
    tab
}

fn focused_tiled_pane_exists(tab: &Tab, client_id: ClientId) -> bool {
    tab.tiled_panes
        .focused_pane_id(client_id)
        .map(|id| tab.tiled_panes.panes_contain(&id))
        .unwrap_or(false)
}

#[test]
pub fn hiding_a_focused_pane_in_a_stack_list_moves_absent_clients_to_the_visible_pane() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let (mut tab, _receivers) = create_new_tab_with_receivers(size, true, true);
    new_tiled_pane_in_tab(&mut tab, 2);
    assert_eq!(
        tab.tiled_panes.focused_pane_id(1),
        Some(PaneId::Terminal(2))
    );
    tab.remove_client(1);
    tab.new_pane(
        PaneId::Terminal(3),
        None,
        None,
        false,
        false,
        NewPanePlacement::Stacked {
            pane_id_to_stack_under: Some(zellij_utils::data::PaneId::Terminal(2)),
            borderless: None,
            border_style: None,
        },
        None,
        None,
    )
    .unwrap();
    assert!(
        !tab.tiled_panes.panes_contain(&PaneId::Terminal(2)),
        "the previously focused pane is now a hidden member of the stack list"
    );
    assert!(
        focused_tiled_pane_exists(&tab, 1),
        "the absent client's focus was moved to the visible member of the stack list"
    );
    tab.add_client(1, None).unwrap();
    tab.focus_pane_on_edge(Direction::Left, 1);
    assert!(focused_tiled_pane_exists(&tab, 1));
}

#[test]
pub fn returning_client_with_focus_on_a_removed_pane_is_moved_to_an_existing_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    new_tiled_pane_in_tab(&mut tab, 2);
    tab.remove_client(1);
    tab.tiled_panes.extract_pane(PaneId::Terminal(2));
    tab.add_client(1, None).unwrap();
    assert_eq!(
        tab.tiled_panes.focused_pane_id(1),
        Some(PaneId::Terminal(1))
    );
    tab.focus_pane_on_edge(Direction::Right, 1);
    assert!(focused_tiled_pane_exists(&tab, 1));
}

#[test]
pub fn new_client_is_not_focused_on_a_remembered_pane_that_was_closed() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    new_tiled_pane_in_tab(&mut tab, 2);
    tab.remove_client(1);
    tab.close_pane(PaneId::Terminal(2), false, None);
    tab.add_client(2, None).unwrap();
    assert_eq!(
        tab.tiled_panes.focused_pane_id(2),
        Some(PaneId::Terminal(1))
    );
}

#[test]
pub fn focus_commands_do_not_crash_when_only_floating_panes_are_selectable() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::Floating(None),
        Some(1),
        None,
    )
    .unwrap();
    tab.hide_floating_panes();
    tab.set_pane_selectable(PaneId::Terminal(1), false);
    assert_eq!(tab.tiled_panes.focused_pane_id(1), None);
    tab.focus_pane_on_edge(Direction::Left, 1);
    tab.focus_next_pane(1);
    tab.focus_previous_pane(1);
    tab.focus_last_pane(1);
    tab.move_active_pane(1);
    tab.move_active_pane_backwards(1);
    assert_eq!(tab.tiled_panes.focused_pane_id(1), None);
}

#[test]
pub fn replacing_a_missing_pane_does_not_point_clients_at_the_replacement() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    new_tiled_pane_in_tab(&mut tab, 2);
    tab.tiled_panes.extract_pane(PaneId::Terminal(2));
    let unplaced = tab
        .tiled_panes
        .replace_pane(PaneId::Terminal(2), detached_terminal_pane(3))
        .err()
        .expect("the replacement pane is handed back");
    assert_eq!(unplaced.pid(), PaneId::Terminal(3));
    assert!(!tab.tiled_panes.panes_contain(&PaneId::Terminal(3)));
    assert_ne!(
        tab.tiled_panes.focused_pane_id(1),
        Some(PaneId::Terminal(3)),
        "clients are not moved to a pane that was never inserted"
    );
}

#[test]
pub fn failing_to_focus_a_pane_does_not_change_the_last_focused_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    new_tiled_pane_in_tab(&mut tab, 2);
    assert_eq!(
        tab.tiled_panes.get_last_pane_id(1),
        Some(PaneId::Terminal(1))
    );
    tab.tiled_panes.focus_pane(PaneId::Terminal(99), 1);
    assert_eq!(
        tab.tiled_panes.focused_pane_id(1),
        Some(PaneId::Terminal(2))
    );
    assert_eq!(
        tab.tiled_panes.get_last_pane_id(1),
        Some(PaneId::Terminal(1))
    );
}

#[test]
pub fn focus_commands_do_not_crash_when_client_has_no_focused_tiled_pane() {
    let mut tab = tab_with_three_panes();
    tab.tiled_panes.clear_active_panes();
    tab.focus_next_pane(1);
    tab.focus_previous_pane(1);
    tab.focus_last_pane(1);
    tab.move_active_pane(1);
    tab.move_active_pane_backwards(1);
    assert!(!tab.move_focus_left(1).unwrap());
    assert!(!tab.move_focus_right(1).unwrap());
    assert!(!tab.move_focus_up(1).unwrap());
    assert!(!tab.move_focus_down(1).unwrap());
    tab.focus_pane_on_edge(Direction::Left, 1);
    assert!(focused_tiled_pane_exists(&tab, 1));
}

#[test]
pub fn focus_on_edge_with_a_removed_focused_pane_still_focuses_the_edge_pane() {
    let mut tab = tab_with_three_panes();
    let focused = tab.tiled_panes.focused_pane_id(1).unwrap();
    tab.tiled_panes.extract_pane(focused);
    tab.tiled_panes.focus_pane_on_edge(Direction::Left, 1);
    assert!(focused_tiled_pane_exists(&tab, 1));
}

#[test]
pub fn focus_last_pane_with_a_removed_focused_pane_still_focuses_the_last_pane() {
    let mut tab = tab_with_three_panes();
    let last = tab.tiled_panes.get_last_pane_id(1).unwrap();
    let focused = tab.tiled_panes.focused_pane_id(1).unwrap();
    tab.tiled_panes.extract_pane(focused);
    tab.tiled_panes.focus_last_pane(1);
    assert_eq!(tab.tiled_panes.focused_pane_id(1), Some(last));
}

#[test]
pub fn moving_focus_with_a_removed_focused_pane_does_not_crash() {
    let mut tab = tab_with_three_panes();
    let focused = tab.tiled_panes.focused_pane_id(1).unwrap();
    tab.tiled_panes.extract_pane(focused);
    tab.tiled_panes.move_focus_left(1);
    tab.tiled_panes.move_focus_right(1);
    tab.tiled_panes.move_focus_up(1);
    tab.tiled_panes.move_focus_down(1);
    tab.tiled_panes.focus_next_pane(1);
    tab.tiled_panes.focus_previous_pane(1);
}

#[test]
pub fn moving_a_missing_pane_does_not_crash_or_change_the_layout() {
    let mut tab = tab_with_three_panes();
    let geoms_before: Vec<PaneGeom> = tab
        .tiled_panes
        .panes
        .values()
        .map(|p| p.position_and_size())
        .collect();
    let missing = PaneId::Terminal(99);
    tab.move_pane(missing);
    tab.move_pane_down(missing);
    tab.move_pane_up(missing);
    tab.move_pane_left(missing);
    tab.move_pane_right(missing);
    tab.tiled_panes.switch_active_pane_with(missing);
    let geoms_after: Vec<PaneGeom> = tab
        .tiled_panes
        .panes
        .values()
        .map(|p| p.position_and_size())
        .collect();
    assert_eq!(geoms_before, geoms_after);
}

#[test]
pub fn moving_panes_when_resize_messages_cannot_be_sent_still_swaps_them() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, false);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    let TabReceivers {
        pty_writer, plugin, ..
    } = receivers;
    drop(pty_writer);
    drop(plugin);
    let left_geom_before = tab
        .tiled_panes
        .get_pane(PaneId::Terminal(1))
        .unwrap()
        .position_and_size();
    tab.move_pane_right(PaneId::Terminal(1));
    assert_ne!(
        tab.tiled_panes
            .get_pane(PaneId::Terminal(1))
            .unwrap()
            .position_and_size(),
        left_geom_before,
        "the pane was moved even though its pty could not be told about the new size"
    );
    tab.move_pane_left(PaneId::Terminal(1));
    tab.move_pane(PaneId::Terminal(1));
    tab.tiled_panes.switch_active_pane_with(PaneId::Terminal(2));
    assert_eq!(tab.tiled_panes.panes.len(), 2);
}

#[test]
pub fn stacked_moves_when_resize_messages_cannot_be_sent_do_not_crash() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, false);
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::Stacked {
            pane_id_to_stack_under: None,
            borderless: None,
            border_style: None,
        },
        Some(1),
        None,
    )
    .unwrap();
    let TabReceivers {
        pty_writer, plugin, ..
    } = receivers;
    drop(pty_writer);
    drop(plugin);
    tab.move_pane_up(PaneId::Terminal(2));
    tab.move_pane_down(PaneId::Terminal(1));
    tab.resize(
        1,
        ResizeStrategy::new(Resize::Decrease, Some(Direction::Up)),
    )
    .non_fatal();
    tab.resize(
        1,
        ResizeStrategy::new(Resize::Increase, Some(Direction::Down)),
    )
    .non_fatal();
    assert_eq!(tab.tiled_panes.panes.len(), 2);
}

#[test]
pub fn leaving_fullscreen_after_a_hidden_pane_was_replaced_does_not_crash() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    new_tiled_pane_in_tab(&mut tab, 2);
    tab.focus_pane_with_id(PaneId::Terminal(1), false, false, 1)
        .unwrap();
    tab.toggle_active_pane_fullscreen(1);
    assert!(tab.tiled_panes.fullscreen_is_active());
    assert!(tab.tiled_panes.panes_to_hide_contains(PaneId::Terminal(2)));
    tab.suppress_pane_and_replace_with_other_pane(
        PaneId::Terminal(2),
        detached_terminal_pane(3),
        None,
    );
    assert!(
        tab.tiled_panes.panes_to_hide_contains(PaneId::Terminal(3)),
        "the replacement pane stays hidden behind the fullscreen pane"
    );
    assert!(!tab.tiled_panes.panes_to_hide_contains(PaneId::Terminal(2)));
    tab.toggle_active_pane_fullscreen(1);
    assert!(!tab.tiled_panes.fullscreen_is_active());
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(3)));
}

#[test]
pub fn new_pane_without_room_is_closed_and_reported_as_failed() {
    let size = Size { cols: 8, rows: 4 };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    let (completion, completion_receiver) = completion_and_receiver();
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::default(),
        Some(1),
        Some(completion),
    )
    .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(2)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(2)]);
    assert_eq!(
        pane_closed_events(&receivers),
        vec![zellij_utils::data::PaneId::Terminal(2)]
    );
    assert_reported_as_not_created(completion_receiver, PaneNotCreatedReason::NoRoom);
    assert_eq!(
        tab.tiled_panes.get_last_pane_id(1),
        None,
        "the last focused pane is not changed by the failed pane"
    );
}

#[test]
pub fn new_floating_pane_without_room_is_closed() {
    let size = Size { cols: 8, rows: 4 };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    let (completion, completion_receiver) = completion_and_receiver();
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::Floating(None),
        Some(1),
        Some(completion),
    )
    .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(2)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(2)]);
    assert_reported_as_not_created(completion_receiver, PaneNotCreatedReason::NoRoom);
}

#[test]
pub fn split_without_room_is_closed_and_reported_as_failed() {
    let size = Size { cols: 121, rows: 4 };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    let (completion, completion_receiver) = completion_and_receiver();
    tab.horizontal_split(PaneId::Terminal(2), None, 1, Some(completion), None)
        .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(2)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(2)]);
    assert_reported_as_not_created(completion_receiver, PaneNotCreatedReason::NoRoom);
}

#[test]
pub fn split_while_floating_panes_are_visible_is_closed() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::Floating(None),
        Some(1),
        None,
    )
    .unwrap();
    let (completion, completion_receiver) = completion_and_receiver();
    tab.vertical_split(PaneId::Terminal(3), None, 1, Some(completion), None)
        .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(3)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(3)]);
    assert_reported_as_not_created(
        completion_receiver,
        PaneNotCreatedReason::FloatingPanesVisible,
    );
}

#[test]
pub fn splitting_with_a_plugin_pane_closes_the_plugin() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    tab.vertical_split(PaneId::Plugin(5), None, 1, None, None)
        .unwrap();
    tab.horizontal_split_of_pane_id(
        PaneId::Plugin(6),
        None,
        None,
        PaneId::Terminal(1),
        None,
        None,
    )
    .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Plugin(5)));
    assert!(!tab.has_pane_with_pid(&PaneId::Plugin(6)));
    assert_eq!(
        closed_pane_ids(&receivers),
        vec![PaneId::Plugin(5), PaneId::Plugin(6)]
    );
}

#[test]
pub fn splitting_a_missing_pane_closes_the_new_pane() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    let (completion, completion_receiver) = completion_and_receiver();
    tab.vertical_split_of_pane_id(
        PaneId::Terminal(2),
        None,
        None,
        PaneId::Terminal(99),
        Some(completion),
        None,
    )
    .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(2)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(2)]);
    assert_reported_as_not_created(
        completion_receiver,
        PaneNotCreatedReason::TargetPaneNotFound,
    );
}

#[test]
pub fn splitting_a_pane_without_room_closes_the_new_pane() {
    let size = Size { cols: 8, rows: 4 };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    let (completion, completion_receiver) = completion_and_receiver();
    tab.horizontal_split_of_pane_id(
        PaneId::Terminal(2),
        None,
        None,
        PaneId::Terminal(1),
        Some(completion),
        None,
    )
    .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(2)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(2)]);
    assert_reported_as_not_created(completion_receiver, PaneNotCreatedReason::NoRoom);
}

#[test]
pub fn stacked_pane_without_a_target_is_closed() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    let (completion, completion_receiver) = completion_and_receiver();
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::Stacked {
            pane_id_to_stack_under: None,
            borderless: None,
            border_style: None,
        },
        None,
        Some(completion),
    )
    .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(2)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(2)]);
    assert_reported_as_not_created(
        completion_receiver,
        PaneNotCreatedReason::NoClientOrTargetPane,
    );
}

#[test]
pub fn stacked_pane_without_room_is_closed() {
    let size = Size { cols: 8, rows: 4 };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    let (completion, completion_receiver) = completion_and_receiver();
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::Stacked {
            pane_id_to_stack_under: Some(zellij_utils::data::PaneId::Terminal(1)),
            borderless: None,
            border_style: None,
        },
        Some(1),
        Some(completion),
    )
    .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(2)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(2)]);
    assert_reported_as_not_created(completion_receiver, PaneNotCreatedReason::NoRoom);
    let (completion, completion_receiver) = completion_and_receiver();
    tab.new_pane(
        PaneId::Terminal(3),
        None,
        None,
        false,
        true,
        NewPanePlacement::Stacked {
            pane_id_to_stack_under: None,
            borderless: None,
            border_style: None,
        },
        Some(1),
        Some(completion),
    )
    .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(3)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(3)]);
    assert_reported_as_not_created(completion_receiver, PaneNotCreatedReason::NoRoom);
}

#[test]
pub fn tiled_panes_hand_back_panes_they_could_not_place() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    let missing = PaneId::Terminal(99);
    assert!(tab
        .tiled_panes
        .split_pane_id_horizontally(PaneId::Terminal(2), detached_terminal_pane(2), missing)
        .is_some());
    assert!(tab
        .tiled_panes
        .split_pane_id_vertically(PaneId::Terminal(2), detached_terminal_pane(2), missing)
        .is_some());
    assert!(tab
        .tiled_panes
        .add_pane_to_stack_of_pane_id(PaneId::Terminal(2), detached_terminal_pane(2), missing)
        .is_some());
    tab.tiled_panes.clear_active_panes();
    assert!(tab
        .tiled_panes
        .split_pane_horizontally(PaneId::Terminal(2), detached_terminal_pane(2), 1)
        .is_some());
    assert!(tab
        .tiled_panes
        .split_pane_vertically(PaneId::Terminal(2), detached_terminal_pane(2), 1)
        .is_some());
    assert!(tab
        .tiled_panes
        .add_pane_to_stack_of_active_pane(PaneId::Terminal(2), detached_terminal_pane(2), 1)
        .is_some());
    assert!(!tab.tiled_panes.panes_contain(&PaneId::Terminal(2)));

    let tiny = Size { cols: 8, rows: 4 };
    let mut tiny_tab = create_new_tab(tiny, true);
    assert!(tiny_tab
        .tiled_panes
        .insert_pane(PaneId::Terminal(2), detached_terminal_pane(2), Some(1))
        .is_some());
    assert!(tiny_tab
        .tiled_panes
        .insert_pane_without_relayout(PaneId::Terminal(2), detached_terminal_pane(2), None)
        .is_some());
    assert!(tiny_tab
        .tiled_panes
        .add_pane_to_stack(&PaneId::Terminal(1), detached_terminal_pane(2))
        .is_some());
}

fn new_suppressed_pane_in_tab(tab: &mut Tab, id: u32) {
    tab.new_pane(
        PaneId::Terminal(id),
        None,
        None,
        true,
        false,
        NewPanePlacement::default(),
        Some(1),
        None,
    )
    .unwrap();
}

#[test]
pub fn existing_pane_without_tiled_room_is_floated_instead_of_dropped() {
    let size = Size { cols: 10, rows: 10 };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    new_suppressed_pane_in_tab(&mut tab, 2);
    tab.unsuppress_pane(PaneId::Terminal(2), false);
    assert!(tab.floating_panes.panes_contain(&PaneId::Terminal(2)));
    assert!(closed_pane_ids(&receivers).is_empty());
}

#[test]
pub fn existing_pane_without_floating_room_is_tiled_instead_of_dropped() {
    let size = Size { cols: 121, rows: 9 };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    new_suppressed_pane_in_tab(&mut tab, 2);
    tab.unsuppress_pane(PaneId::Terminal(2), true);
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(2)));
    assert!(closed_pane_ids(&receivers).is_empty());
}

#[test]
pub fn existing_pane_without_any_room_stays_suppressed_instead_of_dropped() {
    let size = Size { cols: 8, rows: 4 };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    new_suppressed_pane_in_tab(&mut tab, 2);
    tab.unsuppress_pane(PaneId::Terminal(2), false);
    assert!(tab.has_pane_with_pid(&PaneId::Terminal(2)));
    assert!(!tab.has_non_suppressed_pane_with_pid(&PaneId::Terminal(2)));
    tab.unsuppress_pane(PaneId::Terminal(2), true);
    assert!(tab.has_pane_with_pid(&PaneId::Terminal(2)));
    assert!(closed_pane_ids(&receivers).is_empty());
}

#[test]
pub fn stacking_onto_a_missing_pane_keeps_the_panes() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    tab.stack_panes(PaneId::Terminal(99), vec![detached_terminal_pane(2)]);
    assert!(tab.has_non_suppressed_pane_with_pid(&PaneId::Terminal(2)));
}

#[test]
pub fn stacking_onto_a_full_stack_keeps_the_panes() {
    let size = Size { cols: 121, rows: 9 };
    let mut tab = create_new_tab(size, true);
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::Stacked {
            pane_id_to_stack_under: Some(zellij_utils::data::PaneId::Terminal(1)),
            borderless: None,
            border_style: None,
        },
        Some(1),
        None,
    )
    .unwrap();
    for id in 3..12 {
        tab.stack_panes(PaneId::Terminal(1), vec![detached_terminal_pane(id)]);
    }
    for id in 2..12 {
        assert!(
            tab.has_pane_with_pid(&PaneId::Terminal(id)),
            "pane {} was kept",
            id
        );
    }
}

#[test]
pub fn replacing_missing_panes_hands_the_replacement_back() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    let missing = PaneId::Terminal(99);
    let unplaced = tab
        .floating_panes
        .replace_pane(missing, detached_terminal_pane(2))
        .err()
        .expect("the replacement pane is handed back");
    assert_eq!(unplaced.pid(), PaneId::Terminal(2));
    let unplaced = tab
        .floating_panes
        .replace_active_pane(detached_terminal_pane(2), 1)
        .err()
        .expect("the replacement pane is handed back");
    assert_eq!(unplaced.pid(), PaneId::Terminal(2));
    tab.tiled_panes.clear_active_panes();
    let unplaced = tab
        .tiled_panes
        .replace_active_pane(detached_terminal_pane(2), 1)
        .err()
        .expect("the replacement pane is handed back");
    assert_eq!(unplaced.pid(), PaneId::Terminal(2));
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(2)));
}

#[test]
pub fn replacing_a_floating_pane_keeps_its_stacking_order() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::Floating(None),
        Some(1),
        None,
    )
    .unwrap();
    let removed = tab
        .floating_panes
        .replace_pane(PaneId::Terminal(2), detached_terminal_pane(3))
        .ok()
        .expect("the floating pane is replaced");
    assert_eq!(removed.pid(), PaneId::Terminal(2));
    assert!(tab.floating_panes.panes_contain(&PaneId::Terminal(3)));
    assert_eq!(
        tab.floating_panes.active_pane_id(1),
        Some(PaneId::Terminal(3))
    );
}

#[test]
pub fn editor_pane_that_cannot_replace_its_target_is_closed() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    tab.replace_pane_with_editor_pane(PaneId::Terminal(5), PaneId::Terminal(99))
        .unwrap();
    new_suppressed_pane_in_tab(&mut tab, 2);
    tab.replace_pane_with_editor_pane(PaneId::Terminal(6), PaneId::Terminal(2))
        .unwrap();
    tab.replace_pane_with_editor_pane(PaneId::Plugin(7), PaneId::Terminal(1))
        .unwrap();
    tab.tiled_panes.clear_active_panes();
    tab.replace_active_pane_with_editor_pane(PaneId::Terminal(8), 1)
        .unwrap();
    for id in [
        PaneId::Terminal(5),
        PaneId::Terminal(6),
        PaneId::Plugin(7),
        PaneId::Terminal(8),
    ] {
        assert!(!tab.has_pane_with_pid(&id));
    }
    assert_eq!(
        closed_pane_ids(&receivers),
        vec![
            PaneId::Terminal(5),
            PaneId::Terminal(6),
            PaneId::Plugin(7),
            PaneId::Terminal(8)
        ]
    );
    assert!(tab.has_pane_with_pid(&PaneId::Terminal(1)));
    assert!(tab.has_pane_with_pid(&PaneId::Terminal(2)));
}

#[test]
pub fn in_place_pane_that_cannot_replace_its_target_is_closed_and_reported_as_failed() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    let (completion, completion_receiver) = completion_and_receiver();
    tab.suppress_pane_and_replace_with_pid(
        PaneId::Terminal(99),
        PaneId::Terminal(5),
        false,
        None,
        Some(completion),
        None,
    )
    .unwrap();
    tab.suppress_pane_and_replace_with_pid(
        PaneId::Terminal(99),
        PaneId::Plugin(6),
        false,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(5)));
    assert!(!tab.has_pane_with_pid(&PaneId::Plugin(6)));
    assert_eq!(
        closed_pane_ids(&receivers),
        vec![PaneId::Terminal(5), PaneId::Plugin(6)]
    );
    assert_reported_as_not_created(
        completion_receiver,
        PaneNotCreatedReason::PaneToReplaceNotFound,
    );
}

#[test]
pub fn in_place_plugin_that_closes_the_replaced_pane_closes_its_process() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    tab.suppress_pane_and_replace_with_pid(
        PaneId::Terminal(1),
        PaneId::Plugin(6),
        true,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(tab.has_pane_with_pid(&PaneId::Plugin(6)));
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(1)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(1)]);
}

#[test]
pub fn existing_pane_that_cannot_replace_its_target_is_handed_back() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    let unplaced = tab
        .suppress_pane_and_replace_with_other_pane(
            PaneId::Terminal(99),
            detached_terminal_pane(2),
            None,
        )
        .expect("the pane is handed back");
    assert_eq!(unplaced.pid(), PaneId::Terminal(2));
    let unplaced = tab
        .close_pane_and_replace_with_other_pane(
            PaneId::Terminal(99),
            detached_terminal_pane(3),
            None,
        )
        .expect("the pane is handed back");
    assert_eq!(unplaced.pid(), PaneId::Terminal(3));
    assert!(closed_pane_ids(&receivers).is_empty());
}

#[test]
pub fn closing_an_editor_restores_the_tiled_pane_while_floating_panes_are_visible() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    tab.replace_pane_with_editor_pane(PaneId::Terminal(5), PaneId::Terminal(1))
        .unwrap();
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(5)));
    tab.new_pane(
        PaneId::Terminal(2),
        None,
        None,
        false,
        true,
        NewPanePlacement::Floating(None),
        Some(1),
        None,
    )
    .unwrap();
    assert!(tab.are_floating_panes_visible());
    tab.close_pane(PaneId::Terminal(5), false, None);
    assert!(
        tab.tiled_panes.panes_contain(&PaneId::Terminal(1)),
        "the original pane is restored in place of the editor"
    );
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(5)));
}

#[test]
pub fn restoring_a_suppressed_pane_whose_replacer_is_gone_places_it_in_the_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    new_tiled_pane_in_tab(&mut tab, 2);
    tab.replace_pane_with_editor_pane(PaneId::Terminal(5), PaneId::Terminal(2))
        .unwrap();
    tab.tiled_panes.extract_pane(PaneId::Terminal(5));
    tab.close_pane(PaneId::Terminal(5), false, None);
    assert!(
        tab.has_non_suppressed_pane_with_pid(&PaneId::Terminal(2)),
        "the original pane is not lost"
    );
}

#[test]
pub fn closing_a_self_suppressed_pane_removes_it() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    new_suppressed_pane_in_tab(&mut tab, 2);
    tab.close_pane(PaneId::Terminal(2), false, None);
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(2)));
}

fn new_floating_pane_in_tab(tab: &mut Tab, id: u32) {
    tab.new_pane(
        PaneId::Terminal(id),
        None,
        None,
        false,
        true,
        NewPanePlacement::Floating(None),
        Some(1),
        None,
    )
    .unwrap();
}

fn tab_with_two_floating_panes() -> Tab {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let mut tab = create_new_tab(size, false);
    new_floating_pane_in_tab(&mut tab, 2);
    new_floating_pane_in_tab(&mut tab, 3);
    tab
}

fn leave_only_floating_pane(tab: &mut Tab, pane_id_to_keep: PaneId) {
    let mut drained = tab.floating_panes.drain();
    let pane = drained.remove(&pane_id_to_keep).unwrap();
    tab.floating_panes.add_pane(pane_id_to_keep, pane);
}

fn floating_focus_is_valid(tab: &Tab, client_id: ClientId) -> bool {
    tab.floating_panes
        .active_pane_id(client_id)
        .map(|id| tab.floating_panes.panes_contain(&id))
        .unwrap_or(false)
}

#[test]
pub fn a_pane_that_fails_to_render_does_not_fail_the_tab_render() {
    let mut tab = tab_with_two_floating_panes();
    let failing_floating = Box::new(super::test_panes::FailingRenderPane {
        inner: detached_terminal_pane(5),
    });
    tab.add_floating_pane(failing_floating, PaneId::Terminal(5), None, true, Some(1))
        .unwrap();
    let failing_tiled = Box::new(super::test_panes::FailingRenderPane {
        inner: detached_terminal_pane(6),
    });
    tab.add_tiled_pane(failing_tiled, PaneId::Terminal(6), false, None)
        .unwrap();
    assert!(tab.floating_panes.panes_contain(&PaneId::Terminal(5)));
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(6)));
    let mut output = crate::output::Output::default();
    assert!(tab.render(&mut output, None).is_ok());
    let mut tiled_output = crate::output::Output::default();
    tiled_output.add_clients(&[1].into_iter().collect(), tab.link_handler.clone(), None);
    let floating_panes_are_visible = false;
    assert!(tab
        .tiled_panes
        .render(
            &mut tiled_output,
            floating_panes_are_visible,
            &HashMap::new(),
            HashMap::new(),
            None,
            &HashMap::new(),
            false,
            false,
        )
        .is_ok());
    let mut floating_output = crate::output::Output::default();
    floating_output.add_clients(&[1].into_iter().collect(), tab.link_handler.clone(), None);
    assert!(tab
        .floating_panes
        .render(
            &mut floating_output,
            &HashMap::new(),
            HashMap::new(),
            None,
            &HashMap::new(),
            false,
            false,
        )
        .is_ok());
}

#[test]
pub fn failing_resize_messages_do_not_fail_resizing_all_panes() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, false);
    new_floating_pane_in_tab(&mut tab, 2);
    new_floating_pane_in_tab(&mut tab, 3);
    let TabReceivers {
        pty_writer, plugin, ..
    } = receivers;
    drop(pty_writer);
    drop(plugin);
    assert!(tab.resize_pty_all_panes().is_ok());
    assert!(tab
        .resize_whole_tab(Size {
            cols: 100,
            rows: 30
        })
        .is_ok());
    assert_eq!(tab.floating_panes.visible_panes_count(), 2);
    assert!(tab
        .floating_panes
        .resize_pane_with_id(
            ResizeStrategy::new(Resize::Increase, None),
            PaneId::Terminal(2)
        )
        .is_ok());
}

#[test]
pub fn resizing_with_a_removed_focused_floating_pane_does_nothing() {
    let mut tab = tab_with_two_floating_panes();
    assert_eq!(
        tab.floating_panes.active_pane_id(1),
        Some(PaneId::Terminal(3))
    );
    leave_only_floating_pane(&mut tab, PaneId::Terminal(2));
    let geom_before = tab
        .floating_panes
        .get_pane(PaneId::Terminal(2))
        .unwrap()
        .position_and_size();
    assert!(tab
        .resize(1, ResizeStrategy::new(Resize::Increase, None))
        .is_ok());
    assert_eq!(
        tab.floating_panes
            .get_pane(PaneId::Terminal(2))
            .unwrap()
            .position_and_size(),
        geom_before
    );
}

#[test]
pub fn removing_a_floating_pane_clears_fullscreen_drag_and_focus_on_it() {
    let mut tab = tab_with_two_floating_panes();
    tab.floating_panes
        .toggle_pane_fullscreen(PaneId::Terminal(3));
    assert_eq!(
        tab.floating_panes.fullscreen_pane_id(),
        Some(PaneId::Terminal(3))
    );
    tab.floating_panes.set_pane_being_moved_with_mouse(
        PaneId::Terminal(3),
        zellij_utils::position::Position::new(10, 10),
    );
    let removed = tab.floating_panes.remove_pane(PaneId::Terminal(3));
    assert!(removed.is_some());
    assert_eq!(tab.floating_panes.fullscreen_pane_id(), None);
    assert!(!tab.floating_panes.pane_is_being_moved_with_mouse());
    assert_eq!(
        tab.floating_panes.active_pane_id(1),
        Some(PaneId::Terminal(2))
    );
}

#[test]
pub fn closing_a_floating_pane_being_dragged_stops_the_drag() {
    let mut tab = tab_with_two_floating_panes();
    tab.floating_panes.set_pane_being_moved_with_mouse(
        PaneId::Terminal(3),
        zellij_utils::position::Position::new(10, 10),
    );
    tab.close_pane(PaneId::Terminal(3), false, None);
    assert!(!tab.floating_panes.pane_is_being_moved_with_mouse());
}

#[test]
pub fn focusing_a_missing_floating_pane_for_all_clients_changes_nothing() {
    let mut tab = tab_with_two_floating_panes();
    let missing = PaneId::Terminal(99);
    tab.floating_panes.focus_pane_for_all_clients(missing);
    assert_eq!(
        tab.floating_panes.active_pane_id(1),
        Some(PaneId::Terminal(3))
    );
    assert_eq!(tab.floating_panes.get_pane_z_index(missing), None);
}

#[test]
pub fn failing_to_focus_a_floating_pane_does_not_change_the_last_focused_pane() {
    let mut tab = tab_with_two_floating_panes();
    tab.floating_panes.focus_pane(PaneId::Terminal(2), 1);
    tab.floating_panes.focus_pane(PaneId::Terminal(3), 1);
    assert_eq!(
        tab.floating_panes.get_last_pane_id(1),
        Some(PaneId::Terminal(2))
    );
    tab.floating_panes.focus_pane(PaneId::Terminal(99), 1);
    assert_eq!(
        tab.floating_panes.active_pane_id(1),
        Some(PaneId::Terminal(3))
    );
    assert_eq!(
        tab.floating_panes.get_last_pane_id(1),
        Some(PaneId::Terminal(2))
    );
}

#[test]
pub fn moving_focus_with_a_removed_focused_floating_pane_focuses_an_existing_pane() {
    let mut tab = tab_with_two_floating_panes();
    leave_only_floating_pane(&mut tab, PaneId::Terminal(2));
    assert!(!floating_focus_is_valid(&tab, 1));
    tab.move_focus_left(1).unwrap();
    assert!(floating_focus_is_valid(&tab, 1));
    assert!(tab
        .floating_panes
        .get_pane_z_index(PaneId::Terminal(2))
        .is_some());
}

#[test]
pub fn moving_focus_without_a_focused_floating_pane_keeps_stacking_order() {
    let mut tab = tab_with_two_floating_panes();
    tab.floating_panes.defocus_pane(1);
    tab.move_focus_right(1).unwrap();
    assert!(floating_focus_is_valid(&tab, 1));
    assert!(tab
        .floating_panes
        .get_pane_z_index(PaneId::Terminal(2))
        .is_some());
    assert!(tab
        .floating_panes
        .get_pane_z_index(PaneId::Terminal(3))
        .is_some());
}

#[test]
pub fn focusing_the_edge_keeps_fullscreen_when_the_focused_pane_was_removed() {
    let mut tab = tab_with_two_floating_panes();
    tab.floating_panes
        .toggle_pane_fullscreen(PaneId::Terminal(2));
    tab.floating_panes.focus_pane(PaneId::Terminal(3), 1);
    leave_only_floating_pane(&mut tab, PaneId::Terminal(2));
    tab.floating_panes
        .get_pane_mut(PaneId::Terminal(2))
        .unwrap()
        .set_selectable(false);
    tab.floating_panes.focus_pane_on_edge(Direction::Left, 1);
    assert_eq!(
        tab.floating_panes.fullscreen_pane_id(),
        Some(PaneId::Terminal(2))
    );
}

#[test]
pub fn joining_client_does_not_move_the_focus_of_other_clients() {
    let mut tab = tab_with_two_floating_panes();
    tab.add_client(2, None).unwrap();
    tab.floating_panes.focus_pane(PaneId::Terminal(2), 2);
    tab.remove_client(2);
    tab.floating_panes.focus_pane(PaneId::Terminal(3), 1);
    assert_eq!(
        tab.floating_panes.active_pane_id(2),
        Some(PaneId::Terminal(2))
    );
    tab.add_client(2, None).unwrap();
    assert_eq!(
        tab.floating_panes.active_pane_id(1),
        Some(PaneId::Terminal(3)),
        "the client that was already here keeps its focus"
    );
    assert_eq!(
        tab.floating_panes.active_pane_id(2),
        Some(PaneId::Terminal(3)),
        "the joining client shares the focus of the clients already here"
    );
}

#[test]
pub fn joining_client_is_focused_on_a_selectable_floating_pane() {
    let mut tab = tab_with_two_floating_panes();
    tab.floating_panes
        .get_pane_mut(PaneId::Terminal(2))
        .unwrap()
        .set_selectable(false);
    tab.remove_client(1);
    tab.add_client(3, None).unwrap();
    assert_eq!(
        tab.floating_panes.active_pane_id(3),
        Some(PaneId::Terminal(3))
    );
}

#[test]
pub fn focus_of_disconnected_clients_does_not_count_as_floating_focus() {
    let mut tab = tab_with_two_floating_panes();
    assert!(tab.floating_panes.has_active_panes());
    tab.remove_client(1);
    assert!(!tab.floating_panes.has_active_panes());
    assert!(tab.floating_panes.has_active_panes_for(Some(1)));
}

#[test]
pub fn background_tab_reports_the_remembered_floating_focus() {
    let mut tab = tab_with_two_floating_panes();
    tab.remove_client(1);
    let focused: Vec<_> = tab
        .floating_panes
        .pane_info(&HashMap::new())
        .into_iter()
        .filter(|p| p.is_focused)
        .map(|p| p.id)
        .collect();
    assert_eq!(focused, vec![3]);
}

#[test]
pub fn moving_a_missing_floating_pane_reports_no_move() {
    let mut tab = tab_with_two_floating_panes();
    let missing = PaneId::Terminal(99);
    assert!(!tab.floating_panes.move_pane_left(missing));
    assert!(!tab.floating_panes.move_pane_right(missing));
    assert!(!tab.floating_panes.move_pane_up(missing));
    assert!(!tab.floating_panes.move_pane_down(missing));
    leave_only_floating_pane(&mut tab, PaneId::Terminal(2));
    assert!(!tab.floating_panes.move_active_pane_left(1));
    assert!(tab.floating_panes.move_pane_right(PaneId::Terminal(2)));
}

#[test]
pub fn toggling_fullscreen_with_a_removed_focused_floating_pane_does_nothing() {
    let mut tab = tab_with_two_floating_panes();
    leave_only_floating_pane(&mut tab, PaneId::Terminal(2));
    tab.toggle_active_pane_fullscreen(1);
    tab.toggle_active_pane_no_ui_fullscreen(1);
    assert!(!tab.floating_panes.fullscreen_is_active());
    assert!(!tab.tiled_panes.fullscreen_is_active());
}

#[test]
pub fn mouse_resize_of_a_closed_floating_pane_releases_the_mouse() {
    let mut tab = tab_with_two_floating_panes();
    let start_geom = tab
        .floating_panes
        .get_pane(PaneId::Terminal(3))
        .unwrap()
        .position_and_size();
    tab.pane_being_resized_with_mouse = Some(super::PaneResizeState {
        pane_id: PaneId::Terminal(3),
        edge: super::PaneEdge::Right,
        start_position: zellij_utils::position::Position::new(10, 10),
        start_geom,
        is_floating: true,
    });
    tab.close_pane(PaneId::Terminal(3), false, None);
    let motion = zellij_utils::input::mouse::MouseEvent::new_left_motion_event(
        zellij_utils::position::Position::new(12, 14),
    );
    assert!(tab.handle_mouse_event(&motion, 1).is_ok());
    assert!(tab.pane_being_resized_with_mouse.is_none());
    tab.pane_being_resized_with_mouse = Some(super::PaneResizeState {
        pane_id: PaneId::Terminal(99),
        edge: super::PaneEdge::Right,
        start_position: zellij_utils::position::Position::new(10, 10),
        start_geom,
        is_floating: true,
    });
    let release = zellij_utils::input::mouse::MouseEvent::new_left_release_event(
        zellij_utils::position::Position::new(12, 14),
    );
    assert!(tab.handle_mouse_event(&release, 1).is_ok());
    assert!(tab.pane_being_resized_with_mouse.is_none());
}

#[test]
pub fn floating_pane_positions_do_not_underflow_on_tiny_viewports() {
    for (cols, rows) in [(1, 1), (2, 2), (3, 3), (9, 9)] {
        let size = Size { cols, rows };
        let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
        tab.new_pane(
            PaneId::Terminal(2),
            None,
            None,
            false,
            true,
            NewPanePlacement::Floating(None),
            Some(1),
            None,
        )
        .unwrap();
        assert!(!tab.floating_panes.panes_contain(&PaneId::Terminal(2)));
        assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(2)]);
    }
}

#[test]
pub fn focusing_a_suppressed_pane_finds_it_by_its_own_id() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let mut tab = create_new_tab(size, false);
    tab.replace_pane_with_editor_pane(PaneId::Terminal(5), PaneId::Terminal(1))
        .unwrap();
    assert!(!tab.has_non_suppressed_pane_with_pid(&PaneId::Terminal(1)));
    tab.focus_suppressed_pane_for_all_clients(PaneId::Terminal(1));
    assert!(tab.floating_panes.panes_contain(&PaneId::Terminal(1)));
    assert_eq!(
        tab.floating_panes.active_pane_id(1),
        Some(PaneId::Terminal(1))
    );
}

#[test]
pub fn focusing_a_suppressed_pane_without_floating_room_tiles_it() {
    let size = Size { cols: 121, rows: 9 };
    let mut tab = create_new_tab(size, false);
    new_suppressed_pane_in_tab(&mut tab, 2);
    tab.focus_suppressed_pane_for_all_clients(PaneId::Terminal(2));
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(2)));
    assert!(!tab.floating_panes.panes_contain(&PaneId::Terminal(2)));
    assert_eq!(tab.floating_panes.active_pane_id(1), None);
    assert!(!tab.are_floating_panes_visible());
}

#[test]
pub fn changing_floating_coordinates_of_a_tiled_pane_floats_it() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let mut tab = create_new_tab(size, false);
    new_tiled_pane_in_tab(&mut tab, 2);
    let coordinates = zellij_utils::data::FloatingPaneCoordinates {
        x: Some(zellij_utils::input::layout::PercentOrFixed::Fixed(5)),
        y: Some(zellij_utils::input::layout::PercentOrFixed::Fixed(5)),
        width: Some(zellij_utils::input::layout::PercentOrFixed::Fixed(30)),
        height: Some(zellij_utils::input::layout::PercentOrFixed::Fixed(10)),
        pinned: None,
        borderless: None,
        border_style: None,
    };
    tab.change_floating_pane_coordinates(&PaneId::Terminal(2), coordinates.clone(), true)
        .unwrap();
    assert!(tab.floating_panes.panes_contain(&PaneId::Terminal(2)));
    let geom = tab
        .floating_panes
        .get_pane(PaneId::Terminal(2))
        .unwrap()
        .position_and_size();
    assert_eq!((geom.x, geom.y), (5, 5));
    assert!(tab
        .change_floating_pane_coordinates(&PaneId::Terminal(1), coordinates, true)
        .is_err());
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(1)));
}

#[test]
pub fn new_plugin_pane_without_a_plugin_thread_is_closed() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    tab.senders.to_plugin = None;
    for (id, placement) in [
        (7, NewPanePlacement::Floating(None)),
        (8, NewPanePlacement::default()),
        (
            9,
            NewPanePlacement::Stacked {
                pane_id_to_stack_under: None,
                borderless: None,
                border_style: None,
            },
        ),
    ] {
        let (completion, completion_receiver) = completion_and_receiver();
        tab.new_pane(
            PaneId::Plugin(id),
            None,
            None,
            false,
            true,
            placement,
            Some(1),
            Some(completion),
        )
        .unwrap();
        assert!(!tab.has_pane_with_pid(&PaneId::Plugin(id)));
        assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Plugin(id)]);
        assert_reported_as_not_created(
            completion_receiver,
            PaneNotCreatedReason::PluginThreadUnavailable,
        );
    }
    tab.suppress_pane_and_replace_with_pid(
        PaneId::Terminal(1),
        PaneId::Plugin(10),
        false,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(1)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Plugin(10)]);
}

#[test]
pub fn suppressing_into_an_occupied_slot_keeps_both_panes() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let mut tab = create_new_tab(size, false);
    tab.replace_pane_with_editor_pane(PaneId::Terminal(5), PaneId::Terminal(1))
        .unwrap();
    new_tiled_pane_in_tab(&mut tab, 2);
    tab.suppress_pane_and_replace_with_pid(
        PaneId::Terminal(2),
        PaneId::Terminal(5),
        false,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(tab.has_pane_with_pid(&PaneId::Terminal(1)));
    assert!(tab.has_pane_with_pid(&PaneId::Terminal(2)));
}

#[derive(Clone, Default)]
struct RecordingOsApi {
    writes: std::sync::Arc<std::sync::Mutex<Vec<(u32, String)>>>,
}

impl ServerOsApi for RecordingOsApi {
    fn set_terminal_size_using_terminal_id(
        &self,
        _id: u32,
        _cols: u16,
        _rows: u16,
        _width_in_pixels: Option<u16>,
        _height_in_pixels: Option<u16>,
    ) -> Result<()> {
        Ok(())
    }
    fn spawn_terminal(
        &self,
        _file_to_open: TerminalAction,
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _default_editor: Option<PathBuf>,
        _pane_env: &crate::os_input_output::PaneEnv,
    ) -> Result<(u32, Box<dyn AsyncReader>, Option<u32>)> {
        unimplemented!()
    }
    fn write_to_tty_stdin(&self, id: u32, buf: &[u8]) -> Result<usize> {
        self.writes
            .lock()
            .unwrap()
            .push((id, String::from_utf8_lossy(buf).to_string()));
        Ok(buf.len())
    }
    fn tcdrain(&self, _id: u32) -> Result<()> {
        Ok(())
    }
    fn kill(&self, _pid: u32) -> Result<()> {
        Ok(())
    }
    fn force_kill(&self, _pid: u32) -> Result<()> {
        Ok(())
    }
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new((*self).clone())
    }
    fn send_to_client(&self, _client_id: ClientId, _msg: ServerToClientMsg) -> Result<()> {
        Ok(())
    }
    fn register_client(
        &mut self,
        _client_id: ClientId,
        _receiver: &IpcReceiverWithContext<ClientToServerMsg>,
    ) -> Result<()> {
        Ok(())
    }
    fn register_client_with_reply(
        &mut self,
        _client_id: ClientId,
        _reply_stream: LocalSocketStream,
    ) -> Result<()> {
        Ok(())
    }
    fn remove_client(&mut self, _client_id: ClientId) -> Result<()> {
        Ok(())
    }
    fn load_palette(&self) -> Palette {
        Palette::default()
    }
    fn get_cwd(&self, _pid: u32) -> Option<PathBuf> {
        None
    }
    fn write_to_file(&mut self, _buf: String, _name: Option<String>) -> Result<()> {
        Ok(())
    }
    fn re_run_command_in_terminal(
        &self,
        _terminal_id: u32,
        _run_command: RunCommand,
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _pane_env: &crate::os_input_output::PaneEnv,
    ) -> Result<(Box<dyn AsyncReader>, Option<u32>)> {
        unimplemented!()
    }
    fn clear_terminal_id(&self, _terminal_id: u32) -> Result<()> {
        Ok(())
    }
    fn send_sigint(&self, _pid: u32) -> Result<()> {
        Ok(())
    }
}

fn plugin_run() -> zellij_utils::input::layout::RunPluginOrAlias {
    zellij_utils::input::layout::RunPluginOrAlias::from_url(
        "file:/path/to/fake/plugin",
        &None,
        None,
        None,
    )
    .unwrap()
}

#[test]
pub fn max_panes_limits_tiled_panes_without_crashing() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    tab.max_panes = Some(1);
    new_tiled_pane_in_tab(&mut tab, 2);
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(2)));
    assert!(!tab.tiled_panes.panes_contain(&PaneId::Terminal(1)));
    assert!(closed_pane_ids(&receivers).contains(&PaneId::Terminal(1)));
    tab.max_panes = Some(0);
    new_tiled_pane_in_tab(&mut tab, 3);
    assert!(tab.has_pane_with_pid(&PaneId::Terminal(3)));
}

#[test]
pub fn resizing_to_a_tiny_size_with_fixed_bars_keeps_all_panes() {
    let size = Size { cols: 50, rows: 20 };
    let mut top_bar = TiledPaneLayout::default();
    top_bar.split_size = Some(SplitSize::Fixed(1));
    let mut bottom_bar = TiledPaneLayout::default();
    bottom_bar.split_size = Some(SplitSize::Fixed(2));
    let mut layout = TiledPaneLayout::default();
    layout.children_split_direction = SplitDirection::Horizontal;
    layout.children = vec![top_bar, TiledPaneLayout::default(), bottom_bar];
    let mut tab = create_new_tab_with_layout(size, layout);
    assert_eq!(tab.tiled_panes.panes.len(), 3);
    for rows in [3, 2, 1, 20] {
        assert!(tab.resize_whole_tab(Size { cols: 50, rows }).is_ok());
        assert_eq!(tab.tiled_panes.panes.len(), 3);
    }
}

#[test]
pub fn in_place_pane_without_a_target_is_closed_and_reported_as_failed() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    let (completion, completion_receiver) = completion_and_receiver();
    tab.new_in_place_pane(
        PaneId::Terminal(5),
        None,
        None,
        None,
        false,
        None,
        Some(completion),
        None,
    )
    .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(5)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(5)]);
    assert_reported_as_not_created(
        completion_receiver,
        PaneNotCreatedReason::NoClientOrTargetPane,
    );
    tab.tiled_panes.clear_active_panes();
    let (completion, completion_receiver) = completion_and_receiver();
    tab.new_in_place_pane(
        PaneId::Terminal(6),
        None,
        None,
        None,
        false,
        Some(1),
        Some(completion),
        None,
    )
    .unwrap();
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(6)));
    assert_eq!(closed_pane_ids(&receivers), vec![PaneId::Terminal(6)]);
    assert_reported_as_not_created(
        completion_receiver,
        PaneNotCreatedReason::PaneToReplaceNotFound,
    );
}

#[test]
pub fn pane_moved_to_floating_with_focus_shows_floating_panes() {
    let size = Size { cols: 10, rows: 10 };
    let mut tab = create_new_tab(size, false);
    new_suppressed_pane_in_tab(&mut tab, 2);
    assert!(!tab.are_floating_panes_visible());
    tab.focus_pane_with_id(PaneId::Terminal(2), false, false, 1)
        .unwrap();
    assert!(tab.floating_panes.panes_contain(&PaneId::Terminal(2)));
    assert!(tab.are_floating_panes_visible());
    assert_eq!(
        tab.get_active_pane_id(1),
        Some(PaneId::Terminal(2)),
        "the moved pane is focused"
    );
}

#[test]
pub fn pane_moved_to_tiled_hides_the_empty_floating_layer() {
    let size = Size { cols: 121, rows: 9 };
    let mut tab = create_new_tab(size, false);
    tab.show_floating_panes();
    tab.add_floating_pane(
        detached_terminal_pane(3),
        PaneId::Terminal(3),
        None,
        true,
        Some(1),
    )
    .unwrap();
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(3)));
    assert!(!tab.are_floating_panes_visible());
}

#[test]
pub fn broken_stack_list_is_dissolved_and_its_members_are_kept() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let (mut tab, _receivers) = create_new_tab_with_receivers(size, true, true);
    new_tiled_pane_in_tab(&mut tab, 2);
    tab.new_pane(
        PaneId::Terminal(3),
        None,
        None,
        false,
        true,
        NewPanePlacement::Stacked {
            pane_id_to_stack_under: None,
            borderless: None,
            border_style: None,
        },
        Some(1),
        None,
    )
    .unwrap();
    assert!(tab.has_stack_lists());
    assert!(tab.pane_is_hidden_stack_list_member(&PaneId::Terminal(2)));
    let visible = tab.tiled_panes.extract_pane(PaneId::Terminal(3)).unwrap();
    tab.focus_pane_with_id(PaneId::Terminal(2), false, false, 1)
        .unwrap();
    assert!(!tab.has_stack_lists());
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(2)));
    assert!(focused_tiled_pane_exists(&tab, 1));
    drop(visible);
}

#[test]
pub fn changing_coordinates_from_a_plugin_does_not_float_tiled_panes() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let mut tab = create_new_tab(size, false);
    new_tiled_pane_in_tab(&mut tab, 2);
    let coordinates = zellij_utils::data::FloatingPaneCoordinates {
        x: Some(zellij_utils::input::layout::PercentOrFixed::Fixed(5)),
        y: None,
        width: None,
        height: None,
        pinned: None,
        borderless: None,
        border_style: None,
    };
    assert!(tab
        .change_floating_pane_coordinates(&PaneId::Terminal(2), coordinates.clone(), false)
        .is_err());
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(2)));
    new_suppressed_pane_in_tab(&mut tab, 3);
    tab.change_floating_pane_coordinates(&PaneId::Terminal(3), coordinates, false)
        .unwrap();
    assert!(tab.floating_panes.panes_contain(&PaneId::Terminal(3)));
}

#[test]
pub fn joining_client_has_its_floating_pane_brought_to_the_top() {
    let mut tab = tab_with_two_floating_panes();
    tab.remove_client(1);
    tab.add_floating_pane(
        detached_terminal_pane(4),
        PaneId::Terminal(4),
        None,
        false,
        None,
    )
    .unwrap();
    let z = |tab: &Tab, id| tab.floating_panes.get_pane_z_index(PaneId::Terminal(id));
    assert!(z(&tab, 4) > z(&tab, 3));
    tab.add_client(1, None).unwrap();
    assert_eq!(
        tab.floating_panes.active_pane_id(1),
        Some(PaneId::Terminal(3))
    );
    assert!(z(&tab, 3) > z(&tab, 4));
}

#[test]
pub fn reapplying_focus_without_clients_keeps_the_focused_pane_on_top() {
    let mut tab = tab_with_two_floating_panes();
    tab.remove_client(1);
    tab.floating_panes
        .add_pane(PaneId::Terminal(4), detached_terminal_pane(4));
    let z = |tab: &Tab, id| tab.floating_panes.get_pane_z_index(PaneId::Terminal(id));
    assert!(z(&tab, 4) > z(&tab, 3));
    tab.floating_panes.reapply_pane_focus();
    assert!(z(&tab, 3) > z(&tab, 4));
}

#[test]
pub fn moving_focus_without_floating_focus_reports_that_focus_moved() {
    let mut tab = tab_with_two_floating_panes();
    tab.floating_panes.defocus_pane(1);
    assert!(tab.move_focus_left(1).unwrap());
    assert!(floating_focus_is_valid(&tab, 1));
    leave_only_floating_pane(&mut tab, PaneId::Terminal(2));
    tab.floating_panes.focus_pane(PaneId::Terminal(2), 1);
    assert!(!tab.move_focus_left(1).unwrap());
}

#[test]
pub fn input_routing_and_active_pane_agree_without_floating_focus() {
    let mut tab = tab_with_two_floating_panes();
    tab.floating_panes.defocus_pane(1);
    assert_eq!(tab.get_active_pane_id(1), None);
    assert!(tab.get_active_pane_or_floating_pane_mut(1).is_none());
    tab.toggle_active_pane_fullscreen(1);
    assert!(!tab.tiled_panes.fullscreen_is_active());
}

#[test]
pub fn failed_tiled_add_keeps_fullscreen() {
    let size = Size { cols: 12, rows: 10 };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    assert_eq!(tab.tiled_panes.panes.len(), 2);
    tab.focus_pane_with_id(PaneId::Terminal(1), false, false, 1)
        .unwrap();
    tab.toggle_active_pane_fullscreen(1);
    assert_eq!(
        tab.tiled_panes.fullscreen_pane_id(),
        Some(PaneId::Terminal(1))
    );
    new_tiled_pane_in_tab(&mut tab, 3);
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(3)));
    assert!(closed_pane_ids(&receivers).contains(&PaneId::Terminal(3)));
    assert_eq!(
        tab.tiled_panes.fullscreen_pane_id(),
        Some(PaneId::Terminal(1))
    );
}

#[test]
pub fn focus_events_are_sent_once_per_pane_and_only_for_connected_clients() {
    let os_api = RecordingOsApi::default();
    let writes = os_api.writes.clone();
    let boxed_os_api: Box<dyn ServerOsApi> = Box::new(os_api);
    let mut active_panes = crate::panes::ActivePanes::new(&boxed_os_api);
    let mut panes: std::collections::BTreeMap<PaneId, Box<dyn Pane>> =
        std::collections::BTreeMap::new();
    for id in [2, 3] {
        panes.insert(
            PaneId::Terminal(id),
            Box::new(super::test_panes::FocusReportingPane {
                inner: detached_terminal_pane(id),
            }),
        );
    }
    active_panes.insert(1, PaneId::Terminal(2), &mut panes);
    active_panes.insert(2, PaneId::Terminal(2), &mut panes);
    active_panes.insert(3, PaneId::Terminal(3), &mut panes);
    writes.lock().unwrap().clear();
    let connected_clients: std::collections::HashSet<ClientId> = [1, 2].into_iter().collect();
    active_panes.focus_all_panes(&mut panes, &connected_clients);
    assert_eq!(*writes.lock().unwrap(), vec![(2, "focus-in".to_owned())]);
    writes.lock().unwrap().clear();
    active_panes.unfocus_all_panes(&mut panes, &connected_clients);
    assert_eq!(*writes.lock().unwrap(), vec![(2, "focus-out".to_owned())]);
}

#[test]
pub fn replacing_a_pane_moves_the_last_pane_record_and_the_drag_to_the_replacement() {
    let mut tab = tab_with_two_floating_panes();
    tab.floating_panes.focus_pane(PaneId::Terminal(2), 1);
    tab.floating_panes.focus_pane(PaneId::Terminal(3), 1);
    assert_eq!(
        tab.floating_panes.get_last_pane_id(1),
        Some(PaneId::Terminal(2))
    );
    tab.floating_panes.set_pane_being_moved_with_mouse(
        PaneId::Terminal(2),
        zellij_utils::position::Position::new(10, 10),
    );
    tab.floating_panes
        .replace_pane(PaneId::Terminal(2), detached_terminal_pane(5))
        .ok()
        .unwrap();
    assert_eq!(
        tab.floating_panes.get_last_pane_id(1),
        Some(PaneId::Terminal(5))
    );
    assert_eq!(
        tab.floating_panes.pane_being_moved_with_mouse_id(),
        Some(PaneId::Terminal(5))
    );
    let mut tiled_tab = tab_with_three_panes();
    let last = tiled_tab.tiled_panes.get_last_pane_id(1).unwrap();
    tiled_tab
        .tiled_panes
        .replace_pane(last, detached_terminal_pane(6))
        .ok()
        .unwrap();
    assert_eq!(
        tiled_tab.tiled_panes.get_last_pane_id(1),
        Some(PaneId::Terminal(6))
    );
    tiled_tab.tiled_panes.focus_last_pane(1);
    assert_eq!(
        tiled_tab.tiled_panes.focused_pane_id(1),
        Some(PaneId::Terminal(6))
    );
}

#[test]
pub fn adding_a_floating_pane_twice_keeps_one_stacking_entry() {
    let mut tab = tab_with_two_floating_panes();
    let pane = tab
        .floating_panes
        .drain()
        .remove(&PaneId::Terminal(2))
        .unwrap();
    let mut tab_with_duplicate = tab_with_two_floating_panes();
    tab_with_duplicate
        .floating_panes
        .add_pane(PaneId::Terminal(2), pane);
    assert_eq!(tab_with_duplicate.floating_panes.visible_panes_count(), 2);
    assert_eq!(
        tab_with_duplicate
            .floating_panes
            .stack()
            .map(|stack| stack.layers.len()),
        Some(2)
    );
    assert_eq!(
        tab_with_duplicate
            .floating_panes
            .get_pane_z_index(PaneId::Terminal(2)),
        Some(1)
    );
}

#[test]
pub fn hidden_pane_is_kept_when_its_resize_message_fails() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, false);
    let TabReceivers {
        pty_writer, plugin, ..
    } = receivers;
    drop(pty_writer);
    drop(plugin);
    new_suppressed_pane_in_tab(&mut tab, 2);
    assert!(tab.has_pane_with_pid(&PaneId::Terminal(2)));
}

#[test]
pub fn closing_a_pane_suppressed_by_itself_reports_it_as_closed() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let (mut tab, receivers) = create_new_tab_with_receivers(size, false, true);
    new_suppressed_pane_in_tab(&mut tab, 2);
    tab.close_pane(PaneId::Terminal(2), false, Some(0));
    assert!(!tab.has_pane_with_pid(&PaneId::Terminal(2)));
    assert_eq!(
        pane_closed_events(&receivers),
        vec![zellij_utils::data::PaneId::Terminal(2)]
    );
}

#[test]
pub fn focusing_a_suppressed_pane_prefers_the_entry_stored_under_its_own_id() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let mut tab = create_new_tab(size, false);
    new_tiled_pane_in_tab(&mut tab, 2);
    tab.replace_pane_with_editor_pane(PaneId::Terminal(5), PaneId::Terminal(2))
        .unwrap();
    new_suppressed_pane_in_tab(&mut tab, 6);
    tab.suppress_pane_and_replace_with_other_pane(
        PaneId::Terminal(1),
        detached_terminal_pane(7),
        None,
    );
    tab.focus_suppressed_pane_for_all_clients(PaneId::Terminal(6));
    assert!(tab.floating_panes.panes_contain(&PaneId::Terminal(6)));
    assert!(tab.has_pane_with_pid(&PaneId::Terminal(2)));
    assert!(!tab.has_non_suppressed_pane_with_pid(&PaneId::Terminal(2)));
}

#[test]
pub fn floating_panes_render_for_a_watcher_following_a_departed_client() {
    let mut tab = tab_with_two_floating_panes();
    tab.remove_client(1);
    let mut output = crate::output::Output::default();
    tab.render(&mut output, Some(1)).unwrap();
    let rendered = output.drain_pane_render_report();
    assert!(
        rendered
            .all_pane_contents
            .values()
            .any(|contents| contents.contains_key(&zellij_utils::data::PaneId::Terminal(3))),
        "the floating pane contents are rendered for the watcher"
    );
}

#[test]
pub fn a_pane_that_keeps_failing_to_render_is_remembered_and_others_still_render() {
    let mut tab = tab_with_two_floating_panes();
    let failing = Box::new(super::test_panes::FailingRenderPane {
        inner: detached_terminal_pane(5),
    });
    tab.add_floating_pane(failing, PaneId::Terminal(5), None, true, Some(1))
        .unwrap();
    tab.add_client(2, None).unwrap();
    for _ in 0..2 {
        let mut output = crate::output::Output::default();
        tab.render(&mut output, None).unwrap();
        assert!(tab
            .floating_panes
            .panes_with_logged_render_errors()
            .contains(&PaneId::Terminal(5)));
        let report = output.drain_pane_render_report();
        for client_id in [1, 2] {
            assert!(report
                .all_pane_contents
                .get(&client_id)
                .map(|c| c.contains_key(&zellij_utils::data::PaneId::Terminal(2)))
                .unwrap_or(false));
        }
    }
    tab.close_pane(PaneId::Terminal(5), false, None);
    let mut output = crate::output::Output::default();
    tab.render(&mut output, None).unwrap();
    assert!(tab
        .floating_panes
        .panes_with_logged_render_errors()
        .is_empty());
}

#[test]
pub fn layout_whose_floating_panes_are_not_created_opens_no_shell_and_unloads_unused_plugins() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let (mut tab, receivers) = create_tab_with_receivers_without_layout(size, false, true);
    let mut floating_layout = zellij_utils::input::layout::FloatingPaneLayout::default();
    floating_layout.run = Some(zellij_utils::input::layout::Run::Plugin(plugin_run()));
    let mut new_plugin_ids = HashMap::new();
    let other_plugin = zellij_utils::input::layout::RunPluginOrAlias::from_url(
        "file:/path/to/other/plugin",
        &None,
        None,
        None,
    )
    .unwrap();
    new_plugin_ids.insert(other_plugin, vec![9]);
    tab.apply_layout(
        TiledPaneLayout::default(),
        vec![floating_layout],
        vec![(1, None)],
        vec![],
        new_plugin_ids,
        1,
        None,
    )
    .unwrap();
    assert!(!tab.are_floating_panes_visible());
    let spawned_terminals = receivers
        .pty
        .try_iter()
        .filter(|(instruction, _)| matches!(instruction, PtyInstruction::SpawnTerminal(..)))
        .count();
    assert_eq!(spawned_terminals, 0);
    let unloaded: Vec<u32> = receivers
        .plugin
        .try_iter()
        .filter_map(|(instruction, _)| match instruction {
            PluginInstruction::Unload(plugin_id) => Some(plugin_id),
            _ => None,
        })
        .collect();
    assert_eq!(unloaded, vec![9]);
}

#[test]
pub fn failed_layout_override_does_not_leave_the_tab_pending() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, false);
    let mut too_big = TiledPaneLayout::default();
    too_big.split_size = Some(SplitSize::Fixed(500));
    let mut layout = TiledPaneLayout::default();
    layout.children_split_direction = SplitDirection::Horizontal;
    layout.children = vec![too_big.clone(), too_big];
    tab.is_pending = true;
    tab.override_layout(
        layout,
        vec![],
        None,
        None,
        vec![],
        vec![],
        HashMap::new(),
        true,
        true,
        1,
        None,
    )
    .unwrap();
    assert!(!tab.is_pending());
    assert!(tab.has_pane_with_pid(&PaneId::Terminal(1)));
}

#[test]
pub fn floating_panes_survive_shrinking_the_tab_to_a_tiny_size() {
    let mut tab = tab_with_two_floating_panes();
    for size in [
        Size { cols: 9, rows: 9 },
        Size {
            cols: 121,
            rows: 40,
        },
    ] {
        tab.resize_whole_tab(size).unwrap();
        assert!(tab.floating_panes.panes_contain(&PaneId::Terminal(2)));
        assert!(tab.floating_panes.panes_contain(&PaneId::Terminal(3)));
        assert!(floating_focus_is_valid(&tab, 1));
    }
}

#[test]
pub fn returning_a_pane_from_a_hidden_floating_layer_keeps_the_layer_hidden() {
    let mut tab = tab_with_two_floating_panes();
    tab.hide_floating_panes();
    let location = tab.pane_location(&PaneId::Terminal(3));
    assert_eq!(location, super::PaneLocation::Floating { visible: false });
    let pane = tab.extract_pane(PaneId::Terminal(3), true).unwrap();
    tab.return_pane(pane, location);
    assert!(tab.floating_panes.panes_contain(&PaneId::Terminal(3)));
    assert!(!tab.are_floating_panes_visible());
}

#[test]
pub fn changing_coordinates_from_a_plugin_does_not_float_a_tiled_pane_that_suppresses_another() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let mut tab = create_new_tab(size, false);
    new_tiled_pane_in_tab(&mut tab, 2);
    tab.replace_pane_with_editor_pane(PaneId::Terminal(5), PaneId::Terminal(2))
        .unwrap();
    let coordinates = zellij_utils::data::FloatingPaneCoordinates {
        x: Some(zellij_utils::input::layout::PercentOrFixed::Fixed(5)),
        y: None,
        width: None,
        height: None,
        pinned: None,
        borderless: None,
        border_style: None,
    };
    assert!(tab
        .change_floating_pane_coordinates(&PaneId::Terminal(5), coordinates, false)
        .is_err());
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(5)));
}

#[test]
pub fn recovering_a_broken_stack_list_keeps_an_editor_paired_with_the_pane_it_edits() {
    let size = Size {
        cols: 121,
        rows: 40,
    };
    let (mut tab, _receivers) = create_new_tab_with_receivers(size, true, true);
    new_tiled_pane_in_tab(&mut tab, 2);
    tab.new_pane(
        PaneId::Terminal(3),
        None,
        None,
        false,
        true,
        NewPanePlacement::Stacked {
            pane_id_to_stack_under: None,
            borderless: None,
            border_style: None,
        },
        Some(1),
        None,
    )
    .unwrap();
    tab.replace_pane_with_editor_pane(PaneId::Terminal(5), PaneId::Terminal(3))
        .unwrap();
    assert!(tab.focus_hidden_stack_list_member(PaneId::Terminal(2), 1));
    let stack_list_id = tab.stack_list_id_of_member(&PaneId::Terminal(5)).unwrap();
    tab.tiled_panes.extract_pane(PaneId::Terminal(2)).unwrap();
    tab.recover_broken_stack_list(stack_list_id);
    assert!(!tab.has_stack_lists());
    assert!(tab.tiled_panes.panes_contain(&PaneId::Terminal(5)));
    assert_eq!(
        tab.suppressed_panes
            .get(&PaneId::Terminal(5))
            .map(|(_, pane)| pane.pid()),
        Some(PaneId::Terminal(3)),
        "the edited pane is still restored when the editor closes"
    );
    for (key, (_, pane)) in tab.suppressed_panes.iter() {
        assert!(
            *key == pane.pid() || *key == PaneId::Terminal(5),
            "pane {:?} is stored under another pane's id {:?}",
            pane.pid(),
            key
        );
    }
}

#[test]
pub fn reasons_for_panes_that_were_not_created_do_not_accumulate() {
    let size = Size { cols: 121, rows: 4 };
    let (mut tab, _receivers) = create_new_tab_with_receivers(size, false, true);
    for id in 2..10 {
        tab.horizontal_split(PaneId::Terminal(id), None, 1, None, None)
            .unwrap();
    }
    assert_eq!(tab.pane_not_created_reasons_count(), 8);
    tab.clear_pane_not_created_reasons();
    assert_eq!(tab.pane_not_created_reasons_count(), 0);
}

fn tab_with_a_pane_above_a_pane() -> Tab {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut tab = create_new_tab(size, true);
    tab.horizontal_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    tab
}

fn geom_of(tab: &Tab, pane_id: PaneId) -> PaneGeom {
    tab.tiled_panes
        .panes
        .get(&pane_id)
        .unwrap()
        .position_and_size()
}

#[test]
fn collapsing_a_pane_gives_its_rows_to_its_neighbor() {
    let mut tab = tab_with_a_pane_above_a_pane();
    let rows_before = geom_of(&tab, PaneId::Terminal(1)).rows.as_usize();

    tab.set_pane_collapsed(PaneId::Terminal(2), true);

    assert_eq!(
        geom_of(&tab, PaneId::Terminal(1)).rows.as_usize(),
        20,
        "the pane that stayed should hold every row the tab has"
    );
    assert!(
        rows_before < 20,
        "the two panes should have been sharing the rows to begin with"
    );
}

#[test]
fn expanding_a_pane_restores_the_geometry_it_had() {
    let mut tab = tab_with_a_pane_above_a_pane();
    let before = (
        geom_of(&tab, PaneId::Terminal(1)),
        geom_of(&tab, PaneId::Terminal(2)),
    );

    tab.set_pane_collapsed(PaneId::Terminal(2), true);
    tab.set_pane_collapsed(PaneId::Terminal(2), false);

    // exact equality rather than a row count: the point of collapsing instead of suppressing
    // is that the layout comes back as it was, position and constraint included
    assert_eq!(
        (
            geom_of(&tab, PaneId::Terminal(1)),
            geom_of(&tab, PaneId::Terminal(2))
        ),
        before
    );
}

#[test]
fn a_collapsed_pane_keeps_the_geometry_it_will_come_back_to() {
    let mut tab = tab_with_a_pane_above_a_pane();
    let before = geom_of(&tab, PaneId::Terminal(2));

    tab.set_pane_collapsed(PaneId::Terminal(2), true);

    assert_eq!(
        geom_of(&tab, PaneId::Terminal(2)),
        before,
        "a collapsed pane holds no space, but it still describes where it will come back, \
         so collapsing on its own must not move it"
    );
}

#[test]
fn collapsing_a_pane_that_is_already_collapsed_does_nothing() {
    let mut tab = tab_with_a_pane_above_a_pane();
    tab.set_pane_collapsed(PaneId::Terminal(2), true);
    let after_first = geom_of(&tab, PaneId::Terminal(1));

    tab.set_pane_collapsed(PaneId::Terminal(2), true);

    assert_eq!(geom_of(&tab, PaneId::Terminal(1)), after_first);
}

#[test]
fn a_collapsed_pane_is_still_collapsed_after_a_fullscreen_comes_and_goes() {
    let mut tab = tab_with_a_pane_above_a_pane();
    tab.set_pane_collapsed(PaneId::Terminal(2), true);
    tab.focus_pane_with_id(PaneId::Terminal(1), false, false, 1)
        .unwrap();

    // fullscreen rebuilds the hidden set from scratch, so a collapse that lived only there
    // would be handed its rows back on the way out
    tab.toggle_active_pane_fullscreen(1);
    tab.toggle_active_pane_fullscreen(1);

    assert!(tab.tiled_panes.pane_is_collapsed(&PaneId::Terminal(2)));
    assert_eq!(
        geom_of(&tab, PaneId::Terminal(1)).rows.as_usize(),
        20,
        "the pane that stayed should still hold every row"
    );
}

#[test]
fn closing_a_collapsed_pane_forgets_that_it_was_collapsed() {
    let mut tab = tab_with_a_pane_above_a_pane();
    tab.set_pane_collapsed(PaneId::Terminal(2), true);

    tab.close_pane(PaneId::Terminal(2), false, None);

    assert!(
        !tab.tiled_panes.pane_is_collapsed(&PaneId::Terminal(2)),
        "a pane id Zellij hands out again must not arrive already invisible"
    );
}

#[test]
fn collapsing_a_pane_that_is_not_in_the_tab_does_nothing() {
    let mut tab = tab_with_a_pane_above_a_pane();
    let before = (
        geom_of(&tab, PaneId::Terminal(1)),
        geom_of(&tab, PaneId::Terminal(2)),
    );

    tab.set_pane_collapsed(PaneId::Terminal(99), true);

    assert!(!tab.tiled_panes.pane_is_collapsed(&PaneId::Terminal(99)));
    assert_eq!(
        (
            geom_of(&tab, PaneId::Terminal(1)),
            geom_of(&tab, PaneId::Terminal(2))
        ),
        before
    );
}

/// The shape a bar plugin actually has: a full-width pane with a one-row pane pinned
/// under it, the way `pane size=1 borderless` in a layout arrives.
fn tab_with_a_one_row_bar_under_a_pane(size: Size) -> Tab {
    let mut layout = TiledPaneLayout::default();
    layout.children_split_direction = SplitDirection::Horizontal;
    let mut bar = TiledPaneLayout::default();
    bar.split_size = Some(SplitSize::Fixed(1));
    layout.children = vec![TiledPaneLayout::default(), bar];
    create_new_tab_with_layout(size, layout)
}

#[test]
fn a_collapsed_pane_follows_the_tab_when_it_is_resized() {
    let mut tab = tab_with_a_one_row_bar_under_a_pane(Size {
        cols: 121,
        rows: 20,
    });
    let main_pane = PaneId::Terminal(0);
    let bar = PaneId::Terminal(1);
    tab.set_pane_collapsed(bar, true);

    // the tab changes shape, the way a nested session's does when its host expands it
    // over the whole display and takes its own bars off the screen with it
    tab.resize_whole_tab(Size {
        cols: 118,
        rows: 23,
    })
    .unwrap();

    // being left out of the solve is what keeps a collapsed pane from holding space, but
    // it must not leave the pane describing a tab that no longer exists: the solver reads
    // the layout tree back off pane geometry, so a stale one breaks the next solve
    assert_eq!(
        geom_of(&tab, bar).y,
        22,
        "the collapsed bar should still be tracking the bottom of the tab"
    );
    assert_eq!(
        geom_of(&tab, bar).cols.as_usize(),
        118,
        "the collapsed bar should still be tracking the width of the tab"
    );
    assert_eq!(
        geom_of(&tab, main_pane).rows.as_usize(),
        23,
        "the pane that stayed should hold every row of the resized tab"
    );

    tab.set_pane_collapsed(bar, false);

    assert_eq!(
        geom_of(&tab, main_pane).rows.as_usize(),
        22,
        "the pane that stayed should give the row back"
    );
    assert_eq!(
        geom_of(&tab, bar).y,
        22,
        "the bar should come back on the bottom row, not partway up the pane above it"
    );
    assert_eq!(geom_of(&tab, bar).rows.as_usize(), 1);
    assert_eq!(geom_of(&tab, bar).cols.as_usize(), 118);
}

fn tiled_geoms_as_serialized(tab: &Tab) -> HashMap<PaneId, PaneGeom> {
    let overrides = tab.tiled_pane_serialization_geoms();
    tab.get_tiled_panes()
        .map(|(pane_id, pane)| {
            (
                *pane_id,
                overrides
                    .get(pane_id)
                    .copied()
                    .unwrap_or_else(|| pane.position_and_size()),
            )
        })
        .collect()
}

fn all_tiled_geoms(tab: &Tab) -> HashMap<PaneId, PaneGeom> {
    tab.get_tiled_panes()
        .map(|(pane_id, pane)| (*pane_id, pane.position_and_size()))
        .collect()
}

#[test]
fn a_collapsed_bar_is_serialized_as_if_it_were_expanded() {
    let mut tab = tab_with_a_one_row_bar_under_a_pane(Size {
        cols: 121,
        rows: 20,
    });
    let before = all_tiled_geoms(&tab);

    tab.set_pane_collapsed(PaneId::Terminal(1), true);

    assert_ne!(all_tiled_geoms(&tab), before);
    assert_eq!(tiled_geoms_as_serialized(&tab), before);
}

#[test]
fn a_collapsed_column_is_serialized_as_if_it_were_expanded() {
    let mut tab = create_new_tab(
        Size {
            cols: 121,
            rows: 20,
        },
        true,
    );
    tab.vertical_split(PaneId::Terminal(2), None, 1, None, None)
        .unwrap();
    let before = all_tiled_geoms(&tab);

    tab.set_pane_collapsed(PaneId::Terminal(2), true);

    assert_ne!(all_tiled_geoms(&tab), before);
    assert_eq!(tiled_geoms_as_serialized(&tab), before);
}

#[test]
fn a_collapsed_pane_between_two_panes_is_serialized_as_if_it_were_expanded() {
    let mut tab = tab_with_a_pane_above_a_pane();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    let before = all_tiled_geoms(&tab);

    tab.set_pane_collapsed(PaneId::Terminal(2), true);

    assert_ne!(all_tiled_geoms(&tab), before);
    assert_eq!(tiled_geoms_as_serialized(&tab), before);
}

#[test]
fn a_collapsed_bar_under_a_stack_is_serialized_as_if_it_were_expanded() {
    let mut layout = TiledPaneLayout::default();
    layout.children_split_direction = SplitDirection::Horizontal;
    let mut stack = TiledPaneLayout::default();
    stack.children_are_stacked = true;
    stack.children = vec![
        TiledPaneLayout::default(),
        TiledPaneLayout::default(),
        TiledPaneLayout::default(),
    ];
    let mut bar = TiledPaneLayout::default();
    bar.split_size = Some(SplitSize::Fixed(1));
    layout.children = vec![stack, bar];
    let mut tab = create_new_tab_with_layout(
        Size {
            cols: 121,
            rows: 20,
        },
        layout,
    );
    let bar = PaneId::Terminal(3);
    assert!(geom_of(&tab, PaneId::Terminal(0)).is_stacked());
    let before = all_tiled_geoms(&tab);

    tab.set_pane_collapsed(bar, true);

    assert_ne!(all_tiled_geoms(&tab), before);
    assert_eq!(tiled_geoms_as_serialized(&tab), before);
}

#[test]
fn nothing_is_overridden_for_serialization_when_no_pane_is_collapsed() {
    let tab = tab_with_a_pane_above_a_pane();

    assert!(tab.tiled_pane_serialization_geoms().is_empty());
}

#[test]
fn collapsing_the_focused_pane_moves_focus_to_a_visible_pane() {
    let mut tab = tab_with_a_pane_above_a_pane();
    assert_eq!(tab.get_active_pane_id(1), Some(PaneId::Terminal(2)));

    tab.set_pane_collapsed(PaneId::Terminal(2), true);

    assert_eq!(tab.get_active_pane_id(1), Some(PaneId::Terminal(1)));
}

#[test]
fn a_collapsed_pane_cannot_be_focused() {
    let mut tab = tab_with_a_pane_above_a_pane();
    tab.set_pane_collapsed(PaneId::Terminal(2), true);

    tab.focus_pane_with_id(PaneId::Terminal(2), false, false, 1)
        .unwrap();

    assert_eq!(tab.get_active_pane_id(1), Some(PaneId::Terminal(1)));
}

#[test]
fn a_collapsed_pane_cannot_be_focused_while_another_pane_is_fullscreen() {
    let mut tab = tab_with_a_pane_above_a_pane();
    tab.set_pane_collapsed(PaneId::Terminal(2), true);
    tab.toggle_active_pane_fullscreen(1);
    assert!(tab.tiled_panes.fullscreen_is_active());

    tab.focus_pane_with_id(PaneId::Terminal(2), false, false, 1)
        .unwrap();

    assert_eq!(tab.get_active_pane_id(1), Some(PaneId::Terminal(1)));
    assert!(
        tab.tiled_panes.fullscreen_is_active(),
        "refusing to focus a collapsed pane should leave fullscreen alone"
    );
}

#[test]
fn collapsing_the_only_selectable_pane_keeps_its_focus() {
    let mut tab = create_new_tab(
        Size {
            cols: 121,
            rows: 20,
        },
        true,
    );

    tab.set_pane_collapsed(PaneId::Terminal(1), true);

    assert_eq!(tab.get_active_pane_id(1), Some(PaneId::Terminal(1)));
}

#[test]
fn a_click_on_a_collapsed_panes_space_reaches_the_pane_that_grew_over_it() {
    let mut tab = tab_with_a_pane_above_a_pane();
    tab.set_pane_collapsed(PaneId::Terminal(1), true);
    let point = Position::new(2, 5);
    assert!(
        geom_of(&tab, PaneId::Terminal(1)).contains(&point),
        "the collapsed pane should still describe the space it will come back to"
    );

    assert_eq!(
        tab.get_pane_id_at(&point, true).unwrap(),
        Some(PaneId::Terminal(2))
    );
    assert_eq!(
        tab.get_pane_id_at(&point, false).unwrap(),
        Some(PaneId::Terminal(2))
    );
}

#[test]
fn closing_a_collapsed_pane_leaves_its_neighbors_where_they_are() {
    let mut tab = tab_with_a_pane_above_a_pane();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.set_pane_collapsed(PaneId::Terminal(2), true);
    let place_of = |tab: &Tab, pane_id: PaneId| {
        let geom = geom_of(tab, pane_id);
        (geom.x, geom.y, geom.cols.as_usize(), geom.rows.as_usize())
    };
    let top = place_of(&tab, PaneId::Terminal(1));
    let bottom = place_of(&tab, PaneId::Terminal(3));

    tab.close_pane(PaneId::Terminal(2), false, None);

    assert_eq!(place_of(&tab, PaneId::Terminal(1)), top);
    assert_eq!(place_of(&tab, PaneId::Terminal(3)), bottom);
    assert_eq!(
        geom_of(&tab, PaneId::Terminal(1)).rows.as_usize()
            + geom_of(&tab, PaneId::Terminal(3)).rows.as_usize(),
        20
    );
    let percents = geom_of(&tab, PaneId::Terminal(1))
        .rows
        .as_percent()
        .unwrap()
        + geom_of(&tab, PaneId::Terminal(3))
            .rows
            .as_percent()
            .unwrap();
    assert!(
        (percents - 100.0).abs() < 0.001,
        "the closed pane's share should go to the panes that took its space, got {}",
        percents
    );
}

#[test]
fn closing_a_collapsed_bar_leaves_the_pane_above_it_holding_the_whole_tab() {
    let mut tab = tab_with_a_one_row_bar_under_a_pane(Size {
        cols: 121,
        rows: 20,
    });
    let main_pane = PaneId::Terminal(0);
    let bar = PaneId::Terminal(1);
    tab.set_pane_collapsed(bar, true);
    let before = geom_of(&tab, main_pane);

    tab.close_pane(bar, false, None);

    assert_eq!(geom_of(&tab, main_pane), before);
    assert_eq!(geom_of(&tab, main_pane).rows.as_usize(), 20);
}

#[test]
fn panes_keep_their_place_after_a_collapsed_pane_closes_and_the_tab_resizes() {
    let mut tab = tab_with_a_pane_above_a_pane();
    tab.horizontal_split(PaneId::Terminal(3), None, 1, None, None)
        .unwrap();
    tab.set_pane_collapsed(PaneId::Terminal(2), true);
    tab.close_pane(PaneId::Terminal(2), false, None);

    tab.resize_whole_tab(Size {
        cols: 121,
        rows: 40,
    })
    .unwrap();

    let top = geom_of(&tab, PaneId::Terminal(1));
    let bottom = geom_of(&tab, PaneId::Terminal(3));
    assert_eq!(top.y, 0);
    assert_eq!(bottom.y, top.rows.as_usize());
    assert_eq!(top.rows.as_usize() + bottom.rows.as_usize(), 40);
}

#[test]
fn pane_info_reports_whether_a_pane_is_collapsed() {
    let mut tab = tab_with_a_pane_above_a_pane();
    let is_collapsed = |tab: &Tab, id: u32| {
        tab.tiled_panes
            .pane_info(&HashMap::new())
            .into_iter()
            .find(|info| info.id == id && !info.is_plugin)
            .map(|info| info.is_collapsed)
            .unwrap()
    };

    tab.set_pane_collapsed(PaneId::Terminal(2), true);

    assert!(is_collapsed(&tab, 2));
    assert!(!is_collapsed(&tab, 1));

    tab.set_pane_collapsed(PaneId::Terminal(2), false);

    assert!(!is_collapsed(&tab, 2));
}

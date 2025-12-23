use crate::panes::sixel::SixelImageStore;
use crate::panes::{FloatingPanes, TiledPanes};
use crate::panes::{LinkHandler, PaneId};
use crate::plugins::PluginInstruction;
use crate::pty::PtyInstruction;
use crate::tab::layout_applier::LayoutApplier;
use crate::{
    os_input_output::{AsyncReader, Pid, ServerOsApi},
    thread_bus::ThreadSenders,
    ClientId,
};
use insta::assert_snapshot;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::rc::Rc;

use interprocess::local_socket::LocalSocketStream;
use zellij_utils::{
    channels::{self, ChannelWithContext, Receiver, SenderWithContext},
    data::{ModeInfo, Palette, Style},
    errors::prelude::*,
    input::command::{RunCommand, TerminalAction},
    input::layout::RunPluginOrAlias,
    input::layout::{FloatingPaneLayout, Layout, Run, TiledPaneLayout},
    ipc::{ClientToServerMsg, IpcReceiverWithContext, ServerToClientMsg},
    pane_size::{Size, SizeInPixels, Viewport},
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
        Ok(())
    }

    fn spawn_terminal(
        &self,
        _file_to_open: TerminalAction,
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
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

    fn write_to_tty_stdin(&self, _id: u32, _buf: &[u8]) -> Result<usize> {
        unimplemented!()
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

    fn write_to_file(&mut self, _buf: String, _name: Option<String>) -> Result<()> {
        unimplemented!()
    }

    fn re_run_command_in_terminal(
        &self,
        _terminal_id: u32,
        _run_command: RunCommand,
        _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    ) -> Result<(RawFd, RawFd)> {
        unimplemented!()
    }

    fn clear_terminal_id(&self, _terminal_id: u32) -> Result<()> {
        unimplemented!()
    }

    fn send_sigint(&self, _pid: Pid) -> Result<()> {
        unimplemented!()
    }
}

/// Parse KDL layout string and extract tiled and floating layouts
fn parse_kdl_layout(kdl_str: &str) -> (TiledPaneLayout, Vec<FloatingPaneLayout>) {
    let layout = Layout::from_kdl(kdl_str, Some("test_layout".into()), None, None)
        .expect("Failed to parse KDL layout");
    layout.new_tab()
}

/// Creates all the fixtures needed for LayoutApplier tests
#[allow(clippy::type_complexity)]
fn create_layout_applier_fixtures(
    size: Size,
) -> (
    Rc<RefCell<Viewport>>,
    ThreadSenders,
    Rc<RefCell<SixelImageStore>>,
    Rc<RefCell<LinkHandler>>,
    Rc<RefCell<Palette>>,
    Rc<RefCell<HashMap<usize, String>>>,
    Rc<RefCell<Option<SizeInPixels>>>,
    Rc<RefCell<HashMap<ClientId, bool>>>,
    Style,
    Rc<RefCell<Size>>,
    TiledPanes,
    FloatingPanes,
    bool,
    Option<PaneId>,
    Box<dyn ServerOsApi>,
    bool,
    bool,
    bool,
    bool,
) {
    let viewport = Rc::new(RefCell::new(Viewport {
        x: 0,
        y: 0,
        rows: size.rows,
        cols: size.cols,
    }));

    let (mock_plugin_sender, _mock_plugin_receiver) = channels::unbounded();
    let mut senders = ThreadSenders::default().silently_fail_on_send();
    senders.replace_to_plugin(SenderWithContext::new(mock_plugin_sender));
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(None));

    let client_id = 1;
    let mut connected_clients_map = HashMap::new();
    connected_clients_map.insert(client_id, false);
    let connected_clients = Rc::new(RefCell::new(connected_clients_map));

    let style = Style::default();
    let display_area = Rc::new(RefCell::new(size));

    let os_api = Box::new(FakeInputOutput {});

    // Create TiledPanes
    let connected_clients_set = Rc::new(RefCell::new(HashSet::from([client_id])));
    let mode_info = Rc::new(RefCell::new(HashMap::new()));
    let stacked_resize = Rc::new(RefCell::new(false));
    let session_is_mirrored = true;
    let draw_pane_frames = true;
    let default_mode_info = ModeInfo::default();

    let tiled_panes = TiledPanes::new(
        display_area.clone(),
        viewport.clone(),
        connected_clients_set.clone(),
        connected_clients.clone(),
        mode_info.clone(),
        character_cell_size.clone(),
        stacked_resize,
        session_is_mirrored,
        draw_pane_frames,
        default_mode_info.clone(),
        style.clone(),
        os_api.box_clone(),
        senders.clone(),
    );

    // Create FloatingPanes
    let floating_panes = FloatingPanes::new(
        display_area.clone(),
        viewport.clone(),
        connected_clients_set,
        connected_clients.clone(),
        mode_info,
        character_cell_size.clone(),
        session_is_mirrored,
        default_mode_info,
        style.clone(),
        os_api.box_clone(),
        senders.clone(),
    );

    let focus_pane_id = None;
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let explicitly_disable_kitty_keyboard_protocol = false;

    (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        tiled_panes,
        floating_panes,
        draw_pane_frames,
        focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    )
}

/// Creates fixtures with receivers for verifying messages sent to pty and plugin threads
#[allow(clippy::type_complexity)]
fn create_layout_applier_fixtures_with_receivers(
    size: Size,
) -> (
    Rc<RefCell<Viewport>>,
    ThreadSenders,
    Rc<RefCell<SixelImageStore>>,
    Rc<RefCell<LinkHandler>>,
    Rc<RefCell<Palette>>,
    Rc<RefCell<HashMap<usize, String>>>,
    Rc<RefCell<Option<SizeInPixels>>>,
    Rc<RefCell<HashMap<ClientId, bool>>>,
    Style,
    Rc<RefCell<Size>>,
    TiledPanes,
    FloatingPanes,
    bool,
    Option<PaneId>,
    Box<dyn ServerOsApi>,
    bool,
    bool,
    bool,
    bool,
    Receiver<(PtyInstruction, zellij_utils::errors::ErrorContext)>,
    Receiver<(PluginInstruction, zellij_utils::errors::ErrorContext)>,
) {
    let viewport = Rc::new(RefCell::new(Viewport {
        x: 0,
        y: 0,
        rows: size.rows,
        cols: size.cols,
    }));

    let (mock_pty_sender, mock_pty_receiver): ChannelWithContext<PtyInstruction> =
        channels::unbounded();
    let (mock_plugin_sender, mock_plugin_receiver): ChannelWithContext<PluginInstruction> =
        channels::unbounded();

    let mut senders = ThreadSenders::default().silently_fail_on_send();
    senders.replace_to_pty(SenderWithContext::new(mock_pty_sender));
    senders.replace_to_plugin(SenderWithContext::new(mock_plugin_sender));

    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let character_cell_size = Rc::new(RefCell::new(None));

    let client_id = 1;
    let mut connected_clients_map = HashMap::new();
    connected_clients_map.insert(client_id, false);
    let connected_clients = Rc::new(RefCell::new(connected_clients_map));

    let style = Style::default();
    let display_area = Rc::new(RefCell::new(size));

    let os_api = Box::new(FakeInputOutput {});

    // Create TiledPanes
    let connected_clients_set = Rc::new(RefCell::new(HashSet::from([client_id])));
    let mode_info = Rc::new(RefCell::new(HashMap::new()));
    let stacked_resize = Rc::new(RefCell::new(false));
    let session_is_mirrored = true;
    let draw_pane_frames = true;
    let default_mode_info = ModeInfo::default();

    let tiled_panes = TiledPanes::new(
        display_area.clone(),
        viewport.clone(),
        connected_clients_set.clone(),
        connected_clients.clone(),
        mode_info.clone(),
        character_cell_size.clone(),
        stacked_resize,
        session_is_mirrored,
        draw_pane_frames,
        default_mode_info.clone(),
        style.clone(),
        os_api.box_clone(),
        senders.clone(),
    );

    // Create FloatingPanes
    let floating_panes = FloatingPanes::new(
        display_area.clone(),
        viewport.clone(),
        connected_clients_set,
        connected_clients.clone(),
        mode_info,
        character_cell_size.clone(),
        session_is_mirrored,
        default_mode_info,
        style.clone(),
        os_api.box_clone(),
        senders.clone(),
    );

    let focus_pane_id = None;
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let explicitly_disable_kitty_keyboard_protocol = false;

    (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        tiled_panes,
        floating_panes,
        draw_pane_frames,
        focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        mock_pty_receiver,
        mock_plugin_receiver,
    )
}

/// Takes a snapshot of the current pane state for assertion
fn take_pane_state_snapshot(
    tiled_panes: &TiledPanes,
    floating_panes: &FloatingPanes,
    focus_pane_id: &Option<PaneId>,
    viewport: &Rc<RefCell<Viewport>>,
    display_area: &Rc<RefCell<Size>>,
) -> String {
    let mut output = String::new();

    // Viewport info
    let viewport_state = viewport.borrow();
    writeln!(
        &mut output,
        "VIEWPORT: x={}, y={}, cols={}, rows={}",
        viewport_state.x, viewport_state.y, viewport_state.cols, viewport_state.rows
    )
    .unwrap();

    let display_state = display_area.borrow();
    writeln!(
        &mut output,
        "DISPLAY: cols={}, rows={}",
        display_state.cols, display_state.rows
    )
    .unwrap();

    // Focus state
    writeln!(&mut output, "FOCUS: {:?}", focus_pane_id).unwrap();

    writeln!(&mut output).unwrap();

    // Tiled panes
    writeln!(&mut output, "TILED PANES ({})", tiled_panes.panes.len()).unwrap();
    let mut tiled_list: Vec<_> = tiled_panes.get_panes().collect();
    tiled_list.sort_by_key(|(id, _)| **id);

    for (pane_id, pane) in tiled_list {
        let geom = pane.position_and_size();
        let run = pane.invoked_with();
        let selectable = pane.selectable();

        writeln!(&mut output, "  {:?}:", pane_id).unwrap();
        writeln!(
            &mut output,
            "    geom: x={}, y={}, cols={}, rows={}",
            geom.x,
            geom.y,
            geom.cols.as_usize(),
            geom.rows.as_usize()
        )
        .unwrap();

        if let Some(logical_pos) = geom.logical_position {
            writeln!(&mut output, "    logical_position: {}", logical_pos).unwrap();
        }

        if let Some(stack_id) = geom.stacked {
            writeln!(&mut output, "    stacked: {}", stack_id).unwrap();
        }

        writeln!(&mut output, "    run: {}", format_run_instruction(run)).unwrap();

        writeln!(&mut output, "    selectable: {}", selectable).unwrap();
        writeln!(&mut output, "    title: {}", pane.current_title()).unwrap();
        writeln!(&mut output, "    borderless: {}", pane.borderless()).unwrap();

        writeln!(&mut output).unwrap();
    }

    // Floating panes
    if floating_panes.pane_ids().count() > 0 {
        writeln!(
            &mut output,
            "FLOATING PANES ({})",
            floating_panes.pane_ids().count()
        )
        .unwrap();
        let mut floating_list: Vec<_> = floating_panes.get_panes().collect();
        floating_list.sort_by_key(|(id, _)| **id);

        for (pane_id, pane) in floating_list {
            let geom = pane.position_and_size();
            let run = pane.invoked_with();

            writeln!(&mut output, "  {:?}:", pane_id).unwrap();
            writeln!(
                &mut output,
                "    geom: x={}, y={}, cols={}, rows={}",
                geom.x,
                geom.y,
                geom.cols.as_usize(),
                geom.rows.as_usize()
            )
            .unwrap();

            if let Some(logical_pos) = geom.logical_position {
                writeln!(&mut output, "    logical_position: {}", logical_pos).unwrap();
            }

            writeln!(&mut output, "    run: {}", format_run_instruction(run)).unwrap();
            writeln!(&mut output, "    pinned: {}", geom.is_pinned).unwrap();
            writeln!(&mut output, "    selectable: {}", pane.selectable()).unwrap();
            writeln!(&mut output, "    title: {}", pane.current_title()).unwrap();
            writeln!(&mut output, "    borderless: {}", pane.borderless()).unwrap();
            writeln!(&mut output).unwrap();
        }
    }

    output
}

/// Format a Run instruction as a human-readable string
fn format_run_instruction(run: &Option<Run>) -> String {
    match run {
        None => "None".to_string(),
        Some(Run::Command(cmd)) => {
            let mut s = format!("Command({})", cmd.command.display());
            if !cmd.args.is_empty() {
                s.push_str(&format!(" args={:?}", cmd.args));
            }
            if let Some(cwd) = &cmd.cwd {
                s.push_str(&format!(" cwd={:?}", cwd));
            }
            s
        },
        Some(Run::Plugin(plugin)) => {
            format!("Plugin({})", plugin.location_string())
        },
        Some(Run::Cwd(path)) => format!("Cwd({:?})", path),
        Some(Run::EditFile(path, line, cwd)) => {
            format!("EditFile({:?}, line={:?}, cwd={:?})", path, line, cwd)
        },
    }
}

/// Collect all close pane messages from the pty receiver
fn collect_close_pane_messages(
    pty_receiver: &Receiver<(PtyInstruction, zellij_utils::errors::ErrorContext)>,
) -> Vec<PaneId> {
    let mut closed_panes = Vec::new();
    while let Ok((instruction, _)) = pty_receiver.try_recv() {
        if let PtyInstruction::ClosePane(pane_id, _) = instruction {
            closed_panes.push(pane_id);
        }
    }
    closed_panes
}

/// Collect all unload plugin messages from the plugin receiver
fn collect_unload_plugin_messages(
    plugin_receiver: &Receiver<(PluginInstruction, zellij_utils::errors::ErrorContext)>,
) -> Vec<u32> {
    let mut unloaded_plugins = Vec::new();
    while let Ok((instruction, _)) = plugin_receiver.try_recv() {
        if let PluginInstruction::Unload(plugin_id) = instruction {
            unloaded_plugins.push(plugin_id);
        }
    }
    unloaded_plugins
}

#[test]
fn test_apply_empty_layout() {
    let kdl_layout = r#"
        layout {
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None, // blocking_terminal
    );

    let result = applier.apply_layout(
        tiled_layout,
        floating_layout,
        terminal_ids,
        vec![],         // new_floating_terminal_ids
        HashMap::new(), // new_plugin_ids
        1,              // client_id
    );

    assert!(result.is_ok());

    let snapshot = take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    );

    assert_snapshot!(snapshot);
}

#[test]
fn test_apply_simple_two_pane_layout() {
    let kdl_layout = r#"
        layout {
            pane
            pane
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    let snapshot = take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    );

    assert_snapshot!(snapshot);
}

#[test]
fn test_apply_three_pane_layout() {
    let kdl_layout = r#"
        layout {
            pane
            pane
            pane
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_horizontal_split_with_sizes() {
    let kdl_layout = r#"
        layout {
            pane split_direction="Horizontal" {
                pane size="30%"
                pane size="70%"
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_vertical_split_with_sizes() {
    let kdl_layout = r#"
        layout {
            pane split_direction="Vertical" {
                pane size="60%"
                pane size="40%"
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_nested_layout() {
    let kdl_layout = r#"
        layout {
            pane split_direction="Vertical" {
                pane size="60%"
                pane size="40%" split_direction="Horizontal" {
                    pane
                    pane
                }
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_focus() {
    let kdl_layout = r#"
        layout {
            pane
            pane focus=true
            pane
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Snapshot should show FOCUS: Some(Terminal(2))
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_commands() {
    let kdl_layout = r#"
        layout {
            pane command="htop"
            pane command="tail" {
                args "-f" "/var/log/syslog"
            }
            pane
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_named_panes() {
    let kdl_layout = r#"
        layout {
            pane name="editor"
            pane name="terminal"
            pane name="logs"
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_borderless_panes() {
    let kdl_layout = r#"
        layout {
            pane borderless=true
            pane
            pane borderless=true
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Snapshot should show viewport adjusted for borderless panes
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_floating_panes() {
    let kdl_layout = r#"
        layout {
            pane
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                }
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None)];
    let floating_terminal_ids = vec![(3, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    let should_show_floating = applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    assert_eq!(should_show_floating, true);

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_floating_pane_with_command() {
    let kdl_layout = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 50
                    height 25
                    command "htop"
                }
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None)];
    let floating_terminal_ids = vec![(2, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_mixed_tiled_and_floating_panes() {
    let kdl_layout = r#"
        layout {
            pane split_direction="Vertical" {
                pane size="60%" command="vim"
                pane size="40%" split_direction="Horizontal" {
                    pane name="terminal" focus=true
                    pane command="tail" {
                        args "-f" "/var/log/syslog"
                    }
                }
            }
            floating_panes {
                pane {
                    x 5
                    y 5
                    width 40
                    height 15
                    command "htop"
                    name "monitor"
                }
                pane {
                    x "50%"
                    y 10
                    width 45
                    height 20
                    name "notes"
                }
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];
    let floating_terminal_ids = vec![(4, None), (5, None)];

    let size = Size {
        cols: 150,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    let should_show_floating = applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    assert_eq!(should_show_floating, true);

    // Snapshot should show:
    // - 3 tiled panes with correct geometries, commands, and names
    // - 2 floating panes with correct positions and properties
    // - Focus on terminal pane (Terminal(2))
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_reapply_layout_exact_match() {
    // First apply initial layout
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Now reapply with commands in different positions
    let new_kdl = r#"
        layout {
            pane
            pane command="htop"
            pane command="vim"
        }
    "#;

    let (new_layout, _) = parse_kdl_layout(new_kdl);

    applier
        .apply_tiled_panes_layout_to_existing_panes(&new_layout)
        .unwrap();

    let snapshot = take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    );

    // Snapshot will show panes matched by command and repositioned
    assert_snapshot!(snapshot);
}

#[test]
fn test_reapply_layout_logical_position_match() {
    // Apply initial layout - 3 panes in horizontal split
    let initial_kdl = r#"
        layout {
            pane
            pane
            pane
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Reapply DIFFERENT layout - still 3 panes but with different split
    // This tests logical position matching (position 0, 1, 2) without exact command match
    let new_kdl = r#"
        layout {
            pane split_direction="Vertical" {
                pane size="50%"
                pane size="50%"
            }
            pane
        }
    "#;

    let (new_layout, _) = parse_kdl_layout(new_kdl);

    applier
        .apply_tiled_panes_layout_to_existing_panes(&new_layout)
        .unwrap();

    // Panes should be repositioned according to new layout structure
    // while being matched by their logical positions (0, 1, 2)
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_reapply_layout_with_more_positions() {
    // Apply initial layout with 2 panes
    let initial_kdl = r#"
        layout {
            pane
            pane
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Reapply with 4 positions (but we only have 2 panes)
    let new_kdl = r#"
        layout {
            pane
            pane
            pane
            pane
        }
    "#;

    let (new_layout, _) = parse_kdl_layout(new_kdl);

    applier
        .apply_tiled_panes_layout_to_existing_panes(&new_layout)
        .unwrap();

    // Should show 2 panes filling first 2 positions, remaining positions empty
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_reapply_floating_pane_layout() {
    // Apply initial layout
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                }
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];
    let floating_terminal_ids = vec![(2, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Reapply with different position
    let new_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                }
            }
        }
    "#;

    let (_, new_floating_layout) = parse_kdl_layout(new_kdl);

    applier
        .apply_floating_panes_layout_to_existing_panes(&new_floating_layout)
        .unwrap();

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_complex_nested_layout() {
    let kdl_layout = r#"
        layout {
            pane split_direction="Vertical" {
                pane size="25%"
                pane size="50%" split_direction="Horizontal" {
                    pane size="60%"
                    pane size="40%" split_direction="Vertical" {
                        pane
                        pane
                    }
                }
                pane size="25%"
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None), (3, None), (4, None), (5, None)];

    let size = Size {
        cols: 200,
        rows: 60,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_stacked_panes() {
    let kdl_layout = r#"
        layout {
            pane split_direction="Vertical" {
                pane size="70%" stacked=true {
                    pane name="editor-1"
                    pane name="editor-2"
                    pane name="editor-3"
                }
                pane size="30%"
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None), (3, None), (4, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Snapshot should show:
    // - 4 panes total (3 in stack + 1 regular)
    // - Stack panes should have stacked field set with same stack_id
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_multiple_stacks() {
    let kdl_layout = r#"
        layout {
            pane split_direction="Vertical" {
                pane size="50%" stacked=true {
                    pane name="left-1"
                    pane name="left-2"
                }
                pane size="50%" stacked=true {
                    pane name="right-1"
                    pane name="right-2"
                    pane name="right-3"
                }
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None), (3, None), (4, None), (5, None)];

    let size = Size {
        cols: 150,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Snapshot should show:
    // - 5 panes in 2 different stacks (different stack_ids)
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_plugin_panes() {
    let kdl_layout = r#"
        layout {
            pane
            pane {
                plugin location="zellij:tab-bar"
            }
            pane {
                plugin location="zellij:status-bar"
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None)];

    // Create plugin IDs - need to match the RunPluginOrAlias from the layout

    let mut new_plugin_ids = HashMap::new();

    // Create plugin aliases that match the layout
    let tab_bar_plugin = RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap();
    let status_bar_plugin =
        RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap();

    new_plugin_ids.insert(tab_bar_plugin, vec![100]);
    new_plugin_ids.insert(status_bar_plugin, vec![101]);

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            new_plugin_ids,
            1,
        )
        .unwrap();

    // Snapshot should show:
    // - 1 terminal pane
    // - 2 plugin panes with correct plugin locations
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_mixed_plugin_and_terminal_panes() {
    let kdl_layout = r#"
        layout {
            pane split_direction="Vertical" {
                pane size="20%" {
                    plugin location="file:///path/to/filebrowser.wasm"
                }
                pane size="60%" split_direction="Horizontal" {
                    pane command="vim"
                    pane command="cargo" {
                        args "watch" "-x" "test"
                    }
                }
                pane size="20%" {
                    plugin location="zellij:compact-bar"
                }
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None), (2, None)];

    let mut new_plugin_ids = HashMap::new();

    let filebrowser_plugin =
        RunPluginOrAlias::from_url("file:///path/to/filebrowser.wasm", &None, None, None).unwrap();
    let compact_bar_plugin =
        RunPluginOrAlias::from_url("zellij:compact-bar", &None, None, None).unwrap();

    new_plugin_ids.insert(filebrowser_plugin, vec![102]);
    new_plugin_ids.insert(compact_bar_plugin, vec![103]);

    let size = Size {
        cols: 200,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            tiled_layout,
            floating_layout,
            terminal_ids,
            vec![],
            new_plugin_ids,
            1,
        )
        .unwrap();

    // Snapshot should show:
    // - 2 terminal panes with commands
    // - 2 plugin panes with different locations
    // - Correct size distribution (20%, 60%, 20%)
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_apply_layout_with_missing_plugin_ids() {
    let kdl_layout = r#"
        layout {
            pane
            pane {
                plugin location="zellij:tab-bar"
            }
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![(1, None)];
    // Don't provide plugin IDs - empty HashMap
    let new_plugin_ids = HashMap::new();

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    let result = applier.apply_layout(
        tiled_layout,
        floating_layout,
        terminal_ids,
        vec![],
        new_plugin_ids,
        1,
    );

    // This should return an error - missing plugin ID
    assert!(result.is_err());
}

#[test]
fn test_apply_layout_with_excess_terminal_ids() {
    let kdl_layout = r#"
        layout {
            pane
            pane
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    // Provide more terminal IDs than needed
    let terminal_ids = vec![(1, None), (2, None), (3, None), (4, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    let result = applier.apply_layout(
        tiled_layout,
        floating_layout,
        terminal_ids,
        vec![],
        HashMap::new(),
        1,
    );

    assert!(result.is_ok());

    // Snapshot should show only 2 panes created
    // Excess IDs should be closed by the applier
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_layout_basic_with_both_tiled_and_floating() {
    // Setup: Apply initial layout with 2 tiled panes + 1 floating pane
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    command "tail"
                    args "-f" "/var/log/syslog"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None)];
    let floating_terminal_ids = vec![(3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Now override with different layout (2 tiled + 1 floating)
    let override_kdl = r#"
        layout {
            pane command="top"
            pane command="htop"
            floating_panes {
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                    command "watch"
                    args "df" "-h"
                }
            }
        }
    "#;

    let (override_tiled, override_floating) = parse_kdl_layout(override_kdl);
    let new_terminal_ids = vec![(4, None)];
    let new_floating_terminal_ids = vec![(5, None)];

    let retain_existing_terminal_panes = false;
    let retain_existing_plugin_panes = false;
    let should_show_floating = applier
        .override_layout(
            override_tiled,
            override_floating,
            new_terminal_ids,
            new_floating_terminal_ids,
            HashMap::new(),
            retain_existing_terminal_panes,
            retain_existing_plugin_panes,
            1,
        )
        .unwrap();

    // Should show floating panes
    assert_eq!(should_show_floating, true);

    // Verify close messages were sent for vim (Terminal(2)) and tail (Terminal(3))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 2);
    assert!(closed_panes.contains(&PaneId::Terminal(2))); // vim
    assert!(closed_panes.contains(&PaneId::Terminal(3))); // tail

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_layout_hide_floating_panes_true() {
    // Setup: Initial layout with floating panes
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];
    let floating_terminal_ids = vec![(2, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override with layout that has hide_floating_panes true
    let override_kdl = r#"
        layout {
            hide_floating_panes true
            pane
            pane
            floating_panes {
                pane {
                    x 15
                    y 15
                    width 45
                    height 22
                }
            }
        }
    "#;

    let (override_tiled, override_floating) = parse_kdl_layout(override_kdl);
    let new_terminal_ids = vec![(3, None)];
    let new_floating_terminal_ids = vec![(4, None)];

    let retain_existing_terminal_panes = false;
    let retain_existing_plugin_panes = false;
    let should_show_floating = applier
        .override_layout(
            override_tiled,
            override_floating,
            new_terminal_ids,
            new_floating_terminal_ids,
            HashMap::new(),
            retain_existing_terminal_panes,
            retain_existing_plugin_panes,
            1,
        )
        .unwrap();

    // Should NOT show floating panes because of hide_floating_panes
    assert_eq!(should_show_floating, false);

    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 0);

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_layout_show_floating_panes() {
    // Setup: Initial layout
    let initial_kdl = r#"
        layout {
            pane
            pane
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override with layout containing floating panes
    let override_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 20
                    y 10
                    width 50
                    height 30
                }
            }
        }
    "#;

    let (override_tiled, override_floating) = parse_kdl_layout(override_kdl);
    let new_floating_terminal_ids = vec![(3, None)];

    let retain_existing_terminal_panes = false;
    let retain_existing_plugin_panes = false;
    let should_show_floating = applier
        .override_layout(
            override_tiled,
            override_floating,
            vec![],
            new_floating_terminal_ids,
            HashMap::new(),
            retain_existing_terminal_panes,
            retain_existing_plugin_panes,
            1,
        )
        .unwrap();

    // Should show floating panes
    assert_eq!(should_show_floating, true);

    // Verify close message was sent for one tiled pane (Terminal(2))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 1);
    assert!(closed_panes.contains(&PaneId::Terminal(2)));

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

// ============================================================================
// Suite 2: override_tiled_panes_layout_for_existing_panes Tests
// ============================================================================

#[test]
fn test_override_tiled_exact_match_preservation_commands() {
    // Setup: Apply initial layout with 3 panes running different commands
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane command="tail" {
                args "-f" "/var/log/syslog"
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: New layout with only htop and vim in different positions
    let override_kdl = r#"
        layout {
            pane command="vim"
            pane command="htop"
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            vec![],
            &mut HashMap::new(),
            false,
            false,
            1,
        )
        .unwrap();

    // htop and vim panes should be preserved (same PaneIds: Terminal(1) and Terminal(2))
    // tail pane should be closed
    // Panes should be repositioned to new layout positions

    // Verify close message was sent for tail pane (Terminal(3))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 1);
    assert!(closed_panes.contains(&PaneId::Terminal(3))); // tail

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_tiled_exact_match_preservation_plugins() {
    // Setup: Initial layout with 2 terminal panes + 1 plugin pane
    let initial_kdl = r#"
        layout {
            pane
            pane
            pane {
                plugin location="zellij:tab-bar"
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None)];

    let mut initial_plugin_ids = HashMap::new();
    let tab_bar_plugin = RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap();
    initial_plugin_ids.insert(tab_bar_plugin.clone(), vec![100]);

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            initial_plugin_ids,
            1,
        )
        .unwrap();

    // Override: New layout with only the plugin pane
    let override_kdl = r#"
        layout {
            pane {
                plugin location="zellij:tab-bar"
            }
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            vec![],
            &mut HashMap::new(),
            false,
            false,
            1,
        )
        .unwrap();

    // Plugin pane should be preserved
    // Terminal panes should be closed
    // Total pane count is 1

    // Verify close messages were sent for both terminal panes (Terminal(1) and Terminal(2))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 2);
    assert!(closed_panes.contains(&PaneId::Terminal(1)));
    assert!(closed_panes.contains(&PaneId::Terminal(2)));

    // No plugins should be unloaded (plugin is preserved)
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_tiled_all_panes_closed_no_matches() {
    // Setup: 3 panes running htop, vim, tail
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane command="tail" {
                args "-f" "/var/log/syslog"
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: New layout with 3 completely different commands
    let override_kdl = r#"
        layout {
            pane command="cargo" {
                args "watch"
            }
            pane command="npm" {
                args "start"
            }
            pane command="python" {
                args "-m" "http.server"
            }
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);
    let new_terminal_ids = vec![(4, None), (5, None), (6, None)];

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            new_terminal_ids,
            &mut HashMap::new(),
            false,
            false,
            1,
        )
        .unwrap();

    // All original pane IDs gone (1, 2, 3 should not be present)
    // 3 new panes with new IDs (4, 5, 6)
    // Total pane count is 3

    // Verify close messages were sent for all original panes (Terminal(1), Terminal(2), Terminal(3))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 3);
    assert!(closed_panes.contains(&PaneId::Terminal(1))); // htop
    assert!(closed_panes.contains(&PaneId::Terminal(2))); // vim
    assert!(closed_panes.contains(&PaneId::Terminal(3))); // tail

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_tiled_mixed_some_matches_some_new() {
    // Setup: 2 panes - one running htop, one generic shell (no command)
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with htop, vim, and generic shell
    let override_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);
    let new_terminal_ids = vec![(3, None), (4, None)];

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            new_terminal_ids,
            &mut HashMap::new(),
            false,
            false,
            1,
        )
        .unwrap();

    // htop preserved with same ID (Terminal(1))
    // Original shell pane closed (generic shells are NOT exact matches)
    // 2 new panes created (vim and new shell)
    // Total pane count is 3

    // Verify close message was not sent for original shell pane (Terminal(2))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 0);

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_tiled_new_panes_for_unmatched_positions() {
    // Setup: 1 pane running htop
    let initial_kdl = r#"
        layout {
            pane command="htop"
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];

    let size = Size {
        cols: 150,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with 4 positions
    let override_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane
            pane
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);
    let new_terminal_ids = vec![(2, None), (3, None), (4, None)];

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            new_terminal_ids,
            &mut HashMap::new(),
            false,
            false,
            1,
        )
        .unwrap();

    // 1 original htop pane preserved (Terminal(1))
    // 3 new panes created (Terminal(2), Terminal(3), Terminal(4))
    // Total 4 panes
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_tiled_focus_on_new_pane() {
    // Setup: 2 panes, first one focused
    let initial_kdl = r#"
        layout {
            pane focus=true
            pane
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with 3 panes where second pane has focus=true
    let override_kdl = r#"
        layout {
            pane
            pane focus=true
            pane
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);
    let new_terminal_ids = vec![(3, None)];

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            new_terminal_ids,
            &mut HashMap::new(),
            false,
            false,
            1,
        )
        .unwrap();

    // focus_pane_id should point to the newly created middle pane (Terminal(3))
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_tiled_focus_when_focused_pane_closed() {
    // Setup: 3 panes, middle one focused running vim
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim" focus=true
            pane command="tail" {
                args "-f" "/var/log/syslog"
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with 2 panes running htop and cargo (not vim)
    let override_kdl = r#"
        layout {
            pane command="htop"
            pane command="cargo" {
                args "check"
            }
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);
    let new_terminal_ids = vec![(4, None)];

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            new_terminal_ids,
            &mut HashMap::new(),
            false,
            false,
            1,
        )
        .unwrap();

    // Focused pane (vim) no longer exists
    // Focus should be moved to one of the remaining panes

    // Verify close messages were sent for vim (Terminal(2)) and tail (Terminal(3))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 2);
    assert!(closed_panes.contains(&PaneId::Terminal(2))); // vim (focused)
    assert!(closed_panes.contains(&PaneId::Terminal(3))); // tail

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_tiled_empty_layout_closes_all() {
    // Setup: 3 panes running various commands
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane command="tail" {
                args "-f" "/var/log/syslog"
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Empty layout
    let override_kdl = r#"
        layout {
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            vec![],
            &mut HashMap::new(),
            false,
            false,
            1,
        )
        .unwrap();

    // No panes in snapshot
    // Pane count is 0

    // Verify close messages were sent for all panes (Terminal(1), Terminal(2), Terminal(3))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 3);
    assert!(closed_panes.contains(&PaneId::Terminal(1))); // htop
    assert!(closed_panes.contains(&PaneId::Terminal(2))); // vim
    assert!(closed_panes.contains(&PaneId::Terminal(3))); // tail

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

// ============================================================================
// Suite 3: override_floating_panes_layout_for_existing_panes Tests
// ============================================================================

#[test]
fn test_override_floating_exact_match_preservation() {
    // Setup: 2 floating panes running htop and vim
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    command "htop"
                }
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                    command "vim"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];
    let floating_terminal_ids = vec![(2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with htop at different x/y position
    let override_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 50
                    y 30
                    width 45
                    height 22
                    command "htop"
                }
            }
        }
    "#;

    let (_, override_floating) = parse_kdl_layout(override_kdl);

    applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            vec![],
            &mut HashMap::new(),
            false,
            false,
        )
        .unwrap();

    // htop pane preserved (Terminal(2)), repositioned
    // vim pane closed (Terminal(3))
    // Total floating pane count is 1

    // Verify close message was sent for vim pane (Terminal(3))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 1);
    assert!(closed_panes.contains(&PaneId::Terminal(3))); // vim

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_floating_all_closed_no_matches() {
    // Setup: 2 floating panes with specific commands
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    command "htop"
                }
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                    command "vim"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];
    let floating_terminal_ids = vec![(2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with different commands
    let override_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 15
                    y 15
                    width 45
                    height 22
                    command "top"
                }
                pane {
                    x 25
                    y 25
                    width 55
                    height 27
                    command "emacs"
                }
            }
        }
    "#;

    let (_, override_floating) = parse_kdl_layout(override_kdl);
    let new_floating_terminal_ids = vec![(4, None), (5, None)];

    applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            new_floating_terminal_ids,
            &mut HashMap::new(),
            false,
            false,
        )
        .unwrap();

    // Both original panes closed (IDs 2, 3 gone)
    // New panes created with new IDs (4, 5)
    // Pane count matches new layout (2 floating panes)

    // Verify close messages were sent for both floating panes (Terminal(2), Terminal(3))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 2);
    assert!(closed_panes.contains(&PaneId::Terminal(2))); // htop
    assert!(closed_panes.contains(&PaneId::Terminal(3))); // vim

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_floating_new_panes_created() {
    // Setup: 1 floating pane running htop
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    command "htop"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];
    let floating_terminal_ids = vec![(2, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with 3 floating panes: htop, vim, and generic shell
    let override_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    command "htop"
                }
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                    command "vim"
                }
                pane {
                    x 30
                    y 30
                    width 45
                    height 22
                }
            }
        }
    "#;

    let (_, override_floating) = parse_kdl_layout(override_kdl);
    let new_floating_terminal_ids = vec![(3, None), (4, None)];

    applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            new_floating_terminal_ids,
            &mut HashMap::new(),
            false,
            false,
        )
        .unwrap();

    // Original htop preserved (Terminal(2))
    // 2 new floating panes created (Terminal(3), Terminal(4))
    // Total 3 floating panes
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_floating_focus_handling() {
    // Setup: 2 floating panes, one focused
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    focus true
                }
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];
    let floating_terminal_ids = vec![(2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with 1 new pane that has focus=true
    let override_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 30
                    y 30
                    width 60
                    height 30
                    focus true
                }
            }
        }
    "#;

    let (_, override_floating) = parse_kdl_layout(override_kdl);
    let new_floating_terminal_ids = vec![(4, None)];

    applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            new_floating_terminal_ids,
            &mut HashMap::new(),
            false,
            false,
        )
        .unwrap();

    // Focus should be set on newly created pane (Terminal(4))

    // Verify close messages were sent for Terminal(3)
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 1);
    assert!(closed_panes.contains(&PaneId::Terminal(3)));

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_floating_position_and_size_update() {
    // Setup: 1 floating pane running htop at specific position
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    command "htop"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];
    let floating_terminal_ids = vec![(2, None)];

    let size = Size {
        cols: 120,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with htop at different position and size
    let override_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 50
                    y 30
                    width 60
                    height 30
                    command "htop"
                }
            }
        }
    "#;

    let (_, override_floating) = parse_kdl_layout(override_kdl);

    applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            vec![],
            &mut HashMap::new(),
            false,
            false,
        )
        .unwrap();

    // Same pane ID preserved (Terminal(2))
    // Geometry updated: x=50, y=30, cols=60, rows=30
    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_floating_return_value_has_panes() {
    // Setup: Empty floating panes
    let initial_kdl = r#"
        layout {
            pane
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
    ) = create_layout_applier_fixtures(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with 1 floating pane
    let override_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 20
                    y 10
                    width 50
                    height 30
                }
            }
        }
    "#;

    let (_, override_floating) = parse_kdl_layout(override_kdl);
    let new_floating_terminal_ids = vec![(2, None)];

    let has_floating_panes = applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            new_floating_terminal_ids,
            &mut HashMap::new(),
            false,
            false,
        )
        .unwrap();

    // Function should return true because layout has floating panes
    assert_eq!(has_floating_panes, true);

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_floating_return_value_no_panes() {
    // Setup: 1 floating pane
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];
    let floating_terminal_ids = vec![(2, None)];

    let size = Size {
        cols: 100,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Empty floating layout (no floating_panes block)
    let override_kdl = r#"
        layout {
            pane
        }
    "#;

    let (_, override_floating) = parse_kdl_layout(override_kdl);

    let has_floating_panes = applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            vec![],
            &mut HashMap::new(),
            false,
            false,
        )
        .unwrap();

    // Function should return false because layout has no floating panes
    assert_eq!(has_floating_panes, false);

    // Verify close message was sent for floating pane (Terminal(2))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 1);
    assert!(closed_panes.contains(&PaneId::Terminal(2)));

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

// ============================================================================
// Suite 4: Integration Tests
// ============================================================================

#[test]
fn test_override_full_tiled_and_floating_together() {
    // Setup: Initial layout with 3 tiled panes + 2 floating panes
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    command "cargo"
                    args "watch"
                }
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                    command "tail"
                    args "-f" "/var/log/syslog"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];
    let floating_terminal_ids = vec![(4, None), (5, None)];

    let size = Size {
        cols: 150,
        rows: 50,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with 2 tiled (htop, npm start) + 1 floating (cargo watch)
    let override_kdl = r#"
        layout {
            pane command="htop"
            pane command="npm" {
                args "start"
            }
            floating_panes {
                pane {
                    x 30
                    y 30
                    width 60
                    height 30
                    command "cargo"
                    args "watch"
                }
            }
        }
    "#;

    let (override_tiled, override_floating) = parse_kdl_layout(override_kdl);
    let new_terminal_ids = vec![(6, None)];

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            new_terminal_ids,
            &mut HashMap::new(),
            false,
            false,
            1,
        )
        .unwrap();

    applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            vec![],
            &mut HashMap::new(),
            false,
            false,
        )
        .unwrap();

    // Tiled: htop preserved (Terminal(1)), vim and shell closed, npm start created (Terminal(6))
    // Floating: cargo watch preserved (Terminal(4)), tail closed
    // Total: 2 tiled + 1 floating

    // Verify close messages were sent for vim (Terminal(2)), shell (Terminal(3)), and tail (Terminal(5))
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 3);
    assert!(closed_panes.contains(&PaneId::Terminal(2))); // vim
    assert!(closed_panes.contains(&PaneId::Terminal(3))); // shell
    assert!(closed_panes.contains(&PaneId::Terminal(5))); // tail

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_viewport_adjustment_with_borderless() {
    // Setup: Initial layout with borderless panes
    let initial_kdl = r#"
        layout {
            pane borderless=true
            pane
            pane borderless=true
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with different borderless configuration
    let override_kdl = r#"
        layout {
            pane
            pane borderless=true
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            vec![],
            &mut HashMap::new(),
            false,
            false,
            1,
        )
        .unwrap();

    // Viewport dimensions should be correctly adjusted for borderless panes

    // Verify close message was sent for at least the extra pane (Terminal(3))
    // Generic panes without commands don't match exactly, so all 3 may be closed
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert!(closed_panes.len() >= 1);
    assert!(closed_panes.contains(&PaneId::Terminal(3)));

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_tiled_retain_terminal_panes_partial_match() {
    // Test that when retain_existing_terminal_panes is true, terminal panes that don't match
    // the new layout are retained instead of being closed.
    // Setup: Apply initial layout with 3 panes running different commands
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane command="tail" {
                args "-f" "/var/log/syslog"
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: New layout with only vim and htop (tail is not in the new layout)
    let override_kdl = r#"
        layout {
            pane command="vim"
            pane command="htop"
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);

    // Override with retain_existing_terminal_panes = true
    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            vec![],
            &mut HashMap::new(),
            true, // retain_existing_terminal_panes
            false,
            1,
        )
        .unwrap();

    // With retain_existing_terminal_panes = true:
    // - NO terminal panes should be closed (not even tail)
    // - All 3 original terminals (Terminal(1), Terminal(2), Terminal(3)) should still exist
    // - vim and htop panes should match the new layout positions
    // - tail pane should be retained and added after the matched panes

    // Verify NO close messages were sent
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(
        closed_panes.len(),
        0,
        "No terminal panes should be closed when retain_existing_terminal_panes is true"
    );

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    // All 3 original terminals should still exist
    assert_eq!(
        tiled_panes.visible_panes_count(),
        3,
        "All 3 terminal panes should be retained"
    );

    // we're not asserting a snapshot here because adding panes uses unstable sorting and so the
    // test would be flaky
}

#[test]
fn test_override_tiled_retain_terminal_panes_no_matches() {
    // Test that when retain_existing_terminal_panes is true and NO panes match,
    // all original terminals are retained AND new terminals are created.
    // Setup: Apply initial layout with 3 panes
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane command="tail" {
                args "-f" "/var/log/syslog"
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: New layout with completely different commands (no matches)
    let override_kdl = r#"
        layout {
            pane command="cargo"
            pane command="npm"
            pane command="python"
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);
    let new_terminal_ids = vec![(4, None), (5, None), (6, None)];

    // Override with retain_existing_terminal_panes = true
    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            new_terminal_ids,
            &mut HashMap::new(),
            true, // retain_existing_terminal_panes
            false,
            1,
        )
        .unwrap();

    // With retain_existing_terminal_panes = true and no matches:
    // - NO terminal panes should be closed
    // - All 3 original terminals (Terminal(1), Terminal(2), Terminal(3)) should still exist
    // - 3 NEW terminals (Terminal(4), Terminal(5), Terminal(6)) should be created
    // - Total: 6 terminal panes

    // Verify NO close messages were sent
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(
        closed_panes.len(),
        0,
        "No terminal panes should be closed when retain_existing_terminal_panes is true"
    );

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    // Should have 6 total panes (3 original + 3 new)
    assert_eq!(
        tiled_panes.visible_panes_count(),
        6,
        "Should have 6 terminal panes (3 original + 3 new)"
    );

    // we're not asserting a snapshot here because adding panes uses unstable sorting and so the
    // test would be flaky
}

#[test]
fn test_override_floating_retain_terminal_panes_partial_match() {
    // Test that when retain_existing_terminal_panes is true, floating terminal panes
    // that don't match the new layout are retained instead of being closed.
    // Setup: 1 tiled pane + 2 floating panes running htop and vim
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    command "htop"
                }
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                    command "vim"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];
    let floating_terminal_ids = vec![(2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with only htop (vim is not in the new layout)
    let override_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 50
                    y 30
                    width 45
                    height 22
                    command "htop"
                }
            }
        }
    "#;

    let (_, override_floating) = parse_kdl_layout(override_kdl);

    // Override with retain_existing_terminal_panes = true
    applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            vec![],
            &mut HashMap::new(),
            true, // retain_existing_terminal_panes
            false,
        )
        .unwrap();

    // With retain_existing_terminal_panes = true:
    // - NO terminal panes should be closed (not even vim)
    // - Both original floating terminals (Terminal(2), Terminal(3)) should still exist
    // - htop pane should match the new layout position
    // - vim pane should be retained as a floating pane

    // Verify NO close messages were sent
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(
        closed_panes.len(),
        0,
        "No terminal panes should be closed when retain_existing_terminal_panes is true"
    );

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    // Both floating panes should still exist
    assert_eq!(
        floating_panes.visible_panes_count(),
        2,
        "Both floating terminal panes should be retained"
    );

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_floating_retain_terminal_panes_no_matches() {
    // Test that when retain_existing_terminal_panes is true and NO floating panes match,
    // all original floating terminals are retained AND new floating terminals are created.
    // Setup: 1 tiled pane + 2 floating panes running htop and vim
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    command "htop"
                }
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                    command "vim"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None)];
    let floating_terminal_ids = vec![(2, None), (3, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: Layout with 2 different floating panes (top and emacs)
    let override_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 30
                    y 30
                    width 40
                    height 20
                    command "top"
                }
                pane {
                    x 40
                    y 40
                    width 50
                    height 25
                    command "emacs"
                }
            }
        }
    "#;

    let (_, override_floating) = parse_kdl_layout(override_kdl);
    let new_floating_terminal_ids = vec![(4, None), (5, None)];

    // Override with retain_existing_terminal_panes = true
    applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            new_floating_terminal_ids,
            &mut HashMap::new(),
            true, // retain_existing_terminal_panes
            false,
        )
        .unwrap();

    // With retain_existing_terminal_panes = true and no matches:
    // - NO terminal panes should be closed
    // - Both original floating terminals (Terminal(2), Terminal(3)) should still exist
    // - 2 NEW floating terminals (Terminal(4), Terminal(5)) should be created
    // - Total: 4 floating panes

    // Verify NO close messages were sent
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(
        closed_panes.len(),
        0,
        "No terminal panes should be closed when retain_existing_terminal_panes is true"
    );

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    // Should have 4 floating panes (2 original + 2 new)
    assert_eq!(
        floating_panes.visible_panes_count(),
        4,
        "Should have 4 floating panes (2 original + 2 new)"
    );

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_mixed_retain_terminal_panes_both_tiled_and_floating() {
    // Test that when retain_existing_terminal_panes is true, both tiled and floating
    // terminal panes that don't match the new layout are retained.
    // Setup: Apply initial layout with 3 tiled panes + 1 floating pane
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane command="tail" {
                args "-f" "/var/log/syslog"
            }
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    command "watch"
                    args "df" "-h"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None), (3, None)];
    let floating_terminal_ids = vec![(4, None)];

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            floating_terminal_ids,
            HashMap::new(),
            1,
        )
        .unwrap();

    // Override: 2 tiled (top, htop) + 1 floating (different watch command)
    let override_kdl = r#"
        layout {
            pane command="top"
            pane command="htop"
            floating_panes {
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                    command "watch"
                    args "free" "-h"
                }
            }
        }
    "#;

    let (override_tiled, override_floating) = parse_kdl_layout(override_kdl);
    let new_terminal_ids = vec![(5, None)];
    let new_floating_terminal_ids = vec![(6, None)];

    let retain_existing_terminal_panes = true;
    let retain_existing_plugin_panes = false;
    applier
        .override_layout(
            override_tiled,
            override_floating,
            new_terminal_ids,
            new_floating_terminal_ids,
            HashMap::new(),
            retain_existing_terminal_panes,
            retain_existing_plugin_panes,
            1,
        )
        .unwrap();

    // With retain_existing_terminal_panes = true:
    // - NO terminal panes should be closed (neither tiled nor floating)
    // - Original tiled terminals: Terminal(1), Terminal(2), Terminal(3) retained
    //   (htop matches, so it's reused; vim and tail are retained)
    // - Original floating terminal: Terminal(4) retained
    // - New tiled terminal: Terminal(5) created (for top, htop matches)
    // - New floating terminal: Terminal(6) created (different watch command)

    // Verify NO close messages were sent
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(
        closed_panes.len(),
        0,
        "No terminal panes should be closed when retain_existing_terminal_panes is true"
    );

    // No plugins should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert!(unloaded_plugins.is_empty());

    // Check tiled pane count (3 original + 1 new = 4)
    assert_eq!(
        tiled_panes.visible_panes_count(),
        4,
        "Should have 4 tiled panes (3 original + 1 new)"
    );

    // Check floating pane count (1 original + 1 new = 2)
    assert_eq!(
        floating_panes.visible_panes_count(),
        2,
        "Should have 2 floating panes (1 original + 1 new)"
    );

    // we're not asserting a snapshot here because adding panes uses unstable sorting and so the
    // test would be flaky
}

#[test]
fn test_override_retain_terminal_but_close_plugin_panes() {
    // Test that when retain_existing_terminal_panes is true, the flag ONLY affects
    // terminal panes and plugin panes are still closed as normal.
    // Setup: Initial layout with 2 terminal panes + 1 plugin pane
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane {
                plugin location="zellij:tab-bar"
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);
    let terminal_ids = vec![(1, None), (2, None)];

    let mut initial_plugin_ids = HashMap::new();
    let tab_bar_plugin = RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap();
    initial_plugin_ids.insert(tab_bar_plugin.clone(), vec![100]);

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            initial_plugin_ids,
            1,
        )
        .unwrap();

    // Override: New layout with only htop (vim and tab-bar plugin not in new layout)
    let override_kdl = r#"
        layout {
            pane command="htop"
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);

    // Override with retain_existing_terminal_panes = true
    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            vec![],
            &mut HashMap::new(),
            true, // retain_existing_terminal_panes
            false,
            1,
        )
        .unwrap();

    // With retain_existing_terminal_panes = true:
    // - Terminal panes NOT closed: Terminal(1) matched (htop), Terminal(2) retained (vim)
    // - Plugin pane IS closed: Plugin(100) unloaded (tab-bar)
    // - Both terminals should exist, but plugin should be gone

    // No terminal panes should be closed
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(
        closed_panes.len(),
        0,
        "No terminal panes should be closed when retain_existing_terminal_panes is true"
    );

    // Plugin pane should be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert_eq!(
        unloaded_plugins.len(),
        1,
        "Plugin pane should be unloaded even with retain_existing_terminal_panes"
    );
    assert!(
        unloaded_plugins.contains(&100),
        "Tab-bar plugin (100) should be unloaded"
    );

    // Both terminals should exist
    assert_eq!(
        tiled_panes.visible_panes_count(),
        2,
        "Both terminal panes should be retained"
    );

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

#[test]
fn test_override_tiled_retain_plugin_panes_partial_match() {
    // Verify that when retain_existing_plugin_panes = true, plugin panes that don't match
    // the new layout are retained instead of being closed
    let initial_kdl = r#"
        layout {
            pane
            pane {
                plugin location="zellij:tab-bar"
            }
            pane {
                plugin location="zellij:status-bar"
            }
            pane {
                plugin location="file:///path/to/custom.wasm"
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);

    let terminal_ids = vec![(1, None)];

    let mut initial_plugin_ids = HashMap::new();
    let tab_bar = RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap();
    let status_bar = RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap();
    let custom =
        RunPluginOrAlias::from_url("file:///path/to/custom.wasm", &None, None, None).unwrap();
    initial_plugin_ids.insert(tab_bar, vec![100]);
    initial_plugin_ids.insert(status_bar, vec![101]);
    initial_plugin_ids.insert(custom, vec![102]);

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            initial_plugin_ids,
            1,
        )
        .unwrap();

    let override_kdl = r#"
        layout {
            pane
            pane {
                plugin location="zellij:tab-bar"
            }
            pane {
                plugin location="zellij:status-bar"
            }
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            vec![],
            &mut HashMap::new(),
            false,
            true,
            1,
        )
        .unwrap();

    // Verify NO plugin panes were unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert_eq!(
        unloaded_plugins.len(),
        0,
        "No plugin panes should be unloaded when retain_existing_plugin_panes is true"
    );

    // No terminals should be closed
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert!(closed_panes.is_empty());

    // All 4 panes should exist (1 terminal + 3 plugins)
    assert_eq!(
        tiled_panes.visible_panes_count(),
        4,
        "All plugin panes should be retained"
    );
}

#[test]
fn test_override_tiled_retain_plugin_panes_no_matches() {
    // When NO plugins match the new layout, all original plugins are retained AND new plugins are created
    let initial_kdl = r#"
        layout {
            pane
            pane {
                plugin location="zellij:tab-bar"
            }
            pane {
                plugin location="zellij:status-bar"
            }
            pane {
                plugin location="zellij:compact-bar"
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);

    let terminal_ids = vec![(1, None)];

    let mut initial_plugin_ids = HashMap::new();
    let tab_bar = RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap();
    let status_bar = RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap();
    let compact_bar = RunPluginOrAlias::from_url("zellij:compact-bar", &None, None, None).unwrap();
    initial_plugin_ids.insert(tab_bar, vec![100]);
    initial_plugin_ids.insert(status_bar, vec![101]);
    initial_plugin_ids.insert(compact_bar, vec![102]);

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            initial_plugin_ids,
            1,
        )
        .unwrap();

    let override_kdl = r#"
        layout {
            pane
            pane {
                plugin location="file:///path/to/plugin1.wasm"
            }
            pane {
                plugin location="file:///path/to/plugin2.wasm"
            }
            pane {
                plugin location="file:///path/to/plugin3.wasm"
            }
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);

    let mut override_plugin_ids = HashMap::new();
    let plugin1 =
        RunPluginOrAlias::from_url("file:///path/to/plugin1.wasm", &None, None, None).unwrap();
    let plugin2 =
        RunPluginOrAlias::from_url("file:///path/to/plugin2.wasm", &None, None, None).unwrap();
    let plugin3 =
        RunPluginOrAlias::from_url("file:///path/to/plugin3.wasm", &None, None, None).unwrap();
    override_plugin_ids.insert(plugin1, vec![103]);
    override_plugin_ids.insert(plugin2, vec![104]);
    override_plugin_ids.insert(plugin3, vec![105]);

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            vec![],
            &mut override_plugin_ids,
            false,
            true,
            1,
        )
        .unwrap();

    // Verify NO plugin panes were unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert_eq!(
        unloaded_plugins.len(),
        0,
        "No plugin panes should be unloaded when retain_existing_plugin_panes is true"
    );

    // Total: 1 terminal + 6 plugins (3 original + 3 new)
    assert_eq!(
        tiled_panes.visible_panes_count(),
        7,
        "All original plugins retained and new plugins created"
    );
}

#[test]
fn test_override_floating_retain_plugin_panes_partial_match() {
    // Floating plugin panes that don't match the new layout are retained
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    plugin location="zellij:tab-bar"
                }
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                    plugin location="zellij:status-bar"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);

    let terminal_ids = vec![(1, None)];

    let mut initial_plugin_ids = HashMap::new();
    let tab_bar = RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap();
    let status_bar = RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap();
    initial_plugin_ids.insert(tab_bar, vec![100]);
    initial_plugin_ids.insert(status_bar, vec![101]);

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            initial_plugin_ids,
            1,
        )
        .unwrap();

    let override_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 15
                    y 15
                    width 45
                    height 22
                    plugin location="zellij:tab-bar"
                }
            }
        }
    "#;

    let (_, override_floating) = parse_kdl_layout(override_kdl);

    applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            vec![],
            &mut HashMap::new(),
            false,
            true,
        )
        .unwrap();

    // Verify NO plugin panes were unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert_eq!(
        unloaded_plugins.len(),
        0,
        "No floating plugin panes should be unloaded when retain_existing_plugin_panes is true"
    );

    // No terminals should be closed
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert!(closed_panes.is_empty());

    // Both floating plugins should still exist
    assert_eq!(
        floating_panes.visible_panes_count(),
        2,
        "Both floating plugin panes should be retained"
    );

    // Verify tiled pane exists
    assert_eq!(tiled_panes.visible_panes_count(), 1);
}

#[test]
fn test_override_floating_retain_plugin_panes_no_matches() {
    // All original floating plugins are retained AND new floating plugins are created when there are no matches
    let initial_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    plugin location="zellij:tab-bar"
                }
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                    plugin location="zellij:status-bar"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);

    let terminal_ids = vec![(1, None)];

    let mut initial_plugin_ids = HashMap::new();
    let tab_bar = RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap();
    let status_bar = RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap();
    initial_plugin_ids.insert(tab_bar, vec![100]);
    initial_plugin_ids.insert(status_bar, vec![101]);

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            initial_plugin_ids,
            1,
        )
        .unwrap();

    let override_kdl = r#"
        layout {
            pane
            floating_panes {
                pane {
                    x 15
                    y 15
                    width 45
                    height 22
                    plugin location="file:///path/to/plugin1.wasm"
                }
                pane {
                    x 25
                    y 25
                    width 55
                    height 27
                    plugin location="file:///path/to/plugin2.wasm"
                }
            }
        }
    "#;

    let (_, override_floating) = parse_kdl_layout(override_kdl);

    let mut override_plugin_ids = HashMap::new();
    let plugin1 =
        RunPluginOrAlias::from_url("file:///path/to/plugin1.wasm", &None, None, None).unwrap();
    let plugin2 =
        RunPluginOrAlias::from_url("file:///path/to/plugin2.wasm", &None, None, None).unwrap();
    override_plugin_ids.insert(plugin1, vec![102]);
    override_plugin_ids.insert(plugin2, vec![103]);

    applier
        .override_floating_panes_layout_for_existing_panes(
            &override_floating,
            vec![],
            &mut override_plugin_ids,
            false,
            true,
        )
        .unwrap();

    // Verify NO plugin panes were unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert_eq!(
        unloaded_plugins.len(),
        0,
        "No floating plugin panes should be unloaded when retain_existing_plugin_panes is true"
    );

    // Total: 4 floating plugins (2 original + 2 new)
    assert_eq!(
        floating_panes.visible_panes_count(),
        4,
        "All original floating plugins retained and new plugins created"
    );

    // 1 tiled terminal pane
    assert_eq!(tiled_panes.visible_panes_count(), 1);
}

#[test]
fn test_override_mixed_retain_plugin_panes_both_tiled_and_floating() {
    // Both tiled and floating plugin panes are retained when the flag is true
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane {
                plugin location="zellij:tab-bar"
            }
            floating_panes {
                pane {
                    x 10
                    y 10
                    width 40
                    height 20
                    plugin location="zellij:status-bar"
                }
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);

    let terminal_ids = vec![(1, None), (2, None)];

    let mut initial_plugin_ids = HashMap::new();
    let tab_bar = RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap();
    let status_bar = RunPluginOrAlias::from_url("zellij:status-bar", &None, None, None).unwrap();
    initial_plugin_ids.insert(tab_bar, vec![100]);
    initial_plugin_ids.insert(status_bar, vec![101]);

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            initial_plugin_ids,
            1,
        )
        .unwrap();

    let override_kdl = r#"
        layout {
            pane command="htop"
            pane {
                plugin location="zellij:tab-bar"
            }
            pane {
                plugin location="zellij:compact-bar"
            }
            floating_panes {
                pane {
                    x 20
                    y 20
                    width 50
                    height 25
                    plugin location="file:///path/to/custom.wasm"
                }
            }
        }
    "#;

    let (override_tiled, override_floating) = parse_kdl_layout(override_kdl);

    let mut override_plugin_ids = HashMap::new();
    let compact_bar = RunPluginOrAlias::from_url("zellij:compact-bar", &None, None, None).unwrap();
    let custom =
        RunPluginOrAlias::from_url("file:///path/to/custom.wasm", &None, None, None).unwrap();
    override_plugin_ids.insert(compact_bar, vec![102]);
    override_plugin_ids.insert(custom, vec![103]);

    let retain_existing_terminal_panes = false;
    let retain_existing_plugin_panes = true;
    applier
        .override_layout(
            override_tiled,
            override_floating,
            vec![],
            vec![],
            override_plugin_ids,
            retain_existing_terminal_panes,
            retain_existing_plugin_panes,
            1,
        )
        .unwrap();

    // Verify NO plugin panes were unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert_eq!(
        unloaded_plugins.len(),
        0,
        "No plugin panes should be unloaded when retain_existing_plugin_panes is true"
    );

    // vim terminal should be closed (doesn't match layout)
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(closed_panes.len(), 1, "vim terminal should be closed");
    assert!(
        closed_panes.contains(&PaneId::Terminal(2)),
        "Terminal(2) (vim) should be closed"
    );

    // Verify plugins exist (exact counts may vary based on where retained panes land)
    assert!(
        tiled_panes.visible_panes_count() >= 2,
        "At least htop and matched plugins"
    );
    assert!(
        floating_panes.visible_panes_count() >= 1,
        "At least one floating plugin"
    );
}

#[test]
fn test_override_retain_plugin_but_close_terminal_panes() {
    // Verify that retain_existing_plugin_panes = true ONLY affects plugin panes; terminals are still closed normally
    let initial_kdl = r#"
        layout {
            pane command="htop"
            pane command="vim"
            pane {
                plugin location="zellij:tab-bar"
            }
        }
    "#;

    let (initial_tiled, initial_floating) = parse_kdl_layout(initial_kdl);

    let terminal_ids = vec![(1, None), (2, None)];

    let mut initial_plugin_ids = HashMap::new();
    let tab_bar = RunPluginOrAlias::from_url("zellij:tab-bar", &None, None, None).unwrap();
    initial_plugin_ids.insert(tab_bar, vec![100]);

    let size = Size {
        cols: 120,
        rows: 40,
    };
    let (
        viewport,
        senders,
        sixel_image_store,
        link_handler,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        character_cell_size,
        connected_clients,
        style,
        display_area,
        mut tiled_panes,
        mut floating_panes,
        draw_pane_frames,
        mut focus_pane_id,
        os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        pty_receiver,
        plugin_receiver,
    ) = create_layout_applier_fixtures_with_receivers(size);

    let mut applier = LayoutApplier::new(
        &viewport,
        &senders,
        &sixel_image_store,
        &link_handler,
        &terminal_emulator_colors,
        &terminal_emulator_color_codes,
        &character_cell_size,
        &connected_clients,
        &style,
        &display_area,
        &mut tiled_panes,
        &mut floating_panes,
        draw_pane_frames,
        &mut focus_pane_id,
        &os_api,
        debug,
        arrow_fonts,
        styled_underlines,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );

    applier
        .apply_layout(
            initial_tiled,
            initial_floating,
            terminal_ids,
            vec![],
            initial_plugin_ids,
            1,
        )
        .unwrap();

    let override_kdl = r#"
        layout {
            pane command="htop"
        }
    "#;

    let (override_tiled, _) = parse_kdl_layout(override_kdl);

    applier
        .override_tiled_panes_layout_for_existing_panes(
            &override_tiled,
            vec![],
            &mut HashMap::new(),
            false,
            true,
            1,
        )
        .unwrap();

    // vim terminal should be closed
    let closed_panes = collect_close_pane_messages(&pty_receiver);
    assert_eq!(
        closed_panes.len(),
        1,
        "Terminal pane should be closed even with retain_existing_plugin_panes"
    );
    assert!(
        closed_panes.contains(&PaneId::Terminal(2)),
        "vim (Terminal(2)) should be closed"
    );

    // Plugin should NOT be unloaded
    let unloaded_plugins = collect_unload_plugin_messages(&plugin_receiver);
    assert_eq!(
        unloaded_plugins.len(),
        0,
        "No plugin panes should be unloaded"
    );

    // Final panes: htop + tab-bar = 2
    assert_eq!(
        tiled_panes.visible_panes_count(),
        2,
        "htop and tab-bar should remain"
    );

    assert_snapshot!(take_pane_state_snapshot(
        &tiled_panes,
        &floating_panes,
        &focus_pane_id,
        &viewport,
        &display_area,
    ));
}

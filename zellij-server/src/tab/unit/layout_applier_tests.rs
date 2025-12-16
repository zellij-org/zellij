use crate::tab::layout_applier::LayoutApplier;
use crate::panes::sixel::SixelImageStore;
use crate::panes::{FloatingPanes, TiledPanes};
use crate::panes::{LinkHandler, PaneId};
use crate::{
    os_input_output::{AsyncReader, Pid, ServerOsApi},
    thread_bus::ThreadSenders,
    ClientId,
};
use insta::assert_snapshot;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::rc::Rc;
use std::fmt::Write;

use interprocess::local_socket::LocalSocketStream;
use zellij_utils::{
    data::{ModeInfo, Palette, Style},
    errors::prelude::*,
    input::command::{RunCommand, TerminalAction},
    input::layout::{FloatingPaneLayout, Layout, Run, TiledPaneLayout},
    ipc::{ClientToServerMsg, IpcReceiverWithContext, ServerToClientMsg},
    pane_size::{Size, SizeInPixels, Viewport},
    input::layout::RunPluginOrAlias,
    channels::{self, SenderWithContext},
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


    let (mock_plugin_sender, _mock_plugin_receiver) =
        channels::unbounded();
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
        }
        Some(Run::Plugin(plugin)) => {
            format!("Plugin({})", plugin.location_string())
        }
        Some(Run::Cwd(path)) => format!("Cwd({:?})", path),
        Some(Run::EditFile(path, line, cwd)) => {
            format!("EditFile({:?}, line={:?}, cwd={:?})", path, line, cwd)
        }
    }
}

#[test]
fn test_apply_empty_layout() {
    let kdl_layout = r#"
        layout {
        }
    "#;

    let (tiled_layout, floating_layout) = parse_kdl_layout(kdl_layout);
    let terminal_ids = vec![];

    let size = Size { cols: 100, rows: 50 };
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
        vec![],          // new_floating_terminal_ids
        HashMap::new(),  // new_plugin_ids
        1,               // client_id
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

    let size = Size { cols: 100, rows: 50 };
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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

    let size = Size { cols: 100, rows: 50 };
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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

    let size = Size { cols: 100, rows: 50 };
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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

    let size = Size { cols: 100, rows: 50 };
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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

    let size = Size { cols: 100, rows: 50 };
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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

    let size = Size { cols: 100, rows: 50 };
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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

    let size = Size { cols: 100, rows: 50 };
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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

    let size = Size { cols: 100, rows: 50 };
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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

    let size = Size { cols: 100, rows: 50 };
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

    let size = Size { cols: 100, rows: 50 };
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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

    let size = Size { cols: 100, rows: 50 };
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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

    let size = Size { cols: 100, rows: 50 };
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1)
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
    let tab_bar_plugin = RunPluginOrAlias::from_url(
        "zellij:tab-bar",
        &None,
        None,
        None,
    ).unwrap();
    let status_bar_plugin = RunPluginOrAlias::from_url(
        "zellij:status-bar",
        &None,
        None,
        None,
    ).unwrap();

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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], new_plugin_ids, 1)
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

    let filebrowser_plugin = RunPluginOrAlias::from_url(
        "file:///path/to/filebrowser.wasm",
        &None,
        None,
        None,
    ).unwrap();
    let compact_bar_plugin = RunPluginOrAlias::from_url(
        "zellij:compact-bar",
        &None,
        None,
        None,
    ).unwrap();

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
        .apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], new_plugin_ids, 1)
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

    let size = Size { cols: 100, rows: 50 };
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

    let size = Size { cols: 100, rows: 50 };
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

    let result = applier.apply_layout(tiled_layout, floating_layout, terminal_ids, vec![], HashMap::new(), 1);

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

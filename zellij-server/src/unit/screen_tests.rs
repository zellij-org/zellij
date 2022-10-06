use super::{screen_thread_main, CopyOptions, Screen, ScreenInstruction};
use crate::panes::PaneId;
use crate::{
    channels::SenderWithContext,
    os_input_output::{AsyncReader, Pid, ServerOsApi},
    route::route_action,
    thread_bus::Bus,
    ClientId, ServerInstruction, SessionMetaData, ThreadSenders,
};
use insta::assert_snapshot;
use std::path::PathBuf;
use zellij_utils::cli::CliAction;
use zellij_utils::errors::ErrorContext;
use zellij_utils::input::actions::{Action, Direction, ResizeDirection};
use zellij_utils::input::command::TerminalAction;
use zellij_utils::input::layout::{PaneLayout, SplitDirection};
use zellij_utils::input::options::Options;
use zellij_utils::ipc::IpcReceiverWithContext;
use zellij_utils::pane_size::{Size, SizeInPixels};

use crate::pty_writer::PtyWriteInstruction;
use std::env::set_var;
use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};

use crate::{pty::PtyInstruction, wasm_vm::PluginInstruction};
use zellij_utils::ipc::PixelDimensions;
use zellij_utils::nix;
use zellij_utils::{
    channels::{self, ChannelWithContext, Receiver},
    data::{InputMode, ModeInfo, Palette, PluginCapabilities},
    interprocess::local_socket::LocalSocketStream,
    ipc::{ClientAttributes, ClientToServerMsg, ServerToClientMsg},
};

use crate::panes::grid::Grid;
use crate::panes::link_handler::LinkHandler;
use crate::panes::sixel::SixelImageStore;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zellij_utils::vte;

// TODO: deduplicate with identical function in tab_integration_tests
fn take_snapshot_and_cursor_coordinates(
    ansi_instructions: &str,
    rows: usize,
    columns: usize,
    palette: Palette,
) -> (Option<(usize, usize)>, String) {
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
    (grid.cursor_coordinates(), format!("{:?}", grid))
}

fn take_snapshots_and_cursor_coordinates_from_render_events<'a>(
    all_events: impl Iterator<Item = &'a ServerInstruction>,
    screen_size: Size,
) -> Vec<(Option<(usize, usize)>, String)> {
    let snapshots: Vec<(Option<(usize, usize)>, String)> = all_events
        .filter_map(|server_instruction| {
            match server_instruction {
                ServerInstruction::Render(output) => {
                    if let Some(output) = output {
                        // note this only takes a snapshot of the first client!
                        let raw_snapshot = output.get(&1).unwrap();
                        let snapshot = take_snapshot_and_cursor_coordinates(
                            raw_snapshot,
                            screen_size.rows,
                            screen_size.cols,
                            Palette::default(),
                        );
                        Some(snapshot)
                    } else {
                        None
                    }
                },
                _ => None,
            }
        })
        .collect();
    snapshots
}

fn send_cli_action_to_server(
    session_metadata: &SessionMetaData,
    cli_action: CliAction,
    mock_screen: &mut MockScreen,
    client_id: ClientId,
) {
    let os_input = Box::new(mock_screen.os_input.clone());
    let to_server = mock_screen.to_server.clone();
    let actions = Action::actions_from_cli(cli_action).unwrap();
    for action in actions {
        route_action(
            action,
            &session_metadata,
            &*os_input,
            &to_server.clone(),
            client_id,
        );
    }
}

#[derive(Clone, Default)]
struct FakeInputOutput {
    fake_filesystem: Arc<Mutex<HashMap<String, String>>>,
    server_to_client_messages: Arc<Mutex<HashMap<ClientId, Vec<ServerToClientMsg>>>>,
}

impl ServerOsApi for FakeInputOutput {
    fn set_terminal_size_using_terminal_id(&self, _terminal_id: u32, _cols: u16, _rows: u16) {
        // noop
    }
    fn spawn_terminal(
        &self,
        _file_to_open: TerminalAction,
        _quit_db: Box<dyn Fn(PaneId) + Send>,
        _default_editor: Option<PathBuf>,
    ) -> Result<(u32, RawFd, RawFd), &'static str> {
        unimplemented!()
    }
    fn read_from_tty_stdout(&self, _fd: RawFd, _buf: &mut [u8]) -> Result<usize, nix::Error> {
        unimplemented!()
    }
    fn async_file_reader(&self, _fd: RawFd) -> Box<dyn AsyncReader> {
        unimplemented!()
    }
    fn write_to_tty_stdin(&self, _id: u32, _buf: &[u8]) -> Result<usize, nix::Error> {
        unimplemented!()
    }
    fn tcdrain(&self, _id: u32) -> Result<(), nix::Error> {
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
    fn send_to_client(
        &self,
        client_id: ClientId,
        msg: ServerToClientMsg,
    ) -> Result<(), &'static str> {
        self.server_to_client_messages
            .lock()
            .unwrap()
            .entry(client_id)
            .or_insert_with(Vec::new)
            .push(msg);
        Ok(())
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
    fn write_to_file(&mut self, contents: String, filename: Option<String>) {
        if let Some(filename) = filename {
            self.fake_filesystem
                .lock()
                .unwrap()
                .insert(filename, contents);
        }
    }
}

fn create_new_screen(size: Size) -> Screen {
    let mut bus: Bus<ScreenInstruction> = Bus::empty();
    let fake_os_input = FakeInputOutput::default();
    bus.os_input = Some(Box::new(fake_os_input));
    let client_attributes = ClientAttributes {
        size,
        ..Default::default()
    };
    let max_panes = None;
    let mut mode_info = ModeInfo::default();
    mode_info.session_name = Some("zellij-test".into());
    let draw_pane_frames = false;
    let session_is_mirrored = true;
    let copy_options = CopyOptions::default();

    Screen::new(
        bus,
        &client_attributes,
        max_panes,
        mode_info,
        draw_pane_frames,
        session_is_mirrored,
        copy_options,
    )
}

struct MockScreen {
    pub main_client_id: u16,
    pub pty_receiver: Option<Receiver<(PtyInstruction, ErrorContext)>>,
    pub pty_writer_receiver: Option<Receiver<(PtyWriteInstruction, ErrorContext)>>,
    pub screen_receiver: Option<Receiver<(ScreenInstruction, ErrorContext)>>,
    pub server_receiver: Option<Receiver<(ServerInstruction, ErrorContext)>>,
    pub plugin_receiver: Option<Receiver<(PluginInstruction, ErrorContext)>>,
    pub to_screen: SenderWithContext<ScreenInstruction>,
    pub to_pty: SenderWithContext<PtyInstruction>,
    pub to_plugin: SenderWithContext<PluginInstruction>,
    pub to_server: SenderWithContext<ServerInstruction>,
    pub to_pty_writer: SenderWithContext<PtyWriteInstruction>,
    pub os_input: FakeInputOutput,
    pub client_attributes: ClientAttributes,
    pub config_options: Options,
    pub session_metadata: SessionMetaData,
}

impl MockScreen {
    pub fn run(&mut self, initial_layout: Option<PaneLayout>) -> std::thread::JoinHandle<()> {
        let config_options = self.config_options.clone();
        let client_attributes = self.client_attributes.clone();
        let screen_bus = Bus::new(
            vec![self.screen_receiver.take().unwrap()],
            None,
            Some(&self.to_pty.clone()),
            Some(&self.to_plugin.clone()),
            Some(&self.to_server.clone()),
            Some(&self.to_pty_writer.clone()),
            Some(Box::new(self.os_input.clone())),
        )
        .should_silently_fail();
        let screen_thread = std::thread::Builder::new()
            .name("screen_thread".to_string())
            .spawn(move || {
                set_var("ZELLIJ_SESSION_NAME", "zellij-test");
                screen_thread_main(
                    screen_bus,
                    None,
                    client_attributes,
                    Box::new(config_options),
                )
                .expect("TEST")
            })
            .unwrap();
        let pane_layout = initial_layout.unwrap_or_default();
        let pane_count = pane_layout.extract_run_instructions().len();
        let mut pane_ids = vec![];
        for i in 0..pane_count {
            pane_ids.push(i as u32);
        }
        let _ = self.to_screen.send(ScreenInstruction::NewTab(
            pane_layout,
            pane_ids,
            self.main_client_id,
        ));
        screen_thread
    }
    pub fn new_tab(&mut self, tab_layout: PaneLayout) {
        let pane_count = tab_layout.extract_run_instructions().len();
        let mut pane_ids = vec![];
        for i in 0..pane_count {
            pane_ids.push(i as u32);
        }
        let _ = self.to_screen.send(ScreenInstruction::NewTab(
            tab_layout,
            pane_ids,
            self.main_client_id,
        ));
    }
    pub fn teardown(&mut self, threads: Vec<std::thread::JoinHandle<()>>) {
        let _ = self.to_pty.send(PtyInstruction::Exit);
        let _ = self.to_pty_writer.send(PtyWriteInstruction::Exit);
        let _ = self.to_screen.send(ScreenInstruction::Exit);
        let _ = self.to_server.send(ServerInstruction::KillSession);
        let _ = self.to_plugin.send(PluginInstruction::Exit);
        for thread in threads {
            let _ = thread.join();
        }
    }
    pub fn clone_session_metadata(&self) -> SessionMetaData {
        // hack that only clones the clonable parts of SessionMetaData
        SessionMetaData {
            senders: self.session_metadata.senders.clone(),
            capabilities: self.session_metadata.capabilities.clone(),
            client_attributes: self.session_metadata.client_attributes.clone(),
            default_shell: self.session_metadata.default_shell.clone(),
            screen_thread: None,
            pty_thread: None,
            wasm_thread: None,
            pty_writer_thread: None,
        }
    }
}

impl MockScreen {
    pub fn new(size: Size) -> Self {
        let (to_server, server_receiver): ChannelWithContext<ServerInstruction> =
            channels::bounded(50);
        let to_server = SenderWithContext::new(to_server);

        let (to_screen, screen_receiver): ChannelWithContext<ScreenInstruction> =
            channels::unbounded();
        let to_screen = SenderWithContext::new(to_screen);

        let (to_plugin, plugin_receiver): ChannelWithContext<PluginInstruction> =
            channels::unbounded();
        let to_plugin = SenderWithContext::new(to_plugin);
        let (to_pty, pty_receiver): ChannelWithContext<PtyInstruction> = channels::unbounded();
        let to_pty = SenderWithContext::new(to_pty);

        let (to_pty_writer, pty_writer_receiver): ChannelWithContext<PtyWriteInstruction> =
            channels::unbounded();
        let to_pty_writer = SenderWithContext::new(to_pty_writer);

        let client_attributes = ClientAttributes {
            size,
            ..Default::default()
        };
        let capabilities = PluginCapabilities {
            arrow_fonts: Default::default(),
        };

        let session_metadata = SessionMetaData {
            senders: ThreadSenders {
                to_screen: Some(to_screen.clone()),
                to_pty: Some(to_pty.clone()),
                to_plugin: Some(to_plugin.clone()),
                to_pty_writer: Some(to_pty_writer.clone()),
                to_server: Some(to_server.clone()),
                should_silently_fail: true,
            },
            capabilities,
            default_shell: None,
            client_attributes: client_attributes.clone(),
            screen_thread: None,
            pty_thread: None,
            wasm_thread: None,
            pty_writer_thread: None,
        };

        let os_input = FakeInputOutput::default();
        let config_options = Options::default();
        let main_client_id = 1;
        MockScreen {
            main_client_id,
            pty_receiver: Some(pty_receiver),
            pty_writer_receiver: Some(pty_writer_receiver),
            screen_receiver: Some(screen_receiver),
            server_receiver: Some(server_receiver),
            plugin_receiver: Some(plugin_receiver),
            to_screen,
            to_pty,
            to_plugin,
            to_server,
            to_pty_writer,
            os_input,
            client_attributes,
            config_options,
            session_metadata,
        }
    }
}

macro_rules! log_actions_in_thread {
    ( $arc_mutex_log:expr, $exit_event:path, $receiver:expr ) => {
        std::thread::Builder::new()
            .name("pty_writer_thread".to_string())
            .spawn({
                let log = $arc_mutex_log.clone();
                move || loop {
                    let (event, _err_ctx) = $receiver
                        .recv()
                        .expect("failed to receive event on channel");
                    match event {
                        $exit_event => {
                            log.lock().unwrap().push(event);
                            break;
                        },
                        _ => {
                            log.lock().unwrap().push(event);
                        },
                    }
                }
            })
            .unwrap()
    };
}

fn new_tab(screen: &mut Screen, pid: u32) {
    let client_id = 1;
    screen
        .new_tab(PaneLayout::default(), vec![pid], client_id)
        .expect("TEST");
}

#[test]
fn open_new_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);

    assert_eq!(screen.tabs.len(), 2, "Screen now has two tabs");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab switched to new tab"
    );
}

#[test]
pub fn switch_to_prev_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    screen.switch_tab_prev(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab switched to previous tab"
    );
}

#[test]
pub fn switch_to_next_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    screen.switch_tab_prev(1).expect("TEST");
    screen.switch_tab_next(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab switched to next tab"
    );
}

#[test]
pub fn close_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    screen.close_tab(1).expect("TEST");

    assert_eq!(screen.tabs.len(), 1, "Only one tab left");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab switched to previous tab"
    );
}

#[test]
pub fn close_the_middle_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    new_tab(&mut screen, 3);
    screen.switch_tab_prev(1).expect("TEST");
    screen.close_tab(1).expect("TEST");

    assert_eq!(screen.tabs.len(), 2, "Two tabs left");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab switched to previous tab"
    );
}

#[test]
fn move_focus_left_at_left_screen_edge_changes_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    new_tab(&mut screen, 3);
    screen.switch_tab_prev(1).expect("TEST");
    screen.move_focus_left_or_previous_tab(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab switched to previous"
    );
}

#[test]
fn move_focus_right_at_right_screen_edge_changes_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    new_tab(&mut screen, 3);
    screen.switch_tab_prev(1).expect("TEST");
    screen.move_focus_right_or_next_tab(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab switched to next"
    );
}

#[test]
pub fn toggle_to_previous_tab_simple() {
    let position_and_size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(position_and_size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    screen.go_to_tab(1, 1).expect("TEST");
    screen.go_to_tab(2, 1).expect("TEST");

    screen.toggle_tab(1).expect("TEST");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab toggler to previous tab"
    );

    screen.toggle_tab(1).expect("TEST");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab toggler to previous tab"
    );
}

#[test]
pub fn toggle_to_previous_tab_create_tabs_only() {
    let position_and_size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(position_and_size);

    new_tab(&mut screen, 1);
    new_tab(&mut screen, 2);
    new_tab(&mut screen, 3);

    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1],
        "Tab history is invalid"
    );

    screen.toggle_tab(1).expect("TEST");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab toggler to previous tab"
    );
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 2],
        "Tab history is invalid"
    );

    screen.toggle_tab(1).expect("TEST");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab toggler to previous tab"
    );
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1],
        "Tab history is invalid"
    );

    screen.toggle_tab(1).expect("TEST");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab toggler to previous tab"
    );
}

#[test]
pub fn toggle_to_previous_tab_delete() {
    let position_and_size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(position_and_size);

    new_tab(&mut screen, 1); // 0
    new_tab(&mut screen, 2); // 1
    new_tab(&mut screen, 3); // 2
    new_tab(&mut screen, 4); // 3

    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1, 2],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        3,
        "Active tab toggler to previous tab"
    );

    screen.toggle_tab(1).expect("TEST");
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1, 3],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab toggler to previous tab"
    );

    screen.toggle_tab(1).expect("TEST");
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1, 2],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        3,
        "Active tab toggler to previous tab"
    );

    screen.switch_tab_prev(1).expect("TEST");
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 1, 3],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab toggler to previous tab"
    );
    screen.switch_tab_prev(1).expect("TEST");
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 3, 2],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab toggler to previous tab"
    );

    screen.close_tab(1).expect("TEST");
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 3],
        "Tab history is invalid"
    );
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab toggler to previous tab"
    );

    screen.toggle_tab(1).expect("TEST");
    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab toggler to previous tab"
    );
    assert_eq!(
        screen.tab_history.get(&1).unwrap(),
        &[0, 2],
        "Tab history is invalid"
    );
}

#[test]
fn switch_to_tab_with_fullscreen() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1);
    {
        let active_tab = screen.get_active_tab_mut(1).unwrap();
        active_tab.new_pane(PaneId::Terminal(2), Some(1));
        active_tab.toggle_active_pane_fullscreen(1);
    }
    new_tab(&mut screen, 2);

    screen.switch_tab_prev(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab switched to previous"
    );
    assert_eq!(
        screen
            .get_active_tab(1)
            .unwrap()
            .get_active_pane_id(1)
            .unwrap(),
        PaneId::Terminal(2),
        "Active pane is still the fullscreen pane"
    );
}

#[test]
fn update_screen_pixel_dimensions() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);
    let initial_pixel_dimensions = screen.pixel_dimensions;
    screen.update_pixel_dimensions(PixelDimensions {
        character_cell_size: Some(SizeInPixels {
            height: 10,
            width: 5,
        }),
        text_area_size: None,
    });
    let pixel_dimensions_after_first_update = screen.pixel_dimensions;
    screen.update_pixel_dimensions(PixelDimensions {
        character_cell_size: None,
        text_area_size: Some(SizeInPixels {
            height: 100,
            width: 50,
        }),
    });
    let pixel_dimensions_after_second_update = screen.pixel_dimensions;
    screen.update_pixel_dimensions(PixelDimensions {
        character_cell_size: None,
        text_area_size: None,
    });
    let pixel_dimensions_after_third_update = screen.pixel_dimensions;
    assert_eq!(
        initial_pixel_dimensions,
        PixelDimensions {
            character_cell_size: None,
            text_area_size: None
        },
        "Initial pixel dimensions empty"
    );
    assert_eq!(
        pixel_dimensions_after_first_update,
        PixelDimensions {
            character_cell_size: Some(SizeInPixels {
                height: 10,
                width: 5
            }),
            text_area_size: None
        },
        "character_cell_size updated properly",
    );
    assert_eq!(
        pixel_dimensions_after_second_update,
        PixelDimensions {
            character_cell_size: Some(SizeInPixels {
                height: 10,
                width: 5
            }),
            text_area_size: Some(SizeInPixels {
                height: 100,
                width: 50,
            }),
        },
        "text_area_size updated properly without overriding character_cell_size",
    );
    assert_eq!(
        pixel_dimensions_after_third_update,
        PixelDimensions {
            character_cell_size: Some(SizeInPixels {
                height: 10,
                width: 5
            }),
            text_area_size: Some(SizeInPixels {
                height: 100,
                width: 50,
            }),
        },
        "empty update does not delete existing data",
    );
}

#[test]
fn attach_after_first_tab_closed() {
    // ensure https://github.com/zellij-org/zellij/issues/1645 is fixed
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1);
    {
        let active_tab = screen.get_active_tab_mut(1).unwrap();
        active_tab.new_pane(PaneId::Terminal(2), Some(1));
        active_tab.toggle_active_pane_fullscreen(1);
    }
    new_tab(&mut screen, 2);

    screen.close_tab_at_index(0).expect("TEST");
    screen.remove_client(1).expect("TEST");
    screen.add_client(1).expect("TEST");
}

// Following are tests for sending CLI actions
// these tests are only partially relevant to Screen
// and are included here for two reasons:
// 1. The best way to "integration test" these is combining the "screen_thread_main" and
//    "route_action" functions and mocking everything around them
// 2. These inadvertently also test many parts of Screen that are not tested elsewhere

#[test]
pub fn send_cli_write_chars_action_to_screen() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let pty_writer_receiver = mock_screen.pty_writer_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(None);
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_writer_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyWriteInstruction::Exit,
        pty_writer_receiver
    );
    let cli_action = CliAction::WriteChars {
        chars: "input from the cli".into(),
    };
    send_cli_action_to_server(&session_metadata, cli_action, &mut mock_screen, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_writer_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_write_action_to_screen() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let pty_writer_receiver = mock_screen.pty_writer_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(None);
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_writer_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyWriteInstruction::Exit,
        pty_writer_receiver
    );
    let cli_action = CliAction::Write {
        bytes: vec![102, 111, 111],
    };
    send_cli_action_to_server(&session_metadata, cli_action, &mut mock_screen, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_writer_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_resize_action_to_screen() {
    let size = Size { cols: 80, rows: 20 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let pty_writer_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let resize_cli_action = CliAction::Resize {
        resize_direction: ResizeDirection::Left,
    };
    send_cli_action_to_server(
        &session_metadata,
        resize_cli_action,
        &mut mock_screen,
        client_id,
    );
    mock_screen.teardown(vec![pty_writer_thread, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_focus_next_pane_action() {
    let size = Size { cols: 80, rows: 20 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let focus_next_pane_action = CliAction::FocusNextPane;
    send_cli_action_to_server(
        &session_metadata,
        focus_next_pane_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (cursor_coordinates, _snapshot) in snapshots {
        // here we assert he cursor_coordinates to let us know if we switched the pane focus
        assert_snapshot!(format!("{:?}", cursor_coordinates));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_focus_previous_pane_action() {
    let size = Size { cols: 80, rows: 20 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let focus_next_pane_action = CliAction::FocusPreviousPane;
    send_cli_action_to_server(
        &session_metadata,
        focus_next_pane_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (cursor_coordinates, _snapshot) in snapshots {
        // here we assert he cursor_coordinates to let us know if we switched the pane focus
        assert_snapshot!(format!("{:?}", cursor_coordinates));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_move_focus_pane_action() {
    let size = Size { cols: 80, rows: 20 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let move_focus_action = CliAction::MoveFocus {
        direction: Direction::Right,
    };
    send_cli_action_to_server(
        &session_metadata,
        move_focus_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (cursor_coordinates, _snapshot) in snapshots {
        // here we assert he cursor_coordinates to let us know if we switched the pane focus
        assert_snapshot!(format!("{:?}", cursor_coordinates));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_move_focus_or_tab_pane_action() {
    let size = Size { cols: 80, rows: 20 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let move_focus_action = CliAction::MoveFocusOrTab {
        direction: Direction::Right,
    };
    send_cli_action_to_server(
        &session_metadata,
        move_focus_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (cursor_coordinates, _snapshot) in snapshots {
        // here we assert he cursor_coordinates to let us know if we switched the pane focus
        assert_snapshot!(format!("{:?}", cursor_coordinates));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_move_pane_action() {
    let size = Size { cols: 80, rows: 20 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let cli_action = CliAction::MovePane {
        direction: Direction::Right,
    };
    send_cli_action_to_server(&session_metadata, cli_action, &mut mock_screen, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_dump_screen_action() {
    let size = Size { cols: 80, rows: 20 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let cli_action = CliAction::DumpScreen {
        path: PathBuf::from("/tmp/foo"),
    };
    let _ = mock_screen.to_screen.send(ScreenInstruction::PtyBytes(
        0,
        "fill pane up with something".as_bytes().to_vec(),
    ));
    send_cli_action_to_server(&session_metadata, cli_action, &mut mock_screen, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_thread, screen_thread]);
    assert_snapshot!(format!(
        "{:?}",
        *mock_screen.os_input.fake_filesystem.lock().unwrap()
    ));
}

#[test]
pub fn send_cli_edit_scrollback_action() {
    let size = Size { cols: 80, rows: 20 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_receiver = mock_screen.pty_receiver.take().unwrap();
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_action = CliAction::EditScrollback;
    let _ = mock_screen.to_screen.send(ScreenInstruction::PtyBytes(
        0,
        "fill pane up with something".as_bytes().to_vec(),
    ));
    send_cli_action_to_server(&session_metadata, cli_action, &mut mock_screen, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![pty_thread, screen_thread]);
    let dumped_file_name = mock_screen
        .os_input
        .fake_filesystem
        .lock()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let mut found_instruction = false;
    for instruction in received_pty_instructions.lock().unwrap().iter() {
        if let PtyInstruction::OpenInPlaceEditor(scrollback_contents_file, terminal_id, client_id) =
            instruction
        {
            assert_eq!(scrollback_contents_file, &PathBuf::from(&dumped_file_name));
            assert_eq!(terminal_id, &Some(1));
            assert_eq!(client_id, &1);
            found_instruction = true;
        }
    }
    assert!(found_instruction);
}

#[test]
pub fn send_cli_scroll_up_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let cli_action = CliAction::ScrollUp;
    let mut pane_contents = String::new();
    for i in 0..20 {
        pane_contents.push_str(&format!("fill pane up with something {}\n\r", i));
    }
    let _ = mock_screen.to_screen.send(ScreenInstruction::PtyBytes(
        0,
        pane_contents.as_bytes().to_vec(),
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // we send two actions here because only the last line in the pane is empty, so one action
    // won't show in a render
    send_cli_action_to_server(
        &session_metadata,
        cli_action.clone(),
        &mut mock_screen,
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        cli_action.clone(),
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_scroll_down_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let scroll_up_cli_action = CliAction::ScrollUp;
    let scroll_down_cli_action = CliAction::ScrollDown;
    let mut pane_contents = String::new();
    for i in 0..20 {
        pane_contents.push_str(&format!("fill pane up with something {}\n\r", i));
    }
    let _ = mock_screen.to_screen.send(ScreenInstruction::PtyBytes(
        0,
        pane_contents.as_bytes().to_vec(),
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // scroll up some
    send_cli_action_to_server(
        &session_metadata,
        scroll_up_cli_action.clone(),
        &mut mock_screen,
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        scroll_up_cli_action.clone(),
        &mut mock_screen,
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        scroll_up_cli_action.clone(),
        &mut mock_screen,
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        scroll_up_cli_action.clone(),
        &mut mock_screen,
        client_id,
    );

    // scroll down some
    send_cli_action_to_server(
        &session_metadata,
        scroll_down_cli_action.clone(),
        &mut mock_screen,
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        scroll_down_cli_action.clone(),
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_scroll_to_bottom_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let scroll_up_cli_action = CliAction::ScrollUp;
    let scroll_to_bottom_action = CliAction::ScrollToBottom;
    let mut pane_contents = String::new();
    for i in 0..20 {
        pane_contents.push_str(&format!("fill pane up with something {}\n\r", i));
    }
    let _ = mock_screen.to_screen.send(ScreenInstruction::PtyBytes(
        0,
        pane_contents.as_bytes().to_vec(),
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // scroll up some
    send_cli_action_to_server(
        &session_metadata,
        scroll_up_cli_action.clone(),
        &mut mock_screen,
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        scroll_up_cli_action.clone(),
        &mut mock_screen,
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        scroll_up_cli_action.clone(),
        &mut mock_screen,
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        scroll_up_cli_action.clone(),
        &mut mock_screen,
        client_id,
    );

    // scroll to bottom
    send_cli_action_to_server(
        &session_metadata,
        scroll_to_bottom_action.clone(),
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_page_scroll_up_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let page_scroll_up_action = CliAction::PageScrollUp;
    let mut pane_contents = String::new();
    for i in 0..20 {
        pane_contents.push_str(&format!("fill pane up with something {}\n\r", i));
    }
    let _ = mock_screen.to_screen.send(ScreenInstruction::PtyBytes(
        0,
        pane_contents.as_bytes().to_vec(),
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    send_cli_action_to_server(
        &session_metadata,
        page_scroll_up_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_page_scroll_down_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let page_scroll_up_action = CliAction::PageScrollUp;
    let page_scroll_down_action = CliAction::PageScrollDown;
    let mut pane_contents = String::new();
    for i in 0..20 {
        pane_contents.push_str(&format!("fill pane up with something {}\n\r", i));
    }
    let _ = mock_screen.to_screen.send(ScreenInstruction::PtyBytes(
        0,
        pane_contents.as_bytes().to_vec(),
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));

    // scroll up some
    send_cli_action_to_server(
        &session_metadata,
        page_scroll_up_action.clone(),
        &mut mock_screen,
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        page_scroll_up_action.clone(),
        &mut mock_screen,
        client_id,
    );

    // scroll down
    send_cli_action_to_server(
        &session_metadata,
        page_scroll_down_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_half_page_scroll_up_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let half_page_scroll_up_action = CliAction::HalfPageScrollUp;
    let mut pane_contents = String::new();
    for i in 0..20 {
        pane_contents.push_str(&format!("fill pane up with something {}\n\r", i));
    }
    let _ = mock_screen.to_screen.send(ScreenInstruction::PtyBytes(
        0,
        pane_contents.as_bytes().to_vec(),
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    send_cli_action_to_server(
        &session_metadata,
        half_page_scroll_up_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_half_page_scroll_down_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let half_page_scroll_up_action = CliAction::HalfPageScrollUp;
    let half_page_scroll_down_action = CliAction::HalfPageScrollDown;
    let mut pane_contents = String::new();
    for i in 0..20 {
        pane_contents.push_str(&format!("fill pane up with something {}\n\r", i));
    }
    let _ = mock_screen.to_screen.send(ScreenInstruction::PtyBytes(
        0,
        pane_contents.as_bytes().to_vec(),
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));

    // scroll up some
    send_cli_action_to_server(
        &session_metadata,
        half_page_scroll_up_action.clone(),
        &mut mock_screen,
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        half_page_scroll_up_action.clone(),
        &mut mock_screen,
        client_id,
    );

    // scroll down
    send_cli_action_to_server(
        &session_metadata,
        half_page_scroll_down_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_toggle_full_screen_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let toggle_full_screen_action = CliAction::ToggleFullscreen;
    send_cli_action_to_server(
        &session_metadata,
        toggle_full_screen_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_toggle_pane_frames_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let toggle_pane_frames_action = CliAction::TogglePaneFrames;
    send_cli_action_to_server(
        &session_metadata,
        toggle_pane_frames_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_toggle_active_tab_sync_action() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let pty_writer_receiver = mock_screen.pty_writer_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_writer_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyWriteInstruction::Exit,
        pty_writer_receiver
    );
    let cli_toggle_active_tab_sync_action = CliAction::ToggleActiveSyncTab;
    let cli_write_action = CliAction::Write {
        bytes: vec![102, 111, 111],
    };
    send_cli_action_to_server(
        &session_metadata,
        cli_toggle_active_tab_sync_action,
        &mut mock_screen,
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        cli_write_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_writer_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_new_pane_action_with_default_parameters() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let pty_receiver = mock_screen.pty_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_new_pane_action = CliAction::NewPane {
        direction: None,
        command: None,
        cwd: None,
        floating: None,
    };
    send_cli_action_to_server(
        &session_metadata,
        cli_new_pane_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_new_pane_action_with_split_direction() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let pty_receiver = mock_screen.pty_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_new_pane_action = CliAction::NewPane {
        direction: Some(Direction::Right),
        command: None,
        cwd: None,
        floating: None,
    };
    send_cli_action_to_server(
        &session_metadata,
        cli_new_pane_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_new_pane_action_with_command_and_cwd() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let pty_receiver = mock_screen.pty_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_new_pane_action = CliAction::NewPane {
        direction: Some(Direction::Right),
        command: Some("htop".into()),
        cwd: Some("/some/folder".into()),
        floating: None,
    };
    send_cli_action_to_server(
        &session_metadata,
        cli_new_pane_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_edit_action_with_default_parameters() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let pty_receiver = mock_screen.pty_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_edit_action = CliAction::Edit {
        file: PathBuf::from("/file/to/edit"),
        direction: None,
        line_number: None,
        floating: None,
    };
    send_cli_action_to_server(
        &session_metadata,
        cli_edit_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_edit_action_with_line_number() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let pty_receiver = mock_screen.pty_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_edit_action = CliAction::Edit {
        file: PathBuf::from("/file/to/edit"),
        direction: None,
        line_number: Some(100),
        floating: None,
    };
    send_cli_action_to_server(
        &session_metadata,
        cli_edit_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_edit_action_with_split_direction() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let pty_receiver = mock_screen.pty_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_edit_action = CliAction::Edit {
        file: PathBuf::from("/file/to/edit"),
        direction: Some(Direction::Down),
        line_number: None,
        floating: None,
    };
    send_cli_action_to_server(
        &session_metadata,
        cli_edit_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_switch_mode_action() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout));
    let cli_switch_mode = CliAction::SwitchMode {
        input_mode: InputMode::Locked,
    };
    send_cli_action_to_server(
        &session_metadata,
        cli_switch_mode,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![screen_thread]);
    assert_snapshot!(format!(
        "{:?}",
        *mock_screen
            .os_input
            .server_to_client_messages
            .lock()
            .unwrap()
    ));
}

#[test]
pub fn send_cli_toggle_pane_embed_or_float() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let toggle_pane_embed_or_floating = CliAction::TogglePaneEmbedOrFloating;
    // first time to float
    send_cli_action_to_server(
        &session_metadata,
        toggle_pane_embed_or_floating.clone(),
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    // second time to embed
    send_cli_action_to_server(
        &session_metadata,
        toggle_pane_embed_or_floating.clone(),
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_toggle_floating_panes() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let toggle_pane_embed_or_floating = CliAction::TogglePaneEmbedOrFloating;
    let toggle_floating_panes = CliAction::ToggleFloatingPanes;
    // float the focused pane
    send_cli_action_to_server(
        &session_metadata,
        toggle_pane_embed_or_floating,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    // toggle floating panes (will hide the floated pane from the previous action)
    send_cli_action_to_server(
        &session_metadata,
        toggle_floating_panes.clone(),
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    // toggle floating panes (will show the floated pane)
    send_cli_action_to_server(
        &session_metadata,
        toggle_floating_panes.clone(),
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_close_pane_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let close_pane_action = CliAction::ClosePane;
    send_cli_action_to_server(
        &session_metadata,
        close_pane_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_instruction, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_new_tab_action_default_params() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_receiver = mock_screen.pty_receiver.take().unwrap();
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let new_tab_action = CliAction::NewTab {
        name: None,
        layout: None,
    };
    send_cli_action_to_server(
        &session_metadata,
        new_tab_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![pty_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_new_tab_action_with_name_and_layout() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_receiver = mock_screen.pty_receiver.take().unwrap();
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let new_tab_action = CliAction::NewTab {
        name: Some("my-awesome-tab-name".into()),
        layout: Some(PathBuf::from(format!(
            "{}/src/unit/fixtures/layout-with-three-panes.kdl",
            env!("CARGO_MANIFEST_DIR")
        ))),
    };
    send_cli_action_to_server(
        &session_metadata,
        new_tab_action,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![pty_thread, screen_thread]);
    let new_tab_instruction = received_pty_instructions
        .lock()
        .unwrap()
        .iter()
        .find(|i| {
            if let PtyInstruction::NewTab(..) = i {
                return true;
            } else {
                return false;
            }
        })
        .unwrap()
        .clone();
    assert_snapshot!(format!("{:#?}", new_tab_instruction));
}

#[test]
pub fn send_cli_next_tab_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut second_tab_layout = PaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let goto_next_tab = CliAction::GoToNextTab;
    send_cli_action_to_server(
        &session_metadata,
        goto_next_tab,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_thread, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_previous_tab_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut second_tab_layout = PaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let goto_previous_tab = CliAction::GoToPreviousTab;
    send_cli_action_to_server(
        &session_metadata,
        goto_previous_tab,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_thread, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_goto_tab_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut second_tab_layout = PaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let goto_tab = CliAction::GoToTab { index: 1 };
    send_cli_action_to_server(&session_metadata, goto_tab, &mut mock_screen, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_thread, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_close_tab_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut second_tab_layout = PaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let close_tab = CliAction::CloseTab;
    send_cli_action_to_server(&session_metadata, close_tab, &mut mock_screen, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_thread, screen_thread]);
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    for (_cursor_coordinates, snapshot) in snapshots {
        assert_snapshot!(format!("{}", snapshot));
    }
    assert_snapshot!(format!("{}", snapshot_count));
}

#[test]
pub fn send_cli_rename_tab() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut second_tab_layout = PaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_plugin_instructions = Arc::new(Mutex::new(vec![]));
    let plugin_receiver = mock_screen.plugin_receiver.take().unwrap();
    let plugin_thread = log_actions_in_thread!(
        received_plugin_instructions,
        PluginInstruction::Exit,
        plugin_receiver
    );
    let rename_tab = CliAction::RenameTab {
        name: "new-tab-name".into(),
    };
    send_cli_action_to_server(&session_metadata, rename_tab, &mut mock_screen, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![plugin_thread, screen_thread]);
    assert_snapshot!(format!(
        "{:#?}",
        *received_plugin_instructions.lock().unwrap()
    ))
}

#[test]
pub fn send_cli_undo_rename_tab() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = PaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut second_tab_layout = PaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![PaneLayout::default(), PaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout));
    let received_plugin_instructions = Arc::new(Mutex::new(vec![]));
    let plugin_receiver = mock_screen.plugin_receiver.take().unwrap();
    let plugin_thread = log_actions_in_thread!(
        received_plugin_instructions,
        PluginInstruction::Exit,
        plugin_receiver
    );
    let rename_tab = CliAction::RenameTab {
        name: "new-tab-name".into(),
    };
    let undo_rename_tab = CliAction::UndoRenameTab;
    // first rename the tab
    send_cli_action_to_server(&session_metadata, rename_tab, &mut mock_screen, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    // then undo the tab rename to go back to the default name
    send_cli_action_to_server(
        &session_metadata,
        undo_rename_tab,
        &mut mock_screen,
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![plugin_thread, screen_thread]);
    assert_snapshot!(format!(
        "{:#?}",
        *received_plugin_instructions.lock().unwrap()
    ))
}

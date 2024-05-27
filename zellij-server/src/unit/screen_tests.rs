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
use zellij_utils::data::{Event, Resize, Style};
use zellij_utils::errors::{prelude::*, ErrorContext};
use zellij_utils::input::actions::Action;
use zellij_utils::input::command::{RunCommand, TerminalAction};
use zellij_utils::input::layout::{
    FloatingPaneLayout, Layout, PluginAlias, PluginUserConfiguration, Run, RunPlugin,
    RunPluginLocation, RunPluginOrAlias, SplitDirection, SplitSize, TiledPaneLayout,
};
use zellij_utils::input::options::Options;
use zellij_utils::ipc::IpcReceiverWithContext;
use zellij_utils::pane_size::{Size, SizeInPixels};

use crate::background_jobs::BackgroundJob;
use crate::pty_writer::PtyWriteInstruction;
use std::env::set_var;
use std::os::unix::io::RawFd;
use std::sync::{Arc, Mutex};

use crate::{plugins::PluginInstruction, pty::PtyInstruction};
use zellij_utils::ipc::PixelDimensions;

use zellij_utils::{
    channels::{self, ChannelWithContext, Receiver},
    data::{Direction, FloatingPaneCoordinates, InputMode, ModeInfo, Palette, PluginCapabilities},
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

fn take_snapshot_and_cursor_coordinates(
    ansi_instructions: &str,
    grid: &mut Grid,
) -> (Option<(usize, usize)>, String) {
    let mut vte_parser = vte::Parser::new();
    for &byte in ansi_instructions.as_bytes() {
        vte_parser.advance(grid, byte);
    }
    (grid.cursor_coordinates(), format!("{:?}", grid))
}

fn take_snapshots_and_cursor_coordinates_from_render_events<'a>(
    all_events: impl Iterator<Item = &'a ServerInstruction>,
    screen_size: Size,
) -> Vec<(Option<(usize, usize)>, String)> {
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
        screen_size.rows,
        screen_size.cols,
        Rc::new(RefCell::new(Palette::default())),
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
    let snapshots: Vec<(Option<(usize, usize)>, String)> = all_events
        .filter_map(|server_instruction| {
            match server_instruction {
                ServerInstruction::Render(output) => {
                    if let Some(output) = output {
                        // note this only takes a snapshot of the first client!
                        let raw_snapshot = output.get(&1).unwrap();
                        let snapshot =
                            take_snapshot_and_cursor_coordinates(raw_snapshot, &mut grid);
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
    client_id: ClientId,
) {
    let get_current_dir = || PathBuf::from(".");
    let actions = Action::actions_from_cli(cli_action, Box::new(get_current_dir), None).unwrap();
    let senders = session_metadata.senders.clone();
    let capabilities = PluginCapabilities::default();
    let client_attributes = ClientAttributes::default();
    let default_shell = None;
    let default_layout = Box::new(Layout::default());
    for action in actions {
        route_action(
            action,
            client_id,
            None,
            senders.clone(),
            capabilities,
            client_attributes.clone(),
            default_shell.clone(),
            default_layout.clone(),
            None,
        )
        .unwrap();
    }
}

#[derive(Clone, Default)]
struct FakeInputOutput {
    fake_filesystem: Arc<Mutex<HashMap<String, String>>>,
    server_to_client_messages: Arc<Mutex<HashMap<ClientId, Vec<ServerToClientMsg>>>>,
}

impl ServerOsApi for FakeInputOutput {
    fn set_terminal_size_using_terminal_id(
        &self,
        _terminal_id: u32,
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
    fn send_to_client(&self, client_id: ClientId, msg: ServerToClientMsg) -> Result<()> {
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
    fn write_to_file(&mut self, contents: String, filename: Option<String>) -> Result<()> {
        if let Some(filename) = filename {
            self.fake_filesystem
                .lock()
                .unwrap()
                .insert(filename, contents);
        }
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
    let auto_layout = true;
    let session_is_mirrored = true;
    let copy_options = CopyOptions::default();
    let default_layout = Box::new(Layout::default());
    let default_layout_name = None;
    let default_shell = None;
    let session_serialization = true;
    let serialize_pane_viewport = false;
    let scrollback_lines_to_serialize = None;
    let layout_dir = None;

    let debug = false;
    let styled_underlines = true;
    let arrow_fonts = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let screen = Screen::new(
        bus,
        &client_attributes,
        max_panes,
        mode_info,
        draw_pane_frames,
        auto_layout,
        session_is_mirrored,
        copy_options,
        debug,
        default_layout,
        default_layout_name,
        default_shell,
        session_serialization,
        serialize_pane_viewport,
        scrollback_lines_to_serialize,
        styled_underlines,
        arrow_fonts,
        layout_dir,
        explicitly_disable_kitty_keyboard_protocol,
    );
    screen
}

struct MockScreen {
    pub main_client_id: u16,
    pub pty_receiver: Option<Receiver<(PtyInstruction, ErrorContext)>>,
    pub pty_writer_receiver: Option<Receiver<(PtyWriteInstruction, ErrorContext)>>,
    pub background_jobs_receiver: Option<Receiver<(BackgroundJob, ErrorContext)>>,
    pub screen_receiver: Option<Receiver<(ScreenInstruction, ErrorContext)>>,
    pub server_receiver: Option<Receiver<(ServerInstruction, ErrorContext)>>,
    pub plugin_receiver: Option<Receiver<(PluginInstruction, ErrorContext)>>,
    pub to_screen: SenderWithContext<ScreenInstruction>,
    pub to_pty: SenderWithContext<PtyInstruction>,
    pub to_plugin: SenderWithContext<PluginInstruction>,
    pub to_server: SenderWithContext<ServerInstruction>,
    pub to_pty_writer: SenderWithContext<PtyWriteInstruction>,
    pub to_background_jobs: SenderWithContext<BackgroundJob>,
    pub os_input: FakeInputOutput,
    pub client_attributes: ClientAttributes,
    pub config_options: Options,
    pub session_metadata: SessionMetaData,
    last_opened_tab_index: Option<usize>,
}

impl MockScreen {
    pub fn run(
        &mut self,
        initial_layout: Option<TiledPaneLayout>,
        initial_floating_panes_layout: Vec<FloatingPaneLayout>,
    ) -> std::thread::JoinHandle<()> {
        let config_options = self.config_options.clone();
        let client_attributes = self.client_attributes.clone();
        let screen_bus = Bus::new(
            vec![self.screen_receiver.take().unwrap()],
            None,
            Some(&self.to_pty.clone()),
            Some(&self.to_plugin.clone()),
            Some(&self.to_server.clone()),
            Some(&self.to_pty_writer.clone()),
            Some(&self.to_background_jobs.clone()),
            Some(Box::new(self.os_input.clone())),
        )
        .should_silently_fail();
        let debug = false;
        let screen_thread = std::thread::Builder::new()
            .name("screen_thread".to_string())
            .spawn(move || {
                set_var("ZELLIJ_SESSION_NAME", "zellij-test");
                screen_thread_main(
                    screen_bus,
                    None,
                    client_attributes,
                    Box::new(config_options),
                    debug,
                    Box::new(Layout::default()),
                )
                .expect("TEST")
            })
            .unwrap();
        let pane_layout = initial_layout.unwrap_or_default();
        let pane_count = pane_layout.extract_run_instructions().len();
        let floating_pane_count = initial_floating_panes_layout.len();
        let mut pane_ids = vec![];
        let mut floating_pane_ids = vec![];
        let mut plugin_ids = HashMap::new();
        plugin_ids.insert(
            RunPluginOrAlias::from_url("file:/path/to/fake/plugin", &None, None, None).unwrap(),
            vec![1],
        );
        for i in 0..pane_count {
            pane_ids.push((i as u32, None));
        }
        for i in 0..floating_pane_count {
            floating_pane_ids.push((i as u32, None));
        }
        let default_shell = None;
        let tab_name = None;
        let tab_index = self.last_opened_tab_index.map(|l| l + 1).unwrap_or(0);
        let _ = self.to_screen.send(ScreenInstruction::NewTab(
            None,
            default_shell,
            Some(pane_layout.clone()),
            initial_floating_panes_layout.clone(),
            tab_name,
            (vec![], vec![]), // swap layouts
            self.main_client_id,
        ));
        let _ = self.to_screen.send(ScreenInstruction::ApplyLayout(
            pane_layout,
            initial_floating_panes_layout,
            pane_ids,
            floating_pane_ids,
            plugin_ids,
            tab_index,
            self.main_client_id,
        ));
        self.last_opened_tab_index = Some(tab_index);
        screen_thread
    }
    // same as the above function, but starts a plugin with a plugin alias
    pub fn run_with_alias(
        &mut self,
        initial_layout: Option<TiledPaneLayout>,
        initial_floating_panes_layout: Vec<FloatingPaneLayout>,
    ) -> std::thread::JoinHandle<()> {
        let config_options = self.config_options.clone();
        let client_attributes = self.client_attributes.clone();
        let screen_bus = Bus::new(
            vec![self.screen_receiver.take().unwrap()],
            None,
            Some(&self.to_pty.clone()),
            Some(&self.to_plugin.clone()),
            Some(&self.to_server.clone()),
            Some(&self.to_pty_writer.clone()),
            Some(&self.to_background_jobs.clone()),
            Some(Box::new(self.os_input.clone())),
        )
        .should_silently_fail();
        let debug = false;
        let screen_thread = std::thread::Builder::new()
            .name("screen_thread".to_string())
            .spawn(move || {
                set_var("ZELLIJ_SESSION_NAME", "zellij-test");
                screen_thread_main(
                    screen_bus,
                    None,
                    client_attributes,
                    Box::new(config_options),
                    debug,
                    Box::new(Layout::default()),
                )
                .expect("TEST")
            })
            .unwrap();
        let pane_layout = initial_layout.unwrap_or_default();
        let pane_count = pane_layout.extract_run_instructions().len();
        let floating_pane_count = initial_floating_panes_layout.len();
        let mut pane_ids = vec![];
        let mut floating_pane_ids = vec![];
        let mut plugin_ids = HashMap::new();
        plugin_ids.insert(
            RunPluginOrAlias::Alias(PluginAlias {
                name: "fixture_plugin_for_tests".to_owned(),
                configuration: Some(Default::default()),
                run_plugin: Some(RunPlugin {
                    location: RunPluginLocation::parse("file:/path/to/fake/plugin", None).unwrap(),
                    configuration: PluginUserConfiguration::default(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            vec![1],
        );
        for i in 0..pane_count {
            pane_ids.push((i as u32, None));
        }
        for i in 0..floating_pane_count {
            floating_pane_ids.push((i as u32, None));
        }
        let default_shell = None;
        let tab_name = None;
        let tab_index = self.last_opened_tab_index.map(|l| l + 1).unwrap_or(0);
        let _ = self.to_screen.send(ScreenInstruction::NewTab(
            None,
            default_shell,
            Some(pane_layout.clone()),
            initial_floating_panes_layout.clone(),
            tab_name,
            (vec![], vec![]), // swap layouts
            self.main_client_id,
        ));
        let _ = self.to_screen.send(ScreenInstruction::ApplyLayout(
            pane_layout,
            initial_floating_panes_layout,
            pane_ids,
            floating_pane_ids,
            plugin_ids,
            tab_index,
            self.main_client_id,
        ));
        self.last_opened_tab_index = Some(tab_index);
        screen_thread
    }
    pub fn new_tab(&mut self, tab_layout: TiledPaneLayout) {
        let pane_count = tab_layout.extract_run_instructions().len();
        let mut pane_ids = vec![];
        let plugin_ids = HashMap::new();
        let default_shell = None;
        let tab_name = None;
        let tab_index = self.last_opened_tab_index.map(|l| l + 1).unwrap_or(0);
        for i in 0..pane_count {
            pane_ids.push((i as u32, None));
        }
        let _ = self.to_screen.send(ScreenInstruction::NewTab(
            None,
            default_shell,
            Some(tab_layout.clone()),
            vec![], // floating_panes_layout
            tab_name,
            (vec![], vec![]), // swap layouts
            self.main_client_id,
        ));
        let _ = self.to_screen.send(ScreenInstruction::ApplyLayout(
            tab_layout,
            vec![], // floating_panes_layout
            pane_ids,
            vec![], // floating panes ids
            plugin_ids,
            0,
            self.main_client_id,
        ));
        self.last_opened_tab_index = Some(tab_index);
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
        let layout = Box::new(Layout::default()); // this is not actually correct!!
        SessionMetaData {
            senders: self.session_metadata.senders.clone(),
            capabilities: self.session_metadata.capabilities.clone(),
            client_attributes: self.session_metadata.client_attributes.clone(),
            default_shell: self.session_metadata.default_shell.clone(),
            screen_thread: None,
            pty_thread: None,
            plugin_thread: None,
            pty_writer_thread: None,
            background_jobs_thread: None,
            config_options: Default::default(),
            layout,
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

        let (to_background_jobs, background_jobs_receiver): ChannelWithContext<BackgroundJob> =
            channels::unbounded();
        let to_background_jobs = SenderWithContext::new(to_background_jobs);

        let client_attributes = ClientAttributes {
            size,
            ..Default::default()
        };
        let capabilities = PluginCapabilities {
            arrow_fonts: Default::default(),
        };

        let layout = Box::new(Layout::default()); // this is not actually correct!!
        let session_metadata = SessionMetaData {
            senders: ThreadSenders {
                to_screen: Some(to_screen.clone()),
                to_pty: Some(to_pty.clone()),
                to_plugin: Some(to_plugin.clone()),
                to_pty_writer: Some(to_pty_writer.clone()),
                to_background_jobs: Some(to_background_jobs.clone()),
                to_server: Some(to_server.clone()),
                should_silently_fail: true,
            },
            capabilities,
            default_shell: None,
            client_attributes: client_attributes.clone(),
            screen_thread: None,
            pty_thread: None,
            plugin_thread: None,
            pty_writer_thread: None,
            background_jobs_thread: None,
            config_options: Default::default(),
            layout,
        };

        let os_input = FakeInputOutput::default();
        let config_options = Options::default();
        let main_client_id = 1;
        MockScreen {
            main_client_id,
            pty_receiver: Some(pty_receiver),
            pty_writer_receiver: Some(pty_writer_receiver),
            background_jobs_receiver: Some(background_jobs_receiver),
            screen_receiver: Some(screen_receiver),
            server_receiver: Some(server_receiver),
            plugin_receiver: Some(plugin_receiver),
            to_screen,
            to_pty,
            to_plugin,
            to_server,
            to_pty_writer,
            to_background_jobs,
            os_input,
            client_attributes,
            config_options,
            session_metadata,
            last_opened_tab_index: None,
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

fn new_tab(screen: &mut Screen, pid: u32, tab_index: usize) {
    let client_id = 1;
    let new_terminal_ids = vec![(pid, None)];
    let new_plugin_ids = HashMap::new();
    screen
        .new_tab(tab_index, (vec![], vec![]), None, client_id)
        .expect("TEST");
    screen
        .apply_layout(
            TiledPaneLayout::default(),
            vec![], // floating panes layout
            new_terminal_ids,
            vec![], // new floating terminal ids
            new_plugin_ids,
            tab_index,
            client_id,
        )
        .expect("TEST");
}

#[test]
fn open_new_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1, 0);
    new_tab(&mut screen, 2, 1);

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

    new_tab(&mut screen, 1, 1);
    new_tab(&mut screen, 2, 2);
    screen.switch_tab_prev(None, true, 1).expect("TEST");

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

    new_tab(&mut screen, 1, 1);
    new_tab(&mut screen, 2, 2);
    screen.switch_tab_prev(None, true, 1).expect("TEST");
    screen.switch_tab_next(None, true, 1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab switched to next tab"
    );
}

#[test]
pub fn switch_to_tab_name() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1, 1);
    new_tab(&mut screen, 2, 2);

    assert_eq!(
        screen
            .switch_active_tab_name("Tab #1".to_string(), 1)
            .expect("TEST"),
        false,
        "Active tab switched to tab by name"
    );
    assert_eq!(
        screen
            .switch_active_tab_name("Tab #2".to_string(), 1)
            .expect("TEST"),
        true,
        "Active tab switched to tab by name"
    );
    assert_eq!(
        screen
            .switch_active_tab_name("Tab #3".to_string(), 1)
            .expect("TEST"),
        true,
        "Active tab switched to tab by name"
    );
}

#[test]
pub fn close_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1, 1);
    new_tab(&mut screen, 2, 2);
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

    new_tab(&mut screen, 1, 1);
    new_tab(&mut screen, 2, 2);
    new_tab(&mut screen, 3, 3);
    screen.switch_tab_prev(None, true, 1).expect("TEST");
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

    new_tab(&mut screen, 1, 1);
    new_tab(&mut screen, 2, 2);
    new_tab(&mut screen, 3, 3);
    screen.switch_tab_prev(None, true, 1).expect("TEST");
    screen.move_focus_left_or_previous_tab(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab switched to previous"
    );
}

#[test]
fn basic_move_of_active_tab_to_left() {
    let mut screen = create_fixed_size_screen();
    new_tab(&mut screen, 1, 0);
    new_tab(&mut screen, 2, 1);
    assert_eq!(screen.get_active_tab(1).unwrap().position, 1);

    screen.move_active_tab_to_left(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab moved to left"
    );
}

fn create_fixed_size_screen() -> Screen {
    create_new_screen(Size {
        cols: 121,
        rows: 20,
    })
}

#[test]
fn move_of_active_tab_to_left_when_there_is_only_one_tab() {
    let mut screen = create_fixed_size_screen();
    new_tab(&mut screen, 1, 0);
    assert_eq!(screen.get_active_tab(1).unwrap().position, 0);

    screen.move_active_tab_to_left(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab moved to left"
    );
}

#[test]
fn move_of_active_tab_to_left_multiple_times() {
    let mut screen = create_fixed_size_screen();
    new_tab(&mut screen, 1, 0);
    new_tab(&mut screen, 2, 1);
    new_tab(&mut screen, 3, 2);
    assert_eq!(screen.get_active_tab(1).unwrap().position, 2);

    screen.move_active_tab_to_left(1).expect("TEST");
    screen.move_active_tab_to_left(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab moved to left twice"
    );
}

#[test]
fn wrapping_move_of_active_tab_to_left() {
    let mut screen = create_fixed_size_screen();
    new_tab(&mut screen, 1, 0);
    new_tab(&mut screen, 2, 1);
    new_tab(&mut screen, 3, 2);
    screen.move_focus_left_or_previous_tab(1).expect("TEST");
    screen.move_focus_left_or_previous_tab(1).expect("TEST");
    assert_eq!(screen.get_active_tab(1).unwrap().position, 0);

    screen.move_active_tab_to_left(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab moved to left until wrapped around"
    );
}

#[test]
fn basic_move_of_active_tab_to_right() {
    let mut screen = create_fixed_size_screen();
    new_tab(&mut screen, 1, 0);
    new_tab(&mut screen, 2, 1);
    screen.move_focus_left_or_previous_tab(1).expect("TEST");
    assert_eq!(screen.get_active_tab(1).unwrap().position, 0);

    screen.move_active_tab_to_right(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        1,
        "Active tab moved to right"
    );
}

#[test]
fn move_of_active_tab_to_right_when_there_is_only_one_tab() {
    let mut screen = create_fixed_size_screen();
    new_tab(&mut screen, 1, 0);
    assert_eq!(screen.get_active_tab(1).unwrap().position, 0);

    screen.move_active_tab_to_right(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab moved to left"
    );
}

#[test]
fn move_of_active_tab_to_right_multiple_times() {
    let mut screen = create_fixed_size_screen();
    new_tab(&mut screen, 1, 0);
    new_tab(&mut screen, 2, 1);
    new_tab(&mut screen, 3, 2);
    screen.move_focus_left_or_previous_tab(1).expect("TEST");
    screen.move_focus_left_or_previous_tab(1).expect("TEST");
    assert_eq!(screen.get_active_tab(1).unwrap().position, 0);

    screen.move_active_tab_to_right(1).expect("TEST");
    screen.move_active_tab_to_right(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        2,
        "Active tab moved to right twice"
    );
}

#[test]
fn wrapping_move_of_active_tab_to_right() {
    let mut screen = create_fixed_size_screen();
    new_tab(&mut screen, 1, 0);
    new_tab(&mut screen, 2, 1);
    new_tab(&mut screen, 3, 2);
    assert_eq!(screen.get_active_tab(1).unwrap().position, 2);

    screen.move_active_tab_to_right(1).expect("TEST");

    assert_eq!(
        screen.get_active_tab(1).unwrap().position,
        0,
        "Active tab moved to right until wrapped around"
    );
}

#[test]
fn move_focus_right_at_right_screen_edge_changes_tab() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1, 1);
    new_tab(&mut screen, 2, 2);
    new_tab(&mut screen, 3, 3);
    screen.switch_tab_prev(None, true, 1).expect("TEST");
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

    new_tab(&mut screen, 1, 1);
    new_tab(&mut screen, 2, 2);
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

    new_tab(&mut screen, 1, 0);
    new_tab(&mut screen, 2, 1);
    new_tab(&mut screen, 3, 2);

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

    new_tab(&mut screen, 1, 0);
    new_tab(&mut screen, 2, 1);
    new_tab(&mut screen, 3, 2);
    new_tab(&mut screen, 4, 3);

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

    screen.switch_tab_prev(None, true, 1).expect("TEST");
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
    screen.switch_tab_prev(None, true, 1).expect("TEST");
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

    new_tab(&mut screen, 1, 1);
    {
        let active_tab = screen.get_active_tab_mut(1).unwrap();
        active_tab
            .new_pane(PaneId::Terminal(2), None, None, None, None, Some(1))
            .unwrap();
        active_tab.toggle_active_pane_fullscreen(1);
    }
    new_tab(&mut screen, 2, 2);

    screen.switch_tab_prev(None, true, 1).expect("TEST");

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

    new_tab(&mut screen, 1, 0);
    {
        let active_tab = screen.get_active_tab_mut(1).unwrap();
        active_tab
            .new_pane(PaneId::Terminal(2), None, None, None, None, Some(1))
            .unwrap();
        active_tab.toggle_active_pane_fullscreen(1);
    }
    new_tab(&mut screen, 2, 1);

    screen.close_tab_at_index(0).expect("TEST");
    screen.remove_client(1).expect("TEST");
    screen.add_client(1).expect("TEST");
}

#[test]
fn open_new_floating_pane_with_custom_coordinates() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1, 0);
    let active_tab = screen.get_active_tab_mut(1).unwrap();
    let should_float = Some(true);
    active_tab
        .new_pane(
            PaneId::Terminal(2),
            None,
            should_float,
            None,
            Some(FloatingPaneCoordinates {
                x: Some(SplitSize::Percent(10)),
                y: Some(SplitSize::Fixed(5)),
                width: Some(SplitSize::Percent(1)),
                height: Some(SplitSize::Fixed(2)),
            }),
            Some(1),
        )
        .unwrap();
    let active_pane = active_tab.get_active_pane(1).unwrap();
    assert_eq!(active_pane.x(), 12, "x coordinates set properly");
    assert_eq!(active_pane.y(), 5, "y coordinates set properly");
    assert_eq!(active_pane.rows(), 2, "rows set properly");
    assert_eq!(active_pane.cols(), 1, "columns set properly");
}

#[test]
fn open_new_floating_pane_with_custom_coordinates_exceeding_viewport() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let mut screen = create_new_screen(size);

    new_tab(&mut screen, 1, 0);
    let active_tab = screen.get_active_tab_mut(1).unwrap();
    let should_float = Some(true);
    active_tab
        .new_pane(
            PaneId::Terminal(2),
            None,
            should_float,
            None,
            Some(FloatingPaneCoordinates {
                x: Some(SplitSize::Fixed(122)),
                y: Some(SplitSize::Fixed(21)),
                width: Some(SplitSize::Fixed(10)),
                height: Some(SplitSize::Fixed(10)),
            }),
            Some(1),
        )
        .unwrap();
    let active_pane = active_tab.get_active_pane(1).unwrap();
    assert_eq!(active_pane.x(), 111, "x coordinates set properly");
    assert_eq!(active_pane.y(), 10, "y coordinates set properly");
    assert_eq!(active_pane.rows(), 10, "rows set properly");
    assert_eq!(active_pane.cols(), 10, "columns set properly");
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
    let screen_thread = mock_screen.run(None, vec![]);
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_writer_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyWriteInstruction::Exit,
        pty_writer_receiver
    );
    let cli_action = CliAction::WriteChars {
        chars: "input from the cli".into(),
    };
    send_cli_action_to_server(&session_metadata, cli_action, client_id);
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
    let screen_thread = mock_screen.run(None, vec![]);
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_writer_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyWriteInstruction::Exit,
        pty_writer_receiver
    );
    let cli_action = CliAction::Write {
        bytes: vec![102, 111, 111],
    };
    send_cli_action_to_server(&session_metadata, cli_action, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_writer_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_resize_action_to_screen() {
    let size = Size { cols: 80, rows: 20 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let pty_writer_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let resize_cli_action = CliAction::Resize {
        resize: Resize::Increase,
        direction: Some(Direction::Left),
    };
    send_cli_action_to_server(&session_metadata, resize_cli_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let focus_next_pane_action = CliAction::FocusNextPane;
    send_cli_action_to_server(&session_metadata, focus_next_pane_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let focus_next_pane_action = CliAction::FocusPreviousPane;
    send_cli_action_to_server(&session_metadata, focus_next_pane_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, move_focus_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, move_focus_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let cli_action = CliAction::MovePane {
        direction: Some(Direction::Right),
    };
    send_cli_action_to_server(&session_metadata, cli_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let cli_action = CliAction::DumpScreen {
        path: PathBuf::from("/tmp/foo"),
        full: true,
    };
    let _ = mock_screen.to_screen.send(ScreenInstruction::PtyBytes(
        0,
        "fill pane up with something".as_bytes().to_vec(),
    ));
    send_cli_action_to_server(&session_metadata, cli_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, cli_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, cli_action.clone(), client_id);
    send_cli_action_to_server(&session_metadata, cli_action.clone(), client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, scroll_up_cli_action.clone(), client_id);
    send_cli_action_to_server(&session_metadata, scroll_up_cli_action.clone(), client_id);
    send_cli_action_to_server(&session_metadata, scroll_up_cli_action.clone(), client_id);
    send_cli_action_to_server(&session_metadata, scroll_up_cli_action.clone(), client_id);

    // scroll down some
    send_cli_action_to_server(&session_metadata, scroll_down_cli_action.clone(), client_id);
    send_cli_action_to_server(&session_metadata, scroll_down_cli_action.clone(), client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, scroll_up_cli_action.clone(), client_id);
    send_cli_action_to_server(&session_metadata, scroll_up_cli_action.clone(), client_id);
    send_cli_action_to_server(&session_metadata, scroll_up_cli_action.clone(), client_id);
    send_cli_action_to_server(&session_metadata, scroll_up_cli_action.clone(), client_id);

    // scroll to bottom
    send_cli_action_to_server(
        &session_metadata,
        scroll_to_bottom_action.clone(),
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
pub fn send_cli_scroll_to_top_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let scroll_to_top_action = CliAction::ScrollToTop;
    let mut pane_contents = String::new();
    for i in 0..20 {
        pane_contents.push_str(&format!("fill pane up with something {}\n\r", i));
    }
    let _ = mock_screen.to_screen.send(ScreenInstruction::PtyBytes(
        0,
        pane_contents.as_bytes().to_vec(),
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // scroll to top
    send_cli_action_to_server(&session_metadata, scroll_to_top_action.clone(), client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, page_scroll_up_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, page_scroll_up_action.clone(), client_id);
    send_cli_action_to_server(&session_metadata, page_scroll_up_action.clone(), client_id);

    // scroll down
    send_cli_action_to_server(&session_metadata, page_scroll_down_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, half_page_scroll_up_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
        client_id,
    );
    send_cli_action_to_server(
        &session_metadata,
        half_page_scroll_up_action.clone(),
        client_id,
    );

    // scroll down
    send_cli_action_to_server(&session_metadata, half_page_scroll_down_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let toggle_full_screen_action = CliAction::ToggleFullscreen;
    send_cli_action_to_server(&session_metadata, toggle_full_screen_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let toggle_pane_frames_action = CliAction::TogglePaneFrames;
    send_cli_action_to_server(&session_metadata, toggle_pane_frames_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
        client_id,
    );
    send_cli_action_to_server(&session_metadata, cli_write_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_new_pane_action = CliAction::NewPane {
        direction: None,
        command: vec![],
        plugin: None,
        cwd: None,
        floating: false,
        in_place: false,
        name: None,
        close_on_exit: false,
        start_suspended: false,
        configuration: None,
        skip_plugin_cache: false,
        x: None,
        y: None,
        width: None,
        height: None,
    };
    send_cli_action_to_server(&session_metadata, cli_new_pane_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_new_pane_action = CliAction::NewPane {
        direction: Some(Direction::Right),
        command: vec![],
        plugin: None,
        cwd: None,
        floating: false,
        in_place: false,
        name: None,
        close_on_exit: false,
        start_suspended: false,
        configuration: None,
        skip_plugin_cache: false,
        x: None,
        y: None,
        width: None,
        height: None,
    };
    send_cli_action_to_server(&session_metadata, cli_new_pane_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_new_pane_action = CliAction::NewPane {
        direction: Some(Direction::Right),
        command: vec!["htop".into()],
        plugin: None,
        cwd: Some("/some/folder".into()),
        floating: false,
        in_place: false,
        name: None,
        close_on_exit: false,
        start_suspended: false,
        configuration: None,
        skip_plugin_cache: false,
        x: None,
        y: None,
        width: None,
        height: None,
    };
    send_cli_action_to_server(&session_metadata, cli_new_pane_action, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_thread, screen_thread]);
    assert_snapshot!(format!("{:?}", *received_pty_instructions.lock().unwrap()));
}

#[test]
pub fn send_cli_new_pane_action_with_floating_pane_and_coordinates() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let pty_receiver = mock_screen.pty_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_new_pane_action = CliAction::NewPane {
        direction: Some(Direction::Right),
        command: vec!["htop".into()],
        plugin: None,
        cwd: Some("/some/folder".into()),
        floating: true,
        in_place: false,
        name: None,
        close_on_exit: false,
        start_suspended: false,
        configuration: None,
        skip_plugin_cache: false,
        x: Some("10".to_owned()),
        y: None,
        width: Some("20%".to_owned()),
        height: None,
    };
    send_cli_action_to_server(&session_metadata, cli_new_pane_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
        floating: false,
        in_place: false,
        cwd: None,
        x: None,
        y: None,
        width: None,
        height: None,
    };
    send_cli_action_to_server(&session_metadata, cli_edit_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
        floating: false,
        in_place: false,
        cwd: None,
        x: None,
        y: None,
        width: None,
        height: None,
    };
    send_cli_action_to_server(&session_metadata, cli_edit_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
        floating: false,
        in_place: false,
        cwd: None,
        x: None,
        y: None,
        width: None,
        height: None,
    };
    send_cli_action_to_server(&session_metadata, cli_edit_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let cli_switch_mode = CliAction::SwitchMode {
        input_mode: InputMode::Locked,
    };
    send_cli_action_to_server(&session_metadata, cli_switch_mode, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
        client_id,
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    // second time to embed
    send_cli_action_to_server(
        &session_metadata,
        toggle_pane_embed_or_floating.clone(),
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, toggle_pane_embed_or_floating, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    // toggle floating panes (will hide the floated pane from the previous action)
    send_cli_action_to_server(&session_metadata, toggle_floating_panes.clone(), client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    // toggle floating panes (will show the floated pane)
    send_cli_action_to_server(&session_metadata, toggle_floating_panes.clone(), client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_instruction = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let close_pane_action = CliAction::ClosePane;
    send_cli_action_to_server(&session_metadata, close_pane_action, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_plugin_instructions = Arc::new(Mutex::new(vec![]));
    let plugin_receiver = mock_screen.plugin_receiver.take().unwrap();
    let plugin_thread = log_actions_in_thread!(
        received_plugin_instructions,
        PluginInstruction::Exit,
        plugin_receiver
    );
    let new_tab_action = CliAction::NewTab {
        name: None,
        layout: None,
        layout_dir: None,
        cwd: None,
    };
    send_cli_action_to_server(&session_metadata, new_tab_action, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![plugin_thread, screen_thread]);
    let received_plugin_instructions = received_plugin_instructions.lock().unwrap();
    let new_tab_action =
        received_plugin_instructions
            .iter()
            .find(|instruction| match instruction {
                PluginInstruction::NewTab(..) => true,
                _ => false,
            });
    assert_snapshot!(format!("{:#?}", new_tab_action));
}

#[test]
pub fn send_cli_new_tab_action_with_name_and_layout() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_plugin_instructions = Arc::new(Mutex::new(vec![]));
    let plugin_receiver = mock_screen.plugin_receiver.take().unwrap();
    let plugin_thread = log_actions_in_thread!(
        received_plugin_instructions,
        PluginInstruction::Exit,
        plugin_receiver
    );
    let new_tab_action = CliAction::NewTab {
        name: Some("my-awesome-tab-name".into()),
        layout: Some(PathBuf::from(format!(
            "{}/src/unit/fixtures/layout-with-three-panes.kdl",
            env!("CARGO_MANIFEST_DIR")
        ))),
        layout_dir: None,
        cwd: None,
    };
    send_cli_action_to_server(&session_metadata, new_tab_action, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![plugin_thread, screen_thread]);
    let new_tab_instruction = received_plugin_instructions
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|i| {
            if let PluginInstruction::NewTab(..) = i {
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut second_tab_layout = TiledPaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let goto_next_tab = CliAction::GoToNextTab;
    send_cli_action_to_server(&session_metadata, goto_next_tab, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut second_tab_layout = TiledPaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let goto_previous_tab = CliAction::GoToPreviousTab;
    send_cli_action_to_server(&session_metadata, goto_previous_tab, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut second_tab_layout = TiledPaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let goto_tab = CliAction::GoToTab { index: 1 };
    send_cli_action_to_server(&session_metadata, goto_tab, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut second_tab_layout = TiledPaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let close_tab = CliAction::CloseTab;
    send_cli_action_to_server(&session_metadata, close_tab, client_id);
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
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut second_tab_layout = TiledPaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, rename_tab, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![plugin_thread, screen_thread]);
    let plugin_rename_tab_instruction = received_plugin_instructions
        .lock()
        .unwrap()
        .iter()
        .find(|instruction| match instruction {
            PluginInstruction::Update(updates) => updates
                .iter()
                .find(|u| match u {
                    (_, _, Event::TabUpdate(..)) => true,
                    _ => false,
                })
                .is_some(),
            _ => false,
        })
        .cloned();
    assert_snapshot!(format!("{:#?}", plugin_rename_tab_instruction))
}

#[test]
pub fn send_cli_undo_rename_tab() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut second_tab_layout = TiledPaneLayout::default();
    second_tab_layout.children_split_direction = SplitDirection::Horizontal;
    second_tab_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(second_tab_layout);
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
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
    send_cli_action_to_server(&session_metadata, rename_tab, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    // then undo the tab rename to go back to the default name
    send_cli_action_to_server(&session_metadata, undo_rename_tab, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![plugin_thread, screen_thread]);
    let plugin_undo_rename_tab_instruction = received_plugin_instructions
        .lock()
        .unwrap()
        .iter()
        .find(|instruction| match instruction {
            PluginInstruction::Update(updates) => updates
                .iter()
                .find(|u| match u {
                    (_, _, Event::TabUpdate(..)) => true,
                    _ => false,
                })
                .is_some(),
            _ => false,
        })
        .cloned();
    assert_snapshot!(format!("{:#?}", plugin_undo_rename_tab_instruction))
}

#[test]
pub fn send_cli_query_tab_names_action() {
    let size = Size { cols: 80, rows: 10 };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    mock_screen.new_tab(TiledPaneLayout::default());
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(Some(TiledPaneLayout::default()), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let query_tab_names = CliAction::QueryTabNames;
    send_cli_action_to_server(&session_metadata, query_tab_names, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100));
    mock_screen.teardown(vec![server_thread, screen_thread]);
    let log_tab_names_instruction = received_server_instructions
        .lock()
        .unwrap()
        .iter()
        .find(|instruction| match instruction {
            ServerInstruction::Log(..) => true,
            _ => false,
        })
        .cloned();
    assert_snapshot!(format!("{:#?}", log_tab_names_instruction));
}

#[test]
pub fn send_cli_launch_or_focus_plugin_action() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let pty_receiver = mock_screen.pty_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let screen_thread = mock_screen.run(None, vec![]);
    let received_pty_instructions = Arc::new(Mutex::new(vec![]));
    let pty_thread = log_actions_in_thread!(
        received_pty_instructions,
        PtyInstruction::Exit,
        pty_receiver
    );
    let cli_action = CliAction::LaunchOrFocusPlugin {
        floating: true,
        in_place: false,
        move_to_focused_tab: true,
        url: "file:/path/to/fake/plugin".to_owned(),
        configuration: Default::default(),
        skip_plugin_cache: false,
    };
    send_cli_action_to_server(&session_metadata, cli_action, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![pty_thread, screen_thread]);

    let pty_fill_plugin_cwd_instruction = received_pty_instructions
        .lock()
        .unwrap()
        .iter()
        .find(|instruction| match instruction {
            PtyInstruction::FillPluginCwd(..) => true,
            _ => false,
        })
        .cloned();

    assert_snapshot!(format!("{:#?}", pty_fill_plugin_cwd_instruction));
}

#[test]
pub fn send_cli_launch_or_focus_plugin_action_when_plugin_is_already_loaded() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let plugin_receiver = mock_screen.plugin_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let mut initial_layout = TiledPaneLayout::default();
    let existing_plugin_pane = TiledPaneLayout {
        run: Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
            _allow_exec_host_cmd: false,
            location: RunPluginLocation::File(PathBuf::from("/path/to/fake/plugin")),
            configuration: Default::default(),
            ..Default::default()
        }))),
        ..Default::default()
    };
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), existing_plugin_pane];
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_plugin_instructions = Arc::new(Mutex::new(vec![]));
    let plugin_thread = log_actions_in_thread!(
        received_plugin_instructions,
        PluginInstruction::Exit,
        plugin_receiver
    );
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let cli_action = CliAction::LaunchOrFocusPlugin {
        floating: true,
        in_place: false,
        move_to_focused_tab: true,
        url: "file:/path/to/fake/plugin".to_owned(),
        configuration: Default::default(),
        skip_plugin_cache: false,
    };
    send_cli_action_to_server(&session_metadata, cli_action, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![plugin_thread, server_thread, screen_thread]);

    let plugin_load_instruction_sent = received_plugin_instructions
        .lock()
        .unwrap()
        .iter()
        .find(|instruction| match instruction {
            PluginInstruction::Load(..) => true,
            _ => false,
        })
        .is_some();
    assert!(
        !plugin_load_instruction_sent,
        "Plugin Load instruction should not be sent for an already loaded plugin"
    );
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    assert_eq!(
        snapshot_count, 2,
        "Another render was sent for focusing the already loaded plugin"
    );
    for (cursor_coordinates, _snapshot) in snapshots.iter().skip(1) {
        assert!(
            cursor_coordinates.is_none(),
            "Cursor moved to existing plugin in final snapshot indicating focus changed"
        );
    }
}

#[test]
pub fn send_cli_launch_or_focus_plugin_action_when_plugin_is_already_loaded_for_plugin_alias() {
    let size = Size {
        cols: 121,
        rows: 20,
    };
    let client_id = 10; // fake client id should not appear in the screen's state
    let mut mock_screen = MockScreen::new(size);
    let plugin_receiver = mock_screen.plugin_receiver.take().unwrap();
    let session_metadata = mock_screen.clone_session_metadata();
    let mut initial_layout = TiledPaneLayout::default();
    let existing_plugin_pane = TiledPaneLayout {
        run: Some(Run::Plugin(RunPluginOrAlias::Alias(PluginAlias {
            name: "fixture_plugin_for_tests".to_owned(),
            configuration: Some(Default::default()),
            run_plugin: Some(RunPlugin {
                _allow_exec_host_cmd: false,
                location: RunPluginLocation::File(PathBuf::from("/path/to/fake/plugin")),
                configuration: Default::default(),
                ..Default::default()
            }),
            ..Default::default()
        }))),
        ..Default::default()
    };
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), existing_plugin_pane];
    let screen_thread = mock_screen.run_with_alias(Some(initial_layout), vec![]);
    let received_plugin_instructions = Arc::new(Mutex::new(vec![]));
    let plugin_thread = log_actions_in_thread!(
        received_plugin_instructions,
        PluginInstruction::Exit,
        plugin_receiver
    );
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let cli_action = CliAction::LaunchOrFocusPlugin {
        floating: true,
        in_place: false,
        move_to_focused_tab: true,
        url: "fixture_plugin_for_tests".to_owned(),
        configuration: Default::default(),
        skip_plugin_cache: false,
    };
    send_cli_action_to_server(&session_metadata, cli_action, client_id);
    std::thread::sleep(std::time::Duration::from_millis(100)); // give time for actions to be
    mock_screen.teardown(vec![plugin_thread, server_thread, screen_thread]);

    let plugin_load_instruction_sent = received_plugin_instructions
        .lock()
        .unwrap()
        .iter()
        .find(|instruction| match instruction {
            PluginInstruction::Load(..) => true,
            _ => false,
        })
        .is_some();
    assert!(
        !plugin_load_instruction_sent,
        "Plugin Load instruction should not be sent for an already loaded plugin"
    );
    let snapshots = take_snapshots_and_cursor_coordinates_from_render_events(
        received_server_instructions.lock().unwrap().iter(),
        size,
    );
    let snapshot_count = snapshots.len();
    assert_eq!(
        snapshot_count, 2,
        "Another render was sent for focusing the already loaded plugin"
    );
    for (cursor_coordinates, _snapshot) in snapshots.iter().skip(1) {
        assert!(
            cursor_coordinates.is_none(),
            "Cursor moved to existing plugin in final snapshot indicating focus changed"
        );
    }
}

#[test]
pub fn screen_can_suppress_pane() {
    let size = Size { cols: 80, rows: 20 };
    let mut initial_layout = TiledPaneLayout::default();
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![TiledPaneLayout::default(), TiledPaneLayout::default()];
    let mut mock_screen = MockScreen::new(size);
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::SuppressPane(PaneId::Terminal(1), 1));
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
pub fn screen_can_break_pane_to_a_new_tab() {
    let size = Size { cols: 80, rows: 20 };
    let mut initial_layout = TiledPaneLayout::default();
    let mut pane_to_break_free = TiledPaneLayout::default();
    pane_to_break_free.name = Some("pane_to_break_free".to_owned());
    let mut pane_to_stay = TiledPaneLayout::default();
    pane_to_stay.name = Some("pane_to_stay".to_owned());
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![pane_to_break_free, pane_to_stay];
    let mut mock_screen = MockScreen::new(size);
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );

    let _ = mock_screen.to_screen.send(ScreenInstruction::BreakPane(
        Box::new(Layout::default()),
        Default::default(),
        1,
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // we send ApplyLayout, because in prod this is eventually received after the message traverses
    // through the plugin and pty threads (to open extra stuff we need in the layout, eg. the
    // default plugins)
    let _ = mock_screen.to_screen.send(ScreenInstruction::ApplyLayout(
        TiledPaneLayout::default(),
        vec![], // floating_panes_layout
        Default::default(),
        vec![], // floating panes ids
        Default::default(),
        1,
        1,
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // move back to make sure the other pane is in the previous tab
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::MoveFocusLeftOrPreviousTab(1));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // move forward to make sure the broken pane is in the previous tab
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::MoveFocusRightOrNextTab(1));
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
pub fn screen_cannot_break_last_selectable_pane_to_a_new_tab() {
    let size = Size { cols: 80, rows: 20 };
    let initial_layout = TiledPaneLayout::default();
    let mut mock_screen = MockScreen::new(size);
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );

    let _ = mock_screen.to_screen.send(ScreenInstruction::BreakPane(
        Box::new(Layout::default()),
        Default::default(),
        1,
    ));
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
pub fn screen_can_break_floating_pane_to_a_new_tab() {
    let size = Size { cols: 80, rows: 20 };
    let mut initial_layout = TiledPaneLayout::default();
    let mut pane_to_break_free = TiledPaneLayout::default();
    pane_to_break_free.name = Some("tiled_pane".to_owned());
    let mut floating_pane = FloatingPaneLayout::default();
    floating_pane.name = Some("floating_pane_to_eject".to_owned());
    let mut floating_panes_layout = vec![floating_pane];
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![pane_to_break_free];
    let mut mock_screen = MockScreen::new(size);
    let screen_thread = mock_screen.run(Some(initial_layout), floating_panes_layout.clone());
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );

    let _ = mock_screen.to_screen.send(ScreenInstruction::BreakPane(
        Box::new(Layout::default()),
        Default::default(),
        1,
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // we send ApplyLayout, because in prod this is eventually received after the message traverses
    // through the plugin and pty threads (to open extra stuff we need in the layout, eg. the
    // default plugins)
    floating_panes_layout.get_mut(0).unwrap().already_running = true;
    let _ = mock_screen.to_screen.send(ScreenInstruction::ApplyLayout(
        TiledPaneLayout::default(),
        floating_panes_layout,
        vec![(1, None)], // tiled pane ids - send these because one needs to be created under the
        // ejected floating pane, lest the tab be closed as having no tiled panes
        // (this happens in prod in the pty thread)
        vec![], // floating panes ids
        Default::default(),
        1,
        1,
    ));
    std::thread::sleep(std::time::Duration::from_millis(200));
    // move back to make sure the other pane is in the previous tab
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::MoveFocusLeftOrPreviousTab(1));
    std::thread::sleep(std::time::Duration::from_millis(200));
    // move forward to make sure the broken pane is in the previous tab
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::MoveFocusRightOrNextTab(1));
    std::thread::sleep(std::time::Duration::from_millis(200));

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
pub fn screen_can_break_plugin_pane_to_a_new_tab() {
    let size = Size { cols: 80, rows: 20 };
    let mut initial_layout = TiledPaneLayout::default();
    let mut pane_to_break_free = TiledPaneLayout::default();
    pane_to_break_free.name = Some("plugin_pane_to_break_free".to_owned());
    pane_to_break_free.run = Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from("/path/to/fake/plugin")),
        configuration: Default::default(),
        ..Default::default()
    })));
    let mut pane_to_stay = TiledPaneLayout::default();
    pane_to_stay.name = Some("pane_to_stay".to_owned());
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![pane_to_break_free, pane_to_stay];
    let mut mock_screen = MockScreen::new(size);
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );

    let _ = mock_screen.to_screen.send(ScreenInstruction::BreakPane(
        Box::new(Layout::default()),
        Default::default(),
        1,
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // we send ApplyLayout, because in prod this is eventually received after the message traverses
    // through the plugin and pty threads (to open extra stuff we need in the layout, eg. the
    // default plugins)
    let _ = mock_screen.to_screen.send(ScreenInstruction::ApplyLayout(
        TiledPaneLayout::default(),
        vec![], // floating_panes_layout
        Default::default(),
        vec![], // floating panes ids
        Default::default(),
        1,
        1,
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // move back to make sure the other pane is in the previous tab
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::MoveFocusLeftOrPreviousTab(1));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // move forward to make sure the broken pane is in the previous tab
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::MoveFocusRightOrNextTab(1));
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
pub fn screen_can_break_floating_plugin_pane_to_a_new_tab() {
    let size = Size { cols: 80, rows: 20 };
    let mut initial_layout = TiledPaneLayout::default();
    let mut pane_to_break_free = TiledPaneLayout::default();
    pane_to_break_free.name = Some("tiled_pane".to_owned());
    let mut floating_pane = FloatingPaneLayout::default();
    floating_pane.name = Some("floating_plugin_pane_to_eject".to_owned());
    floating_pane.run = Some(Run::Plugin(RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(PathBuf::from("/path/to/fake/plugin")),
        configuration: Default::default(),
        ..Default::default()
    })));
    let mut floating_panes_layout = vec![floating_pane];
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![pane_to_break_free];
    let mut mock_screen = MockScreen::new(size);
    let screen_thread = mock_screen.run(Some(initial_layout), floating_panes_layout.clone());
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );

    let _ = mock_screen.to_screen.send(ScreenInstruction::BreakPane(
        Box::new(Layout::default()),
        Default::default(),
        1,
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // we send ApplyLayout, because in prod this is eventually received after the message traverses
    // through the plugin and pty threads (to open extra stuff we need in the layout, eg. the
    // default plugins)
    floating_panes_layout.get_mut(0).unwrap().already_running = true;
    let _ = mock_screen.to_screen.send(ScreenInstruction::ApplyLayout(
        TiledPaneLayout::default(),
        floating_panes_layout,
        vec![(1, None)], // tiled pane ids - send these because one needs to be created under the
        // ejected floating pane, lest the tab be closed as having no tiled panes
        // (this happens in prod in the pty thread)
        vec![], // floating panes ids
        Default::default(),
        1,
        1,
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // move back to make sure the other pane is in the previous tab
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::MoveFocusLeftOrPreviousTab(1));
    std::thread::sleep(std::time::Duration::from_millis(100));
    // move forward to make sure the broken pane is in the previous tab
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::MoveFocusRightOrNextTab(1));
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
pub fn screen_can_move_pane_to_a_new_tab_right() {
    let size = Size { cols: 80, rows: 20 };
    let mut initial_layout = TiledPaneLayout::default();
    let mut pane_to_break_free = TiledPaneLayout::default();
    pane_to_break_free.name = Some("pane_to_break_free".to_owned());
    let mut pane_to_stay = TiledPaneLayout::default();
    pane_to_stay.name = Some("pane_to_stay".to_owned());
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![pane_to_break_free, pane_to_stay];
    let mut mock_screen = MockScreen::new(size);
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );

    let _ = mock_screen.to_screen.send(ScreenInstruction::BreakPane(
        Box::new(Layout::default()),
        Default::default(),
        1,
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::MoveFocusLeftOrPreviousTab(1));
    std::thread::sleep(std::time::Duration::from_millis(100));
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::BreakPaneRight(1));
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
pub fn screen_can_move_pane_to_a_new_tab_left() {
    let size = Size { cols: 80, rows: 20 };
    let mut initial_layout = TiledPaneLayout::default();
    let mut pane_to_break_free = TiledPaneLayout::default();
    pane_to_break_free.name = Some("pane_to_break_free".to_owned());
    let mut pane_to_stay = TiledPaneLayout::default();
    pane_to_stay.name = Some("pane_to_stay".to_owned());
    initial_layout.children_split_direction = SplitDirection::Vertical;
    initial_layout.children = vec![pane_to_break_free, pane_to_stay];
    let mut mock_screen = MockScreen::new(size);
    let screen_thread = mock_screen.run(Some(initial_layout), vec![]);
    let received_server_instructions = Arc::new(Mutex::new(vec![]));
    let server_receiver = mock_screen.server_receiver.take().unwrap();
    let server_thread = log_actions_in_thread!(
        received_server_instructions,
        ServerInstruction::KillSession,
        server_receiver
    );

    let _ = mock_screen.to_screen.send(ScreenInstruction::BreakPane(
        Box::new(Layout::default()),
        Default::default(),
        1,
    ));
    std::thread::sleep(std::time::Duration::from_millis(100));
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::MoveFocusLeftOrPreviousTab(1));
    std::thread::sleep(std::time::Duration::from_millis(100));
    let _ = mock_screen
        .to_screen
        .send(ScreenInstruction::BreakPaneLeft(1));
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

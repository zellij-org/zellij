use directories_next::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::thread;
use std::{collections::HashMap, fs};
use std::{
    collections::HashSet,
    str::FromStr,
    sync::{Arc, Mutex},
};
use wasmer::{ChainableNamedResolver, Instance, Module, Store, Value};
use wasmer_wasi::{Pipe, WasiState};
use zellij_tile::data::{Event, EventType, ModeInfo};

use crate::cli::CliArgs;
use crate::client::ClientInstruction;
use crate::common::pty_bus::VteEvent;
use crate::common::{
    errors::{ContextType, ErrorContext, PluginContext, PtyContext, ScreenContext, ServerContext},
    os_input_output::ServerOsApi,
    pty_bus::{PtyBus, PtyInstruction},
    screen::{Screen, ScreenInstruction},
    wasm_vm::{wasi_stdout, wasi_write_string, zellij_imports, PluginEnv, PluginInstruction},
    ChannelWithContext, SenderType, SenderWithContext, OPENCALLS,
};
use crate::layout::Layout;
use crate::panes::PaneId;
use crate::panes::PositionAndSize;

/// Instructions related to server-side application including the
/// ones sent by client to server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerInstruction {
    OpenFile(PathBuf),
    SplitHorizontally,
    SplitVertically,
    MoveFocus,
    NewClient(String, PositionAndSize),
    ToPty(PtyInstruction),
    ToScreen(ScreenInstruction),
    Render(String),
    PluginUpdate(Option<u32>, Event),
    DoneClosingPane,
    DoneOpeningNewPane,
    DoneUpdatingTabs,
    ClientExit,
    ClientShouldExit,
    // notify router thread to exit
    Exit,
}
impl ServerInstruction {
    // ToPty
    pub fn spawn_terminal(path: Option<PathBuf>) -> Self {
        Self::ToPty(PtyInstruction::SpawnTerminal(path))
    }
    pub fn spawn_terminal_vertically(path: Option<PathBuf>) -> Self {
        Self::ToPty(PtyInstruction::SpawnTerminalVertically(path))
    }
    pub fn spawn_terminal_horizontally(path: Option<PathBuf>) -> Self {
        Self::ToPty(PtyInstruction::SpawnTerminalHorizontally(path))
    }
    pub fn pty_new_tab() -> Self {
        Self::ToPty(PtyInstruction::NewTab)
    }
    pub fn pty_close_pane(id: PaneId) -> Self {
        Self::ToPty(PtyInstruction::ClosePane(id))
    }
    pub fn pty_close_tab(ids: Vec<PaneId>) -> Self {
        Self::ToPty(PtyInstruction::CloseTab(ids))
    }
    pub fn pty_exit() -> Self {
        Self::ToPty(PtyInstruction::Exit)
    }

    // ToScreen
    pub fn render() -> Self {
        Self::ToScreen(ScreenInstruction::Render)
    }
    pub fn new_pane(id: PaneId) -> Self {
        Self::ToScreen(ScreenInstruction::NewPane(id))
    }
    pub fn horizontal_split(id: PaneId) -> Self {
        Self::ToScreen(ScreenInstruction::HorizontalSplit(id))
    }
    pub fn vertical_split(id: PaneId) -> Self {
        Self::ToScreen(ScreenInstruction::VerticalSplit(id))
    }
    pub fn write_character(chars: Vec<u8>) -> Self {
        Self::ToScreen(ScreenInstruction::WriteCharacter(chars))
    }
    pub fn resize_left() -> Self {
        Self::ToScreen(ScreenInstruction::ResizeLeft)
    }
    pub fn resize_right() -> Self {
        Self::ToScreen(ScreenInstruction::ResizeRight)
    }
    pub fn resize_down() -> Self {
        Self::ToScreen(ScreenInstruction::ResizeDown)
    }
    pub fn resize_up() -> Self {
        Self::ToScreen(ScreenInstruction::ResizeUp)
    }
    pub fn move_focus() -> Self {
        Self::ToScreen(ScreenInstruction::MoveFocus)
    }
    pub fn move_focus_left() -> Self {
        Self::ToScreen(ScreenInstruction::MoveFocusLeft)
    }
    pub fn move_focus_right() -> Self {
        Self::ToScreen(ScreenInstruction::MoveFocusRight)
    }
    pub fn move_focus_down() -> Self {
        Self::ToScreen(ScreenInstruction::MoveFocusDown)
    }
    pub fn move_focus_up() -> Self {
        Self::ToScreen(ScreenInstruction::MoveFocusUp)
    }
    pub fn screen_exit() -> Self {
        Self::ToScreen(ScreenInstruction::Exit)
    }
    pub fn scroll_up() -> Self {
        Self::ToScreen(ScreenInstruction::ScrollUp)
    }
    pub fn scroll_down() -> Self {
        Self::ToScreen(ScreenInstruction::ScrollDown)
    }
    pub fn clear_scroll() -> Self {
        Self::ToScreen(ScreenInstruction::ClearScroll)
    }
    pub fn close_focused_pane() -> Self {
        Self::ToScreen(ScreenInstruction::CloseFocusedPane)
    }
    pub fn toggle_active_terminal_fullscreen() -> Self {
        Self::ToScreen(ScreenInstruction::ToggleActiveTerminalFullscreen)
    }
    pub fn set_selectable(pane_id: PaneId, value: bool) -> Self {
        Self::ToScreen(ScreenInstruction::SetSelectable(pane_id, value))
    }
    pub fn set_max_height(pane_id: PaneId, max_height: usize) -> Self {
        Self::ToScreen(ScreenInstruction::SetMaxHeight(pane_id, max_height))
    }
    pub fn set_invisible_borders(pane_id: PaneId, value: bool) -> Self {
        Self::ToScreen(ScreenInstruction::SetInvisibleBorders(pane_id, value))
    }
    pub fn screen_close_pane(pane_id: PaneId) -> Self {
        Self::ToScreen(ScreenInstruction::ClosePane(pane_id))
    }
    pub fn apply_layout(layout: PathBuf, pids: Vec<RawFd>) -> Self {
        Self::ToScreen(ScreenInstruction::ApplyLayout(layout, pids))
    }
    pub fn screen_new_tab(fd: RawFd) -> Self {
        Self::ToScreen(ScreenInstruction::NewTab(fd))
    }
    pub fn switch_tab_prev() -> Self {
        Self::ToScreen(ScreenInstruction::SwitchTabPrev)
    }
    pub fn switch_tab_next() -> Self {
        Self::ToScreen(ScreenInstruction::SwitchTabPrev)
    }
    pub fn screen_close_tab() -> Self {
        Self::ToScreen(ScreenInstruction::CloseTab)
    }
    pub fn go_to_tab(tab_id: u32) -> Self {
        Self::ToScreen(ScreenInstruction::GoToTab(tab_id))
    }
    pub fn update_tab_name(tab_ids: Vec<u8>) -> Self {
        Self::ToScreen(ScreenInstruction::UpdateTabName(tab_ids))
    }
    pub fn change_mode(mode_info: ModeInfo) -> Self {
        Self::ToScreen(ScreenInstruction::ChangeMode(mode_info))
    }
    pub fn pty(fd: RawFd, event: VteEvent) -> Self {
        Self::ToScreen(ScreenInstruction::Pty(fd, event))
    }
    pub fn terminal_resize(new_size: PositionAndSize) -> Self {
        Self::ToScreen(ScreenInstruction::TerminalResize(new_size))
    }
}

struct ClientMetaData {
    pub send_pty_instructions: SenderWithContext<PtyInstruction>,
    pub send_screen_instructions: SenderWithContext<ScreenInstruction>,
    pub send_plugin_instructions: SenderWithContext<PluginInstruction>,
    screen_thread: Option<thread::JoinHandle<()>>,
    pty_thread: Option<thread::JoinHandle<()>>,
    wasm_thread: Option<thread::JoinHandle<()>>,
}

impl ClientMetaData {
    fn update(&mut self, err_ctx: ErrorContext) {
        self.send_plugin_instructions.update(err_ctx);
        self.send_screen_instructions.update(err_ctx);
        self.send_pty_instructions.update(err_ctx);
    }
}

impl Drop for ClientMetaData {
    fn drop(&mut self) {
        let _ = self.send_pty_instructions.send(PtyInstruction::Exit);
        let _ = self.send_screen_instructions.send(ScreenInstruction::Exit);
        let _ = self.send_plugin_instructions.send(PluginInstruction::Exit);
        let _ = self.screen_thread.take().unwrap().join();
        let _ = self.pty_thread.take().unwrap().join();
        let _ = self.wasm_thread.take().unwrap().join();
    }
}

pub fn start_server(mut os_input: Box<dyn ServerOsApi>, opts: CliArgs) -> thread::JoinHandle<()> {
    let (send_server_instructions, receive_server_instructions): ChannelWithContext<
        ServerInstruction,
    > = channel();
    let send_server_instructions = SenderWithContext::new(
        ErrorContext::new(),
        SenderType::Sender(send_server_instructions),
    );
    let router_thread = thread::Builder::new()
        .name("server_router".to_string())
        .spawn({
            let os_input = os_input.clone();
            let mut send_server_instructions = send_server_instructions.clone();
            move || loop {
                let (instruction, err_ctx) = os_input.server_recv();
                send_server_instructions.update(err_ctx);
                match instruction {
                    ServerInstruction::Exit => break,
                    _ => {
                        send_server_instructions.send(instruction).unwrap();
                    }
                }
            }
        })
        .unwrap();

    thread::Builder::new()
        .name("ipc_server".to_string())
        .spawn({
            let mut clients: HashMap<String, ClientMetaData> = HashMap::new();
            // We handle only single client for now
            let mut client: Option<String> = None;
            move || loop {
                let (instruction, mut err_ctx) = receive_server_instructions.recv().unwrap();
                err_ctx.add_call(ContextType::IPCServer(ServerContext::from(&instruction)));
                os_input.update_senders(err_ctx);
                if let Some(ref c) = client {
                    clients.get_mut(c).unwrap().update(err_ctx);
                }
                match instruction {
                    ServerInstruction::OpenFile(file_name) => {
                        let path = PathBuf::from(file_name);
                        clients[client.as_ref().unwrap()]
                            .send_pty_instructions
                            .send(PtyInstruction::SpawnTerminal(Some(path)))
                            .unwrap();
                    }
                    ServerInstruction::SplitHorizontally => {
                        clients[client.as_ref().unwrap()]
                            .send_pty_instructions
                            .send(PtyInstruction::SpawnTerminalHorizontally(None))
                            .unwrap();
                    }
                    ServerInstruction::SplitVertically => {
                        clients[client.as_ref().unwrap()]
                            .send_pty_instructions
                            .send(PtyInstruction::SpawnTerminalVertically(None))
                            .unwrap();
                    }
                    ServerInstruction::MoveFocus => {
                        clients[client.as_ref().unwrap()]
                            .send_screen_instructions
                            .send(ScreenInstruction::MoveFocus)
                            .unwrap();
                    }
                    ServerInstruction::NewClient(buffer_path, full_screen_ws) => {
                        client = Some(buffer_path.clone());
                        let client_data = init_client(
                            os_input.clone(),
                            opts.clone(),
                            send_server_instructions.clone(),
                            full_screen_ws,
                        );
                        clients.insert(buffer_path.clone(), client_data);
                        clients[client.as_ref().unwrap()]
                            .send_pty_instructions
                            .send(PtyInstruction::NewTab)
                            .unwrap();
                        os_input.add_client_sender(buffer_path);
                    }
                    ServerInstruction::ToScreen(instruction) => {
                        clients[client.as_ref().unwrap()]
                            .send_screen_instructions
                            .send(instruction)
                            .unwrap();
                    }
                    ServerInstruction::ToPty(instruction) => {
                        clients[client.as_ref().unwrap()]
                            .send_pty_instructions
                            .send(instruction)
                            .unwrap();
                    }
                    ServerInstruction::DoneClosingPane => {
                        os_input.send_to_client(ClientInstruction::DoneClosingPane);
                    }
                    ServerInstruction::DoneOpeningNewPane => {
                        os_input.send_to_client(ClientInstruction::DoneOpeningNewPane);
                    }
                    ServerInstruction::DoneUpdatingTabs => {
                        os_input.send_to_client(ClientInstruction::DoneUpdatingTabs);
                    }
                    ServerInstruction::ClientShouldExit => {
                        os_input.send_to_client(ClientInstruction::Exit);
                    }
                    ServerInstruction::PluginUpdate(pid, event) => {
                        clients[client.as_ref().unwrap()]
                            .send_plugin_instructions
                            .send(PluginInstruction::Update(pid, event))
                            .unwrap();
                    }
                    ServerInstruction::ClientExit => {
                        clients.remove(client.as_ref().unwrap()).unwrap();
                        os_input.server_exit();
                        let _ = router_thread.join();
                        let _ = os_input.send_to_client(ClientInstruction::Exit);
                        break;
                    }
                    ServerInstruction::Render(output) => {
                        os_input.send_to_client(ClientInstruction::Render(output))
                    }
                    _ => {}
                }
            }
        })
        .unwrap()
}

fn init_client(
    os_input: Box<dyn ServerOsApi>,
    opts: CliArgs,
    send_server_instructions: SenderWithContext<ServerInstruction>,
    full_screen_ws: PositionAndSize,
) -> ClientMetaData {
    let err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());
    let (send_screen_instructions, receive_screen_instructions): ChannelWithContext<
        ScreenInstruction,
    > = channel();
    let send_screen_instructions =
        SenderWithContext::new(err_ctx, SenderType::Sender(send_screen_instructions));

    let (send_plugin_instructions, receive_plugin_instructions): ChannelWithContext<
        PluginInstruction,
    > = channel();
    let send_plugin_instructions =
        SenderWithContext::new(err_ctx, SenderType::Sender(send_plugin_instructions));
    let (send_pty_instructions, receive_pty_instructions): ChannelWithContext<PtyInstruction> =
        channel();
    let send_pty_instructions =
        SenderWithContext::new(err_ctx, SenderType::Sender(send_pty_instructions));

    // Don't use default layouts in tests, but do everywhere else
    #[cfg(not(test))]
    let default_layout = Some(PathBuf::from("default"));
    #[cfg(test)]
    let default_layout = None;
    let maybe_layout = opts.layout.or(default_layout);

    let mut pty_bus = PtyBus::new(
        receive_pty_instructions,
        os_input.clone(),
        send_screen_instructions.clone(),
        send_plugin_instructions.clone(),
        opts.debug,
    );

    let pty_thread = thread::Builder::new()
        .name("pty".to_string())
        .spawn({
            let send_server_instructions = send_server_instructions.clone();
            move || loop {
                let (event, mut err_ctx) = pty_bus
                    .receive_pty_instructions
                    .recv()
                    .expect("failed to receive event on channel");
                err_ctx.add_call(ContextType::Pty(PtyContext::from(&event)));
                match event {
                    PtyInstruction::SpawnTerminal(file_to_open) => {
                        let pid = pty_bus.spawn_terminal(file_to_open);
                        pty_bus
                            .send_screen_instructions
                            .send(ScreenInstruction::NewPane(PaneId::Terminal(pid)))
                            .unwrap();
                    }
                    PtyInstruction::SpawnTerminalVertically(file_to_open) => {
                        let pid = pty_bus.spawn_terminal(file_to_open);
                        pty_bus
                            .send_screen_instructions
                            .send(ScreenInstruction::VerticalSplit(PaneId::Terminal(pid)))
                            .unwrap();
                    }
                    PtyInstruction::SpawnTerminalHorizontally(file_to_open) => {
                        let pid = pty_bus.spawn_terminal(file_to_open);
                        pty_bus
                            .send_screen_instructions
                            .send(ScreenInstruction::HorizontalSplit(PaneId::Terminal(pid)))
                            .unwrap();
                    }
                    PtyInstruction::NewTab => {
                        if let Some(layout) = maybe_layout.clone() {
                            pty_bus.spawn_terminals_for_layout(layout);
                        } else {
                            let pid = pty_bus.spawn_terminal(None);
                            pty_bus
                                .send_screen_instructions
                                .send(ScreenInstruction::NewTab(pid))
                                .unwrap();
                        }
                    }
                    PtyInstruction::ClosePane(id) => {
                        pty_bus.close_pane(id);
                        send_server_instructions
                            .send(ServerInstruction::DoneClosingPane)
                            .unwrap();
                    }
                    PtyInstruction::CloseTab(ids) => {
                        pty_bus.close_tab(ids);
                        send_server_instructions
                            .send(ServerInstruction::DoneClosingPane)
                            .unwrap();
                    }
                    PtyInstruction::Exit => {
                        break;
                    }
                }
            }
        })
        .unwrap();

    let screen_thread = thread::Builder::new()
        .name("screen".to_string())
        .spawn({
            let os_input = os_input.clone();
            let send_plugin_instructions = send_plugin_instructions.clone();
            let send_pty_instructions = send_pty_instructions.clone();
            let send_server_instructions = send_server_instructions.clone();
            let max_panes = opts.max_panes;

            move || {
                let mut screen = Screen::new(
                    receive_screen_instructions,
                    send_plugin_instructions,
                    send_pty_instructions,
                    send_server_instructions,
                    &full_screen_ws,
                    os_input,
                    max_panes,
                    ModeInfo::default(),
                );
                loop {
                    let (event, mut err_ctx) = screen
                        .receiver
                        .recv()
                        .expect("failed to receive event on channel");
                    err_ctx.add_call(ContextType::Screen(ScreenContext::from(&event)));
                    screen.send_server_instructions.update(err_ctx);
                    screen.send_pty_instructions.update(err_ctx);
                    screen.send_plugin_instructions.update(err_ctx);
                    match event {
                        ScreenInstruction::Pty(pid, vte_event) => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .handle_pty_event(pid, vte_event);
                        }
                        ScreenInstruction::Render => {
                            screen.render();
                        }
                        ScreenInstruction::NewPane(pid) => {
                            screen.get_active_tab_mut().unwrap().new_pane(pid);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::DoneOpeningNewPane)
                                .unwrap();
                        }
                        ScreenInstruction::HorizontalSplit(pid) => {
                            screen.get_active_tab_mut().unwrap().horizontal_split(pid);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::DoneOpeningNewPane)
                                .unwrap();
                        }
                        ScreenInstruction::VerticalSplit(pid) => {
                            screen.get_active_tab_mut().unwrap().vertical_split(pid);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::DoneOpeningNewPane)
                                .unwrap();
                        }
                        ScreenInstruction::WriteCharacter(bytes) => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .write_to_active_terminal(bytes);
                        }
                        ScreenInstruction::ResizeLeft => {
                            screen.get_active_tab_mut().unwrap().resize_left();
                        }
                        ScreenInstruction::ResizeRight => {
                            screen.get_active_tab_mut().unwrap().resize_right();
                        }
                        ScreenInstruction::ResizeDown => {
                            screen.get_active_tab_mut().unwrap().resize_down();
                        }
                        ScreenInstruction::ResizeUp => {
                            screen.get_active_tab_mut().unwrap().resize_up();
                        }
                        ScreenInstruction::MoveFocus => {
                            screen.get_active_tab_mut().unwrap().move_focus();
                        }
                        ScreenInstruction::MoveFocusLeft => {
                            screen.get_active_tab_mut().unwrap().move_focus_left();
                        }
                        ScreenInstruction::MoveFocusDown => {
                            screen.get_active_tab_mut().unwrap().move_focus_down();
                        }
                        ScreenInstruction::MoveFocusRight => {
                            screen.get_active_tab_mut().unwrap().move_focus_right();
                        }
                        ScreenInstruction::MoveFocusUp => {
                            screen.get_active_tab_mut().unwrap().move_focus_up();
                        }
                        ScreenInstruction::ScrollUp => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .scroll_active_terminal_up();
                        }
                        ScreenInstruction::ScrollDown => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .scroll_active_terminal_down();
                        }
                        ScreenInstruction::ClearScroll => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .clear_active_terminal_scroll();
                        }
                        ScreenInstruction::CloseFocusedPane => {
                            screen.get_active_tab_mut().unwrap().close_focused_pane();
                            screen.render();
                        }
                        ScreenInstruction::SetSelectable(id, selectable) => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .set_pane_selectable(id, selectable);
                        }
                        ScreenInstruction::SetMaxHeight(id, max_height) => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .set_pane_max_height(id, max_height);
                        }
                        ScreenInstruction::SetInvisibleBorders(id, invisible_borders) => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .set_pane_invisible_borders(id, invisible_borders);
                            screen.render();
                        }
                        ScreenInstruction::ClosePane(id) => {
                            screen.get_active_tab_mut().unwrap().close_pane(id);
                            screen.render();
                        }
                        ScreenInstruction::ToggleActiveTerminalFullscreen => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .toggle_active_pane_fullscreen();
                        }
                        ScreenInstruction::NewTab(pane_id) => {
                            screen.new_tab(pane_id);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::DoneUpdatingTabs)
                                .unwrap();
                        }
                        ScreenInstruction::SwitchTabNext => {
                            screen.switch_tab_next();
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::DoneUpdatingTabs)
                                .unwrap();
                        }
                        ScreenInstruction::SwitchTabPrev => {
                            screen.switch_tab_prev();
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::DoneUpdatingTabs)
                                .unwrap();
                        }
                        ScreenInstruction::CloseTab => {
                            screen.close_tab();
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::DoneUpdatingTabs)
                                .unwrap();
                        }
                        ScreenInstruction::ApplyLayout(layout, new_pane_pids) => {
                            screen.apply_layout(Layout::new(layout), new_pane_pids);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::DoneUpdatingTabs)
                                .unwrap();
                        }
                        ScreenInstruction::GoToTab(tab_index) => {
                            screen.go_to_tab(tab_index as usize);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::DoneUpdatingTabs)
                                .unwrap();
                        }
                        ScreenInstruction::UpdateTabName(c) => {
                            screen.update_active_tab_name(c);
                            screen
                                .send_server_instructions
                                .send(ServerInstruction::DoneUpdatingTabs)
                                .unwrap();
                        }
                        ScreenInstruction::ChangeMode(mode_info) => {
                            screen.change_mode(mode_info);
                        }
                        ScreenInstruction::TerminalResize(new_size) => {
                            screen.resize_to_screen(new_size);
                        }
                        ScreenInstruction::Exit => {
                            break;
                        }
                    }
                }
            }
        })
        .unwrap();

    let wasm_thread = thread::Builder::new()
        .name("wasm".to_string())
        .spawn({
            let mut send_screen_instructions = send_screen_instructions.clone();
            let mut send_pty_instructions = send_pty_instructions.clone();

            let store = Store::default();
            let mut plugin_id = 0;
            let mut plugin_map = HashMap::new();
            move || loop {
                let (event, mut err_ctx) = receive_plugin_instructions
                    .recv()
                    .expect("failed to receive event on channel");
                err_ctx.add_call(ContextType::Plugin(PluginContext::from(&event)));
                send_screen_instructions.update(err_ctx);
                send_pty_instructions.update(err_ctx);
                match event {
                    PluginInstruction::Load(pid_tx, path) => {
                        let project_dirs =
                            ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
                        let plugin_dir = project_dirs.data_dir().join("plugins/");
                        let wasm_bytes = fs::read(&path)
                            .or_else(|_| fs::read(&path.with_extension("wasm")))
                            .or_else(|_| fs::read(&plugin_dir.join(&path).with_extension("wasm")))
                            .unwrap_or_else(|_| panic!("cannot find plugin {}", &path.display()));

                        // FIXME: Cache this compiled module on disk. I could use `(de)serialize_to_file()` for that
                        let module = Module::new(&store, &wasm_bytes).unwrap();

                        let output = Pipe::new();
                        let input = Pipe::new();
                        let mut wasi_env = WasiState::new("Zellij")
                            .env("CLICOLOR_FORCE", "1")
                            .preopen(|p| {
                                p.directory(".") // FIXME: Change this to a more meaningful dir
                                    .alias(".")
                                    .read(true)
                                    .write(true)
                                    .create(true)
                            })
                            .unwrap()
                            .stdin(Box::new(input))
                            .stdout(Box::new(output))
                            .finalize()
                            .unwrap();

                        let wasi = wasi_env.import_object(&module).unwrap();

                        let plugin_env = PluginEnv {
                            plugin_id,
                            send_screen_instructions: send_screen_instructions.clone(),
                            send_pty_instructions: send_pty_instructions.clone(),
                            wasi_env,
                            subscriptions: Arc::new(Mutex::new(HashSet::new())),
                        };

                        let zellij = zellij_imports(&store, &plugin_env);
                        let instance = Instance::new(&module, &zellij.chain_back(wasi)).unwrap();

                        let start = instance.exports.get_function("_start").unwrap();

                        // This eventually calls the `.init()` method
                        start.call(&[]).unwrap();

                        plugin_map.insert(plugin_id, (instance, plugin_env));
                        pid_tx.send(plugin_id).unwrap();
                        plugin_id += 1;
                    }
                    PluginInstruction::Update(pid, event) => {
                        for (&i, (instance, plugin_env)) in &plugin_map {
                            let subs = plugin_env.subscriptions.lock().unwrap();
                            // FIXME: This is very janky... Maybe I should write my own macro for Event -> EventType?
                            let event_type = EventType::from_str(&event.to_string()).unwrap();
                            if (pid.is_none() || pid == Some(i)) && subs.contains(&event_type) {
                                let update = instance.exports.get_function("update").unwrap();
                                wasi_write_string(
                                    &plugin_env.wasi_env,
                                    &serde_json::to_string(&event).unwrap(),
                                );
                                update.call(&[]).unwrap();
                            }
                        }
                        drop(send_screen_instructions.send(ScreenInstruction::Render));
                    }
                    PluginInstruction::Render(buf_tx, pid, rows, cols) => {
                        let (instance, plugin_env) = plugin_map.get(&pid).unwrap();

                        let render = instance.exports.get_function("render").unwrap();

                        render
                            .call(&[Value::I32(rows as i32), Value::I32(cols as i32)])
                            .unwrap();

                        buf_tx.send(wasi_stdout(&plugin_env.wasi_env)).unwrap();
                    }
                    PluginInstruction::Unload(pid) => drop(plugin_map.remove(&pid)),
                    PluginInstruction::Exit => break,
                }
            }
        })
        .unwrap();
    ClientMetaData {
        send_plugin_instructions,
        send_screen_instructions,
        send_pty_instructions,
        screen_thread: Some(screen_thread),
        pty_thread: Some(pty_thread),
        wasm_thread: Some(wasm_thread),
    }
}

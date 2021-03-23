pub mod command_is_executing;
pub mod errors;
pub mod input;
pub mod ipc;
pub mod os_input_output;
pub mod pty_bus;
pub mod screen;
pub mod setup;
pub mod utils;
pub mod wasm_vm;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::{collections::HashMap, fs};

use crate::panes::PaneId;
use directories_next::ProjectDirs;
use input::handler::InputMode;
use serde::{Deserialize, Serialize};
use termion::input::TermRead;
use wasm_vm::PluginEnv;
use wasmer::{ChainableNamedResolver, Instance, Module, Store, Value};
use wasmer_wasi::{Pipe, WasiState};

use crate::cli::CliArgs;
use crate::layout::Layout;
use crate::server::start_server;
use command_is_executing::CommandIsExecuting;
use errors::{AppContext, ContextType, ErrorContext, PluginContext, ScreenContext};
use input::handler::input_loop;
use os_input_output::{ClientOsApi, ServerOsApi, ServerOsApiInstruction};
use pty_bus::PtyInstruction;
use screen::{Screen, ScreenInstruction};
use utils::consts::ZELLIJ_ROOT_PLUGIN_DIR;
use wasm_vm::{
    wasi_stdout, wasi_write_string, zellij_imports, EventType, PluginInputType, PluginInstruction,
};

pub const IPC_BUFFER_SIZE: u32 = 8192;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerInstruction {
    OpenFile(PathBuf),
    SplitHorizontally,
    SplitVertically,
    MoveFocus,
    NewClient(String),
    ToPty(PtyInstruction),
    ToScreen(ScreenInstruction),
    OsApi(ServerOsApiInstruction),
    DoneClosingPane,
    ClosePluginPane(u32),
    Exit,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientInstruction {
    ToScreen(ScreenInstruction),
    ClosePluginPane(u32),
    Error(String),
    DoneClosingPane,
    Exit,
}

// FIXME: It would be good to add some more things to this over time
#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub input_mode: InputMode,
}

// FIXME: Make this a method on the big `Communication` struct, so that app_tx can be extracted
// from self instead of being explicitly passed here
pub fn update_state(
    app_tx: &SenderWithContext<AppInstruction>,
    update_fn: impl FnOnce(AppState) -> AppState,
) {
    let (state_tx, state_rx) = mpsc::channel();

    drop(app_tx.send(AppInstruction::GetState(state_tx)));
    let state = state_rx.recv().unwrap();

    drop(app_tx.send(AppInstruction::SetState(update_fn(state))))
}

/// An [MPSC](mpsc) asynchronous channel with added error context.
pub type ChannelWithContext<T> = (
    mpsc::Sender<(T, ErrorContext)>,
    mpsc::Receiver<(T, ErrorContext)>,
);
/// An [MPSC](mpsc) synchronous channel with added error context.
pub type SyncChannelWithContext<T> = (
    mpsc::SyncSender<(T, ErrorContext)>,
    mpsc::Receiver<(T, ErrorContext)>,
);

/// Wrappers around the two standard [MPSC](mpsc) sender types, [`mpsc::Sender`] and [`mpsc::SyncSender`], with an additional [`ErrorContext`].
#[derive(Clone)]
pub enum SenderType<T: Clone> {
    /// A wrapper around an [`mpsc::Sender`], adding an [`ErrorContext`].
    Sender(mpsc::Sender<(T, ErrorContext)>),
    /// A wrapper around an [`mpsc::SyncSender`], adding an [`ErrorContext`].
    SyncSender(mpsc::SyncSender<(T, ErrorContext)>),
}

/// Sends messages on an [MPSC](std::sync::mpsc) channel, along with an [`ErrorContext`],
/// synchronously or asynchronously depending on the underlying [`SenderType`].
#[derive(Clone)]
pub struct SenderWithContext<T: Clone> {
    sender: SenderType<T>,
}

impl<T: Clone> SenderWithContext<T> {
    pub fn new(err_ctx: ErrorContext, sender: SenderType<T>) -> Self {
        Self { err_ctx, sender }
    }

    /// Sends an event, along with the current [`ErrorContext`], on this
    /// [`SenderWithContext`]'s channel.
    pub fn send(&self, event: T) -> Result<(), mpsc::SendError<(T, ErrorContext)>> {
        let err_ctx = get_current_ctx();
        match self.sender {
            SenderType::Sender(ref s) => s.send((event, err_ctx)),
            SenderType::SyncSender(ref s) => s.send((event, err_ctx)),
        }
    }
}

unsafe impl<T: Clone> Send for SenderWithContext<T> {}
unsafe impl<T: Clone> Sync for SenderWithContext<T> {}

thread_local!(
    /// A key to some thread local storage (TLS) that holds a representation of the thread's call
    /// stack in the form of an [`ErrorContext`].
    static OPENCALLS: RefCell<ErrorContext> = RefCell::default()
);

task_local! {
    /// A key to some task local storage that holds a representation of the task's call
    /// stack in the form of an [`ErrorContext`].
    static ASYNCOPENCALLS: RefCell<ErrorContext> = RefCell::default()
}

/// Instructions related to the entire application.
#[derive(Clone)]
pub enum AppInstruction {
    Exit,
    Error(String),
    ToPty(PtyInstruction),
    ToScreen(ScreenInstruction),
    ToPlugin(PluginInstruction),
    OsApi(ServerOsApiInstruction),
    DoneClosingPane,
}

impl From<ClientInstruction> for AppInstruction {
    fn from(item: ClientInstruction) -> Self {
        match item {
            ClientInstruction::ToScreen(s) => AppInstruction::ToScreen(s),
            ClientInstruction::Error(e) => AppInstruction::Error(e),
            ClientInstruction::ClosePluginPane(p) => {
                AppInstruction::ToPlugin(PluginInstruction::Unload(p))
            }
            ClientInstruction::DoneClosingPane => AppInstruction::DoneClosingPane,
            ClientInstruction::Exit => AppInstruction::Exit,
        }
    }
}

/// Start Zellij with the specified [`OsApi`] and command-line arguments.
// FIXME this should definitely be modularized and split into different functions.
pub fn start(
    mut os_input: Box<dyn ClientOsApi>,
    opts: CliArgs,
    server_os_input: Box<dyn ServerOsApi>,
) {
    let ipc_thread = start_server(server_os_input, opts.clone());

    let take_snapshot = "\u{1b}[?1049h";
    os_input.unset_raw_mode(0);
    let _ = os_input
        .get_stdout_writer()
        .write(take_snapshot.as_bytes())
        .unwrap();

    let mut command_is_executing = CommandIsExecuting::new();

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    os_input.set_raw_mode(0);
    let (send_screen_instructions, receive_screen_instructions): ChannelWithContext<
        ScreenInstruction,
    > = mpsc::channel();
    let send_screen_instructions =
        SenderWithContext::new(SenderType::Sender(send_screen_instructions));

    let (send_plugin_instructions, receive_plugin_instructions): ChannelWithContext<
        PluginInstruction,
    > = mpsc::channel();
    let send_plugin_instructions =
        SenderWithContext::new(SenderType::Sender(send_plugin_instructions));

    let (send_app_instructions, receive_app_instructions): SyncChannelWithContext<AppInstruction> =
        mpsc::sync_channel(500);
    let mut send_app_instructions =
        SenderWithContext::new(err_ctx, SenderType::SyncSender(send_app_instructions));

    os_input.notify_server();

    #[cfg(not(test))]
    std::panic::set_hook({
        use crate::errors::handle_panic;
        let send_app_instructions = send_app_instructions.clone();
        Box::new(move |info| {
            handle_panic(info, &send_app_instructions);
        })
    });

    let screen_thread = thread::Builder::new()
        .name("screen".to_string())
        .spawn({
            let mut command_is_executing = command_is_executing.clone();
            let os_input = os_input.clone();
            let send_plugin_instructions = send_plugin_instructions.clone();
            let send_app_instructions = send_app_instructions.clone();
            let max_panes = opts.max_panes;
            let colors = os_input.load_palette();
            move || {
                let mut screen = Screen::new(
                    receive_screen_instructions,
                    send_plugin_instructions,
                    send_app_instructions,
                    &full_screen_ws,
                    os_input,
                    max_panes,
                    ModeInfo {
                        palette: colors,
                        ..ModeInfo::default()
                    },
                    InputMode::Normal,
                    colors,
                );
                loop {
                    let (event, mut err_ctx) = screen
                        .receiver
                        .recv()
                        .expect("failed to receive event on channel");
                    err_ctx.add_call(ContextType::Screen(ScreenContext::from(&event)));
                    screen.send_app_instructions.update(err_ctx);
                    match event {
                        ScreenInstruction::PtyBytes(pid, vte_bytes) => {
                            let active_tab = screen.get_active_tab_mut().unwrap();
                            if active_tab.has_terminal_pid(pid) {
                                // it's most likely that this event is directed at the active tab
                                // look there first
                                active_tab.handle_pty_bytes(pid, vte_bytes);
                            } else {
                                // if this event wasn't directed at the active tab, start looking
                                // in other tabs
                                let all_tabs = screen.get_tabs_mut();
                                for tab in all_tabs.values_mut() {
                                    if tab.has_terminal_pid(pid) {
                                        tab.handle_pty_bytes(pid, vte_bytes);
                                        break;
                                    }
                                }
                            }
                        }
                        ScreenInstruction::Render => {
                            screen.render();
                        }
                        ScreenInstruction::NewPane(pid) => {
                            screen.get_active_tab_mut().unwrap().new_pane(pid);
                            command_is_executing.done_opening_new_pane();
                        }
                        ScreenInstruction::HorizontalSplit(pid) => {
                            screen.get_active_tab_mut().unwrap().horizontal_split(pid);
                            command_is_executing.done_opening_new_pane();
                        }
                        ScreenInstruction::VerticalSplit(pid) => {
                            screen.get_active_tab_mut().unwrap().vertical_split(pid);
                            command_is_executing.done_opening_new_pane();
                        }
                        ScreenInstruction::WriteCharacter(bytes) => {
                            let active_tab = screen.get_active_tab_mut().unwrap();
                            match active_tab.is_sync_panes_active() {
                                true => active_tab.write_to_terminals_on_current_tab(bytes),
                                false => active_tab.write_to_active_terminal(bytes),
                            }
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
                        ScreenInstruction::SwitchFocus => {
                            screen.get_active_tab_mut().unwrap().move_focus();
                        }
                        ScreenInstruction::FocusNextPane => {
                            screen.get_active_tab_mut().unwrap().focus_next_pane();
                        }
                        ScreenInstruction::FocusPreviousPane => {
                            screen.get_active_tab_mut().unwrap().focus_previous_pane();
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
                        ScreenInstruction::PageScrollUp => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .scroll_active_terminal_up_page();
                        }
                        ScreenInstruction::PageScrollDown => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .scroll_active_terminal_down_page();
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
                            command_is_executing.done_opening_new_pane();
                        }
                        ScreenInstruction::SwitchTabNext => screen.switch_tab_next(),
                        ScreenInstruction::SwitchTabPrev => screen.switch_tab_prev(),
                        ScreenInstruction::CloseTab => {
                            screen.close_tab();
                        }
                        ScreenInstruction::ApplyLayout((layout, new_pane_pids)) => {
                            screen.apply_layout(Layout::new(layout), new_pane_pids);
                            command_is_executing.done_opening_new_pane();
                        }
                        ScreenInstruction::GoToTab(tab_index) => {
                            screen.go_to_tab(tab_index as usize)
                        }
                        ScreenInstruction::UpdateTabName(c) => {
                            screen.update_active_tab_name(c);
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
            let mut send_app_instructions = send_app_instructions.clone();

            let store = Store::default();
            let mut plugin_id = 0;
            let mut plugin_map = HashMap::new();
            move || loop {
                let (event, mut err_ctx) = receive_plugin_instructions
                    .recv()
                    .expect("failed to receive event on channel");
                err_ctx.add_call(ContextType::Plugin(PluginContext::from(&event)));
                send_screen_instructions.update(err_ctx);
                send_app_instructions.update(err_ctx);
                match event {
                    PluginInstruction::Load(pid_tx, path) => {
                        let plugin_dir = data_dir.join("plugins/");
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
                            send_app_instructions: send_app_instructions.clone(),
                            send_plugin_instructions: send_plugin_instructions.clone(),
                            wasi_env,
                            subscriptions: Arc::new(Mutex::new(HashSet::new())),
                        };

                        let zellij = zellij_exports(&store, &plugin_env);
                        let instance = Instance::new(&module, &zellij.chain_back(wasi)).unwrap();

                        let start = instance.exports.get_function("_start").unwrap();

                        // This eventually calls the `.load()` method
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
                                wasi_write_object(&plugin_env.wasi_env, &event);
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

                        buf_tx.send(wasi_read_string(&plugin_env.wasi_env)).unwrap();
                    }
                    PluginInstruction::Unload(pid) => drop(plugin_map.remove(&pid)),
                    PluginInstruction::Exit => break,
                }
            }
        })
        .unwrap();

    let _stdin_thread = thread::Builder::new()
        .name("stdin_handler".to_string())
        .spawn({
            let send_screen_instructions = send_screen_instructions.clone();
            let send_plugin_instructions = send_plugin_instructions.clone();
            let send_app_instructions = send_app_instructions.clone();
            let command_is_executing = command_is_executing.clone();
            let os_input = os_input.clone();
            let config = config;
            move || {
                input_loop(
                    os_input,
                    config,
                    command_is_executing,
                    send_screen_instructions,
                    send_plugin_instructions,
                    send_app_instructions,
                )
            }
        });

    let router_thread = thread::Builder::new()
        .name("router".to_string())
        .spawn({
            let os_input = os_input.clone();
            move || loop {
                let (instruction, err_ctx) = os_input.client_recv();
                send_app_instructions.update(err_ctx);
                match instruction {
                    ClientInstruction::Exit => break,
                    _ => {
                        send_app_instructions
                            .send(AppInstruction::from(instruction))
                            .unwrap();
                    }
                }
            }
        })
        .unwrap();

    #[warn(clippy::never_loop)]
    loop {
        let (app_instruction, mut err_ctx) = receive_app_instructions
            .recv()
            .expect("failed to receive app instruction on channel");

        err_ctx.add_call(ContextType::App(AppContext::from(&app_instruction)));
        send_screen_instructions.update(err_ctx);
        os_input.update_senders(err_ctx);
        match app_instruction {
            AppInstruction::GetState(state_tx) => drop(state_tx.send(app_state.clone())),
            AppInstruction::SetState(state) => app_state = state,
            AppInstruction::Exit => break,
            AppInstruction::Error(backtrace) => {
                let _ = os_input.send_to_server(ServerInstruction::Exit);
                let _ = send_screen_instructions.send(ScreenInstruction::Exit);
                let _ = send_plugin_instructions.send(PluginInstruction::Exit);
                let _ = screen_thread.join();
                let _ = wasm_thread.join();
                let _ = ipc_thread.join();
                //let _ = router_thread.join();
                os_input.unset_raw_mode(0);
                let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
                let error = format!("{}\n{}", goto_start_of_last_line, backtrace);
                let _ = os_input
                    .get_stdout_writer()
                    .write(error.as_bytes())
                    .unwrap();
                std::process::exit(1);
            }
            AppInstruction::ToScreen(instruction) => {
                send_screen_instructions.send(instruction).unwrap();
            }
            AppInstruction::ToPlugin(instruction) => {
                send_plugin_instructions.send(instruction).unwrap();
            }
            AppInstruction::ToPty(instruction) => {
                let _ = os_input.send_to_server(ServerInstruction::ToPty(instruction));
            }
            AppInstruction::OsApi(instruction) => {
                let _ = os_input.send_to_server(ServerInstruction::OsApi(instruction));
            }
            AppInstruction::DoneClosingPane => command_is_executing.done_closing_pane(),
        }
    }

    let _ = os_input.send_to_server(ServerInstruction::Exit);
    let _ = send_screen_instructions.send(ScreenInstruction::Exit);
    let _ = send_plugin_instructions.send(PluginInstruction::Exit);
    screen_thread.join().unwrap();
    wasm_thread.join().unwrap();
    ipc_thread.join().unwrap();
    router_thread.join().unwrap();

    // cleanup();
    let reset_style = "\u{1b}[m";
    let show_cursor = "\u{1b}[?25h";
    let restore_snapshot = "\u{1b}[?1049l";
    let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
    let goodbye_message = format!(
        "{}\n{}{}{}Bye from Zellij!\n",
        goto_start_of_last_line, restore_snapshot, reset_style, show_cursor
    );

    os_input.unset_raw_mode(0);
    let _ = os_input
        .get_stdout_writer()
        .write(goodbye_message.as_bytes())
        .unwrap();
    os_input.get_stdout_writer().flush().unwrap();
}

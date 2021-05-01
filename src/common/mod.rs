pub mod command_is_executing;
pub mod errors;
pub mod input;
pub mod install;
pub mod ipc;
pub mod os_input_output;
pub mod pty;
pub mod screen;
pub mod utils;
pub mod wasm_vm;

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::{collections::HashMap, fs};
use std::{
    collections::HashSet,
    env,
    io::Write,
    str::FromStr,
    sync::{Arc, Mutex},
};

use crate::cli::CliArgs;
use crate::common::input::config::Config;
use crate::layout::Layout;
use crate::panes::PaneId;
use async_std::task_local;
use command_is_executing::CommandIsExecuting;
use directories_next::ProjectDirs;
use errors::{
    get_current_ctx, AppContext, ContextType, ErrorContext, PluginContext, ScreenContext,
};
use input::handler::input_loop;
use install::populate_data_dir;
use os_input_output::OsApi;
use pty::{pty_thread_main, Pty, PtyInstruction};
use screen::{Screen, ScreenInstruction};
use serde::{Deserialize, Serialize};
use utils::consts::ZELLIJ_IPC_PIPE;
use wasm_vm::{wasi_read_string, wasi_write_object, zellij_exports, PluginEnv, PluginInstruction};
use wasmer::{ChainableNamedResolver, Instance, Module, Store, Value};
use wasmer_wasi::{Pipe, WasiState};
use zellij_tile::data::{EventType, ModeInfo};

#[derive(Serialize, Deserialize, Debug)]
pub enum ApiCommand {
    OpenFile(PathBuf),
    SplitHorizontally,
    SplitVertically,
    MoveFocus,
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
enum SenderType<T: Clone> {
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
    fn new(sender: SenderType<T>) -> Self {
        Self { sender }
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
}

#[derive(Clone)]
pub struct ThreadSenders {
    to_screen: Option<SenderWithContext<ScreenInstruction>>,
    to_pty: Option<SenderWithContext<PtyInstruction>>,
    to_plugin: Option<SenderWithContext<PluginInstruction>>,
    to_app: Option<SenderWithContext<AppInstruction>>,
}

pub struct Bus<T> {
    receiver: Option<mpsc::Receiver<(T, ErrorContext)>>,
    senders: ThreadSenders,
    os_input: Option<Box<dyn OsApi>>,
}

impl<T> Bus<T> {
    fn new(
        receiver: Option<mpsc::Receiver<(T, ErrorContext)>>,
        to_screen: Option<&SenderWithContext<ScreenInstruction>>,
        to_pty: Option<&SenderWithContext<PtyInstruction>>,
        to_plugin: Option<&SenderWithContext<PluginInstruction>>,
        to_app: Option<&SenderWithContext<AppInstruction>>,
        os_input: Option<Box<dyn OsApi>>,
    ) -> Self {
        Bus {
            receiver,
            senders: ThreadSenders {
                to_screen: to_screen.cloned(),
                to_pty: to_pty.cloned(),
                to_plugin: to_plugin.cloned(),
                to_app: to_app.cloned(),
            },
            os_input: os_input.clone(),
        }
    }
}

/// Start Zellij with the specified [`OsApi`] and command-line arguments.
// FIXME this should definitely be modularized and split into different functions.
pub fn start(mut os_input: Box<dyn OsApi>, opts: CliArgs) {
    let take_snapshot = "\u{1b}[?1049h";
    os_input.unset_raw_mode(0);
    let _ = os_input
        .get_stdout_writer()
        .write(take_snapshot.as_bytes())
        .unwrap();

    env::set_var(&"ZELLIJ", "0");

    let config_dir = opts.config_dir.or_else(install::default_config_dir);

    let config = Config::from_cli_config(opts.config, opts.option, config_dir)
        .map_err(|e| {
            eprintln!("There was an error in the config file:\n{}", e);
            std::process::exit(1);
        })
        .unwrap();

    let command_is_executing = CommandIsExecuting::new();

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    os_input.set_raw_mode(0);
    let (to_screen, from_screen): ChannelWithContext<ScreenInstruction> = mpsc::channel();
    let to_screen = SenderWithContext::new(SenderType::Sender(to_screen));

    let (to_pty, from_pty): ChannelWithContext<PtyInstruction> = mpsc::channel();
    let to_pty = SenderWithContext::new(SenderType::Sender(to_pty));

    let (to_plugin, from_plugin): ChannelWithContext<PluginInstruction> = mpsc::channel();
    let to_plugin = SenderWithContext::new(SenderType::Sender(to_plugin));

    let (to_app, from_app): SyncChannelWithContext<AppInstruction> = mpsc::sync_channel(0);
    let to_app = SenderWithContext::new(SenderType::SyncSender(to_app));

    // Determine and initialize the data directory
    let project_dirs = ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
    let data_dir = opts
        .data_dir
        .unwrap_or_else(|| project_dirs.data_dir().to_path_buf());
    populate_data_dir(&data_dir);

    // Don't use default layouts in tests, but do everywhere else
    #[cfg(not(test))]
    let default_layout = Some(PathBuf::from("default"));
    #[cfg(test)]
    let default_layout = None;
    let maybe_layout = opts
        .layout
        .map(|p| Layout::new(&p, &data_dir))
        .or_else(|| default_layout.map(|p| Layout::from_defaults(&p, &data_dir)));

    #[cfg(not(test))]
    std::panic::set_hook({
        use crate::errors::handle_panic;
        let to_app = to_app.clone();
        Box::new(move |info| {
            handle_panic(info, &to_app);
        })
    });

    let pty_thread = thread::Builder::new()
        .name("pty".to_string())
        .spawn({
            let pty = Pty::new(
                Bus::new(
                    Some(from_pty),
                    Some(&to_screen),
                    None,
                    Some(&to_plugin),
                    None,
                    Some(os_input.clone()),
                ),
                opts.debug,
            );

            let command_is_executing = command_is_executing.clone();
            move || pty_thread_main(pty, command_is_executing, maybe_layout)
        })
        .unwrap();

    let screen_thread = thread::Builder::new()
        .name("screen".to_string())
        .spawn({
            let mut command_is_executing = command_is_executing.clone();
            let screen_bus = Bus::new(
                Some(from_screen),
                None,
                Some(&to_pty),
                Some(&to_plugin),
                Some(&to_app),
                Some(os_input.clone()),
            );
            let max_panes = opts.max_panes;

            move || {
                let mut screen =
                    Screen::new(screen_bus, &full_screen_ws, max_panes, ModeInfo::default());
                loop {
                    let (event, mut err_ctx) = screen
                        .bus
                        .receiver
                        .as_ref()
                        .unwrap()
                        .recv()
                        .expect("failed to receive event on channel");
                    err_ctx.add_call(ContextType::Screen(ScreenContext::from(&event)));
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
                        ScreenInstruction::CloseTab => screen.close_tab(),
                        ScreenInstruction::ApplyLayout((layout, new_pane_pids)) => {
                            screen.apply_layout(layout, new_pane_pids);
                            command_is_executing.done_opening_new_pane();
                        }
                        ScreenInstruction::GoToTab(tab_index) => {
                            screen.go_to_tab(tab_index as usize)
                        }
                        ScreenInstruction::UpdateTabName(c) => {
                            screen.update_active_tab_name(c);
                        }
                        ScreenInstruction::TerminalResize => {
                            screen.resize_to_screen();
                        }
                        ScreenInstruction::ChangeMode(mode_info) => {
                            screen.change_mode(mode_info);
                        }
                        ScreenInstruction::ToggleActiveSyncPanes => {
                            screen
                                .get_active_tab_mut()
                                .unwrap()
                                .toggle_sync_panes_is_active();
                            screen.update_tabs();
                        }
                        ScreenInstruction::Quit => {
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
            let plugin_bus = Bus::new(
                Some(from_plugin),
                Some(&to_screen),
                Some(&to_pty),
                Some(&to_plugin),
                Some(&to_app),
                None,
            );

            let store = Store::default();
            let mut plugin_id = 0;
            let mut plugin_map = HashMap::new();
            move || loop {
                let (event, mut err_ctx) = plugin_bus
                    .receiver
                    .as_ref()
                    .unwrap()
                    .recv()
                    .expect("failed to receive event on channel");
                err_ctx.add_call(ContextType::Plugin(PluginContext::from(&event)));
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
                            senders: plugin_bus.senders.clone(),
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
                        drop(
                            plugin_bus
                                .senders
                                .to_screen
                                .as_ref()
                                .unwrap()
                                .send(ScreenInstruction::Render),
                        );
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
                    PluginInstruction::Quit => break,
                }
            }
        })
        .unwrap();

    let _signal_thread = thread::Builder::new()
        .name("signal_listener".to_string())
        .spawn({
            let os_input = os_input.clone();
            let to_screen = to_screen.clone();
            move || {
                os_input.receive_sigwinch(Box::new(move || {
                    let _ = to_screen.send(ScreenInstruction::TerminalResize);
                }));
            }
        })
        .unwrap();

    // TODO: currently we don't wait for this to quit
    // because otherwise the app will hang. Need to fix this so it both
    // listens to the ipc-bus and is able to quit cleanly
    #[cfg(not(test))]
    let _ipc_thread = thread::Builder::new()
        .name("ipc_server".to_string())
        .spawn({
            use std::io::Read;
            let to_pty = to_pty.clone();
            let to_screen = to_screen.clone();
            move || {
                std::fs::remove_file(ZELLIJ_IPC_PIPE).ok();
                let listener = std::os::unix::net::UnixListener::bind(ZELLIJ_IPC_PIPE)
                    .expect("could not listen on ipc socket");
                let mut err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());
                err_ctx.add_call(ContextType::IpcServer);

                for stream in listener.incoming() {
                    match stream {
                        Ok(mut stream) => {
                            let mut buffer = [0; 65535]; // TODO: more accurate
                            let _ = stream
                                .read(&mut buffer)
                                .expect("failed to parse ipc message");
                            let decoded: ApiCommand = bincode::deserialize(&buffer)
                                .expect("failed to deserialize ipc message");
                            match &decoded {
                                ApiCommand::OpenFile(file_name) => {
                                    let path = PathBuf::from(file_name);
                                    to_pty
                                        .send(PtyInstruction::SpawnTerminal(Some(path)))
                                        .unwrap();
                                }
                                ApiCommand::SplitHorizontally => {
                                    to_pty
                                        .send(PtyInstruction::SpawnTerminalHorizontally(None))
                                        .unwrap();
                                }
                                ApiCommand::SplitVertically => {
                                    to_pty
                                        .send(PtyInstruction::SpawnTerminalVertically(None))
                                        .unwrap();
                                }
                                ApiCommand::MoveFocus => {
                                    to_screen.send(ScreenInstruction::FocusNextPane).unwrap();
                                }
                            }
                        }
                        Err(err) => {
                            panic!("err {:?}", err);
                        }
                    }
                }
            }
        })
        .unwrap();

    let _stdin_thread = thread::Builder::new()
        .name("stdin_handler".to_string())
        .spawn({
            let to_screen = to_screen.clone();
            let to_pty = to_pty.clone();
            let to_plugin = to_plugin.clone();
            let os_input = os_input.clone();
            let config = config;
            move || {
                input_loop(
                    os_input,
                    config,
                    command_is_executing,
                    to_screen,
                    to_pty,
                    to_plugin,
                    to_app,
                )
            }
        });

    #[warn(clippy::never_loop)]
    loop {
        let (app_instruction, mut err_ctx) = from_app
            .recv()
            .expect("failed to receive app instruction on channel");

        err_ctx.add_call(ContextType::App(AppContext::from(&app_instruction)));
        match app_instruction {
            AppInstruction::Exit => {
                break;
            }
            AppInstruction::Error(backtrace) => {
                let _ = to_screen.send(ScreenInstruction::Quit);
                let _ = screen_thread.join();
                let _ = to_pty.send(PtyInstruction::Quit);
                let _ = pty_thread.join();
                let _ = to_plugin.send(PluginInstruction::Quit);
                let _ = wasm_thread.join();
                os_input.unset_raw_mode(0);
                let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
                let restore_snapshot = "\u{1b}[?1049l";
                let error = format!(
                    "{}\n{}{}",
                    goto_start_of_last_line, restore_snapshot, backtrace
                );
                let _ = os_input
                    .get_stdout_writer()
                    .write(error.as_bytes())
                    .unwrap();
                std::process::exit(1);
            }
        }
    }

    let _ = to_pty.send(PtyInstruction::Quit);
    pty_thread.join().unwrap();
    let _ = to_screen.send(ScreenInstruction::Quit);
    screen_thread.join().unwrap();
    let _ = to_plugin.send(PluginInstruction::Quit);
    wasm_thread.join().unwrap();

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

pub mod command_is_executing;
pub mod errors;
pub mod input;
pub mod install;
pub mod ipc;
pub mod os_input_output;
pub mod pty_bus;
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
    io::Write,
    str::FromStr,
    sync::{Arc, Mutex},
};

use crate::cli::CliArgs;
use crate::layout::Layout;
use crate::panes::PaneId;
use colors_transform::{Color, Rgb};
use command_is_executing::CommandIsExecuting;
use directories_next::ProjectDirs;
use errors::{AppContext, ContextType, ErrorContext, PluginContext, PtyContext, ScreenContext};
use input::handler::input_loop;
use os_input_output::OsApi;
use pty_bus::{PtyBus, PtyInstruction};
use screen::{Screen, ScreenInstruction};
use serde::{Deserialize, Serialize};
use utils::consts::ZELLIJ_IPC_PIPE;
use wasm_vm::PluginEnv;
use wasm_vm::{wasi_stdout, wasi_write_string, zellij_imports, PluginInstruction};
use wasmer::{ChainableNamedResolver, Instance, Module, Store, Value};
use wasmer_wasi::{Pipe, WasiState};
use xrdb::Colors;
use zellij_tile::data::{EventType, InputMode, ModeInfo, Palette, Theme};

use self::utils::logging::debug_log_to_file;

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
    err_ctx: ErrorContext,
    sender: SenderType<T>,
}

impl<T: Clone> SenderWithContext<T> {
    fn new(err_ctx: ErrorContext, sender: SenderType<T>) -> Self {
        Self { err_ctx, sender }
    }

    /// Sends an event, along with the current [`ErrorContext`], on this
    /// [`SenderWithContext`]'s channel.
    pub fn send(&self, event: T) -> Result<(), mpsc::SendError<(T, ErrorContext)>> {
        match self.sender {
            SenderType::Sender(ref s) => s.send((event, self.err_ctx)),
            SenderType::SyncSender(ref s) => s.send((event, self.err_ctx)),
        }
    }

    /// Updates this [`SenderWithContext`]'s [`ErrorContext`]. This is the way one adds
    /// a call to the error context.
    ///
    /// Updating [`ErrorContext`]s works in this way so that these contexts are only ever
    /// allocated on the stack (which is thread-specific), and not on the heap.
    pub fn update(&mut self, new_ctx: ErrorContext) {
        self.err_ctx = new_ctx;
    }
}

unsafe impl<T: Clone> Send for SenderWithContext<T> {}
unsafe impl<T: Clone> Sync for SenderWithContext<T> {}

thread_local!(
    /// A key to some thread local storage (TLS) that holds a representation of the thread's call
    /// stack in the form of an [`ErrorContext`].
    static OPENCALLS: RefCell<ErrorContext> = RefCell::default()
);

/// Instructions related to the entire application.
#[derive(Clone)]
pub enum AppInstruction {
    Exit,
    Error(String),
}

pub mod colors {
    pub const WHITE: (u8, u8, u8) = (238, 238, 238);
    pub const GREEN: (u8, u8, u8) = (175, 255, 0);
    pub const GRAY: (u8, u8, u8) = (68, 68, 68);
    pub const BRIGHT_GRAY: (u8, u8, u8) = (138, 138, 138);
    pub const RED: (u8, u8, u8) = (135, 0, 0);
    pub const BLACK: (u8, u8, u8) = (0, 0, 0);
}

pub fn detect_theme(bg: (u8, u8, u8)) -> Theme {
    let (r, g, b) = bg;
    // HSP, P stands for perceived brightness
    let hsp: f64 = (0.299 * (r as f64 * r as f64)
        + 0.587 * (g as f64 * g as f64)
        + 0.114 * (b as f64 * b as f64))
        .sqrt();
    match hsp > 127.5 {
        true => Theme::Light,
        false => Theme::Dark,
    }
}

pub fn load_palette() -> Palette {
    let palette = match Colors::new("xresources") {
        Some(colors) => {
            let fg = colors.fg.unwrap();
            let fg_imm = &fg;
            let fg_hex: &str = &fg_imm;
            let fg = Rgb::from_hex_str(fg_hex).unwrap().as_tuple();
            let fg = (fg.0 as u8, fg.1 as u8, fg.2 as u8);
            let bg = colors.bg.unwrap();
            let bg_imm = &bg;
            let bg_hex: &str = &bg_imm;
            let bg = Rgb::from_hex_str(bg_hex).unwrap().as_tuple();
            let bg = (bg.0 as u8, bg.1 as u8, bg.2 as u8);
            let colors: Vec<(u8, u8, u8)> = colors
                .colors
                .iter()
                .map(|c| {
                    let c = c.clone();
                    let imm_str = &c.unwrap();
                    let hex_str: &str = &imm_str;
                    let rgb = Rgb::from_hex_str(hex_str).unwrap().as_tuple();
                    (rgb.0 as u8, rgb.1 as u8, rgb.2 as u8)
                })
                .collect();
            let theme = detect_theme(bg);
            debug_log_to_file(format!(
                "{:?} {:?}, white: {:?}, black: {:?}, fg: {:?}",
                theme, bg, colors[7], colors[0], fg
            ));
            Palette {
                theme,
                fg,
                bg,
                black: colors[0],
                red: colors[1],
                green: colors[2],
                yellow: colors[3],
                blue: colors[4],
                magenta: colors[5],
                cyan: colors[6],
                white: colors[7],
            }
        }
        None => Palette {
            theme: Theme::Dark,
            fg: colors::BRIGHT_GRAY,
            bg: colors::BLACK,
            black: colors::BLACK,
            red: colors::RED,
            green: colors::GREEN,
            yellow: colors::GRAY,
            blue: colors::GRAY,
            magenta: colors::GRAY,
            cyan: colors::GRAY,
            white: colors::WHITE,
        },
    };
    palette
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

    let command_is_executing = CommandIsExecuting::new();

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    os_input.set_raw_mode(0);
    let (send_screen_instructions, receive_screen_instructions): ChannelWithContext<
        ScreenInstruction,
    > = mpsc::channel();
    let err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());
    let mut send_screen_instructions =
        SenderWithContext::new(err_ctx, SenderType::Sender(send_screen_instructions));

    let (send_pty_instructions, receive_pty_instructions): ChannelWithContext<PtyInstruction> =
        mpsc::channel();
    let mut send_pty_instructions =
        SenderWithContext::new(err_ctx, SenderType::Sender(send_pty_instructions));

    let (send_plugin_instructions, receive_plugin_instructions): ChannelWithContext<
        PluginInstruction,
    > = mpsc::channel();
    let send_plugin_instructions =
        SenderWithContext::new(err_ctx, SenderType::Sender(send_plugin_instructions));

    let (send_app_instructions, receive_app_instructions): SyncChannelWithContext<AppInstruction> =
        mpsc::sync_channel(0);
    let send_app_instructions =
        SenderWithContext::new(err_ctx, SenderType::SyncSender(send_app_instructions));

    let mut pty_bus = PtyBus::new(
        receive_pty_instructions,
        send_screen_instructions.clone(),
        send_plugin_instructions.clone(),
        os_input.clone(),
        opts.debug,
    );

    // Don't use default layouts in tests, but do everywhere else
    #[cfg(not(test))]
    let default_layout = Some(PathBuf::from("default"));
    #[cfg(test)]
    let default_layout = None;
    let maybe_layout = opts.layout.or(default_layout).map(Layout::new);

    #[cfg(not(test))]
    std::panic::set_hook({
        use crate::errors::handle_panic;
        let send_app_instructions = send_app_instructions.clone();
        Box::new(move |info| {
            handle_panic(info, &send_app_instructions);
        })
    });

    let pty_thread = thread::Builder::new()
        .name("pty".to_string())
        .spawn({
            let mut command_is_executing = command_is_executing.clone();
            send_pty_instructions.send(PtyInstruction::NewTab).unwrap();
            move || loop {
                let (event, mut err_ctx) = pty_bus
                    .receive_pty_instructions
                    .recv()
                    .expect("failed to receive event on channel");
                err_ctx.add_call(ContextType::Pty(PtyContext::from(&event)));
                pty_bus.send_screen_instructions.update(err_ctx);
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
                        command_is_executing.done_closing_pane();
                    }
                    PtyInstruction::CloseTab(ids) => {
                        pty_bus.close_tab(ids);
                        command_is_executing.done_closing_pane();
                    }
                    PtyInstruction::Quit => {
                        break;
                    }
                }
            }
        })
        .unwrap();

    let screen_thread = thread::Builder::new()
        .name("screen".to_string())
        .spawn({
            let mut command_is_executing = command_is_executing.clone();
            let os_input = os_input.clone();
            let send_pty_instructions = send_pty_instructions.clone();
            let send_plugin_instructions = send_plugin_instructions.clone();
            let send_app_instructions = send_app_instructions.clone();
            let max_panes = opts.max_panes;
            let colors = load_palette();
            // debug_log_to_file(format!("{:?}", colors));
            move || {
                let mut screen = Screen::new(
                    receive_screen_instructions,
                    send_pty_instructions,
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
                    screen.send_pty_instructions.update(err_ctx);
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
            let mut send_pty_instructions = send_pty_instructions.clone();
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
                send_pty_instructions.update(err_ctx);
                send_app_instructions.update(err_ctx);
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
                            send_pty_instructions: send_pty_instructions.clone(),
                            send_screen_instructions: send_screen_instructions.clone(),
                            send_app_instructions: send_app_instructions.clone(),
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
                    PluginInstruction::Quit => break,
                }
            }
        })
        .unwrap();

    let _signal_thread = thread::Builder::new()
        .name("signal_listener".to_string())
        .spawn({
            let os_input = os_input.clone();
            let send_screen_instructions = send_screen_instructions.clone();
            move || {
                os_input.receive_sigwinch(Box::new(move || {
                    let _ = send_screen_instructions.send(ScreenInstruction::TerminalResize);
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
            let mut send_pty_instructions = send_pty_instructions.clone();
            let mut send_screen_instructions = send_screen_instructions.clone();
            move || {
                std::fs::remove_file(ZELLIJ_IPC_PIPE).ok();
                let listener = std::os::unix::net::UnixListener::bind(ZELLIJ_IPC_PIPE)
                    .expect("could not listen on ipc socket");
                let mut err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());
                err_ctx.add_call(ContextType::IpcServer);
                send_pty_instructions.update(err_ctx);
                send_screen_instructions.update(err_ctx);

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
                                    send_pty_instructions
                                        .send(PtyInstruction::SpawnTerminal(Some(path)))
                                        .unwrap();
                                }
                                ApiCommand::SplitHorizontally => {
                                    send_pty_instructions
                                        .send(PtyInstruction::SpawnTerminalHorizontally(None))
                                        .unwrap();
                                }
                                ApiCommand::SplitVertically => {
                                    send_pty_instructions
                                        .send(PtyInstruction::SpawnTerminalVertically(None))
                                        .unwrap();
                                }
                                ApiCommand::MoveFocus => {
                                    send_screen_instructions
                                        .send(ScreenInstruction::MoveFocus)
                                        .unwrap();
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
            let send_screen_instructions = send_screen_instructions.clone();
            let send_pty_instructions = send_pty_instructions.clone();
            let send_plugin_instructions = send_plugin_instructions.clone();
            let os_input = os_input.clone();
            move || {
                input_loop(
                    os_input,
                    command_is_executing,
                    send_screen_instructions,
                    send_pty_instructions,
                    send_plugin_instructions,
                    send_app_instructions,
                )
            }
        });

    #[warn(clippy::never_loop)]
    loop {
        let (app_instruction, mut err_ctx) = receive_app_instructions
            .recv()
            .expect("failed to receive app instruction on channel");

        err_ctx.add_call(ContextType::App(AppContext::from(&app_instruction)));
        send_screen_instructions.update(err_ctx);
        send_pty_instructions.update(err_ctx);
        match app_instruction {
            AppInstruction::Exit => {
                break;
            }
            AppInstruction::Error(backtrace) => {
                let _ = send_screen_instructions.send(ScreenInstruction::Quit);
                let _ = screen_thread.join();
                let _ = send_pty_instructions.send(PtyInstruction::Quit);
                let _ = pty_thread.join();
                let _ = send_plugin_instructions.send(PluginInstruction::Quit);
                let _ = wasm_thread.join();
                os_input.unset_raw_mode(0);
                let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
                let error = format!("{}\n{}", goto_start_of_last_line, backtrace);
                let _ = os_input
                    .get_stdout_writer()
                    .write(error.as_bytes())
                    .unwrap();
                std::process::exit(1);
            }
        }
    }

    let _ = send_pty_instructions.send(PtyInstruction::Quit);
    pty_thread.join().unwrap();
    let _ = send_screen_instructions.send(ScreenInstruction::Quit);
    screen_thread.join().unwrap();
    let _ = send_plugin_instructions.send(PluginInstruction::Quit);
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

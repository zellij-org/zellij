#[cfg(test)]
mod tests;

mod boundaries;
mod command_is_executing;
mod daemon;
mod errors;
mod input;
mod layout;
mod os_input_output;
mod pty_bus;
mod screen;
mod tab;
mod terminal_pane;
mod utils;
#[cfg(feature = "wasm-wip")]
mod wasm_vm;

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread;

use ipc_channel::ipc::{channel, IpcOneShotServer, IpcReceiver, IpcSender};
use ipc_channel::Error;
use serde::{Deserialize, Serialize};
use structopt::StructOpt;

use crate::command_is_executing::CommandIsExecuting;
use crate::daemon::{start_daemon, ServerInstruction};
use crate::errors::{AppContext, ContextType, ErrorContext, ScreenContext};
use crate::input::input_loop;
use crate::layout::Layout;
use crate::os_input_output::{get_os_input, OsApi};
use crate::pty_bus::{PtyInstruction, VteEvent};
use crate::screen::{Screen, ScreenInstruction};
use crate::utils::{
    consts::{MOSAIC_IPC_PIPE, MOSAIC_TMP_DIR, MOSAIC_TMP_LOG_DIR},
    logging::*,
};
use std::cell::RefCell;

thread_local!(static OPENCALLS: RefCell<ErrorContext> = RefCell::new(ErrorContext::new()));

pub type ClientId = usize;

#[derive(Clone, Serialize, Deserialize)]
pub struct SenderWithContext<T: Serialize> {
    client_id: ClientId,
    err_ctx: ErrorContext,
    sender: IpcSender<(ClientId, T, ErrorContext)>,
}

impl<T: Serialize> SenderWithContext<T> {
    fn new(
        client_id: ClientId,
        err_ctx: ErrorContext,
        sender: IpcSender<(ClientId, T, ErrorContext)>,
    ) -> Self {
        Self {
            client_id,
            err_ctx,
            sender,
        }
    }

    pub fn send(&self, event: T) -> Result<(), Error> {
        self.sender.send((self.client_id, event, self.err_ctx))
    }

    pub fn update_ctx(&mut self, new_ctx: ErrorContext) {
        self.err_ctx = new_ctx;
    }

    pub fn update_id(&mut self, new_id: ClientId) {
        self.client_id = new_id;
    }
}

impl<T: Serialize> std::fmt::Debug for SenderWithContext<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        f.debug_struct("SenderWithContext")
            .field("err_ctx", &self.err_ctx)
            .field("sender", &String::from(".."))
            .finish()
    }
}

#[derive(StructOpt, Debug, Default)]
#[structopt(name = "mosaic")]
pub struct Opt {
    #[structopt(short, long)]
    /// Send "split (direction h == horizontal / v == vertical)" to active mosaic session
    split: Option<char>,
    #[structopt(short, long)]
    /// Send "move focused pane" to active mosaic session
    move_focus: bool,
    #[structopt(short, long)]
    /// Send "open file in new pane" to active mosaic session
    open_file: Option<PathBuf>,
    #[structopt(long)]
    /// Maximum panes on screen, caution: opening more panes will close old ones
    max_panes: Option<usize>,
    #[structopt(short, long)]
    /// Path to a layout yaml file
    layout: Option<PathBuf>,
    #[structopt(short, long)]
    debug: bool,
    #[structopt(long)]
    daemon: bool,
}

pub fn main() {
    let opts = Opt::from_args();
    if opts.daemon {
        let os_input = get_os_input();
        start_daemon(Box::new(os_input), opts);
    }
    /*else if let Some(file_to_open) = opts.open_file {
        let mut stream = UnixStream::connect(MOSAIC_IPC_PIPE).unwrap();
        let server_instr = bincode::serialize(&ServerInstruction::OpenFile(file_to_open)).unwrap();
        stream.write_all(&server_instr).unwrap();
    }*/
    else {
        let os_input = get_os_input();
        atomic_create_dir(MOSAIC_TMP_DIR).unwrap();
        atomic_create_dir(MOSAIC_TMP_LOG_DIR).unwrap();
        start(Box::new(os_input), opts);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppInstruction {
    InitClient {
        app_sender: SenderWithContext<AppInstruction>,
        pty_sender: SenderWithContext<PtyInstruction>,
        client_id: ClientId,
    },
    Exit,
    Error(String),
}

pub fn start(mut os_input: Box<dyn OsApi>, opts: Opt) {
    let mut stream = if let Ok(stream) = UnixStream::connect(MOSAIC_IPC_PIPE) {
        stream
    } else {
        let _ = Command::new(std::env::current_exe().unwrap())
            .arg("--daemon")
            .spawn()
            .unwrap();
        while UnixStream::connect(MOSAIC_IPC_PIPE).is_err() {}
        UnixStream::connect(MOSAIC_IPC_PIPE).unwrap()
    };

    let err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());

    let (app_server, token) =
        IpcOneShotServer::<(ClientId, AppInstruction, ErrorContext)>::new().unwrap();
    let server_instr = bincode::serialize(&ServerInstruction::NewClient(token)).unwrap();
    stream.write_all(&server_instr).unwrap();

    let (send_screen_instructions, receive_screen_instructions): (
        IpcSender<(ClientId, ScreenInstruction, ErrorContext)>,
        IpcReceiver<(ClientId, ScreenInstruction, ErrorContext)>,
    ) = channel().unwrap();
    let mut send_screen_instructions = SenderWithContext::new(0, err_ctx, send_screen_instructions);

    let mut active_threads = vec![];

    let command_is_executing = CommandIsExecuting::new();

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    os_input.into_raw_mode(0);

    let (receive_app_instructions, app_instr) = app_server.accept().unwrap();
    let (mut send_app_instructions, mut send_pty_instructions, client_id) = match app_instr {
        (
            _,
            AppInstruction::InitClient {
                app_sender,
                pty_sender,
                client_id,
            },
            _,
        ) => (app_sender, pty_sender, client_id),
        _ => panic!("Received wrong message from server"),
    };

    send_app_instructions.update_id(client_id);
    send_app_instructions.update_ctx(err_ctx);
    send_screen_instructions.update_id(client_id);
    send_pty_instructions.update_id(client_id);

    send_pty_instructions
        .send(PtyInstruction::NewScreen(send_screen_instructions.clone()))
        .unwrap();

    #[cfg(feature = "wasm-wip")]
    use crate::wasm_vm::PluginInstruction;
    #[cfg(feature = "wasm-wip")]
    let (send_plugin_instructions, receive_plugin_instructions): (
        IpcSender<(ClientId, PluginInstruction, ErrorContext)>,
        IpcReceiver<(ClientId, PluginInstruction, ErrorContext)>,
    ) = channel();
    #[cfg(feature = "wasm-wip")]
    let send_plugin_instructions =
        SenderWithContext::new(client_id, err_ctx, send_plugin_instructions);

    let maybe_layout = opts.layout.map(Layout::new);

    let (panic_sender, panic_receiver): (SyncSender<AppInstruction>, Receiver<AppInstruction>) =
        sync_channel(0);
    #[cfg(not(test))]
    std::panic::set_hook({
        use crate::errors::handle_panic;
        Box::new(move |info| {
            handle_panic(info, Some(&panic_sender));
        })
    });

    if let Some(layout) = maybe_layout {
        #[cfg(feature = "wasm-wip")]
        for plugin_path in layout.list_plugins() {
            dbg!(send_plugin_instructions.send(PluginInstruction::Load(plugin_path.clone())))
                .unwrap();
        }

        send_pty_instructions
            .send(PtyInstruction::SpawnLayout(layout))
            .unwrap();
    } else {
        send_pty_instructions.send(PtyInstruction::NewTab).unwrap();
    }

    active_threads.push(
        thread::Builder::new()
            .name("screen".to_string())
            .spawn({
                let mut command_is_executing = command_is_executing.clone();
                let os_input = os_input.clone();
                let send_pty_instructions = send_pty_instructions.clone();
                let send_app_instructions = send_app_instructions.clone();
                let max_panes = opts.max_panes;

                move || {
                    let mut screen = Screen::new(
                        receive_screen_instructions,
                        send_pty_instructions,
                        send_app_instructions,
                        &full_screen_ws,
                        os_input,
                        max_panes,
                    );
                    loop {
                        let (_, event, mut err_ctx) = screen
                            .receiver
                            .recv()
                            .expect("failed to receive event on channel");
                        err_ctx.add_call(ContextType::Screen(ScreenContext::from(&event)));
                        screen.send_app_instructions.update_ctx(err_ctx);
                        screen.send_pty_instructions.update_ctx(err_ctx);
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
                                command_is_executing.done_closing_pane();
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
                                    .toggle_active_terminal_fullscreen();
                            }
                            ScreenInstruction::NewTab(pane_id) => {
                                screen.new_tab(pane_id);
                                command_is_executing.done_opening_new_pane();
                            }
                            ScreenInstruction::SwitchTabNext => screen.switch_tab_next(),
                            ScreenInstruction::SwitchTabPrev => screen.switch_tab_prev(),
                            ScreenInstruction::CloseTab => {
                                screen.close_tab();
                                command_is_executing.done_closing_pane();
                            }
                            ScreenInstruction::ApplyLayout((layout, new_pane_pids)) => {
                                screen.apply_layout(layout, new_pane_pids)
                            }
                            ScreenInstruction::Quit => {
                                break;
                            }
                        }
                    }
                }
            })
            .unwrap(),
    );

    // Here be dragons! This is very much a work in progress, and isn't quite functional
    // yet. It's being left out of the tests because is slows them down massively (by
    // recompiling a WASM module for every single test). Stay tuned for more updates!
    #[cfg(feature = "wasm-wip")]
    active_threads.push(
        thread::Builder::new()
            .name("wasm".to_string())
            .spawn(move || {
                use crate::errors::PluginContext;
                use crate::wasm_vm::{mosaic_imports, wasi_stdout};
                use std::io;
                use wasmer::{ChainableNamedResolver, Instance, Module, Store, Value};
                use wasmer_wasi::{Pipe, WasiState};

                let store = Store::default();

                loop {
                    let (event, mut err_ctx) = receive_plugin_instructions
                        .recv()
                        .expect("failed to receive event on channel");
                    err_ctx.add_call(ContextType::Plugin(PluginContext::from(&event)));
                    // FIXME: Clueless on how many of these lines I need...
                    // screen.send_app_instructions.update_ctx(err_ctx);
                    // screen.send_pty_instructions.update_ctx(err_ctx);
                    match event {
                        PluginInstruction::Load(path) => {
                            // FIXME: Cache this compiled module on disk. I could use `(de)serialize_to_file()` for that
                            let module = Module::from_file(&store, path).unwrap();

                            let output = Pipe::new();
                            let input = Pipe::new();
                            let mut wasi_env = WasiState::new("mosaic")
                                .env("CLICOLOR_FORCE", "1")
                                .preopen(|p| {
                                    p.directory(".") // FIXME: Change this to a more meaningful dir
                                        .alias(".")
                                        .read(true)
                                        .write(true)
                                        .create(true)
                                }).unwrap()
                                .stdin(Box::new(input))
                                .stdout(Box::new(output))
                                .finalize().unwrap();

                            let wasi = wasi_env.import_object(&module).unwrap();
                            let mosaic = mosaic_imports(&store, &wasi_env);
                            let instance = Instance::new(&module, &mosaic.chain_back(wasi)).unwrap();

                            let start = instance.exports.get_function("_start").unwrap();
                            let handle_key = instance.exports.get_function("handle_key").unwrap();
                            let draw = instance.exports.get_function("draw").unwrap();

                            // This eventually calls the `.init()` method
                            start.call(&[]).unwrap();

                            #[warn(clippy::never_loop)]
                            loop {
                                let (cols, rows) = (80, 24); //terminal::size()?;
                                draw.call(&[Value::I32(rows), Value::I32(cols)]).unwrap();

                                // Needed because raw mode doesn't implicitly return to the start of the line
                                write!(
                                    io::stdout(),
                                    "{}\n\r",
                                    wasi_stdout(&wasi_env)
                                        .lines()
                                        .collect::<Vec<_>>()
                                        .join("\n\r")
                                ).unwrap();

                                /* match event::read().unwrap() {
                                    Event::Key(KeyEvent {
                                        code: KeyCode::Char('q'),
                                        ..
                                    }) => break,
                                    Event::Key(e) => {
                                        wasi_write_string(&wasi_env, serde_json::to_string(&e).unwrap());
                                        handle_key.call(&[])?;
                                    }
                                    _ => (),
                                } */
                                break;
                            }
                            debug_log_to_file("WASM module loaded and exited cleanly :)".to_string()).unwrap();
                        }
                        PluginInstruction::Quit => break,
                        i => panic!("Yo, dawg, nice job calling the wasm thread!\n {:?} is defo not implemented yet...", i),
                    }
                }
            }
        ).unwrap(),
    );

    let _stdin_thread = thread::Builder::new()
        .name("stdin_handler".to_string())
        .spawn({
            let send_screen_instructions = send_screen_instructions.clone();
            let send_pty_instructions = send_pty_instructions.clone();
            let os_input = os_input.clone();
            move || {
                input_loop(
                    os_input,
                    command_is_executing,
                    send_screen_instructions,
                    send_pty_instructions,
                    send_app_instructions,
                )
            }
        });

    #[warn(clippy::never_loop)]
    loop {
        let app_instruction = panic_receiver.try_recv().map_or_else(
            |_| {
                receive_app_instructions
                    .try_recv()
                    .ok()
                    .map(|(_, app_instruction, mut err_ctx)| {
                        err_ctx.add_call(ContextType::App(AppContext::from(&app_instruction)));
                        send_screen_instructions.update_ctx(err_ctx);
                        send_pty_instructions.update_ctx(err_ctx);
                        app_instruction
                    })
            },
            |a| Some(a),
        );
        if let Some(instr) = app_instruction {
            match instr {
                AppInstruction::Exit => {
                    let _ = send_screen_instructions.send(ScreenInstruction::Quit);
                    let api_command =
                        bincode::serialize(&ServerInstruction::ClientExit(client_id)).unwrap();
                    stream.write_all(&api_command).unwrap();
                    #[cfg(feature = "wasm-wip")]
                    let _ = send_plugin_instructions.send(PluginInstruction::Quit);
                    break;
                }
                AppInstruction::Error(backtrace) => {
                    let _ = send_screen_instructions.send(ScreenInstruction::Quit);
                    #[cfg(feature = "wasm-wip")]
                    let _ = send_plugin_instructions.send(PluginInstruction::Quit);
                    let api_command =
                        bincode::serialize(&ServerInstruction::ClientExit(client_id)).unwrap();
                    stream.write_all(&api_command);
                    os_input.unset_raw_mode(0);
                    let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
                    let error = format!("{}\n{}", goto_start_of_last_line, backtrace);
                    let _ = os_input
                        .get_stdout_writer()
                        .write(error.as_bytes())
                        .unwrap();
                    for thread_handler in active_threads {
                        let _ = thread_handler.join();
                    }
                    std::process::exit(1);
                }
                _ => panic!("Received unexpected message: {:?}", instr),
            }
        }
    }

    for thread_handler in active_threads {
        thread_handler.join().unwrap();
    }
    // cleanup();
    let reset_style = "\u{1b}[m";
    let show_cursor = "\u{1b}[?25h";
    let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
    let goodbye_message = format!(
        "{}\n{}{}Bye from Mosaic!",
        goto_start_of_last_line, reset_style, show_cursor
    );

    os_input.unset_raw_mode(0);
    let _ = os_input
        .get_stdout_writer()
        .write(goodbye_message.as_bytes())
        .unwrap();
    os_input.get_stdout_writer().flush().unwrap();
}

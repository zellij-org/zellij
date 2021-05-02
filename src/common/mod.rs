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

use std::{cell::RefCell, sync::mpsc::RecvError};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::{env, io::Write};

use crate::cli::CliArgs;
use crate::common::input::config::Config;
use crate::layout::Layout;
use crate::panes::PaneId;
use async_std::task_local;
use command_is_executing::CommandIsExecuting;
use directories_next::ProjectDirs;
use errors::{get_current_ctx, AppContext, ContextType, ErrorContext};
use input::handler::input_loop;
use install::populate_data_dir;
use os_input_output::OsApi;
use pty::{pty_thread_main, Pty, PtyInstruction};
use screen::{screen_thread_main, ScreenInstruction};
use serde::{Deserialize, Serialize};
use utils::consts::ZELLIJ_IPC_PIPE;
use wasm_vm::{wasm_thread_main, PluginInstruction};
use wasmer::Store;

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

impl ThreadSenders {
    fn send_to_screen(
        &self,
        instruction: ScreenInstruction,
    ) -> Result<(), mpsc::SendError<(ScreenInstruction, ErrorContext)>> {
        self.to_screen.as_ref().unwrap().send(instruction)
    }

    fn send_to_pty(
        &self,
        instruction: PtyInstruction,
    ) -> Result<(), mpsc::SendError<(PtyInstruction, ErrorContext)>> {
        self.to_pty.as_ref().unwrap().send(instruction)
    }

    fn send_to_plugin(
        &self,
        instruction: PluginInstruction,
    ) -> Result<(), mpsc::SendError<(PluginInstruction, ErrorContext)>> {
        self.to_plugin.as_ref().unwrap().send(instruction)
    }

    fn send_to_app(
        &self,
        instruction: AppInstruction,
    ) -> Result<(), mpsc::SendError<(AppInstruction, ErrorContext)>> {
        self.to_app.as_ref().unwrap().send(instruction)
    }
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

    fn recv(&self) -> Result<(T, ErrorContext), RecvError> {
        self.receiver.as_ref().unwrap().recv()
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
            let screen_bus = Bus::new(
                Some(from_screen),
                None,
                Some(&to_pty),
                Some(&to_plugin),
                Some(&to_app),
                Some(os_input.clone()),
            );
            let command_is_executing = command_is_executing.clone();
            let max_panes = opts.max_panes;

            move || screen_thread_main(screen_bus, command_is_executing, max_panes, full_screen_ws)
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

            move || wasm_thread_main(plugin_bus, store, data_dir)
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
    // TODO: This will also be rearranged by the client-server model changes
    #[cfg(not(test))]
    let _ipc_thread = thread::Builder::new()
        .name("ipc_server".to_string())
        .spawn({
            use std::io::Read;
            let senders = ThreadSenders {
                to_pty: Some(to_pty.clone()),
                to_screen: Some(to_screen.clone()),
                to_app: None,
                to_plugin: None,
            };
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
                                    senders
                                        .send_to_pty(PtyInstruction::SpawnTerminal(Some(path)))
                                        .unwrap();
                                }
                                ApiCommand::SplitHorizontally => {
                                    senders
                                        .send_to_pty(PtyInstruction::SpawnTerminalHorizontally(
                                            None,
                                        ))
                                        .unwrap();
                                }
                                ApiCommand::SplitVertically => {
                                    senders
                                        .send_to_pty(PtyInstruction::SpawnTerminalVertically(None))
                                        .unwrap();
                                }
                                ApiCommand::MoveFocus => {
                                    senders
                                        .send_to_screen(ScreenInstruction::FocusNextPane)
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
            let senders = ThreadSenders {
                to_pty: Some(to_pty.clone()),
                to_screen: Some(to_screen.clone()),
                to_plugin: Some(to_plugin.clone()),
                to_app: Some(to_app),
            };
            let os_input = os_input.clone();
            let config = config;

            move || input_loop(os_input, config, command_is_executing, senders)
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

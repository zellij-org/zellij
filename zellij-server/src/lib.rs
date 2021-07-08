pub mod os_input_output;
pub mod panes;
pub mod tab;

mod pty;
mod route;
mod screen;
mod thread_bus;
mod ui;
mod wasm_vm;

use zellij_utils::zellij_tile;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use wasmer::Store;
use zellij_tile::data::{Event, ModeInfo, Palette, PluginCapabilities};

use crate::{
    os_input_output::ServerOsApi,
    pty::{pty_thread_main, Pty, PtyInstruction},
    screen::{screen_thread_main, ScreenInstruction},
    thread_bus::{Bus, ThreadSenders},
    wasm_vm::{wasm_thread_main, PluginInstruction},
};
use route::route_thread_main;
use zellij_utils::{
    channels::{self, ChannelWithContext, SenderWithContext},
    cli::CliArgs,
    errors::{ContextType, ErrorInstruction, ServerContext},
    input::{
        command::{RunCommand, TerminalAction},
        layout::Layout,
        options::Options,
    },
    ipc::{ClientAttributes, ClientToServerMsg, ExitReason, ServerToClientMsg},
    setup::get_default_data_dir,
};

/// Instructions related to server-side application
#[derive(Debug, Clone)]
pub(crate) enum ServerInstruction {
    NewClient(ClientAttributes, Box<CliArgs>, Box<Options>, Option<Layout>),
    Render(Option<String>),
    UnblockInputThread,
    ClientExit,
    Error(String),
    DetachSession,
    AttachClient(ClientAttributes, bool, Options),
}

impl From<ClientToServerMsg> for ServerInstruction {
    fn from(instruction: ClientToServerMsg) -> Self {
        match instruction {
            ClientToServerMsg::NewClient(attrs, opts, options, layout) => {
                ServerInstruction::NewClient(attrs, opts, options, layout)
            }
            ClientToServerMsg::AttachClient(attrs, force, options) => {
                ServerInstruction::AttachClient(attrs, force, options)
            }
            _ => unreachable!(),
        }
    }
}

impl From<&ServerInstruction> for ServerContext {
    fn from(server_instruction: &ServerInstruction) -> Self {
        match *server_instruction {
            ServerInstruction::NewClient(..) => ServerContext::NewClient,
            ServerInstruction::Render(_) => ServerContext::Render,
            ServerInstruction::UnblockInputThread => ServerContext::UnblockInputThread,
            ServerInstruction::ClientExit => ServerContext::ClientExit,
            ServerInstruction::Error(_) => ServerContext::Error,
            ServerInstruction::DetachSession => ServerContext::DetachSession,
            ServerInstruction::AttachClient(..) => ServerContext::AttachClient,
        }
    }
}

impl ErrorInstruction for ServerInstruction {
    fn error(err: String) -> Self {
        ServerInstruction::Error(err)
    }
}

pub(crate) struct SessionMetaData {
    pub senders: ThreadSenders,
    pub capabilities: PluginCapabilities,
    pub palette: Palette,
    pub default_shell: Option<TerminalAction>,
    screen_thread: Option<thread::JoinHandle<()>>,
    pty_thread: Option<thread::JoinHandle<()>>,
    wasm_thread: Option<thread::JoinHandle<()>>,
}

impl Drop for SessionMetaData {
    fn drop(&mut self) {
        let _ = self.senders.send_to_pty(PtyInstruction::Exit);
        let _ = self.senders.send_to_screen(ScreenInstruction::Exit);
        let _ = self.senders.send_to_plugin(PluginInstruction::Exit);
        let _ = self.screen_thread.take().unwrap().join();
        let _ = self.pty_thread.take().unwrap().join();
        let _ = self.wasm_thread.take().unwrap().join();
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum SessionState {
    Attached,
    Detached,
    Uninitialized,
}

pub fn start_server(os_input: Box<dyn ServerOsApi>, socket_path: PathBuf) {
    daemonize::Daemonize::new()
        .working_directory(std::env::current_dir().unwrap())
        .umask(0o077)
        // FIXME: My cherished `dbg!` was broken, so this is a hack to bring it back
        //.stderr(std::fs::File::create("dbg.log").unwrap())
        .start()
        .expect("could not daemonize the server process");

    std::env::set_var(&"ZELLIJ", "0");

    let (to_server, server_receiver): ChannelWithContext<ServerInstruction> = channels::bounded(50);
    let to_server = SenderWithContext::new(to_server);
    let session_data: Arc<RwLock<Option<SessionMetaData>>> = Arc::new(RwLock::new(None));
    let session_state = Arc::new(RwLock::new(SessionState::Uninitialized));

    std::panic::set_hook({
        use zellij_utils::errors::handle_panic;
        let to_server = to_server.clone();
        Box::new(move |info| {
            handle_panic(info, &to_server);
        })
    });

    let thread_handles = Arc::new(Mutex::new(Vec::new()));

    let _ = thread::Builder::new()
        .name("server_listener".to_string())
        .spawn({
            use zellij_utils::{
                interprocess::local_socket::LocalSocketListener, shared::set_permissions,
            };

            let os_input = os_input.clone();
            let session_data = session_data.clone();
            let session_state = session_state.clone();
            let to_server = to_server.clone();
            let socket_path = socket_path.clone();
            let thread_handles = thread_handles.clone();
            move || {
                drop(std::fs::remove_file(&socket_path));
                let listener = LocalSocketListener::bind(&*socket_path).unwrap();
                set_permissions(&socket_path).unwrap();
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            let mut os_input = os_input.clone();
                            os_input.update_receiver(stream);
                            let session_data = session_data.clone();
                            let session_state = session_state.clone();
                            let to_server = to_server.clone();
                            thread_handles.lock().unwrap().push(
                                thread::Builder::new()
                                    .name("server_router".to_string())
                                    .spawn({
                                        let session_data = session_data.clone();
                                        let os_input = os_input.clone();
                                        let to_server = to_server.clone();

                                        move || {
                                            route_thread_main(
                                                session_data,
                                                session_state,
                                                os_input,
                                                to_server,
                                            )
                                        }
                                    })
                                    .unwrap(),
                            );
                        }
                        Err(err) => {
                            panic!("err {:?}", err);
                        }
                    }
                }
            }
        });

    loop {
        let (instruction, mut err_ctx) = server_receiver.recv().unwrap();
        err_ctx.add_call(ContextType::IPCServer((&instruction).into()));
        match instruction {
            ServerInstruction::NewClient(client_attributes, opts, config_options, layout) => {
                let session = init_session(
                    os_input.clone(),
                    opts,
                    config_options.clone(),
                    to_server.clone(),
                    client_attributes,
                    session_state.clone(),
                    layout,
                );
                *session_data.write().unwrap() = Some(session);
                *session_state.write().unwrap() = SessionState::Attached;

                let default_shell = config_options.default_shell.map(|shell| {
                    TerminalAction::RunCommand(RunCommand {
                        command: shell,
                        ..Default::default()
                    })
                });

                session_data
                    .read()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_pty(PtyInstruction::NewTab(default_shell.clone()))
                    .unwrap();
            }
            ServerInstruction::AttachClient(attrs, _, options) => {
                *session_state.write().unwrap() = SessionState::Attached;
                let rlock = session_data.read().unwrap();
                let session_data = rlock.as_ref().unwrap();
                session_data
                    .senders
                    .send_to_screen(ScreenInstruction::TerminalResize(attrs.position_and_size))
                    .unwrap();
                let default_mode = options.default_mode.unwrap_or_default();
                let mode_info =
                    ModeInfo::new(default_mode, attrs.palette, session_data.capabilities);
                session_data
                    .senders
                    .send_to_screen(ScreenInstruction::ChangeMode(mode_info.clone()))
                    .unwrap();
                session_data
                    .senders
                    .send_to_plugin(PluginInstruction::Update(
                        None,
                        Event::ModeUpdate(mode_info),
                    ))
                    .unwrap();
            }
            ServerInstruction::UnblockInputThread => {
                if *session_state.read().unwrap() == SessionState::Attached {
                    os_input.send_to_client(ServerToClientMsg::UnblockInputThread);
                }
            }
            ServerInstruction::ClientExit => {
                *session_data.write().unwrap() = None;
                os_input.send_to_client(ServerToClientMsg::Exit(ExitReason::Normal));
                break;
            }
            ServerInstruction::DetachSession => {
                *session_state.write().unwrap() = SessionState::Detached;
                os_input.send_to_client(ServerToClientMsg::Exit(ExitReason::Normal));
                os_input.remove_client_sender();
            }
            ServerInstruction::Render(output) => {
                if *session_state.read().unwrap() == SessionState::Attached {
                    // Here output is of the type Option<String> sent by screen thread.
                    // If `Some(_)`- unwrap it and forward it to the client to render.
                    // If `None`- Send an exit instruction. This is the case when the user closes last Tab/Pane.
                    if let Some(op) = output {
                        os_input.send_to_client(ServerToClientMsg::Render(op));
                    } else {
                        os_input.send_to_client(ServerToClientMsg::Exit(ExitReason::Normal));
                        break;
                    }
                }
            }
            ServerInstruction::Error(backtrace) => {
                if *session_state.read().unwrap() == SessionState::Attached {
                    os_input.send_to_client(ServerToClientMsg::Exit(ExitReason::Error(backtrace)));
                }
                break;
            }
        }
    }
    thread_handles
        .lock()
        .unwrap()
        .drain(..)
        .for_each(|h| drop(h.join()));
    drop(std::fs::remove_file(&socket_path));
}

fn init_session(
    os_input: Box<dyn ServerOsApi>,
    opts: Box<CliArgs>,
    config_options: Box<Options>,
    to_server: SenderWithContext<ServerInstruction>,
    client_attributes: ClientAttributes,
    session_state: Arc<RwLock<SessionState>>,
    layout: Option<Layout>,
) -> SessionMetaData {
    let (to_screen, screen_receiver): ChannelWithContext<ScreenInstruction> = channels::unbounded();
    let to_screen = SenderWithContext::new(to_screen);

    let (to_screen_bounded, bounded_screen_receiver): ChannelWithContext<ScreenInstruction> =
        channels::bounded(50);
    let to_screen_bounded = SenderWithContext::new(to_screen_bounded);

    let (to_plugin, plugin_receiver): ChannelWithContext<PluginInstruction> = channels::unbounded();
    let to_plugin = SenderWithContext::new(to_plugin);
    let (to_pty, pty_receiver): ChannelWithContext<PtyInstruction> = channels::unbounded();
    let to_pty = SenderWithContext::new(to_pty);

    // Determine and initialize the data directory
    let data_dir = opts.data_dir.unwrap_or_else(get_default_data_dir);

    let capabilities = PluginCapabilities {
        arrow_fonts: config_options.simplified_ui,
    };

    let default_shell = config_options.default_shell.clone().map(|command| {
        TerminalAction::RunCommand(RunCommand {
            command,
            ..Default::default()
        })
    });

    let pty_thread = thread::Builder::new()
        .name("pty".to_string())
        .spawn({
            let pty = Pty::new(
                Bus::new(
                    vec![pty_receiver],
                    Some(&to_screen_bounded),
                    None,
                    Some(&to_plugin),
                    Some(&to_server),
                    Some(os_input.clone()),
                ),
                opts.debug,
            );

            move || pty_thread_main(pty, layout)
        })
        .unwrap();

    let screen_thread = thread::Builder::new()
        .name("screen".to_string())
        .spawn({
            let screen_bus = Bus::new(
                vec![screen_receiver, bounded_screen_receiver],
                None,
                Some(&to_pty),
                Some(&to_plugin),
                Some(&to_server),
                Some(os_input.clone()),
            );
            let max_panes = opts.max_panes;

            move || {
                screen_thread_main(
                    screen_bus,
                    max_panes,
                    client_attributes,
                    config_options,
                    session_state,
                );
            }
        })
        .unwrap();

    let wasm_thread = thread::Builder::new()
        .name("wasm".to_string())
        .spawn({
            let plugin_bus = Bus::new(
                vec![plugin_receiver],
                Some(&to_screen),
                Some(&to_pty),
                None,
                None,
                None,
            );
            let store = Store::default();

            move || wasm_thread_main(plugin_bus, store, data_dir)
        })
        .unwrap();
    SessionMetaData {
        senders: ThreadSenders {
            to_screen: Some(to_screen),
            to_pty: Some(to_pty),
            to_plugin: Some(to_plugin),
            to_server: None,
            should_silently_fail: false,
        },
        capabilities,
        default_shell,
        palette: client_attributes.palette,
        screen_thread: Some(screen_thread),
        pty_thread: Some(pty_thread),
        wasm_thread: Some(wasm_thread),
    }
}

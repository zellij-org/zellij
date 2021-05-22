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
use zellij_tile::data::{Event, InputMode, PluginCapabilities};

use crate::{
    os_input_output::ServerOsApi,
    pty::{pty_thread_main, Pty, PtyInstruction},
    screen::{screen_thread_main, ScreenInstruction},
    thread_bus::{Bus, ThreadSenders},
    ui::layout::Layout,
    wasm_vm::{wasm_thread_main, PluginInstruction},
};
use route::route_thread_main;
use zellij_utils::{
    channels,
    channels::{ChannelWithContext, SenderType, SenderWithContext},
    cli::CliArgs,
    errors::{ContextType, ErrorInstruction, ServerContext},
    input::{get_mode_info, options::Options},
    ipc::{ClientAttributes, ClientToServerMsg, ExitReason, ServerToClientMsg},
    setup::{get_default_data_dir, install::populate_data_dir},
};

/// Instructions related to server-side application
#[derive(Debug, Clone)]
pub(crate) enum ServerInstruction {
    NewClient(ClientAttributes, Box<CliArgs>, Box<Options>),
    Render(Option<String>),
    UnblockInputThread,
    ClientExit,
    Error(String),
    DetachSession,
    AttachClient(ClientAttributes, bool),
}

impl From<ClientToServerMsg> for ServerInstruction {
    fn from(instruction: ClientToServerMsg) -> Self {
        match instruction {
            ClientToServerMsg::NewClient(attrs, opts, options) => {
                ServerInstruction::NewClient(attrs, opts, options)
            }
            ClientToServerMsg::AttachClient(attrs, force) => {
                ServerInstruction::AttachClient(attrs, force)
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
    #[cfg(not(any(feature = "test", test)))]
    daemonize::Daemonize::new()
        .working_directory(std::env::current_dir().unwrap())
        .umask(0o077)
        .start()
        .expect("could not daemonize the server process");

    std::env::set_var(&"ZELLIJ", "0");

    let (to_server, server_receiver): ChannelWithContext<ServerInstruction> = channels::bounded(50);
    let to_server = SenderWithContext::new(SenderType::Sender(to_server));
    let session_data: Arc<RwLock<Option<SessionMetaData>>> = Arc::new(RwLock::new(None));
    let session_state = Arc::new(RwLock::new(SessionState::Uninitialized));

    #[cfg(not(any(feature = "test", test)))]
    std::panic::set_hook({
        use zellij_utils::errors::handle_panic;
        let to_server = to_server.clone();
        Box::new(move |info| {
            handle_panic(info, &to_server);
        })
    });

    let thread_handles = Arc::new(Mutex::new(Vec::new()));

    #[cfg(any(feature = "test", test))]
    thread_handles.lock().unwrap().push(
        thread::Builder::new()
            .name("server_router".to_string())
            .spawn({
                let session_data = session_data.clone();
                let os_input = os_input.clone();
                let to_server = to_server.clone();
                let session_state = session_state.clone();

                move || route_thread_main(session_data, session_state, os_input, to_server)
            })
            .unwrap(),
    );
    #[cfg(not(any(feature = "test", test)))]
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
            ServerInstruction::NewClient(client_attributes, opts, config_options) => {
                let session = init_session(
                    os_input.clone(),
                    opts,
                    config_options,
                    to_server.clone(),
                    client_attributes,
                    session_state.clone(),
                );
                *session_data.write().unwrap() = Some(session);
                *session_state.write().unwrap() = SessionState::Attached;
                session_data
                    .read()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_pty(PtyInstruction::NewTab)
                    .unwrap();
            }
            ServerInstruction::AttachClient(attrs, _) => {
                *session_state.write().unwrap() = SessionState::Attached;
                let rlock = session_data.read().unwrap();
                let session_data = rlock.as_ref().unwrap();
                session_data
                    .senders
                    .send_to_screen(ScreenInstruction::TerminalResize(attrs.position_and_size))
                    .unwrap();
                let mode_info =
                    get_mode_info(InputMode::Normal, attrs.palette, session_data.capabilities);
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
    #[cfg(not(any(feature = "test", test)))]
    drop(std::fs::remove_file(&socket_path));
}

fn init_session(
    os_input: Box<dyn ServerOsApi>,
    opts: Box<CliArgs>,
    config_options: Box<Options>,
    to_server: SenderWithContext<ServerInstruction>,
    client_attributes: ClientAttributes,
    session_state: Arc<RwLock<SessionState>>,
) -> SessionMetaData {
    let (to_screen, screen_receiver): ChannelWithContext<ScreenInstruction> = channels::unbounded();
    let to_screen = SenderWithContext::new(SenderType::Sender(to_screen));

    let (to_plugin, plugin_receiver): ChannelWithContext<PluginInstruction> = channels::unbounded();
    let to_plugin = SenderWithContext::new(SenderType::Sender(to_plugin));
    let (to_pty, pty_receiver): ChannelWithContext<PtyInstruction> = channels::unbounded();
    let to_pty = SenderWithContext::new(SenderType::Sender(to_pty));

    // Determine and initialize the data directory
    let data_dir = opts.data_dir.unwrap_or_else(get_default_data_dir);

    #[cfg(not(disable_automatic_asset_installation))]
    populate_data_dir(&data_dir);

    let capabilities = PluginCapabilities {
        arrow_fonts: config_options.simplified_ui,
    };

    // Don't use default layouts in tests, but do everywhere else
    #[cfg(not(any(feature = "test", test)))]
    let default_layout = Some(PathBuf::from("default"));
    #[cfg(any(feature = "test", test))]
    let default_layout = None;
    let layout_path = opts.layout_path;
    let maybe_layout = opts
        .layout
        .as_ref()
        .map(|p| Layout::from_dir(&p, &data_dir))
        .or_else(|| layout_path.map(|p| Layout::new(&p)))
        .or_else(|| default_layout.map(|p| Layout::from_dir(&p, &data_dir)));

    let pty_thread = thread::Builder::new()
        .name("pty".to_string())
        .spawn({
            let pty = Pty::new(
                Bus::new(
                    pty_receiver,
                    Some(&to_screen),
                    None,
                    Some(&to_plugin),
                    Some(&to_server),
                    Some(os_input.clone()),
                ),
                opts.debug,
            );

            move || pty_thread_main(pty, maybe_layout)
        })
        .unwrap();

    let screen_thread = thread::Builder::new()
        .name("screen".to_string())
        .spawn({
            let screen_bus = Bus::new(
                screen_receiver,
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
                plugin_receiver,
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
        },
        capabilities,
        screen_thread: Some(screen_thread),
        pty_thread: Some(pty_thread),
        wasm_thread: Some(wasm_thread),
    }
}

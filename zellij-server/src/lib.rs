pub mod os_input_output;
pub mod output;
pub mod panes;
pub mod tab;

mod background_jobs;
mod logging_pipe;
mod plugins;
mod pty;
mod pty_writer;
mod route;
mod screen;
mod session_layout_metadata;
mod terminal_bytes;
mod thread_bus;
mod ui;

use background_jobs::{background_jobs_main, BackgroundJob};
use log::info;
use pty_writer::{pty_writer_main, PtyWriteInstruction};
use std::collections::{HashMap, HashSet};
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
    thread,
};
use zellij_utils::envs;
use zellij_utils::nix::sys::stat::{umask, Mode};
use zellij_utils::pane_size::Size;

use wasmer::Store;

use crate::{
    os_input_output::ServerOsApi,
    plugins::{plugin_thread_main, PluginInstruction},
    pty::{get_default_shell, pty_thread_main, Pty, PtyInstruction},
    screen::{screen_thread_main, ScreenInstruction},
    thread_bus::{Bus, ThreadSenders},
};
use route::route_thread_main;
use zellij_utils::{
    channels::{self, ChannelWithContext, SenderWithContext},
    cli::CliArgs,
    consts::{DEFAULT_SCROLL_BUFFER_SIZE, SCROLL_BUFFER_SIZE},
    data::{ConnectToSession, Event, PluginCapabilities},
    errors::{prelude::*, ContextType, ErrorInstruction, FatalError, ServerContext},
    home::{default_layout_dir, get_default_data_dir},
    input::{
        command::{RunCommand, TerminalAction},
        get_mode_info,
        layout::Layout,
        options::Options,
        plugins::PluginAliases,
    },
    ipc::{ClientAttributes, ExitReason, ServerToClientMsg},
};

pub type ClientId = u16;

/// Instructions related to server-side application
#[derive(Debug, Clone)]
pub enum ServerInstruction {
    NewClient(
        ClientAttributes,
        Box<CliArgs>,
        Box<Options>,
        Box<Layout>,
        Box<PluginAliases>,
        ClientId,
    ),
    Render(Option<HashMap<ClientId, String>>),
    UnblockInputThread,
    ClientExit(ClientId),
    RemoveClient(ClientId),
    Error(String),
    KillSession,
    DetachSession(Vec<ClientId>),
    AttachClient(
        ClientAttributes,
        Options,
        Option<usize>,       // tab position to focus
        Option<(u32, bool)>, // (pane_id, is_plugin) => pane_id to focus
        ClientId,
    ),
    ConnStatus(ClientId),
    ActiveClients(ClientId),
    Log(Vec<String>, ClientId),
    LogError(Vec<String>, ClientId),
    SwitchSession(ConnectToSession, ClientId),
    UnblockCliPipeInput(String),   // String -> Pipe name
    CliPipeOutput(String, String), // String -> Pipe name, String -> Output
    AssociatePipeWithClient {
        pipe_id: String,
        client_id: ClientId,
    },
    DisconnectAllClientsExcept(ClientId),
}

impl From<&ServerInstruction> for ServerContext {
    fn from(server_instruction: &ServerInstruction) -> Self {
        match *server_instruction {
            ServerInstruction::NewClient(..) => ServerContext::NewClient,
            ServerInstruction::Render(..) => ServerContext::Render,
            ServerInstruction::UnblockInputThread => ServerContext::UnblockInputThread,
            ServerInstruction::ClientExit(..) => ServerContext::ClientExit,
            ServerInstruction::RemoveClient(..) => ServerContext::RemoveClient,
            ServerInstruction::Error(_) => ServerContext::Error,
            ServerInstruction::KillSession => ServerContext::KillSession,
            ServerInstruction::DetachSession(..) => ServerContext::DetachSession,
            ServerInstruction::AttachClient(..) => ServerContext::AttachClient,
            ServerInstruction::ConnStatus(..) => ServerContext::ConnStatus,
            ServerInstruction::ActiveClients(_) => ServerContext::ActiveClients,
            ServerInstruction::Log(..) => ServerContext::Log,
            ServerInstruction::LogError(..) => ServerContext::LogError,
            ServerInstruction::SwitchSession(..) => ServerContext::SwitchSession,
            ServerInstruction::UnblockCliPipeInput(..) => ServerContext::UnblockCliPipeInput,
            ServerInstruction::CliPipeOutput(..) => ServerContext::CliPipeOutput,
            ServerInstruction::AssociatePipeWithClient { .. } => {
                ServerContext::AssociatePipeWithClient
            },
            ServerInstruction::DisconnectAllClientsExcept(..) => {
                ServerContext::DisconnectAllClientsExcept
            },
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
    pub client_attributes: ClientAttributes,
    pub default_shell: Option<TerminalAction>,
    pub layout: Box<Layout>,
    pub config_options: Box<Options>,
    screen_thread: Option<thread::JoinHandle<()>>,
    pty_thread: Option<thread::JoinHandle<()>>,
    plugin_thread: Option<thread::JoinHandle<()>>,
    pty_writer_thread: Option<thread::JoinHandle<()>>,
    background_jobs_thread: Option<thread::JoinHandle<()>>,
}

impl Drop for SessionMetaData {
    fn drop(&mut self) {
        let _ = self.senders.send_to_pty(PtyInstruction::Exit);
        let _ = self.senders.send_to_screen(ScreenInstruction::Exit);
        let _ = self.senders.send_to_plugin(PluginInstruction::Exit);
        let _ = self.senders.send_to_pty_writer(PtyWriteInstruction::Exit);
        let _ = self.senders.send_to_background_jobs(BackgroundJob::Exit);
        if let Some(screen_thread) = self.screen_thread.take() {
            let _ = screen_thread.join();
        }
        if let Some(pty_thread) = self.pty_thread.take() {
            let _ = pty_thread.join();
        }
        if let Some(plugin_thread) = self.plugin_thread.take() {
            let _ = plugin_thread.join();
        }
        if let Some(pty_writer_thread) = self.pty_writer_thread.take() {
            let _ = pty_writer_thread.join();
        }
        if let Some(background_jobs_thread) = self.background_jobs_thread.take() {
            let _ = background_jobs_thread.join();
        }
    }
}

macro_rules! remove_client {
    ($client_id:expr, $os_input:expr, $session_state:expr) => {
        $os_input.remove_client($client_id).unwrap();
        $session_state.write().unwrap().remove_client($client_id);
    };
}

macro_rules! send_to_client {
    ($client_id:expr, $os_input:expr, $msg:expr, $session_state:expr) => {
        let send_to_client_res = $os_input.send_to_client($client_id, $msg);
        if let Err(e) = send_to_client_res {
            // Try to recover the message
            let context = match e.downcast_ref::<ZellijError>() {
                Some(ZellijError::ClientTooSlow { .. }) => {
                    format!(
                        "client {} is processing server messages too slow",
                        $client_id
                    )
                },
                _ => {
                    format!("failed to route server message to client {}", $client_id)
                },
            };
            // Log it so it isn't lost
            Err::<(), _>(e).context(context).non_fatal();
            // failed to send to client, remove it
            remove_client!($client_id, $os_input, $session_state);
        }
    };
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SessionState {
    clients: HashMap<ClientId, Option<Size>>,
    pipes: HashMap<String, ClientId>, // String => pipe_id
}

impl SessionState {
    pub fn new() -> Self {
        SessionState {
            clients: HashMap::new(),
            pipes: HashMap::new(),
        }
    }
    pub fn new_client(&mut self) -> ClientId {
        let clients: HashSet<ClientId> = self.clients.keys().copied().collect();
        let mut next_client_id = 1;
        loop {
            if clients.contains(&next_client_id) {
                next_client_id += 1;
            } else {
                break;
            }
        }
        self.clients.insert(next_client_id, None);
        next_client_id
    }
    pub fn associate_pipe_with_client(&mut self, pipe_id: String, client_id: ClientId) {
        self.pipes.insert(pipe_id, client_id);
    }
    pub fn remove_client(&mut self, client_id: ClientId) {
        self.clients.remove(&client_id);
        self.pipes.retain(|_p_id, c_id| c_id != &client_id);
    }
    pub fn set_client_size(&mut self, client_id: ClientId, size: Size) {
        self.clients.insert(client_id, Some(size));
    }
    pub fn min_client_terminal_size(&self) -> Option<Size> {
        // None if there are no client sizes
        let mut rows: Vec<usize> = self
            .clients
            .values()
            .filter_map(|size| size.map(|size| size.rows))
            .collect();
        rows.sort_unstable();
        let mut cols: Vec<usize> = self
            .clients
            .values()
            .filter_map(|size| size.map(|size| size.cols))
            .collect();
        cols.sort_unstable();
        let min_rows = rows.first();
        let min_cols = cols.first();
        match (min_rows, min_cols) {
            (Some(min_rows), Some(min_cols)) => Some(Size {
                rows: *min_rows,
                cols: *min_cols,
            }),
            _ => None,
        }
    }
    pub fn client_ids(&self) -> Vec<ClientId> {
        self.clients.keys().copied().collect()
    }
    pub fn get_pipe(&self, pipe_name: &str) -> Option<ClientId> {
        self.pipes.get(pipe_name).copied()
    }
    pub fn active_clients_are_connected(&self) -> bool {
        let ids_of_pipe_clients: HashSet<ClientId> = self.pipes.values().copied().collect();
        let mut active_clients_connected = false;
        for client_id in self.clients.keys() {
            if ids_of_pipe_clients.contains(client_id) {
                continue;
            }
            active_clients_connected = true;
        }
        active_clients_connected
    }
}

pub fn start_server(mut os_input: Box<dyn ServerOsApi>, socket_path: PathBuf) {
    info!("Starting Zellij server!");

    // preserve the current umask: read current value by setting to another mode, and then restoring it
    let current_umask = umask(Mode::all());
    umask(current_umask);
    daemonize::Daemonize::new()
        .working_directory(std::env::current_dir().unwrap())
        .umask(current_umask.bits())
        .start()
        .expect("could not daemonize the server process");

    envs::set_zellij("0".to_string());

    let (to_server, server_receiver): ChannelWithContext<ServerInstruction> = channels::bounded(50);
    let to_server = SenderWithContext::new(to_server);
    let session_data: Arc<RwLock<Option<SessionMetaData>>> = Arc::new(RwLock::new(None));
    let session_state = Arc::new(RwLock::new(SessionState::new()));

    std::panic::set_hook({
        use zellij_utils::errors::handle_panic;
        let to_server = to_server.clone();
        Box::new(move |info| {
            handle_panic(info, &to_server);
        })
    });

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
            move || {
                drop(std::fs::remove_file(&socket_path));
                let listener = LocalSocketListener::bind(&*socket_path).unwrap();
                // set the sticky bit to avoid the socket file being potentially cleaned up
                // https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html states that for XDG_RUNTIME_DIR:
                // "To ensure that your files are not removed, they should have their access time timestamp modified at least once every 6 hours of monotonic time or the 'sticky' bit should be set on the file. "
                // It is not guaranteed that all platforms allow setting the sticky bit on sockets!
                drop(set_permissions(&socket_path, 0o1700));
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            let mut os_input = os_input.clone();
                            let client_id = session_state.write().unwrap().new_client();
                            let receiver = os_input.new_client(client_id, stream).unwrap();
                            let session_data = session_data.clone();
                            let session_state = session_state.clone();
                            let to_server = to_server.clone();
                            thread::Builder::new()
                                .name("server_router".to_string())
                                .spawn(move || {
                                    route_thread_main(
                                        session_data,
                                        session_state,
                                        os_input,
                                        to_server,
                                        receiver,
                                        client_id,
                                    )
                                    .fatal()
                                })
                                .unwrap();
                        },
                        Err(err) => {
                            panic!("err {:?}", err);
                        },
                    }
                }
            }
        });

    loop {
        let (instruction, mut err_ctx) = server_receiver.recv().unwrap();
        err_ctx.add_call(ContextType::IPCServer((&instruction).into()));
        match instruction {
            ServerInstruction::NewClient(
                client_attributes,
                opts,
                config_options,
                layout,
                plugin_aliases,
                client_id,
            ) => {
                let session = init_session(
                    os_input.clone(),
                    to_server.clone(),
                    client_attributes.clone(),
                    SessionOptions {
                        opts,
                        layout: layout.clone(),
                        config_options: config_options.clone(),
                    },
                    plugin_aliases,
                );
                *session_data.write().unwrap() = Some(session);
                session_state
                    .write()
                    .unwrap()
                    .set_client_size(client_id, client_attributes.size);

                let default_shell = config_options.default_shell.map(|shell| {
                    TerminalAction::RunCommand(RunCommand {
                        command: shell,
                        cwd: config_options.default_cwd.clone(),
                        ..Default::default()
                    })
                });
                let cwd = config_options.default_cwd;

                let spawn_tabs = |tab_layout, floating_panes_layout, tab_name, swap_layouts| {
                    session_data
                        .read()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .senders
                        .send_to_screen(ScreenInstruction::NewTab(
                            cwd.clone(),
                            default_shell.clone(),
                            tab_layout,
                            floating_panes_layout,
                            tab_name,
                            swap_layouts,
                            client_id,
                        ))
                        .unwrap()
                };

                if layout.has_tabs() {
                    for (tab_name, tab_layout, floating_panes_layout) in layout.tabs() {
                        spawn_tabs(
                            Some(tab_layout.clone()),
                            floating_panes_layout.clone(),
                            tab_name,
                            (
                                layout.swap_tiled_layouts.clone(),
                                layout.swap_floating_layouts.clone(),
                            ),
                        );
                    }

                    if let Some(focused_tab_index) = layout.focused_tab_index() {
                        session_data
                            .read()
                            .unwrap()
                            .as_ref()
                            .unwrap()
                            .senders
                            .send_to_screen(ScreenInstruction::GoToTab(
                                (focused_tab_index + 1) as u32,
                                Some(client_id),
                            ))
                            .unwrap();
                    }
                } else {
                    spawn_tabs(
                        None,
                        layout.template.map(|t| t.1).clone().unwrap_or_default(),
                        None,
                        (
                            layout.swap_tiled_layouts.clone(),
                            layout.swap_floating_layouts.clone(),
                        ),
                    );
                }
                session_data
                    .read()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_plugin(PluginInstruction::AddClient(client_id))
                    .unwrap();
            },
            ServerInstruction::AttachClient(
                attrs,
                options,
                tab_position_to_focus,
                pane_id_to_focus,
                client_id,
            ) => {
                let rlock = session_data.read().unwrap();
                let session_data = rlock.as_ref().unwrap();
                session_state
                    .write()
                    .unwrap()
                    .set_client_size(client_id, attrs.size);
                let min_size = session_state
                    .read()
                    .unwrap()
                    .min_client_terminal_size()
                    .unwrap();
                session_data
                    .senders
                    .send_to_screen(ScreenInstruction::TerminalResize(min_size))
                    .unwrap();
                session_data
                    .senders
                    .send_to_screen(ScreenInstruction::AddClient(
                        client_id,
                        tab_position_to_focus,
                        pane_id_to_focus,
                    ))
                    .unwrap();
                session_data
                    .senders
                    .send_to_plugin(PluginInstruction::AddClient(client_id))
                    .unwrap();
                let default_mode = options.default_mode.unwrap_or_default();
                let mode_info = get_mode_info(default_mode, &attrs, session_data.capabilities);
                let mode = mode_info.mode;
                session_data
                    .senders
                    .send_to_screen(ScreenInstruction::ChangeMode(mode_info.clone(), client_id))
                    .unwrap();
                session_data
                    .senders
                    .send_to_plugin(PluginInstruction::Update(vec![(
                        None,
                        Some(client_id),
                        Event::ModeUpdate(mode_info),
                    )]))
                    .unwrap();
                send_to_client!(
                    client_id,
                    os_input,
                    ServerToClientMsg::SwitchToMode(mode),
                    session_state
                );
            },
            ServerInstruction::UnblockInputThread => {
                let client_ids = session_state.read().unwrap().client_ids();
                for client_id in client_ids {
                    send_to_client!(
                        client_id,
                        os_input,
                        ServerToClientMsg::UnblockInputThread,
                        session_state
                    );
                }
            },
            ServerInstruction::UnblockCliPipeInput(pipe_name) => {
                let pipe = session_state.read().unwrap().get_pipe(&pipe_name);
                match pipe {
                    Some(client_id) => {
                        send_to_client!(
                            client_id,
                            os_input,
                            ServerToClientMsg::UnblockCliPipeInput(pipe_name.clone()),
                            session_state
                        );
                    },
                    None => {
                        // send to all clients, this pipe might not have been associated yet
                        let client_ids = session_state.read().unwrap().client_ids();
                        for client_id in client_ids {
                            send_to_client!(
                                client_id,
                                os_input,
                                ServerToClientMsg::UnblockCliPipeInput(pipe_name.clone()),
                                session_state
                            );
                        }
                    },
                }
            },
            ServerInstruction::CliPipeOutput(pipe_name, output) => {
                let pipe = session_state.read().unwrap().get_pipe(&pipe_name);
                match pipe {
                    Some(client_id) => {
                        send_to_client!(
                            client_id,
                            os_input,
                            ServerToClientMsg::CliPipeOutput(pipe_name.clone(), output.clone()),
                            session_state
                        );
                    },
                    None => {
                        // send to all clients, this pipe might not have been associated yet
                        let client_ids = session_state.read().unwrap().client_ids();
                        for client_id in client_ids {
                            send_to_client!(
                                client_id,
                                os_input,
                                ServerToClientMsg::CliPipeOutput(pipe_name.clone(), output.clone()),
                                session_state
                            );
                        }
                    },
                }
            },
            ServerInstruction::ClientExit(client_id) => {
                let _ =
                    os_input.send_to_client(client_id, ServerToClientMsg::Exit(ExitReason::Normal));
                remove_client!(client_id, os_input, session_state);
                if let Some(min_size) = session_state.read().unwrap().min_client_terminal_size() {
                    session_data
                        .write()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .senders
                        .send_to_screen(ScreenInstruction::TerminalResize(min_size))
                        .unwrap();
                }
                session_data
                    .write()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_screen(ScreenInstruction::RemoveClient(client_id))
                    .unwrap();
                session_data
                    .write()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_plugin(PluginInstruction::RemoveClient(client_id))
                    .unwrap();
                if !session_state.read().unwrap().active_clients_are_connected() {
                    *session_data.write().unwrap() = None;
                    let client_ids_to_cleanup: Vec<ClientId> = session_state
                        .read()
                        .unwrap()
                        .clients
                        .keys()
                        .copied()
                        .collect();
                    // these are just the pipes
                    for client_id in client_ids_to_cleanup {
                        remove_client!(client_id, os_input, session_state);
                    }
                    break;
                }
            },
            ServerInstruction::RemoveClient(client_id) => {
                remove_client!(client_id, os_input, session_state);
                if let Some(min_size) = session_state.read().unwrap().min_client_terminal_size() {
                    session_data
                        .write()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .senders
                        .send_to_screen(ScreenInstruction::TerminalResize(min_size))
                        .unwrap();
                }
                session_data
                    .write()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_screen(ScreenInstruction::RemoveClient(client_id))
                    .unwrap();
                session_data
                    .write()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_plugin(PluginInstruction::RemoveClient(client_id))
                    .unwrap();
            },
            ServerInstruction::KillSession => {
                let client_ids = session_state.read().unwrap().client_ids();
                for client_id in client_ids {
                    let _ = os_input
                        .send_to_client(client_id, ServerToClientMsg::Exit(ExitReason::Normal));
                    remove_client!(client_id, os_input, session_state);
                }
                break;
            },
            ServerInstruction::DisconnectAllClientsExcept(client_id) => {
                let client_ids: Vec<ClientId> = session_state
                    .read()
                    .unwrap()
                    .client_ids()
                    .iter()
                    .copied()
                    .filter(|c| c != &client_id)
                    .collect();
                for client_id in client_ids {
                    let _ = os_input
                        .send_to_client(client_id, ServerToClientMsg::Exit(ExitReason::Normal));
                    remove_client!(client_id, os_input, session_state);
                }
            },
            ServerInstruction::DetachSession(client_ids) => {
                for client_id in client_ids {
                    let _ = os_input
                        .send_to_client(client_id, ServerToClientMsg::Exit(ExitReason::Normal));
                    remove_client!(client_id, os_input, session_state);
                    if let Some(min_size) = session_state.read().unwrap().min_client_terminal_size()
                    {
                        session_data
                            .write()
                            .unwrap()
                            .as_ref()
                            .unwrap()
                            .senders
                            .send_to_screen(ScreenInstruction::TerminalResize(min_size))
                            .unwrap();
                    }
                    session_data
                        .write()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .senders
                        .send_to_screen(ScreenInstruction::RemoveClient(client_id))
                        .unwrap();
                    session_data
                        .write()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .senders
                        .send_to_plugin(PluginInstruction::RemoveClient(client_id))
                        .unwrap();
                }
            },
            ServerInstruction::Render(serialized_output) => {
                let client_ids = session_state.read().unwrap().client_ids();
                // If `Some(_)`- unwrap it and forward it to the clients to render.
                // If `None`- Send an exit instruction. This is the case when a user closes the last Tab/Pane.
                if let Some(output) = &serialized_output {
                    for (client_id, client_render_instruction) in output.iter() {
                        // TODO: When a client is too slow or unresponsive, the channel fills up
                        // and this call will disconnect the client in turn. Should this be
                        // changed?
                        send_to_client!(
                            *client_id,
                            os_input,
                            ServerToClientMsg::Render(client_render_instruction.clone()),
                            session_state
                        );
                    }
                } else {
                    for client_id in client_ids {
                        let _ = os_input
                            .send_to_client(client_id, ServerToClientMsg::Exit(ExitReason::Normal));
                        remove_client!(client_id, os_input, session_state);
                    }
                    break;
                }
            },
            ServerInstruction::Error(backtrace) => {
                let client_ids = session_state.read().unwrap().client_ids();
                for client_id in client_ids {
                    let _ = os_input.send_to_client(
                        client_id,
                        ServerToClientMsg::Exit(ExitReason::Error(backtrace.clone())),
                    );
                    remove_client!(client_id, os_input, session_state);
                }
                break;
            },
            ServerInstruction::ConnStatus(client_id) => {
                let _ = os_input.send_to_client(client_id, ServerToClientMsg::Connected);
                remove_client!(client_id, os_input, session_state);
            },
            ServerInstruction::ActiveClients(client_id) => {
                let client_ids = session_state.read().unwrap().client_ids();
                send_to_client!(
                    client_id,
                    os_input,
                    ServerToClientMsg::ActiveClients(client_ids),
                    session_state
                );
            },
            ServerInstruction::Log(lines_to_log, client_id) => {
                send_to_client!(
                    client_id,
                    os_input,
                    ServerToClientMsg::Log(lines_to_log),
                    session_state
                );
            },
            ServerInstruction::LogError(lines_to_log, client_id) => {
                send_to_client!(
                    client_id,
                    os_input,
                    ServerToClientMsg::LogError(lines_to_log),
                    session_state
                );
            },
            ServerInstruction::SwitchSession(mut connect_to_session, client_id) => {
                let layout_dir = session_data
                    .read()
                    .unwrap()
                    .as_ref()
                    .and_then(|c| c.config_options.layout_dir.clone())
                    .or_else(|| default_layout_dir());
                if let Some(layout_dir) = layout_dir {
                    connect_to_session.apply_layout_dir(&layout_dir);
                }
                if let Some(min_size) = session_state.read().unwrap().min_client_terminal_size() {
                    session_data
                        .write()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .senders
                        .send_to_screen(ScreenInstruction::TerminalResize(min_size))
                        .unwrap();
                }
                session_data
                    .write()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_screen(ScreenInstruction::RemoveClient(client_id))
                    .unwrap();
                session_data
                    .write()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_plugin(PluginInstruction::RemoveClient(client_id))
                    .unwrap();
                send_to_client!(
                    client_id,
                    os_input,
                    ServerToClientMsg::SwitchSession(connect_to_session),
                    session_state
                );
                remove_client!(client_id, os_input, session_state);
            },
            ServerInstruction::AssociatePipeWithClient { pipe_id, client_id } => {
                session_state
                    .write()
                    .unwrap()
                    .associate_pipe_with_client(pipe_id, client_id);
            },
        }
    }

    // Drop cached session data before exit.
    *session_data.write().unwrap() = None;

    drop(std::fs::remove_file(&socket_path));
}

pub struct SessionOptions {
    pub opts: Box<CliArgs>,
    pub config_options: Box<Options>,
    pub layout: Box<Layout>,
}

fn init_session(
    os_input: Box<dyn ServerOsApi>,
    to_server: SenderWithContext<ServerInstruction>,
    client_attributes: ClientAttributes,
    options: SessionOptions,
    plugin_aliases: Box<PluginAliases>,
) -> SessionMetaData {
    let SessionOptions {
        opts,
        config_options,
        layout,
    } = options;

    let _ = SCROLL_BUFFER_SIZE.set(
        config_options
            .scroll_buffer_size
            .unwrap_or(DEFAULT_SCROLL_BUFFER_SIZE),
    );

    let (to_screen, screen_receiver): ChannelWithContext<ScreenInstruction> = channels::unbounded();
    let to_screen = SenderWithContext::new(to_screen);

    let (to_screen_bounded, bounded_screen_receiver): ChannelWithContext<ScreenInstruction> =
        channels::bounded(50);
    let to_screen_bounded = SenderWithContext::new(to_screen_bounded);

    let (to_plugin, plugin_receiver): ChannelWithContext<PluginInstruction> = channels::unbounded();
    let to_plugin = SenderWithContext::new(to_plugin);
    let (to_pty, pty_receiver): ChannelWithContext<PtyInstruction> = channels::unbounded();
    let to_pty = SenderWithContext::new(to_pty);

    let (to_pty_writer, pty_writer_receiver): ChannelWithContext<PtyWriteInstruction> =
        channels::unbounded();
    let to_pty_writer = SenderWithContext::new(to_pty_writer);

    let (to_background_jobs, background_jobs_receiver): ChannelWithContext<BackgroundJob> =
        channels::unbounded();
    let to_background_jobs = SenderWithContext::new(to_background_jobs);

    // Determine and initialize the data directory
    let data_dir = opts.data_dir.unwrap_or_else(get_default_data_dir);

    let capabilities = PluginCapabilities {
        arrow_fonts: config_options.simplified_ui.unwrap_or_default(),
    };

    let serialization_interval = config_options.serialization_interval;
    let disable_session_metadata = config_options.disable_session_metadata.unwrap_or(false);

    let default_shell = config_options.default_shell.clone().map(|command| {
        TerminalAction::RunCommand(RunCommand {
            command,
            ..Default::default()
        })
    });
    let path_to_default_shell = config_options
        .default_shell
        .clone()
        .unwrap_or_else(|| get_default_shell());

    let pty_thread = thread::Builder::new()
        .name("pty".to_string())
        .spawn({
            let layout = layout.clone();
            let pty = Pty::new(
                Bus::new(
                    vec![pty_receiver],
                    Some(&to_screen_bounded),
                    None,
                    Some(&to_plugin),
                    Some(&to_server),
                    Some(&to_pty_writer),
                    Some(&to_background_jobs),
                    Some(os_input.clone()),
                ),
                opts.debug,
                config_options.scrollback_editor.clone(),
            );

            move || pty_thread_main(pty, layout.clone()).fatal()
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
                Some(&to_pty_writer),
                Some(&to_background_jobs),
                Some(os_input.clone()),
            );
            let max_panes = opts.max_panes;

            let client_attributes_clone = client_attributes.clone();
            let debug = opts.debug;
            let layout = layout.clone();
            let config_options = config_options.clone();
            move || {
                screen_thread_main(
                    screen_bus,
                    max_panes,
                    client_attributes_clone,
                    config_options,
                    debug,
                    layout,
                )
                .fatal();
            }
        })
        .unwrap();

    let zellij_cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let plugin_thread = thread::Builder::new()
        .name("wasm".to_string())
        .spawn({
            let plugin_bus = Bus::new(
                vec![plugin_receiver],
                Some(&to_screen_bounded),
                Some(&to_pty),
                Some(&to_plugin),
                Some(&to_server),
                Some(&to_pty_writer),
                Some(&to_background_jobs),
                None,
            );
            let store = get_store();

            let layout = layout.clone();
            let client_attributes = client_attributes.clone();
            let default_shell = default_shell.clone();
            let capabilities = capabilities.clone();
            move || {
                plugin_thread_main(
                    plugin_bus,
                    store,
                    data_dir,
                    layout,
                    path_to_default_shell,
                    zellij_cwd,
                    capabilities,
                    client_attributes,
                    default_shell,
                    plugin_aliases,
                )
                .fatal()
            }
        })
        .unwrap();

    let pty_writer_thread = thread::Builder::new()
        .name("pty_writer".to_string())
        .spawn({
            let pty_writer_bus = Bus::new(
                vec![pty_writer_receiver],
                Some(&to_screen),
                Some(&to_pty),
                Some(&to_plugin),
                Some(&to_server),
                None,
                Some(&to_background_jobs),
                Some(os_input.clone()),
            );
            || pty_writer_main(pty_writer_bus).fatal()
        })
        .unwrap();

    let background_jobs_thread = thread::Builder::new()
        .name("background_jobs".to_string())
        .spawn({
            let background_jobs_bus = Bus::new(
                vec![background_jobs_receiver],
                Some(&to_screen),
                Some(&to_pty),
                Some(&to_plugin),
                Some(&to_server),
                Some(&to_pty_writer),
                None,
                Some(os_input.clone()),
            );
            move || {
                background_jobs_main(
                    background_jobs_bus,
                    serialization_interval,
                    disable_session_metadata,
                )
                .fatal()
            }
        })
        .unwrap();

    SessionMetaData {
        senders: ThreadSenders {
            to_screen: Some(to_screen),
            to_pty: Some(to_pty),
            to_plugin: Some(to_plugin),
            to_pty_writer: Some(to_pty_writer),
            to_background_jobs: Some(to_background_jobs),
            to_server: Some(to_server),
            should_silently_fail: false,
        },
        capabilities,
        default_shell,
        client_attributes,
        layout,
        config_options: config_options.clone(),
        screen_thread: Some(screen_thread),
        pty_thread: Some(pty_thread),
        plugin_thread: Some(plugin_thread),
        pty_writer_thread: Some(pty_writer_thread),
        background_jobs_thread: Some(background_jobs_thread),
    }
}

#[cfg(not(feature = "singlepass"))]
fn get_store() -> Store {
    use wasmer::{BaseTunables, Cranelift, Engine, Pages, Target};
    log::info!("Compiling plugins using Cranelift");

    // workaround for https://github.com/bytecodealliance/wasmtime/security/advisories/GHSA-ff4p-7xrq-q5r8
    let mut tunables = BaseTunables::for_target(&Target::default());
    tunables.static_memory_bound = Pages(0);
    let compiler = Cranelift::default();
    let mut engine: Engine = compiler.into();
    engine.set_tunables(tunables);

    Store::new(engine)
}

#[cfg(feature = "singlepass")]
fn get_store() -> Store {
    log::info!("Compiling plugins using Singlepass");
    Store::new(wasmer::Singlepass::default())
}

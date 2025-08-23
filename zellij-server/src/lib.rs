pub mod os_input_output;
pub mod output;
pub mod panes;
pub mod tab;

mod background_jobs;
mod logging_pipe;
mod pane_groups;
mod plugins;
mod pty;
mod pty_writer;
mod route;
mod screen;
mod session_layout_metadata;
mod terminal_bytes;
mod thread_bus;
mod ui;

pub use daemonize;

use background_jobs::{background_jobs_main, BackgroundJob};
use log::info;
use nix::sys::stat::{umask, Mode};
use pty_writer::{pty_writer_main, PtyWriteInstruction};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    sync::{Arc, RwLock},
    thread,
};
use zellij_utils::envs;
use zellij_utils::pane_size::Size;

use zellij_utils::setup::{find_default_config_dir, get_layout_dir, Setup};
use zellij_utils::input::cli_assets::CliAssets;

use wasmtime::{Config as WasmtimeConfig, Engine, Strategy};

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
    consts::{
        DEFAULT_SCROLL_BUFFER_SIZE, SCROLL_BUFFER_SIZE, ZELLIJ_SEEN_RELEASE_NOTES_CACHE_FILE,
    },
    data::{ConnectToSession, Event, InputMode, KeyWithModifier, PluginCapabilities, WebSharing, Style},
    errors::{prelude::*, ContextType, ErrorInstruction, FatalError, ServerContext},
    home::{default_layout_dir, get_default_data_dir},
    input::{
        actions::Action,
        command::{RunCommand, TerminalAction},
        config::Config,
        get_mode_info,
        keybinds::Keybinds,
        layout::{FloatingPaneLayout, Layout, PluginAlias, Run, RunPluginOrAlias},
        options::Options,
        plugins::PluginAliases,
    },
    ipc::{ClientAttributes, ExitReason, ServerToClientMsg},
    shared::{web_server_base_url, default_palette},
};

pub type ClientId = u16;

/// Instructions related to server-side application
#[derive(Debug, Clone)]
pub enum ServerInstruction {
    NewClient(
        CliAssets,
        Box<CliArgs>,
        Box<Layout>,
        Box<PluginAliases>,
        bool, // should launch setup wizard
        bool, // is_web_client
        bool, // layout_is_welcome_screen
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
        CliAssets,
        Option<usize>,       // tab position to focus
        Option<(u32, bool)>, // (pane_id, is_plugin) => pane_id to focus
        bool,                // is_web_client
        ClientId,
    ),
    ConnStatus(ClientId),
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
    ChangeMode(ClientId, InputMode),
    ChangeModeForAllClients(InputMode),
    Reconfigure {
        client_id: ClientId,
        config: String,
        write_config_to_disk: bool,
    },
    // ConfigWrittenToDisk(ClientId, Config),
    FailedToWriteConfigToDisk(ClientId, Option<PathBuf>), // Pathbuf - file we failed to write
    RebindKeys {
        client_id: ClientId,
        keys_to_rebind: Vec<(InputMode, KeyWithModifier, Vec<Action>)>,
        keys_to_unbind: Vec<(InputMode, KeyWithModifier)>,
        write_config_to_disk: bool,
    },
    StartWebServer(ClientId),
    ShareCurrentSession(ClientId),
    StopSharingCurrentSession(ClientId),
    SendWebClientsForbidden(ClientId),
    WebServerStarted(String), // String -> base_url
    FailedToStartWebServer(String),
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
            ServerInstruction::ChangeMode(..) => ServerContext::ChangeMode,
            ServerInstruction::ChangeModeForAllClients(..) => {
                ServerContext::ChangeModeForAllClients
            },
            ServerInstruction::Reconfigure { .. } => ServerContext::Reconfigure,
            // ServerInstruction::ConfigWrittenToDisk(..) => ServerContext::ConfigWrittenToDisk,
            ServerInstruction::FailedToWriteConfigToDisk(..) => {
                ServerContext::FailedToWriteConfigToDisk
            },
            ServerInstruction::RebindKeys { .. } => ServerContext::RebindKeys,
            ServerInstruction::StartWebServer(..) => ServerContext::StartWebServer,
            ServerInstruction::ShareCurrentSession(..) => ServerContext::ShareCurrentSession,
            ServerInstruction::StopSharingCurrentSession(..) => {
                ServerContext::StopSharingCurrentSession
            },
            ServerInstruction::WebServerStarted(..) => ServerContext::WebServerStarted,
            ServerInstruction::FailedToStartWebServer(..) => ServerContext::FailedToStartWebServer,
            ServerInstruction::SendWebClientsForbidden(..) => {
                ServerContext::SendWebClientsForbidden
            },
        }
    }
}

impl ErrorInstruction for ServerInstruction {
    fn error(err: String) -> Self {
        ServerInstruction::Error(err)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionConfiguration {
    runtime_config: HashMap<ClientId, Config>, // if present, overrides the saved_config
    saved_config: HashMap<ClientId, Config>,   // the config as it is on disk (not guaranteed),
                                               // when changed, this resets the runtime config to
                                               // be identical to it and override any previous
                                               // changes
}

impl SessionConfiguration {
    pub fn new_saved_config(
        &mut self,
        client_id: ClientId,
        new_saved_config: Config,
    ) -> Vec<(ClientId, Config)> {
        self.saved_config
            .insert(client_id, new_saved_config.clone());

        let mut config_changes = vec![];
        for (client_id, current_runtime_config) in self.runtime_config.iter_mut() {
            if *current_runtime_config != new_saved_config {
                *current_runtime_config = new_saved_config.clone();
                config_changes.push((*client_id, new_saved_config.clone()))
            }
        }
        config_changes
    }
    pub fn set_client_saved_configuration(&mut self, client_id: ClientId, client_config: Config) {
        self.saved_config.insert(client_id, client_config);
    }
    pub fn set_client_runtime_configuration(&mut self, client_id: ClientId, client_config: Config) {
        self.runtime_config.insert(client_id, client_config);
    }
    pub fn get_client_keybinds(&self, client_id: &ClientId) -> Keybinds {
        self.runtime_config
            .get(client_id)
            .or_else(|| self.saved_config.get(client_id))
            .map(|c| c.keybinds.clone())
            .unwrap_or_default()
    }
    pub fn get_client_default_input_mode(&self, client_id: &ClientId) -> InputMode {
        self.runtime_config
            .get(client_id)
            .or_else(|| self.saved_config.get(client_id))
            .and_then(|c| c.options.default_mode.clone())
            .unwrap_or_default()
    }
    pub fn get_client_configuration(&self, client_id: &ClientId) -> Config {
        self.runtime_config
            .get(client_id)
            .or_else(|| self.saved_config.get(client_id))
            .cloned()
            .unwrap_or_default()
    }
    pub fn reconfigure_runtime_config(
        &mut self,
        client_id: &ClientId,
        stringified_config: String,
    ) -> (Option<Config>, bool) {
        // bool is whether the config changed
        let mut full_reconfigured_config = None;
        let mut config_changed = false;
        let current_client_configuration = self.get_client_configuration(client_id);
        match Config::from_kdl(
            &stringified_config,
            Some(current_client_configuration.clone()),
        ) {
            Ok(new_config) => {
                config_changed = current_client_configuration != new_config;
                full_reconfigured_config = Some(new_config.clone());
                self.runtime_config.insert(*client_id, new_config);
            },
            Err(e) => {
                log::error!("Failed to reconfigure runtime config: {}", e);
            },
        }
        (full_reconfigured_config, config_changed)
    }
    pub fn rebind_keys(
        &mut self,
        client_id: &ClientId,
        keys_to_rebind: Vec<(InputMode, KeyWithModifier, Vec<Action>)>,
        keys_to_unbind: Vec<(InputMode, KeyWithModifier)>,
    ) -> (Option<Config>, bool) {
        let mut full_reconfigured_config = None;
        let mut config_changed = false;

        if self.runtime_config.get(client_id).is_none() {
            if let Some(saved_config) = self.saved_config.get(client_id) {
                self.runtime_config.insert(*client_id, saved_config.clone());
            }
        }
        match self.runtime_config.get_mut(client_id) {
            Some(config) => {
                for (input_mode, key_with_modifier) in keys_to_unbind {
                    let keys_in_mode = config
                        .keybinds
                        .0
                        .entry(input_mode)
                        .or_insert_with(Default::default);
                    let removed = keys_in_mode.remove(&key_with_modifier);
                    if removed.is_some() {
                        config_changed = true;
                    }
                }
                for (input_mode, key_with_modifier, actions) in keys_to_rebind {
                    let keys_in_mode = config
                        .keybinds
                        .0
                        .entry(input_mode)
                        .or_insert_with(Default::default);
                    if keys_in_mode.get(&key_with_modifier) != Some(&actions) {
                        config_changed = true;
                        keys_in_mode.insert(key_with_modifier, actions);
                    }
                }
                if config_changed {
                    full_reconfigured_config = Some(config.clone());
                }
            },
            None => {
                log::error!(
                    "Could not find runtime or saved configuration for client, cannot rebind keys"
                );
            },
        }

        (full_reconfigured_config, config_changed)
    }
}

pub(crate) struct SessionMetaData {
    pub senders: ThreadSenders,
    pub capabilities: PluginCapabilities,
    pub client_attributes: ClientAttributes,
    pub default_shell: Option<TerminalAction>,
    pub layout: Box<Layout>,
    pub current_input_modes: HashMap<ClientId, InputMode>,
    pub session_configuration: SessionConfiguration,
    pub web_sharing: WebSharing, // this is a special attribute explicitly set on session
    // initialization because we don't want it to be overridden by
    // configuration changes, the only way it can be overwritten is by
    // explicit plugin action
    screen_thread: Option<thread::JoinHandle<()>>,
    pty_thread: Option<thread::JoinHandle<()>>,
    plugin_thread: Option<thread::JoinHandle<()>>,
    pty_writer_thread: Option<thread::JoinHandle<()>>,
    background_jobs_thread: Option<thread::JoinHandle<()>>,
}

impl SessionMetaData {
    pub fn get_client_keybinds_and_mode(
        &self,
        client_id: &ClientId,
    ) -> Option<(Keybinds, &InputMode, InputMode)> {
        // (keybinds, current_input_mode,
        // default_input_mode)
        let client_keybinds = self.session_configuration.get_client_keybinds(client_id);
        let default_input_mode = self
            .session_configuration
            .get_client_default_input_mode(client_id);
        match self.current_input_modes.get(client_id) {
            Some(client_input_mode) => {
                Some((client_keybinds, client_input_mode, default_input_mode))
            },
            _ => None,
        }
    }
    pub fn change_mode_for_all_clients(&mut self, input_mode: InputMode) {
        let all_clients: Vec<ClientId> = self.current_input_modes.keys().copied().collect();
        for client_id in all_clients {
            self.current_input_modes.insert(client_id, input_mode);
        }
    }
    pub fn propagate_configuration_changes(
        &mut self,
        config_changes: Vec<(ClientId, Config)>,
        config_was_written_to_disk: bool,
    ) {
        for (client_id, new_config) in config_changes {
            self.default_shell = new_config.options.default_shell.as_ref().map(|shell| {
                TerminalAction::RunCommand(RunCommand {
                    command: shell.clone(),
                    cwd: new_config.options.default_cwd.clone(),
                    use_terminal_title: true,
                    ..Default::default()
                })
            });
            self.senders
                .send_to_screen(ScreenInstruction::Reconfigure {
                    client_id,
                    keybinds: new_config.keybinds.clone(),
                    default_mode: new_config
                        .options
                        .default_mode
                        .unwrap_or_else(Default::default),
                    theme: new_config
                        .theme_config(new_config.options.theme.as_ref())
                        .unwrap_or_else(|| default_palette().into()),
                    simplified_ui: new_config.options.simplified_ui.unwrap_or(false),
                    default_shell: new_config.options.default_shell,
                    pane_frames: new_config.options.pane_frames.unwrap_or(true),
                    copy_command: new_config.options.copy_command,
                    copy_to_clipboard: new_config.options.copy_clipboard,
                    copy_on_select: new_config.options.copy_on_select.unwrap_or(true),
                    auto_layout: new_config.options.auto_layout.unwrap_or(true),
                    rounded_corners: new_config.ui.pane_frames.rounded_corners,
                    hide_session_name: new_config.ui.pane_frames.hide_session_name,
                    stacked_resize: new_config.options.stacked_resize.unwrap_or(true),
                    default_editor: new_config.options.scrollback_editor.clone(),
                    advanced_mouse_actions: new_config
                        .options
                        .advanced_mouse_actions
                        .unwrap_or(true),
                })
                .unwrap();
            self.senders
                .send_to_plugin(PluginInstruction::Reconfigure {
                    client_id,
                    keybinds: Some(new_config.keybinds),
                    default_mode: new_config.options.default_mode,
                    default_shell: self.default_shell.clone(),
                    was_written_to_disk: config_was_written_to_disk,
                })
                .unwrap();
            self.senders
                .send_to_pty(PtyInstruction::Reconfigure {
                    client_id,
                    default_editor: new_config.options.scrollback_editor,
                    post_command_discovery_hook: new_config.options.post_command_discovery_hook,
                })
                .unwrap();
        }
    }
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
    clients: HashMap<ClientId, Option<(Size, bool)>>, // bool -> is_web_client
    pipes: HashMap<String, ClientId>,                 // String => pipe_id
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
        self.clients
            .entry(client_id)
            .or_insert_with(Default::default)
            .as_mut()
            .map(|(s, _is_web_client)| *s = size);
    }
    pub fn set_client_data(&mut self, client_id: ClientId, size: Size, is_web_client: bool) {
        self.clients.insert(client_id, Some((size, is_web_client)));
    }
    pub fn min_client_terminal_size(&self) -> Option<Size> {
        // None if there are no client sizes
        let mut rows: Vec<usize> = self
            .clients
            .values()
            .filter_map(|size_and_is_web_client| {
                size_and_is_web_client.map(|(size, _is_web_client)| size.rows)
            })
            .collect();
        rows.sort_unstable();
        let mut cols: Vec<usize> = self
            .clients
            .values()
            .filter_map(|size_and_is_web_client| {
                size_and_is_web_client.map(|(size, _is_web_client)| size.cols)
            })
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
    pub fn web_client_ids(&self) -> Vec<ClientId> {
        self.clients
            .iter()
            .filter_map(|(c_id, size_and_is_web_client)| {
                size_and_is_web_client
                    .and_then(|(_s, is_web_client)| if is_web_client { Some(*c_id) } else { None })
            })
            .collect()
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
        .umask(current_umask.bits() as u32)
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
            use interprocess::local_socket::LocalSocketListener;
            use zellij_utils::shared::set_permissions;

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
                // TODO: rename to FirstClientConnected?
                cli_assets,
                opts,
                layout,
                plugin_aliases,
                should_launch_setup_wizard,
                is_web_client,
                layout_is_welcome_screen,
                client_id,
            ) => {

                let config = Config::from(&cli_assets);

                let runtime_config_options = match cli_assets.explicit_cli_options {
                    Some(explicit_cli_options) => config.options.merge(explicit_cli_options),
                    None => config.options.clone()
                };

                let client_attributes = ClientAttributes {
                    size: cli_assets.terminal_window_size,
                    style: Style {
                        colors: config
                            .theme_config(runtime_config_options.theme.as_ref())
                            .unwrap_or_else(|| default_palette().into()),
                        rounded_corners: config.ui.pane_frames.rounded_corners,
                        hide_session_name: config.ui.pane_frames.hide_session_name,
                    },
                };


                let mut session = init_session(
                    os_input.clone(),
                    to_server.clone(),
                    client_attributes.clone(),
                    SessionOptions {
                        opts,
                        layout: layout.clone(), // TODO: no box
                        config_options: Box::new(runtime_config_options.clone()), // TODO: no box
                    },
                    config.clone(),
                    plugin_aliases,
                    client_id,
                );
                let mut runtime_configuration = config.clone();
                runtime_configuration.options = runtime_config_options.clone();
                session
                    .session_configuration
                    .set_client_saved_configuration(client_id, config.clone());
                session
                    .session_configuration
                    .set_client_runtime_configuration(client_id, runtime_configuration);
                let default_input_mode = runtime_config_options.default_mode.unwrap_or_default();
                session
                    .current_input_modes
                    .insert(client_id, default_input_mode);

                *session_data.write().unwrap() = Some(session);
                session_state.write().unwrap().set_client_data(
                    client_id,
                    client_attributes.size,
                    is_web_client,
                );

                let default_shell = runtime_config_options.default_shell.map(|shell| {
                    TerminalAction::RunCommand(RunCommand {
                        command: shell,
                        cwd: config.options.default_cwd.clone(),
                        use_terminal_title: true,
                        ..Default::default()
                    })
                });
                let cwd = runtime_config_options.default_cwd;

                let spawn_tabs = |tab_layout,
                                  floating_panes_layout,
                                  tab_name,
                                  swap_layouts,
                                  should_focus_tab| {
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
                            should_focus_tab,
                            (client_id, is_web_client),
                        ))
                        .unwrap()
                };

                if layout.has_tabs() {
                    let focused_tab_index = layout.focused_tab_index().unwrap_or(0);
                    for (tab_index, (tab_name, tab_layout, floating_panes_layout)) in
                        layout.tabs().into_iter().enumerate()
                    {
                        let should_focus_tab = tab_index == focused_tab_index;
                        spawn_tabs(
                            Some(tab_layout.clone()),
                            floating_panes_layout.clone(),
                            tab_name,
                            (
                                layout.swap_tiled_layouts.clone(),
                                layout.swap_floating_layouts.clone(),
                            ),
                            should_focus_tab,
                        );
                    }
                } else {
                    let mut floating_panes =
                        layout.template.map(|t| t.1).clone().unwrap_or_default();
                    if should_launch_setup_wizard {
                        // we only do this here (and only once) because otherwise it will be
                        // intrusive
                        let setup_wizard = setup_wizard_floating_pane();
                        floating_panes.push(setup_wizard);
                    } else if should_show_release_notes(
                        runtime_config_options.show_release_notes,
                        layout_is_welcome_screen,
                    ) {
                        let about = about_floating_pane();
                        floating_panes.push(about);
                    } else if should_show_startup_tip(
                        runtime_config_options.show_startup_tips,
                        layout_is_welcome_screen,
                    ) {
                        let tip = tip_floating_pane();
                        floating_panes.push(tip);
                    }
                    spawn_tabs(
                        None,
                        floating_panes,
                        None,
                        (
                            layout.swap_tiled_layouts.clone(),
                            layout.swap_floating_layouts.clone(),
                        ),
                        true,
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
                cli_assets,
                tab_position_to_focus,
                pane_id_to_focus,
                is_web_client,
                client_id,
            ) => {

                // TODO(REFACTOR): things we need instead of opts:
                // 1. opts.config (config_file_path)
                // 2. are we running with setup --clean (so that we only use the default config)
                // 3. opts.config_dir
                // 4. the cli config Options (also the attach command options)
                // 5. opts.layout (the path to the layout if any, including layout_dir)
                // 6. the theme (and the theme dir)
                //
                // CONTINUE HERE -
                // * pass all the other stuff from the client that doesn't need to be there and do
                // it here (eg.ClientAttributes and such) <== CONTINUE HERE (already started,
                // follow compilation errors)
                // * make sure the start_server_detached thing still works
                // * get config live reloading  to work again (also for the web server)
                // * refactor
                // * do a  go-over to see if we missed anything, test thoroughly and commit
                // * fix web_client/session_managements.rs to also construct cli_assets -
                //    placeholder, do later
                //
                // 
                //

//                 let (
//                     config,
//                     layout,
//                     config_options,
//                     mut config_without_layout,
//                     mut config_options_without_layout,
//                 ) = match Setup::from_cli_args(&opts) {
//                     Ok(results) => results,
//                     Err(e) => {
//                         // TODO: default config somehow...?
//                         unimplemented!();
//                     },
//                 };

                let config = Config::from(&cli_assets);
                // TODO(REFACTOR): here we changed merge_from_cli to merge - I *think* this is more
                // correct, but it's technically a behavior change... need to play with this a
                // little to make sure it works properly
                let runtime_config_options = match cli_assets.explicit_cli_options {
                    Some(explicit_cli_options) => config.options.merge(explicit_cli_options),
                    None => config.options.clone()
                };

                let client_attributes = ClientAttributes {
                    size: cli_assets.terminal_window_size,
                    style: Style {
                        colors: config
                            .theme_config(runtime_config_options.theme.as_ref())
                            .unwrap_or_else(|| default_palette().into()),
                        rounded_corners: config.ui.pane_frames.rounded_corners,
                        hide_session_name: config.ui.pane_frames.hide_session_name,
                    },
                };

                let mut rlock = session_data.write().unwrap();
                let session_data = rlock.as_mut().unwrap();

                let mut runtime_configuration = config.clone();
                runtime_configuration.options = runtime_config_options.clone();
                session_data
                    .session_configuration
                    .set_client_saved_configuration(client_id, config.clone());
                session_data
                    .session_configuration
                    .set_client_runtime_configuration(client_id, runtime_configuration);

                let default_input_mode = config.options.default_mode.unwrap_or_default();
                session_data
                    .current_input_modes
                    .insert(client_id, default_input_mode);

                session_state.write().unwrap().set_client_data(
                    client_id,
                    client_attributes.size,
                    is_web_client,
                );
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
                        is_web_client,
                        tab_position_to_focus,
                        pane_id_to_focus,
                    ))
                    .unwrap();
                session_data
                    .senders
                    .send_to_plugin(PluginInstruction::AddClient(client_id))
                    .unwrap();
                let default_mode = config.options.default_mode.unwrap_or_default();
                let mode_info = get_mode_info(
                    default_mode,
                    &client_attributes,
                    session_data.capabilities,
                    &session_data
                        .session_configuration
                        .get_client_keybinds(&client_id),
                    Some(default_mode),
                );
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
            ServerInstruction::SendWebClientsForbidden(client_id) => {
                let _ = os_input.send_to_client(
                    client_id,
                    ServerToClientMsg::Exit(ExitReason::WebClientsForbidden),
                );
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
                let current_session_name = envs::get_session_name();
                if connect_to_session.name == current_session_name.ok() {
                    log::error!("Cannot attach to same session");
                } else {
                    let layout_dir = session_data
                        .read()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .session_configuration
                        .get_client_configuration(&client_id)
                        .options
                        .layout_dir
                        .or_else(|| default_layout_dir());
                    if let Some(layout_dir) = layout_dir {
                        connect_to_session.apply_layout_dir(&layout_dir);
                    }
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
                    send_to_client!(
                        client_id,
                        os_input,
                        ServerToClientMsg::SwitchSession(connect_to_session),
                        session_state
                    );
                    remove_client!(client_id, os_input, session_state);
                }
            },
            ServerInstruction::AssociatePipeWithClient { pipe_id, client_id } => {
                session_state
                    .write()
                    .unwrap()
                    .associate_pipe_with_client(pipe_id, client_id);
            },
            ServerInstruction::ChangeMode(client_id, input_mode) => {
                session_data
                    .write()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .current_input_modes
                    .insert(client_id, input_mode);
            },
            ServerInstruction::ChangeModeForAllClients(input_mode) => {
                session_data
                    .write()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .change_mode_for_all_clients(input_mode);
            },
            ServerInstruction::Reconfigure {
                client_id,
                config,
                write_config_to_disk,
            } => {
                let (new_config, runtime_config_changed) = session_data
                    .write()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .session_configuration
                    .reconfigure_runtime_config(&client_id, config);

                if let Some(new_config) = new_config {
                    if write_config_to_disk {
                        let clear_defaults = true;
//                         send_to_client!(
//                             client_id,
//                             os_input,
//                             ServerToClientMsg::WriteConfigToDisk {
//                                 config: new_config.to_string(clear_defaults)
//                             },
//                             session_state
//                         );
                    }

                    if runtime_config_changed {
                        let config_was_written_to_disk = false;
                        session_data
                            .write()
                            .unwrap()
                            .as_mut()
                            .unwrap()
                            .propagate_configuration_changes(
                                vec![(client_id, new_config)],
                                config_was_written_to_disk,
                            );
                    }
                }
            },
//             ServerInstruction::ConfigWrittenToDisk(client_id, new_config) => {
//                 let changes = session_data
//                     .write()
//                     .unwrap()
//                     .as_mut()
//                     .unwrap()
//                     .session_configuration
//                     .new_saved_config(client_id, new_config);
//                 let config_was_written_to_disk = true;
//                 session_data
//                     .write()
//                     .unwrap()
//                     .as_mut()
//                     .unwrap()
//                     .propagate_configuration_changes(changes, config_was_written_to_disk);
//             },
            ServerInstruction::FailedToWriteConfigToDisk(_client_id, file_path) => {
                session_data
                    .write()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_plugin(PluginInstruction::FailedToWriteConfigToDisk { file_path })
                    .unwrap();
            },
            ServerInstruction::RebindKeys {
                client_id,
                keys_to_rebind,
                keys_to_unbind,
                write_config_to_disk,
            } => {
                let (new_config, runtime_config_changed) = session_data
                    .write()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .session_configuration
                    .rebind_keys(&client_id, keys_to_rebind, keys_to_unbind);
                if let Some(new_config) = new_config {
                    if write_config_to_disk {
                        let clear_defaults = true;
//                         send_to_client!(
//                             client_id,
//                             os_input,
//                             ServerToClientMsg::WriteConfigToDisk {
//                                 config: new_config.to_string(clear_defaults)
//                             },
//                             session_state
//                         );
                    }

                    if runtime_config_changed {
                        let config_was_written_to_disk = false;
                        session_data
                            .write()
                            .unwrap()
                            .as_mut()
                            .unwrap()
                            .propagate_configuration_changes(
                                vec![(client_id, new_config)],
                                config_was_written_to_disk,
                            );
                    }
                }
            },
            ServerInstruction::StartWebServer(client_id) => {
                if cfg!(feature = "web_server_capability") {
                    send_to_client!(
                        client_id,
                        os_input,
                        ServerToClientMsg::StartWebServer,
                        session_state
                    );
                } else {
                    // TODO: test this
                    log::error!("Cannot start web server: this instance of Zellij was compiled without web_server_capability");
                }
            },
            ServerInstruction::ShareCurrentSession(_client_id) => {
                if cfg!(feature = "web_server_capability") {
                    let successfully_changed = session_data
                        .write()
                        .ok()
                        .and_then(|mut s| s.as_mut().map(|s| s.web_sharing.set_sharing()))
                        .unwrap_or(false);
                    if successfully_changed {
                        session_data
                            .write()
                            .unwrap()
                            .as_ref()
                            .unwrap()
                            .senders
                            .send_to_screen(ScreenInstruction::SessionSharingStatusChange(true))
                            .unwrap();
                    }
                } else {
                    log::error!("Cannot share session: this instance of Zellij was compiled without web_server_capability");
                }
            },
            ServerInstruction::StopSharingCurrentSession(_client_id) => {
                if cfg!(feature = "web_server_capability") {
                    let successfully_changed = session_data
                        .write()
                        .ok()
                        .and_then(|mut s| s.as_mut().map(|s| s.web_sharing.set_not_sharing()))
                        .unwrap_or(false);
                    if successfully_changed {
                        // disconnect existing web clients
                        let web_client_ids: Vec<ClientId> = session_state
                            .read()
                            .unwrap()
                            .web_client_ids()
                            .iter()
                            .copied()
                            .collect();
                        for client_id in web_client_ids {
                            let _ = os_input.send_to_client(
                                client_id,
                                ServerToClientMsg::Exit(ExitReason::WebClientsForbidden),
                            );
                            remove_client!(client_id, os_input, session_state);
                        }

                        session_data
                            .write()
                            .unwrap()
                            .as_ref()
                            .unwrap()
                            .senders
                            .send_to_screen(ScreenInstruction::SessionSharingStatusChange(false))
                            .unwrap();
                    }
                } else {
                    // TODO: test this
                    log::error!("Cannot start web server: this instance of Zellij was compiled without web_server_capability");
                }
            },
            ServerInstruction::WebServerStarted(base_url) => {
                session_data
                    .write()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_plugin(PluginInstruction::WebServerStarted(base_url))
                    .unwrap();
            },
            ServerInstruction::FailedToStartWebServer(error) => {
                session_data
                    .write()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_plugin(PluginInstruction::FailedToStartWebServer(error))
                    .unwrap();
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
    mut config: Config,
    plugin_aliases: Box<PluginAliases>,
    client_id: ClientId,
) -> SessionMetaData {
    let SessionOptions {
        opts,
        config_options,
        layout,
    } = options;
    config.options = config.options.merge(*config_options.clone());

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
    let web_server_ip = config_options
        .web_server_ip
        .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    let web_server_port = config_options.web_server_port.unwrap_or_else(|| 8082);
    let has_certificate =
        config_options.web_server_cert.is_some() && config_options.web_server_key.is_some();
    let enforce_https_for_localhost = config_options.enforce_https_for_localhost.unwrap_or(false);

    let default_shell = config_options.default_shell.clone().map(|command| {
        TerminalAction::RunCommand(RunCommand {
            command,
            use_terminal_title: true,
            ..Default::default()
        })
    });
    let path_to_default_shell = config_options
        .default_shell
        .clone()
        .unwrap_or_else(|| get_default_shell());

    let default_mode = config_options.default_mode.unwrap_or_default();
    let default_keybinds = config.keybinds.clone();

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
                config_options.post_command_discovery_hook.clone(),
            );

            move || pty_thread_main(pty, layout.clone()).fatal()
        })
        .unwrap();

    let screen_thread = thread::Builder::new()
        .name("screen".to_string())
        .spawn({
            let screen_bus = Bus::new(
                vec![screen_receiver, bounded_screen_receiver],
                Some(&to_screen), // there are certain occasions (eg. caching) where the screen
                // needs to send messages to itself
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
            let config = config.clone();
            move || {
                screen_thread_main(
                    screen_bus,
                    max_panes,
                    client_attributes_clone,
                    config,
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
            let engine = get_engine();

            let layout = layout.clone();
            let client_attributes = client_attributes.clone();
            let default_shell = default_shell.clone();
            let capabilities = capabilities.clone();
            let layout_dir = config_options.layout_dir.clone();
            let background_plugins = config.background_plugins.clone();
            move || {
                plugin_thread_main(
                    plugin_bus,
                    engine,
                    data_dir,
                    layout,
                    layout_dir,
                    path_to_default_shell,
                    zellij_cwd,
                    capabilities,
                    client_attributes,
                    default_shell,
                    plugin_aliases,
                    default_mode,
                    default_keybinds,
                    background_plugins,
                    client_id,
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
            let web_server_base_url = web_server_base_url(
                web_server_ip,
                web_server_port,
                has_certificate,
                enforce_https_for_localhost,
            );
            move || {
                background_jobs_main(
                    background_jobs_bus,
                    serialization_interval,
                    disable_session_metadata,
                    web_server_base_url,
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
        session_configuration: Default::default(),
        current_input_modes: HashMap::new(),
        screen_thread: Some(screen_thread),
        pty_thread: Some(pty_thread),
        plugin_thread: Some(plugin_thread),
        pty_writer_thread: Some(pty_writer_thread),
        background_jobs_thread: Some(background_jobs_thread),
        #[cfg(feature = "web_server_capability")]
        web_sharing: config.options.web_sharing.unwrap_or(WebSharing::Off),
        #[cfg(not(feature = "web_server_capability"))]
        web_sharing: WebSharing::Disabled,
    }
}

fn setup_wizard_floating_pane() -> FloatingPaneLayout {
    let mut setup_wizard_pane = FloatingPaneLayout::new();
    let configuration = BTreeMap::from_iter([("is_setup_wizard".to_owned(), "true".to_owned())]);
    setup_wizard_pane.run = Some(Run::Plugin(RunPluginOrAlias::Alias(PluginAlias::new(
        "configuration",
        &Some(configuration),
        None,
    ))));
    setup_wizard_pane
}

fn about_floating_pane() -> FloatingPaneLayout {
    let mut about_pane = FloatingPaneLayout::new();
    let configuration = BTreeMap::from_iter([("is_release_notes".to_owned(), "true".to_owned())]);
    about_pane.run = Some(Run::Plugin(RunPluginOrAlias::Alias(PluginAlias::new(
        "about",
        &Some(configuration),
        None,
    ))));
    about_pane
}

fn tip_floating_pane() -> FloatingPaneLayout {
    let mut about_pane = FloatingPaneLayout::new();
    let configuration = BTreeMap::from_iter([("is_startup_tip".to_owned(), "true".to_owned())]);
    about_pane.run = Some(Run::Plugin(RunPluginOrAlias::Alias(PluginAlias::new(
        "about",
        &Some(configuration),
        None,
    ))));
    about_pane
}

fn should_show_release_notes(
    should_show_release_notes_config: Option<bool>,
    layout_is_welcome_screen: bool,
) -> bool {
    if layout_is_welcome_screen {
        return false;
    }
    if let Some(should_show_release_notes_config) = should_show_release_notes_config {
        if !should_show_release_notes_config {
            // if we were explicitly told not to show release notes, we don't show them,
            // otherwise we make sure we only show them if they were not seen AND we know
            // we are able to write to the cache
            return false;
        }
    }
    if ZELLIJ_SEEN_RELEASE_NOTES_CACHE_FILE.exists() {
        return false;
    } else {
        if let Err(e) = std::fs::write(&*ZELLIJ_SEEN_RELEASE_NOTES_CACHE_FILE, &[]) {
            log::error!(
                "Failed to write seen release notes indication to disk: {}",
                e
            );
            return false;
        }
        return true;
    }
}

fn should_show_startup_tip(
    should_show_startup_tip_config: Option<bool>,
    layout_is_welcome_screen: bool,
) -> bool {
    if layout_is_welcome_screen {
        false
    } else {
        should_show_startup_tip_config.unwrap_or(true)
    }
}

#[cfg(not(feature = "singlepass"))]
fn get_engine() -> Engine {
    log::info!("Compiling plugins using Cranelift");
    Engine::new(WasmtimeConfig::new().strategy(Strategy::Cranelift)).unwrap()
}

#[cfg(feature = "singlepass")]
fn get_engine() -> Engine {
    log::info!("Compiling plugins using Singlepass");
    Engine::new(WasmtimeConfig::new().strategy(Strategy::Winch)).unwrap()
}

mod plugin_loader;
mod plugin_map;
mod plugin_worker;
mod wasm_bridge;
mod watch_filesystem;
mod zellij_exports;
use log::info;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
};
use wasmer::Store;

use crate::screen::ScreenInstruction;
use crate::{pty::PtyInstruction, thread_bus::Bus, ClientId, ServerInstruction};

use wasm_bridge::WasmBridge;

use zellij_utils::{
    consts::ZELLIJ_PLUGIN_PERMISSIONS_FILE,
    data::{Event, EventType, PermissionType, PluginCapabilities},
    errors::{prelude::*, ContextType, PluginContext},
    input::{
        command::TerminalAction,
        layout::{FloatingPaneLayout, Layout, Run, RunPlugin, RunPluginLocation, TiledPaneLayout},
        permission::GrantedPermission,
        plugins::PluginsConfig,
    },
    ipc::ClientAttributes,
    pane_size::Size,
};

pub type PluginId = u32;

#[derive(Clone, Debug)]
pub enum PluginInstruction {
    Load(
        Option<bool>,   // should float
        Option<String>, // pane title
        RunPlugin,
        usize, // tab index
        ClientId,
        Size,
    ),
    Update(Vec<(Option<PluginId>, Option<ClientId>, Event)>), // Focused plugin / broadcast, client_id, event data
    Unload(PluginId),                                         // plugin_id
    Reload(
        Option<bool>,   // should float
        Option<String>, // pane title
        RunPlugin,
        usize, // tab index
        Size,
    ),
    Resize(PluginId, usize, usize), // plugin_id, columns, rows
    AddClient(ClientId),
    RemoveClient(ClientId),
    NewTab(
        Option<PathBuf>,
        Option<TerminalAction>,
        Option<TiledPaneLayout>,
        Vec<FloatingPaneLayout>,
        usize, // tab_index
        ClientId,
    ),
    ApplyCachedEvents(Vec<PluginId>),
    ApplyCachedWorkerMessages(PluginId),
    PostMessagesToPluginWorker(
        PluginId,
        ClientId,
        String, // worker name
        Vec<(
            String, // serialized message name
            String, // serialized payload
        )>,
    ),
    PostMessageToPlugin(
        PluginId,
        ClientId,
        String, // serialized message
        String, // serialized payload
    ),
    PluginSubscribedToEvents(PluginId, ClientId, HashSet<EventType>),
    PermissionRequestResult(PluginId, Option<ClientId>, Vec<PermissionType>, bool),
    Exit,
}

impl From<&PluginInstruction> for PluginContext {
    fn from(plugin_instruction: &PluginInstruction) -> Self {
        match *plugin_instruction {
            PluginInstruction::Load(..) => PluginContext::Load,
            PluginInstruction::Update(..) => PluginContext::Update,
            PluginInstruction::Unload(..) => PluginContext::Unload,
            PluginInstruction::Reload(..) => PluginContext::Reload,
            PluginInstruction::Resize(..) => PluginContext::Resize,
            PluginInstruction::Exit => PluginContext::Exit,
            PluginInstruction::AddClient(_) => PluginContext::AddClient,
            PluginInstruction::RemoveClient(_) => PluginContext::RemoveClient,
            PluginInstruction::NewTab(..) => PluginContext::NewTab,
            PluginInstruction::ApplyCachedEvents(..) => PluginContext::ApplyCachedEvents,
            PluginInstruction::ApplyCachedWorkerMessages(..) => {
                PluginContext::ApplyCachedWorkerMessages
            },
            PluginInstruction::PostMessagesToPluginWorker(..) => {
                PluginContext::PostMessageToPluginWorker
            },
            PluginInstruction::PostMessageToPlugin(..) => PluginContext::PostMessageToPlugin,
            PluginInstruction::PluginSubscribedToEvents(..) => {
                PluginContext::PluginSubscribedToEvents
            },
            PluginInstruction::PermissionRequestResult(..) => {
                PluginContext::PermissionRequestResult
            },
        }
    }
}

pub(crate) fn plugin_thread_main(
    bus: Bus<PluginInstruction>,
    store: Store,
    data_dir: PathBuf,
    plugins: PluginsConfig,
    layout: Box<Layout>,
    path_to_default_shell: PathBuf,
    zellij_cwd: PathBuf,
    capabilities: PluginCapabilities,
    client_attributes: ClientAttributes,
    default_shell: Option<TerminalAction>,
) -> Result<()> {
    info!("Wasm main thread starts");

    let plugin_dir = data_dir.join("plugins/");
    let plugin_global_data_dir = plugin_dir.join("data");

    let granted_permission =
        match fs::read_to_string(plugin_dir.join(ZELLIJ_PLUGIN_PERMISSIONS_FILE)) {
            Ok(s) => match GrantedPermission::from_string(s) {
                Ok(p) => p,
                Err(_) => GrantedPermission::default(),
            },
            Err(_) => GrantedPermission::default(),
        };

    let mut wasm_bridge = WasmBridge::new(
        plugins,
        bus.senders.clone(),
        store,
        plugin_dir,
        granted_permission,
        path_to_default_shell,
        zellij_cwd,
        capabilities,
        client_attributes,
        default_shell,
        layout.clone(),
    );

    loop {
        let (event, mut err_ctx) = bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Plugin((&event).into()));
        match event {
            PluginInstruction::Load(should_float, pane_title, run, tab_index, client_id, size) => {
                match wasm_bridge.load_plugin(&run, tab_index, size, Some(client_id)) {
                    Ok(plugin_id) => {
                        drop(bus.senders.send_to_screen(ScreenInstruction::AddPlugin(
                            should_float,
                            run,
                            pane_title,
                            tab_index,
                            plugin_id,
                        )));
                    },
                    Err(e) => {
                        log::error!("Failed to load plugin: {e}");
                    },
                }
            },
            PluginInstruction::Update(updates) => {
                wasm_bridge.update_plugins(updates)?;
            },
            PluginInstruction::Unload(pid) => {
                wasm_bridge.unload_plugin(pid)?;
            },
            PluginInstruction::Reload(should_float, pane_title, run, tab_index, size) => {
                match wasm_bridge.reload_plugin(&run) {
                    Ok(_) => {
                        let _ = bus
                            .senders
                            .send_to_server(ServerInstruction::UnblockInputThread);
                    },
                    Err(err) => match err.downcast_ref::<ZellijError>() {
                        Some(ZellijError::PluginDoesNotExist) => {
                            log::warn!("Plugin {} not found, starting it instead", run.location);
                            // we intentionally do not provide the client_id here because it belongs to
                            // the cli who spawned the command and is not an existing client_id
                            match wasm_bridge.load_plugin(&run, tab_index, size, None) {
                                Ok(plugin_id) => {
                                    drop(bus.senders.send_to_screen(ScreenInstruction::AddPlugin(
                                        should_float,
                                        run,
                                        pane_title,
                                        tab_index,
                                        plugin_id,
                                    )));
                                },
                                Err(e) => {
                                    log::error!("Failed to load plugin: {e}");
                                },
                            };
                        },
                        _ => {
                            return Err(err);
                        },
                    },
                }
            },
            PluginInstruction::Resize(pid, new_columns, new_rows) => {
                wasm_bridge.resize_plugin(pid, new_columns, new_rows)?;
            },
            PluginInstruction::AddClient(client_id) => {
                wasm_bridge.add_client(client_id)?;
            },
            PluginInstruction::RemoveClient(client_id) => {
                wasm_bridge.remove_client(client_id);
            },
            PluginInstruction::NewTab(
                cwd,
                terminal_action,
                tab_layout,
                floating_panes_layout,
                tab_index,
                client_id,
            ) => {
                let mut plugin_ids: HashMap<RunPluginLocation, Vec<PluginId>> = HashMap::new();
                let mut extracted_run_instructions = tab_layout
                    .clone()
                    .unwrap_or_else(|| layout.new_tab().0)
                    .extract_run_instructions();
                let size = Size::default();
                let floating_panes_layout = if floating_panes_layout.is_empty() {
                    layout.new_tab().1
                } else {
                    floating_panes_layout
                };
                let mut extracted_floating_plugins: Vec<Option<Run>> = floating_panes_layout
                    .iter()
                    .map(|f| f.run.clone())
                    .collect();
                extracted_run_instructions.append(&mut extracted_floating_plugins);
                for run_instruction in extracted_run_instructions {
                    if let Some(Run::Plugin(run)) = run_instruction {
                        let plugin_id =
                            wasm_bridge.load_plugin(&run, tab_index, size, Some(client_id))?;
                        plugin_ids.entry(run.location).or_default().push(plugin_id);
                    }
                }
                drop(bus.senders.send_to_pty(PtyInstruction::NewTab(
                    cwd,
                    terminal_action,
                    tab_layout,
                    floating_panes_layout,
                    tab_index,
                    plugin_ids,
                    client_id,
                )));
            },
            PluginInstruction::ApplyCachedEvents(plugin_id) => {
                wasm_bridge.apply_cached_events(plugin_id)?;
            },
            PluginInstruction::ApplyCachedWorkerMessages(plugin_id) => {
                wasm_bridge.apply_cached_worker_messages(plugin_id)?;
            },
            PluginInstruction::PostMessagesToPluginWorker(
                plugin_id,
                client_id,
                worker_name,
                messages,
            ) => {
                wasm_bridge.post_messages_to_plugin_worker(
                    plugin_id,
                    client_id,
                    worker_name,
                    messages,
                )?;
            },
            PluginInstruction::PostMessageToPlugin(plugin_id, client_id, message, payload) => {
                let updates = vec![(
                    Some(plugin_id),
                    Some(client_id),
                    Event::CustomMessage(message, payload),
                )];
                wasm_bridge.update_plugins(updates)?;
            },
            PluginInstruction::PluginSubscribedToEvents(_plugin_id, _client_id, events) => {
                for event in events {
                    if let EventType::FileSystemCreate
                    | EventType::FileSystemRead
                    | EventType::FileSystemUpdate
                    | EventType::FileSystemDelete = event
                    {
                        wasm_bridge.start_fs_watcher_if_not_started();
                    }
                }
            },
            PluginInstruction::PermissionRequestResult(
                plugin_id,
                client_id,
                permissions,
                result,
            ) => {
                let permissions = if result { permissions } else { Vec::new() };
                match wasm_bridge.caching_plugin_permissions(plugin_id, client_id, permissions) {
                    Ok(_) => {},
                    Err(e) => log::error!("{}", e),
                }

                let updates = vec![(
                    Some(plugin_id),
                    client_id,
                    Event::PermissionRequestResult(result),
                )];
                wasm_bridge.update_plugins(updates)?;
            },
            PluginInstruction::Exit => {
                wasm_bridge.cleanup();
                break;
            },
        }
    }
    info!("wasm main thread exits");

    fs::remove_dir_all(&plugin_global_data_dir)
        .or_else(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                // I don't care...
                Ok(())
            } else {
                Err(err)
            }
        })
        .context("failed to cleanup plugin data directory")
}

#[path = "./unit/plugin_tests.rs"]
#[cfg(test)]
mod plugin_tests;

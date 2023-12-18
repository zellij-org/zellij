mod plugin_loader;
mod plugin_map;
mod plugin_worker;
mod wasm_bridge;
mod watch_filesystem;
mod zellij_exports;
use log::info;
use std::{
    collections::{HashMap, HashSet, BTreeMap},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use wasmer::Store;

use crate::panes::PaneId;
use crate::screen::ScreenInstruction;
use crate::session_layout_metadata::SessionLayoutMetadata;
use crate::{pty::PtyInstruction, thread_bus::Bus, ClientId, ServerInstruction};

use wasm_bridge::WasmBridge;

use zellij_utils::{
    async_std::{channel, future::timeout, task},
    data::{Event, EventType, PermissionStatus, PermissionType, PluginCapabilities},
    errors::{prelude::*, ContextType, PluginContext},
    input::{
        command::TerminalAction,
        layout::{
            FloatingPaneLayout, Layout, PluginUserConfiguration, Run, RunPlugin, RunPluginLocation,
            TiledPaneLayout,
        },
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
        bool,           // should be opened in place
        Option<String>, // pane title
        RunPlugin,
        usize,          // tab index
        Option<PaneId>, // pane id to replace if this is to be opened "in-place"
        ClientId,
        Size,
        Option<PathBuf>, // cwd
        bool,            // skip cache
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
    PermissionRequestResult(
        PluginId,
        Option<ClientId>,
        Vec<PermissionType>,
        PermissionStatus,
        Option<PathBuf>,
    ),
    DumpLayout(SessionLayoutMetadata, ClientId),
    LogLayoutToHd(SessionLayoutMetadata),
    SubscribePluginToCustomMessage(PluginId, ClientId, String), // String -> custom message name
    Message {
        name: String,
        payload: Option<String>,
        plugin: Option<String>,
        args: Option<BTreeMap<String, String>>,
    },
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
            PluginInstruction::DumpLayout(..) => PluginContext::DumpLayout,
            PluginInstruction::LogLayoutToHd(..) => PluginContext::LogLayoutToHd,
            PluginInstruction::SubscribePluginToCustomMessage(..) => PluginContext::SubscribePluginToCustomMessage,
            PluginInstruction::Message{..} => PluginContext::Message,
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

    let store = Arc::new(Mutex::new(store));

    // use this channel to ensure that tasks spawned from this thread terminate before exiting
    // https://tokio.rs/tokio/topics/shutdown#waiting-for-things-to-finish-shutting-down
    let (shutdown_send, shutdown_receive) = channel::bounded::<()>(1);

    let mut wasm_bridge = WasmBridge::new(
        plugins,
        bus.senders.clone(),
        store,
        plugin_dir,
        path_to_default_shell,
        zellij_cwd,
        capabilities,
        client_attributes,
        default_shell,
        layout.clone(),
    );

    let mut plugin_custom_message_subscriptions: HashMap<(PluginId, ClientId), HashSet<String>> = HashMap::new();

    loop {
        let (event, mut err_ctx) = bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Plugin((&event).into()));
        match event {
            PluginInstruction::Load(
                should_float,
                should_be_open_in_place,
                pane_title,
                run,
                tab_index,
                pane_id_to_replace,
                client_id,
                size,
                cwd,
                skip_cache,
            ) => match wasm_bridge.load_plugin(
                &run,
                tab_index,
                size,
                cwd.clone(),
                skip_cache,
                Some(client_id),
            ) {
                Ok(plugin_id) => {
                    drop(bus.senders.send_to_screen(ScreenInstruction::AddPlugin(
                        should_float,
                        should_be_open_in_place,
                        run,
                        pane_title,
                        tab_index,
                        plugin_id,
                        pane_id_to_replace,
                        cwd,
                        Some(client_id),
                    )));
                },
                Err(e) => {
                    log::error!("Failed to load plugin: {e}");
                },
            },
            PluginInstruction::Update(updates) => {
                wasm_bridge.update_plugins(updates, shutdown_send.clone())?;
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
                            let skip_cache = true; // when reloading we always skip cache
                            match wasm_bridge
                                .load_plugin(&run, tab_index, size, None, skip_cache, None)
                            {
                                Ok(plugin_id) => {
                                    let should_be_open_in_place = false;
                                    drop(bus.senders.send_to_screen(ScreenInstruction::AddPlugin(
                                        should_float,
                                        should_be_open_in_place,
                                        run,
                                        pane_title,
                                        tab_index,
                                        plugin_id,
                                        None,
                                        None,
                                        None,
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
                wasm_bridge.resize_plugin(pid, new_columns, new_rows, shutdown_send.clone())?;
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
                let mut plugin_ids: HashMap<
                    (RunPluginLocation, PluginUserConfiguration),
                    Vec<PluginId>,
                > = HashMap::new();
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
                    .filter(|f| !f.already_running)
                    .map(|f| f.run.clone())
                    .collect();
                extracted_run_instructions.append(&mut extracted_floating_plugins);
                for run_instruction in extracted_run_instructions {
                    if let Some(Run::Plugin(run)) = run_instruction {
                        let skip_cache = false;
                        let plugin_id = wasm_bridge.load_plugin(
                            &run,
                            tab_index,
                            size,
                            None,
                            skip_cache,
                            Some(client_id),
                        )?;
                        plugin_ids
                            .entry((run.location, run.configuration))
                            .or_default()
                            .push(plugin_id);
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
                wasm_bridge.apply_cached_events(plugin_id, shutdown_send.clone())?;
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
                wasm_bridge.update_plugins(updates, shutdown_send.clone())?;
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
                status,
                cache_path,
            ) => {
                if let Err(e) = wasm_bridge.cache_plugin_permissions(
                    plugin_id,
                    client_id,
                    permissions,
                    status,
                    cache_path,
                ) {
                    log::error!("{}", e);
                }

                let updates = vec![(
                    Some(plugin_id),
                    client_id,
                    Event::PermissionRequestResult(status),
                )];
                wasm_bridge.update_plugins(updates, shutdown_send.clone())?;
            },
            PluginInstruction::DumpLayout(mut session_layout_metadata, client_id) => {
                populate_session_layout_metadata(&mut session_layout_metadata, &wasm_bridge);
                drop(bus.senders.send_to_pty(PtyInstruction::DumpLayout(
                    session_layout_metadata,
                    client_id,
                )));
            },
            PluginInstruction::LogLayoutToHd(mut session_layout_metadata) => {
                populate_session_layout_metadata(&mut session_layout_metadata, &wasm_bridge);
                drop(
                    bus.senders
                        .send_to_pty(PtyInstruction::LogLayoutToHd(session_layout_metadata)),
                );
            },
            PluginInstruction::SubscribePluginToCustomMessage(plugin_id, client_id, custom_message_name) => {
                plugin_custom_message_subscriptions.entry((plugin_id, client_id)).or_insert_with(HashSet::new).insert(custom_message_name);
            },
            PluginInstruction::Message{name, payload, plugin, args} => { // TODO: remove client_id,
                                                                      // it's from the cli
                // TODO CONTINUE HERE(18/12):
                // * make plugin pretty and make POC with pausing and filtering
                // * remove subscribe mechanism,
                // * we send name+payload either to all plugins if
                // plugin is None or to the specific plugin if it is Some, then adjust accordingly
                // in cli_client et al. - DONE (untested with single plugin)
                let mut updates = vec![];
                match plugin {
                    Some(plugin_url) => {
                        match RunPlugin::from_url(&plugin_url) {
                            Ok(run_plugin) => {
                                let all_plugin_ids = wasm_bridge.all_plugin_and_client_ids_for_plugin_location(&run_plugin.location);
                                for (plugin_id, client_id) in all_plugin_ids {
                                    updates.push((Some(plugin_id), Some(client_id), Event::Message {name: name.clone(), payload: payload.clone(), args: args.clone() }));
                                }
                            },
                            Err(e) => {
                                log::error!("Failed to parse plugin url: {:?}", e);
                                // TODO: inform cli client
                            }
                        }
                    },
                    None => {
                        // send to all plugins
                        let all_plugin_ids = wasm_bridge.all_plugin_ids();
                        for (plugin_id, client_id) in all_plugin_ids {
                            updates.push((Some(plugin_id), Some(client_id), Event::Message{ name: name.clone(), payload: payload.clone(), args: args.clone()}));
                        }
                    }
                }
                wasm_bridge.update_plugins(updates, shutdown_send.clone())?;
            },
            PluginInstruction::Exit => {
                break;
            },
        }
    }
    info!("wasm main thread exits");

    // first drop our sender, then call recv.
    // once all senders are dropped or the timeout is reached, recv will return an error, that we ignore

    drop(shutdown_send);
    task::block_on(async {
        let result = timeout(EXIT_TIMEOUT, shutdown_receive.recv()).await;
        if let Err(err) = result {
            log::error!("timeout waiting for plugin tasks to finish: {}", err);
        }
    });

    wasm_bridge.cleanup();

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

fn populate_session_layout_metadata(
    session_layout_metadata: &mut SessionLayoutMetadata,
    wasm_bridge: &WasmBridge,
) {
    let plugin_ids = session_layout_metadata.all_plugin_ids();
    let mut plugin_ids_to_cmds: HashMap<u32, RunPlugin> = HashMap::new();
    for plugin_id in plugin_ids {
        let plugin_cmd = wasm_bridge.run_plugin_of_plugin_id(plugin_id);
        match plugin_cmd {
            Some(plugin_cmd) => {
                plugin_ids_to_cmds.insert(plugin_id, plugin_cmd.clone());
            },
            None => log::error!("Plugin with id: {plugin_id} not found"),
        }
    }
    session_layout_metadata.update_plugin_cmds(plugin_ids_to_cmds);
}

const EXIT_TIMEOUT: Duration = Duration::from_secs(3);

#[path = "./unit/plugin_tests.rs"]
#[cfg(test)]
mod plugin_tests;

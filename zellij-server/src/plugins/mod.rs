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
    data::{Event, EventType, PermissionStatus, PermissionType, PluginCapabilities, MessageToPlugin},
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
    ApplyCachedEvents { plugin_ids: Vec<PluginId>, done_receiving_permissions: bool },
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
    CliMessage {
        input_pipe_id: String,
        name: String,
        payload: Option<String>,
        plugin: Option<String>,
        args: Option<BTreeMap<String, String>>,
        configuration: Option<BTreeMap<String, String>>,
        floating: Option<bool>,
        pane_id_to_replace: Option<PaneId>,
        pane_title: Option<String>,
        cwd: Option<PathBuf>,
        skip_cache: bool,
    },
    CachePluginEvents { plugin_id: PluginId },
    MessageFromPlugin(MessageToPlugin),
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
            PluginInstruction::ApplyCachedEvents{..} => PluginContext::ApplyCachedEvents,
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
            PluginInstruction::CliMessage {..} => PluginContext::CliMessage,
            PluginInstruction::CachePluginEvents{..} => PluginContext::CachePluginEvents,
            PluginInstruction::MessageFromPlugin{..} => PluginContext::MessageFromPlugin,
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
                Some(tab_index),
                size,
                cwd.clone(),
                skip_cache,
                Some(client_id),
            ) {
                Ok((plugin_id, client_id)) => {
                    drop(bus.senders.send_to_screen(ScreenInstruction::AddPlugin(
                        should_float,
                        should_be_open_in_place,
                        run,
                        pane_title,
                        Some(tab_index),
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
                                .load_plugin(&run, Some(tab_index), size, None, skip_cache, None)
                            {
                                Ok((plugin_id, client_id)) => {
                                    let should_be_open_in_place = false;
                                    drop(bus.senders.send_to_screen(ScreenInstruction::AddPlugin(
                                        should_float,
                                        should_be_open_in_place,
                                        run,
                                        pane_title,
                                        Some(tab_index),
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
                        let (plugin_id, client_id) = wasm_bridge.load_plugin(
                            &run,
                            Some(tab_index),
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
            PluginInstruction::ApplyCachedEvents{plugin_ids, done_receiving_permissions} => {
                wasm_bridge.apply_cached_events(plugin_ids, done_receiving_permissions, shutdown_send.clone())?;
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
                let done_receiving_permissions = true;
                wasm_bridge.apply_cached_events(vec![plugin_id], done_receiving_permissions, shutdown_send.clone())?;
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
            PluginInstruction::CliMessage {
                input_pipe_id,
                name,
                payload,
                plugin,
                args,
                mut configuration,
                floating,
                pane_id_to_replace,
                pane_title,
                cwd,
                skip_cache
            } => {
                // TODO CONTINUE HERE(18/12):
                // * make plugin pretty and make POC with pausing and filtering - DONE
                // * remove subscribe mechanism, - DONE
                // * change unblock mechanism to block - Nah
                // * add permissions - DONE
                // * tests? - DONE
                // * launch plugin if no instances exist and messaging/piping
                //  - test: see that we don't lose messages - DONE
                //  - while we're here, see if the cached messages are unordered/reversed? - DONE!!
                //  - test: launching 2 plugins in different places in the chain (copy wasm file to
                //  another path) - DONE
                //  - make tests pass again (we probably created chaos) - DONE
                //  - test that we're not losing messages when we already have permission - DONE
                //  - same plugin 2 different places in path (with/without permissions) - DONE
                // * allow plugins to send/pipe messages to each other
                //  - change Message (all the way) to CliMessage (also the unblock and pipeoutput
                //  methods' names should reflect this change) - DONE
                //  - create a send_message plugin api that would act like Message but without
                //  backpressure - DONE
                //  - write some plugin api tests for it (like the clioutput, etc. tests) - DONE
                // * bring all the custo moverride stuff form the cli/plugin (the static variables
                // at the beginning of the vairous PluginInstruction methods)
                //  - TODO: check various core multi-tab operations, with plugins, various plugin -
                //  DONE
                //  - TODO: check multiple simultaneous pipes, I think we have some deadlocks with
                //  one Arc or another there - DONE
                //  - TODO: create a unique id for cli pipes and use it for unblocking instead of
                //  the message name - DONE
                //  - TODO: remove the launch_new from everything except the cli place thing - DONE
                //  - TODO: consider re-adding the skip_cache flag - DONE
                //  - TODO: only send messages (unblockclipipeinput, clipipeoutput) to the relevant client and not all of them
                //  - TODO: look into leaking messages (simultaneously piping to 2 instances of the
                //  plugin with --launch-new)
                // * bring all the custo moverride stuff form the plugin messages for when
                // launching a new plugin with a message (like we did through the cli)
                // * add permissions
                // * work on cli error messages, must be clearer

                // TODO:
                // * if the plugin is not running
                // * do a wasm_bridge.load_plugin with as much defaults as possible (accept
                // overrides in the instruction later)
                // * make sure the below all_plugin_and_client_ids_for_plugin_location also returns
                // pending plugins
                // * continue as normal below (make sure to test this with a pipe, maybe even with
                // a pipe to multiple plugins where one of them is not loaded)

                // TODO CONTINUE HERE: accept these as parameters and adjust defaults somewhere/somehow, then test everything manually

                let should_float = floating.unwrap_or(true);
                let size = Size::default(); // TODO: why??
                let mut updates = vec![];
                match plugin {
                    Some(plugin_url) => {
                        match RunPlugin::from_url(&plugin_url) {
                            Ok(mut run_plugin) => {
                                if let Some(configuration) = configuration.take() {
                                    run_plugin.configuration = PluginUserConfiguration::new(configuration);
                                }
                                let all_plugin_ids = wasm_bridge.get_or_load_plugins(
                                    run_plugin,
                                    size,
                                    cwd,
                                    skip_cache,
                                    should_float,
                                    pane_id_to_replace.is_some(),
                                    pane_title,
                                    pane_id_to_replace,
                                );
                                for (plugin_id, client_id) in all_plugin_ids {
                                    updates.push((Some(plugin_id), client_id, Event::CliMessage {input_pipe_id: input_pipe_id.clone(), name: name.clone(), payload: payload.clone(), args: args.clone() }));
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
                            updates.push((Some(plugin_id), Some(client_id), Event::CliMessage{ input_pipe_id: input_pipe_id.clone(), name: name.clone(), payload: payload.clone(), args: args.clone()}));
                        }
                    }
                }
                wasm_bridge.update_plugins(updates, shutdown_send.clone())?;
            },
            PluginInstruction::CachePluginEvents { plugin_id } => {
                wasm_bridge.cache_plugin_events(plugin_id);
            }
            PluginInstruction::MessageFromPlugin(message_to_plugin) => {
                let cwd = None;
                let tab_index = 0;
                let size = Size::default();
                let mut updates = vec![];
                let skip_cache = false;
                let should_float = true;
                let should_be_open_in_place = false;
                let pane_title = None;
                let pane_id_to_replace = None;
                match message_to_plugin.plugin_url {
                    Some(plugin_url) => {
                        match RunPlugin::from_url(&plugin_url) {
                            Ok(run_plugin) => {
                                let all_plugin_ids = wasm_bridge.get_or_load_plugins(
                                    run_plugin,
                                    size,
                                    cwd,
                                    skip_cache,
                                    should_float,
                                    should_be_open_in_place,
                                    pane_title,
                                    pane_id_to_replace,
                                );
                                for (plugin_id, client_id) in all_plugin_ids {
                                    updates.push((Some(plugin_id), client_id, Event::MessageFromPlugin {
                                        name: message_to_plugin.message_name.clone(),
                                        payload: message_to_plugin.message_payload.clone(),
                                        args: Some(message_to_plugin.message_args.clone()),
                                    }));
                                }
                            },
                            Err(e) => {
                                log::error!("Failed to parse plugin url: {:?}", e);
                            }
                        }
                    },
                    None => {
                        // send to all plugins
                        let all_plugin_ids = wasm_bridge.all_plugin_ids();
                        for (plugin_id, client_id) in all_plugin_ids {
                            updates.push((Some(plugin_id), Some(client_id), Event::MessageFromPlugin {
                                name: message_to_plugin.message_name.clone(),
                                payload: message_to_plugin.message_payload.clone(),
                                args: Some(message_to_plugin.message_args.clone()),
                            }));
                        }
                    }
                }
                wasm_bridge.update_plugins(updates, shutdown_send.clone())?;
            }
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

mod pipes;
mod plugin_loader;
mod plugin_map;
mod plugin_worker;
mod wasm_bridge;
mod watch_filesystem;
mod zellij_exports;
use log::info;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
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

pub use wasm_bridge::PluginRenderAsset;
use wasm_bridge::WasmBridge;

use zellij_utils::{
    async_std::{channel, future::timeout, task},
    data::{
        Event, EventType, MessageToPlugin, PermissionStatus, PermissionType, PipeMessage,
        PipeSource, PluginCapabilities,
    },
    errors::{prelude::*, ContextType, PluginContext},
    input::{
        command::TerminalAction,
        layout::{
            FloatingPaneLayout, Layout, Run, RunPlugin, RunPluginOrAlias,
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
        RunPluginOrAlias,
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
        RunPluginOrAlias,
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
    ApplyCachedEvents {
        plugin_ids: Vec<PluginId>,
        done_receiving_permissions: bool,
    },
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
    CliPipe {
        pipe_id: String,
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
        cli_client_id: ClientId,
    },
    CachePluginEvents {
        plugin_id: PluginId,
    },
    MessageFromPlugin {
        source_plugin_id: u32,
        message: MessageToPlugin,
    },
    UnblockCliPipes(Vec<PluginRenderAsset>),
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
            PluginInstruction::ApplyCachedEvents { .. } => PluginContext::ApplyCachedEvents,
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
            PluginInstruction::CliPipe { .. } => PluginContext::CliPipe,
            PluginInstruction::CachePluginEvents { .. } => PluginContext::CachePluginEvents,
            PluginInstruction::MessageFromPlugin { .. } => PluginContext::MessageFromPlugin,
            PluginInstruction::UnblockCliPipes { .. } => PluginContext::UnblockCliPipes,
        }
    }
}

pub(crate) fn plugin_thread_main(
    bus: Bus<PluginInstruction>,
    store: Store,
    data_dir: PathBuf,
    plugins: PluginsConfig,
    mut layout: Box<Layout>,
    path_to_default_shell: PathBuf,
    zellij_cwd: PathBuf,
    capabilities: PluginCapabilities,
    client_attributes: ClientAttributes,
    default_shell: Option<TerminalAction>,
) -> Result<()> {
    info!("Wasm main thread starts");

    let plugin_dir = data_dir.join("plugins/");
    let plugin_global_data_dir = plugin_dir.join("data");

    // TODO: better, not hard coded here, etc.
    let mut plugin_aliases = HashMap::new();
    let mut filter_configuration = BTreeMap::new();
    filter_configuration.insert("key_from_dict".to_owned(), "value_from_dict".to_owned());
    plugin_aliases.insert("session-manager-alias", RunPlugin::from_url("zellij:session-manager").unwrap());
    plugin_aliases.insert("filepicker", RunPlugin::from_url("zellij:strider").unwrap());
    plugin_aliases.insert(
        "filter",
        RunPlugin::from_url("file:/home/aram/code/rust-plugin-example/target/wasm32-wasi/debug/rust-plugin-example.wasm").unwrap().with_configuration(filter_configuration)
    );
    layout.populate_plugin_aliases_in_layout(&plugin_aliases);

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
                mut run_plugin_or_alias,
                tab_index,
                pane_id_to_replace,
                client_id,
                size,
                cwd,
                skip_cache,
            ) => {
                log::info!("run_plugin_or_alias: {:?}", run_plugin_or_alias);
                run_plugin_or_alias.populate_run_plugin_if_needed(&plugin_aliases);
                let run_plugin = run_plugin_or_alias.get_run_plugin();
                match wasm_bridge.load_plugin(
                    &run_plugin,
                    Some(tab_index),
                    size,
                    cwd.clone(),
                    skip_cache,
                    Some(client_id),
                    None,
                ) {
                    Ok((plugin_id, client_id)) => {
                        drop(bus.senders.send_to_screen(ScreenInstruction::AddPlugin(
                            should_float,
                            should_be_open_in_place,
                            run_plugin_or_alias,
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
                }
            }
            PluginInstruction::Update(updates) => {
                wasm_bridge.update_plugins(updates, shutdown_send.clone())?;
            },
            PluginInstruction::Unload(pid) => {
                wasm_bridge.unload_plugin(pid)?;
            },
            PluginInstruction::Reload(should_float, pane_title, mut run_plugin_or_alias, tab_index, size) => {
                run_plugin_or_alias.populate_run_plugin_if_needed(&plugin_aliases);
                match run_plugin_or_alias.get_run_plugin() {
                    Some(run_plugin) => {
                        match wasm_bridge.reload_plugin(&run_plugin) {
                            Ok(_) => {
                                let _ = bus
                                    .senders
                                    .send_to_server(ServerInstruction::UnblockInputThread);
                            },
                            Err(err) => match err.downcast_ref::<ZellijError>() {
                                Some(ZellijError::PluginDoesNotExist) => {
                                    log::warn!("Plugin {} not found, starting it instead", run_plugin.location);
                                    // we intentionally do not provide the client_id here because it belongs to
                                    // the cli who spawned the command and is not an existing client_id
                                    let skip_cache = true; // when reloading we always skip cache
                                    match wasm_bridge.load_plugin(
                                        &Some(run_plugin),
                                        Some(tab_index),
                                        size,
                                        None,
                                        skip_cache,
                                        None,
                                        None,
                                    ) {
                                        Ok((plugin_id, _client_id)) => {
                                            let should_be_open_in_place = false;
                                            drop(bus.senders.send_to_screen(ScreenInstruction::AddPlugin(
                                                should_float,
                                                should_be_open_in_place,
                                                run_plugin_or_alias,
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
                    None => {
                        log::error!("Failed to find plugin info for: {:?}", run_plugin_or_alias)
                    }
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
                mut tab_layout,
                mut floating_panes_layout,
                tab_index,
                client_id,
            ) => {
                let mut plugin_ids: HashMap<
                    RunPluginOrAlias,
                    Vec<PluginId>,
                > = HashMap::new();
                tab_layout = tab_layout.or_else(|| Some(layout.new_tab().0)); // TODO: does this ruin the
                                                                        // universe???
                tab_layout.as_mut().map(|t| t.populate_plugin_aliases_in_layout(&plugin_aliases));
                floating_panes_layout
                    .iter_mut().for_each(|f| {
                        f.run.as_mut().map(|f| f.populate_run_plugin_if_needed(&plugin_aliases));
                    });
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
                    // if let Some(Run::Plugin(run)) = run_instruction {
                    // TODO: handle plugin alias
                    // if let Some(Run::Plugin(RunPluginOrAlias::RunPlugin(run))) = run_instruction {
                    if let Some(Run::Plugin(run_plugin_or_alias)) = run_instruction {
                        // TODO: add the ability to add an alias to plugin_ids, then add the
                        // alias here and get a new plugin id from wasm_bridge
                        // (wasm_bridge.plugin_not_found(alias)?)
                        // then when applying the layout get it from there if it is None
                        // the above method should use the error capabilities to send a plugin
                        // not found message to said pane
                        let run_plugin = run_plugin_or_alias.get_run_plugin();
                        let skip_cache = false;
                        let (plugin_id, _client_id) = wasm_bridge.load_plugin(
                            &run_plugin,
                            Some(tab_index),
                            size,
                            None,
                            skip_cache,
                            Some(client_id),
                            None,
                        )?;
                        plugin_ids
                            .entry(run_plugin_or_alias.clone())
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
            PluginInstruction::ApplyCachedEvents {
                plugin_ids,
                done_receiving_permissions,
            } => {
                wasm_bridge.apply_cached_events(
                    plugin_ids,
                    done_receiving_permissions,
                    shutdown_send.clone(),
                )?;
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
                wasm_bridge.apply_cached_events(
                    vec![plugin_id],
                    done_receiving_permissions,
                    shutdown_send.clone(),
                )?;
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
            PluginInstruction::CliPipe {
                pipe_id,
                name,
                payload,
                plugin,
                args,
                configuration,
                floating,
                pane_id_to_replace,
                pane_title,
                cwd,
                skip_cache,
                cli_client_id,
            } => {
                let should_float = floating.unwrap_or(true);
                let mut pipe_messages = vec![];
                match plugin {
                    Some(plugin_url) => {
                        // send to specific plugin(s)
                        pipe_to_specific_plugins(
                            PipeSource::Cli(pipe_id.clone()),
                            &plugin_url,
                            &configuration,
                            &cwd,
                            skip_cache,
                            should_float,
                            &pane_id_to_replace,
                            &pane_title,
                            Some(cli_client_id),
                            &mut pipe_messages,
                            &name,
                            &payload,
                            &args,
                            &bus,
                            &mut wasm_bridge,
                            &plugin_aliases,
                        );
                    },
                    None => {
                        // no specific destination, send to all plugins
                        pipe_to_all_plugins(
                            PipeSource::Cli(pipe_id.clone()),
                            &name,
                            &payload,
                            &args,
                            &mut wasm_bridge,
                            &mut pipe_messages,
                        );
                    },
                }
                wasm_bridge.pipe_messages(pipe_messages, shutdown_send.clone())?;
            },
            PluginInstruction::CachePluginEvents { plugin_id } => {
                wasm_bridge.cache_plugin_events(plugin_id);
            },
            PluginInstruction::MessageFromPlugin {
                source_plugin_id,
                message,
            } => {
                let cwd = message.new_plugin_args.as_ref().and_then(|n| n.cwd.clone());
                let mut pipe_messages = vec![];
                let skip_cache = message
                    .new_plugin_args
                    .as_ref()
                    .map(|n| n.skip_cache)
                    .unwrap_or(false);
                let should_float = message
                    .new_plugin_args
                    .as_ref()
                    .and_then(|n| n.should_float)
                    .unwrap_or(true);
                let pane_title = message
                    .new_plugin_args
                    .as_ref()
                    .and_then(|n| n.pane_title.clone());
                let pane_id_to_replace = message
                    .new_plugin_args
                    .as_ref()
                    .and_then(|n| n.pane_id_to_replace);
                match message.plugin_url {
                    Some(plugin_url) => {
                        // send to specific plugin(s)
                        pipe_to_specific_plugins(
                            PipeSource::Plugin(source_plugin_id),
                            &plugin_url,
                            &Some(message.plugin_config),
                            &cwd,
                            skip_cache,
                            should_float,
                            &pane_id_to_replace.map(|p| p.into()),
                            &pane_title,
                            None,
                            &mut pipe_messages,
                            &message.message_name,
                            &message.message_payload,
                            &Some(message.message_args),
                            &bus,
                            &mut wasm_bridge,
                            &plugin_aliases,
                        );
                    },
                    None => {
                        // send to all plugins
                        pipe_to_all_plugins(
                            PipeSource::Plugin(source_plugin_id),
                            &message.message_name,
                            &message.message_payload,
                            &Some(message.message_args),
                            &mut wasm_bridge,
                            &mut pipe_messages,
                        );
                    },
                }
                wasm_bridge.pipe_messages(pipe_messages, shutdown_send.clone())?;
            },
            PluginInstruction::UnblockCliPipes(pipes_to_unblock) => {
                let pipes_to_unblock = wasm_bridge.update_cli_pipe_state(pipes_to_unblock);
                for pipe_name in pipes_to_unblock {
                    let _ = bus
                        .senders
                        .send_to_server(ServerInstruction::UnblockCliPipeInput(pipe_name))
                        .context("failed to unblock input pipe");
                }
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

fn pipe_to_all_plugins(
    pipe_source: PipeSource,
    name: &str,
    payload: &Option<String>,
    args: &Option<BTreeMap<String, String>>,
    wasm_bridge: &mut WasmBridge,
    pipe_messages: &mut Vec<(Option<PluginId>, Option<ClientId>, PipeMessage)>,
) {
    let is_private = false;
    let all_plugin_ids = wasm_bridge.all_plugin_ids();
    for (plugin_id, client_id) in all_plugin_ids {
        pipe_messages.push((
            Some(plugin_id),
            Some(client_id),
            PipeMessage::new(pipe_source.clone(), name, payload, &args, is_private),
        ));
    }
}

fn pipe_to_specific_plugins(
    pipe_source: PipeSource,
    plugin_url: &str,
    configuration: &Option<BTreeMap<String, String>>,
    cwd: &Option<PathBuf>,
    skip_cache: bool,
    should_float: bool,
    pane_id_to_replace: &Option<PaneId>,
    pane_title: &Option<String>,
    cli_client_id: Option<ClientId>,
    pipe_messages: &mut Vec<(Option<PluginId>, Option<ClientId>, PipeMessage)>,
    name: &str,
    payload: &Option<String>,
    args: &Option<BTreeMap<String, String>>,
    bus: &Bus<PluginInstruction>,
    wasm_bridge: &mut WasmBridge,
    plugin_aliases: &HashMap<&str, RunPlugin>,
) {
    let is_private = true;
    let size = Size::default();
    match RunPluginOrAlias::from_url(&plugin_url, configuration, Some(plugin_aliases), cwd.clone()) {
    // match RunPlugin::from_url(&plugin_url) {
        Ok(run_plugin_or_alias) => {
//             if let Some(configuration) = configuration {
//                 run_plugin.configuration = PluginUserConfiguration::new(configuration.clone());
//             }
            let all_plugin_ids = wasm_bridge.get_or_load_plugins(
                run_plugin_or_alias,
                size,
                cwd.clone(),
                skip_cache,
                should_float,
                pane_id_to_replace.is_some(),
                pane_title.clone(),
                pane_id_to_replace.clone(),
                cli_client_id,
            );
            for (plugin_id, client_id) in all_plugin_ids {
                pipe_messages.push((
                    Some(plugin_id),
                    client_id,
                    PipeMessage::new(pipe_source.clone(), name, payload, args, is_private), // PipeMessage::new(PipeSource::Cli(pipe_id.clone()), &name, &payload, &args, is_private)
                ));
            }
        },
        Err(e) => match cli_client_id {
            Some(cli_client_id) => {
                let _ = bus.senders.send_to_server(ServerInstruction::LogError(
                    vec![format!("Failed to parse plugin url: {}", e)],
                    cli_client_id,
                ));
            },
            None => {
                log::error!("Failed to parse plugin url: {}", e);
            },
        },
    }
}

const EXIT_TIMEOUT: Duration = Duration::from_secs(3);

#[path = "./unit/plugin_tests.rs"]
#[cfg(test)]
mod plugin_tests;

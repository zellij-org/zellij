mod plugin_loader;
mod plugin_map;
mod wasm_bridge;
mod zellij_exports;
use log::info;
use std::{collections::HashMap, fs, path::PathBuf};
use wasmer::Store;

use crate::screen::ScreenInstruction;
use crate::{pty::PtyInstruction, thread_bus::Bus, ClientId, ServerInstruction};

use wasm_bridge::WasmBridge;

use zellij_utils::{
    data::Event,
    errors::{prelude::*, ContextType, PluginContext},
    input::{
        command::TerminalAction,
        layout::{FloatingPaneLayout, Layout, Run, RunPlugin, RunPluginLocation, TiledPaneLayout},
        plugins::PluginsConfig,
    },
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
    Update(Vec<(Option<u32>, Option<ClientId>, Event)>), // Focused plugin / broadcast, client_id, event data
    Unload(u32),                                         // plugin_id
    Reload(
        Option<bool>,   // should float
        Option<String>, // pane title
        RunPlugin,
        usize, // tab index
        Size,
    ),
    Resize(u32, usize, usize), // plugin_id, columns, rows
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
    ApplyCachedEvents(Vec<u32>), // a list of plugin id
    PostMessageToPluginWorker(
        PluginId,
        ClientId,
        String, // worker name
        String, // serialized message
        String, // serialized payload
    ),
    PostMessageToPlugin(
        PluginId,
        ClientId,
        String, // serialized message
        String, // serialized payload
    ),
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
            PluginInstruction::PostMessageToPluginWorker(..) => PluginContext::PostMessageToPluginWorker,
            PluginInstruction::PostMessageToPlugin(..) => PluginContext::PostMessageToPlugin,
        }
    }
}

pub(crate) fn plugin_thread_main(
    bus: Bus<PluginInstruction>,
    store: Store,
    data_dir: PathBuf,
    plugins: PluginsConfig,
    layout: Box<Layout>,
) -> Result<()> {
    info!("Wasm main thread starts");

    let plugin_dir = data_dir.join("plugins/");
    let plugin_global_data_dir = plugin_dir.join("data");

    let mut wasm_bridge = WasmBridge::new(plugins, bus.senders.clone(), store, plugin_dir);

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
                let mut plugin_ids: HashMap<RunPluginLocation, Vec<u32>> = HashMap::new();
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
            PluginInstruction::PostMessageToPluginWorker(plugin_id, client_id, worker_name, message, payload) => {
                wasm_bridge.post_message_to_plugin_worker(plugin_id, client_id, worker_name, message, payload)?;
            },
            PluginInstruction::PostMessageToPlugin(plugin_id, client_id, message, payload) => {
                let updates = vec![(
                    Some(plugin_id),
                    Some(client_id),
                    Event::CustomMessage(message, payload)
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

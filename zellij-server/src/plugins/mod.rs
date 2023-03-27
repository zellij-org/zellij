mod wasm_bridge;
mod start_plugin;
use log::info;
use std::{collections::HashMap, fs, path::PathBuf};
use wasmer::Store;

use crate::{pty::PtyInstruction, thread_bus::Bus, ClientId};
use crate::screen::ScreenInstruction;

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

#[derive(Clone, Debug)]
pub enum PluginInstruction {
    Load(Option<bool>, Option<String>, RunPlugin, usize, ClientId, Size), // Option<String> is the pane title, should_float, plugin metadata, tab_index, client_ids
    Update(Vec<(Option<u32>, Option<ClientId>, Event)>), // Focused plugin / broadcast, client_id, event data
    Unload(u32),                                         // plugin_id
    Resize(u32, usize, usize),                           // plugin_id, columns, rows
    AddClient(ClientId),
    RemoveClient(ClientId),
    NewTab(
        Option<TerminalAction>,
        Option<TiledPaneLayout>,
        Vec<FloatingPaneLayout>,
        usize, // tab_index
        ClientId,
    ),
    ApplyCachedEvents(u32), // u32 is the plugin id
    Exit,
}

impl From<&PluginInstruction> for PluginContext {
    fn from(plugin_instruction: &PluginInstruction) -> Self {
        match *plugin_instruction {
            PluginInstruction::Load(..) => PluginContext::Load,
            PluginInstruction::Update(..) => PluginContext::Update,
            PluginInstruction::Unload(..) => PluginContext::Unload,
            PluginInstruction::Resize(..) => PluginContext::Resize,
            PluginInstruction::Exit => PluginContext::Exit,
            PluginInstruction::AddClient(_) => PluginContext::AddClient,
            PluginInstruction::RemoveClient(_) => PluginContext::RemoveClient,
            PluginInstruction::NewTab(..) => PluginContext::NewTab,
            PluginInstruction::ApplyCachedEvents(..) => PluginContext::ApplyCachedEvents,
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
                let plugin_id = wasm_bridge.load_plugin(&run, tab_index, size, client_id)?;
                drop(bus.senders.send_to_screen(ScreenInstruction::AddPlugin(should_float, run, pane_title, tab_index, plugin_id)));
            },
            PluginInstruction::Update(updates) => {
                wasm_bridge.update_plugins(updates)?;
            },
            PluginInstruction::Unload(pid) => {
                wasm_bridge.unload_plugin(pid)?;
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
                            wasm_bridge.load_plugin(&run, tab_index, size, client_id)?;
                        plugin_ids.entry(run.location).or_default().push(plugin_id);
                    }
                }
                drop(bus.senders.send_to_pty(PtyInstruction::NewTab(
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
            }
            PluginInstruction::Exit => {
                wasm_bridge.cleanup();
                break;
            }
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

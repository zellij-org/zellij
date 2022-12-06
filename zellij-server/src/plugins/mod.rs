mod wasm_bridge;
use log::info;
use std::{collections::HashMap, fs, path::PathBuf};
use wasmer::Store;

use crate::{pty::PtyInstruction, thread_bus::Bus, ClientId};

use wasm_bridge::WasmBridge;

use zellij_utils::{
    data::Event,
    errors::{prelude::*, ContextType, PluginContext},
    input::{
        command::TerminalAction,
        layout::{Layout, PaneLayout, Run, RunPlugin, RunPluginLocation},
        plugins::PluginsConfig,
    },
    pane_size::Size,
};

#[derive(Clone, Debug)]
pub enum PluginInstruction {
    Load(RunPlugin, usize, ClientId, Size), // plugin metadata, tab_index, client_ids
    Update(Option<u32>, Option<ClientId>, Event), // Focused plugin / broadcast, client_id, event data
    Unload(u32),                                  // plugin_id
    Resize(u32, usize, usize),                    // plugin_id, columns, rows
    AddClient(ClientId),
    RemoveClient(ClientId),
    NewTab(
        TerminalAction,
        Option<PaneLayout>,
        Option<String>, // tab name
        usize,          // tab_index
        ClientId,
    ),
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
            // TODO: remove pid_tx from here
            PluginInstruction::Load(run, tab_index, client_id, size) => {
                wasm_bridge.load_plugin(&run, tab_index, size, client_id)?;
            },
            PluginInstruction::Update(pid, cid, event) => {
                wasm_bridge.update_plugins(pid, cid, event)?;
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
                tab_name,
                tab_index,
                client_id,
            ) => {
                let mut plugin_ids: HashMap<RunPluginLocation, Vec<u32>> = HashMap::new();
                let extracted_run_instructions = tab_layout
                    .clone()
                    .unwrap_or_else(|| layout.new_tab())
                    .extract_run_instructions();
                let size = Size::default(); // TODO: is this bad?
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
                    tab_name,
                    tab_index,
                    plugin_ids,
                    client_id,
                )));
            },
            PluginInstruction::Exit => break,
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

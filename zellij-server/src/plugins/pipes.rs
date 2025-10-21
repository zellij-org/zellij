use super::{PluginId, PluginInstruction};
use crate::plugins::plugin_map::RunningPlugin;
use crate::plugins::wasm_bridge::PluginRenderAsset;
use crate::plugins::zellij_exports::{wasi_read_string, wasi_write_object};
use std::collections::{HashMap, HashSet};
use zellij_utils::data::{PipeMessage, PipeSource};
use zellij_utils::plugin_api::pipe_message::ProtobufPipeMessage;

use prost::Message;
use zellij_utils::errors::prelude::*;

use crate::{thread_bus::ThreadSenders, ClientId};

#[derive(Debug, Clone)]
pub enum PipeStateChange {
    NoChange,
    Block,
    Unblock,
}

#[derive(Debug, Clone, Default)]
pub struct PendingPipes {
    pipes: HashMap<String, PendingPipeInfo>,
}

impl PendingPipes {
    pub fn mark_being_processed(
        &mut self,
        pipe_id: &str,
        plugin_id: &PluginId,
        client_id: &ClientId,
    ) {
        if self.pipes.contains_key(pipe_id) {
            self.pipes.get_mut(pipe_id).map(|pending_pipe_info| {
                pending_pipe_info.add_processing_plugin(plugin_id, client_id)
            });
        } else {
            self.pipes.insert(
                pipe_id.to_owned(),
                PendingPipeInfo::new(plugin_id, client_id),
            );
        }
    }
    // returns a list of pipes that are no longer pending and should be unblocked
    pub fn update_pipe_state_change(
        &mut self,
        cli_pipe_name: &str,
        pipe_state_change: PipeStateChange,
        plugin_id: &PluginId,
        client_id: &ClientId,
    ) -> Vec<String> {
        let mut pipe_names_to_unblock = vec![];
        match self.pipes.get_mut(cli_pipe_name) {
            Some(pending_pipe_info) => {
                let should_unblock_this_pipe =
                    pending_pipe_info.update_state_change(pipe_state_change, plugin_id, client_id);
                if should_unblock_this_pipe {
                    pipe_names_to_unblock.push(cli_pipe_name.to_owned());
                }
            },
            None => {
                // state somehow corrupted, let's recover...
                pipe_names_to_unblock.push(cli_pipe_name.to_owned());
            },
        }
        for pipe_name in &pipe_names_to_unblock {
            self.pipes.remove(pipe_name);
        }
        pipe_names_to_unblock
    }
    // returns a list of pipes that are no longer pending and should be unblocked
    pub fn unload_plugin(&mut self, plugin_id: &PluginId) -> Vec<String> {
        let mut pipe_names_to_unblock = vec![];
        for (pipe_name, pending_pipe_info) in self.pipes.iter_mut() {
            let should_unblock_this_pipe = pending_pipe_info.unload_plugin(plugin_id);
            if should_unblock_this_pipe {
                pipe_names_to_unblock.push(pipe_name.to_owned());
            }
        }
        for pipe_name in &pipe_names_to_unblock {
            self.pipes.remove(pipe_name);
        }
        pipe_names_to_unblock
    }
}

#[derive(Debug, Clone, Default)]
pub struct PendingPipeInfo {
    is_explicitly_blocked: bool,
    currently_being_processed_by: HashSet<(PluginId, ClientId)>,
}

impl PendingPipeInfo {
    pub fn new(plugin_id: &PluginId, client_id: &ClientId) -> Self {
        let mut currently_being_processed_by = HashSet::new();
        currently_being_processed_by.insert((*plugin_id, *client_id));
        PendingPipeInfo {
            currently_being_processed_by,
            ..Default::default()
        }
    }
    pub fn add_processing_plugin(&mut self, plugin_id: &PluginId, client_id: &ClientId) {
        self.currently_being_processed_by
            .insert((*plugin_id, *client_id));
    }
    // returns true if this pipe should be unblocked
    pub fn update_state_change(
        &mut self,
        pipe_state_change: PipeStateChange,
        plugin_id: &PluginId,
        client_id: &ClientId,
    ) -> bool {
        match pipe_state_change {
            PipeStateChange::Block => {
                self.is_explicitly_blocked = true;
            },
            PipeStateChange::Unblock => {
                self.is_explicitly_blocked = false;
            },
            _ => {},
        };
        self.currently_being_processed_by
            .remove(&(*plugin_id, *client_id));
        let pipe_should_be_unblocked =
            self.currently_being_processed_by.is_empty() && !self.is_explicitly_blocked;
        pipe_should_be_unblocked
    }
    // returns true if this pipe should be unblocked
    pub fn unload_plugin(&mut self, plugin_id_to_unload: &PluginId) -> bool {
        self.currently_being_processed_by
            .retain(|(plugin_id, _)| plugin_id != plugin_id_to_unload);
        if self.currently_being_processed_by.is_empty() && !self.is_explicitly_blocked {
            true
        } else {
            false
        }
    }
}

pub fn apply_pipe_message_to_plugin(
    plugin_id: PluginId,
    client_id: ClientId,
    running_plugin: &mut RunningPlugin,
    pipe_message: &PipeMessage,
    plugin_render_assets: &mut Vec<PluginRenderAsset>,
    senders: &ThreadSenders,
) -> Result<()> {
    let instance = &running_plugin.instance;
    let rows = running_plugin.rows;
    let columns = running_plugin.columns;

    let err_context = || format!("Failed to apply event to plugin {plugin_id}");
    let protobuf_pipe_message: ProtobufPipeMessage = pipe_message
        .clone()
        .try_into()
        .map_err(|e| anyhow!("Failed to convert to protobuf: {:?}", e))?;
    match instance.get_typed_func::<(), i32>(&mut running_plugin.store, "pipe") {
        Ok(pipe) => {
            wasi_write_object(
                running_plugin.store.data(),
                &protobuf_pipe_message.encode_to_vec(),
            )
            .with_context(err_context)?;
            let should_render = pipe
                .call(&mut running_plugin.store, ())
                .with_context(err_context)?;
            let should_render = should_render == 1;
            if rows > 0 && columns > 0 && should_render {
                let rendered_bytes = instance
                    .get_typed_func::<(i32, i32), ()>(&mut running_plugin.store, "render")
                    .and_then(|render| {
                        render.call(&mut running_plugin.store, (rows as i32, columns as i32))
                    })
                    .map_err(|e| anyhow!(e))
                    .and_then(|_| {
                        wasi_read_string(running_plugin.store.data()).map_err(|e| anyhow!(e))
                    })
                    .with_context(err_context)?;
                let pipes_to_block_or_unblock =
                    pipes_to_block_or_unblock(running_plugin, Some(&pipe_message.source));
                let plugin_render_asset = PluginRenderAsset::new(
                    plugin_id,
                    client_id,
                    rendered_bytes.as_bytes().to_vec(),
                )
                .with_pipes(pipes_to_block_or_unblock);
                plugin_render_assets.push(plugin_render_asset);
            } else {
                let pipes_to_block_or_unblock =
                    pipes_to_block_or_unblock(running_plugin, Some(&pipe_message.source));
                let plugin_render_asset = PluginRenderAsset::new(plugin_id, client_id, vec![])
                    .with_pipes(pipes_to_block_or_unblock);
                let _ = senders
                    .send_to_plugin(PluginInstruction::UnblockCliPipes(vec![
                        plugin_render_asset,
                    ]))
                    .context("failed to unblock input pipe");
            }
        },
        Err(_e) => {
            // no-op, this is probably an old plugin that does not have this interface
            // we don't log this error because if we do the logs will be super crowded
            let pipes_to_block_or_unblock =
                pipes_to_block_or_unblock(running_plugin, Some(&pipe_message.source));
            let plugin_render_asset = PluginRenderAsset::new(
                plugin_id,
                client_id,
                vec![], // nothing to render
            )
            .with_pipes(pipes_to_block_or_unblock);
            let _ = senders
                .send_to_plugin(PluginInstruction::UnblockCliPipes(vec![
                    plugin_render_asset,
                ]))
                .context("failed to unblock input pipe");
        },
    }
    Ok(())
}

pub fn pipes_to_block_or_unblock(
    running_plugin: &mut RunningPlugin,
    current_pipe: Option<&PipeSource>,
) -> HashMap<String, PipeStateChange> {
    let mut pipe_state_changes = HashMap::new();
    let mut input_pipes_to_unblock: HashSet<String> = running_plugin
        .store
        .data()
        .input_pipes_to_unblock
        .lock()
        .unwrap()
        .drain()
        .collect();
    let mut input_pipes_to_block: HashSet<String> = running_plugin
        .store
        .data()
        .input_pipes_to_block
        .lock()
        .unwrap()
        .drain()
        .collect();
    if let Some(PipeSource::Cli(current_pipe)) = current_pipe {
        pipe_state_changes.insert(current_pipe.to_owned(), PipeStateChange::NoChange);
    }
    for pipe in input_pipes_to_block.drain() {
        pipe_state_changes.insert(pipe, PipeStateChange::Block);
    }
    for pipe in input_pipes_to_unblock.drain() {
        // unblock has priority over block if they happened simultaneously
        pipe_state_changes.insert(pipe, PipeStateChange::Unblock);
    }
    pipe_state_changes
}

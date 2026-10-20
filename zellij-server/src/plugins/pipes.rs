use super::{PluginId, PluginInstruction};
use crate::plugins::plugin_map::RunningPlugin;
use crate::plugins::wasm_bridge::{apply_pending_popup_size, PluginRenderAsset};
use crate::plugins::zellij_exports::{wasi_read_string, wasi_write_object};
use std::collections::{HashMap, HashSet};
use zellij_utils::data::{PipeMessage, PipeSource};
use zellij_utils::plugin_api::pipe_message::ProtobufPipeMessage;

use prost::Message;
use zellij_utils::errors::prelude::*;

use crate::{thread_bus::ThreadSenders, ClientId};

#[derive(Debug, Clone, PartialEq)]
pub enum PipeStateChange {
    NoChange,
    Block,
    Unblock,
    Crashed,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PipeRelease {
    pub pipe_id: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Default)]
pub struct PendingPipes {
    pipes: HashMap<String, PendingPipeInfo>,
    exit_codes: HashMap<String, (i32, PluginId)>,
    finished: HashSet<String>,
}

impl PendingPipes {
    pub fn set_exit_code(&mut self, pipe_id: &str, exit_code: i32, plugin_id: PluginId) {
        if self.finished.contains(pipe_id) {
            return;
        }
        self.exit_codes
            .insert(pipe_id.to_owned(), (exit_code, plugin_id));
    }
    pub fn is_finished(&self, pipe_id: &str) -> bool {
        self.finished.contains(pipe_id)
    }
    pub fn is_pending(&self, pipe_id: &str) -> bool {
        self.pipes.contains_key(pipe_id)
    }
    fn release(&mut self, pipe_id: String, is_final: bool) -> PipeRelease {
        let exit_code = if is_final {
            self.exit_codes
                .remove(&pipe_id)
                .map(|(exit_code, _)| exit_code)
        } else {
            None
        };
        if exit_code.is_some() {
            self.pipes.remove(&pipe_id);
            self.finished.insert(pipe_id.clone());
        }
        PipeRelease { pipe_id, exit_code }
    }
    pub fn release_with_code(&mut self, pipe_id: &str, exit_code: i32) -> PipeRelease {
        self.exit_codes.remove(pipe_id);
        self.pipes.remove(pipe_id);
        self.finished.insert(pipe_id.to_owned());
        PipeRelease {
            pipe_id: pipe_id.to_owned(),
            exit_code: Some(exit_code),
        }
    }
    pub fn update_pipe_state_change(
        &mut self,
        cli_pipe_name: &str,
        pipe_state_change: PipeStateChange,
        plugin_id: &PluginId,
        client_id: &ClientId,
    ) -> Vec<PipeRelease> {
        if self.finished.contains(cli_pipe_name) {
            return vec![];
        }
        let is_final = matches!(
            pipe_state_change,
            PipeStateChange::Unblock | PipeStateChange::Crashed
        );
        let has_exit_code = self.exit_codes.contains_key(cli_pipe_name);
        if pipe_state_change == PipeStateChange::Crashed && has_exit_code {
            return vec![self.release(cli_pipe_name.to_owned(), true)];
        }
        self.legacy_update_pipe_state_change(cli_pipe_name, pipe_state_change, plugin_id, client_id)
            .into_iter()
            .map(|pipe_name| self.release(pipe_name, is_final))
            .collect()
    }
    pub fn unload_plugin(&mut self, plugin_id: &PluginId) -> Vec<PipeRelease> {
        let owned_pipes: Vec<String> = self
            .exit_codes
            .iter()
            .filter(|(_, (_, owner))| owner == plugin_id)
            .map(|(pipe_id, _)| pipe_id.clone())
            .collect();
        let mut releases: Vec<PipeRelease> = owned_pipes
            .into_iter()
            .map(|pipe_id| self.release(pipe_id, true))
            .collect();
        for pipe_name in self.legacy_unload_plugin(plugin_id) {
            releases.push(self.release(pipe_name, false));
        }
        releases
    }
    pub fn unload_plugin_client(
        &mut self,
        plugin_id: &PluginId,
        client_id: &ClientId,
    ) -> Vec<PipeRelease> {
        self.legacy_unload_plugin_client(plugin_id, client_id)
            .into_iter()
            .map(|pipe_name| self.release(pipe_name, false))
            .collect()
    }
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
    fn legacy_update_pipe_state_change(
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
    fn legacy_unload_plugin(&mut self, plugin_id: &PluginId) -> Vec<String> {
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
    fn legacy_unload_plugin_client(
        &mut self,
        plugin_id: &PluginId,
        client_id: &ClientId,
    ) -> Vec<String> {
        let mut pipe_names_to_unblock = vec![];
        for (pipe_name, pending_pipe_info) in self.pipes.iter_mut() {
            let should_unblock_this_pipe =
                pending_pipe_info.unload_plugin_client(plugin_id, client_id);
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
    blocked_by: Option<PluginId>,
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
                self.blocked_by = Some(*plugin_id);
            },
            PipeStateChange::Unblock => {
                self.is_explicitly_blocked = false;
                self.blocked_by = None;
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
        if self.blocked_by == Some(*plugin_id_to_unload) {
            self.is_explicitly_blocked = false;
            self.blocked_by = None;
        }
        if self.currently_being_processed_by.is_empty() && !self.is_explicitly_blocked {
            true
        } else {
            false
        }
    }
    pub fn unload_plugin_client(
        &mut self,
        plugin_id_to_unload: &PluginId,
        client_id_to_unload: &ClientId,
    ) -> bool {
        self.currently_being_processed_by
            .retain(|(plugin_id, client_id)| {
                plugin_id != plugin_id_to_unload || client_id != client_id_to_unload
            });
        self.currently_being_processed_by.is_empty() && !self.is_explicitly_blocked
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
    let result = apply_pipe_message_to_plugin_inner(
        plugin_id,
        client_id,
        running_plugin,
        pipe_message,
        plugin_render_assets,
        senders,
    );
    if result.is_err() {
        release_pipe_of_crashed_plugin(plugin_id, client_id, pipe_message, senders);
    }
    result
}

fn release_pipe_of_crashed_plugin(
    plugin_id: PluginId,
    client_id: ClientId,
    pipe_message: &PipeMessage,
    senders: &ThreadSenders,
) {
    if let PipeSource::Cli(pipe_id) = &pipe_message.source {
        let mut pipe_state_changes = HashMap::new();
        pipe_state_changes.insert(pipe_id.to_owned(), PipeStateChange::Crashed);
        let plugin_render_asset =
            PluginRenderAsset::new(plugin_id, client_id, vec![]).with_pipes(pipe_state_changes);
        let _ = senders
            .send_to_plugin(PluginInstruction::UnblockCliPipes(vec![
                plugin_render_asset,
            ]))
            .context("failed to unblock input pipe of crashed plugin");
    }
}

fn apply_pipe_message_to_plugin_inner(
    plugin_id: PluginId,
    client_id: ClientId,
    running_plugin: &mut RunningPlugin,
    pipe_message: &PipeMessage,
    plugin_render_assets: &mut Vec<PluginRenderAsset>,
    senders: &ThreadSenders,
) -> Result<()> {
    let instance = running_plugin.instance.clone();

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
            let mut should_render = should_render == 1;
            if apply_pending_popup_size(running_plugin) {
                should_render = true;
            }
            let rows = running_plugin.rows;
            let columns = running_plugin.columns;
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
                .with_pipes(pipes_to_block_or_unblock)
                .rendered_at(rows, columns);
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

#[cfg(test)]
mod tests {
    use super::*;

    const PLUGIN: PluginId = 7;
    const CLIENT: ClientId = 1;

    fn pending_pipe(pipes: &mut PendingPipes, pipe_id: &str) {
        pipes.mark_being_processed(pipe_id, &PLUGIN, &CLIENT);
    }

    #[test]
    fn a_release_without_a_stored_exit_code_carries_none() {
        let mut pipes = PendingPipes::default();
        pending_pipe(&mut pipes, "p");
        let releases =
            pipes.update_pipe_state_change("p", PipeStateChange::NoChange, &PLUGIN, &CLIENT);
        assert_eq!(
            releases,
            vec![PipeRelease {
                pipe_id: "p".to_owned(),
                exit_code: None
            }]
        );
        assert!(!pipes.is_finished("p"));
    }

    #[test]
    fn the_stored_exit_code_is_sent_when_the_plugin_unblocks_the_pipe() {
        let mut pipes = PendingPipes::default();
        pending_pipe(&mut pipes, "p");
        pipes.update_pipe_state_change("p", PipeStateChange::Block, &PLUGIN, &CLIENT);
        pipes.set_exit_code("p", 0, PLUGIN);
        let releases =
            pipes.update_pipe_state_change("p", PipeStateChange::Unblock, &PLUGIN, &CLIENT);
        assert_eq!(
            releases,
            vec![PipeRelease {
                pipe_id: "p".to_owned(),
                exit_code: Some(0)
            }]
        );
        assert!(pipes.is_finished("p"));
    }

    #[test]
    fn an_automatic_release_after_a_message_is_not_final_even_with_an_exit_code() {
        let mut pipes = PendingPipes::default();
        pending_pipe(&mut pipes, "p");
        pipes.set_exit_code("p", 1, PLUGIN);
        let releases =
            pipes.update_pipe_state_change("p", PipeStateChange::NoChange, &PLUGIN, &CLIENT);
        assert_eq!(releases[0].exit_code, None);
        assert!(!pipes.is_finished("p"));
    }

    #[test]
    fn an_explicit_unblock_while_no_message_is_pending_still_releases_with_the_code() {
        let mut pipes = PendingPipes::default();
        pipes.set_exit_code("p", 0, PLUGIN);
        let releases =
            pipes.update_pipe_state_change("p", PipeStateChange::Unblock, &PLUGIN, &CLIENT);
        assert_eq!(releases[0].exit_code, Some(0));
    }

    #[test]
    fn the_stored_exit_code_is_sent_when_the_plugin_closes() {
        let mut pipes = PendingPipes::default();
        pending_pipe(&mut pipes, "p");
        pipes.update_pipe_state_change("p", PipeStateChange::Block, &PLUGIN, &CLIENT);
        pipes.set_exit_code("p", 1, PLUGIN);
        let releases = pipes.unload_plugin(&PLUGIN);
        assert_eq!(
            releases,
            vec![PipeRelease {
                pipe_id: "p".to_owned(),
                exit_code: Some(1)
            }]
        );
    }

    #[test]
    fn the_stored_exit_code_is_sent_when_the_plugin_closes_between_messages() {
        let mut pipes = PendingPipes::default();
        pipes.set_exit_code("p", 1, PLUGIN);
        let releases = pipes.unload_plugin(&PLUGIN);
        assert_eq!(releases[0].exit_code, Some(1));
        assert!(pipes.is_finished("p"));
    }

    #[test]
    fn the_stored_exit_code_is_sent_when_the_plugin_crashes() {
        let mut pipes = PendingPipes::default();
        pending_pipe(&mut pipes, "p");
        pipes.update_pipe_state_change("p", PipeStateChange::Block, &PLUGIN, &CLIENT);
        pipes.set_exit_code("p", 1, PLUGIN);
        pending_pipe(&mut pipes, "p");
        let releases =
            pipes.update_pipe_state_change("p", PipeStateChange::Crashed, &PLUGIN, &CLIENT);
        assert_eq!(releases[0].exit_code, Some(1));
    }

    #[test]
    fn a_closing_plugin_releases_a_pipe_it_blocked_without_an_exit_code() {
        let mut pipes = PendingPipes::default();
        pending_pipe(&mut pipes, "p");
        pipes.update_pipe_state_change("p", PipeStateChange::Block, &PLUGIN, &CLIENT);
        let releases = pipes.unload_plugin(&PLUGIN);
        assert_eq!(
            releases,
            vec![PipeRelease {
                pipe_id: "p".to_owned(),
                exit_code: None
            }]
        );
    }

    #[test]
    fn a_finished_pipe_ignores_later_changes_and_exit_codes() {
        let mut pipes = PendingPipes::default();
        pipes.set_exit_code("p", 0, PLUGIN);
        pipes.update_pipe_state_change("p", PipeStateChange::Unblock, &PLUGIN, &CLIENT);
        pipes.set_exit_code("p", 1, PLUGIN);
        assert!(pipes
            .update_pipe_state_change("p", PipeStateChange::Unblock, &PLUGIN, &CLIENT)
            .is_empty());
        assert!(pipes.unload_plugin(&PLUGIN).is_empty());
    }
}

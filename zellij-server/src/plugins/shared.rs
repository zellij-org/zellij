use super::PluginId;
use crate::background_jobs::BackgroundJob;
use crate::plugins::pipes::{pipes_to_block_or_unblock, PipeStateChange};
use crate::plugins::plugin_loader::PluginLoader;
use crate::plugins::plugin_map::{PluginMap, RunningPlugin, SharedSlot, Subscriptions};
use crate::plugins::plugin_worker::MessageToWorker;
use crate::plugins::wasm_bridge::{
    check_event_permission, handle_plugin_crash, handle_plugin_loading_failure,
    handle_plugin_successful_loading, LoadingContext, PluginCache, PluginRenderAsset,
};
use crate::plugins::zellij_exports::{wasi_read_bytes, wasi_read_string, wasi_write_object};
use crate::plugins::PluginInstruction;
use crate::screen::ScreenInstruction;
use crate::ui::loading_indication::LoadingIndication;
use crate::{thread_bus::ThreadSenders, ClientId, SharedKeybinds};
use prost::Message;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use wasmi::Engine;
use zellij_utils::data::KeybindsVec;
use zellij_utils::data::{
    Event, EventContext, EventType, PermissionStatus, PipeMessage, PipeSource, Render, Slot,
};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::command::TerminalAction;
use zellij_utils::input::layout::RunPluginLocation;
use zellij_utils::plugin_api::event::{event_to_protobuf_with_keybinds, ProtobufEvent};
use zellij_utils::plugin_api::pipe_message::ProtobufPipeMessage;
use zellij_utils::plugin_api::shared_plugin::{
    ProtobufEventContext, ProtobufRenderRequest, ProtobufSlot,
};

pub const SHARED_PIPE_CLIENT: ClientId = ClientId::MAX;
pub const SHARED_MARKER_EXPORT: &str = "zellij_shared_plugin";

pub type SharedKey = (RunPluginLocation, Option<String>);

pub type SharedEventBatchEntry = (Event, EventContext, Option<SharedKeybinds>);

fn read_leb_u32(bytes: &[u8], position: &mut usize) -> Option<u32> {
    let mut result: u32 = 0;
    let mut shift = 0;
    loop {
        let byte = *bytes.get(*position)?;
        *position += 1;
        result |= ((byte & 0x7f) as u32).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift > 28 {
            return None;
        }
    }
}

pub fn wasm_module_exports_function(bytes: &[u8], name: &str) -> bool {
    if bytes.len() < 8 || &bytes[0..4] != b"\0asm" {
        return false;
    }
    let mut position = 8;
    while position < bytes.len() {
        let section_id = bytes[position];
        position += 1;
        let Some(section_size) = read_leb_u32(bytes, &mut position) else {
            return false;
        };
        let section_end = position + section_size as usize;
        if section_end > bytes.len() {
            return false;
        }
        if section_id == 7 {
            let mut cursor = position;
            let Some(count) = read_leb_u32(bytes, &mut cursor) else {
                return false;
            };
            for _ in 0..count {
                let Some(name_len) = read_leb_u32(bytes, &mut cursor) else {
                    return false;
                };
                let name_end = cursor + name_len as usize;
                if name_end > section_end {
                    return false;
                }
                let export_name = &bytes[cursor..name_end];
                cursor = name_end;
                let Some(kind) = bytes.get(cursor).copied() else {
                    return false;
                };
                cursor += 1;
                if read_leb_u32(bytes, &mut cursor).is_none() {
                    return false;
                }
                if kind == 0 && export_name == name.as_bytes() {
                    return true;
                }
            }
            return false;
        }
        position = section_end;
    }
    false
}

fn set_call_context(
    running_plugin: &mut RunningPlugin,
    slot_id: Option<PluginId>,
    client_id: Option<ClientId>,
    self_commands_allowed: bool,
) {
    if let Some(shared) = running_plugin.store.data_mut().shared.as_mut() {
        shared.current_slot = slot_id;
        shared.current_client = client_id;
        shared.self_commands_allowed = self_commands_allowed;
    }
}

fn clear_call_context(running_plugin: &mut RunningPlugin) {
    set_call_context(running_plugin, None, None, false);
}

fn call_with_i32(running_plugin: &mut RunningPlugin, name: &str, value: i32) -> Result<()> {
    let instance = running_plugin.instance.clone();
    instance
        .get_typed_func::<i32, ()>(&mut running_plugin.store, name)
        .and_then(|function| function.call(&mut running_plugin.store, value))
        .map_err(|e| anyhow!(e))
        .with_context(|| format!("failed to call {} on shared plugin", name))
}

fn call_without_args(running_plugin: &mut RunningPlugin, name: &str) -> Result<()> {
    let instance = running_plugin.instance.clone();
    instance
        .get_typed_func::<(), ()>(&mut running_plugin.store, name)
        .and_then(|function| function.call(&mut running_plugin.store, ()))
        .map_err(|e| anyhow!(e))
        .with_context(|| format!("failed to call {} on shared plugin", name))
}

fn call_returning_i32(running_plugin: &mut RunningPlugin, name: &str) -> Result<i32> {
    let instance = running_plugin.instance.clone();
    instance
        .get_typed_func::<(), i32>(&mut running_plugin.store, name)
        .and_then(|function| function.call(&mut running_plugin.store, ()))
        .map_err(|e| anyhow!(e))
        .with_context(|| format!("failed to call {} on shared plugin", name))
}

pub fn pane_slot_ids(running_plugin: &RunningPlugin) -> Vec<PluginId> {
    running_plugin
        .store
        .data()
        .shared
        .as_ref()
        .map(|shared| shared.pane_slot_ids())
        .unwrap_or_default()
}

pub fn call_client_connected(running_plugin: &mut RunningPlugin, client_id: ClientId) -> Result<()> {
    let is_new = running_plugin
        .store
        .data_mut()
        .shared
        .as_mut()
        .map(|shared| shared.clients.insert(client_id))
        .unwrap_or(false);
    if !is_new {
        return Ok(());
    }
    set_call_context(running_plugin, None, Some(client_id), false);
    let result = call_with_i32(running_plugin, "client_connected", client_id as i32);
    clear_call_context(running_plugin);
    result
}

pub fn call_client_disconnected(
    running_plugin: &mut RunningPlugin,
    client_id: ClientId,
) -> Result<()> {
    let was_connected = running_plugin
        .store
        .data_mut()
        .shared
        .as_mut()
        .map(|shared| {
            shared.visible_slots.remove(&client_id);
            shared
                .last_events
                .retain(|(_, event_client), _| *event_client != Some(client_id));
            shared.clients.remove(&client_id)
        })
        .unwrap_or(false);
    if !was_connected {
        return Ok(());
    }
    call_with_i32(running_plugin, "client_disconnected", client_id as i32)
}

pub fn call_slot_added(
    running_plugin: &mut RunningPlugin,
    slot_id: PluginId,
    slot: SharedSlot,
) -> Result<()> {
    if let Some(shared) = running_plugin.store.data_mut().shared.as_mut() {
        shared.slots.insert(slot_id, slot.clone());
    }
    let protobuf_slot: ProtobufSlot = Slot {
        id: slot_id,
        kind: slot.kind,
        configuration: slot.configuration,
    }
    .into();
    wasi_write_object(running_plugin.store.data(), &protobuf_slot.encode_to_vec())?;
    set_call_context(running_plugin, Some(slot_id), None, true);
    let result = call_without_args(running_plugin, "slot_added");
    clear_call_context(running_plugin);
    result
}

pub fn call_slot_removed(running_plugin: &mut RunningPlugin, slot_id: PluginId) -> Result<bool> {
    let slots_left = match running_plugin.store.data_mut().shared.as_mut() {
        Some(shared) => {
            if shared.slots.remove(&slot_id).is_none() {
                return Ok(!shared.slots.is_empty());
            }
            for visible in shared.visible_slots.values_mut() {
                visible.remove(&slot_id);
            }
            !shared.slots.is_empty()
        },
        None => false,
    };
    call_with_i32(running_plugin, "slot_removed", slot_id as i32)?;
    Ok(slots_left)
}

fn read_render_request(running_plugin: &RunningPlugin) -> Render {
    match wasi_read_bytes(running_plugin.store.data()).and_then(|bytes| {
        ProtobufRenderRequest::decode(bytes.as_slice()).map_err(|e| anyhow!(e))
    }) {
        Ok(request) => Render::from(request),
        Err(e) => {
            log::error!("Failed to read render request from shared plugin: {:?}", e);
            Render::All
        },
    }
}

fn render_from_return_value(running_plugin: &RunningPlugin, value: i32) -> Render {
    match value {
        0 => Render::Nothing,
        1 => Render::All,
        _ => read_render_request(running_plugin),
    }
}

pub fn call_update(
    running_plugin: &mut RunningPlugin,
    event: &Event,
    keybinds: Option<&KeybindsVec>,
    context: EventContext,
) -> Result<Render> {
    let protobuf_context: ProtobufEventContext = context.into();
    let protobuf_event: ProtobufEvent = event_to_protobuf_with_keybinds(event.clone(), keybinds)
        .map_err(|e| anyhow!("Failed to convert to protobuf: {:?}", e))?;
    wasi_write_object(running_plugin.store.data(), &protobuf_context.encode_to_vec())?;
    wasi_write_object(running_plugin.store.data(), &protobuf_event.encode_to_vec())?;
    set_call_context(running_plugin, context.slot_id, context.client_id, false);
    let result = call_returning_i32(running_plugin, "shared_update");
    clear_call_context(running_plugin);
    Ok(render_from_return_value(running_plugin, result?))
}

pub fn call_pipe(
    running_plugin: &mut RunningPlugin,
    pipe_message: &PipeMessage,
    context: EventContext,
) -> Result<Render> {
    let protobuf_context: ProtobufEventContext = context.into();
    let protobuf_pipe_message: ProtobufPipeMessage = pipe_message
        .clone()
        .try_into()
        .map_err(|e| anyhow!("Failed to convert to protobuf: {:?}", e))?;
    wasi_write_object(running_plugin.store.data(), &protobuf_context.encode_to_vec())?;
    wasi_write_object(
        running_plugin.store.data(),
        &protobuf_pipe_message.encode_to_vec(),
    )?;
    set_call_context(running_plugin, context.slot_id, context.client_id, false);
    let result = call_returning_i32(running_plugin, "shared_pipe");
    clear_call_context(running_plugin);
    Ok(render_from_return_value(running_plugin, result?))
}

pub fn render_targets(
    running_plugin: &mut RunningPlugin,
    targets: &[(PluginId, ClientId)],
) -> Result<Vec<PluginRenderAsset>> {
    let mut assets = vec![];
    for (slot_id, client_id) in targets {
        let size = running_plugin
            .store
            .data()
            .shared
            .as_ref()
            .and_then(|shared| shared.renderable_slot(*slot_id));
        let Some((rows, columns)) = size else {
            continue;
        };
        set_call_context(running_plugin, Some(*slot_id), Some(*client_id), true);
        let instance = running_plugin.instance.clone();
        let result = instance
            .get_typed_func::<(i32, i32, i32, i32), ()>(&mut running_plugin.store, "shared_render")
            .and_then(|render| {
                render.call(
                    &mut running_plugin.store,
                    (
                        rows as i32,
                        columns as i32,
                        *slot_id as i32,
                        *client_id as i32,
                    ),
                )
            })
            .map_err(|e| anyhow!(e));
        clear_call_context(running_plugin);
        result.with_context(|| format!("failed to render shared plugin slot {}", slot_id))?;
        let rendered = wasi_read_string(running_plugin.store.data())?;
        assets.push(PluginRenderAsset::new(
            *slot_id,
            *client_id,
            rendered.into_bytes(),
        ));
    }
    Ok(assets)
}

fn targets_for(running_plugin: &RunningPlugin, render: &Render) -> Vec<(PluginId, ClientId)> {
    running_plugin
        .store
        .data()
        .shared
        .as_ref()
        .map(|shared| shared.render_targets(render))
        .unwrap_or_default()
}

fn render_and_send(
    senders: &ThreadSenders,
    running_plugin: &mut RunningPlugin,
    targets: Vec<(PluginId, ClientId)>,
) -> Result<()> {
    if targets.is_empty() {
        return Ok(());
    }
    let assets = render_targets(running_plugin, &targets)?;
    if !assets.is_empty() {
        let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(assets));
    }
    Ok(())
}

fn render_request(
    senders: &ThreadSenders,
    running_plugin: &mut RunningPlugin,
    render: Render,
) -> Result<()> {
    let targets = targets_for(running_plugin, &render);
    render_and_send(senders, running_plugin, targets)
}

fn report_error(senders: &ThreadSenders, pane_slots: Vec<PluginId>, error: anyhow::Error) {
    log::error!("{:?}", error);
    let message = format!("{:?}", error).replace("\n", "\n\r");
    for slot_id in pane_slots {
        handle_plugin_crash(slot_id, message.clone(), senders.clone());
    }
}

fn send_pipe_changes(
    senders: &ThreadSenders,
    running_plugin: &mut RunningPlugin,
    instance_id: PluginId,
    current_pipe: Option<&PipeSource>,
) {
    let changes = pipes_to_block_or_unblock(running_plugin, current_pipe);
    if !changes.is_empty() {
        let asset = PluginRenderAsset::new(instance_id, SHARED_PIPE_CLIENT, vec![]).with_pipes(changes);
        let _ = senders.send_to_plugin(PluginInstruction::UnblockCliPipes(vec![asset]));
    }
}

fn release_pipe(senders: &ThreadSenders, instance_id: PluginId, pipe_message: &PipeMessage) {
    if let PipeSource::Cli(pipe_id) = &pipe_message.source {
        let mut changes = HashMap::new();
        changes.insert(pipe_id.to_owned(), PipeStateChange::NoChange);
        let asset = PluginRenderAsset::new(instance_id, SHARED_PIPE_CLIENT, vec![]).with_pipes(changes);
        let _ = senders.send_to_plugin(PluginInstruction::UnblockCliPipes(vec![asset]));
    }
}

fn lookup(
    plugin_map: &Arc<Mutex<PluginMap>>,
    instance_id: PluginId,
) -> Option<(Arc<Mutex<RunningPlugin>>, Arc<Mutex<Subscriptions>>)> {
    plugin_map.lock().unwrap().shared_running_plugin(instance_id)
}

fn is_deduplicated_event(event_type: EventType) -> bool {
    matches!(
        event_type,
        EventType::ModeUpdate | EventType::TabUpdate | EventType::PaneUpdate
    )
}

pub fn apply_events_job(
    senders: ThreadSenders,
    plugin_map: Arc<Mutex<PluginMap>>,
    instance_id: PluginId,
    events: Vec<SharedEventBatchEntry>,
) {
    let Some((running_plugin, subscriptions)) = lookup(&plugin_map, instance_id) else {
        return;
    };
    let subscriptions = subscriptions.lock().unwrap().clone();
    let mut running_plugin = running_plugin.lock().unwrap();
    let mut render = Render::Nothing;
    let strip_keybinds = subscriptions.contains(&EventType::InitialKeybinds);
    for (mut event, context, mut keybinds) in events {
        let Ok(event_type) = EventType::from_str(&event.to_string()) else {
            continue;
        };
        if let Event::ModeUpdate(mode_info) = &mut event {
            mode_info.keybinds = vec![];
        }
        if strip_keybinds {
            keybinds = None;
        }
        if !subscriptions.contains(&event_type) && event_type != EventType::PermissionRequestResult
        {
            continue;
        }
        if is_deduplicated_event(event_type) {
            if let Some(shared) = running_plugin.store.data_mut().shared.as_mut() {
                let key = (event_type, context.client_id);
                let already_delivered = shared
                    .last_events
                    .get(&key)
                    .map(|(last_event, last_keybinds)| {
                        last_event == &event && last_keybinds == &keybinds
                    })
                    .unwrap_or(false);
                if already_delivered {
                    continue;
                }
                shared
                    .last_events
                    .insert(key, (event.clone(), keybinds.clone()));
            }
        }
        match check_event_permission(running_plugin.store.data(), &event) {
            (PermissionStatus::Granted, _) => {},
            (PermissionStatus::Denied, permission) => {
                log::error!(
                    "Shared plugin '{}' permission '{}' is not allowed - Event '{:?}' denied",
                    instance_id,
                    permission
                        .map(|p| p.to_string())
                        .unwrap_or("UNKNOWN".to_owned()),
                    event_type
                );
                continue;
            },
        }
        match call_update(&mut running_plugin, &event, keybinds.as_deref(), context) {
            Ok(requested) => {
                render = render.merge(requested);
                if event_type == EventType::PermissionRequestResult {
                    render = render.merge(Render::All);
                }
            },
            Err(e) => {
                let pane_slots = pane_slot_ids(&running_plugin);
                report_error(&senders, pane_slots, e);
                return;
            },
        }
    }
    send_pipe_changes(&senders, &mut running_plugin, instance_id, None);
    if let Err(e) = render_request(&senders, &mut running_plugin, render) {
        let pane_slots = pane_slot_ids(&running_plugin);
        report_error(&senders, pane_slots, e);
    }
}

pub fn apply_pipes_job(
    senders: ThreadSenders,
    plugin_map: Arc<Mutex<PluginMap>>,
    instance_id: PluginId,
    messages: Vec<(PipeMessage, EventContext)>,
) {
    let Some((running_plugin, _subscriptions)) = lookup(&plugin_map, instance_id) else {
        for (pipe_message, _) in &messages {
            release_pipe(&senders, instance_id, pipe_message);
        }
        return;
    };
    let mut running_plugin = running_plugin.lock().unwrap();
    let mut render = Render::Nothing;
    let mut messages = messages.into_iter();
    while let Some((pipe_message, context)) = messages.next() {
        match call_pipe(&mut running_plugin, &pipe_message, context) {
            Ok(requested) => {
                render = render.merge(requested);
                send_pipe_changes(
                    &senders,
                    &mut running_plugin,
                    instance_id,
                    Some(&pipe_message.source),
                );
            },
            Err(e) => {
                release_pipe(&senders, instance_id, &pipe_message);
                for (remaining, _) in messages {
                    release_pipe(&senders, instance_id, &remaining);
                }
                let pane_slots = pane_slot_ids(&running_plugin);
                report_error(&senders, pane_slots, e);
                return;
            },
        }
    }
    if let Err(e) = render_request(&senders, &mut running_plugin, render) {
        let pane_slots = pane_slot_ids(&running_plugin);
        report_error(&senders, pane_slots, e);
    }
}

pub fn add_slot_job(
    senders: ThreadSenders,
    plugin_map: Arc<Mutex<PluginMap>>,
    instance_id: PluginId,
    slot_id: PluginId,
    slot: SharedSlot,
    slot_added: std::sync::mpsc::Sender<()>,
) {
    let Some((running_plugin, _subscriptions)) = lookup(&plugin_map, instance_id) else {
        let mut loading_indication = LoadingIndication::new(String::new());
        handle_plugin_loading_failure(
            &senders,
            slot_id,
            &mut loading_indication,
            "Shared plugin instance failed to load",
            None,
        );
        return;
    };
    let mut running_plugin = running_plugin.lock().unwrap();
    let added = call_slot_added(&mut running_plugin, slot_id, slot);
    let _ = slot_added.send(());
    let result = added.and_then(|_| {
        render_request(&senders, &mut running_plugin, Render::Slots(vec![slot_id]))
    });
    match result {
        Ok(()) => {
            let plugin_list = plugin_map.lock().unwrap().list_plugins();
            handle_plugin_successful_loading(&senders, slot_id, plugin_list);
        },
        Err(e) => {
            let _ = senders.send_to_background_jobs(BackgroundJob::StopPluginLoadingAnimation(slot_id));
            report_error(&senders, vec![slot_id], e);
        },
    }
}

pub fn remove_slot_job(
    senders: ThreadSenders,
    plugin_map: Arc<Mutex<PluginMap>>,
    instance_id: PluginId,
    slot_id: PluginId,
    tear_down: bool,
) {
    if let Some((running_plugin, _subscriptions)) = lookup(&plugin_map, instance_id) {
        let mut running_plugin = running_plugin.lock().unwrap();
        if let Err(e) = call_slot_removed(&mut running_plugin, slot_id) {
            log::error!("Failed to remove slot {} from shared plugin: {:?}", slot_id, e);
        }
    }
    if tear_down {
        tear_down_instance(&senders, &plugin_map, instance_id);
    }
}

fn tear_down_instance(
    senders: &ThreadSenders,
    plugin_map: &Arc<Mutex<PluginMap>>,
    instance_id: PluginId,
) {
    let removed = plugin_map.lock().unwrap().remove_shared(instance_id);
    let Some((running_plugin, subscriptions, workers)) = removed else {
        return;
    };
    for (_worker_name, worker_sender) in workers {
        drop(worker_sender.send(MessageToWorker::Exit));
    }
    let needs_before_close = subscriptions
        .lock()
        .unwrap()
        .contains(&EventType::BeforeClose);
    let mut running_plugin = running_plugin.lock().unwrap();
    if needs_before_close {
        if let Err(e) = call_update(
            &mut running_plugin,
            &Event::BeforeClose,
            None,
            EventContext::default(),
        ) {
            log::error!("Failed to send BeforeClose to shared plugin: {:?}", e);
        }
        send_pipe_changes(senders, &mut running_plugin, instance_id, None);
    }
    if running_plugin.intercepting_key_presses() {
        let clients: Vec<ClientId> = running_plugin
            .store
            .data()
            .shared
            .as_ref()
            .map(|shared| shared.clients.iter().copied().collect())
            .unwrap_or_default();
        for client_id in clients {
            let _ = senders.send_to_screen(ScreenInstruction::ClearKeyPressesIntercepts(client_id));
        }
    }
    let data_dir = running_plugin.store.data().plugin_own_data_dir.clone();
    if let Err(e) = std::fs::remove_dir_all(&data_dir) {
        log::error!("Failed to remove data dir for shared plugin: {:?}", e);
    }
    let _ = senders.send_to_screen(ScreenInstruction::ClearAllPluginHighlights(instance_id));
}

pub fn client_job(
    senders: ThreadSenders,
    plugin_map: Arc<Mutex<PluginMap>>,
    instance_id: PluginId,
    client_id: ClientId,
    connected: bool,
) {
    let Some((running_plugin, _subscriptions)) = lookup(&plugin_map, instance_id) else {
        return;
    };
    let mut running_plugin = running_plugin.lock().unwrap();
    let result = if connected {
        call_client_connected(&mut running_plugin, client_id)
            .and_then(|_| render_request(&senders, &mut running_plugin, Render::Client(client_id)))
    } else {
        call_client_disconnected(&mut running_plugin, client_id)
    };
    if let Err(e) = result {
        let pane_slots = pane_slot_ids(&running_plugin);
        report_error(&senders, pane_slots, e);
    }
}

pub fn resize_job(
    senders: ThreadSenders,
    plugin_map: Arc<Mutex<PluginMap>>,
    instance_id: PluginId,
    slot_id: PluginId,
    rows: usize,
    columns: usize,
) {
    let Some((running_plugin, _subscriptions)) = lookup(&plugin_map, instance_id) else {
        return;
    };
    let mut running_plugin = running_plugin.lock().unwrap();
    let changed = match running_plugin.store.data_mut().shared.as_mut() {
        Some(shared) => match shared.slots.get_mut(&slot_id) {
            Some(slot) => {
                let changed = slot.rows != rows || slot.columns != columns;
                slot.rows = rows;
                slot.columns = columns;
                changed
            },
            None => false,
        },
        None => false,
    };
    if changed {
        if let Err(e) = render_request(&senders, &mut running_plugin, Render::Slots(vec![slot_id]))
        {
            let pane_slots = pane_slot_ids(&running_plugin);
            report_error(&senders, pane_slots, e);
        }
    }
}

pub fn visibility_job(
    senders: ThreadSenders,
    plugin_map: Arc<Mutex<PluginMap>>,
    instance_id: PluginId,
    visible_slots: HashMap<ClientId, HashSet<PluginId>>,
) {
    let Some((running_plugin, _subscriptions)) = lookup(&plugin_map, instance_id) else {
        return;
    };
    let mut running_plugin = running_plugin.lock().unwrap();
    let before: HashSet<(PluginId, ClientId)> =
        targets_for(&running_plugin, &Render::All).into_iter().collect();
    if let Some(shared) = running_plugin.store.data_mut().shared.as_mut() {
        shared.visible_slots = visible_slots;
    }
    let newly_visible: Vec<(PluginId, ClientId)> = targets_for(&running_plugin, &Render::All)
        .into_iter()
        .filter(|target| !before.contains(target))
        .collect();
    if let Err(e) = render_and_send(&senders, &mut running_plugin, newly_visible) {
        let pane_slots = pane_slot_ids(&running_plugin);
        report_error(&senders, pane_slots, e);
    }
}

pub fn host_settings_job(
    plugin_map: Arc<Mutex<PluginMap>>,
    instance_id: PluginId,
    default_shell: Option<TerminalAction>,
    layout_dir: Option<PathBuf>,
) {
    let Some((running_plugin, _subscriptions)) = lookup(&plugin_map, instance_id) else {
        return;
    };
    let mut running_plugin = running_plugin.lock().unwrap();
    running_plugin.update_default_shell(default_shell);
    running_plugin.update_layout_dir(layout_dir);
}

pub fn start_instance_job(
    senders: ThreadSenders,
    plugin_map: Arc<Mutex<PluginMap>>,
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
    plugin_cache: PluginCache,
    engine: Engine,
    loading_context: LoadingContext,
    skip_cache: bool,
    instance_id: PluginId,
    initial_slots: BTreeMap<PluginId, SharedSlot>,
) {
    let mut initial_slots = initial_slots;
    let previous = plugin_map.lock().unwrap().remove_shared(instance_id);
    let previous_visible_slots = match previous {
        Some((running_plugin, _subscriptions, workers)) => {
            for (_worker_name, worker_sender) in workers {
                drop(worker_sender.send(MessageToWorker::Exit));
            }
            let running_plugin = running_plugin.lock().unwrap();
            match running_plugin.store.data().shared.as_ref() {
                Some(shared) => {
                    for (slot_id, slot) in shared.slots.iter() {
                        initial_slots
                            .entry(*slot_id)
                            .or_insert_with(|| slot.clone());
                    }
                    shared.visible_slots.clone()
                },
                None => HashMap::new(),
            }
        },
        None => HashMap::new(),
    };
    let started = {
        let mut plugin_map = plugin_map.lock().unwrap();
        PluginLoader::new(
            skip_cache,
            loading_context,
            senders.clone(),
            engine,
            plugin_cache,
            &mut plugin_map,
            connected_clients.clone(),
        )
        .start_shared_plugin()
    };
    let running_plugin = match started {
        Ok(running_plugin) => running_plugin,
        Err(e) => {
            for slot_id in initial_slots.keys() {
                let mut loading_indication = LoadingIndication::new(String::new());
                handle_plugin_loading_failure(
                    &senders,
                    *slot_id,
                    &mut loading_indication,
                    format!("{:?}", e),
                    None,
                );
            }
            return;
        },
    };
    let mut running_plugin = running_plugin.lock().unwrap();
    if let Some(shared) = running_plugin.store.data_mut().shared.as_mut() {
        shared.visible_slots = previous_visible_slots;
    }
    let clients: Vec<ClientId> = connected_clients.lock().unwrap().iter().copied().collect();
    let mut result: Result<()> = Ok(());
    for client_id in clients {
        if result.is_ok() {
            result = call_client_connected(&mut running_plugin, client_id);
        }
    }
    for (slot_id, slot) in initial_slots.iter() {
        if result.is_ok() {
            result = call_slot_added(&mut running_plugin, *slot_id, slot.clone());
        }
    }
    if result.is_ok() {
        result = render_request(&senders, &mut running_plugin, Render::All);
    }
    match result {
        Ok(()) => {
            let plugin_list = plugin_map.lock().unwrap().list_plugins();
            for slot_id in initial_slots.keys() {
                handle_plugin_successful_loading(&senders, *slot_id, plugin_list.clone());
            }
        },
        Err(e) => {
            for slot_id in initial_slots.keys() {
                let _ = senders
                    .send_to_background_jobs(BackgroundJob::StopPluginLoadingAnimation(*slot_id));
            }
            let pane_slots = pane_slot_ids(&running_plugin);
            report_error(&senders, pane_slots, e);
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module_with_exports(names: &[&str]) -> Vec<u8> {
        let mut section = vec![names.len() as u8];
        for name in names {
            section.push(name.len() as u8);
            section.extend_from_slice(name.as_bytes());
            section.push(0);
            section.push(0);
        }
        let mut bytes = b"\0asm\x01\0\0\0".to_vec();
        bytes.push(1);
        bytes.push(0);
        bytes.push(7);
        bytes.push(section.len() as u8);
        bytes.extend(section);
        bytes
    }

    #[test]
    fn detects_shared_marker_export() {
        let bytes = module_with_exports(&["load", SHARED_MARKER_EXPORT]);
        assert!(wasm_module_exports_function(&bytes, SHARED_MARKER_EXPORT));
    }

    #[test]
    fn legacy_module_is_not_shared() {
        let bytes = module_with_exports(&["load", "update", "render"]);
        assert!(!wasm_module_exports_function(&bytes, SHARED_MARKER_EXPORT));
    }

    #[test]
    fn garbage_is_not_shared() {
        assert!(!wasm_module_exports_function(b"not a wasm module", SHARED_MARKER_EXPORT));
    }
}

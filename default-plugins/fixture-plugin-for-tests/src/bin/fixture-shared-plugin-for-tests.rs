use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::prelude::*;

#[allow(dead_code)]
#[derive(Default)]
struct State {
    load_count: usize,
    instance_id: u32,
    configuration: BTreeMap<String, String>,
    slots: BTreeMap<SlotId, Slot>,
    removed_slots: Vec<SlotId>,
    clients: BTreeSet<ClientId>,
    disconnected_clients: Vec<ClientId>,
    key_events: Vec<(Option<SlotId>, Option<ClientId>)>,
    self_command_results: Vec<String>,
}

#[cfg(target_family = "wasm")]
register_shared_plugin!(State);

#[allow(dead_code)]
fn render_line(state: &State, rows: usize, cols: usize, slot_id: SlotId, client_id: ClientId) -> String {
    let slot_ids: Vec<SlotId> = state.slots.keys().copied().collect();
    let clients: Vec<ClientId> = state.clients.iter().copied().collect();
    let label = state
        .slots
        .get(&slot_id)
        .and_then(|slot| slot.configuration.get("label").cloned())
        .unwrap_or_default();
    format!(
        "instance:{} loads:{} slot:{} label:{} client:{} rows:{} cols:{} slots:{:?} removed:{:?} clients:{:?} disconnected:{:?} keys:{:?} self_commands:{:?}",
        state.instance_id,
        state.load_count,
        slot_id,
        label,
        client_id,
        rows,
        cols,
        slot_ids,
        state.removed_slots,
        clients,
        state.disconnected_clients,
        state.key_events,
        state.self_command_results,
    )
}

#[cfg(target_family = "wasm")]
impl ZellijSharedPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.load_count += 1;
        self.configuration = configuration;
        self.instance_id = get_plugin_ids().plugin_id;
        subscribe(&[EventType::Key]);
    }
    fn slot_added(&mut self, slot: Slot) {
        if slot.kind == SlotKind::Pane {
            set_selectable(true);
        }
        self.slots.insert(slot.id, slot);
    }
    fn slot_removed(&mut self, slot_id: SlotId) {
        self.slots.remove(&slot_id);
        self.removed_slots.push(slot_id);
    }
    fn client_connected(&mut self, client_id: ClientId) {
        self.clients.insert(client_id);
    }
    fn client_disconnected(&mut self, client_id: ClientId) {
        self.clients.remove(&client_id);
        self.disconnected_clients.push(client_id);
    }
    fn update(&mut self, event: Event, context: EventContext) -> RenderResponse {
        match event {
            Event::Key(_) => {
                self.key_events.push((context.slot_id, context.client_id));
                match context.client_id {
                    Some(client_id) => RenderResponse::Client(client_id),
                    None => RenderResponse::All,
                }
            },
            _ => RenderResponse::Nothing,
        }
    }
    fn pipe(&mut self, pipe_message: PipeMessage, context: EventContext) -> RenderResponse {
        match pipe_message.name.as_str() {
            "render_all" => RenderResponse::All,
            "render_slot" => match pipe_message.payload.and_then(|p| p.parse::<SlotId>().ok()) {
                Some(slot_id) => RenderResponse::Slots(vec![slot_id]),
                None => RenderResponse::Nothing,
            },
            "selectable_slot" => {
                let results: Vec<String> = self
                    .slots
                    .keys()
                    .copied()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|slot_id| format!("{:?}", set_selectable_slot(slot_id, false)))
                    .collect();
                self.self_command_results.extend(results);
                self.self_command_results
                    .push(format!("{:?}", set_selectable_slot(u32::MAX, false)));
                RenderResponse::All
            },
            "close_first_slot" => {
                if let Some(slot_id) = self.slots.keys().next().copied() {
                    let result = close_slot(slot_id);
                    self.self_command_results.push(format!("{:?}", result));
                }
                RenderResponse::All
            },
            _ => {
                let _ = context;
                RenderResponse::Nothing
            },
        }
    }
    fn render(&mut self, rows: usize, cols: usize, slot_id: SlotId, client_id: ClientId) {
        print!("{}", render_line(self, rows, cols, slot_id, client_id));
    }
}

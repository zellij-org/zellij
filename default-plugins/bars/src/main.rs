mod compact_bar;
mod keybinds;
mod link;
mod session_indicator;
mod status_bar;
mod tab_bar;

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

use compact_bar::CompactBar;
use keybinds::{take_keybinds, KeybindStore};
use link::Link;
use status_bar::StatusBar;
use tab_bar::TabBar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    TabBar,
    StatusBar,
    CompactBar,
    Link,
}

impl Role {
    fn from_configuration(configuration: &BTreeMap<String, String>) -> Role {
        match configuration.get("role").map(|r| r.as_str()) {
            Some("status-bar") => Role::StatusBar,
            Some("compact-bar") => Role::CompactBar,
            Some("link") => Role::Link,
            _ => Role::TabBar,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClientSeed {
    pub mode_info: ModeInfo,
    pub tabs: Vec<TabInfo>,
}

impl ClientSeed {
    fn apply(&mut self, event: &Event) {
        match event {
            Event::ModeUpdate(mode_info) => {
                self.mode_info = mode_info.clone();
            },
            Event::TabUpdate(tabs) => {
                if tabs.iter().any(|t| t.active) {
                    self.tabs = tabs.clone();
                }
            },
            _ => {},
        }
    }
}

#[derive(Default)]
struct State {
    slots: BTreeMap<SlotId, Role>,
    pending: BTreeMap<ClientId, ClientSeed>,
    tab_bar: TabBar,
    status_bar: StatusBar,
    compact_bar: CompactBar,
    link: Link,
    keybinds: KeybindStore,
}

register_shared_plugin!(State);

impl State {
    fn role_has_slots(&self, role: Role) -> bool {
        match role {
            Role::TabBar => self.tab_bar.has_slots(),
            Role::StatusBar => self.status_bar.has_slots(),
            Role::CompactBar => self.compact_bar.has_slots(),
            Role::Link => self.link.has_slots(),
        }
    }

    fn any_bar_has_slots(&self) -> bool {
        self.tab_bar.has_slots() || self.status_bar.has_slots() || self.compact_bar.has_slots()
    }

    fn snapshot_for(&self, role: Role) -> BTreeMap<ClientId, ClientSeed> {
        if role != Role::TabBar && self.tab_bar.has_slots() {
            self.tab_bar.snapshot()
        } else if role != Role::StatusBar && self.status_bar.has_slots() {
            self.status_bar.snapshot()
        } else if role != Role::CompactBar && self.compact_bar.has_slots() {
            self.compact_bar.snapshot()
        } else {
            self.pending.clone()
        }
    }

    fn store_keybinds(&mut self, event: &mut Event, client_id: Option<ClientId>) {
        let Some(keybinds) = take_keybinds(event) else {
            return;
        };
        match client_id {
            Some(client_id) => self.keybinds.set(client_id, keybinds),
            None => {
                let mut client_ids: Vec<ClientId> = self.keybinds.client_ids().collect();
                client_ids.extend(self.pending.keys().copied());
                client_ids.extend(self.tab_bar.client_ids());
                client_ids.extend(self.status_bar.client_ids());
                client_ids.extend(self.compact_bar.client_ids());
                client_ids.sort_unstable();
                client_ids.dedup();
                for client_id in client_ids {
                    self.keybinds.set(client_id, keybinds.clone());
                }
            },
        }
    }

    fn snapshot_of(&self, role: Role) -> BTreeMap<ClientId, ClientSeed> {
        match role {
            Role::TabBar => self.tab_bar.snapshot(),
            Role::StatusBar => self.status_bar.snapshot(),
            Role::CompactBar => self.compact_bar.snapshot(),
            Role::Link => BTreeMap::new(),
        }
    }
}

impl ZellijSharedPlugin for State {
    fn slot_added(&mut self, slot: Slot) {
        let role = Role::from_configuration(&slot.configuration);
        if slot.kind == SlotKind::Pane {
            set_selectable(false);
        }
        if role != Role::Link && !self.role_has_slots(role) {
            let seeds = self.snapshot_for(role);
            match role {
                Role::TabBar => self.tab_bar.seed(&seeds),
                Role::StatusBar => self.status_bar.seed(&seeds),
                Role::CompactBar => self.compact_bar.seed(&seeds),
                Role::Link => {},
            }
        }
        self.slots.insert(slot.id, role);
        match role {
            Role::TabBar => self.tab_bar.slot_added(&slot),
            Role::StatusBar => self.status_bar.slot_added(&slot),
            Role::CompactBar => self.compact_bar.slot_added(&slot),
            Role::Link => self.link.slot_added(&slot),
        }
        if self.any_bar_has_slots() {
            self.pending.clear();
        }
    }

    fn slot_removed(&mut self, slot_id: SlotId) {
        let Some(role) = self.slots.remove(&slot_id) else {
            return;
        };
        match role {
            Role::TabBar => self.tab_bar.slot_removed(slot_id),
            Role::StatusBar => self.status_bar.slot_removed(slot_id),
            Role::CompactBar => self.compact_bar.slot_removed(slot_id),
            Role::Link => self.link.slot_removed(slot_id),
        }
        if role != Role::Link && !self.role_has_slots(role) {
            if !self.any_bar_has_slots() {
                self.pending = self.snapshot_of(role);
            }
            match role {
                Role::TabBar => self.tab_bar.reset(),
                Role::StatusBar => self.status_bar.reset(),
                Role::CompactBar => self.compact_bar.reset(),
                Role::Link => {},
            }
        }
    }

    fn client_connected(&mut self, client_id: ClientId) {
        if self.tab_bar.has_slots() {
            self.tab_bar.client_connected(client_id);
        }
        if self.status_bar.has_slots() {
            self.status_bar.client_connected(client_id);
        }
        if self.compact_bar.has_slots() {
            self.compact_bar.client_connected(client_id);
        }
    }

    fn client_disconnected(&mut self, client_id: ClientId) {
        self.pending.remove(&client_id);
        self.keybinds.remove(client_id);
        self.tab_bar.client_disconnected(client_id);
        self.status_bar.client_disconnected(client_id);
        self.compact_bar.client_disconnected(client_id);
    }

    fn update(&mut self, mut event: Event, context: EventContext) -> RenderResponse {
        self.compact_bar.ensure_toggle_keybinds(context.client_id);
        self.store_keybinds(&mut event, context.client_id);
        match &event {
            Event::Mouse(_) | Event::Key(_) => {
                let role = context
                    .slot_id
                    .and_then(|slot_id| self.slots.get(&slot_id).copied());
                match role {
                    Some(Role::TabBar) => self.tab_bar.update(&event, context),
                    Some(Role::StatusBar) => self.status_bar.update(&event, context),
                    Some(Role::CompactBar) => {
                        self.compact_bar.update(&event, context, &mut self.keybinds)
                    },
                    _ => RenderResponse::Nothing,
                }
            },
            _ => {
                let mut render = RenderResponse::Nothing;
                if !self.any_bar_has_slots() {
                    if let Some(client_id) = context.client_id {
                        if matches!(
                            event,
                            Event::ModeUpdate(_) | Event::TabUpdate(_) | Event::InitialKeybinds(_)
                        ) {
                            self.pending.entry(client_id).or_default().apply(&event);
                        }
                    }
                }
                if self.tab_bar.has_slots() || matches!(event, Event::Timer(_)) {
                    let tab_bar_render = self.tab_bar.update(&event, context);
                    if self.tab_bar.has_slots() {
                        render = render.merge(tab_bar_render);
                    }
                }
                if self.status_bar.has_slots() {
                    render = render.merge(self.status_bar.update(&event, context));
                }
                if self.compact_bar.has_slots() {
                    render =
                        render.merge(self.compact_bar.update(&event, context, &mut self.keybinds));
                }
                if self.link.has_slots() {
                    self.link.update(&event);
                }
                render
            },
        }
    }

    fn pipe(&mut self, pipe_message: PipeMessage, context: EventContext) -> RenderResponse {
        self.compact_bar.ensure_toggle_keybinds(context.client_id);
        if pipe_message.name == session_indicator::PANEL_SIZE_PIPE {
            let mut render = RenderResponse::Nothing;
            if self.tab_bar.has_slots() {
                render = render.merge(self.tab_bar.pipe(&pipe_message));
            }
            if self.compact_bar.has_slots() {
                render = render.merge(self.compact_bar.session_indicator_pipe(&pipe_message));
            }
            return render;
        }
        if self.compact_bar.has_slots() {
            self.compact_bar
                .pipe(pipe_message, context, &mut self.keybinds)
        } else {
            RenderResponse::Nothing
        }
    }

    fn render(&mut self, rows: usize, cols: usize, slot_id: SlotId, client_id: ClientId) {
        match self.slots.get(&slot_id) {
            Some(Role::TabBar) => {
                self.tab_bar
                    .render(rows, cols, slot_id, client_id, &mut self.keybinds)
            },
            Some(Role::StatusBar) => {
                self.status_bar
                    .render(rows, cols, slot_id, client_id, &mut self.keybinds)
            },
            Some(Role::CompactBar) => {
                self.compact_bar
                    .render(rows, cols, slot_id, client_id, &mut self.keybinds)
            },
            _ => {},
        }
    }
}

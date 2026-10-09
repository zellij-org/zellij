mod action_types;
mod clipboard_utils;
mod keybind_utils;
mod line;
mod tab;
mod tooltip;

use std::cmp::{max, min};
use std::collections::{BTreeMap, BTreeSet};

use tab::get_clicked_line_part;
use zellij_tile::prelude::*;

use crate::click_actions::ActionRunner;
use crate::double_click::{
    apply_double_click_outcome, classify_click, double_click_outcome, ClickTarget,
    DoubleClickConfig,
};
use crate::keybinds::KeybindStore;
use crate::tab_drag::{
    apply_release, is_tab_move_completion, request_tab_move, segments_of, switch_outcome,
    tab_parts_in_line, TabDrag, TabPart, TabSegment,
};
use crate::ClientSeed;
use clipboard_utils::{system_clipboard_error, text_copied_hint};
use line::{tab_line, CompactHover};
use tab::tab_style;
use tooltip::{tooltip_region_at, TooltipRegion, TooltipRenderer};

static ARROW_SEPARATOR: &str = "";

const CONFIG_IS_TOOLTIP: &str = "is_tooltip";
const CONFIG_TOGGLE_TOOLTIP_KEY: &str = "tooltip";
const MSG_TOGGLE_TOOLTIP: &str = "toggle_tooltip";
const MSG_TOGGLE_PERSISTED_TOOLTIP: &str = "toggle_persisted_tooltip";
const MSG_LAUNCH_TOOLTIP: &str = "launch_tooltip_if_not_launched";
const RUNNER_OWNER: &str = "compact-bar";

#[derive(Debug, Default)]
pub struct LinePart {
    part: String,
    len: usize,
    tab_index: Option<usize>,
}

struct TabRenderData {
    tabs: Vec<LinePart>,
    active_tab_index: usize,
    active_swap_layout_name: Option<String>,
    is_swap_layout_dirty: bool,
}

fn click_target(slot_client: &SlotClientState, col: usize) -> ClickTarget {
    let in_reserved_range =
        matches!(slot_client.breadcrumb_range, Some((start, end)) if col >= start && col < end);
    let clicked_part = get_clicked_line_part(&slot_client.tab_line, col)
        .map(|line_part| (line_part.tab_index, line_part.part.as_str()));
    classify_click(clicked_part, in_reserved_range)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompactRegion {
    Tab(usize),
    Marker { left: bool, position: usize },
    Breadcrumb,
    Mode,
}

fn region_at(regions: &[(usize, usize, CompactRegion)], col: usize) -> Option<CompactRegion> {
    regions
        .iter()
        .find(|(start, end, _)| col >= *start && col < *end)
        .map(|(_, _, region)| *region)
}

fn mode_click_target(mode_info: &ModeInfo) -> Option<InputMode> {
    let base_mode = mode_info.base_mode.unwrap_or(InputMode::Normal);
    if mode_info.mode != base_mode {
        Some(base_mode)
    } else {
        None
    }
}

fn compact_regions(
    parts: &[LinePart],
    rendered_tabs: &[String],
    active_tab_position: usize,
    breadcrumb_range: Option<(usize, usize)>,
    mode_range: Option<(usize, usize)>,
) -> (Vec<(usize, usize, CompactRegion)>, Vec<TabSegment>) {
    let mut regions = vec![];
    if let Some((start, end)) = breadcrumb_range {
        regions.push((start, end, CompactRegion::Breadcrumb));
    }
    if let Some((start, end)) = mode_range {
        regions.push((start, end, CompactRegion::Mode));
    }
    let tab_parts = tab_parts_in_line(
        parts
            .iter()
            .map(|part| (part.len, part.tab_index, part.part.as_str())),
        rendered_tabs,
        active_tab_position,
    );
    let segments = segments_of(&tab_parts);
    for (start, end, kind) in tab_parts {
        regions.push((
            start,
            end,
            match kind {
                TabPart::Tab(position) => CompactRegion::Tab(position),
                TabPart::Marker { left, position } => CompactRegion::Marker { left, position },
            },
        ));
    }
    (regions, segments)
}

#[derive(Default)]
struct SlotConfig {
    config: BTreeMap<String, String>,
    is_tooltip: bool,
    toggle_tooltip_key: Option<String>,
    persist: bool,
    is_first_run: bool,
    double_click: DoubleClickConfig,
}

#[derive(Default)]
struct ClientState {
    tabs: Vec<TabInfo>,
    active_tab_idx: usize,
    mode_info: ModeInfo,
    display_area_rows: usize,
    display_area_cols: usize,
    text_copy_destination: Option<CopyDestination>,
    display_system_clipboard_failure: bool,
    last_left_click: Option<(SlotId, ClickTarget)>,
    hovered: Option<CompactRegion>,
    hover_at: Option<(SlotId, usize)>,
    drag: Option<TabDrag>,
    switch_when_settled: Option<usize>,
    tooltip_hovered: Option<Vec<zellij_tile::prelude::actions::Action>>,
    tooltip_hover_at: Option<(SlotId, usize, usize)>,
}

fn hoverable(client: &ClientState, region: &CompactRegion) -> bool {
    match region {
        CompactRegion::Mode => mode_click_target(&client.mode_info).is_some(),
        CompactRegion::Tab(position) => client
            .tabs
            .iter()
            .any(|t| t.position == *position && !t.active),
        _ => true,
    }
}

#[derive(Default)]
struct SlotClientState {
    tab_line: Vec<LinePart>,
    breadcrumb_range: Option<(usize, usize)>,
    regions: Vec<(usize, usize, CompactRegion)>,
    segments: Vec<TabSegment>,
    tooltip_regions: Vec<TooltipRegion>,
    cols: usize,
}

#[derive(Default)]
pub struct CompactBar {
    slots: BTreeMap<SlotId, SlotConfig>,
    slot_tabs: BTreeMap<SlotId, usize>,
    clients: BTreeMap<ClientId, ClientState>,
    slot_clients: BTreeMap<(SlotId, ClientId), SlotClientState>,
    tooltip_is_active: bool,
    configured_toggle_keys: BTreeSet<(String, ClientId)>,
    runner: ActionRunner,
}

impl CompactBar {
    pub fn has_slots(&self) -> bool {
        !self.slots.is_empty()
    }

    pub fn slot_added(&mut self, slot: &Slot) {
        let config = slot.configuration.clone();
        let is_tooltip = config
            .get(CONFIG_IS_TOOLTIP)
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);
        let toggle_tooltip_key = if is_tooltip {
            None
        } else {
            config.get(CONFIG_TOGGLE_TOOLTIP_KEY).cloned()
        };
        let events = if is_tooltip {
            vec![
                EventType::ModeUpdate,
                EventType::TabUpdate,
                EventType::InitialKeybinds,
                EventType::Mouse,
                EventType::ActionComplete,
            ]
        } else {
            vec![
                EventType::TabUpdate,
                EventType::PaneUpdate,
                EventType::ModeUpdate,
                EventType::Mouse,
                EventType::CopyToClipboard,
                EventType::InputReceived,
                EventType::SystemClipboardFailure,
                EventType::InitialKeybinds,
                EventType::ActionComplete,
            ]
        };
        subscribe(&events);
        let double_click = DoubleClickConfig::from_configuration(&config);
        self.slots.insert(
            slot.id,
            SlotConfig {
                config,
                is_tooltip,
                toggle_tooltip_key,
                persist: false,
                is_first_run: is_tooltip,
                double_click,
            },
        );
    }

    pub fn slot_removed(&mut self, slot_id: SlotId) {
        self.slots.remove(&slot_id);
        self.slot_tabs.remove(&slot_id);
        self.slot_clients.retain(|(s, _), _| *s != slot_id);
    }

    pub fn client_connected(&mut self, client_id: ClientId) {
        self.clients.entry(client_id).or_default();
    }

    pub fn client_disconnected(&mut self, client_id: ClientId) {
        self.clients.remove(&client_id);
        self.slot_clients.retain(|(_, c), _| *c != client_id);
        self.configured_toggle_keys.retain(|(_, c)| *c != client_id);
    }

    pub fn client_ids(&self) -> impl Iterator<Item = ClientId> + '_ {
        self.clients.keys().copied()
    }

    fn swap_keybinds(&mut self, client_id: ClientId, keybinds: &mut KeybindStore) -> bool {
        match self.clients.get_mut(&client_id) {
            Some(client) => keybinds.swap(client_id, &mut client.mode_info.keybinds),
            None => false,
        }
    }

    pub fn reset(&mut self) {
        self.clients.clear();
        self.slot_clients.clear();
        self.slot_tabs.clear();
        self.tooltip_is_active = false;
    }

    pub fn snapshot(&self) -> BTreeMap<ClientId, ClientSeed> {
        self.clients
            .iter()
            .map(|(client_id, client)| {
                (
                    *client_id,
                    ClientSeed {
                        mode_info: client.mode_info.clone(),
                        tabs: client.tabs.clone(),
                    },
                )
            })
            .collect()
    }

    pub fn seed(&mut self, seeds: &BTreeMap<ClientId, ClientSeed>) {
        for (client_id, seed) in seeds {
            let client = self.clients.entry(*client_id).or_default();
            client.mode_info = seed.mode_info.clone();
            update_display_area(client, &seed.tabs);
            if let Some(active_tab_index) = seed.tabs.iter().position(|t| t.active) {
                client.active_tab_idx = active_tab_index + 1;
                client.tabs = seed.tabs.clone();
            }
        }
    }

    pub fn ensure_toggle_keybinds(&mut self, client_id: Option<ClientId>) {
        let Some(client_id) = client_id else {
            return;
        };
        for slot in self.slots.values() {
            if slot.is_tooltip {
                continue;
            }
            if let Some(toggle_key) = &slot.toggle_tooltip_key {
                let entry = (toggle_key.clone(), client_id);
                if !self.configured_toggle_keys.contains(&entry) {
                    reconfigure(bind_toggle_key_config(toggle_key, client_id), false);
                    self.configured_toggle_keys.insert(entry);
                }
            }
        }
    }

    fn has_toggle_key(&self) -> bool {
        self.slots
            .values()
            .any(|s| !s.is_tooltip && s.toggle_tooltip_key.is_some())
    }

    fn main_slot_ids(&self) -> Vec<SlotId> {
        self.slots
            .iter()
            .filter(|(_, s)| !s.is_tooltip)
            .map(|(id, _)| *id)
            .collect()
    }

    fn tooltip_slot_ids(&self) -> Vec<SlotId> {
        self.slots
            .iter()
            .filter(|(_, s)| s.is_tooltip)
            .map(|(id, _)| *id)
            .collect()
    }

    fn active_tab_index_of(&self, client_id: ClientId) -> Option<usize> {
        self.clients
            .get(&client_id)
            .filter(|c| c.active_tab_idx > 0)
            .map(|c| c.active_tab_idx - 1)
    }

    fn tooltip_slots_in_tab(&self, tab_index: Option<usize>) -> Vec<SlotId> {
        self.tooltip_slot_ids()
            .into_iter()
            .filter(|id| match (tab_index, self.slot_tabs.get(id)) {
                (Some(tab_index), Some(slot_tab)) => *slot_tab == tab_index,
                _ => true,
            })
            .collect()
    }

    fn tooltip_slots_for_client(&self, client_id: ClientId) -> Vec<SlotId> {
        self.tooltip_slots_in_tab(self.active_tab_index_of(client_id))
    }

    fn main_slot_for_client(&self, client_id: ClientId) -> Option<SlotId> {
        let active_tab_index = self.active_tab_index_of(client_id);
        let candidates: Vec<SlotId> = self
            .slots
            .iter()
            .filter(|(_, s)| !s.is_tooltip && s.toggle_tooltip_key.is_some())
            .map(|(id, _)| *id)
            .collect();
        candidates
            .iter()
            .find(|id| {
                active_tab_index.is_some() && self.slot_tabs.get(*id) == active_tab_index.as_ref()
            })
            .or_else(|| candidates.first())
            .copied()
    }

    fn main_slot_in_active_tab(&self, client_id: ClientId) -> Option<SlotId> {
        let active_tab_index = self
            .clients
            .get(&client_id)
            .map(|c| c.active_tab_idx.saturating_sub(1))?;
        self.slots
            .iter()
            .filter(|(_, s)| !s.is_tooltip && s.toggle_tooltip_key.is_some())
            .map(|(id, _)| *id)
            .find(|id| self.slot_tabs.get(id) == Some(&active_tab_index))
    }

    pub fn update(
        &mut self,
        event: &Event,
        context: EventContext,
        keybinds: &mut KeybindStore,
    ) -> RenderResponse {
        match event {
            Event::ModeUpdate(_) | Event::TabUpdate(_) | Event::InitialKeybinds(_) => {
                for slot in self.slots.values_mut() {
                    slot.is_first_run = false;
                }
            },
            _ => {},
        }
        match event {
            Event::PaneUpdate(pane_manifest) => {
                if self.handle_pane_update(pane_manifest) {
                    RenderResponse::Slots(self.main_slot_ids())
                } else {
                    RenderResponse::Nothing
                }
            },
            Event::Mouse(mouse_event) => {
                if let (Some(slot_id), Some(client_id)) = (context.slot_id, context.client_id) {
                    if self.handle_mouse_event(slot_id, client_id, mouse_event) {
                        return RenderResponse::Client(client_id);
                    }
                }
                RenderResponse::Nothing
            },
            _ => {
                let target_clients: Vec<ClientId> = match context.client_id {
                    Some(client_id) => vec![client_id],
                    None => self.clients.keys().copied().collect(),
                };
                let mut should_render = false;
                for client_id in target_clients {
                    if self.update_client(event, client_id, keybinds) {
                        should_render = true;
                    }
                }
                if !should_render {
                    RenderResponse::Nothing
                } else if let Some(client_id) = context.client_id {
                    RenderResponse::Client(client_id)
                } else {
                    RenderResponse::Slots(self.slots.keys().copied().collect())
                }
            },
        }
    }

    fn update_client(
        &mut self,
        event: &Event,
        client_id: ClientId,
        keybinds: &mut KeybindStore,
    ) -> bool {
        match event {
            Event::InitialKeybinds(_) => {
                self.clients.entry(client_id).or_default();
                true
            },
            Event::ModeUpdate(mode_info) => {
                self.handle_mode_update(client_id, mode_info.clone(), keybinds)
            },
            Event::TabUpdate(tabs) => self.handle_tab_update(client_id, tabs),
            Event::CopyToClipboard(copy_destination) => {
                self.handle_clipboard_copy(client_id, *copy_destination)
            },
            Event::SystemClipboardFailure => self.handle_clipboard_failure(client_id),
            Event::InputReceived => self.handle_input_received(client_id),
            Event::ActionComplete(..) if !is_tab_move_completion(event) => {
                self.runner.action_completed(RUNNER_OWNER, client_id, event);
                false
            },
            event if is_tab_move_completion(event) => {
                let Some(client) = self.clients.get_mut(&client_id) else {
                    return false;
                };
                let mut should_render = false;
                if let Some(drag) = client.drag.as_mut() {
                    should_render = drag.move_completed();
                }
                if let Some(tab_id) = client.switch_when_settled.take() {
                    apply_release(switch_outcome(&client.tabs, tab_id));
                }
                should_render
            },
            _ => false,
        }
    }

    fn handle_mode_update(
        &mut self,
        client_id: ClientId,
        mode_info: ModeInfo,
        keybinds: &mut KeybindStore,
    ) -> bool {
        let client = self.clients.entry(client_id).or_default();
        let should_render = client.mode_info != mode_info;
        let old_mode = client.mode_info.mode;
        let new_mode = mode_info.mode;
        let base_mode = mode_info.base_mode.unwrap_or(InputMode::Normal);

        client.mode_info = mode_info;

        let lent = self.swap_keybinds(client_id, keybinds);
        for slot_id in self.tooltip_slots_for_client(client_id) {
            self.handle_tooltip_mode_update(slot_id, client_id, old_mode, new_mode, base_mode);
        }
        self.handle_main_mode_update(client_id, new_mode, base_mode);
        if lent {
            self.swap_keybinds(client_id, keybinds);
        }

        should_render
    }

    fn handle_main_mode_update(
        &self,
        client_id: ClientId,
        new_mode: InputMode,
        base_mode: InputMode,
    ) {
        if self.has_toggle_key() && new_mode != base_mode && !is_restricted_mode(new_mode) {
            if let Some(slot_id) = self.main_slot_for_client(client_id) {
                self.launch_tooltip_if_not_launched(slot_id, client_id, new_mode);
            }
        }
    }

    fn handle_tooltip_mode_update(
        &mut self,
        slot_id: SlotId,
        client_id: ClientId,
        old_mode: InputMode,
        new_mode: InputMode,
        base_mode: InputMode,
    ) {
        let persist = self.slots.get(&slot_id).map(|s| s.persist).unwrap_or(false);
        if !persist && (new_mode == base_mode || is_restricted_mode(new_mode)) {
            let _ = close_slot(slot_id);
        } else if new_mode != old_mode || persist {
            self.update_tooltip_for_mode_change(slot_id, client_id, new_mode);
        }
    }

    fn handle_tab_update(&mut self, client_id: ClientId, tabs: &[TabInfo]) -> bool {
        let client = self.clients.entry(client_id).or_default();
        update_display_area(client, tabs);

        if let Some(active_tab_index) = tabs.iter().position(|t| t.active) {
            let active_tab_idx = active_tab_index + 1;
            let previous_active_tab_idx = client.active_tab_idx;
            let should_render =
                client.active_tab_idx != active_tab_idx || client.tabs.as_slice() != tabs;

            client.active_tab_idx = active_tab_idx;
            client.tabs = tabs.to_vec();
            if let Some(drag) = client.drag.as_mut() {
                drag.tabs_updated(tabs);
            }
            if let Some(tab_id) = client.switch_when_settled.take() {
                apply_release(switch_outcome(tabs, tab_id));
            }

            if previous_active_tab_idx != 0 && previous_active_tab_idx != active_tab_idx {
                let tooltips = self.tooltip_slots_in_tab(Some(previous_active_tab_idx - 1));
                for slot_id in tooltips {
                    self.move_tooltip_to_new_tab(slot_id, active_tab_idx);
                }
            }

            should_render
        } else {
            false
        }
    }

    fn handle_pane_update(&mut self, pane_manifest: &PaneManifest) -> bool {
        for (tab_index, panes) in &pane_manifest.panes {
            for pane in panes {
                if pane.is_plugin && self.slots.contains_key(&pane.id) {
                    self.slot_tabs.insert(pane.id, *tab_index);
                }
            }
        }
        if self.has_toggle_key() {
            let previous_tooltip_state = self.tooltip_is_active;
            self.tooltip_is_active = self.detect_tooltip_presence(pane_manifest);
            previous_tooltip_state != self.tooltip_is_active
        } else {
            false
        }
    }

    fn handle_mouse_event(
        &mut self,
        slot_id: SlotId,
        client_id: ClientId,
        mouse_event: &Mouse,
    ) -> bool {
        let Some(slot) = self.slots.get(&slot_id) else {
            return false;
        };
        if slot.is_tooltip {
            return self.handle_tooltip_mouse_event(slot_id, client_id, mouse_event);
        }
        let double_click_config = slot.double_click;
        let Some(client) = self.clients.get_mut(&client_id) else {
            return false;
        };
        let empty_slot_client = SlotClientState::default();
        let slot_client = self
            .slot_clients
            .get(&(slot_id, client_id))
            .unwrap_or(&empty_slot_client);

        let mut should_render = false;
        match mouse_event {
            Mouse::LeftClick(_, col) => {
                let col = *col;
                client.hover_at = Some((slot_id, col));
                client.last_left_click = Some((slot_id, click_target(slot_client, col)));
                if client.drag.take().is_some() {
                    should_render = true;
                }
                match region_at(&slot_client.regions, col) {
                    Some(CompactRegion::Breadcrumb) => {
                        focus_host_session();
                    },
                    Some(CompactRegion::Mode) => {
                        if let Some(base_mode) = mode_click_target(&client.mode_info) {
                            switch_to_input_mode(&base_mode);
                        }
                    },
                    Some(CompactRegion::Marker { position, .. }) => {
                        switch_tab_to(position as u32 + 1);
                    },
                    Some(CompactRegion::Tab(position)) => {
                        if let Some(tab) = client.tabs.iter().find(|t| t.position == position) {
                            client.drag = Some(TabDrag::new(Some(slot_id), tab.tab_id));
                            should_render = true;
                        }
                    },
                    None => {},
                }
            },
            Mouse::Hold(_, col) => {
                client.hover_at = Some((slot_id, *col));
                if let Some(drag) = client
                    .drag
                    .as_mut()
                    .filter(|drag| drag.slot_id == Some(slot_id))
                {
                    if let Some((tab_id, target)) =
                        drag.hold(&client.tabs, &slot_client.segments, *col)
                    {
                        request_tab_move(tab_id, target);
                    }
                }
            },
            Mouse::Release(_, col) => {
                client.hover_at = Some((slot_id, *col));
                if let Some(drag) = client.drag.take() {
                    client.switch_when_settled = apply_release(drag.release(&client.tabs));
                    should_render = true;
                }
            },
            Mouse::Hover(_, col) => {
                let col = *col;
                let on_screen = col < slot_client.cols;
                let owns_hover = client
                    .hover_at
                    .map(|(hover_slot, _)| hover_slot == slot_id)
                    .unwrap_or(true);
                if on_screen || owns_hover {
                    client.hover_at = if on_screen {
                        Some((slot_id, col))
                    } else {
                        None
                    };
                    let hovered = region_at(&slot_client.regions, col)
                        .filter(|region| on_screen && hoverable(client, region));
                    if client.hovered != hovered {
                        client.hovered = hovered;
                        should_render = true;
                    }
                }
            },
            Mouse::DoubleClick(_, col) => {
                if client.drag.take().is_some() {
                    should_render = true;
                }
                let target = click_target(slot_client, *col);
                let last_left_click = client
                    .last_left_click
                    .take()
                    .filter(|(last_slot_id, _)| *last_slot_id == slot_id)
                    .map(|(_, last_target)| last_target);
                let outcome = double_click_outcome(&double_click_config, last_left_click, target);
                apply_double_click_outcome(outcome, &client.tabs, client.active_tab_idx);
            },
            Mouse::RightClick(line, col) => {
                let target = match get_clicked_line_part(&slot_client.tab_line, *col)
                    .and_then(|line_part| line_part.tab_index)
                {
                    Some(tab_index) => ContextMenuTarget::Tab(tab_index),
                    None => ContextMenuTarget::Bar,
                };
                open_context_menu(target, (*line).max(0) as usize, *col);
            },
            Mouse::ScrollUp(_) => {
                let next_tab = min(client.active_tab_idx + 1, client.tabs.len());
                switch_tab_to(next_tab as u32);
            },
            Mouse::ScrollDown(_) => {
                let prev_tab = max(client.active_tab_idx.saturating_sub(1), 1);
                switch_tab_to(prev_tab as u32);
            },
            _ => {},
        }
        should_render
    }

    fn handle_tooltip_mouse_event(
        &mut self,
        slot_id: SlotId,
        client_id: ClientId,
        mouse_event: &Mouse,
    ) -> bool {
        let Some(client) = self.clients.get_mut(&client_id) else {
            return false;
        };
        let empty_slot_client = SlotClientState::default();
        let slot_client = self
            .slot_clients
            .get(&(slot_id, client_id))
            .unwrap_or(&empty_slot_client);
        let regions = slot_client.tooltip_regions.as_slice();
        match mouse_event {
            Mouse::LeftClick(line, col) => {
                if let Some(region) = tooltip_region_at(regions, (*line).max(0) as usize, *col) {
                    self.runner
                        .run(RUNNER_OWNER, client_id, region.actions.clone());
                }
                false
            },
            Mouse::Hover(line, col) => {
                let on_screen = *line >= 0 && *col < slot_client.cols;
                client.tooltip_hover_at = if on_screen {
                    Some((slot_id, *line as usize, *col))
                } else {
                    None
                };
                let hovered = if on_screen {
                    tooltip_region_at(regions, *line as usize, *col)
                        .map(|region| region.actions.clone())
                } else {
                    None
                };
                if client.tooltip_hovered != hovered {
                    client.tooltip_hovered = hovered;
                    true
                } else {
                    false
                }
            },
            _ => false,
        }
    }

    fn handle_clipboard_copy(
        &mut self,
        client_id: ClientId,
        copy_destination: CopyDestination,
    ) -> bool {
        let client = self.clients.entry(client_id).or_default();
        let should_render = match client.text_copy_destination {
            Some(current) => current != copy_destination,
            None => true,
        };

        client.text_copy_destination = Some(copy_destination);
        should_render
    }

    fn handle_clipboard_failure(&mut self, client_id: ClientId) -> bool {
        let client = self.clients.entry(client_id).or_default();
        client.display_system_clipboard_failure = true;
        true
    }

    fn handle_input_received(&mut self, client_id: ClientId) -> bool {
        let client = self.clients.entry(client_id).or_default();
        let should_render =
            client.text_copy_destination.is_some() || client.display_system_clipboard_failure;
        client.text_copy_destination = None;
        client.display_system_clipboard_failure = false;
        should_render
    }

    pub fn pipe(
        &mut self,
        message: PipeMessage,
        context: EventContext,
        keybinds: &mut KeybindStore,
    ) -> RenderResponse {
        if let Some(slot_id) = context.slot_id {
            if self
                .slots
                .get(&slot_id)
                .map(|s| s.is_tooltip)
                .unwrap_or(false)
            {
                if message.is_private {
                    self.handle_tooltip_pipe(slot_id, &message);
                }
                return RenderResponse::Nothing;
            }
        }
        if message.name == MSG_TOGGLE_TOOLTIP && message.is_private {
            let client_id = message
                .payload
                .as_ref()
                .and_then(|p| p.trim().parse::<ClientId>().ok());
            if let Some(client_id) = client_id {
                if let Some(slot_id) = self.main_slot_in_active_tab(client_id) {
                    let mode = self
                        .clients
                        .get(&client_id)
                        .map(|c| c.mode_info.mode)
                        .unwrap_or(InputMode::Normal);
                    let lent = self.swap_keybinds(client_id, keybinds);
                    self.toggle_persisted_tooltip(slot_id, client_id, mode);
                    if lent {
                        self.swap_keybinds(client_id, keybinds);
                    }
                }
            }
        } else if message.is_private
            && (message.name == MSG_TOGGLE_PERSISTED_TOOLTIP || message.name == MSG_LAUNCH_TOOLTIP)
        {
            let targets = match context.client_id {
                Some(client_id) => self.tooltip_slots_for_client(client_id),
                None => self.tooltip_slot_ids(),
            };
            for slot_id in targets {
                self.handle_tooltip_pipe(slot_id, &message);
            }
        }
        RenderResponse::Nothing
    }

    fn handle_tooltip_pipe(&mut self, slot_id: SlotId, message: &PipeMessage) {
        if message.name == MSG_TOGGLE_PERSISTED_TOOLTIP {
            let Some(slot) = self.slots.get_mut(&slot_id) else {
                return;
            };
            if slot.is_first_run {
                slot.persist = true;
            } else {
                #[cfg(target_family = "wasm")]
                let _ = close_slot(slot_id);
            }
        }
    }

    fn toggle_persisted_tooltip(&self, slot_id: SlotId, client_id: ClientId, new_mode: InputMode) {
        #[allow(unused_variables)]
        let message = self
            .create_tooltip_message(slot_id, client_id, MSG_TOGGLE_PERSISTED_TOOLTIP, new_mode)
            .with_args(create_persist_args());

        #[cfg(target_family = "wasm")]
        pipe_message_to_plugin(message);
    }

    fn launch_tooltip_if_not_launched(
        &self,
        slot_id: SlotId,
        client_id: ClientId,
        new_mode: InputMode,
    ) {
        let message = self.create_tooltip_message(slot_id, client_id, MSG_LAUNCH_TOOLTIP, new_mode);
        pipe_message_to_plugin(message);
    }

    fn create_tooltip_message(
        &self,
        slot_id: SlotId,
        client_id: ClientId,
        name: &str,
        mode: InputMode,
    ) -> MessageToPlugin {
        let mut tooltip_config = self
            .slots
            .get(&slot_id)
            .map(|s| s.config.clone())
            .unwrap_or_default();
        tooltip_config.insert(CONFIG_IS_TOOLTIP.to_string(), "true".to_string());

        MessageToPlugin::new(name)
            .with_plugin_url("zellij:OWN_URL")
            .with_plugin_config(tooltip_config)
            .with_floating_pane_coordinates(self.calculate_tooltip_coordinates(client_id))
            .new_plugin_instance_should_have_pane_title(format!("{:?}", mode))
    }

    fn update_tooltip_for_mode_change(
        &self,
        slot_id: SlotId,
        client_id: ClientId,
        new_mode: InputMode,
    ) {
        let coordinates = self.calculate_tooltip_coordinates(client_id);
        change_floating_panes_coordinates(vec![(PaneId::Plugin(slot_id), coordinates)]);
        rename_plugin_pane(slot_id, format!("{:?}", new_mode));
    }

    fn move_tooltip_to_new_tab(&mut self, slot_id: SlotId, new_tab_index: usize) {
        break_panes_to_tab_with_index(
            &[PaneId::Plugin(slot_id)],
            new_tab_index.saturating_sub(1),
            false,
        );
        self.slot_tabs
            .insert(slot_id, new_tab_index.saturating_sub(1));
    }

    fn calculate_tooltip_coordinates(&self, client_id: ClientId) -> FloatingPaneCoordinates {
        let default_client = ClientState::default();
        let client = self.clients.get(&client_id).unwrap_or(&default_client);
        let tooltip_renderer = TooltipRenderer::new(&client.mode_info);
        let (tooltip_rows, tooltip_cols) =
            tooltip_renderer.calculate_dimensions(client.mode_info.mode);

        let width = tooltip_cols + 4;
        let height = tooltip_rows + 2;
        let x_position = 2;
        let y_position = client.display_area_rows.saturating_sub(height + 2);

        FloatingPaneCoordinates::new(
            Some(x_position.to_string()),
            Some(y_position.to_string()),
            Some(width.to_string()),
            Some(height.to_string()),
            Some(true),
            Some(false),
        )
        .unwrap_or_default()
    }

    fn detect_tooltip_presence(&self, pane_manifest: &PaneManifest) -> bool {
        for (_tab_index, panes) in &pane_manifest.panes {
            for pane in panes {
                if pane.is_plugin
                    && self
                        .slots
                        .get(&pane.id)
                        .map(|s| s.is_tooltip)
                        .unwrap_or(false)
                    && pane.pane_x != pane.pane_content_x
                {
                    return true;
                }
            }
        }
        false
    }

    pub fn render(
        &mut self,
        rows: usize,
        cols: usize,
        slot_id: SlotId,
        client_id: ClientId,
        keybinds: &mut KeybindStore,
    ) {
        let lent = self.swap_keybinds(client_id, keybinds);
        self.render_client(rows, cols, slot_id, client_id);
        if lent {
            self.swap_keybinds(client_id, keybinds);
        }
    }

    fn render_client(&mut self, rows: usize, cols: usize, slot_id: SlotId, client_id: ClientId) {
        let Some(slot) = self.slots.get(&slot_id) else {
            return;
        };
        let Some(client) = self.clients.get(&client_id) else {
            return;
        };
        if slot.is_tooltip {
            let tooltip_renderer = TooltipRenderer::new(&client.mode_info);
            let regions = tooltip_renderer.regions(rows, cols);
            let hovered = client
                .tooltip_hover_at
                .filter(|(hover_slot, _, _)| *hover_slot == slot_id)
                .and_then(|(_, row, col)| tooltip_region_at(&regions, row, col))
                .map(|region| region.actions.clone());
            tooltip_renderer.render(rows, cols, hovered.as_ref());
            if let Some(client) = self.clients.get_mut(&client_id) {
                client.tooltip_hovered = hovered;
            }
            let slot_client = self.slot_clients.entry((slot_id, client_id)).or_default();
            slot_client.tooltip_regions = regions;
            slot_client.cols = cols;
            return;
        }
        if let Some(copy_destination) = client.text_copy_destination {
            let hint = text_copied_hint(copy_destination).part;
            render_background_with_text(&client.mode_info, &hint);
        } else if client.display_system_clipboard_failure {
            let hint = system_clipboard_error().part;
            render_background_with_text(&client.mode_info, &hint);
        } else {
            if client.tabs.is_empty() {
                return;
            }

            let compose = |hovered: Option<CompactRegion>| {
                let tab_data = prepare_tab_data(client, hovered);
                let rendered_tabs: Vec<String> =
                    tab_data.tabs.iter().map(|t| t.part.clone()).collect();
                let active_tab_position = tab_data.active_tab_index;
                let hover = CompactHover {
                    breadcrumb: hovered == Some(CompactRegion::Breadcrumb),
                };
                let tab_line_output = tab_line(
                    &client.mode_info,
                    tab_data,
                    cols,
                    slot.toggle_tooltip_key.clone(),
                    self.tooltip_is_active,
                    hover,
                );
                let (regions, segments) = compact_regions(
                    &tab_line_output.parts,
                    &rendered_tabs,
                    active_tab_position,
                    tab_line_output.breadcrumb_range,
                    tab_line_output.mode_range,
                );
                (tab_line_output, regions, segments)
            };
            let hover_col = client
                .hover_at
                .filter(|(hover_slot, _)| *hover_slot == slot_id)
                .map(|(_, col)| col);
            let mut hovered = hover_col.and(client.hovered);
            let (mut tab_line_output, mut regions, mut segments) = compose(hovered);
            if let Some(col) = hover_col {
                let under_mouse =
                    region_at(&regions, col).filter(|region| hoverable(client, region));
                if under_mouse != hovered {
                    hovered = under_mouse;
                    (tab_line_output, regions, segments) = compose(hovered);
                }
            }
            let mode_info = client.mode_info.clone();
            if let Some(client) = self.clients.get_mut(&client_id) {
                if hover_col.is_some() {
                    client.hovered = hovered;
                }
                if let Some(drag) = client.drag.as_mut() {
                    drag.laid_out();
                }
            }
            let slot_client = self.slot_clients.entry((slot_id, client_id)).or_default();
            slot_client.cols = cols;
            slot_client.tab_line = tab_line_output.parts;
            slot_client.breadcrumb_range = tab_line_output.breadcrumb_range;
            slot_client.regions = regions;
            slot_client.segments = segments;

            let output = slot_client
                .tab_line
                .iter()
                .fold(String::new(), |acc, part| acc + &part.part);

            render_background_with_text(&mode_info, &output);
        }
    }
}

fn update_display_area(client: &mut ClientState, tabs: &[TabInfo]) {
    for tab in tabs {
        if tab.active {
            client.display_area_rows = tab.display_area_rows;
            client.display_area_cols = tab.display_area_columns;
            break;
        }
    }
}

fn is_restricted_mode(mode: InputMode) -> bool {
    matches!(
        mode,
        InputMode::Locked
            | InputMode::EnterSearch
            | InputMode::RenameTab
            | InputMode::RenamePane
            | InputMode::Prompt
            | InputMode::Tmux
    )
}

fn create_persist_args() -> BTreeMap<String, String> {
    let mut args = BTreeMap::new();
    args.insert("persist".to_string(), String::new());
    args
}

fn render_background_with_text(mode_info: &ModeInfo, text: &str) {
    let background = mode_info.style.colors.text_unselected.background;
    match background {
        PaletteColor::Rgb((r, g, b)) => {
            print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", text, r, g, b);
        },
        PaletteColor::EightBit(color) => {
            print!("{}\u{1b}[48;5;{}m\u{1b}[0K", text, color);
        },
    }
}

fn prepare_tab_data(client: &ClientState, hovered: Option<CompactRegion>) -> TabRenderData {
    let mut all_tabs = Vec::new();
    let mut active_tab_index = 0;
    let mut active_swap_layout_name = None;
    let mut is_swap_layout_dirty = false;
    let mut is_alternate_tab = false;
    let dimmed = client.mode_info.session_ascended == Some(true)
        || client.mode_info.session_dimmed == Some(true);

    for tab in &client.tabs {
        let tab_name = get_tab_display_name(client, tab);

        if tab.active {
            active_tab_index = tab.position;
            if client.mode_info.mode != InputMode::RenameTab {
                is_swap_layout_dirty = tab.is_swap_layout_dirty;
                active_swap_layout_name = tab.active_swap_layout_name.clone();
            }
        }

        let is_hovered = hovered == Some(CompactRegion::Tab(tab.position));
        let is_dragged = client
            .drag
            .as_ref()
            .map(|drag| drag.tab_id == tab.tab_id)
            .unwrap_or(false);
        let styled_tab = tab_style(
            tab_name,
            tab,
            is_alternate_tab,
            is_hovered,
            is_dragged,
            client.mode_info.style.colors,
            client.mode_info.capabilities,
            dimmed,
        );

        is_alternate_tab = !is_alternate_tab;
        all_tabs.push(styled_tab);
    }

    TabRenderData {
        tabs: all_tabs,
        active_tab_index,
        active_swap_layout_name,
        is_swap_layout_dirty,
    }
}

fn get_tab_display_name(client: &ClientState, tab: &TabInfo) -> String {
    let mut tab_name = tab.name.clone();
    if tab.active && client.mode_info.mode == InputMode::RenameTab && tab_name.is_empty() {
        tab_name = "Enter name...".to_string();
    }
    tab_name
}

fn bind_toggle_key_config(toggle_key: &str, client_id: ClientId) -> String {
    format!(
        r#"
        keybinds {{
            shared {{
                bind "{}" {{
                  MessagePlugin "compact-bar" {{
                      name "toggle_tooltip"
                      tooltip "{}"
                      payload "{}"
                  }}
                }}
            }}
        }}
    "#,
        toggle_key, toggle_key, client_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(text: &str, tab_index: Option<usize>) -> LinePart {
        LinePart {
            part: text.to_owned(),
            len: text.chars().count(),
            tab_index,
        }
    }

    fn client_with_tabs(ids: &[usize], active: usize, mode: InputMode) -> ClientState {
        ClientState {
            tabs: ids
                .iter()
                .enumerate()
                .map(|(position, id)| TabInfo {
                    position,
                    tab_id: *id,
                    active: position == active,
                    ..Default::default()
                })
                .collect(),
            active_tab_idx: active + 1,
            mode_info: ModeInfo {
                mode,
                base_mode: Some(InputMode::Normal),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn bar(client: ClientState) -> CompactBar {
        let mut bar = CompactBar::default();
        bar.slots.insert(0, SlotConfig::default());
        bar.clients.insert(1, client);
        let line = vec![
            part(" Zellij ", None),
            part(" PANE ", None),
            part("<1", Some(0)),
            part("T1", Some(1)),
            part("T2", Some(2)),
        ];
        let rendered = vec!["T0".to_owned(), "T1".to_owned(), "T2".to_owned()];
        let (regions, segments) = compact_regions(&line, &rendered, 1, None, Some((8, 14)));
        bar.slot_clients.insert(
            (0, 1),
            SlotClientState {
                tab_line: line,
                regions,
                segments,
                cols: 100,
                ..Default::default()
            },
        );
        bar
    }

    #[test]
    fn compact_regions_cover_mode_markers_and_tabs() {
        let bar = bar(client_with_tabs(&[10, 11, 12], 1, InputMode::Pane));
        let regions = &bar.slot_clients[&(0, 1)].regions;
        assert_eq!(region_at(regions, 9), Some(CompactRegion::Mode));
        assert_eq!(
            region_at(regions, 14),
            Some(CompactRegion::Marker {
                left: true,
                position: 0
            })
        );
        assert_eq!(region_at(regions, 16), Some(CompactRegion::Tab(1)));
        assert_eq!(region_at(regions, 18), Some(CompactRegion::Tab(2)));
        assert_eq!(bar.slot_clients[&(0, 1)].segments.len(), 2);
    }

    #[test]
    fn the_mode_label_returns_to_the_base_mode_only_outside_it() {
        let mut mode_info = ModeInfo {
            mode: InputMode::Pane,
            base_mode: Some(InputMode::Normal),
            ..Default::default()
        };
        assert_eq!(mode_click_target(&mode_info), Some(InputMode::Normal));
        mode_info.mode = InputMode::Normal;
        assert_eq!(mode_click_target(&mode_info), None);
        mode_info.base_mode = Some(InputMode::Locked);
        assert_eq!(mode_click_target(&mode_info), Some(InputMode::Locked));
    }

    #[test]
    fn hover_ignores_the_active_tab_and_the_mode_label_in_the_base_mode() {
        let mut bar = bar(client_with_tabs(&[10, 11, 12], 1, InputMode::Normal));
        assert!(!bar.handle_mouse_event(0, 1, &Mouse::Hover(0, 9)));
        assert!(!bar.handle_mouse_event(0, 1, &Mouse::Hover(0, 16)));
        assert!(bar.handle_mouse_event(0, 1, &Mouse::Hover(0, 18)));
        assert_eq!(bar.clients[&1].hovered, Some(CompactRegion::Tab(2)));
        assert!(bar.handle_mouse_event(0, 1, &Mouse::Hover(0, 300)));
        assert_eq!(bar.clients[&1].hovered, None);
        bar.clients.get_mut(&1).unwrap().mode_info.mode = InputMode::Pane;
        assert!(bar.handle_mouse_event(0, 1, &Mouse::Hover(0, 9)));
        assert_eq!(bar.clients[&1].hovered, Some(CompactRegion::Mode));
    }

    #[test]
    fn dragging_a_compact_tab_moves_it_once_per_layout() {
        let mut bar = bar(client_with_tabs(&[10, 11, 12], 1, InputMode::Normal));
        assert!(bar.handle_mouse_event(0, 1, &Mouse::LeftClick(0, 16)));
        assert_eq!(bar.clients[&1].drag.as_ref().map(|d| d.tab_id), Some(11));
        bar.handle_mouse_event(0, 1, &Mouse::Hold(0, 19));
        assert_eq!(
            bar.clients[&1].drag.as_ref().and_then(|d| d.pending_target),
            Some(2)
        );
        let moved = client_with_tabs(&[10, 12, 11], 2, InputMode::Normal).tabs;
        bar.handle_tab_update(1, &moved);
        assert_eq!(
            bar.clients[&1].drag.as_ref().and_then(|d| d.pending_target),
            None
        );
        assert!(bar.handle_mouse_event(0, 1, &Mouse::Release(0, 19)));
        assert!(bar.clients[&1].drag.is_none());
    }

    #[test]
    fn a_resting_mouse_over_the_tooltip_keeps_its_hover_after_a_redraw() {
        let mut bar = bar(client_with_tabs(&[10], 0, InputMode::Resize));
        bar.slots.insert(
            5,
            SlotConfig {
                is_tooltip: true,
                ..Default::default()
            },
        );
        let increase = vec![actions::Action::Resize {
            resize: Resize::Increase,
            direction: None,
        }];
        bar.clients.get_mut(&1).unwrap().mode_info.keybinds = vec![(
            InputMode::Resize,
            vec![(KeyWithModifier::new(BareKey::Char('+')), increase.clone())],
        )];
        bar.render_client(10, 40, 5, 1);
        let region = bar.slot_clients[&(5, 1)].tooltip_regions[0].clone();
        bar.handle_mouse_event(
            5,
            1,
            &Mouse::Hover(region.row as isize, region.region.start),
        );
        bar.handle_mouse_event(
            5,
            1,
            &Mouse::LeftClick(region.row as isize, region.region.start),
        );
        bar.clients.get_mut(&1).unwrap().tooltip_hovered = None;
        bar.render_client(10, 40, 5, 1);
        assert_eq!(bar.clients[&1].tooltip_hovered, Some(increase));
    }

    #[test]
    fn tooltip_clicks_and_hover_use_the_stored_tooltip_regions() {
        let mut bar = bar(client_with_tabs(&[10], 0, InputMode::Resize));
        bar.slots.insert(
            5,
            SlotConfig {
                is_tooltip: true,
                ..Default::default()
            },
        );
        let actions = vec![actions::Action::Resize {
            resize: Resize::Increase,
            direction: None,
        }];
        bar.slot_clients.entry((5, 1)).or_default().cols = 100;
        bar.slot_clients.entry((5, 1)).or_default().tooltip_regions = vec![TooltipRegion {
            row: 2,
            region: crate::click_actions::ClickRegion::new(4, 5, actions.clone()),
        }];
        assert!(bar.handle_mouse_event(5, 1, &Mouse::Hover(2, 4)));
        assert_eq!(bar.clients[&1].tooltip_hovered, Some(actions.clone()));
        assert!(!bar.handle_mouse_event(5, 1, &Mouse::Hover(2, 4)));
        assert!(bar.handle_mouse_event(5, 1, &Mouse::Hover(1, 4)));
        assert_eq!(bar.clients[&1].tooltip_hovered, None);
        bar.handle_mouse_event(5, 1, &Mouse::LeftClick(2, 4));
        assert_eq!(bar.runner.pending_for(1), 0);
    }
}

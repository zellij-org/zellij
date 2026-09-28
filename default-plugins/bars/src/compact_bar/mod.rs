mod action_types;
mod clipboard_utils;
mod keybind_utils;
mod line;
mod tab;
mod tooltip;

use std::cmp::{max, min};
use std::collections::{BTreeMap, BTreeSet};
use std::convert::TryInto;

use tab::get_tab_to_focus;
use zellij_tile::prelude::*;

use crate::keybinds::KeybindStore;
use crate::ClientSeed;
use clipboard_utils::{system_clipboard_error, text_copied_hint};
use line::tab_line;
use tab::tab_style;
use tooltip::TooltipRenderer;

static ARROW_SEPARATOR: &str = "";

const CONFIG_IS_TOOLTIP: &str = "is_tooltip";
const CONFIG_TOGGLE_TOOLTIP_KEY: &str = "tooltip";
const MSG_TOGGLE_TOOLTIP: &str = "toggle_tooltip";
const MSG_TOGGLE_PERSISTED_TOOLTIP: &str = "toggle_persisted_tooltip";
const MSG_LAUNCH_TOOLTIP: &str = "launch_tooltip_if_not_launched";

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

#[derive(Default)]
struct SlotConfig {
    config: BTreeMap<String, String>,
    is_tooltip: bool,
    toggle_tooltip_key: Option<String>,
    persist: bool,
    is_first_run: bool,
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
}

#[derive(Default)]
struct SlotClientState {
    tab_line: Vec<LinePart>,
    breadcrumb_range: Option<(usize, usize)>,
}

#[derive(Default)]
pub struct CompactBar {
    slots: BTreeMap<SlotId, SlotConfig>,
    slot_tabs: BTreeMap<SlotId, usize>,
    clients: BTreeMap<ClientId, ClientState>,
    slot_clients: BTreeMap<(SlotId, ClientId), SlotClientState>,
    tooltip_is_active: bool,
    configured_toggle_keys: BTreeSet<(String, ClientId)>,
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
            ]
        };
        subscribe(&events);
        self.slots.insert(
            slot.id,
            SlotConfig {
                config,
                is_tooltip,
                toggle_tooltip_key,
                persist: false,
                is_first_run: is_tooltip,
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
                    self.handle_mouse_event(slot_id, client_id, mouse_event);
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

    fn handle_mouse_event(&mut self, slot_id: SlotId, client_id: ClientId, mouse_event: &Mouse) {
        if self
            .slots
            .get(&slot_id)
            .map(|s| s.is_tooltip)
            .unwrap_or(true)
        {
            return;
        }
        let Some(client) = self.clients.get(&client_id) else {
            return;
        };
        let empty_slot_client = SlotClientState::default();
        let slot_client = self
            .slot_clients
            .get(&(slot_id, client_id))
            .unwrap_or(&empty_slot_client);

        match mouse_event {
            Mouse::LeftClick(_, col) => {
                let col = *col;
                if matches!(slot_client.breadcrumb_range, Some((start, end)) if col >= start && col < end)
                {
                    focus_host_session();
                } else if let Some(tab_idx) =
                    get_tab_to_focus(&slot_client.tab_line, client.active_tab_idx, col)
                {
                    switch_tab_to(tab_idx.try_into().unwrap());
                }
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
            tooltip_renderer.render(rows, cols);
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

            let tab_data = prepare_tab_data(client);
            let tab_line_output = tab_line(
                &client.mode_info,
                tab_data,
                cols,
                slot.toggle_tooltip_key.clone(),
                self.tooltip_is_active,
            );
            let slot_client = self.slot_clients.entry((slot_id, client_id)).or_default();
            slot_client.tab_line = tab_line_output.parts;
            slot_client.breadcrumb_range = tab_line_output.breadcrumb_range;

            let output = slot_client
                .tab_line
                .iter()
                .fold(String::new(), |acc, part| acc + &part.part);

            render_background_with_text(&client.mode_info, &output);
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

fn prepare_tab_data(client: &ClientState) -> TabRenderData {
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

        let styled_tab = tab_style(
            tab_name,
            tab,
            is_alternate_tab,
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

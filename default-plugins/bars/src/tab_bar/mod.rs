mod line;
mod tab;

use std::cmp::{max, min};
use std::collections::{BTreeMap, VecDeque};
use std::convert::TryInto;

use tab::get_tab_to_focus;
use zellij_tile::prelude::*;

use crate::keybinds::KeybindStore;
use crate::ClientSeed;
use line::tab_line;
use tab::tab_style;

#[derive(Debug, Default)]
pub struct LinePart {
    part: String,
    len: usize,
    tab_index: Option<usize>,
}

impl LinePart {
    pub fn append(&mut self, to_append: &LinePart) {
        self.part.push_str(&to_append.part);
        self.len += to_append.len;
    }
}

static ARROW_SEPARATOR: &str = "";

#[derive(Debug, Default)]
struct SlotConfig {
    hide_swap_layout_indication: bool,
}

#[derive(Debug, Default)]
struct ClientState {
    tabs: Vec<TabInfo>,
    active_tab_idx: usize,
    mode_info: ModeInfo,
    active_pane_scroll: Option<(usize, usize)>,
    hovered_tab_idx: Option<usize>,
    hovered_new_tab_button: bool,
    hint_text: Option<BTreeMap<usize, StyledText>>,
    outstanding_hint_timeouts: usize,
}

#[derive(Debug, Default)]
struct SlotClientState {
    tab_line: Vec<LinePart>,
    new_tab_button_range: Option<(usize, usize)>,
    breadcrumb_range: Option<(usize, usize)>,
}

#[derive(Debug, Default)]
pub struct TabBar {
    slots: BTreeMap<SlotId, SlotConfig>,
    clients: BTreeMap<ClientId, ClientState>,
    slot_clients: BTreeMap<(SlotId, ClientId), SlotClientState>,
    hint_timeout_queue: VecDeque<ClientId>,
}

impl TabBar {
    pub fn has_slots(&self) -> bool {
        !self.slots.is_empty()
    }

    pub fn slot_added(&mut self, slot: &Slot) {
        let hide_swap_layout_indication = slot
            .configuration
            .get("hide_swap_layout_indication")
            .map(|s| s == "true")
            .unwrap_or(false);
        self.slots.insert(
            slot.id,
            SlotConfig {
                hide_swap_layout_indication,
            },
        );
        subscribe(&[
            EventType::TabUpdate,
            EventType::ModeUpdate,
            EventType::Mouse,
            EventType::InitialKeybinds,
            EventType::ActivePaneScroll,
            EventType::HintText,
            EventType::Timer,
            EventType::InputReceived,
        ]);
    }

    pub fn slot_removed(&mut self, slot_id: SlotId) {
        self.slots.remove(&slot_id);
        self.slot_clients.retain(|(s, _), _| *s != slot_id);
    }

    pub fn client_connected(&mut self, client_id: ClientId) {
        self.clients.entry(client_id).or_default();
    }

    pub fn client_disconnected(&mut self, client_id: ClientId) {
        self.clients.remove(&client_id);
        self.slot_clients.retain(|(_, c), _| *c != client_id);
    }

    pub fn client_ids(&self) -> impl Iterator<Item = ClientId> + '_ {
        self.clients.keys().copied()
    }

    pub fn reset(&mut self) {
        self.clients.clear();
        self.slot_clients.clear();
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
            if let Some(active_tab_index) = seed.tabs.iter().position(|t| t.active) {
                client.active_tab_idx = active_tab_index + 1;
                client.tabs = seed.tabs.clone();
            }
        }
    }

    fn target_clients(&self, context: &EventContext) -> Vec<ClientId> {
        match context.client_id {
            Some(client_id) => vec![client_id],
            None => self.clients.keys().copied().collect(),
        }
    }

    fn render_request(&self, context: &EventContext, should_render: bool) -> Render {
        if !should_render {
            Render::Nothing
        } else if let Some(client_id) = context.client_id {
            Render::Client(client_id)
        } else {
            Render::Slots(self.slots.keys().copied().collect())
        }
    }

    pub fn update(&mut self, event: &Event, context: EventContext) -> Render {
        if let Event::Timer(_) = event {
            let Some(client_id) = self.hint_timeout_queue.pop_front() else {
                return Render::Nothing;
            };
            let Some(client) = self.clients.get_mut(&client_id) else {
                return Render::Nothing;
            };
            let mut should_render = false;
            client.outstanding_hint_timeouts = client.outstanding_hint_timeouts.saturating_sub(1);
            if client.outstanding_hint_timeouts == 0 && client.hint_text.is_some() {
                client.hint_text = None;
                should_render = true;
            }
            if client.tabs.is_empty() {
                should_render = false;
            }
            return if should_render {
                Render::Client(client_id)
            } else {
                Render::Nothing
            };
        }
        let mut should_render = false;
        for client_id in self.target_clients(&context) {
            if self.update_client(event, client_id, context.slot_id) {
                should_render = true;
            }
        }
        self.render_request(&context, should_render)
    }

    fn update_client(
        &mut self,
        event: &Event,
        client_id: ClientId,
        slot_id: Option<SlotId>,
    ) -> bool {
        let client = self.clients.entry(client_id).or_default();
        let empty_slot_client = SlotClientState::default();
        let slot_client = slot_id
            .and_then(|slot_id| self.slot_clients.get(&(slot_id, client_id)))
            .unwrap_or(&empty_slot_client);
        let mut should_render = false;
        match event {
            Event::InitialKeybinds(_) => {
                should_render = true;
            },
            Event::ModeUpdate(mode_info) => {
                if &client.mode_info != mode_info {
                    should_render = true;
                }
                client.mode_info = mode_info.clone();
            },
            Event::TabUpdate(tabs) => {
                if let Some(active_tab_index) = tabs.iter().position(|t| t.active) {
                    let active_tab_idx = active_tab_index + 1;

                    if client.active_tab_idx != active_tab_idx || &client.tabs != tabs {
                        should_render = true;
                    }
                    client.active_tab_idx = active_tab_idx;
                    client.tabs = tabs.clone();
                } else {
                    eprintln!("Could not find active tab.");
                }
            },
            Event::ActivePaneScroll(scroll) => {
                if client.active_pane_scroll != *scroll {
                    should_render = true;
                }
                client.active_pane_scroll = *scroll;
            },
            Event::HintText(hint_variants) => {
                if hint_variants.is_empty() {
                    if client.hint_text.is_some() {
                        client.hint_text = None;
                        should_render = true;
                    }
                } else {
                    client.hint_text = Some(hint_variants.clone());
                    client.outstanding_hint_timeouts += 1;
                    self.hint_timeout_queue.push_back(client_id);
                    set_timeout(5.0);
                    should_render = true;
                }
            },
            Event::InputReceived => {
                if client.hint_text.is_some() {
                    client.hint_text = None;
                    should_render = true;
                }
            },
            Event::Mouse(me) => match me {
                Mouse::LeftClick(_, col) => {
                    let col = *col;
                    if let Some((start, end)) = slot_client.breadcrumb_range {
                        if col >= start && col < end {
                            focus_host_session();
                            return should_render;
                        }
                    }
                    if let Some((start, end)) = slot_client.new_tab_button_range {
                        if col >= start && col < end {
                            new_tab::<&str>(None, None);
                            return should_render;
                        }
                    }
                    let tab_to_focus =
                        get_tab_to_focus(&slot_client.tab_line, client.active_tab_idx, col);
                    if let Some(idx) = tab_to_focus {
                        switch_tab_to(idx.try_into().unwrap());
                    }
                },
                Mouse::Hover(_, col) => {
                    let col = *col;
                    let simplified_ui = client.mode_info.capabilities.arrow_fonts;
                    let mut new_hovered_new_tab_button = false;
                    let mut new_hovered_tab_idx = None;
                    if !simplified_ui {
                        if let Some((start, end)) = slot_client.new_tab_button_range {
                            if col >= start && col < end {
                                new_hovered_new_tab_button = true;
                            }
                        }
                        if !new_hovered_new_tab_button {
                            new_hovered_tab_idx = get_tab_to_focus(
                                &slot_client.tab_line,
                                client.active_tab_idx,
                                col,
                            );
                        }
                    }
                    if client.hovered_new_tab_button != new_hovered_new_tab_button
                        || client.hovered_tab_idx != new_hovered_tab_idx
                    {
                        client.hovered_new_tab_button = new_hovered_new_tab_button;
                        client.hovered_tab_idx = new_hovered_tab_idx;
                        should_render = true;
                    }
                },
                Mouse::ScrollUp(_) => {
                    switch_tab_to(min(client.active_tab_idx + 1, client.tabs.len()) as u32);
                },
                Mouse::ScrollDown(_) => {
                    switch_tab_to(max(client.active_tab_idx.saturating_sub(1), 1) as u32);
                },
                _ => {},
            },
            _ => {},
        }
        if client.tabs.is_empty() {
            should_render = false;
        }
        should_render
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

    fn swap_keybinds(&mut self, client_id: ClientId, keybinds: &mut KeybindStore) -> bool {
        match self.clients.get_mut(&client_id) {
            Some(client) => keybinds.swap(client_id, &mut client.mode_info.keybinds),
            None => false,
        }
    }

    fn render_client(&mut self, _rows: usize, cols: usize, slot_id: SlotId, client_id: ClientId) {
        let Some(client) = self.clients.get(&client_id) else {
            return;
        };
        if client.tabs.is_empty() {
            return;
        }
        let hide_swap_layout_indication = self
            .slots
            .get(&slot_id)
            .map(|s| s.hide_swap_layout_indication)
            .unwrap_or(false);
        let dimmed = client.mode_info.session_ascended == Some(true)
            || client.mode_info.session_dimmed == Some(true);
        let mut all_tabs: Vec<LinePart> = vec![];
        let mut active_tab_index = 0;
        let mut is_alternate_tab = false;
        for t in &client.tabs {
            let mut tabname = t.name.clone();
            if t.active && client.mode_info.mode == InputMode::RenameTab {
                if tabname.is_empty() {
                    tabname = String::from("Enter name...");
                }
                active_tab_index = t.position;
            } else if t.active {
                active_tab_index = t.position;
            }
            let is_hovered = client.hovered_tab_idx == Some(t.position + 1);
            let tab = tab_style(
                tabname,
                t,
                is_alternate_tab,
                is_hovered,
                client.mode_info.style.colors,
                client.mode_info.capabilities,
                dimmed,
            );
            is_alternate_tab = !is_alternate_tab;
            all_tabs.push(tab);
        }

        let background = client.mode_info.style.colors.text_unselected.background;

        let full_pane_frames = client.mode_info.pane_frame_style == Some(PaneFrameStyle::Full);
        let hint_text = if full_pane_frames {
            None
        } else {
            client.hint_text.as_ref()
        };
        let breadcrumb_ancestry: Vec<String> = if client.mode_info.host_fullscreen == Some(true) {
            client.mode_info.session_ancestry.clone()
        } else {
            vec![]
        };
        let (line, new_tab_button_range, breadcrumb_range) = tab_line(
            client.mode_info.session_name.as_deref(),
            all_tabs,
            active_tab_index,
            cols.saturating_sub(1),
            client.mode_info.style.colors,
            client.mode_info.capabilities,
            client.mode_info.style.hide_session_name,
            client.tabs.iter().find(|t| t.active),
            &client.mode_info,
            hide_swap_layout_indication,
            &background,
            client.active_pane_scroll,
            hint_text,
            is_alternate_tab,
            client.hovered_new_tab_button,
            dimmed,
            &breadcrumb_ancestry,
        );
        let slot_client = self.slot_clients.entry((slot_id, client_id)).or_default();
        slot_client.tab_line = line;
        slot_client.new_tab_button_range = new_tab_button_range;
        slot_client.breadcrumb_range = breadcrumb_range;

        let output = slot_client
            .tab_line
            .iter()
            .fold(String::new(), |output, part| output + &part.part);

        match background {
            PaletteColor::Rgb((r, g, b)) => {
                print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", output, r, g, b);
            },
            PaletteColor::EightBit(color) => {
                print!("{}\u{1b}[48;5;{}m\u{1b}[0K", output, color);
            },
        }
    }
}

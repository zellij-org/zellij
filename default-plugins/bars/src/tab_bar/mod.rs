mod line;
mod tab;

use std::cmp::{max, min};
use std::collections::{BTreeMap, VecDeque};

use tab::get_clicked_line_part;
use zellij_tile::prelude::*;

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
use line::{tab_line, TabLineHover};
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

fn in_range(range: Option<(usize, usize)>, col: usize) -> bool {
    matches!(range, Some((start, end)) if col >= start && col < end)
}

fn click_target(slot_client: &SlotClientState, col: usize) -> ClickTarget {
    let in_reserved_range = in_range(slot_client.breadcrumb_range, col)
        || in_range(slot_client.new_tab_button_range, col);
    let clicked_part = get_clicked_line_part(&slot_client.tab_line, col)
        .map(|line_part| (line_part.tab_index, line_part.part.as_str()));
    classify_click(clicked_part, in_reserved_range)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabBarRegion {
    Tab(usize),
    Marker { left: bool, position: usize },
    NewTabButton,
    Breadcrumb,
}

fn region_at(regions: &[(usize, usize, TabBarRegion)], col: usize) -> Option<TabBarRegion> {
    regions
        .iter()
        .find(|(start, end, _)| col >= *start && col < *end)
        .map(|(_, _, region)| *region)
}

pub(crate) fn line_regions(
    line: &[LinePart],
    tab_parts: &[String],
    active_tab_position: usize,
    new_tab_button_range: Option<(usize, usize)>,
    breadcrumb_range: Option<(usize, usize)>,
) -> (Vec<(usize, usize, TabBarRegion)>, Vec<TabSegment>) {
    let mut regions = vec![];
    if let Some((start, end)) = breadcrumb_range {
        regions.push((start, end, TabBarRegion::Breadcrumb));
    }
    let parts = tab_parts_in_line(
        line.iter()
            .map(|part| (part.len, part.tab_index, part.part.as_str())),
        tab_parts,
        active_tab_position,
    );
    let segments = segments_of(&parts);
    for (start, end, kind) in parts {
        regions.push((
            start,
            end,
            match kind {
                TabPart::Tab(position) => TabBarRegion::Tab(position),
                TabPart::Marker { left, position } => TabBarRegion::Marker { left, position },
            },
        ));
    }
    if let Some((start, end)) = new_tab_button_range {
        regions.push((start, end, TabBarRegion::NewTabButton));
    }
    (regions, segments)
}

#[derive(Debug, Default)]
struct SlotConfig {
    hide_swap_layout_indication: bool,
    double_click: DoubleClickConfig,
}

#[derive(Debug, Default)]
struct ClientState {
    last_left_click: Option<(Option<SlotId>, ClickTarget)>,
    tabs: Vec<TabInfo>,
    active_tab_idx: usize,
    mode_info: ModeInfo,
    active_pane_scroll: Option<(usize, usize)>,
    hovered: Option<TabBarRegion>,
    hover_at: Option<(SlotId, usize)>,
    drag: Option<TabDrag>,
    switch_when_settled: Option<usize>,
    hint_text: Option<BTreeMap<usize, StyledText>>,
    outstanding_hint_timeouts: usize,
}

#[derive(Debug, Default)]
struct SlotClientState {
    tab_line: Vec<LinePart>,
    new_tab_button_range: Option<(usize, usize)>,
    breadcrumb_range: Option<(usize, usize)>,
    regions: Vec<(usize, usize, TabBarRegion)>,
    segments: Vec<TabSegment>,
    cols: usize,
}

struct ComposedLine {
    line: Vec<LinePart>,
    new_tab_button_range: Option<(usize, usize)>,
    breadcrumb_range: Option<(usize, usize)>,
    regions: Vec<(usize, usize, TabBarRegion)>,
    segments: Vec<TabSegment>,
    background: PaletteColor,
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
                double_click: DoubleClickConfig::from_configuration(&slot.configuration),
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
            EventType::ActionComplete,
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

    fn render_request(&self, context: &EventContext, should_render: bool) -> RenderResponse {
        if !should_render {
            RenderResponse::Nothing
        } else if let Some(client_id) = context.client_id {
            RenderResponse::Client(client_id)
        } else {
            RenderResponse::Slots(self.slots.keys().copied().collect())
        }
    }

    pub fn update(&mut self, event: &Event, context: EventContext) -> RenderResponse {
        if let Event::Timer(_) = event {
            let Some(client_id) = self.hint_timeout_queue.pop_front() else {
                return RenderResponse::Nothing;
            };
            let Some(client) = self.clients.get_mut(&client_id) else {
                return RenderResponse::Nothing;
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
                RenderResponse::Client(client_id)
            } else {
                RenderResponse::Nothing
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
        let double_click_config = slot_id
            .and_then(|slot_id| self.slots.get(&slot_id))
            .map(|slot| slot.double_click)
            .unwrap_or_default();
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
                    if let Some(drag) = client.drag.as_mut() {
                        drag.tabs_updated(tabs);
                    }
                    if let Some(tab_id) = client.switch_when_settled.take() {
                        apply_release(switch_outcome(tabs, tab_id));
                    }
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
            event if is_tab_move_completion(event) => {
                if let Some(drag) = client.drag.as_mut() {
                    if drag.move_completed() {
                        should_render = true;
                    }
                }
                if let Some(tab_id) = client.switch_when_settled.take() {
                    apply_release(switch_outcome(&client.tabs, tab_id));
                }
            },
            Event::Mouse(me) => match me {
                Mouse::LeftClick(_, col) => {
                    let col = *col;
                    if let Some(slot_id) = slot_id {
                        client.hover_at = Some((slot_id, col));
                    }
                    client.last_left_click = Some((slot_id, click_target(slot_client, col)));
                    client.drag = None;
                    match region_at(&slot_client.regions, col) {
                        Some(TabBarRegion::Breadcrumb) => {
                            focus_host_session();
                        },
                        Some(TabBarRegion::NewTabButton) => {
                            new_tab::<&str>(None, None);
                        },
                        Some(TabBarRegion::Marker { position, .. }) => {
                            switch_tab_to(position as u32 + 1);
                        },
                        Some(TabBarRegion::Tab(position)) => {
                            if let Some(tab) = client.tabs.iter().find(|t| t.position == position) {
                                client.drag = Some(TabDrag::new(slot_id, tab.tab_id));
                                should_render = true;
                            }
                        },
                        None => {},
                    }
                },
                Mouse::Hold(_, col) => {
                    if let Some(slot_id) = slot_id {
                        client.hover_at = Some((slot_id, *col));
                    }
                    if let Some(drag) = client.drag.as_mut().filter(|drag| drag.slot_id == slot_id)
                    {
                        if let Some((tab_id, target)) =
                            drag.hold(&client.tabs, &slot_client.segments, *col)
                        {
                            request_tab_move(tab_id, target);
                        }
                    }
                },
                Mouse::Release(_, col) => {
                    if let Some(slot_id) = slot_id {
                        client.hover_at = Some((slot_id, *col));
                    }
                    if let Some(drag) = client.drag.take() {
                        client.switch_when_settled = apply_release(drag.release(&client.tabs));
                        should_render = true;
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
                    let outcome =
                        double_click_outcome(&double_click_config, last_left_click, target);
                    apply_double_click_outcome(outcome, &client.tabs, client.active_tab_idx);
                },
                Mouse::Hover(_, col) => {
                    if let Some(slot_id) = slot_id {
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
                            let hovered =
                                region_at(&slot_client.regions, col).filter(|_| on_screen);
                            if client.hovered != hovered {
                                client.hovered = hovered;
                                should_render = true;
                            }
                        }
                    }
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

    fn compose_line(
        &self,
        cols: usize,
        slot_id: SlotId,
        client_id: ClientId,
        hovered: Option<TabBarRegion>,
    ) -> Option<ComposedLine> {
        let client = self.clients.get(&client_id)?;
        if client.tabs.is_empty() {
            return None;
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
            let is_hovered = !t.active && hovered == Some(TabBarRegion::Tab(t.position));
            let is_dragged = client
                .drag
                .as_ref()
                .map(|drag| drag.tab_id == t.tab_id)
                .unwrap_or(false);
            let tab = tab_style(
                tabname,
                t,
                is_alternate_tab,
                is_hovered,
                is_dragged,
                client.mode_info.style.colors,
                client.mode_info.capabilities,
                &client.mode_info,
            );
            is_alternate_tab = !is_alternate_tab;
            all_tabs.push(tab);
        }

        let tab_parts: Vec<String> = all_tabs.iter().map(|t| t.part.clone()).collect();
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
            TabLineHover {
                new_tab_button: hovered == Some(TabBarRegion::NewTabButton),
                breadcrumb: hovered == Some(TabBarRegion::Breadcrumb),
            },
            dimmed,
            &breadcrumb_ancestry,
        );
        let (regions, segments) = line_regions(
            &line,
            &tab_parts,
            active_tab_index,
            new_tab_button_range,
            breadcrumb_range,
        );
        Some(ComposedLine {
            line,
            new_tab_button_range,
            breadcrumb_range,
            regions,
            segments,
            background,
        })
    }

    fn render_client(&mut self, _rows: usize, cols: usize, slot_id: SlotId, client_id: ClientId) {
        let Some(client) = self.clients.get(&client_id) else {
            return;
        };
        let hover_col = client
            .hover_at
            .filter(|(hover_slot, _)| *hover_slot == slot_id)
            .map(|(_, col)| col);
        let mut hovered = hover_col.and(client.hovered);
        let Some(mut composed) = self.compose_line(cols, slot_id, client_id, hovered) else {
            return;
        };
        if let Some(col) = hover_col {
            let under_mouse = region_at(&composed.regions, col);
            if under_mouse != hovered {
                hovered = under_mouse;
                if let Some(recomposed) = self.compose_line(cols, slot_id, client_id, hovered) {
                    composed = recomposed;
                }
            }
        }
        if let Some(client) = self.clients.get_mut(&client_id) {
            if hover_col.is_some() {
                client.hovered = hovered;
            }
            if let Some(drag) = client.drag.as_mut() {
                drag.laid_out();
            }
        }
        let background = composed.background;
        let slot_client = self.slot_clients.entry((slot_id, client_id)).or_default();
        slot_client.tab_line = composed.line;
        slot_client.new_tab_button_range = composed.new_tab_button_range;
        slot_client.breadcrumb_range = composed.breadcrumb_range;
        slot_client.regions = composed.regions;
        slot_client.segments = composed.segments;
        slot_client.cols = cols;

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

    #[test]
    fn regions_tell_tabs_apart_from_the_hidden_tab_markers() {
        let tab_parts = vec![
            "TAB0".to_owned(),
            "TAB1".to_owned(),
            "TAB2".to_owned(),
            "TAB3".to_owned(),
            "TAB4".to_owned(),
        ];
        let line = vec![
            part(" Zellij ", None),
            part("<+1", Some(0)),
            part("TAB1", Some(1)),
            part("TAB2", Some(2)),
            part("+2>", Some(3)),
            part(" + ", None),
        ];
        let (regions, segments) = line_regions(&line, &tab_parts, 2, Some((22, 25)), Some((1, 4)));
        assert_eq!(
            regions,
            vec![
                (1, 4, TabBarRegion::Breadcrumb),
                (
                    8,
                    11,
                    TabBarRegion::Marker {
                        left: true,
                        position: 0
                    }
                ),
                (11, 15, TabBarRegion::Tab(1)),
                (15, 19, TabBarRegion::Tab(2)),
                (
                    19,
                    22,
                    TabBarRegion::Marker {
                        left: false,
                        position: 3
                    }
                ),
                (22, 25, TabBarRegion::NewTabButton),
            ]
        );
        assert_eq!(
            segments,
            vec![
                TabSegment {
                    start: 11,
                    end: 15,
                    position: 1
                },
                TabSegment {
                    start: 15,
                    end: 19,
                    position: 2
                },
            ]
        );
        assert_eq!(region_at(&regions, 2), Some(TabBarRegion::Breadcrumb));
        assert_eq!(region_at(&regions, 12), Some(TabBarRegion::Tab(1)));
        assert_eq!(region_at(&regions, 40), None);
    }

    fn client_with_tabs(ids: &[usize], active: usize) -> ClientState {
        let tabs: Vec<TabInfo> = ids
            .iter()
            .enumerate()
            .map(|(position, id)| TabInfo {
                position,
                tab_id: *id,
                active: position == active,
                name: format!("t{}", id),
                ..Default::default()
            })
            .collect();
        ClientState {
            active_tab_idx: active + 1,
            tabs,
            ..Default::default()
        }
    }

    fn bar_with_line(client: ClientState) -> TabBar {
        let mut bar = TabBar::default();
        bar.clients.insert(1, client);
        let line = vec![
            part("AAAA", Some(0)),
            part("BBBB", Some(1)),
            part("CCCC", Some(2)),
        ];
        let tab_parts: Vec<String> = line.iter().map(|p| p.part.clone()).collect();
        let (regions, segments) = line_regions(&line, &tab_parts, 0, None, None);
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
    fn hovering_highlights_the_region_under_the_mouse_and_leaving_clears_it() {
        let mut bar = bar_with_line(client_with_tabs(&[10, 11, 12], 0));
        assert!(bar.update_client(&Event::Mouse(Mouse::Hover(0, 5)), 1, Some(0)));
        assert_eq!(bar.clients[&1].hovered, Some(TabBarRegion::Tab(1)));
        assert!(!bar.update_client(&Event::Mouse(Mouse::Hover(0, 6)), 1, Some(0)));
        assert!(bar.update_client(&Event::Mouse(Mouse::Hover(0, 500)), 1, Some(0)));
        assert_eq!(bar.clients[&1].hovered, None);
    }

    #[test]
    fn pressing_a_tab_starts_a_drag_and_a_double_click_cancels_it() {
        let mut bar = bar_with_line(client_with_tabs(&[10, 11, 12], 0));
        bar.update_client(&Event::Mouse(Mouse::LeftClick(0, 5)), 1, Some(0));
        assert_eq!(bar.clients[&1].drag.as_ref().map(|d| d.tab_id), Some(11));
        bar.slots.insert(0, SlotConfig::default());
        bar.slots.get_mut(&0).unwrap().double_click.tab =
            crate::double_click::DoubleClickTabAction::Nothing;
        bar.update_client(&Event::Mouse(Mouse::DoubleClick(0, 5)), 1, Some(0));
        assert!(bar.clients[&1].drag.is_none());
    }

    #[test]
    fn a_drag_only_moves_once_per_confirmed_layout() {
        let mut bar = bar_with_line(client_with_tabs(&[10, 11, 12], 0));
        bar.update_client(&Event::Mouse(Mouse::LeftClick(0, 1)), 1, Some(0));
        bar.update_client(&Event::Mouse(Mouse::Hold(0, 5)), 1, Some(0));
        assert_eq!(
            bar.clients[&1].drag.as_ref().and_then(|d| d.pending_target),
            Some(1)
        );
        bar.update_client(&Event::Mouse(Mouse::Hold(0, 9)), 1, Some(0));
        assert_eq!(
            bar.clients[&1].drag.as_ref().and_then(|d| d.pending_target),
            Some(1)
        );
        let moved = client_with_tabs(&[11, 10, 12], 1).tabs;
        bar.update_client(&Event::TabUpdate(moved), 1, Some(0));
        let drag = bar.clients[&1].drag.clone().unwrap();
        assert_eq!(drag.pending_target, None);
        assert!(drag.awaiting_layout);
    }

    #[test]
    fn a_resting_mouse_is_found_again_after_the_tabs_change() {
        let mut bar = bar_with_line(client_with_tabs(&[10, 11, 12], 0));
        bar.slots.insert(0, SlotConfig::default());
        bar.render_client(1, 100, 0, 1);
        let second_tab = bar.slot_clients[&(0, 1)]
            .regions
            .iter()
            .find(|(_, _, region)| *region == TabBarRegion::Tab(1))
            .map(|(start, _, _)| *start)
            .unwrap();
        bar.update_client(&Event::Mouse(Mouse::LeftClick(0, second_tab)), 1, Some(0));
        bar.update_client(&Event::Mouse(Mouse::Release(0, second_tab)), 1, Some(0));
        bar.clients.get_mut(&1).unwrap().hovered = None;
        bar.render_client(1, 100, 0, 1);
        assert_eq!(bar.clients[&1].hovered, Some(TabBarRegion::Tab(1)));
    }

    #[test]
    fn releasing_ends_the_drag() {
        let mut bar = bar_with_line(client_with_tabs(&[10, 11, 12], 0));
        bar.update_client(&Event::Mouse(Mouse::LeftClick(0, 1)), 1, Some(0));
        assert!(bar.update_client(&Event::Mouse(Mouse::Release(0, 1)), 1, Some(0)));
        assert!(bar.clients[&1].drag.is_none());
        assert!(bar.clients[&1].switch_when_settled.is_none());
    }
}

//! `Tab`s holds multiple panes. It tracks their coordinates (x/y) and size,
//! as well as how they should be resized

use zellij_utils::{position::Position, serde, zellij_tile};

use crate::ui::pane_resizer::PaneResizer;
use crate::{
    os_input_output::ServerOsApi,
    panes::{PaneId, PluginPane, TerminalPane},
    pty::{PtyInstruction, VteBytes},
    thread_bus::ThreadSenders,
    ui::boundaries::Boundaries,
    wasm_vm::PluginInstruction,
    ClientId, ServerInstruction,
};
use serde::{Deserialize, Serialize};
use std::os::unix::io::RawFd;
use std::sync::mpsc::channel;
use std::time::Instant;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap, HashSet},
};
use zellij_tile::data::{Event, InputMode, ModeInfo, Palette, PaletteColor};
use zellij_utils::{
    input::{
        layout::{Direction, Layout, Run},
        parse_keys,
    },
    pane_size::{Dimension, Offset, PaneGeom, Size, Viewport},
};

const CURSOR_HEIGHT_WIDTH_RATIO: usize = 4; // this is not accurate and kind of a magic number, TODO: look into this

// FIXME: This should be replaced by `RESIZE_PERCENT` at some point
const MIN_TERMINAL_HEIGHT: usize = 5;
const MIN_TERMINAL_WIDTH: usize = 5;

const RESIZE_PERCENT: f64 = 5.0;

const MAX_PENDING_VTE_EVENTS: usize = 7000;

type BorderAndPaneIds = (usize, Vec<PaneId>);

fn split(direction: Direction, rect: &PaneGeom) -> Option<(PaneGeom, PaneGeom)> {
    let space = match direction {
        Direction::Vertical => rect.cols,
        Direction::Horizontal => rect.rows,
    };
    if let Some(p) = space.as_percent() {
        let first_rect = match direction {
            Direction::Vertical => PaneGeom {
                cols: Dimension::percent(p / 2.0),
                ..*rect
            },
            Direction::Horizontal => PaneGeom {
                rows: Dimension::percent(p / 2.0),
                ..*rect
            },
        };
        let second_rect = match direction {
            Direction::Vertical => PaneGeom {
                x: first_rect.x + 1,
                cols: first_rect.cols,
                ..*rect
            },
            Direction::Horizontal => PaneGeom {
                y: first_rect.y + 1,
                rows: first_rect.rows,
                ..*rect
            },
        };
        Some((first_rect, second_rect))
    } else {
        None
    }
}

fn pane_content_offset(position_and_size: &PaneGeom, viewport: &Viewport) -> (usize, usize) {
    // (columns_offset, rows_offset)
    // if the pane is not on the bottom or right edge on the screen, we need to reserve one space
    // from its content to leave room for the boundary between it and the next pane (if it doesn't
    // draw its own frame)
    let columns_offset = if position_and_size.x + position_and_size.cols.as_usize() < viewport.cols
    {
        1
    } else {
        0
    };
    let rows_offset = if position_and_size.y + position_and_size.rows.as_usize() < viewport.rows {
        1
    } else {
        0
    };
    (columns_offset, rows_offset)
}

#[derive(Clone, Debug, Default)]
pub struct Output {
    pub client_render_instructions: HashMap<ClientId, String>,
}

impl Output {
    pub fn add_clients(&mut self, client_ids: &HashSet<ClientId>) {
        for client_id in client_ids {
            self.client_render_instructions
                .insert(*client_id, String::new());
        }
    }
    pub fn push_str_to_all_clients(&mut self, to_push: &str) {
        for render_instruction in self.client_render_instructions.values_mut() {
            render_instruction.push_str(to_push)
        }
    }
}

pub(crate) struct Tab {
    pub index: usize,
    pub position: usize,
    pub name: String,
    panes: BTreeMap<PaneId, Box<dyn Pane>>,
    pub panes_to_hide: HashSet<PaneId>,
    active_panes: HashMap<ClientId, PaneId>,
    max_panes: Option<usize>,
    viewport: Viewport, // includes all non-UI panes
    display_area: Size, // includes all panes (including eg. the status bar and tab bar in the default layout)
    fullscreen_is_active: bool,
    os_api: Box<dyn ServerOsApi>,
    pub senders: ThreadSenders,
    synchronize_is_active: bool,
    should_clear_display_before_rendering: bool,
    pub mode_info: ModeInfo,
    pub colors: Palette,
    connected_clients: HashSet<ClientId>,
    draw_pane_frames: bool,
    pending_vte_events: HashMap<RawFd, Vec<VteBytes>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(crate = "self::serde")]
pub(crate) struct TabData {
    pub position: usize,
    pub name: String,
    pub active: bool,
    pub mode_info: ModeInfo,
    pub colors: Palette,
}

// FIXME: Use a struct that has a pane_type enum, to reduce all of the duplication
pub trait Pane {
    fn x(&self) -> usize;
    fn y(&self) -> usize;
    fn rows(&self) -> usize;
    fn cols(&self) -> usize;
    fn get_content_x(&self) -> usize;
    fn get_content_y(&self) -> usize;
    fn get_content_columns(&self) -> usize;
    fn get_content_rows(&self) -> usize;
    fn reset_size_and_position_override(&mut self);
    fn set_geom(&mut self, position_and_size: PaneGeom);
    fn get_geom_override(&mut self, pane_geom: PaneGeom);
    fn handle_pty_bytes(&mut self, bytes: VteBytes);
    fn cursor_coordinates(&self) -> Option<(usize, usize)>;
    fn adjust_input_to_terminal(&self, input_bytes: Vec<u8>) -> Vec<u8>;
    fn position_and_size(&self) -> PaneGeom;
    fn current_geom(&self) -> PaneGeom;
    fn geom_override(&self) -> Option<PaneGeom>;
    fn should_render(&self) -> bool;
    fn set_should_render(&mut self, should_render: bool);
    fn set_should_render_boundaries(&mut self, _should_render: bool) {}
    fn selectable(&self) -> bool;
    fn set_selectable(&mut self, selectable: bool);
    fn render(&mut self) -> Option<String>;
    fn pid(&self) -> PaneId;
    fn reduce_height(&mut self, percent: f64);
    fn increase_height(&mut self, percent: f64);
    fn reduce_width(&mut self, percent: f64);
    fn increase_width(&mut self, percent: f64);
    fn push_down(&mut self, count: usize);
    fn push_right(&mut self, count: usize);
    fn pull_left(&mut self, count: usize);
    fn pull_up(&mut self, count: usize);
    fn scroll_up(&mut self, count: usize);
    fn scroll_down(&mut self, count: usize);
    fn clear_scroll(&mut self);
    fn is_scrolled(&self) -> bool;
    fn active_at(&self) -> Instant;
    fn set_active_at(&mut self, instant: Instant);
    fn set_frame(&mut self, frame: bool);
    fn set_content_offset(&mut self, offset: Offset);
    fn cursor_shape_csi(&self) -> String {
        "\u{1b}[0 q".to_string() // default to non blinking block
    }
    fn contains(&self, position: &Position) -> bool {
        match self.geom_override() {
            Some(position_and_size) => position_and_size.contains(position),
            None => self.position_and_size().contains(position),
        }
    }
    fn start_selection(&mut self, _start: &Position) {}
    fn update_selection(&mut self, _position: &Position) {}
    fn end_selection(&mut self, _end: Option<&Position>) {}
    fn reset_selection(&mut self) {}
    fn get_selected_text(&self) -> Option<String> {
        None
    }

    fn right_boundary_x_coords(&self) -> usize {
        self.x() + self.cols()
    }
    fn bottom_boundary_y_coords(&self) -> usize {
        self.y() + self.rows()
    }
    fn is_directly_right_of(&self, other: &dyn Pane) -> bool {
        self.x() == other.x() + other.cols()
    }
    fn is_directly_left_of(&self, other: &dyn Pane) -> bool {
        self.x() + self.cols() == other.x()
    }
    fn is_directly_below(&self, other: &dyn Pane) -> bool {
        self.y() == other.y() + other.rows()
    }
    fn is_directly_above(&self, other: &dyn Pane) -> bool {
        self.y() + self.rows() == other.y()
    }
    fn horizontally_overlaps_with(&self, other: &dyn Pane) -> bool {
        (self.y() >= other.y() && self.y() < (other.y() + other.rows()))
            || ((self.y() + self.rows()) <= (other.y() + other.rows())
                && (self.y() + self.rows()) > other.y())
            || (self.y() <= other.y() && (self.y() + self.rows() >= (other.y() + other.rows())))
            || (other.y() <= self.y() && (other.y() + other.rows() >= (self.y() + self.rows())))
    }
    fn get_horizontal_overlap_with(&self, other: &dyn Pane) -> usize {
        std::cmp::min(self.y() + self.rows(), other.y() + other.rows())
            - std::cmp::max(self.y(), other.y())
    }
    fn vertically_overlaps_with(&self, other: &dyn Pane) -> bool {
        (self.x() >= other.x() && self.x() < (other.x() + other.cols()))
            || ((self.x() + self.cols()) <= (other.x() + other.cols())
                && (self.x() + self.cols()) > other.x())
            || (self.x() <= other.x() && (self.x() + self.cols() >= (other.x() + other.cols())))
            || (other.x() <= self.x() && (other.x() + other.cols() >= (self.x() + self.cols())))
    }
    fn get_vertical_overlap_with(&self, other: &dyn Pane) -> usize {
        std::cmp::min(self.x() + self.cols(), other.x() + other.cols())
            - std::cmp::max(self.x(), other.x())
    }
    fn can_reduce_height_by(&self, reduce_by: usize) -> bool {
        self.rows() > reduce_by && self.rows() - reduce_by >= self.min_height()
    }
    fn can_reduce_width_by(&self, reduce_by: usize) -> bool {
        self.cols() > reduce_by && self.cols() - reduce_by >= self.min_width()
    }
    fn min_width(&self) -> usize {
        MIN_TERMINAL_WIDTH
    }
    fn min_height(&self) -> usize {
        MIN_TERMINAL_HEIGHT
    }
    fn drain_messages_to_pty(&mut self) -> Vec<Vec<u8>> {
        // TODO: this is only relevant to terminal panes
        // we should probably refactor away from this trait at some point
        vec![]
    }
    fn render_full_viewport(&mut self) {}
    fn relative_position(&self, position_on_screen: &Position) -> Position {
        position_on_screen.relative_to(self.get_content_y(), self.get_content_x())
    }
    fn set_boundary_color(&mut self, _color: Option<PaletteColor>) {}
    fn set_borderless(&mut self, borderless: bool);
    fn borderless(&self) -> bool;
}

impl Tab {
    // FIXME: Still too many arguments for clippy to be happy...
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index: usize,
        position: usize,
        name: String,
        display_area: Size,
        os_api: Box<dyn ServerOsApi>,
        senders: ThreadSenders,
        max_panes: Option<usize>,
        mode_info: ModeInfo,
        colors: Palette,
        draw_pane_frames: bool,
        client_id: ClientId,
    ) -> Self {
        let panes = BTreeMap::new();

        let name = if name.is_empty() {
            format!("Tab #{}", index + 1)
        } else {
            name
        };

        let mut connected_clients = HashSet::new();
        connected_clients.insert(client_id);

        Tab {
            index,
            position,
            panes,
            name,
            max_panes,
            panes_to_hide: HashSet::new(),
            active_panes: HashMap::new(),
            viewport: display_area.into(),
            display_area,
            fullscreen_is_active: false,
            synchronize_is_active: false,
            os_api,
            senders,
            should_clear_display_before_rendering: false,
            mode_info,
            colors,
            draw_pane_frames,
            pending_vte_events: HashMap::new(),
            connected_clients,
        }
    }

    pub fn apply_layout(&mut self, layout: Layout, new_pids: Vec<RawFd>, tab_index: usize) {
        // TODO: this should be an attribute on Screen instead of full_screen_ws
        let free_space = PaneGeom::default();
        self.panes_to_hide.clear();
        let positions_in_layout = layout.position_panes_in_space(&free_space);

        let mut positions_and_size = positions_in_layout.iter();
        for (pane_kind, terminal_pane) in self.panes.iter_mut() {
            // for now the layout only supports terminal panes
            if let PaneId::Terminal(pid) = pane_kind {
                match positions_and_size.next() {
                    Some(&(_, position_and_size)) => {
                        terminal_pane.reset_size_and_position_override();
                        terminal_pane.set_geom(position_and_size);
                    }
                    None => {
                        // we filled the entire layout, no room for this pane
                        // TODO: handle active terminal
                        self.panes_to_hide.insert(PaneId::Terminal(*pid));
                    }
                }
            }
        }
        let mut new_pids = new_pids.iter();

        for (layout, position_and_size) in positions_and_size {
            // A plugin pane
            if let Some(Run::Plugin(run)) = layout.run.clone() {
                let (pid_tx, pid_rx) = channel();
                let pane_title = run.location.to_string();
                self.senders
                    .send_to_plugin(PluginInstruction::Load(pid_tx, run, tab_index))
                    .unwrap();
                let pid = pid_rx.recv().unwrap();
                let mut new_plugin = PluginPane::new(
                    pid,
                    *position_and_size,
                    self.senders.to_plugin.as_ref().unwrap().clone(),
                    pane_title,
                );
                new_plugin.set_borderless(layout.borderless);
                self.panes.insert(PaneId::Plugin(pid), Box::new(new_plugin));
                // Send an initial mode update to the newly loaded plugin only!
                self.senders
                    .send_to_plugin(PluginInstruction::Update(
                        Some(pid),
                        Event::ModeUpdate(self.mode_info.clone()),
                    ))
                    .unwrap();
            } else {
                // there are still panes left to fill, use the pids we received in this method
                let pid = new_pids.next().unwrap(); // if this crashes it means we got less pids than there are panes in this layout
                let next_terminal_position = self.get_next_terminal_position();
                let mut new_pane = TerminalPane::new(
                    *pid,
                    *position_and_size,
                    self.colors,
                    next_terminal_position,
                );
                new_pane.set_borderless(layout.borderless);
                self.panes
                    .insert(PaneId::Terminal(*pid), Box::new(new_pane));
            }
        }
        for unused_pid in new_pids {
            // this is a bit of a hack and happens because we don't have any central location that
            // can query the screen as to how many panes it needs to create a layout
            // fixing this will require a bit of an architecture change
            self.senders
                .send_to_pty(PtyInstruction::ClosePane(PaneId::Terminal(*unused_pid)))
                .unwrap();
        }
        // FIXME: This is another hack to crop the viewport to fixed-size panes. Once you can have
        // non-fixed panes that are part of the viewport, get rid of this!
        self.resize_whole_tab(self.display_area);
        let boundary_geom: Vec<_> = self
            .panes
            .values()
            .filter_map(|p| {
                let geom = p.position_and_size();
                if geom.cols.is_fixed() || geom.rows.is_fixed() {
                    Some(geom.into())
                } else {
                    None
                }
            })
            .collect();
        for geom in boundary_geom {
            self.offset_viewport(&geom)
        }
        self.set_pane_frames(self.draw_pane_frames);
        // This is the end of the nasty viewport hack...
        let next_selectable_pane_id = self.panes.iter().filter(|(_id, pane)| pane.selectable()).map(|(id, _)| id.to_owned()).next();
        match next_selectable_pane_id {
            Some(active_pane_id) => {
                let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
                for client_id in connected_clients {
                    self.active_panes.insert(client_id, active_pane_id);
                }
            },
            None => {
                // this is very likely a configuration error (layout with no selectable panes)
                self.active_panes.clear();
            }
        }
    }
    pub fn add_client(&mut self, client_id: ClientId) {
        match self.connected_clients.iter().next() {
            Some(first_client_id) => {
                let first_active_pane_id = *self.active_panes.get(first_client_id).unwrap();
                self.connected_clients.insert(client_id);
                self.active_panes.insert(client_id, first_active_pane_id);
            },
            None => {
                let mut pane_ids: Vec<PaneId> = self.panes.keys().copied().collect();
                if pane_ids.is_empty() {
                    // no panes here, bye bye
                    return;
                }
                pane_ids.sort(); // TODO: make this predictable
                let first_pane_id = pane_ids.iter().next().unwrap();
                self.connected_clients.insert(client_id);
                self.active_panes.insert(client_id, *first_pane_id);
            }

        }
        // TODO: we might be able to avoid this, we do this so that newly connected clients will
        // necessarily get a full render
        self.set_force_render();
    }
    pub fn add_multiple_clients(&mut self, client_ids: &[ClientId]) {
        for client_id in client_ids {
            self.add_client(*client_id);
        }
    }
    pub fn remove_client(&mut self, client_id: ClientId) {
        self.connected_clients.remove(&client_id);
    }
    pub fn drain_connected_clients(&mut self) -> Vec<ClientId> {
        self.connected_clients.drain().collect()
    }
    pub fn new_pane(&mut self, pid: PaneId, client_id: Option<ClientId>) {
        self.close_down_to_max_terminals();
        if self.fullscreen_is_active {
            self.unset_fullscreen();
        }
        // TODO: check minimum size of active terminal

        let (_largest_terminal_size, terminal_id_to_split) = self.get_panes().fold(
            (0, None),
            |(current_largest_terminal_size, current_terminal_id_to_split),
             id_and_terminal_to_check| {
                let (id_of_terminal_to_check, terminal_to_check) = id_and_terminal_to_check;
                let terminal_size = (terminal_to_check.rows() * CURSOR_HEIGHT_WIDTH_RATIO)
                    * terminal_to_check.cols();
                let terminal_can_be_split = terminal_to_check.cols() >= MIN_TERMINAL_WIDTH
                    && terminal_to_check.rows() >= MIN_TERMINAL_HEIGHT
                    && ((terminal_to_check.cols() > terminal_to_check.min_width() * 2)
                        || (terminal_to_check.rows() > terminal_to_check.min_height() * 2));
                if terminal_can_be_split && terminal_size > current_largest_terminal_size {
                    (terminal_size, Some(*id_of_terminal_to_check))
                } else {
                    (current_largest_terminal_size, current_terminal_id_to_split)
                }
            },
        );
        if terminal_id_to_split.is_none() {
            self.senders
                .send_to_pty(PtyInstruction::ClosePane(pid)) // we can't open this pane, close the pty
                .unwrap();
            return; // likely no terminal large enough to split
        }
        let terminal_id_to_split = terminal_id_to_split.unwrap();
        let next_terminal_position = self.get_next_terminal_position();
        let terminal_to_split = self.panes.get_mut(&terminal_id_to_split).unwrap();
        let terminal_ws = terminal_to_split.position_and_size();
        if terminal_to_split.rows() * CURSOR_HEIGHT_WIDTH_RATIO > terminal_to_split.cols()
            && terminal_to_split.rows() > terminal_to_split.min_height() * 2
        {
            if let PaneId::Terminal(term_pid) = pid {
                if let Some((top_winsize, bottom_winsize)) =
                    split(Direction::Horizontal, &terminal_ws)
                {
                    let new_terminal = TerminalPane::new(
                        term_pid,
                        bottom_winsize,
                        self.colors,
                        next_terminal_position,
                    );
                    terminal_to_split.set_geom(top_winsize);
                    self.panes.insert(pid, Box::new(new_terminal));
                    self.relayout_tab(Direction::Vertical);
                }
            }
        } else if terminal_to_split.cols() > terminal_to_split.min_width() * 2 {
            if let PaneId::Terminal(term_pid) = pid {
                if let Some((left_winsize, right_winsize)) =
                    split(Direction::Vertical, &terminal_ws)
                {
                    let new_terminal = TerminalPane::new(
                        term_pid,
                        right_winsize,
                        self.colors,
                        next_terminal_position,
                    );
                    terminal_to_split.set_geom(left_winsize);
                    self.panes.insert(pid, Box::new(new_terminal));
                    self.relayout_tab(Direction::Horizontal);
                }
            }
        }
        if let Some(client_id) = client_id {
            // right now we administratively change focus of all clients until the
            // mirroring/multiplayer situation is sorted out
            let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
            for client_id in connected_clients {
                self.active_panes.insert(client_id, pid);
            }
        }
    }
    pub fn horizontal_split(&mut self, pid: PaneId, client_id: ClientId) {
        self.close_down_to_max_terminals();
        if self.fullscreen_is_active {
            self.toggle_active_pane_fullscreen(client_id);
        }
        if let PaneId::Terminal(term_pid) = pid {
            let next_terminal_position = self.get_next_terminal_position();
            let active_pane_id = &self.get_active_pane_id(client_id).unwrap();
            let active_pane = self.panes.get_mut(active_pane_id).unwrap();
            if active_pane.rows() < MIN_TERMINAL_HEIGHT * 2 {
                self.senders
                    .send_to_pty(PtyInstruction::ClosePane(pid)) // we can't open this pane, close the pty
                    .unwrap();
                return;
            }
            let terminal_ws = active_pane.position_and_size();
            if let Some((top_winsize, bottom_winsize)) = split(Direction::Horizontal, &terminal_ws)
            {
                let new_terminal = TerminalPane::new(
                    term_pid,
                    bottom_winsize,
                    self.colors,
                    next_terminal_position,
                );
                active_pane.set_geom(top_winsize);
                self.panes.insert(pid, Box::new(new_terminal));

                // right now we administratively change focus of all clients until the
                // mirroring/multiplayer situation is sorted out
                let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
                for client_id in connected_clients {
                    self.active_panes.insert(client_id, pid);
                }

                self.relayout_tab(Direction::Vertical);
            }
        }
    }
    pub fn vertical_split(&mut self, pid: PaneId, client_id: ClientId) {
        self.close_down_to_max_terminals();
        if self.fullscreen_is_active {
            self.toggle_active_pane_fullscreen(client_id);
        }
        if let PaneId::Terminal(term_pid) = pid {
            // TODO: check minimum size of active terminal
            let next_terminal_position = self.get_next_terminal_position();
            let active_pane_id = &self.get_active_pane_id(client_id).unwrap();
            let active_pane = self.panes.get_mut(active_pane_id).unwrap();
            if active_pane.cols() < MIN_TERMINAL_WIDTH * 2 {
                self.senders
                    .send_to_pty(PtyInstruction::ClosePane(pid)) // we can't open this pane, close the pty
                    .unwrap();
                return;
            }
            let terminal_ws = active_pane.position_and_size();
            if let Some((left_winsize, right_winsize)) = split(Direction::Vertical, &terminal_ws) {
                let new_terminal =
                    TerminalPane::new(term_pid, right_winsize, self.colors, next_terminal_position);
                active_pane.set_geom(left_winsize);
                self.panes.insert(pid, Box::new(new_terminal));
            }

            // right now we administratively change focus of all clients until the
            // mirroring/multiplayer situation is sorted out
            let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
            for client_id in connected_clients {
                self.active_panes.insert(client_id, pid);
            }

            self.relayout_tab(Direction::Horizontal);
        }
    }
    pub fn has_active_panes(&self) -> bool {
        // a tab without active panes is a dead tab and should close
        // a pane can be active even if there are no connected clients,
        // we remember that pane for one the client focuses the tab next
        !self.active_panes.is_empty()
    }
    pub fn get_active_pane(&self, client_id: ClientId) -> Option<&dyn Pane> {
        self.get_active_pane_id(client_id)
            .and_then(|ap| self.panes.get(&ap).map(Box::as_ref))
    }
    fn get_active_pane_id(&self, client_id: ClientId) -> Option<PaneId> {
        // TODO: why do we need this?
        self.active_panes.get(&client_id).copied()
    }
    fn get_active_terminal_id(&self, client_id: ClientId) -> Option<RawFd> {
        if let Some(PaneId::Terminal(pid)) = self.active_panes.get(&client_id).copied() {
            Some(pid)
        } else {
            None
        }
    }
    pub fn has_terminal_pid(&self, pid: RawFd) -> bool {
        self.panes.contains_key(&PaneId::Terminal(pid))
    }
    pub fn handle_pty_bytes(&mut self, pid: RawFd, bytes: VteBytes) {
        if let Some(terminal_output) = self.panes.get_mut(&PaneId::Terminal(pid)) {
            // If the pane is scrolled buffer the vte events
            if terminal_output.is_scrolled() {
                self.pending_vte_events.entry(pid).or_default().push(bytes);
                if let Some(evs) = self.pending_vte_events.get(&pid) {
                    // Reset scroll - and process all pending events for this pane
                    if evs.len() >= MAX_PENDING_VTE_EVENTS {
                        terminal_output.clear_scroll();
                        self.process_pending_vte_events(pid);
                    }
                }
                return;
            }
        }
        self.process_pty_bytes(pid, bytes);
    }
    pub fn process_pending_vte_events(&mut self, pid: RawFd) {
        if let Some(pending_vte_events) = self.pending_vte_events.get_mut(&pid) {
            let vte_events: Vec<VteBytes> = pending_vte_events.drain(..).collect();
            for vte_event in vte_events {
                self.process_pty_bytes(pid, vte_event);
            }
        }
    }
    fn process_pty_bytes(&mut self, pid: RawFd, bytes: VteBytes) {
        // if we don't have the terminal in self.terminals it's probably because
        // of a race condition where the terminal was created in pty but has not
        // yet been created in Screen. These events are currently not buffered, so
        // if you're debugging seemingly randomly missing stdout data, this is
        // the reason
        if let Some(terminal_output) = self.panes.get_mut(&PaneId::Terminal(pid)) {
            terminal_output.handle_pty_bytes(bytes);
            let messages_to_pty = terminal_output.drain_messages_to_pty();
            for message in messages_to_pty {
                self.write_to_pane_id(message, PaneId::Terminal(pid));
            }
        }
    }
    pub fn write_to_terminals_on_current_tab(&mut self, input_bytes: Vec<u8>) {
        let pane_ids = self.get_pane_ids();
        pane_ids.iter().for_each(|&pane_id| {
            self.write_to_pane_id(input_bytes.clone(), pane_id);
        });
    }
    pub fn write_to_active_terminal(&mut self, input_bytes: Vec<u8>, client_id: ClientId) {
        self.write_to_pane_id(input_bytes, self.get_active_pane_id(client_id).unwrap());
    }
    pub fn write_to_pane_id(&mut self, input_bytes: Vec<u8>, pane_id: PaneId) {
        match pane_id {
            PaneId::Terminal(active_terminal_id) => {
                let active_terminal = self.panes.get(&pane_id).unwrap();
                let adjusted_input = active_terminal.adjust_input_to_terminal(input_bytes);
                self.os_api
                    .write_to_tty_stdin(active_terminal_id, &adjusted_input)
                    .expect("failed to write to terminal");
                self.os_api
                    .tcdrain(active_terminal_id)
                    .expect("failed to drain terminal");
            }
            PaneId::Plugin(pid) => {
                for key in parse_keys(&input_bytes) {
                    self.senders
                        .send_to_plugin(PluginInstruction::Update(Some(pid), Event::Key(key)))
                        .unwrap()
                }
            }
        }
    }
    pub fn get_active_terminal_cursor_position(&self, client_id: ClientId) -> Option<(usize, usize)> {
        // (x, y)
        let active_terminal = &self.get_active_pane(client_id)?;
        active_terminal
            .cursor_coordinates()
            .map(|(x_in_terminal, y_in_terminal)| {
                let x = active_terminal.x() + x_in_terminal;
                let y = active_terminal.y() + y_in_terminal;
                (x, y)
            })
    }
    pub fn unset_fullscreen(&mut self) {
        if self.fullscreen_is_active {
            let first_client_id = self.connected_clients.iter().next().unwrap(); // this is a temporary hack until we fix the ui for multiple clients
            let active_pane_id = self.active_panes.get(first_client_id).unwrap();
            for terminal_id in self.panes_to_hide.iter() {
                let pane = self.panes.get_mut(terminal_id).unwrap();
                pane.set_should_render(true);
                pane.set_should_render_boundaries(true);
            }
            let viewport_pane_ids: Vec<_> = self
                .get_pane_ids()
                .into_iter()
                .filter(|id| !self.is_inside_viewport(id))
                .collect();
            for pid in viewport_pane_ids {
                let viewport_pane = self.panes.get_mut(&pid).unwrap();
                viewport_pane.reset_size_and_position_override();
            }
            self.panes_to_hide.clear();
            let active_terminal = self.panes.get_mut(&active_pane_id).unwrap();
            active_terminal.reset_size_and_position_override();
            self.set_force_render();
            self.resize_whole_tab(self.display_area);
            self.toggle_fullscreen_is_active();
        }
    }
    pub fn toggle_active_pane_fullscreen(&mut self, client_id: ClientId) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            if self.fullscreen_is_active {
                self.unset_fullscreen();
            } else {
                let panes = self.get_panes();
                let pane_ids_to_hide = panes.filter_map(|(&id, _pane)| {
                    if id != active_pane_id && self.is_inside_viewport(&id) {
                        Some(id)
                    } else {
                        None
                    }
                });
                self.panes_to_hide = pane_ids_to_hide.collect();
                if self.panes_to_hide.is_empty() {
                    // nothing to do, pane is already as fullscreen as it can be, let's bail
                    return;
                } else {
                    // For all of the panes outside of the viewport staying on the fullscreen
                    // screen, switch them to using override positions as well so that the resize
                    // system doesn't get confused by viewport and old panes that no longer line up
                    let viewport_pane_ids: Vec<_> = self
                        .get_pane_ids()
                        .into_iter()
                        .filter(|id| !self.is_inside_viewport(id))
                        .collect();
                    for pid in viewport_pane_ids {
                        let viewport_pane = self.panes.get_mut(&pid).unwrap();
                        viewport_pane.get_geom_override(viewport_pane.position_and_size());
                    }
                    let active_terminal = self.panes.get_mut(&active_pane_id).unwrap();
                    let full_screen_geom = PaneGeom {
                        x: self.viewport.x,
                        y: self.viewport.y,
                        ..Default::default()
                    };
                    active_terminal.get_geom_override(full_screen_geom);
                }
                self.set_force_render();
                self.resize_whole_tab(self.display_area);
                self.toggle_fullscreen_is_active();
            }
        }
    }
    pub fn is_fullscreen_active(&self) -> bool {
        self.fullscreen_is_active
    }
    pub fn toggle_fullscreen_is_active(&mut self) {
        self.fullscreen_is_active = !self.fullscreen_is_active;
    }
    pub fn set_force_render(&mut self) {
        for pane in self.panes.values_mut() {
            pane.set_should_render(true);
            pane.set_should_render_boundaries(true);
            pane.render_full_viewport();
        }
    }
    pub fn is_sync_panes_active(&self) -> bool {
        self.synchronize_is_active
    }
    pub fn toggle_sync_panes_is_active(&mut self) {
        self.synchronize_is_active = !self.synchronize_is_active;
    }
    pub fn mark_active_pane_for_rerender(&mut self, client_id: ClientId) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            self.panes.get_mut(&active_pane_id).unwrap().set_should_render(true)
        }
    }
    pub fn set_pane_frames(&mut self, draw_pane_frames: bool) {
        self.draw_pane_frames = draw_pane_frames;
        self.should_clear_display_before_rendering = true;
        let viewport = self.viewport;
        for (pane_id, pane) in self.panes.iter_mut() {
            if !pane.borderless() {
                pane.set_frame(draw_pane_frames);
            }

            #[allow(clippy::if_same_then_else)]
            if draw_pane_frames & !pane.borderless() {
                // there's definitely a frame around this pane, offset its contents
                pane.set_content_offset(Offset::frame(1));
            } else if draw_pane_frames && pane.borderless() {
                // there's no frame around this pane, and the tab isn't handling the boundaries
                // between panes (they each draw their own frames as they please)
                // this one doesn't - do not offset its content
                pane.set_content_offset(Offset::default());
            } else if !is_inside_viewport(&viewport, pane) {
                // this pane is outside the viewport and has no border - it should not have an offset
                pane.set_content_offset(Offset::default());
            } else {
                // no draw_pane_frames and this pane should have a separation to other panes
                // according to its position in the viewport (eg. no separation if its at the
                // viewport bottom) - offset its content accordingly
                let position_and_size = pane.current_geom();
                let (pane_columns_offset, pane_rows_offset) =
                    pane_content_offset(&position_and_size, &self.viewport);
                pane.set_content_offset(Offset::shift(pane_rows_offset, pane_columns_offset));
            }

            // FIXME: This, and all other `set_terminal_size_using_fd` calls, would be best in
            // `TerminalPane::reflow_lines`
            if let PaneId::Terminal(pid) = pane_id {
                self.os_api.set_terminal_size_using_fd(
                    *pid,
                    pane.get_content_columns() as u16,
                    pane.get_content_rows() as u16,
                );
            }
        }
    }
    pub fn render(&mut self, output: &mut Output) {
        if self.connected_clients.is_empty() || self.active_panes.is_empty() {
            return;
        }
        for connected_client in self.connected_clients.iter() {
            // TODO: move this out of the render function
            self.senders
                .send_to_pty(PtyInstruction::UpdateActivePane(
                    self.active_panes.get(connected_client).copied(),
                    *connected_client,
                ))
                .unwrap();
        }
        output.add_clients(&self.connected_clients);
        let mut boundaries = Boundaries::new(self.viewport);
        let hide_cursor = "\u{1b}[?25l";
        output.push_str_to_all_clients(hide_cursor);
        if self.should_clear_display_before_rendering {
            let clear_display = "\u{1b}[2J";
            output.push_str_to_all_clients(clear_display);
            self.should_clear_display_before_rendering = false;
        }
        let first_client_id = self.connected_clients.iter().next().unwrap(); // this is a temporary hack until we fix the ui for multiple clients
        for (_kind, pane) in self.panes.iter_mut() {
            if !self.panes_to_hide.contains(&pane.pid()) {
                match self.active_panes.get(first_client_id).copied().unwrap() == pane.pid() {
                    true => {
                        pane.set_active_at(Instant::now());
                        match self.mode_info.mode {
                            InputMode::Normal | InputMode::Locked => {
                                pane.set_boundary_color(Some(self.colors.green));
                            }
                            _ => {
                                pane.set_boundary_color(Some(self.colors.orange));
                            }
                        }
                        if !self.draw_pane_frames {
                            boundaries.add_rect(
                                pane.as_ref(),
                                self.mode_info.mode,
                                Some(self.colors),
                            )
                        }
                    }
                    false => {
                        pane.set_boundary_color(None);
                        if !self.draw_pane_frames {
                            boundaries.add_rect(pane.as_ref(), self.mode_info.mode, None);
                        }
                    }
                }
                if let Some(vte_output) = pane.render() {
                    // FIXME: Use Termion for cursor and style clearing?
                    output.push_str_to_all_clients(&format!(
                        "\u{1b}[{};{}H\u{1b}[m{}",
                        pane.y() + 1,
                        pane.x() + 1,
                        vte_output
                    ));
                }
            }
        }

        if !self.draw_pane_frames {
            output.push_str_to_all_clients(&boundaries.vte_output());
        }

        match self.get_active_terminal_cursor_position(*first_client_id) {
            Some((cursor_position_x, cursor_position_y)) => {
                let show_cursor = "\u{1b}[?25h";
                let change_cursor_shape = self.get_active_pane(*first_client_id).unwrap().cursor_shape_csi();
                let goto_cursor_position = &format!(
                    "\u{1b}[{};{}H\u{1b}[m{}",
                    cursor_position_y + 1,
                    cursor_position_x + 1,
                    change_cursor_shape
                ); // goto row/col
                output.push_str_to_all_clients(show_cursor);
                output.push_str_to_all_clients(goto_cursor_position);
            }
            None => {
                let hide_cursor = "\u{1b}[?25l";
                output.push_str_to_all_clients(hide_cursor);
            }
        }
    }
    fn get_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.panes.iter()
    }
    fn get_selectable_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.panes.iter().filter(|(_, p)| p.selectable())
    }
    fn get_next_terminal_position(&self) -> usize {
        self.panes
            .iter()
            .filter(|(k, _)| match k {
                PaneId::Plugin(_) => false,
                PaneId::Terminal(_) => true,
            })
            .count()
            + 1
    }
    fn has_selectable_panes(&self) -> bool {
        let mut all_terminals = self.get_selectable_panes();
        all_terminals.next().is_some()
    }
    fn next_active_pane(&self, panes: &[PaneId]) -> Option<PaneId> {
        panes
            .iter()
            .rev()
            .find(|pid| self.panes.get(pid).unwrap().selectable())
            .copied()
    }
    fn pane_ids_directly_left_of(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let mut ids = vec![];
        let terminal_to_check = self.panes.get(id).unwrap();
        if terminal_to_check.x() == 0 {
            return None;
        }
        for (&pid, terminal) in self.get_panes() {
            if terminal.x() + terminal.cols() == terminal_to_check.x() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn pane_ids_directly_right_of(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let mut ids = vec![];
        let terminal_to_check = self.panes.get(id).unwrap();
        for (&pid, terminal) in self.get_panes() {
            if terminal.x() == terminal_to_check.x() + terminal_to_check.cols() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn pane_ids_directly_below(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let mut ids = vec![];
        let terminal_to_check = self.panes.get(id).unwrap();
        for (&pid, terminal) in self.get_panes() {
            if terminal.y() == terminal_to_check.y() + terminal_to_check.rows() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn pane_ids_directly_above(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let mut ids = vec![];
        let terminal_to_check = self.panes.get(id).unwrap();
        for (&pid, terminal) in self.get_panes() {
            if terminal.y() + terminal.rows() == terminal_to_check.y() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn panes_top_aligned_with_pane(&self, pane: &dyn Pane) -> Vec<&dyn Pane> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(t_id).unwrap().as_ref())
            .filter(|terminal| terminal.pid() != pane.pid() && terminal.y() == pane.y())
            .collect()
    }
    fn panes_bottom_aligned_with_pane(&self, pane: &dyn Pane) -> Vec<&dyn Pane> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(t_id).unwrap().as_ref())
            .filter(|terminal| {
                terminal.pid() != pane.pid()
                    && terminal.y() + terminal.rows() == pane.y() + pane.rows()
            })
            .collect()
    }
    fn panes_right_aligned_with_pane(&self, pane: &dyn Pane) -> Vec<&dyn Pane> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(t_id).unwrap().as_ref())
            .filter(|terminal| {
                terminal.pid() != pane.pid()
                    && terminal.x() + terminal.cols() == pane.x() + pane.cols()
            })
            .collect()
    }
    fn panes_left_aligned_with_pane(&self, pane: &dyn Pane) -> Vec<&dyn Pane> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(t_id).unwrap().as_ref())
            .filter(|terminal| terminal.pid() != pane.pid() && terminal.x() == pane.x())
            .collect()
    }
    fn right_aligned_contiguous_panes_above(
        &self,
        id: &PaneId,
        terminal_borders_to_the_right: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(id)
            .expect("terminal id does not exist")
            .as_ref();
        let mut right_aligned_terminals = self.panes_right_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        right_aligned_terminals.sort_by_key(|a| Reverse(a.y()));
        for terminal in right_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() + terminal.rows() == terminal_to_check.y() {
                terminals.push(terminal);
            }
        }
        // top-most border aligned with a pane border to the right
        let mut top_resize_border = 0;
        for terminal in &terminals {
            let bottom_terminal_boundary = terminal.y() + terminal.rows();
            if terminal_borders_to_the_right
                .get(&bottom_terminal_boundary)
                .is_some()
                && top_resize_border < bottom_terminal_boundary
            {
                top_resize_border = bottom_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.y() >= top_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let top_resize_border = if terminals.is_empty() {
            terminal_to_check.y()
        } else {
            top_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (top_resize_border, terminal_ids)
    }
    fn right_aligned_contiguous_panes_below(
        &self,
        id: &PaneId,
        terminal_borders_to_the_right: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(id)
            .expect("terminal id does not exist")
            .as_ref();
        let mut right_aligned_terminals = self.panes_right_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        right_aligned_terminals.sort_by_key(|a| a.y());
        for terminal in right_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() == terminal_to_check.y() + terminal_to_check.rows() {
                terminals.push(terminal);
            }
        }
        // bottom-most border aligned with a pane border to the right
        let mut bottom_resize_border = self.viewport.y + self.viewport.rows;
        for terminal in &terminals {
            let top_terminal_boundary = terminal.y();
            if terminal_borders_to_the_right
                .get(&(top_terminal_boundary))
                .is_some()
                && top_terminal_boundary < bottom_resize_border
            {
                bottom_resize_border = top_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.y() + terminal.rows() <= bottom_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let bottom_resize_border = if terminals.is_empty() {
            terminal_to_check.y() + terminal_to_check.rows()
        } else {
            bottom_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (bottom_resize_border, terminal_ids)
    }
    fn left_aligned_contiguous_panes_above(
        &self,
        id: &PaneId,
        terminal_borders_to_the_left: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(id)
            .expect("terminal id does not exist")
            .as_ref();
        let mut left_aligned_terminals = self.panes_left_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        left_aligned_terminals.sort_by_key(|a| Reverse(a.y()));
        for terminal in left_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() + terminal.rows() == terminal_to_check.y() {
                terminals.push(terminal);
            }
        }
        // top-most border aligned with a pane border to the right
        let mut top_resize_border = 0;
        for terminal in &terminals {
            let bottom_terminal_boundary = terminal.y() + terminal.rows();
            if terminal_borders_to_the_left
                .get(&bottom_terminal_boundary)
                .is_some()
                && top_resize_border < bottom_terminal_boundary
            {
                top_resize_border = bottom_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.y() >= top_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let top_resize_border = if terminals.is_empty() {
            terminal_to_check.y()
        } else {
            top_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (top_resize_border, terminal_ids)
    }
    fn left_aligned_contiguous_panes_below(
        &self,
        id: &PaneId,
        terminal_borders_to_the_left: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(id)
            .expect("terminal id does not exist")
            .as_ref();
        let mut left_aligned_terminals = self.panes_left_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        left_aligned_terminals.sort_by_key(|a| a.y());
        for terminal in left_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() == terminal_to_check.y() + terminal_to_check.rows() {
                terminals.push(terminal);
            }
        }
        // bottom-most border aligned with a pane border to the left
        let mut bottom_resize_border = self.viewport.y + self.viewport.rows;
        for terminal in &terminals {
            let top_terminal_boundary = terminal.y();
            if terminal_borders_to_the_left
                .get(&(top_terminal_boundary))
                .is_some()
                && top_terminal_boundary < bottom_resize_border
            {
                bottom_resize_border = top_terminal_boundary;
            }
        }
        terminals.retain(|terminal| {
            // terminal.y() + terminal.rows() < bottom_resize_border
            terminal.y() + terminal.rows() <= bottom_resize_border
        });
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let bottom_resize_border = if terminals.is_empty() {
            terminal_to_check.y() + terminal_to_check.rows()
        } else {
            bottom_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (bottom_resize_border, terminal_ids)
    }
    fn top_aligned_contiguous_panes_to_the_left(
        &self,
        id: &PaneId,
        terminal_borders_above: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(id)
            .expect("terminal id does not exist")
            .as_ref();
        let mut top_aligned_terminals = self.panes_top_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        top_aligned_terminals.sort_by_key(|a| Reverse(a.x()));
        for terminal in top_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() + terminal.cols() == terminal_to_check.x() {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut left_resize_border = 0;
        for terminal in &terminals {
            let right_terminal_boundary = terminal.x() + terminal.cols();
            if terminal_borders_above
                .get(&right_terminal_boundary)
                .is_some()
                && left_resize_border < right_terminal_boundary
            {
                left_resize_border = right_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.x() >= left_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let left_resize_border = if terminals.is_empty() {
            terminal_to_check.x()
        } else {
            left_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (left_resize_border, terminal_ids)
    }
    fn top_aligned_contiguous_panes_to_the_right(
        &self,
        id: &PaneId,
        terminal_borders_above: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.panes.get(id).unwrap().as_ref();
        let mut top_aligned_terminals = self.panes_top_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        top_aligned_terminals.sort_by_key(|a| a.x());
        for terminal in top_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() == terminal_to_check.x() + terminal_to_check.cols() {
                terminals.push(terminal);
            }
        }
        // rightmost border aligned with a pane border above
        let mut right_resize_border = self.viewport.x + self.viewport.cols;
        for terminal in &terminals {
            let left_terminal_boundary = terminal.x();
            if terminal_borders_above
                .get(&left_terminal_boundary)
                .is_some()
                && right_resize_border > left_terminal_boundary
            {
                right_resize_border = left_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.x() + terminal.cols() <= right_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let right_resize_border = if terminals.is_empty() {
            terminal_to_check.x() + terminal_to_check.cols()
        } else {
            right_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (right_resize_border, terminal_ids)
    }
    fn bottom_aligned_contiguous_panes_to_the_left(
        &self,
        id: &PaneId,
        terminal_borders_below: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.panes.get(id).unwrap().as_ref();
        let mut bottom_aligned_terminals = self.panes_bottom_aligned_with_pane(terminal_to_check);
        bottom_aligned_terminals.sort_by_key(|a| Reverse(a.x()));
        // terminals that are next to each other up to current
        for terminal in bottom_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() + terminal.cols() == terminal_to_check.x() {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut left_resize_border = 0;
        for terminal in &terminals {
            let right_terminal_boundary = terminal.x() + terminal.cols();
            if terminal_borders_below
                .get(&right_terminal_boundary)
                .is_some()
                && left_resize_border < right_terminal_boundary
            {
                left_resize_border = right_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.x() >= left_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let left_resize_border = if terminals.is_empty() {
            terminal_to_check.x()
        } else {
            left_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (left_resize_border, terminal_ids)
    }
    fn bottom_aligned_contiguous_panes_to_the_right(
        &self,
        id: &PaneId,
        terminal_borders_below: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.panes.get(id).unwrap().as_ref();
        let mut bottom_aligned_terminals = self.panes_bottom_aligned_with_pane(terminal_to_check);
        bottom_aligned_terminals.sort_by_key(|a| a.x());
        // terminals that are next to each other up to current
        for terminal in bottom_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() == terminal_to_check.x() + terminal_to_check.cols() {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut right_resize_border = self.viewport.x + self.viewport.cols;
        for terminal in &terminals {
            let left_terminal_boundary = terminal.x();
            if terminal_borders_below
                .get(&left_terminal_boundary)
                .is_some()
                && right_resize_border > left_terminal_boundary
            {
                right_resize_border = left_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.x() + terminal.cols() <= right_resize_border);
        let right_resize_border = if terminals.is_empty() {
            terminal_to_check.x() + terminal_to_check.cols()
        } else {
            right_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (right_resize_border, terminal_ids)
    }
    fn reduce_pane_height(&mut self, id: &PaneId, percent: f64) {
        let terminal = self.panes.get_mut(id).unwrap();
        terminal.reduce_height(percent);
    }
    fn increase_pane_height(&mut self, id: &PaneId, percent: f64) {
        let terminal = self.panes.get_mut(id).unwrap();
        terminal.increase_height(percent);
    }
    fn increase_pane_width(&mut self, id: &PaneId, percent: f64) {
        let terminal = self.panes.get_mut(id).unwrap();
        terminal.increase_width(percent);
    }
    fn reduce_pane_width(&mut self, id: &PaneId, percent: f64) {
        let terminal = self.panes.get_mut(id).unwrap();
        terminal.reduce_width(percent);
    }
    fn pane_is_between_vertical_borders(
        &self,
        id: &PaneId,
        left_border_x: usize,
        right_border_x: usize,
    ) -> bool {
        let terminal = self
            .panes
            .get(id)
            .expect("could not find terminal to check between borders");
        terminal.x() >= left_border_x && terminal.x() + terminal.cols() <= right_border_x
    }
    fn pane_is_between_horizontal_borders(
        &self,
        id: &PaneId,
        top_border_y: usize,
        bottom_border_y: usize,
    ) -> bool {
        let terminal = self
            .panes
            .get(id)
            .expect("could not find terminal to check between borders");
        terminal.y() >= top_border_y && terminal.y() + terminal.rows() <= bottom_border_y
    }
    fn reduce_pane_and_surroundings_up(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_below = self
            .pane_ids_directly_below(id)
            .expect("can't reduce pane size up if there are no terminals below");
        let terminal_borders_below: HashSet<usize> = terminals_below
            .iter()
            .map(|t| self.panes.get(t).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.bottom_aligned_contiguous_panes_to_the_left(id, &terminal_borders_below);
        let (right_resize_border, terminals_to_the_right) =
            self.bottom_aligned_contiguous_panes_to_the_right(id, &terminal_borders_below);
        terminals_below.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });

        // FIXME: This checks that we aren't violating the resize constraints of the aligned panes
        // above and below this one. This should be moved to a `can_resize` function eventually.
        for terminal_id in terminals_to_the_left
            .iter()
            .chain(terminals_to_the_right.iter())
        {
            let pane = self.panes.get(terminal_id).unwrap();
            if pane.current_geom().rows.as_percent().unwrap() - percent < RESIZE_PERCENT {
                return;
            }
        }

        self.reduce_pane_height(id, percent);
        for terminal_id in terminals_below {
            self.increase_pane_height(&terminal_id, percent);
        }
        for terminal_id in terminals_to_the_left
            .iter()
            .chain(terminals_to_the_right.iter())
        {
            self.reduce_pane_height(terminal_id, percent);
        }
    }
    fn reduce_pane_and_surroundings_down(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_above = self
            .pane_ids_directly_above(id)
            .expect("can't reduce pane size down if there are no terminals above");
        let terminal_borders_above: HashSet<usize> = terminals_above
            .iter()
            .map(|t| self.panes.get(t).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.top_aligned_contiguous_panes_to_the_left(id, &terminal_borders_above);
        let (right_resize_border, terminals_to_the_right) =
            self.top_aligned_contiguous_panes_to_the_right(id, &terminal_borders_above);
        terminals_above.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });

        // FIXME: This checks that we aren't violating the resize constraints of the aligned panes
        // above and below this one. This should be moved to a `can_resize` function eventually.
        for terminal_id in terminals_to_the_left
            .iter()
            .chain(terminals_to_the_right.iter())
        {
            let pane = self.panes.get(terminal_id).unwrap();
            if pane.current_geom().rows.as_percent().unwrap() - percent < RESIZE_PERCENT {
                return;
            }
        }

        self.reduce_pane_height(id, percent);
        for terminal_id in terminals_above {
            self.increase_pane_height(&terminal_id, percent);
        }
        for terminal_id in terminals_to_the_left
            .iter()
            .chain(terminals_to_the_right.iter())
        {
            self.reduce_pane_height(terminal_id, percent);
        }
    }
    fn reduce_pane_and_surroundings_right(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_to_the_left = self
            .pane_ids_directly_left_of(id)
            .expect("can't reduce pane size right if there are no terminals to the left");
        let terminal_borders_to_the_left: HashSet<usize> = terminals_to_the_left
            .iter()
            .map(|t| self.panes.get(t).unwrap().y())
            .collect();
        let (top_resize_border, terminals_above) =
            self.left_aligned_contiguous_panes_above(id, &terminal_borders_to_the_left);
        let (bottom_resize_border, terminals_below) =
            self.left_aligned_contiguous_panes_below(id, &terminal_borders_to_the_left);
        terminals_to_the_left.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });

        // FIXME: This checks that we aren't violating the resize constraints of the aligned panes
        // above and below this one. This should be moved to a `can_resize` function eventually.
        for terminal_id in terminals_above.iter().chain(terminals_below.iter()) {
            let pane = self.panes.get(terminal_id).unwrap();
            if pane.current_geom().cols.as_percent().unwrap() - percent < RESIZE_PERCENT {
                return;
            }
        }

        self.reduce_pane_width(id, percent);
        for terminal_id in terminals_to_the_left {
            self.increase_pane_width(&terminal_id, percent);
        }
        for terminal_id in terminals_above.iter().chain(terminals_below.iter()) {
            self.reduce_pane_width(terminal_id, percent);
        }
    }
    fn reduce_pane_and_surroundings_left(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_to_the_right = self
            .pane_ids_directly_right_of(id)
            .expect("can't reduce pane size left if there are no terminals to the right");
        let terminal_borders_to_the_right: HashSet<usize> = terminals_to_the_right
            .iter()
            .map(|t| self.panes.get(t).unwrap().y())
            .collect();
        let (top_resize_border, terminals_above) =
            self.right_aligned_contiguous_panes_above(id, &terminal_borders_to_the_right);
        let (bottom_resize_border, terminals_below) =
            self.right_aligned_contiguous_panes_below(id, &terminal_borders_to_the_right);
        terminals_to_the_right.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });

        // FIXME: This checks that we aren't violating the resize constraints of the aligned panes
        // above and below this one. This should be moved to a `can_resize` function eventually.
        for terminal_id in terminals_above.iter().chain(terminals_below.iter()) {
            let pane = self.panes.get(terminal_id).unwrap();
            if pane.current_geom().cols.as_percent().unwrap() - percent < RESIZE_PERCENT {
                return;
            }
        }

        self.reduce_pane_width(id, percent);
        for terminal_id in terminals_to_the_right {
            self.increase_pane_width(&terminal_id, percent);
        }
        for terminal_id in terminals_above.iter().chain(terminals_below.iter()) {
            self.reduce_pane_width(terminal_id, percent);
        }
    }
    fn increase_pane_and_surroundings_up(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_above = self
            .pane_ids_directly_above(id)
            .expect("can't increase pane size up if there are no terminals above");
        let terminal_borders_above: HashSet<usize> = terminals_above
            .iter()
            .map(|t| self.panes.get(t).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.top_aligned_contiguous_panes_to_the_left(id, &terminal_borders_above);
        let (right_resize_border, terminals_to_the_right) =
            self.top_aligned_contiguous_panes_to_the_right(id, &terminal_borders_above);
        terminals_above.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });
        self.increase_pane_height(id, percent);
        for terminal_id in terminals_above {
            self.reduce_pane_height(&terminal_id, percent);
        }
        for terminal_id in terminals_to_the_left
            .iter()
            .chain(terminals_to_the_right.iter())
        {
            self.increase_pane_height(terminal_id, percent);
        }
    }
    fn increase_pane_and_surroundings_down(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_below = self
            .pane_ids_directly_below(id)
            .expect("can't increase pane size down if there are no terminals below");
        let terminal_borders_below: HashSet<usize> = terminals_below
            .iter()
            .map(|t| self.panes.get(t).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.bottom_aligned_contiguous_panes_to_the_left(id, &terminal_borders_below);
        let (right_resize_border, terminals_to_the_right) =
            self.bottom_aligned_contiguous_panes_to_the_right(id, &terminal_borders_below);
        terminals_below.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });
        self.increase_pane_height(id, percent);
        for terminal_id in terminals_below {
            self.reduce_pane_height(&terminal_id, percent);
        }
        for terminal_id in terminals_to_the_left
            .iter()
            .chain(terminals_to_the_right.iter())
        {
            self.increase_pane_height(terminal_id, percent);
        }
    }
    fn increase_pane_and_surroundings_right(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_to_the_right = self
            .pane_ids_directly_right_of(id)
            .expect("can't increase pane size right if there are no terminals to the right");
        let terminal_borders_to_the_right: HashSet<usize> = terminals_to_the_right
            .iter()
            .map(|t| {
                return self.panes.get(t).unwrap().y();
            })
            .collect();
        let (top_resize_border, terminals_above) =
            self.right_aligned_contiguous_panes_above(id, &terminal_borders_to_the_right);
        let (bottom_resize_border, terminals_below) =
            self.right_aligned_contiguous_panes_below(id, &terminal_borders_to_the_right);
        terminals_to_the_right.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });
        self.increase_pane_width(id, percent);
        for terminal_id in terminals_to_the_right {
            self.reduce_pane_width(&terminal_id, percent);
        }
        for terminal_id in terminals_above.iter().chain(terminals_below.iter()) {
            self.increase_pane_width(terminal_id, percent);
        }
    }
    fn increase_pane_and_surroundings_left(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_to_the_left = self
            .pane_ids_directly_left_of(id)
            .expect("can't increase pane size right if there are no terminals to the right");
        let terminal_borders_to_the_left: HashSet<usize> = terminals_to_the_left
            .iter()
            .map(|t| self.panes.get(t).unwrap().y())
            .collect();
        let (top_resize_border, terminals_above) =
            self.left_aligned_contiguous_panes_above(id, &terminal_borders_to_the_left);
        let (bottom_resize_border, terminals_below) =
            self.left_aligned_contiguous_panes_below(id, &terminal_borders_to_the_left);
        terminals_to_the_left.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });
        self.increase_pane_width(id, percent);
        for terminal_id in terminals_to_the_left {
            self.reduce_pane_width(&terminal_id, percent);
        }
        for terminal_id in terminals_above.iter().chain(terminals_below.iter()) {
            self.increase_pane_width(terminal_id, percent);
        }
    }
    // FIXME: The if-let nesting and explicit `false`s are... suboptimal.
    // FIXME: Quite a lot of duplication between these functions...
    fn can_increase_pane_and_surroundings_right(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        if let Some(panes_to_the_right) = self.pane_ids_directly_right_of(pane_id) {
            panes_to_the_right.iter().all(|id| {
                let p = self.panes.get(id).unwrap();
                if let Some(cols) = p.position_and_size().cols.as_percent() {
                    cols - increase_by >= RESIZE_PERCENT
                } else {
                    false
                }
            })
        } else {
            false
        }
    }
    fn can_increase_pane_and_surroundings_left(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        if let Some(panes_to_the_left) = self.pane_ids_directly_left_of(pane_id) {
            panes_to_the_left.iter().all(|id| {
                let p = self.panes.get(id).unwrap();
                if let Some(cols) = p.position_and_size().cols.as_percent() {
                    cols - increase_by >= RESIZE_PERCENT
                } else {
                    false
                }
            })
        } else {
            false
        }
    }
    fn can_increase_pane_and_surroundings_down(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        if let Some(panes_below) = self.pane_ids_directly_below(pane_id) {
            panes_below.iter().all(|id| {
                let p = self.panes.get(id).unwrap();
                if let Some(rows) = p.position_and_size().rows.as_percent() {
                    rows - increase_by >= RESIZE_PERCENT
                } else {
                    false
                }
            })
        } else {
            false
        }
    }
    fn can_increase_pane_and_surroundings_up(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        if let Some(panes_above) = self.pane_ids_directly_above(pane_id) {
            panes_above.iter().all(|id| {
                let p = self.panes.get(id).unwrap();
                if let Some(rows) = p.position_and_size().rows.as_percent() {
                    rows - increase_by >= RESIZE_PERCENT
                } else {
                    false
                }
            })
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_right(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let pane = self.panes.get(pane_id).unwrap();
        if let Some(cols) = pane.position_and_size().cols.as_percent() {
            let ids_left = self.pane_ids_directly_left_of(pane_id);
            let flexible_left = self.ids_are_flexible(Direction::Horizontal, ids_left);
            cols - reduce_by >= RESIZE_PERCENT && flexible_left
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_left(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let pane = self.panes.get(pane_id).unwrap();
        if let Some(cols) = pane.position_and_size().cols.as_percent() {
            let ids_right = self.pane_ids_directly_right_of(pane_id);
            let flexible_right = self.ids_are_flexible(Direction::Horizontal, ids_right);
            cols - reduce_by >= RESIZE_PERCENT && flexible_right
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_down(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let pane = self.panes.get(pane_id).unwrap();
        if let Some(rows) = pane.position_and_size().rows.as_percent() {
            let ids_above = self.pane_ids_directly_above(pane_id);
            let flexible_above = self.ids_are_flexible(Direction::Vertical, ids_above);
            rows - reduce_by >= RESIZE_PERCENT && flexible_above
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_up(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let pane = self.panes.get(pane_id).unwrap();
        if let Some(rows) = pane.position_and_size().rows.as_percent() {
            let ids_below = self.pane_ids_directly_below(pane_id);
            let flexible_below = self.ids_are_flexible(Direction::Vertical, ids_below);
            rows - reduce_by >= RESIZE_PERCENT && flexible_below
        } else {
            false
        }
    }
    fn ids_are_flexible(&self, direction: Direction, pane_ids: Option<Vec<PaneId>>) -> bool {
        pane_ids.is_some()
            && pane_ids.unwrap().iter().all(|id| {
                let geom = self.panes[id].current_geom();
                let dimension = match direction {
                    Direction::Vertical => geom.rows,
                    Direction::Horizontal => geom.cols,
                };
                !dimension.is_fixed()
            })
    }
    pub fn relayout_tab(&mut self, direction: Direction) {
        let mut resizer = PaneResizer::new(self.panes.iter_mut());
        let result = match direction {
            Direction::Horizontal => resizer.layout(direction, self.display_area.cols),
            Direction::Vertical => resizer.layout(direction, self.display_area.rows),
        };
        if let Err(e) = &result {
            log::error!("{:?} relayout of the tab failed: {}", direction, e);
        }
        self.set_pane_frames(self.draw_pane_frames);
    }
    pub fn resize_whole_tab(&mut self, new_screen_size: Size) {
        // FIXME: I *think* that Rust 2021 will let me just write this:
        // let panes = self.panes.iter_mut().filter(|(pid, _)| !self.panes_to_hide.contains(pid));
        // In the meantime, let's appease our borrow-checker overlords:
        let temp_panes_to_hide = &self.panes_to_hide;
        let panes = self
            .panes
            .iter_mut()
            .filter(|(pid, _)| !temp_panes_to_hide.contains(pid));
        let Size { rows, cols } = new_screen_size;
        let mut resizer = PaneResizer::new(panes);
        if resizer.layout(Direction::Horizontal, cols).is_ok() {
            let column_difference = cols as isize - self.display_area.cols as isize;
            // FIXME: Should the viewport be an Offset?
            self.viewport.cols = (self.viewport.cols as isize + column_difference) as usize;
            self.display_area.cols = cols;
        } else {
            log::error!("Failed to horizontally resize the tab!!!");
        }
        if resizer.layout(Direction::Vertical, rows).is_ok() {
            let row_difference = rows as isize - self.display_area.rows as isize;
            self.viewport.rows = (self.viewport.rows as isize + row_difference) as usize;
            self.display_area.rows = rows;
        } else {
            log::error!("Failed to vertically resize the tab!!!");
        }
        self.should_clear_display_before_rendering = true;
        self.set_pane_frames(self.draw_pane_frames);
    }
    pub fn resize_left(&mut self, client_id: ClientId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            if self.can_increase_pane_and_surroundings_left(&active_pane_id, RESIZE_PERCENT) {
                self.increase_pane_and_surroundings_left(&active_pane_id, RESIZE_PERCENT);
            } else if self.can_reduce_pane_and_surroundings_left(&active_pane_id, RESIZE_PERCENT) {
                self.reduce_pane_and_surroundings_left(&active_pane_id, RESIZE_PERCENT);
            }
        }
        self.relayout_tab(Direction::Horizontal);
    }
    pub fn resize_right(&mut self, client_id: ClientId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            if self.can_increase_pane_and_surroundings_right(&active_pane_id, RESIZE_PERCENT) {
                self.increase_pane_and_surroundings_right(&active_pane_id, RESIZE_PERCENT);
            } else if self.can_reduce_pane_and_surroundings_right(&active_pane_id, RESIZE_PERCENT) {
                self.reduce_pane_and_surroundings_right(&active_pane_id, RESIZE_PERCENT);
            }
        }
        self.relayout_tab(Direction::Horizontal);
    }
    pub fn resize_down(&mut self, client_id: ClientId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            if self.can_increase_pane_and_surroundings_down(&active_pane_id, RESIZE_PERCENT) {
                self.increase_pane_and_surroundings_down(&active_pane_id, RESIZE_PERCENT);
            } else if self.can_reduce_pane_and_surroundings_down(&active_pane_id, RESIZE_PERCENT) {
                self.reduce_pane_and_surroundings_down(&active_pane_id, RESIZE_PERCENT);
            }
        }
        self.relayout_tab(Direction::Vertical);
    }
    pub fn resize_up(&mut self, client_id: ClientId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            if self.can_increase_pane_and_surroundings_up(&active_pane_id, RESIZE_PERCENT) {
                self.increase_pane_and_surroundings_up(&active_pane_id, RESIZE_PERCENT);
            } else if self.can_reduce_pane_and_surroundings_up(&active_pane_id, RESIZE_PERCENT) {
                self.reduce_pane_and_surroundings_up(&active_pane_id, RESIZE_PERCENT);
            }
        }
        self.relayout_tab(Direction::Vertical);
    }
    pub fn move_focus(&mut self, client_id: ClientId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.fullscreen_is_active {
            return;
        }
        let current_active_pane_id = self.get_active_pane_id(client_id).unwrap();
        let pane_ids: Vec<PaneId> = self.get_selectable_panes().map(|(&pid, _)| pid).collect(); // TODO: better, no allocations
        let active_pane_id_position = pane_ids
            .iter()
            .position(|id| id == &current_active_pane_id)
            .unwrap();
        let next_active_pane_id = pane_ids
            .get(active_pane_id_position + 1)
            .or_else(|| pane_ids.get(0))
            .copied()
            .unwrap();

        let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
        for client_id in connected_clients {
            self.active_panes.insert(client_id, next_active_pane_id);
        }
    }
    pub fn focus_next_pane(&mut self, client_id: ClientId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.fullscreen_is_active {
            return;
        }
        let active_pane_id = self.get_active_pane_id(client_id).unwrap();
        let mut panes: Vec<(&PaneId, &Box<dyn Pane>)> = self.get_selectable_panes().collect();
        panes.sort_by(|(_a_id, a_pane), (_b_id, b_pane)| {
            if a_pane.y() == b_pane.y() {
                a_pane.x().cmp(&b_pane.x())
            } else {
                a_pane.y().cmp(&b_pane.y())
            }
        });
        let active_pane_position = panes
            .iter()
            .position(|(id, _)| *id == &active_pane_id) // TODO: better
            .unwrap();

        let next_active_pane_id = panes
            .get(active_pane_position + 1)
            .or_else(|| panes.get(0))
            .map(|p| *p.0)
            .unwrap();

        let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
        for client_id in connected_clients {
            self.active_panes.insert(client_id, next_active_pane_id);
        }
    }
    pub fn focus_previous_pane(&mut self, client_id: ClientId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.fullscreen_is_active {
            return;
        }
        let active_pane_id = self.get_active_pane_id(client_id).unwrap();
        let mut panes: Vec<(&PaneId, &Box<dyn Pane>)> = self.get_selectable_panes().collect();
        panes.sort_by(|(_a_id, a_pane), (_b_id, b_pane)| {
            if a_pane.y() == b_pane.y() {
                a_pane.x().cmp(&b_pane.x())
            } else {
                a_pane.y().cmp(&b_pane.y())
            }
        });
        let last_pane = panes.last().unwrap();
        let active_pane_position = panes
            .iter()
            .position(|(id, _)| *id == &active_pane_id) // TODO: better
            .unwrap();

        let next_active_pane_id = if active_pane_position == 0 {
            *last_pane.0
        } else {
            *panes.get(active_pane_position - 1).unwrap().0
        };
        let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
        for client_id in connected_clients {
            self.active_panes.insert(client_id, next_active_pane_id);
        }
    }
    // returns a boolean that indicates whether the focus moved
    pub fn move_focus_left(&mut self, client_id: ClientId) -> bool {
        if !self.has_selectable_panes() {
            return false;
        }
        if self.fullscreen_is_active {
            return false;
        }
        let active_pane = self.get_active_pane(client_id);
        let updated_active_pane = if let Some(active) = active_pane {
            let terminals = self.get_selectable_panes();
            let next_index = terminals
                .enumerate()
                .filter(|(_, (_, c))| {
                    c.is_directly_left_of(active) && c.horizontally_overlaps_with(active)
                })
                .max_by_key(|(_, (_, c))| c.active_at())
                .map(|(_, (pid, _))| pid);
            match next_index {
                Some(&p) => {
                    // render previously active pane so that its frame does not remain actively
                    // colored
                    let previously_active_pane =
                        self.panes.get_mut(&self.active_panes.get(&client_id).unwrap()).unwrap();
                    previously_active_pane.set_should_render(true);
                    let next_active_pane = self.panes.get_mut(&p).unwrap();
                    next_active_pane.set_should_render(true);

                    let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
                    for client_id in connected_clients {
                        self.active_panes.insert(client_id, p);
                    }
                    return true;
                }
                None => Some(active.pid()),
            }
        } else {
            Some(active_pane.unwrap().pid())
        };
        match updated_active_pane {
            Some(updated_active_pane) => {
                let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
                for client_id in connected_clients {
                    self.active_panes.insert(client_id, updated_active_pane);
                }
            },
            None => {
                // TODO: can this happen?
                self.active_panes.clear();
            }
        }

        false
    }
    pub fn move_focus_down(&mut self, client_id: ClientId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.fullscreen_is_active {
            return;
        }
        let active_pane = self.get_active_pane(client_id);
        let updated_active_pane = if let Some(active) = active_pane {
            let panes = self.get_selectable_panes();
            let next_index = panes
                .enumerate()
                .filter(|(_, (_, c))| {
                    c.is_directly_below(active) && c.vertically_overlaps_with(active)
                })
                .max_by_key(|(_, (_, c))| c.active_at())
                .map(|(_, (pid, _))| pid);
            match next_index {
                Some(&p) => {
                    // render previously active pane so that its frame does not remain actively
                    // colored
                    let previously_active_pane =
                        self.panes.get_mut(&self.active_panes.get(&client_id).unwrap()).unwrap();
                    previously_active_pane.set_should_render(true);
                    let next_active_pane = self.panes.get_mut(&p).unwrap();
                    next_active_pane.set_should_render(true);

                    Some(p)
                }
                None => Some(active.pid()),
            }
        } else {
            Some(active_pane.unwrap().pid())
        };
        match updated_active_pane {
            Some(updated_active_pane) => {
                let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
                for client_id in connected_clients {
                    self.active_panes.insert(client_id, updated_active_pane);
                }
            },
            None => {
                // TODO: can this happen?
                self.active_panes.clear();
            }
        }
    }
    pub fn move_focus_up(&mut self, client_id: ClientId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.fullscreen_is_active {
            return;
        }
        let active_pane = self.get_active_pane(client_id);
        let updated_active_pane = if let Some(active) = active_pane {
            let terminals = self.get_selectable_panes();
            let next_index = terminals
                .enumerate()
                .filter(|(_, (_, c))| {
                    c.is_directly_above(active) && c.vertically_overlaps_with(active)
                })
                .max_by_key(|(_, (_, c))| c.active_at())
                .map(|(_, (pid, _))| pid);
            match next_index {
                Some(&p) => {
                    // render previously active pane so that its frame does not remain actively
                    // colored
                    let previously_active_pane =
                        self.panes.get_mut(&self.active_panes.get(&client_id).unwrap()).unwrap();
                    previously_active_pane.set_should_render(true);
                    let next_active_pane = self.panes.get_mut(&p).unwrap();
                    next_active_pane.set_should_render(true);

                    Some(p)
                }
                None => Some(active.pid()),
            }
        } else {
            Some(active_pane.unwrap().pid())
        };
        match updated_active_pane {
            Some(updated_active_pane) => {
                let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
                for client_id in connected_clients {
                    self.active_panes.insert(client_id, updated_active_pane);
                }
            },
            None => {
                // TODO: can this happen?
                self.active_panes.clear();
            }
        }
    }
    // returns a boolean that indicates whether the focus moved
    pub fn move_focus_right(&mut self, client_id: ClientId) -> bool {
        if !self.has_selectable_panes() {
            return false;
        }
        if self.fullscreen_is_active {
            return false;
        }
        let active_pane = self.get_active_pane(client_id);
        let updated_active_pane = if let Some(active) = active_pane {
            let terminals = self.get_selectable_panes();
            let next_index = terminals
                .enumerate()
                .filter(|(_, (_, c))| {
                    c.is_directly_right_of(active) && c.horizontally_overlaps_with(active)
                })
                .max_by_key(|(_, (_, c))| c.active_at())
                .map(|(_, (pid, _))| pid);
            match next_index {
                Some(&p) => {
                    // render previously active pane so that its frame does not remain actively
                    // colored
                    let previously_active_pane =
                        self.panes.get_mut(&self.active_panes.get(&client_id).unwrap()).unwrap();
                    previously_active_pane.set_should_render(true);
                    let next_active_pane = self.panes.get_mut(&p).unwrap();
                    next_active_pane.set_should_render(true);

                    let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
                    for client_id in connected_clients {
                        self.active_panes.insert(client_id, p);
                    }
                    return true;
                }
                None => Some(active.pid()),
            }
        } else {
            Some(active_pane.unwrap().pid())
        };
        match updated_active_pane {
            Some(updated_active_pane) => {
                let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
                for client_id in connected_clients {
                    self.active_panes.insert(client_id, updated_active_pane);
                }
            },
            None => {
                // TODO: can this happen?
                self.active_panes.clear();
            }
        }
        false
    }
    fn horizontal_borders(&self, terminals: &[PaneId]) -> HashSet<usize> {
        terminals.iter().fold(HashSet::new(), |mut borders, t| {
            let terminal = self.panes.get(t).unwrap();
            borders.insert(terminal.y());
            borders.insert(terminal.y() + terminal.rows());
            borders
        })
    }
    fn vertical_borders(&self, terminals: &[PaneId]) -> HashSet<usize> {
        terminals.iter().fold(HashSet::new(), |mut borders, t| {
            let terminal = self.panes.get(t).unwrap();
            borders.insert(terminal.x());
            borders.insert(terminal.x() + terminal.cols());
            borders
        })
    }

    fn panes_to_the_left_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        if let Some(terminal) = self.panes.get(&id) {
            let upper_close_border = terminal.y();
            let lower_close_border = terminal.y() + terminal.rows();

            if let Some(terminals_to_the_left) = self.pane_ids_directly_left_of(&id) {
                let mut selectable_panes: Vec<_> = terminals_to_the_left
                    .into_iter()
                    .filter(|pid| self.panes[pid].selectable())
                    .collect();
                let terminal_borders_to_the_left = self.horizontal_borders(&selectable_panes);
                if terminal_borders_to_the_left.contains(&upper_close_border)
                    && terminal_borders_to_the_left.contains(&lower_close_border)
                {
                    selectable_panes.retain(|t| {
                        self.pane_is_between_horizontal_borders(
                            t,
                            upper_close_border,
                            lower_close_border,
                        )
                    });
                    return Some(selectable_panes);
                }
            }
        }
        None
    }
    fn panes_to_the_right_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        if let Some(terminal) = self.panes.get(&id) {
            let upper_close_border = terminal.y();
            let lower_close_border = terminal.y() + terminal.rows();

            if let Some(terminals_to_the_right) = self.pane_ids_directly_right_of(&id) {
                let mut selectable_panes: Vec<_> = terminals_to_the_right
                    .into_iter()
                    .filter(|pid| self.panes[pid].selectable())
                    .collect();
                let terminal_borders_to_the_right = self.horizontal_borders(&selectable_panes);
                if terminal_borders_to_the_right.contains(&upper_close_border)
                    && terminal_borders_to_the_right.contains(&lower_close_border)
                {
                    selectable_panes.retain(|t| {
                        self.pane_is_between_horizontal_borders(
                            t,
                            upper_close_border,
                            lower_close_border,
                        )
                    });
                    return Some(selectable_panes);
                }
            }
        }
        None
    }
    fn panes_above_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        if let Some(terminal) = self.panes.get(&id) {
            let left_close_border = terminal.x();
            let right_close_border = terminal.x() + terminal.cols();

            if let Some(terminals_above) = self.pane_ids_directly_above(&id) {
                let mut selectable_panes: Vec<_> = terminals_above
                    .into_iter()
                    .filter(|pid| self.panes[pid].selectable())
                    .collect();
                let terminal_borders_above = self.vertical_borders(&selectable_panes);
                if terminal_borders_above.contains(&left_close_border)
                    && terminal_borders_above.contains(&right_close_border)
                {
                    selectable_panes.retain(|t| {
                        self.pane_is_between_vertical_borders(
                            t,
                            left_close_border,
                            right_close_border,
                        )
                    });
                    return Some(selectable_panes);
                }
            }
        }
        None
    }
    fn panes_below_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        if let Some(terminal) = self.panes.get(&id) {
            let left_close_border = terminal.x();
            let right_close_border = terminal.x() + terminal.cols();

            if let Some(terminals_below) = self.pane_ids_directly_below(&id) {
                let mut selectable_panes: Vec<_> = terminals_below
                    .into_iter()
                    .filter(|pid| self.panes[pid].selectable())
                    .collect();
                let terminal_borders_below = self.vertical_borders(&selectable_panes);
                if terminal_borders_below.contains(&left_close_border)
                    && terminal_borders_below.contains(&right_close_border)
                {
                    selectable_panes.retain(|t| {
                        self.pane_is_between_vertical_borders(
                            t,
                            left_close_border,
                            right_close_border,
                        )
                    });
                    return Some(selectable_panes);
                }
            }
        }
        None
    }
    fn close_down_to_max_terminals(&mut self) {
        if let Some(max_panes) = self.max_panes {
            let terminals = self.get_pane_ids();
            for &pid in terminals.iter().skip(max_panes - 1) {
                self.senders
                    .send_to_pty(PtyInstruction::ClosePane(pid))
                    .unwrap();
                self.close_pane(pid);
            }
        }
    }
    pub fn get_pane_ids(&self) -> Vec<PaneId> {
        self.get_panes().map(|(&pid, _)| pid).collect()
    }
    pub fn set_pane_selectable(&mut self, id: PaneId, selectable: bool) {
        if let Some(pane) = self.panes.get_mut(&id) {
            pane.set_selectable(selectable);
            if !selectable {
                // there are some edge cases in which this causes a hard crash when there are no
                // other selectable panes - ideally this should never happen unless it's a
                // configuration error - but this *does* sometimes happen with the default
                // configuration as well since we set this at run time. I left this here because
                // this should very rarely happen and I hope in my heart that we will stop setting
                // this at runtime in the default configuration at some point
                //
                // If however this is not the case and we find this does cause crashes, we can
                // solve it by adding a "dangling_clients" struct to Tab which we would fill with
                // the relevant client ids in this case and drain as soon as a new selectable pane
                // is opened
                self.move_clients_out_of_pane(id);
            }
        }
    }
    fn move_clients_out_of_pane(&mut self, pane_id: PaneId) {
        let active_panes: Vec<(ClientId, PaneId)> = self.active_panes.iter().map(|(cid, pid)| (*cid, *pid)).collect();
        for (client_id, active_pane_id) in active_panes {
            if active_pane_id == pane_id {
                self.active_panes.insert(client_id, self.next_active_pane(&self.get_pane_ids()).unwrap());
            }
        }
    }
    pub fn close_pane(&mut self, id: PaneId) {
        if self.fullscreen_is_active {
            self.unset_fullscreen();
        }
        if let Some(pane_to_close) = self.panes.get(&id) {
            let freed_space = pane_to_close.position_and_size();
            if let (Some(freed_width), Some(freed_height)) =
                (freed_space.cols.as_percent(), freed_space.rows.as_percent())
            {
                if let Some(panes) = self.panes_to_the_left_between_aligning_borders(id) {
                    for pane_id in panes.iter() {
                        self.increase_pane_width(pane_id, freed_width);
                    }
                    self.panes.remove(&id);
                    self.move_clients_out_of_pane(id);
                    self.relayout_tab(Direction::Horizontal);
                    return;
                }
                if let Some(panes) = self.panes_to_the_right_between_aligning_borders(id) {
                    for pane_id in panes.iter() {
                        self.increase_pane_width(pane_id, freed_width);
                    }
                    self.panes.remove(&id);
                    self.move_clients_out_of_pane(id);
                    self.relayout_tab(Direction::Horizontal);
                    return;
                }
                if let Some(panes) = self.panes_above_between_aligning_borders(id) {
                    for pane_id in panes.iter() {
                        self.increase_pane_height(pane_id, freed_height);
                    }
                    self.panes.remove(&id);
                    self.move_clients_out_of_pane(id);
                    self.relayout_tab(Direction::Vertical);
                    return;
                }
                if let Some(panes) = self.panes_below_between_aligning_borders(id) {
                    for pane_id in panes.iter() {
                        self.increase_pane_height(pane_id, freed_height);
                    }
                    self.panes.remove(&id);
                    self.move_clients_out_of_pane(id);
                    self.relayout_tab(Direction::Vertical);
                    return;
                }
            }
            // if we reached here, this is either the last pane or there's some sort of
            // configuration error (eg. we're trying to close a pane surrounded by fixed panes)
            self.panes.remove(&id);
            self.active_panes.clear();
            self.resize_whole_tab(self.display_area);
        }
    }
    pub fn close_focused_pane(&mut self, client_id: ClientId) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            self.close_pane(active_pane_id);
            self.senders
                .send_to_pty(PtyInstruction::ClosePane(active_pane_id))
                .unwrap();
        }
    }
    pub fn scroll_active_terminal_up(&mut self, client_id: ClientId) {
        if let Some(active_terminal_id) = self.get_active_terminal_id(client_id) {
            let active_terminal = self
                .panes
                .get_mut(&PaneId::Terminal(active_terminal_id))
                .unwrap();
            active_terminal.scroll_up(1);
        }
    }
    pub fn scroll_active_terminal_down(&mut self, client_id: ClientId) {
        if let Some(active_terminal_id) = self.get_active_terminal_id(client_id) {
            let active_terminal = self
                .panes
                .get_mut(&PaneId::Terminal(active_terminal_id))
                .unwrap();
            active_terminal.scroll_down(1);
            if !active_terminal.is_scrolled() {
                self.process_pending_vte_events(active_terminal_id);
            }
        }
    }
    pub fn scroll_active_terminal_up_page(&mut self, client_id: ClientId) {
        if let Some(active_terminal_id) = self.get_active_terminal_id(client_id) {
            let active_terminal = self
                .panes
                .get_mut(&PaneId::Terminal(active_terminal_id))
                .unwrap();
            // prevent overflow when row == 0
            let scroll_columns = active_terminal.rows().max(1) - 1;
            active_terminal.scroll_up(scroll_columns);
        }
    }
    pub fn scroll_active_terminal_down_page(&mut self, client_id: ClientId) {
        if let Some(active_terminal_id) = self.get_active_terminal_id(client_id) {
            let active_terminal = self
                .panes
                .get_mut(&PaneId::Terminal(active_terminal_id))
                .unwrap();
            // prevent overflow when row == 0
            let scroll_columns = active_terminal.rows().max(1) - 1;
            active_terminal.scroll_down(scroll_columns);
            if !active_terminal.is_scrolled() {
                self.process_pending_vte_events(active_terminal_id);
            }
        }
    }
    pub fn scroll_active_terminal_to_bottom(&mut self, client_id: ClientId) {
        if let Some(active_terminal_id) = self.get_active_terminal_id(client_id) {
            let active_terminal = self
                .panes
                .get_mut(&PaneId::Terminal(active_terminal_id))
                .unwrap();
            active_terminal.clear_scroll();
            if !active_terminal.is_scrolled() {
                self.process_pending_vte_events(active_terminal_id);
            }
        }
    }
    pub fn clear_active_terminal_scroll(&mut self, client_id: ClientId) {
        if let Some(active_terminal_id) = self.get_active_terminal_id(client_id) {
            let active_terminal = self
                .panes
                .get_mut(&PaneId::Terminal(active_terminal_id))
                .unwrap();
            active_terminal.clear_scroll();
            if !active_terminal.is_scrolled() {
                self.process_pending_vte_events(active_terminal_id);
            }
        }
    }
    pub fn scroll_terminal_up(&mut self, point: &Position, lines: usize) {
        if let Some(pane) = self.get_pane_at(point, false) {
            pane.scroll_up(lines);
        }
    }
    pub fn scroll_terminal_down(&mut self, point: &Position, lines: usize) {
        if let Some(pane) = self.get_pane_at(point, false) {
            pane.scroll_down(lines);
            if !pane.is_scrolled() {
                if let PaneId::Terminal(pid) = pane.pid() {
                    self.process_pending_vte_events(pid);
                }
            }
        }
    }
    fn get_pane_at(
        &mut self,
        point: &Position,
        search_selectable: bool,
    ) -> Option<&mut Box<dyn Pane>> {
        if let Some(pane_id) = self.get_pane_id_at(point, search_selectable) {
            self.panes.get_mut(&pane_id)
        } else {
            None
        }
    }

    fn get_pane_id_at(&self, point: &Position, search_selectable: bool) -> Option<PaneId> {
        if self.fullscreen_is_active {
            let first_client_id = self.connected_clients.iter().next().unwrap(); // this is a temporary hack until we fix the ui for multiple clients
            return self.get_active_pane_id(*first_client_id);
        }
        if search_selectable {
            self.get_selectable_panes()
                .find(|(_, p)| p.contains(point))
                .map(|(&id, _)| id)
        } else {
            self.get_panes()
                .find(|(_, p)| p.contains(point))
                .map(|(&id, _)| id)
        }
    }
    pub fn handle_left_click(&mut self, position: &Position, client_id: ClientId) {
        self.focus_pane_at(position, client_id);

        if let Some(pane) = self.get_pane_at(position, false) {
            let relative_position = pane.relative_position(position);
            pane.start_selection(&relative_position);
        };
    }
    fn focus_pane_at(&mut self, point: &Position, client_id: ClientId) {
        if let Some(clicked_pane) = self.get_pane_id_at(point, true) {
            let connected_clients: Vec<ClientId> = self.connected_clients.iter().copied().collect();
            for client_id in connected_clients {
                self.active_panes.insert(client_id, clicked_pane);
            }
        }
    }
    pub fn handle_mouse_release(&mut self, position: &Position, client_id: ClientId) {
        let active_pane_id = self.get_active_pane_id(client_id);
        // on release, get the selected text from the active pane, and reset it's selection
        let mut selected_text = None;
        if active_pane_id != self.get_pane_id_at(position, true) {
            if let Some(active_pane_id) = active_pane_id {
                if let Some(active_pane) = self.panes.get_mut(&active_pane_id) {
                    active_pane.end_selection(None);
                    selected_text = active_pane.get_selected_text();
                    active_pane.reset_selection();
                }
            }
        } else if let Some(pane) = self.get_pane_at(position, true) {
            let relative_position = pane.relative_position(position);
            pane.end_selection(Some(&relative_position));
            selected_text = pane.get_selected_text();
            pane.reset_selection();
        }

        if let Some(selected_text) = selected_text {
            self.write_selection_to_clipboard(&selected_text);
        }
    }
    pub fn handle_mouse_hold(&mut self, position_on_screen: &Position, client_id: ClientId) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            if let Some(active_pane) = self.panes.get_mut(&active_pane_id) {
                let relative_position = active_pane.relative_position(position_on_screen);
                active_pane.update_selection(&relative_position);
            }
        }
    }

    pub fn copy_selection(&self, client_id: ClientId) {
        let selected_text = self.get_active_pane(client_id).and_then(|p| p.get_selected_text());
        if let Some(selected_text) = selected_text {
            self.write_selection_to_clipboard(&selected_text);
            self.senders
                .send_to_plugin(PluginInstruction::Update(None, Event::CopyToClipboard))
                .unwrap();
        }
    }

    fn write_selection_to_clipboard(&self, selection: &str) {
        let mut output = Output::default();
        output.add_clients(&self.connected_clients);
        output.push_str_to_all_clients(&format!(
            "\u{1b}]52;c;{}\u{1b}\\",
            base64::encode(selection)
        ));

        // TODO: ideally we should be sending the Render instruction from the screen
        self.senders
            .send_to_server(ServerInstruction::Render(Some(output)))
            .unwrap();
        self.senders
            .send_to_plugin(PluginInstruction::Update(None, Event::CopyToClipboard))
            .unwrap();
    }
    fn is_inside_viewport(&self, pane_id: &PaneId) -> bool {
        // this is mostly separated to an outside function in order to allow us to pass a clone to
        // it sometimes when we need to get around the borrow checker
        is_inside_viewport(&self.viewport, self.panes.get(pane_id).unwrap())
    }
    fn offset_viewport(&mut self, position_and_size: &Viewport) {
        if position_and_size.x == self.viewport.x
            && position_and_size.x + position_and_size.cols == self.viewport.x + self.viewport.cols
        {
            if position_and_size.y == self.viewport.y {
                self.viewport.y += position_and_size.rows;
                self.viewport.rows -= position_and_size.rows;
            } else if position_and_size.y + position_and_size.rows
                == self.viewport.y + self.viewport.rows
            {
                self.viewport.rows -= position_and_size.rows;
            }
        }
        if position_and_size.y == self.viewport.y
            && position_and_size.y + position_and_size.rows == self.viewport.y + self.viewport.rows
        {
            if position_and_size.x == self.viewport.x {
                self.viewport.x += position_and_size.cols;
                self.viewport.cols -= position_and_size.cols;
            } else if position_and_size.x + position_and_size.cols
                == self.viewport.x + self.viewport.cols
            {
                self.viewport.cols -= position_and_size.cols;
            }
        }
    }

    pub fn visible(&self, visible: bool) {
        let pids_in_this_tab = self.panes.keys().filter_map(|p| match p {
            PaneId::Plugin(pid) => Some(pid),
            _ => None,
        });
        for pid in pids_in_this_tab {
            self.senders
                .send_to_plugin(PluginInstruction::Update(
                    Some(*pid),
                    Event::Visible(visible),
                ))
                .unwrap();
        }
    }
}

#[allow(clippy::borrowed_box)]
fn is_inside_viewport(viewport: &Viewport, pane: &Box<dyn Pane>) -> bool {
    let pane_position_and_size = pane.current_geom();
    pane_position_and_size.y >= viewport.y
        && pane_position_and_size.y + pane_position_and_size.rows.as_usize()
            <= viewport.y + viewport.rows
}

#[cfg(test)]
#[path = "./unit/tab_tests.rs"]
mod tab_tests;

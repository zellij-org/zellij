use zellij_utils::position::{Column, Line};
use zellij_utils::{position::Position, serde, zellij_tile};
use std::collections::VecDeque;

use crate::ui::pane_boundaries_frame::FrameParams;
use crate::panes::selection::Selection;
use crate::panes::Row;

use crate::{
    os_input_output::ServerOsApi,
    panes::{PaneId, PluginPane, TerminalPane, TerminalCharacter, EMPTY_TERMINAL_CHARACTER, LinkHandler},
    pty::{ClientOrTabIndex, PtyInstruction, VteBytes},
    thread_bus::ThreadSenders,
    ui::boundaries::Boundaries,
    ui::pane_contents_and_ui::PaneContentsAndUi,
    wasm_vm::PluginInstruction,
    ClientId, ServerInstruction,
    panes::terminal_character::{
        CharacterStyles, CursorShape, AnsiCode,
    },
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fmt::Write;
use std::os::unix::io::RawFd;
use std::rc::Rc;
use std::sync::mpsc::channel;
use std::time::Instant;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    str,
};
use zellij_tile::data::{Event, InputMode, ModeInfo, Palette, PaletteColor};
use zellij_utils::{
    input::{
        command::{RunCommand, TerminalAction},
        layout::{Direction, Layout, Run},
        parse_keys,
    },
    pane_size::{Offset, PaneGeom, Size, Viewport},
};

const MAX_PENDING_VTE_EVENTS: usize = 7000;

#[derive(Clone, Debug, Default)]
pub struct Output {
    pre_vte_instructions: HashMap<ClientId, Vec<String>>, // TODO: is this actually a good idea? performance-wise and such? lookup in HashSet might be too expensive?
    post_vte_instructions: HashMap<ClientId, Vec<String>>, // TODO: is this actually a good idea? performance-wise and such? lookup in HashSet might be too expensive?
    client_character_chunks: HashMap<ClientId, Vec<CharacterChunk>>,
    link_handler: Option<Rc<RefCell<LinkHandler>>>,
    floating_panes_stack: Option<FloatingPanesStack>,
}

// this belongs to output but is not on its impl because of borrow checker stuffs
fn serialize_character_chunks(character_chunks: Vec<CharacterChunk>, link_handler: Option<&mut Rc<RefCell<LinkHandler>>>) -> String {
    let mut vte_output = String::new();
    for character_chunk in character_chunks {
        let chunk_selection_and_background_color = character_chunk.selection_and_background_color();
        let chunk_changed_colors = character_chunk.changed_colors();
        let mut character_styles = CharacterStyles::new();
        write!(
            &mut vte_output,
            "\u{1b}[{};{}H\u{1b}[m",
            character_chunk.y + 1,
            character_chunk.x + 1,
        )
        .unwrap(); // goto top of viewport

        let mut chunk_width = character_chunk.x;
        for (i, t_character) in character_chunk.terminal_characters.iter().enumerate() {
            let mut t_character_styles = chunk_selection_and_background_color.and_then(|(selection, background_color)| {
                if selection.contains(character_chunk.y, chunk_width) {
                    Some(t_character.styles.background(Some(background_color)))
                } else {
                    None
                }
            }).unwrap_or(t_character.styles);
            if let Some(new_styles) = character_styles.update_and_return_diff(&t_character_styles, chunk_changed_colors) {
                if let Some(osc8_link) = link_handler.as_ref().and_then(|l_h| l_h.borrow().output_osc8(new_styles.link_anchor)) {
                    write!(
                        &mut vte_output,
                        "{}{}",
                        new_styles,
                        osc8_link,
                    )
                    .unwrap();
                } else {
                    write!(
                        &mut vte_output,
                        "{}",
                        new_styles,
                    )
                    .unwrap();
                }

            }
            chunk_width += t_character.width;
            vte_output.push(t_character.character);
        }
        character_styles.clear();
    }
    vte_output
}

impl Output {
    pub fn add_clients(&mut self, client_ids: &HashSet<ClientId>, link_handler: Rc<RefCell<LinkHandler>>, floating_panes_stack: Option<FloatingPanesStack>) {
        self.link_handler = Some(link_handler);
        self.floating_panes_stack = floating_panes_stack;
        for client_id in client_ids {
            self.client_character_chunks
                .insert(*client_id, vec![]);
        }
    }
    pub fn add_character_chunks_to_client(&mut self, client_id: ClientId, mut character_chunks: Vec<CharacterChunk>, z_index: Option<usize>) {
        if let Some(client_character_chunks) = self.client_character_chunks.get_mut(&client_id) {
            if let Some(floating_panes_stack) = &self.floating_panes_stack {
                let mut visible_character_chunks = floating_panes_stack.visible_character_chunks(character_chunks, z_index);
                client_character_chunks.append(&mut visible_character_chunks);
            } else {
                client_character_chunks.append(&mut character_chunks);
            }
        }
    }
    pub fn add_character_chunks_to_multiple_clients(&mut self, character_chunks: Vec<CharacterChunk>, client_ids: impl Iterator<Item = ClientId>, z_index: Option<usize>) {
        for client_id in client_ids {
            self.add_character_chunks_to_client(client_id, character_chunks.clone(), z_index); // TODO: forgo clone by adding an all_clients thing?
        }
    }
    pub fn add_post_vte_instruction_to_multiple_clients(&mut self, client_ids: impl Iterator<Item = ClientId>, vte_instruction: &str) {
        for client_id in client_ids {
            let mut entry = self.post_vte_instructions.entry(client_id).or_insert(vec![]);
            entry.push(String::from(vte_instruction));
        }
    }
    pub fn add_pre_vte_instruction_to_multiple_clients(&mut self, client_ids: impl Iterator<Item = ClientId>, vte_instruction: &str) {
        for client_id in client_ids {
            let mut entry = self.pre_vte_instructions.entry(client_id).or_insert(vec![]);
            entry.push(String::from(vte_instruction));
        }
    }
    pub fn add_post_vte_instruction_to_client(&mut self, client_id: ClientId, vte_instruction: &str) {
        let mut entry = self.post_vte_instructions.entry(client_id).or_insert(vec![]);
        entry.push(String::from(vte_instruction));
    }
    pub fn add_pre_vte_instruction_to_client(&mut self, client_id: ClientId, vte_instruction: &str) {
        let mut entry = self.pre_vte_instructions.entry(client_id).or_insert(vec![]);
        entry.push(String::from(vte_instruction));
    }
    pub fn serialize(&mut self, mut current_cache: Option<&mut HashMap<ClientId, Vec<Vec<Option<TerminalCharacter>>>>>) -> HashMap<ClientId, String> {
        let mut serialized_render_instructions = HashMap::new();
        for (client_id, client_character_chunks) in self.client_character_chunks.drain() {
            let mut client_serialized_render_instructions = String::new();
            if let Some(pre_vte_instructions_for_client) = self.pre_vte_instructions.remove(&client_id) {
                for vte_instruction in pre_vte_instructions_for_client {
                    client_serialized_render_instructions.push_str(&vte_instruction);
                }
            }
            client_serialized_render_instructions.push_str(&serialize_character_chunks(client_character_chunks, self.link_handler.as_mut())); // TODO: less allocations?
            if let Some(post_vte_instructions_for_client) = self.post_vte_instructions.remove(&client_id) {
                for vte_instruction in post_vte_instructions_for_client {
                    client_serialized_render_instructions.push_str(&vte_instruction);
                }
            }

            serialized_render_instructions.insert(client_id, client_serialized_render_instructions);
        }
        serialized_render_instructions
    }
}

#[derive(Debug, Clone, Default)]
pub struct FloatingPanesStack {
    pub layers: Vec<PaneGeom>,
}

impl FloatingPanesStack {
    pub fn visible_character_chunks(&self, mut character_chunks: Vec<CharacterChunk>, z_index: Option<usize>) -> Vec<CharacterChunk> {
        let z_index = z_index.unwrap_or(0);
        let mut chunks_to_check: Vec<CharacterChunk> = character_chunks.drain(..).collect();
        let mut visible_chunks = vec![];
        'chunk_loop: loop {
            match chunks_to_check.pop() {
                Some(mut c_chunk) => {
                    let panes_to_check = self.layers.iter().skip(z_index);
                    for pane_geom in panes_to_check {
                        let pane_top_edge = pane_geom.y;
                        let pane_left_edge = pane_geom.x;
                        let pane_bottom_edge = pane_geom.y + pane_geom.rows.as_usize().saturating_sub(1);
                        let pane_right_edge = pane_geom.x + pane_geom.cols.as_usize().saturating_sub(1);

                        let c_chunk_left_side = c_chunk.x;
                        let c_chunk_right_side = c_chunk.x + (c_chunk.width()).saturating_sub(1);
                        if pane_top_edge <= c_chunk.y && pane_bottom_edge >= c_chunk.y {
                            if pane_left_edge <= c_chunk_left_side && pane_right_edge >= c_chunk_right_side {
                                // pane covers chunk completely
                                continue 'chunk_loop;
                            } else if pane_right_edge > c_chunk_left_side && pane_right_edge < c_chunk_right_side && pane_left_edge <= c_chunk_left_side {
                                // pane covers chunk partially to the left
                                c_chunk.drain_by_width(pane_right_edge + 1 - c_chunk_left_side); // 2 - one to get to the actual right edge, one to get an extra character
                                c_chunk.x = pane_right_edge + 1;
                            } else if pane_left_edge > c_chunk_left_side && pane_left_edge < c_chunk_right_side && pane_right_edge >= c_chunk_right_side {
                                // pane covers chunk partially to the right
                                c_chunk.retain_by_width(pane_left_edge - c_chunk_left_side);
                            } else if pane_left_edge >= c_chunk_left_side && pane_right_edge <= c_chunk_right_side {
                                // pane covers chunk middle
                                let (left_chunk_characters, right_chunk_characters) = c_chunk.cut_middle_out(pane_left_edge - c_chunk_left_side, (pane_right_edge + 1) - c_chunk_left_side);
                                let left_chunk_x = c_chunk_left_side;
                                let right_chunk_x = pane_right_edge + 1;
                                let left_chunk = CharacterChunk::new(left_chunk_characters, left_chunk_x, c_chunk.y);
                                c_chunk.x = right_chunk_x;
                                c_chunk.terminal_characters = right_chunk_characters;
                                chunks_to_check.push(left_chunk);
                            }
                        }
                    }
                    visible_chunks.push(c_chunk);
                },
                None => {
                    break 'chunk_loop;
                }
            }
        }
        visible_chunks
    }
}

#[derive(Debug, Clone, Default)]
pub struct CharacterChunk {
    pub terminal_characters: Vec<TerminalCharacter>,
    pub x: usize,
    pub y: usize,
    pub changed_colors: Option<[Option<AnsiCode>; 256]>,
    selection_and_background_color: Option<(Selection, AnsiCode)>,
}

impl CharacterChunk {
    pub fn new(terminal_characters: Vec<TerminalCharacter>, x: usize, y: usize) -> Self {
        CharacterChunk {
            terminal_characters,
            x,
            y,
            ..Default::default()
        }
    }
    pub fn add_selection_and_background(&mut self, selection: Selection, background_color: AnsiCode, offset_x: usize, offset_y: usize) {
        self.selection_and_background_color = Some((selection.offset(offset_x, offset_y), background_color));
    }
    pub fn selection_and_background_color(&self) -> Option<(Selection, AnsiCode)> {
        self.selection_and_background_color
    }
    pub fn add_changed_colors(&mut self, changed_colors: Option<[Option<AnsiCode>; 256]>) {
        self.changed_colors = changed_colors;
    }
    pub fn changed_colors(&self) -> Option<[Option<AnsiCode>; 256]> {
        self.changed_colors
    }
    pub fn width(&self) -> usize {
        let mut width = 0;
        for t_character in &self.terminal_characters {
            width += t_character.width
        }
        width
    }
    pub fn drain_by_width(&mut self, x: usize) -> impl Iterator<Item=TerminalCharacter> {
        let mut drained_part: VecDeque<TerminalCharacter> = VecDeque::new();
        let mut drained_part_len = 0;
        loop {
            if self.terminal_characters.is_empty() {
                break;
            }
            let next_character = self.terminal_characters.remove(0); // TODO: consider copying self.terminal_characters into a VecDeque to make this process faster?
            if drained_part_len + next_character.width <= x {
                drained_part.push_back(next_character);
                drained_part_len += next_character.width;
            } else {
                if drained_part_len == x {
                    self.terminal_characters.insert(0, next_character); // put it back
                } else if next_character.width > 1 {
                    for _ in 1..next_character.width {
                        self.terminal_characters.insert(0, EMPTY_TERMINAL_CHARACTER);
                        drained_part.push_back(EMPTY_TERMINAL_CHARACTER);
                    }
                }
                break;
            }
        }
        drained_part.into_iter()
    }
    pub fn retain_by_width(&mut self, x: usize) {
        let part_to_retain = self.drain_by_width(x); // TODO: better
        self.terminal_characters = part_to_retain.collect();
    }
    pub fn cut_middle_out(&mut self, middle_start: usize, middle_end: usize) -> (Vec<TerminalCharacter>, Vec<TerminalCharacter>) {
        let mut absolute_middle_start_index = None;
        let mut absolute_middle_end_index = None;
        let mut current_x = 0;
        let mut pad_left_end_by = 0;
        let mut pad_right_start_by = 0;
        for (absolute_index, t_character) in self.terminal_characters.iter().enumerate() {
            current_x += t_character.width;
            if current_x >= middle_start && absolute_middle_start_index.is_none() {
                if current_x > middle_start {
                    pad_left_end_by = current_x - middle_start;
                    absolute_middle_start_index = Some(absolute_index);
                } else {
                    absolute_middle_start_index = Some(absolute_index + 1);
                }
            }
            if current_x >= middle_end && absolute_middle_end_index.is_none() {
                absolute_middle_end_index = Some(absolute_index + 1);
                if current_x > middle_end {
                    pad_right_start_by = current_x - middle_end;
                }
            }
        }
        let absolute_middle_start_index = absolute_middle_start_index.unwrap();
        let absolute_middle_end_index = absolute_middle_end_index.unwrap();
        let mut terminal_characters: Vec<TerminalCharacter> = self.terminal_characters.drain(..).collect();
        let mut characters_on_the_right: Vec<TerminalCharacter> = terminal_characters.drain(absolute_middle_end_index..).collect();
        let mut characters_on_the_left: Vec<TerminalCharacter>  = terminal_characters.drain(..absolute_middle_start_index).collect();
        if pad_left_end_by > 0 {
            for _ in 0..pad_left_end_by {
                characters_on_the_left.push(EMPTY_TERMINAL_CHARACTER);
            }
        }
        if pad_right_start_by > 0 {
            for _ in 0..pad_right_start_by {
                characters_on_the_right.insert(0, EMPTY_TERMINAL_CHARACTER);
            }
        }
        (characters_on_the_left, characters_on_the_right)
    }
}

#[derive(Clone, Debug)]
pub struct OutputBuffer {
    changed_lines: Vec<usize>, // line index
    should_update_all_lines: bool,
}

impl Default for OutputBuffer {
    fn default() -> Self {
        OutputBuffer {
            changed_lines: vec![],
            should_update_all_lines: true, // first time we should do a full render
        }
    }
}

impl OutputBuffer {
    pub fn update_line(&mut self, line_index: usize) {
        if !self.should_update_all_lines {
            self.changed_lines.push(line_index);
        }
    }
    pub fn update_all_lines(&mut self) {
        self.clear();
        self.should_update_all_lines = true;
    }
    pub fn clear(&mut self) {
        self.changed_lines.clear();
        self.should_update_all_lines = false;
    }
    pub fn changed_chunks_in_viewport(
        &self,
        viewport: &[Row],
        viewport_width: usize,
        viewport_height: usize,
        x_offset: usize,
        y_offset: usize,
    ) -> Vec<CharacterChunk> {
        if self.should_update_all_lines {
            let mut changed_chunks = Vec::with_capacity(viewport.len());
            for line_index in 0..viewport_height {
                let terminal_characters =
                    self.extract_line_from_viewport(line_index, viewport, viewport_width);
                let x = 0 + x_offset; // right now we only buffer full lines as this doesn't seem to have a huge impact on performance, but the infra is here if we want to change this
                let y = line_index + y_offset;
                changed_chunks.push(CharacterChunk::new(terminal_characters, x, y));
            }
            changed_chunks
        } else {
            let mut line_changes = self.changed_lines.to_vec();
            line_changes.sort_unstable();
            line_changes.dedup();
            let mut changed_chunks = Vec::with_capacity(line_changes.len());
            for line_index in line_changes {
                let terminal_characters =
                    self.extract_line_from_viewport(line_index, viewport, viewport_width);
                let x = 0 + x_offset;
                let y = line_index + y_offset;
                changed_chunks.push(CharacterChunk::new(terminal_characters, x, y));
            }
            changed_chunks
        }
    }
    fn extract_characters_from_row(
        &self,
        row: &Row,
        viewport_width: usize,
    ) -> Vec<TerminalCharacter> {
        let mut terminal_characters: Vec<TerminalCharacter> = row.columns.iter().copied().collect();
        // pad row
        let row_width = row.width();
        if row_width < viewport_width {
            let mut padding = vec![EMPTY_TERMINAL_CHARACTER; viewport_width - row_width];
            terminal_characters.append(&mut padding);
        }
        terminal_characters
    }
    fn extract_line_from_viewport(
        &self,
        line_index: usize,
        viewport: &[Row],
        viewport_width: usize,
    ) -> Vec<TerminalCharacter> {
        match viewport.get(line_index) {
            // TODO: iterator?
            Some(row) => self.extract_characters_from_row(row, viewport_width),
            None => {
                vec![EMPTY_TERMINAL_CHARACTER; viewport_width]
            }
        }
    }
}

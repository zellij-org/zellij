//! `Tab`s holds multiple panes. It tracks their coordinates (x/y) and size,
//! as well as how they should be resized

use zellij_utils::position::{Column, Line};
use zellij_utils::{position::Position, serde, zellij_tile};

use crate::ui::pane_boundaries_frame::FrameParams;
// use pane_grid::{split, FloatingPaneGrid, PaneGrid};

use crate::{
    os_input_output::ServerOsApi,
    panes::{PaneId, PluginPane, TerminalPane, TerminalCharacter, EMPTY_TERMINAL_CHARACTER, CharacterChunk, LinkHandler},
    pty::{ClientOrTabIndex, PtyInstruction, VteBytes},
    thread_bus::ThreadSenders,
    ui::boundaries::Boundaries,
    ui::pane_contents_and_ui::PaneContentsAndUi,
    wasm_vm::PluginInstruction,
    ClientId, ServerInstruction,
    panes::terminal_character::{
        CharacterStyles, CursorShape,
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

macro_rules! resize_pty {
    ($pane:expr, $os_input:expr) => {
        if let PaneId::Terminal(ref pid) = $pane.pid() {
            // FIXME: This `set_terminal_size_using_fd` call would be best in
            // `TerminalPane::reflow_lines`
            $os_input.set_terminal_size_using_fd(
                *pid,
                $pane.get_content_columns() as u16,
                $pane.get_content_rows() as u16,
            );
        }
    };
}


// FIXME: This should be replaced by `RESIZE_PERCENT` at some point
const MIN_TERMINAL_HEIGHT: usize = 5;
const MIN_TERMINAL_WIDTH: usize = 5;

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

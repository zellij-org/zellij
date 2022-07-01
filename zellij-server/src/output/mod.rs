use std::collections::VecDeque;

use crate::panes::selection::Selection;
use crate::panes::Row;

use zellij_utils::pane_size::SizeInPixels;
use crate::{
    panes::sixel::SixelImageStore,
    panes::terminal_character::{AnsiCode, CharacterStyles},
    panes::{LinkHandler, TerminalCharacter, EMPTY_TERMINAL_CHARACTER},
    ClientId,
};
use std::cell::RefCell;
use std::fmt::Write;
use std::rc::Rc;
use std::{
    collections::{HashMap, HashSet},
    str,
};
use zellij_utils::pane_size::PaneGeom;

fn vte_goto_instruction(x_coords: usize, y_coords: usize, vte_output: &mut String) {
    write!(
        vte_output,
        "\u{1b}[{};{}H\u{1b}[m",
        y_coords + 1, // + 1 because VTE is 1 indexed
        x_coords + 1,
    )
    .unwrap();
}

fn adjust_styles_for_possible_selection(
    chunk_selection_and_background_color: Option<(Selection, AnsiCode)>,
    character_styles: CharacterStyles,
    chunk_y: usize,
    chunk_width: usize,
) -> CharacterStyles {
    chunk_selection_and_background_color
        .and_then(|(selection, background_color)| {
            if selection.contains(chunk_y, chunk_width) {
                Some(character_styles.background(Some(background_color)))
            } else {
                None
            }
        })
        .unwrap_or(character_styles)
}

fn write_changed_styles(
    character_styles: &mut CharacterStyles,
    current_character_styles: CharacterStyles,
    chunk_changed_colors: Option<[Option<AnsiCode>; 256]>,
    link_handler: Option<&std::cell::Ref<LinkHandler>>,
    vte_output: &mut String,
) {
    if let Some(new_styles) =
        character_styles.update_and_return_diff(&current_character_styles, chunk_changed_colors)
    {
        if let Some(osc8_link) =
            link_handler.and_then(|l_h| l_h.output_osc8(new_styles.link_anchor))
        {
            write!(vte_output, "{}{}", new_styles, osc8_link).unwrap();
        } else {
            write!(vte_output, "{}", new_styles).unwrap();
        }
    }
}

fn serialize_chunks(
    character_chunks: Vec<CharacterChunk>,
    sixel_chunks: &Vec<SixelImageChunk>,
    link_handler: Option<&mut Rc<RefCell<LinkHandler>>>,
    sixel_image_store: &mut SixelImageStore,
) -> String {
    let mut vte_output = String::new();
    let mut sixel_vte: Option<String> = None;
    let link_handler = link_handler.map(|l_h| l_h.borrow());
    for character_chunk in character_chunks {
        let chunk_selection_and_background_color = character_chunk.selection_and_background_color();
        let chunk_changed_colors = character_chunk.changed_colors();
        let mut character_styles = CharacterStyles::new();
        vte_goto_instruction(character_chunk.x, character_chunk.y, &mut vte_output);
        let mut chunk_width = character_chunk.x;
        for t_character in character_chunk.terminal_characters.iter() {
            let current_character_styles = adjust_styles_for_possible_selection(
                chunk_selection_and_background_color,
                t_character.styles,
                character_chunk.y,
                chunk_width,
            );
            write_changed_styles(
                &mut character_styles,
                current_character_styles,
                chunk_changed_colors,
                link_handler.as_ref(),
                &mut vte_output,
            );
            chunk_width += t_character.width;
            vte_output.push(t_character.character);
        }
        character_styles.clear();
    }
    for sixel_chunk in sixel_chunks {
        let serialized_sixel_image = sixel_image_store.serialize_image(
            sixel_chunk.sixel_image_id,
            sixel_chunk.sixel_image_pixel_x,
            sixel_chunk.sixel_image_pixel_y,
            sixel_chunk.sixel_image_pixel_width,
            sixel_chunk.sixel_image_pixel_height
        );
        if let Some(serialized_sixel_image) = serialized_sixel_image {
            let mut sixel_vte = sixel_vte.get_or_insert_with(|| String::new());
            vte_goto_instruction(sixel_chunk.cell_x, sixel_chunk.cell_y, &mut sixel_vte);
            sixel_vte.push_str(&serialized_sixel_image);
        }
    }
    if let Some(ref sixel_vte) = sixel_vte {
        // we do this at the end because of the implied z-index,
        // images should be above text unless the text was explicitly inserted after them (the
        // latter being a case we handle in our own internal state and not in the output)
        let save_cursor_position = "\u{1b}[s";
        let restore_cursor_position = "\u{1b}[u";
        vte_output.push_str(save_cursor_position);
        vte_output.push_str(&sixel_vte);
        vte_output.push_str(restore_cursor_position);
    }
    vte_output
}

type AbsoluteMiddleStart = usize;
type AbsoluteMiddleEnd = usize;
type PadLeftEndBy = usize;
type PadRightStartBy = usize;
fn adjust_middle_segment_for_wide_chars(
    middle_start: usize,
    middle_end: usize,
    terminal_characters: &[TerminalCharacter],
) -> (
    AbsoluteMiddleStart,
    AbsoluteMiddleEnd,
    PadLeftEndBy,
    PadRightStartBy,
) {
    let mut absolute_middle_start_index = None;
    let mut absolute_middle_end_index = None;
    let mut current_x = 0;
    let mut pad_left_end_by = 0;
    let mut pad_right_start_by = 0;
    for (absolute_index, t_character) in terminal_characters.iter().enumerate() {
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
    (
        absolute_middle_start_index.unwrap(),
        absolute_middle_end_index.unwrap(),
        pad_left_end_by,
        pad_right_start_by,
    )
}

#[derive(Clone, Debug, Default)]
pub struct Output {
    pre_vte_instructions: HashMap<ClientId, Vec<String>>,
    post_vte_instructions: HashMap<ClientId, Vec<String>>,
    client_character_chunks: HashMap<ClientId, Vec<CharacterChunk>>,
    sixel_chunks: Vec<SixelImageChunk>,
    link_handler: Option<Rc<RefCell<LinkHandler>>>,
    sixel_image_store: Rc<RefCell<SixelImageStore>>,
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
    floating_panes_stack: Option<FloatingPanesStack>,
}

impl Output {
    pub fn new(sixel_image_store: Rc<RefCell<SixelImageStore>>, character_cell_size: Rc<RefCell<Option<SizeInPixels>>>) -> Self {
        Output {
            sixel_image_store,
            character_cell_size,
            ..Default::default()
        }
    }
    pub fn add_clients(
        &mut self,
        client_ids: &HashSet<ClientId>,
        link_handler: Rc<RefCell<LinkHandler>>,
        floating_panes_stack: Option<FloatingPanesStack>,
    ) {
        self.link_handler = Some(link_handler);
        self.floating_panes_stack = floating_panes_stack;
        for client_id in client_ids {
            self.client_character_chunks.insert(*client_id, vec![]);
        }
    }
    pub fn add_character_chunks_to_client(
        &mut self,
        client_id: ClientId,
        mut character_chunks: Vec<CharacterChunk>,
        z_index: Option<usize>,
    ) {
        if let Some(client_character_chunks) = self.client_character_chunks.get_mut(&client_id) {
            if let Some(floating_panes_stack) = &self.floating_panes_stack {
                let mut visible_character_chunks =
                    floating_panes_stack.visible_character_chunks(character_chunks, z_index);
                client_character_chunks.append(&mut visible_character_chunks);
            } else {
                client_character_chunks.append(&mut character_chunks);
            }
        }
    }
    pub fn add_character_chunks_to_multiple_clients(
        &mut self,
        character_chunks: Vec<CharacterChunk>,
        client_ids: impl Iterator<Item = ClientId>,
        z_index: Option<usize>,
    ) {
        for client_id in client_ids {
            self.add_character_chunks_to_client(client_id, character_chunks.clone(), z_index);
            // TODO: forgo clone by adding an all_clients thing?
        }
    }
    pub fn add_post_vte_instruction_to_multiple_clients(
        &mut self,
        client_ids: impl Iterator<Item = ClientId>,
        vte_instruction: &str,
    ) {
        for client_id in client_ids {
            let entry = self
                .post_vte_instructions
                .entry(client_id)
                .or_insert_with(Vec::new);
            entry.push(String::from(vte_instruction));
        }
    }
    pub fn add_pre_vte_instruction_to_multiple_clients(
        &mut self,
        client_ids: impl Iterator<Item = ClientId>,
        vte_instruction: &str,
    ) {
        for client_id in client_ids {
            let entry = self
                .pre_vte_instructions
                .entry(client_id)
                .or_insert_with(Vec::new);
            entry.push(String::from(vte_instruction));
        }
    }
    pub fn add_post_vte_instruction_to_client(
        &mut self,
        client_id: ClientId,
        vte_instruction: &str,
    ) {
        let entry = self
            .post_vte_instructions
            .entry(client_id)
            .or_insert_with(Vec::new);
        entry.push(String::from(vte_instruction));
    }
    pub fn add_pre_vte_instruction_to_client(
        &mut self,
        client_id: ClientId,
        vte_instruction: &str,
    ) {
        let entry = self
            .pre_vte_instructions
            .entry(client_id)
            .or_insert_with(Vec::new);
        entry.push(String::from(vte_instruction));
    }
    pub fn add_sixel_image_chunks_to_all_clients(&mut self, mut sixel_image_chunks: Vec<SixelImageChunk>, z_index: Option<usize>) {
        if let Some(character_cell_size) = *self.character_cell_size.borrow() {
            if let Some(floating_panes_stack) = &self.floating_panes_stack {
                let mut visible_sixel_image_chunks =
                    floating_panes_stack.visible_sixel_image_chunks(sixel_image_chunks, z_index, &character_cell_size);
                self.sixel_chunks.append(&mut visible_sixel_image_chunks);
            } else {
                self.sixel_chunks.append(&mut sixel_image_chunks);
            }
        }
    }
    pub fn serialize(&mut self) -> HashMap<ClientId, String> {
        let mut serialized_render_instructions = HashMap::new();

        for (client_id, client_character_chunks) in self.client_character_chunks.drain() {
            let mut client_serialized_render_instructions = String::new();

            // append pre-vte instructions for this client
            if let Some(pre_vte_instructions_for_client) =
                self.pre_vte_instructions.remove(&client_id)
            {
                for vte_instruction in pre_vte_instructions_for_client {
                    client_serialized_render_instructions.push_str(&vte_instruction);
                }
            }

            // append the actual vte
            client_serialized_render_instructions.push_str(&serialize_chunks(
                client_character_chunks,
                &self.sixel_chunks,
                self.link_handler.as_mut(),
                &mut self.sixel_image_store.borrow_mut(),
            )); // TODO: less allocations?

            // append post-vte instructions for this client
            if let Some(post_vte_instructions_for_client) =
                self.post_vte_instructions.remove(&client_id)
            {
                for vte_instruction in post_vte_instructions_for_client {
                    client_serialized_render_instructions.push_str(&vte_instruction);
                }
            }

            serialized_render_instructions.insert(client_id, client_serialized_render_instructions);
        }
        serialized_render_instructions
    }
}

// this struct represents the geometry of a group of floating panes
// we use it to filter out CharacterChunks who are behind these geometries
// and so would not be visible. If a chunk is partially covered, it is adjusted
// to include only the non-covered parts
#[derive(Debug, Clone, Default)]
pub struct FloatingPanesStack {
    pub layers: Vec<PaneGeom>,
}

impl FloatingPanesStack {
    pub fn visible_character_chunks(
        &self,
        mut character_chunks: Vec<CharacterChunk>,
        z_index: Option<usize>,
    ) -> Vec<CharacterChunk> {
        let z_index = z_index.unwrap_or(0);
        let mut chunks_to_check: Vec<CharacterChunk> = character_chunks.drain(..).collect();
        let mut visible_chunks = vec![];
        'chunk_loop: loop {
            match chunks_to_check.pop() {
                Some(mut c_chunk) => {
                    let panes_to_check = self.layers.iter().skip(z_index);
                    for pane_geom in panes_to_check {
                        let new_chunk_to_check = self.remove_covered_parts(pane_geom, &mut c_chunk);
                        if let Some(new_chunk_to_check) = new_chunk_to_check {
                            // this happens when the pane covers the middle of the chunk, and so we
                            // end up with an extra chunk we need to check (eg. against panes above
                            // this one)
                            chunks_to_check.push(new_chunk_to_check);
                        }
                        if c_chunk.terminal_characters.is_empty() {
                            continue 'chunk_loop;
                        }
                    }
                    visible_chunks.push(c_chunk);
                }
                None => {
                    break 'chunk_loop;
                }
            }
        }
        visible_chunks
    }
    pub fn visible_sixel_image_chunks(
        &self,
        mut sixel_image_chunks: Vec<SixelImageChunk>,
        z_index: Option<usize>,
        character_cell_size: &SizeInPixels,
    ) -> Vec<SixelImageChunk> {
        let z_index = z_index.unwrap_or(0);
        let mut chunks_to_check: Vec<SixelImageChunk> = sixel_image_chunks.drain(..).collect();
        let panes_to_check = self.layers.iter().skip(z_index);
        for pane_geom in panes_to_check {
            let chunks_to_check_against_this_pane: Vec<SixelImageChunk> = chunks_to_check.drain(..).collect();
            for s_chunk in chunks_to_check_against_this_pane {
                let mut uncovered_chunks = self.remove_covered_sixel_parts(pane_geom, &s_chunk, character_cell_size);
                chunks_to_check.append(&mut uncovered_chunks);
            }
        }
        chunks_to_check
    }
    fn remove_covered_parts(
        &self,
        pane_geom: &PaneGeom,
        c_chunk: &mut CharacterChunk,
    ) -> Option<CharacterChunk> {
        let pane_top_edge = pane_geom.y;
        let pane_left_edge = pane_geom.x;
        let pane_bottom_edge = pane_geom.y + pane_geom.rows.as_usize().saturating_sub(1);
        let pane_right_edge = pane_geom.x + pane_geom.cols.as_usize().saturating_sub(1);
        let c_chunk_left_side = c_chunk.x;
        let c_chunk_right_side = c_chunk.x + (c_chunk.width()).saturating_sub(1);
        if pane_top_edge <= c_chunk.y && pane_bottom_edge >= c_chunk.y {
            if pane_left_edge <= c_chunk_left_side && pane_right_edge >= c_chunk_right_side {
                // pane covers chunk completely
                drop(c_chunk.terminal_characters.drain(..));
                return None;
            } else if pane_right_edge > c_chunk_left_side
                && pane_right_edge < c_chunk_right_side
                && pane_left_edge <= c_chunk_left_side
            {
                // pane covers chunk partially to the left
                let covered_part = c_chunk.drain_by_width(pane_right_edge + 1 - c_chunk_left_side);
                drop(covered_part);
                c_chunk.x = pane_right_edge + 1;
                return None;
            } else if pane_left_edge > c_chunk_left_side
                && pane_left_edge < c_chunk_right_side
                && pane_right_edge >= c_chunk_right_side
            {
                // pane covers chunk partially to the right
                c_chunk.retain_by_width(pane_left_edge - c_chunk_left_side);
                return None;
            } else if pane_left_edge >= c_chunk_left_side && pane_right_edge <= c_chunk_right_side {
                // pane covers chunk middle
                let (left_chunk_characters, right_chunk_characters) = c_chunk.cut_middle_out(
                    pane_left_edge - c_chunk_left_side,
                    (pane_right_edge + 1) - c_chunk_left_side,
                );
                let left_chunk_x = c_chunk_left_side;
                let right_chunk_x = pane_right_edge + 1;
                let left_chunk =
                    CharacterChunk::new(left_chunk_characters, left_chunk_x, c_chunk.y);
                c_chunk.x = right_chunk_x;
                c_chunk.terminal_characters = right_chunk_characters;
                return Some(left_chunk);
            }
        };
        None
    }
    fn remove_covered_sixel_parts(
        &self,
        pane_geom: &PaneGeom,
        s_chunk: &SixelImageChunk,
        character_cell_size: &SizeInPixels,
    ) -> Vec<SixelImageChunk> {
        // round these up to the nearest cell edge
        let rounded_sixel_image_pixel_height = if s_chunk.sixel_image_pixel_height % character_cell_size.height > 0 {
            let modulus = s_chunk.sixel_image_pixel_height % character_cell_size.height;
            s_chunk.sixel_image_pixel_height + (character_cell_size.height - modulus)
        } else {
            s_chunk.sixel_image_pixel_height
        };
        let rounded_sixel_image_pixel_width = if s_chunk.sixel_image_pixel_width % character_cell_size.width > 0 {
            let modulus = s_chunk.sixel_image_pixel_width % character_cell_size.width;
            s_chunk.sixel_image_pixel_width + (character_cell_size.width - modulus)
        } else {
            s_chunk.sixel_image_pixel_width
        };

        let pane_top_edge = pane_geom.y * character_cell_size.height;
        let pane_left_edge = pane_geom.x * character_cell_size.width;
        let pane_bottom_edge = (pane_geom.y + pane_geom.rows.as_usize().saturating_sub(1)) * character_cell_size.height;
        let pane_right_edge = (pane_geom.x + pane_geom.cols.as_usize().saturating_sub(1)) * character_cell_size.width;
        let s_chunk_top_edge = s_chunk.cell_y * character_cell_size.height;
        let s_chunk_bottom_edge = s_chunk_top_edge + rounded_sixel_image_pixel_height;
        let s_chunk_left_edge = s_chunk.cell_x * character_cell_size.width;
        let s_chunk_right_edge = s_chunk_left_edge + rounded_sixel_image_pixel_width;

        let mut uncovered_chunks = vec![];
        let pane_covers_chunk_completely = pane_top_edge <= s_chunk_top_edge &&
            pane_bottom_edge >= s_chunk_bottom_edge &&
            pane_left_edge <= s_chunk_left_edge &&
            pane_right_edge >= s_chunk_right_edge;
        let pane_intersects_with_chunk_vertically =
            (pane_left_edge >= s_chunk_left_edge && pane_left_edge <= s_chunk_right_edge) ||
            (pane_right_edge >= s_chunk_left_edge && pane_right_edge <= s_chunk_right_edge) ||
            (pane_left_edge <= s_chunk_left_edge && pane_right_edge >= s_chunk_right_edge);
        let pane_intersects_with_chunk_horizontally =
            (pane_top_edge >= s_chunk_top_edge && pane_top_edge <= s_chunk_bottom_edge) ||
            (pane_bottom_edge >= s_chunk_top_edge && pane_bottom_edge <= s_chunk_bottom_edge) ||
            (pane_top_edge <= s_chunk_top_edge && pane_bottom_edge >= s_chunk_bottom_edge);
        if pane_covers_chunk_completely {
            return uncovered_chunks;
        }
        if pane_top_edge >= s_chunk_top_edge && pane_top_edge <= s_chunk_bottom_edge && pane_intersects_with_chunk_vertically {
            // pane covers image bottom
            let top_image_chunk = SixelImageChunk {
                cell_x: s_chunk.cell_x,
                cell_y: s_chunk.cell_y,
                sixel_image_pixel_x: s_chunk.sixel_image_pixel_x,
                sixel_image_pixel_y: s_chunk.sixel_image_pixel_y,
                sixel_image_pixel_width: rounded_sixel_image_pixel_width,
                sixel_image_pixel_height: pane_top_edge - s_chunk_top_edge,
                sixel_image_id: s_chunk.sixel_image_id,
            };
            uncovered_chunks.push(top_image_chunk);
        }
        if pane_bottom_edge <= s_chunk_bottom_edge && pane_bottom_edge >= s_chunk_top_edge && pane_intersects_with_chunk_vertically {
            // pane covers image top
            let bottom_image_chunk = SixelImageChunk {
                cell_x: s_chunk.cell_x,
                cell_y: (pane_bottom_edge / character_cell_size.height) + 1,
                sixel_image_pixel_x: s_chunk.sixel_image_pixel_x,
                sixel_image_pixel_y: s_chunk.sixel_image_pixel_y + (pane_bottom_edge - s_chunk_top_edge) + character_cell_size.height,
                sixel_image_pixel_width: rounded_sixel_image_pixel_width,
                sixel_image_pixel_height: (rounded_sixel_image_pixel_height - (pane_bottom_edge - s_chunk_top_edge)).saturating_sub(character_cell_size.height),
                sixel_image_id: s_chunk.sixel_image_id,
            };
            uncovered_chunks.push(bottom_image_chunk);
        }
        if pane_left_edge >= s_chunk_left_edge && pane_left_edge <= s_chunk_right_edge && pane_intersects_with_chunk_horizontally {
            // pane covers image right
            let sixel_image_pixel_y = if s_chunk_top_edge < pane_top_edge {
                s_chunk.sixel_image_pixel_y + (pane_top_edge - s_chunk_top_edge)
            } else {
                s_chunk.sixel_image_pixel_y
            };
            let max_image_height = if s_chunk_top_edge < pane_top_edge {
                rounded_sixel_image_pixel_height.saturating_sub(pane_top_edge - s_chunk_top_edge)
            } else {
                rounded_sixel_image_pixel_height
            };
            let left_image_chunk = SixelImageChunk {
                cell_x: s_chunk.cell_x,
                // if the pane_top_edge is lower than the image, we want to start there, because we
                // already cut that part above when checking if the pane covered the chunk bottom
                cell_y: std::cmp::max(s_chunk.cell_y, pane_top_edge / character_cell_size.height),
                sixel_image_pixel_x: s_chunk.sixel_image_pixel_x,
                sixel_image_pixel_y,
                sixel_image_pixel_width: rounded_sixel_image_pixel_width.saturating_sub(s_chunk_right_edge.saturating_sub(pane_left_edge)),
                sixel_image_pixel_height: std::cmp::min(pane_bottom_edge - pane_top_edge + character_cell_size.height, max_image_height),
                sixel_image_id: s_chunk.sixel_image_id,
            };
            uncovered_chunks.push(left_image_chunk);
        }
        if pane_right_edge <= s_chunk_right_edge && pane_right_edge >= s_chunk_left_edge && pane_intersects_with_chunk_horizontally {
            // pane covers image left
            let sixel_image_pixel_y = if s_chunk_top_edge < pane_top_edge {
                s_chunk.sixel_image_pixel_y + (pane_top_edge - s_chunk_top_edge)
            } else {
                s_chunk.sixel_image_pixel_y
            };
            let max_image_height = if s_chunk_top_edge < pane_top_edge {
                rounded_sixel_image_pixel_height.saturating_sub(pane_top_edge - s_chunk_top_edge)
            } else {
                rounded_sixel_image_pixel_height
            };
            let sixel_image_pixel_x = s_chunk.sixel_image_pixel_x + (pane_right_edge - s_chunk_left_edge) + character_cell_size.width;
            let right_image_chunk = SixelImageChunk {
                cell_x: (pane_right_edge / character_cell_size.width) + 1,
                // if the pane_top_edge is lower than the image, we want to start there, because we
                // already cut that part above when checking if the pane covered the chunk bottom
                cell_y: std::cmp::max(s_chunk.cell_y, pane_top_edge / character_cell_size.height),
                sixel_image_pixel_x,
                sixel_image_pixel_y,
                sixel_image_pixel_width: (rounded_sixel_image_pixel_width.saturating_sub(pane_right_edge - s_chunk_left_edge)).saturating_sub(character_cell_size.width),
                sixel_image_pixel_height: std::cmp::min(pane_bottom_edge - pane_top_edge + character_cell_size.height, max_image_height),
                sixel_image_id: s_chunk.sixel_image_id,
            };
            uncovered_chunks.push(right_image_chunk);
        }
        if uncovered_chunks.is_empty() {
            // the pane doesn't cover the chunk at all, so we return it as is
            uncovered_chunks.push(*s_chunk);
        }
        uncovered_chunks
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

#[derive(Debug, Clone, Copy, Default)]
pub struct SixelImageChunk {
    pub cell_x: usize,
    pub cell_y: usize,
    pub sixel_image_pixel_x: usize,
    pub sixel_image_pixel_y: usize,
    pub sixel_image_pixel_width: usize,
    pub sixel_image_pixel_height: usize,
    pub sixel_image_id: usize,
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
    pub fn add_selection_and_background(
        &mut self,
        selection: Selection,
        background_color: AnsiCode,
        offset_x: usize,
        offset_y: usize,
    ) {
        self.selection_and_background_color =
            Some((selection.offset(offset_x, offset_y), background_color));
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
    pub fn drain_by_width(&mut self, x: usize) -> impl Iterator<Item = TerminalCharacter> {
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
        let part_to_retain = self.drain_by_width(x);
        self.terminal_characters = part_to_retain.collect();
    }
    pub fn cut_middle_out(
        &mut self,
        middle_start: usize,
        middle_end: usize,
    ) -> (Vec<TerminalCharacter>, Vec<TerminalCharacter>) {
        let (
            absolute_middle_start_index,
            absolute_middle_end_index,
            pad_left_end_by,
            pad_right_start_by,
        ) = adjust_middle_segment_for_wide_chars(
            middle_start,
            middle_end,
            &self.terminal_characters,
        );
        let mut terminal_characters: Vec<TerminalCharacter> =
            self.terminal_characters.drain(..).collect();
        let mut characters_on_the_right: Vec<TerminalCharacter> = terminal_characters
            .drain(absolute_middle_end_index..)
            .collect();
        let mut characters_on_the_left: Vec<TerminalCharacter> = terminal_characters
            .drain(..absolute_middle_start_index)
            .collect();
        if pad_left_end_by > 0 {
            characters_on_the_left.resize(pad_left_end_by, EMPTY_TERMINAL_CHARACTER);
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
    pub changed_lines: Vec<usize>, // line index
    pub should_update_all_lines: bool,
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

                let x = x_offset; // right now we only buffer full lines as this doesn't seem to have a huge impact on performance, but the infra is here if we want to change this
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
                let x = x_offset;
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
    pub fn changed_rects_in_viewport(&self, viewport_height: usize) -> HashMap<usize, usize> {
        // group the changed lines into "changed_rects", which indicate where the line starts (the
        // hashmap key) and how many lines are in there (its value)
        let mut changed_rects: HashMap<usize, usize> = HashMap::new(); // <start_line_index, line_count>
        let mut last_changed_line_index: Option<usize> = None;
        let mut changed_line_count = 0;
        let mut add_changed_line = |line_index| {
            match last_changed_line_index.as_mut() {
                Some(changed_line_index) => {
                    if *changed_line_index + changed_line_count == line_index {
                            changed_line_count += 1
                    } else {
                        changed_rects.insert(*changed_line_index, changed_line_count);
                        last_changed_line_index = Some(line_index);
                        changed_line_count = 1;
                    }
                },
                None => {
                    last_changed_line_index = Some(line_index);
                    changed_line_count = 1;
                }
            }
        };

        // TODO: move this whole thing to output_buffer
        if self.should_update_all_lines {
            // for line_index in 0..self.viewport.len() {
            for line_index in 0..viewport_height {
                add_changed_line(line_index);
            }
        } else {
            for line_index in self.changed_lines.iter().copied() {
                add_changed_line(line_index);
            }
        }
        if let Some(changed_line_index) = last_changed_line_index {
            changed_rects.insert(changed_line_index, changed_line_count);
        }
        changed_rects
    }
}

use super::parser::{DecodedImage, KittyCommand, KittyError, KittyErrorCode};
use super::store::{InternalImageId, KittyImageStore};
use crate::panes::sixel::PixelRect;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zellij_utils::pane_size::SizeInPixels;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KittyImageChunk {
    pub cell_x: usize,
    pub cell_y: usize,
    pub internal_image_id: InternalImageId,
    pub source_px_x: usize,
    pub source_px_y: usize,
    pub source_px_width: usize,
    pub source_px_height: usize,
    pub cell_offset_x: u32,
    pub cell_offset_y: u32,
    pub z_index: i32,
    pub dest_cells: (u16, u16),
    pub scaled_px: Option<(usize, usize)>,
    pub placement_uid: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KittyVerticalAnchor {
    pub canonical_line: usize,
    pub offset_px_from_line_start: isize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KittyPlacement {
    pub image_id: u32,
    pub placement_id: u32,
    pub placement_uid: u64,
    pub internal_id: InternalImageId,
    pub display_rect: PixelRect,
    pub source_rect: PixelRect,
    pub emit_x: usize,
    pub emit_y: usize,
    pub scaled_px: Option<(usize, usize)>,
    pub dest_cells: (u16, u16),
    pub cell_offset: (u32, u32),
    pub z_index: i32,
    pub vertical_anchor: KittyVerticalAnchor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KittyReplyData {
    pub image_id: Option<u32>,
    pub image_number: Option<u32>,
    pub placement_id: Option<u32>,
    pub quiet: u8,
}

impl KittyReplyData {
    pub fn from_command(command: &KittyCommand) -> Self {
        KittyReplyData {
            image_id: command.image_id,
            image_number: command.image_number,
            placement_id: command.placement_id,
            quiet: command.quiet,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KittyGrid {
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
    pub kitty_image_store: Rc<RefCell<KittyImageStore>>,
    image_ids: HashMap<u32, InternalImageId>,
    image_numbers: HashMap<u32, Vec<u32>>,
    placements: Vec<KittyPlacement>,
    next_synthetic_image_id: u32,
    front_drops: u64,
    commands_handled: u64,
    previous_cell_size: Option<SizeInPixels>,
}

fn einval(command: &KittyCommand, message: &str) -> KittyError {
    KittyError {
        code: KittyErrorCode::Einval,
        message: message.to_owned(),
        image_id: command.image_id,
        image_number: command.image_number,
        placement_id: command.placement_id,
        quiet: command.quiet,
    }
}

fn enoent(command: &KittyCommand, message: &str) -> KittyError {
    KittyError {
        code: KittyErrorCode::Enoent,
        message: message.to_owned(),
        image_id: command.image_id,
        image_number: command.image_number,
        placement_id: command.placement_id,
        quiet: command.quiet,
    }
}

impl KittyGrid {
    pub fn new(
        character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
        kitty_image_store: Rc<RefCell<KittyImageStore>>,
    ) -> Self {
        let previous_cell_size = *character_cell_size.borrow();
        KittyGrid {
            character_cell_size,
            kitty_image_store,
            image_ids: HashMap::new(),
            image_numbers: HashMap::new(),
            placements: Vec::new(),
            next_synthetic_image_id: u32::MAX,
            front_drops: 0,
            commands_handled: 0,
            previous_cell_size,
        }
    }
    pub fn note_command_handled(&mut self) {
        self.commands_handled += 1;
    }
    pub fn commands_handled(&self) -> u64 {
        self.commands_handled
    }
    pub fn front_drops(&self) -> u64 {
        self.front_drops
    }
    pub fn placements(&self) -> &[KittyPlacement] {
        &self.placements
    }
    pub fn placements_mut(&mut self) -> &mut [KittyPlacement] {
        &mut self.placements
    }
    pub fn retain_placements<F: FnMut(&KittyPlacement) -> bool>(&mut self, mut keep: F) {
        let mut store = self.kitty_image_store.borrow_mut();
        let mut kept = Vec::with_capacity(self.placements.len());
        for placement in self.placements.drain(..) {
            if keep(&placement) {
                kept.push(placement);
            } else {
                store.free(placement.internal_id);
            }
        }
        self.placements = kept;
    }
    pub fn placement_count(&self) -> usize {
        self.placements.len()
    }
    pub fn pane_image_id_map(&self) -> &HashMap<u32, InternalImageId> {
        &self.image_ids
    }
    fn alloc_synthetic_image_id(&mut self) -> u32 {
        loop {
            let id = self.next_synthetic_image_id;
            self.next_synthetic_image_id = self.next_synthetic_image_id.wrapping_sub(1);
            if id != 0 && !self.image_ids.contains_key(&id) {
                return id;
            }
        }
    }
    fn drop_image_number_references(&mut self, pane_image_id: u32) {
        for ids in self.image_numbers.values_mut() {
            ids.retain(|id| *id != pane_image_id);
        }
        self.image_numbers.retain(|_, ids| !ids.is_empty());
    }
    pub fn transmit(
        &mut self,
        command: &KittyCommand,
        image: DecodedImage,
    ) -> Result<u32, KittyError> {
        let pane_image_id = if command.image_number.is_some() {
            self.alloc_synthetic_image_id()
        } else {
            command.image_id.unwrap_or(0)
        };
        if pane_image_id != 0 {
            if let Some(old_internal) = self.image_ids.get(&pane_image_id).copied() {
                let mut store = self.kitty_image_store.borrow_mut();
                let mut kept = Vec::with_capacity(self.placements.len());
                for placement in self.placements.drain(..) {
                    if placement.image_id == pane_image_id {
                        store.free(placement.internal_id);
                    } else {
                        kept.push(placement);
                    }
                }
                self.placements = kept;
                if store.get(old_internal).is_some() {
                    store.free(old_internal);
                }
                drop(store);
                self.image_ids.remove(&pane_image_id);
                self.drop_image_number_references(pane_image_id);
            }
        }
        let internal = self
            .kitty_image_store
            .borrow_mut()
            .store_image(image)
            .map_err(|mut error| {
                error.image_id = command.image_id;
                error.image_number = command.image_number;
                error.placement_id = command.placement_id;
                error.quiet = command.quiet;
                error
            })?;
        if let Some(image_number) = command.image_number {
            self.image_numbers
                .entry(image_number)
                .or_insert_with(Vec::new)
                .push(pane_image_id);
        }
        self.image_ids.insert(pane_image_id, internal);
        self.kitty_image_store.borrow_mut().touch(internal);
        Ok(pane_image_id)
    }
    pub fn resolve_display_target(
        &self,
        command: &KittyCommand,
    ) -> Result<(u32, InternalImageId), KittyError> {
        let pane_image_id = if let Some(image_id) = command.image_id {
            image_id
        } else if let Some(image_number) = command.image_number {
            *self
                .image_numbers
                .get(&image_number)
                .and_then(|ids| ids.last())
                .ok_or_else(|| enoent(command, "unknown image number"))?
        } else {
            return Err(enoent(command, "no image id specified"));
        };
        let internal = *self
            .image_ids
            .get(&pane_image_id)
            .ok_or_else(|| enoent(command, "unknown image id"))?;
        if self.kitty_image_store.borrow().get(internal).is_none() {
            return Err(enoent(command, "unknown image id"));
        }
        Ok((pane_image_id, internal))
    }
    pub fn place(
        &mut self,
        pane_image_id: u32,
        internal: InternalImageId,
        command: &KittyCommand,
        cursor_px: (usize, usize),
        cell: SizeInPixels,
        vertical_anchor: KittyVerticalAnchor,
    ) -> Result<(u16, u16), KittyError> {
        let (image_width, image_height) = {
            let store = self.kitty_image_store.borrow();
            let image = store
                .get(internal)
                .ok_or_else(|| enoent(command, "unknown image id"))?;
            (image.width as usize, image.height as usize)
        };
        let source_x = command.source_x as usize;
        let source_y = command.source_y as usize;
        if source_x >= image_width || source_y >= image_height {
            return Err(einval(command, "source rectangle outside image bounds"));
        }
        let source_w = if command.source_w == 0 {
            image_width - source_x
        } else {
            std::cmp::min(command.source_w as usize, image_width - source_x)
        };
        let source_h = if command.source_h == 0 {
            image_height - source_y
        } else {
            std::cmp::min(command.source_h as usize, image_height - source_y)
        };
        if source_w == 0 || source_h == 0 {
            return Err(einval(command, "empty source rectangle"));
        }
        let off_x = std::cmp::min(command.cell_offset_x as usize, cell.width - 1);
        let off_y = std::cmp::min(command.cell_offset_y as usize, cell.height - 1);
        let ceil_div = |a: usize, b: usize| (a + b - 1) / b;
        let (dst_w, dst_h, cols, rows, is_scaled) =
            match (command.columns as usize, command.rows as usize) {
                (0, 0) => (
                    source_w,
                    source_h,
                    ceil_div(source_w + off_x, cell.width),
                    ceil_div(source_h + off_y, cell.height),
                    false,
                ),
                (c, 0) => {
                    let dst_w = c * cell.width;
                    let dst_h = std::cmp::max(
                        ((dst_w as f64) * (source_h as f64) / (source_w as f64)).round() as usize,
                        1,
                    );
                    let height_px = ((dst_w + off_x) as f64) * (source_h as f64) / (source_w as f64);
                    let rows =
                        std::cmp::max((height_px / cell.height as f64).ceil() as usize, 1);
                    (dst_w, dst_h, c, rows, true)
                },
                (0, r) => {
                    let dst_h = r * cell.height;
                    let dst_w = std::cmp::max(
                        ((dst_h as f64) * (source_w as f64) / (source_h as f64)).round() as usize,
                        1,
                    );
                    let width_px = ((dst_h + off_y) as f64) * (source_w as f64) / (source_h as f64);
                    let cols = std::cmp::max((width_px / cell.width as f64).ceil() as usize, 1);
                    (dst_w, dst_h, cols, r, true)
                },
                (c, r) => (c * cell.width, r * cell.height, c, r, true),
            };
        let dest_cells = (cols as u16, rows as u16);
        if is_scaled {
            let scaled_bytes = {
                let store = self.kitty_image_store.borrow();
                let image = store
                    .get(internal)
                    .ok_or_else(|| enoent(command, "unknown image id"))?;
                let cropped = crop_rgba(
                    &image.rgba,
                    image_width,
                    image_height,
                    source_x,
                    source_y,
                    source_w,
                    source_h,
                );
                scale_rgba(&cropped, source_w, source_h, dst_w, dst_h)
            };
            self.kitty_image_store.borrow_mut().add_scaled_variant(
                internal,
                dest_cells,
                scaled_bytes,
            );
        }
        let display_rect = PixelRect {
            x: cursor_px.0 + off_x,
            y: (cursor_px.1 + off_y) as isize,
            width: dst_w,
            height: dst_h,
        };
        let placement_id = if pane_image_id != 0 {
            command.placement_id.unwrap_or(0)
        } else {
            0
        };
        let mut reused_uid: Option<u64> = None;
        if pane_image_id != 0 && placement_id != 0 {
            let mut store = self.kitty_image_store.borrow_mut();
            self.placements.retain(|placement| {
                if placement.image_id == pane_image_id && placement.placement_id == placement_id {
                    store.remove_placement_ref(placement.internal_id);
                    reused_uid = Some(placement.placement_uid);
                    false
                } else {
                    true
                }
            });
        }
        let placement_uid = match reused_uid {
            Some(uid) => uid,
            None => self.kitty_image_store.borrow_mut().next_placement_uid(),
        };
        self.placements.push(KittyPlacement {
            image_id: pane_image_id,
            placement_id,
            placement_uid,
            internal_id: internal,
            display_rect,
            source_rect: PixelRect {
                x: source_x,
                y: source_y as isize,
                width: source_w,
                height: source_h,
            },
            emit_x: 0,
            emit_y: 0,
            scaled_px: if is_scaled {
                Some((dst_w, dst_h))
            } else {
                None
            },
            dest_cells,
            cell_offset: (off_x as u32, off_y as u32),
            z_index: command.z_index,
            vertical_anchor,
        });
        self.kitty_image_store
            .borrow_mut()
            .add_placement_ref(internal);
        Ok(dest_cells)
    }
    pub fn delete(
        &mut self,
        command: &KittyCommand,
        cursor_cell: (usize, usize),
        viewport_cells: (usize, usize),
        scrollback_rows: usize,
    ) -> Result<(), KittyError> {
        let specifier = command.delete_specifier.unwrap_or('a');
        let uppercase = specifier.is_ascii_uppercase();
        let lower = specifier.to_ascii_lowercase();
        let cell = *self.character_cell_size.borrow();
        let geometry_needed = matches!(lower, 'a' | 'c' | 'p' | 'q' | 'x' | 'y');
        let cell = match (cell, geometry_needed) {
            (Some(cell), _) => cell,
            (None, true) => return Ok(()),
            (None, false) => SizeInPixels {
                width: 1,
                height: 1,
            },
        };
        let cell_w = cell.width;
        let cell_h = cell.height;
        let la_px = scrollback_rows * cell_h;
        let probe_rect = |x: usize, y: usize, width: usize, height: usize| PixelRect {
            x,
            y: y as isize,
            width,
            height,
        };
        let target_image_id: Option<u32> = match lower {
            'i' => Some(
                command
                    .image_id
                    .ok_or_else(|| einval(command, "missing image id for delete"))?,
            ),
            'n' => {
                let image_number = command
                    .image_number
                    .ok_or_else(|| einval(command, "missing image number for delete"))?;
                match self
                    .image_numbers
                    .get(&image_number)
                    .and_then(|ids| ids.last())
                {
                    Some(id) => Some(*id),
                    None => return Ok(()),
                }
            },
            _ => None,
        };
        let column_probe_x = |source_x: u32| -> Option<usize> {
            if source_x == 0 {
                None
            } else {
                Some((source_x as usize - 1) * cell_w)
            }
        };
        let row_probe_y = |source_y: u32| -> Option<usize> {
            if source_y == 0 {
                None
            } else {
                Some(la_px + (source_y as usize - 1) * cell_h)
            }
        };
        let matches_placement = |placement: &KittyPlacement| -> bool {
            match lower {
                'a' => placement
                    .display_rect
                    .intersecting_rect(&probe_rect(
                        0,
                        la_px,
                        viewport_cells.0 * cell_w,
                        viewport_cells.1 * cell_h,
                    ))
                    .is_some(),
                'i' | 'n' => {
                    placement.image_id == target_image_id.unwrap()
                        && command
                            .placement_id
                            .map(|placement_id| placement.placement_id == placement_id)
                            .unwrap_or(true)
                },
                'c' => placement
                    .display_rect
                    .intersecting_rect(&probe_rect(
                        cursor_cell.0 * cell_w,
                        la_px + cursor_cell.1 * cell_h,
                        cell_w,
                        cell_h,
                    ))
                    .is_some(),
                'p' => match (column_probe_x(command.source_x), row_probe_y(command.source_y)) {
                    (Some(px), Some(py)) => placement
                        .display_rect
                        .intersecting_rect(&probe_rect(px, py, cell_w, cell_h))
                        .is_some(),
                    _ => false,
                },
                'q' => {
                    placement.z_index == command.z_index
                        && match (column_probe_x(command.source_x), row_probe_y(command.source_y))
                        {
                            (Some(px), Some(py)) => placement
                                .display_rect
                                .intersecting_rect(&probe_rect(px, py, cell_w, cell_h))
                                .is_some(),
                            _ => false,
                        }
                },
                'x' => match column_probe_x(command.source_x) {
                    Some(px) => placement
                        .display_rect
                        .intersecting_rect(&probe_rect(px, 0, cell_w, usize::MAX / 2))
                        .is_some(),
                    None => false,
                },
                'y' => match row_probe_y(command.source_y) {
                    Some(py) => placement
                        .display_rect
                        .intersecting_rect(&probe_rect(0, py, usize::MAX / 2, cell_h))
                        .is_some(),
                    None => false,
                },
                'z' => placement.z_index == command.z_index,
                'r' => {
                    placement.image_id >= command.source_x && placement.image_id <= command.source_y
                },
                _ => false,
            }
        };
        {
            let mut store = self.kitty_image_store.borrow_mut();
            let mut kept = Vec::with_capacity(self.placements.len());
            for placement in self.placements.drain(..) {
                if matches_placement(&placement) {
                    if uppercase {
                        store.free(placement.internal_id);
                    } else {
                        store.remove_placement_ref(placement.internal_id);
                    }
                } else {
                    kept.push(placement);
                }
            }
            self.placements = kept;
        }
        if uppercase {
            match lower {
                'i' | 'n' => {
                    let target = target_image_id.unwrap();
                    let no_surviving = !self
                        .placements
                        .iter()
                        .any(|placement| placement.image_id == target);
                    if no_surviving {
                        if let Some(internal) = self.image_ids.get(&target).copied() {
                            self.kitty_image_store.borrow_mut().free(internal);
                        }
                    }
                },
                'r' => {
                    let in_range: Vec<(u32, InternalImageId)> = self
                        .image_ids
                        .iter()
                        .filter(|(id, _)| **id >= command.source_x && **id <= command.source_y)
                        .map(|(id, internal)| (*id, *internal))
                        .collect();
                    let mut store = self.kitty_image_store.borrow_mut();
                    for (_, internal) in in_range {
                        if store.refcount(internal) == Some(0) {
                            store.free(internal);
                        }
                    }
                },
                _ => {},
            }
            let store = self.kitty_image_store.borrow();
            self.image_ids
                .retain(|_, internal| store.get(*internal).is_some());
            drop(store);
            let live_ids: Vec<u32> = self.image_ids.keys().copied().collect();
            self.image_numbers.values_mut().for_each(|ids| {
                ids.retain(|id| live_ids.contains(id));
            });
            self.image_numbers.retain(|_, ids| !ids.is_empty());
        }
        Ok(())
    }
    pub fn offset_grid_top(&mut self) {
        self.front_drops += 1;
        if let Some(cell) = *self.character_cell_size.borrow() {
            let height_to_reduce = cell.height as isize;
            let mut store = self.kitty_image_store.borrow_mut();
            let mut kept = Vec::with_capacity(self.placements.len());
            for mut placement in self.placements.drain(..) {
                placement.display_rect.y -= height_to_reduce;
                if placement.display_rect.y + placement.display_rect.height as isize <= 0 {
                    store.free(placement.internal_id);
                } else {
                    kept.push(placement);
                }
            }
            self.placements = kept;
        }
    }
    pub fn apply_region_scroll(
        &mut self,
        region_top_px: isize,
        region_bottom_px: isize,
        n_px: isize,
        la_delta_px: isize,
        viewport_top_px: isize,
    ) {
        let delta_inside = la_delta_px - n_px;
        let delta_outside = la_delta_px;
        let shifted_region_top = region_top_px + la_delta_px;
        let shifted_region_bottom = region_bottom_px + la_delta_px;
        let mut store = self.kitty_image_store.borrow_mut();
        let mut kept = Vec::with_capacity(self.placements.len());
        for mut placement in self.placements.drain(..) {
            let top = placement.display_rect.y;
            let bottom = top + placement.display_rect.height as isize;
            if bottom <= viewport_top_px {
                kept.push(placement);
                continue;
            }
            let entirely_inside = top >= region_top_px && bottom <= region_bottom_px;
            let intersects_region = top < region_bottom_px && bottom > region_top_px;
            if entirely_inside {
                placement.display_rect.y += delta_inside;
                let new_top = placement.display_rect.y;
                let new_bottom = new_top + placement.display_rect.height as isize;
                if new_bottom <= shifted_region_top || new_top >= shifted_region_bottom {
                    store.free(placement.internal_id);
                    continue;
                }
                if new_top < shifted_region_top {
                    let cut = (shifted_region_top - new_top) as usize;
                    placement.emit_y += cut;
                    placement.display_rect.y = shifted_region_top;
                    placement.display_rect.height -= cut;
                }
                let new_bottom = placement.display_rect.y + placement.display_rect.height as isize;
                if new_bottom > shifted_region_bottom {
                    placement.display_rect.height -= (new_bottom - shifted_region_bottom) as usize;
                }
                if placement.display_rect.height == 0 {
                    store.free(placement.internal_id);
                    continue;
                }
                kept.push(placement);
            } else if intersects_region {
                placement.display_rect.y += delta_outside;
                let new_top = placement.display_rect.y;
                let new_bottom = new_top + placement.display_rect.height as isize;
                if new_top < shifted_region_top {
                    placement.display_rect.height = (shifted_region_top - new_top) as usize;
                } else if new_bottom > shifted_region_bottom {
                    let cut = (shifted_region_bottom - new_top) as usize;
                    placement.emit_y += cut;
                    placement.display_rect.y = shifted_region_bottom;
                    placement.display_rect.height -= cut;
                } else {
                    store.free(placement.internal_id);
                    continue;
                }
                if placement.display_rect.height == 0 {
                    store.free(placement.internal_id);
                    continue;
                }
                kept.push(placement);
            } else {
                placement.display_rect.y += delta_outside;
                kept.push(placement);
            }
        }
        self.placements = kept;
    }
    pub fn character_cell_size_possibly_changed(&mut self) {
        if let (Some(previous_cell_size), Some(character_cell_size)) =
            (self.previous_cell_size, *self.character_cell_size.borrow())
        {
            if previous_cell_size != character_cell_size {
                let mut regenerations: Vec<(InternalImageId, (u16, u16), PixelRect)> = Vec::new();
                for placement in self.placements.iter_mut() {
                    placement.display_rect.x = (placement.display_rect.x
                        / previous_cell_size.width)
                        * character_cell_size.width;
                    placement.display_rect.y = (placement.display_rect.y
                        / previous_cell_size.height as isize)
                        * character_cell_size.height as isize;
                    if placement.scaled_px.is_some() {
                        let (cols, rows) = placement.dest_cells;
                        placement.display_rect.width = cols as usize * character_cell_size.width;
                        placement.display_rect.height = rows as usize * character_cell_size.height;
                        placement.scaled_px =
                            Some((placement.display_rect.width, placement.display_rect.height));
                        placement.emit_x = 0;
                        placement.emit_y = 0;
                        regenerations.push((
                            placement.internal_id,
                            placement.dest_cells,
                            placement.source_rect,
                        ));
                    }
                }
                for (internal, dest_cells, source_rect) in regenerations {
                    let scaled_bytes = {
                        let store = self.kitty_image_store.borrow();
                        match store.get(internal) {
                            Some(image) => {
                                let cropped = crop_rgba(
                                    &image.rgba,
                                    image.width as usize,
                                    image.height as usize,
                                    source_rect.x,
                                    source_rect.y as usize,
                                    source_rect.width,
                                    source_rect.height,
                                );
                                Some(scale_rgba(
                                    &cropped,
                                    source_rect.width,
                                    source_rect.height,
                                    dest_cells.0 as usize * character_cell_size.width,
                                    dest_cells.1 as usize * character_cell_size.height,
                                ))
                            },
                            None => None,
                        }
                    };
                    if let Some(scaled_bytes) = scaled_bytes {
                        self.kitty_image_store.borrow_mut().add_scaled_variant(
                            internal,
                            dest_cells,
                            scaled_bytes,
                        );
                    }
                }
            }
        }
        self.previous_cell_size = *self.character_cell_size.borrow();
    }
    pub fn clear_all_placements(&mut self) {
        let mut store = self.kitty_image_store.borrow_mut();
        for placement in self.placements.drain(..) {
            store.free(placement.internal_id);
        }
    }
    pub fn clear_visible_placements(&mut self, scrollback_size_in_lines: usize) {
        let cell_size = { *self.character_cell_size.borrow() };
        let cell_height = match cell_size {
            Some(cell) => cell.height as isize,
            None => {
                self.clear_all_placements();
                return;
            },
        };
        let scrollback_top = scrollback_size_in_lines as isize * cell_height;
        let mut store = self.kitty_image_store.borrow_mut();
        let mut kept = Vec::with_capacity(self.placements.len());
        for placement in self.placements.drain(..) {
            let screen_relative_top_row =
                (placement.display_rect.y - scrollback_top).div_euclid(cell_height);
            let dest_rows = placement.dest_cells.1 as isize;
            if screen_relative_top_row + dest_rows > 0 {
                store.free(placement.internal_id);
            } else {
                kept.push(placement);
            }
        }
        self.placements = kept;
    }
    pub fn image_cell_coordinates_in_viewport(
        &self,
        viewport_height: usize,
        scrollback_height: usize,
    ) -> Vec<(usize, usize, usize, usize)> {
        match *self.character_cell_size.borrow() {
            Some(character_cell_size) => self
                .placements
                .iter()
                .map(|placement| {
                    let pixel_rect = &placement.display_rect;
                    let scrollback_size_in_pixels = scrollback_height * character_cell_size.height;
                    let y_pixel_coordinates_in_viewport =
                        pixel_rect.y - scrollback_size_in_pixels as isize;
                    let image_y = std::cmp::max(y_pixel_coordinates_in_viewport, 0) as usize
                        / character_cell_size.height;
                    let image_x = pixel_rect.x / character_cell_size.width;
                    let image_height_in_pixels = if y_pixel_coordinates_in_viewport < 0 {
                        pixel_rect.height as isize + y_pixel_coordinates_in_viewport
                    } else {
                        pixel_rect.height as isize
                    };
                    let image_height = image_height_in_pixels as usize / character_cell_size.height;
                    let image_width = pixel_rect.width / character_cell_size.width;
                    let height_remainder =
                        if image_height_in_pixels as usize % character_cell_size.height > 0 {
                            1
                        } else {
                            0
                        };
                    let width_remainder = if pixel_rect.width % character_cell_size.width > 0 {
                        1
                    } else {
                        0
                    };
                    let image_top_edge = image_y;
                    let image_bottom_edge =
                        std::cmp::min(image_y + image_height + height_remainder, viewport_height);
                    let image_left_edge = image_x;
                    let image_right_edge = image_x + image_width + width_remainder;
                    (
                        image_top_edge,
                        image_bottom_edge,
                        image_left_edge,
                        image_right_edge,
                    )
                })
                .collect(),
            None => vec![],
        }
    }
    pub fn viewport_kitty_chunks(
        &self,
        viewport_height: usize,
        scrollback_size_in_lines: usize,
        viewport_width_in_cells: usize,
        viewport_x_offset: usize,
        viewport_y_offset: usize,
    ) -> Vec<KittyImageChunk> {
        let mut full = HashMap::new();
        full.insert(0, viewport_height);
        self.changed_kitty_chunks_in_viewport(
            full,
            scrollback_size_in_lines,
            viewport_width_in_cells,
            viewport_x_offset,
            viewport_y_offset,
        )
    }
    pub fn changed_kitty_chunks_in_viewport(
        &self,
        changed_rects: HashMap<usize, usize>,
        scrollback_size_in_lines: usize,
        viewport_width_in_cells: usize,
        viewport_x_offset: usize,
        viewport_y_offset: usize,
    ) -> Vec<KittyImageChunk> {
        let mut changed_kitty_image_chunks = vec![];
        if let Some(character_cell_size) = { *self.character_cell_size.borrow() } {
            for placement in &self.placements {
                let image_pixel_rect = &placement.display_rect;
                for (line_index, line_count) in &changed_rects {
                    let changed_rect_pixel_height = line_count * character_cell_size.height;
                    let changed_rect_top_edge = ((line_index + scrollback_size_in_lines)
                        * character_cell_size.height)
                        as isize;
                    let changed_rect_bottom_edge =
                        changed_rect_top_edge + changed_rect_pixel_height as isize;
                    let image_top_edge = image_pixel_rect.y;
                    let image_bottom_edge = image_pixel_rect.y + image_pixel_rect.height as isize;

                    let cell_x_in_current_pane = image_pixel_rect.x / character_cell_size.width;
                    let cell_x = viewport_x_offset + cell_x_in_current_pane;
                    let image_pixel_width = if image_pixel_rect.x + image_pixel_rect.width
                        <= (viewport_width_in_cells * character_cell_size.width)
                    {
                        image_pixel_rect.width
                    } else {
                        (viewport_width_in_cells * character_cell_size.width)
                            .saturating_sub(image_pixel_rect.x)
                    };
                    if image_pixel_width == 0 {
                        continue;
                    }

                    let image_cell_distance_from_scrollback_top = std::cmp::max(
                        image_top_edge.div_euclid(character_cell_size.height as isize),
                        0,
                    ) as usize;
                    let image_cell_distance_from_changed_rect_top =
                        image_cell_distance_from_scrollback_top
                            .saturating_sub(line_index + scrollback_size_in_lines);
                    let cell_y =
                        viewport_y_offset + line_index + image_cell_distance_from_changed_rect_top;
                    let source_px_x = placement.emit_x;
                    let source_px_y = placement.emit_y
                        + std::cmp::max(changed_rect_top_edge - image_top_edge, 0) as usize;
                    let source_px_height = std::cmp::min(
                        (std::cmp::min(changed_rect_bottom_edge, image_bottom_edge)
                            - std::cmp::max(changed_rect_top_edge, image_top_edge))
                            as usize,
                        image_pixel_rect.height,
                    );

                    if (image_top_edge >= changed_rect_top_edge
                        && image_top_edge <= changed_rect_bottom_edge)
                        || (image_bottom_edge >= changed_rect_top_edge
                            && image_bottom_edge <= changed_rect_bottom_edge)
                        || (image_bottom_edge >= changed_rect_bottom_edge
                            && image_top_edge <= changed_rect_top_edge)
                    {
                        changed_kitty_image_chunks.push(KittyImageChunk {
                            cell_x,
                            cell_y,
                            internal_image_id: placement.internal_id,
                            source_px_x,
                            source_px_y,
                            source_px_width: image_pixel_width,
                            source_px_height,
                            cell_offset_x: placement.cell_offset.0,
                            cell_offset_y: placement.cell_offset.1,
                            z_index: placement.z_index,
                            dest_cells: placement.dest_cells,
                            scaled_px: placement.scaled_px,
                            placement_uid: placement.placement_uid,
                        });
                    }
                }
            }
        }
        changed_kitty_image_chunks
    }
}

pub fn crop_rgba(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        if row >= src_h {
            break;
        }
        let row_start = (row * src_w + x) * 4;
        let row_end = std::cmp::min(row_start + w * 4, (row * src_w + src_w) * 4);
        out.extend_from_slice(&src[row_start..row_end]);
    }
    out
}

pub fn scale_rgba(src: &[u8], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(dst_w * dst_h * 4);
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return out;
    }
    for dy in 0..dst_h {
        let fy = (dy as f64 + 0.5) * (src_h as f64) / (dst_h as f64) - 0.5;
        let y0 = fy.floor().max(0.0).min((src_h - 1) as f64) as usize;
        let y1 = std::cmp::min(y0 + 1, src_h - 1);
        let ty = (fy - y0 as f64).max(0.0).min(1.0);
        for dx in 0..dst_w {
            let fx = (dx as f64 + 0.5) * (src_w as f64) / (dst_w as f64) - 0.5;
            let x0 = fx.floor().max(0.0).min((src_w - 1) as f64) as usize;
            let x1 = std::cmp::min(x0 + 1, src_w - 1);
            let tx = (fx - x0 as f64).max(0.0).min(1.0);
            for channel in 0..4 {
                let p00 = src[(y0 * src_w + x0) * 4 + channel] as f64;
                let p10 = src[(y0 * src_w + x1) * 4 + channel] as f64;
                let p01 = src[(y1 * src_w + x0) * 4 + channel] as f64;
                let p11 = src[(y1 * src_w + x1) * 4 + channel] as f64;
                let value =
                    (1.0 - ty) * ((1.0 - tx) * p00 + tx * p10) + ty * ((1.0 - tx) * p01 + tx * p11);
                out.push(value.round().max(0.0).min(255.0) as u8);
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "./unit/grid_state_tests.rs"]
mod grid_state_tests;

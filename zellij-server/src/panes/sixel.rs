use crate::output::SixelImageChunk;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use sixel_image::{SixelDeserializer, SixelImage};
use sixel_tokenizer::SixelEvent;

use std::fmt::Debug;

use zellij_utils::pane_size::SizeInPixels;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PixelRect {
    pub x: usize,
    pub y: isize, // this can potentially be negative (eg. when the image top has scrolled past the edge of the scrollbuffer)
    pub width: usize,
    pub height: usize,
}

impl PixelRect {
    pub fn new(x: usize, y: usize, height: usize, width: usize) -> Self {
        PixelRect {
            x,
            y: y as isize,
            width,
            height,
        }
    }
    pub fn intersecting_rect(&self, other: &PixelRect) -> Option<PixelRect> {
        // if the two rects intersect, this returns a PixelRect *relative to self*
        let self_top_edge = self.y;
        let self_bottom_edge = self.y + self.height as isize;
        let self_left_edge = self.x;
        let self_right_edge = self.x + self.width;
        let other_top_edge = other.y;
        let other_bottom_edge = other.y + other.height as isize;
        let other_left_edge = other.x;
        let other_right_edge = other.x + other.width;

        let absolute_x = std::cmp::max(self_left_edge, other_left_edge);
        let absolute_y = std::cmp::max(self_top_edge, other_top_edge);
        let absolute_right_edge = std::cmp::min(self_right_edge, other_right_edge);
        let absolute_bottom_edge = std::cmp::min(self_bottom_edge, other_bottom_edge);
        let width = absolute_right_edge.saturating_sub(absolute_x);
        let height = absolute_bottom_edge.saturating_sub(absolute_y);
        let x = absolute_x - self.x;
        let y = absolute_y - self.y;
        if width > 0 && height > 0 {
            Some(PixelRect {
                x,
                y,
                width,
                height: height as usize,
            })
        } else {
            None
        }
    }
}

fn sixel_image_chunk_from_rects(
    sixel_image_id: usize,
    image_rect: &PixelRect,
    changed_rect: &PixelRect,
    changed_line_index: usize,
    character_cell_size: &SizeInPixels,
    viewport_x_offset: usize,
    viewport_y_offset: usize,
) -> Option<SixelImageChunk> {
    if character_cell_size.width == 0 || character_cell_size.height == 0 {
        return None;
    }
    let intersection = image_rect.intersecting_rect(changed_rect)?;
    let absolute_intersection_x = image_rect.x.checked_add(intersection.x)?;
    let absolute_intersection_top = image_rect.y.checked_add(intersection.y)?;
    let row_offset = usize::try_from(absolute_intersection_top - changed_rect.y).ok()?
        / character_cell_size.height;
    Some(SixelImageChunk {
        cell_x: viewport_x_offset
            .saturating_add(absolute_intersection_x / character_cell_size.width),
        cell_y: viewport_y_offset
            .saturating_add(changed_line_index)
            .saturating_add(row_offset),
        sixel_image_pixel_x: intersection.x,
        sixel_image_pixel_y: usize::try_from(intersection.y).ok()?,
        sixel_image_pixel_width: intersection.width,
        sixel_image_pixel_height: intersection.height,
        sixel_image_id,
    })
}

// Limits on the image a program can make us build while it is being parsed.
// sixel-image allocates for whatever the stream declares or draws (a raster
// attributes header, a repeat count, a run of newlines) with no limit of its
// own, so the events are checked here before they are handed to it.
const MAX_SIXEL_DIMENSION: usize = 10_000;
// sixel-image stores 4 bytes per pixel, this is 100 MiB of them
const MAX_SIXEL_PIXELS: usize = 26_214_400;

/// Tracks the size the deserializer's pixel buffer would reach for the events
/// seen so far, and refuses the event that would take it over the limits.
/// It follows the buffer the deserializer actually builds: raster padding is
/// only modeled for an opaque background, and nothing is counted once the
/// deserializer has stopped at its configured maximum height.
#[derive(Debug, Clone, Copy, Default)]
struct SixelSizeGuard {
    cursor_x: usize,
    cursor_y: usize,
    // lines in the buffer
    height: usize,
    // the widest line in the buffer
    width: usize,
    transparent_background: bool,
    max_height: Option<usize>,
    // the deserializer ignores every event once it has stopped
    stopped: bool,
}

impl SixelSizeGuard {
    fn new(max_height: Option<usize>) -> Self {
        SixelSizeGuard {
            max_height,
            ..Default::default()
        }
    }
    fn admit(&mut self, event: &SixelEvent) -> bool {
        if self.stopped {
            return true;
        }
        match event {
            SixelEvent::Dcs {
                transparent_background,
                ..
            } => {
                if *transparent_background == Some(1) {
                    self.transparent_background = true;
                }
                true
            },
            SixelEvent::RasterAttribute { ph, pv, .. } => {
                // with a transparent background sixel-image only records the
                // attributes; otherwise it pads the buffer out to `pv` lines
                // of `pv` pixels (when it has fewer lines) and then every
                // line to `ph` pixels
                if self.transparent_background {
                    return true;
                }
                let mut width = self.width;
                let mut height = self.height;
                if let Some(pv) = pv {
                    if *pv > height {
                        width = std::cmp::max(width, *pv);
                        height = *pv;
                    }
                }
                if let Some(ph) = ph {
                    width = std::cmp::max(width, *ph);
                }
                self.grow_to(width, height)
            },
            SixelEvent::Data { .. } => self.draw(1),
            SixelEvent::Repeat { repeat_count, .. } => self.draw(*repeat_count),
            SixelEvent::GotoBeginningOfLine => {
                self.cursor_x = 0;
                true
            },
            SixelEvent::GotoNextLine => {
                if let Some(max_height) = self.max_height {
                    if self.cursor_y.saturating_add(12) > max_height {
                        self.stopped = true;
                        return true;
                    }
                }
                self.cursor_x = 0;
                match self.cursor_y.checked_add(6) {
                    Some(cursor_y) if cursor_y <= MAX_SIXEL_DIMENSION => {
                        self.cursor_y = cursor_y;
                        true
                    },
                    _ => false,
                }
            },
            _ => true,
        }
    }
    fn draw(&mut self, pixels_wide: usize) -> bool {
        let right_edge = match self.cursor_x.checked_add(pixels_wide) {
            Some(right_edge) => right_edge,
            None => return false,
        };
        // a sixel is six pixels tall
        let bottom_edge = self.cursor_y.saturating_add(6);
        if !self.grow_to(right_edge, bottom_edge) {
            return false;
        }
        self.cursor_x = right_edge;
        true
    }
    fn grow_to(&mut self, width: usize, height: usize) -> bool {
        let width = std::cmp::max(self.width, width);
        let height = std::cmp::max(self.height, height);
        let within_limits = width <= MAX_SIXEL_DIMENSION
            && height <= MAX_SIXEL_DIMENSION
            && width
                .checked_mul(height)
                .is_some_and(|pixels| pixels <= MAX_SIXEL_PIXELS);
        if within_limits {
            self.width = width;
            self.height = height;
        }
        within_limits
    }
}

#[derive(Debug, Clone, Default)]
pub struct SixelGrid {
    sixel_image_locations: HashMap<usize, PixelRect>,
    previous_cell_size: Option<SizeInPixels>,
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
    currently_parsing: Option<SixelDeserializer>,
    size_guard: SixelSizeGuard,
    image_ids_to_reap: Vec<usize>,
    images_to_check_for_emptiness: HashSet<usize>,
    sixel_parser: Option<sixel_tokenizer::Parser>,
    pub sixel_image_store: Rc<RefCell<SixelImageStore>>,
}

impl SixelGrid {
    pub fn new(
        character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
        sixel_image_store: Rc<RefCell<SixelImageStore>>,
    ) -> Self {
        let previous_cell_size = *character_cell_size.borrow();
        SixelGrid {
            previous_cell_size,
            character_cell_size,
            sixel_image_store,
            ..Default::default()
        }
    }
    pub fn handle_byte(&mut self, byte: u8) {
        let Some(parser) = self.sixel_parser.as_mut() else {
            return;
        };
        parser.advance(&byte, |sixel_event| {
            Self::feed_event(
                &mut self.currently_parsing,
                &mut self.size_guard,
                sixel_event,
            )
        });
    }
    pub fn handle_event(&mut self, sixel_event: SixelEvent) {
        Self::feed_event(
            &mut self.currently_parsing,
            &mut self.size_guard,
            sixel_event,
        );
    }
    // an event that would take the image over the size limits abandons the
    // whole image; the rest of its bytes are then read and ignored
    fn feed_event(
        currently_parsing: &mut Option<SixelDeserializer>,
        size_guard: &mut SixelSizeGuard,
        sixel_event: SixelEvent,
    ) {
        if let Some(deserializer) = currently_parsing.as_mut() {
            if size_guard.admit(&sixel_event) {
                let _ = deserializer.handle_event(sixel_event);
            } else {
                log::warn!("Dropping a sixel image that exceeds the size limits");
                *currently_parsing = None;
            }
        }
    }
    pub fn is_parsing(&self) -> bool {
        self.sixel_parser.is_some()
    }
    /// Drop a sixel that was opened and never finished. Nothing is published.
    pub fn abandon_image(&mut self) {
        self.sixel_parser = None;
        self.currently_parsing = None;
    }
    pub fn start_image(
        &mut self,
        max_height_in_pixels: Option<usize>,
        dcs_intermediates: Vec<&u8>,
        dcs_params: Vec<&[u16]>,
    ) {
        self.sixel_parser = Some(sixel_tokenizer::Parser::new());
        self.size_guard = SixelSizeGuard::new(max_height_in_pixels);
        match max_height_in_pixels {
            Some(max_height_in_pixels) => {
                self.currently_parsing =
                    Some(SixelDeserializer::new().max_height(max_height_in_pixels));
            },
            None => {
                self.currently_parsing = Some(SixelDeserializer::new());
            },
        }

        self.handle_byte(27);
        self.handle_byte(b'P');

        for byte in dcs_intermediates {
            self.handle_byte(*byte);
        }

        // send DCS event to parser
        for (i, param) in dcs_params.iter().enumerate() {
            if i != 0 {
                self.handle_byte(b';');
            }
            for subparam in param.iter() {
                let mut b = [0; 4];
                for digit in subparam.to_string().chars() {
                    let len = digit.encode_utf8(&mut b).len();
                    for byte in b.iter().take(len) {
                        self.handle_byte(*byte);
                    }
                }
            }
        }
        self.handle_byte(b'q');
    }
    pub fn end_image(
        &mut self,
        new_image_id: usize,
        x_pixel_coordinates: usize,
        y_pixel_coordinates: usize,
    ) -> Option<SixelImage> {
        // usize is image_id
        self.sixel_parser = None;
        if let Some(sixel_deserializer) = self.currently_parsing.as_mut() {
            if let Ok(sixel_image) = sixel_deserializer.create_image() {
                let image_pixel_size = sixel_image.pixel_size();
                let image_size_and_coordinates = PixelRect::new(
                    x_pixel_coordinates,
                    y_pixel_coordinates,
                    image_pixel_size.0,
                    image_pixel_size.1,
                );

                // here we remove images which this image covers completely to save on system
                // resources - TODO: also do this with partial covers, eg. if several images
                // together cover one image
                for (image_id, pixel_rect) in &self.sixel_image_locations {
                    if let Some(intersecting_rect) =
                        pixel_rect.intersecting_rect(&image_size_and_coordinates)
                    {
                        if intersecting_rect.x == 0
                            && intersecting_rect.y == 0
                            && intersecting_rect.height == pixel_rect.height
                            && intersecting_rect.width == pixel_rect.width
                        {
                            self.image_ids_to_reap.push(*image_id);
                        }
                    }
                }
                for image_id in &self.image_ids_to_reap {
                    self.sixel_image_locations.remove(image_id);
                }

                self.sixel_image_locations
                    .insert(new_image_id, image_size_and_coordinates);
                self.currently_parsing = None;
                Some(sixel_image)
            } else {
                None
            }
        } else {
            None
        }
    }
    pub fn image_coordinates(&self) -> impl Iterator<Item = (usize, &PixelRect)> {
        self.sixel_image_locations
            .iter()
            .map(|(image_id, pixel_rect)| (*image_id, pixel_rect))
    }
    pub fn has_images(&self) -> bool {
        !self.sixel_image_locations.is_empty()
    }
    pub fn cut_off_rect_from_images(
        &mut self,
        rect_to_cut_out: PixelRect,
    ) -> Option<Vec<(usize, PixelRect)>> {
        // if there is an image at this cursor location, this returns the image ID and the PixelRect inside the image to be removed
        let mut ret = None;
        for (image_id, pixel_rect) in &self.sixel_image_locations {
            if let Some(intersecting_rect) = pixel_rect.intersecting_rect(&rect_to_cut_out) {
                let ret = ret.get_or_insert(vec![]);
                ret.push((*image_id, intersecting_rect));
            }
        }
        ret
    }
    pub fn offset_grid_top(&mut self) {
        if let Some(character_cell_size) = *self.character_cell_size.borrow() {
            let height_to_reduce = character_cell_size.height as isize;
            for (sixel_image_id, pixel_rect) in self.sixel_image_locations.iter_mut() {
                pixel_rect.y -= height_to_reduce;
                if pixel_rect.y + pixel_rect.height as isize <= 0 {
                    self.image_ids_to_reap.push(*sixel_image_id);
                }
            }
            for image_id in &self.image_ids_to_reap {
                self.sixel_image_locations.remove(image_id);
            }
        }
    }
    pub fn drain_image_ids_to_reap(&mut self) -> Option<Vec<usize>> {
        if !self.images_to_check_for_emptiness.is_empty() {
            let store = self.sixel_image_store.borrow();
            for image_id in self.images_to_check_for_emptiness.drain() {
                let has_lit_pixels = store.sixel_images.get(&image_id).map_or(false, |stored| {
                    stored.image.pixels.iter().flatten().any(|pixel| pixel.on)
                });
                if !has_lit_pixels {
                    self.sixel_image_locations.remove(&image_id);
                    self.image_ids_to_reap.push(image_id);
                }
            }
        }
        let images_to_reap = self.image_ids_to_reap.drain(..);
        if images_to_reap.len() > 0 {
            Some(images_to_reap.collect())
        } else {
            None
        }
    }
    pub fn character_cell_size_possibly_changed(&mut self) {
        if let (Some(previous_cell_size), Some(character_cell_size)) =
            (self.previous_cell_size, *self.character_cell_size.borrow())
        {
            if previous_cell_size != character_cell_size {
                for (_image_id, pixel_rect) in self.sixel_image_locations.iter_mut() {
                    pixel_rect.x =
                        (pixel_rect.x / previous_cell_size.width) * character_cell_size.width;
                    pixel_rect.y = (pixel_rect.y / previous_cell_size.height as isize)
                        * character_cell_size.height as isize;
                }
            }
        }
        self.previous_cell_size = *self.character_cell_size.borrow();
    }
    pub fn clear(&mut self) -> Option<Vec<usize>> {
        // returns image ids to reap
        let mut image_ids: Vec<usize> = self
            .sixel_image_locations
            .drain()
            .map(|(image_id, _image_rect)| image_id)
            .collect();
        image_ids.append(&mut self.image_ids_to_reap);
        self.images_to_check_for_emptiness.clear();
        if !image_ids.is_empty() {
            Some(image_ids)
        } else {
            None
        }
    }
    pub fn next_image_id(&self) -> usize {
        let mut store = self.sixel_image_store.borrow_mut();
        let id = store.next_image_id;
        store.next_image_id += 1;
        id
    }
    pub fn new_sixel_image(&mut self, sixel_image_id: usize, sixel_image: SixelImage) -> bool {
        let referenced_image_ids: HashSet<usize> =
            self.sixel_image_locations.keys().copied().collect();
        let admitted = self.sixel_image_store.borrow_mut().insert_image(
            sixel_image_id,
            sixel_image,
            &referenced_image_ids,
        );
        if admitted {
            self.images_to_check_for_emptiness.insert(sixel_image_id);
        } else {
            log::warn!("Refusing a sixel image that exceeds the image store budget");
            self.sixel_image_locations.remove(&sixel_image_id);
        }
        admitted
    }
    pub fn remove_pixels_from_image(&mut self, image_id: usize, pixel_rect: PixelRect) {
        let mut store = self.sixel_image_store.borrow_mut();
        let freed_cache_bytes = if let Some(stored) = store.sixel_images.get_mut(&image_id) {
            stored.image.cut_out(
                pixel_rect.x,
                pixel_rect.y as usize,
                pixel_rect.width,
                pixel_rect.height,
            );
            let freed_cache_bytes = stored.cache.bytes;
            stored.cache.clear(); // TODO: more intelligent cache clearing
            freed_cache_bytes
        } else {
            0
        };
        store.total_bytes = store.total_bytes.saturating_sub(freed_cache_bytes);
        drop(store);
        self.images_to_check_for_emptiness.insert(image_id);
    }
    pub fn reap_images(&mut self, ids_to_reap: Vec<usize>) {
        let mut store = self.sixel_image_store.borrow_mut();
        for id in ids_to_reap {
            store.remove_image(id);
        }
    }
    pub fn image_cell_coordinates_in_viewport(
        &self,
        viewport_height: usize,
        scrollback_height: usize,
    ) -> Vec<(usize, usize, usize, usize)> {
        match *self.character_cell_size.borrow() {
            Some(character_cell_size) => self
                .sixel_image_locations
                .iter()
                .map(|(_image_id, pixel_rect)| {
                    let scrollback_size_in_pixels = scrollback_height * character_cell_size.height;
                    let y_pixel_coordinates_in_viewport =
                        pixel_rect.y - scrollback_size_in_pixels as isize;
                    let image_y = usize::try_from(y_pixel_coordinates_in_viewport.max(0))
                        .unwrap_or(0)
                        / character_cell_size.height;
                    let image_x = pixel_rect.x / character_cell_size.width;
                    let image_height_in_pixels = if y_pixel_coordinates_in_viewport < 0 {
                        pixel_rect.height as isize + y_pixel_coordinates_in_viewport
                    } else {
                        pixel_rect.height as isize
                    };
                    let image_height_in_pixels =
                        usize::try_from(image_height_in_pixels.max(0)).unwrap_or(0);
                    let image_height = image_height_in_pixels / character_cell_size.height;
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
    pub fn changed_sixel_chunks_in_viewport(
        &self,
        changed_rects: HashMap<usize, usize>,
        scrollback_size_in_lines: usize,
        viewport_width_in_cells: usize,
        viewport_x_offset: usize,
        viewport_y_offset: usize,
    ) -> Vec<SixelImageChunk> {
        let mut changed_sixel_image_chunks = vec![];
        if let Some(character_cell_size) = { *self.character_cell_size.borrow() } {
            for (sixel_image_id, sixel_image_pixel_rect) in self.image_coordinates() {
                for (line_index, line_count) in &changed_rects {
                    let changed_rect_pixel_height = line_count * character_cell_size.height;
                    let changed_rect = PixelRect {
                        x: 0,
                        y: ((line_index + scrollback_size_in_lines) * character_cell_size.height)
                            as isize,
                        width: viewport_width_in_cells * character_cell_size.width,
                        height: changed_rect_pixel_height,
                    };
                    if let Some(chunk) = sixel_image_chunk_from_rects(
                        sixel_image_id,
                        sixel_image_pixel_rect,
                        &changed_rect,
                        *line_index,
                        &character_cell_size,
                        viewport_x_offset,
                        viewport_y_offset,
                    ) {
                        changed_sixel_image_chunks.push(chunk);
                    }
                }
            }
        }
        changed_sixel_image_chunks
    }
}

/// Aggregate bound for serialized crop encodings cached per sixel image.
/// Scrolling and resizing can generate many distinct crop rectangles;
/// the cache is a pure regeneration cache, so entries are evicted
/// least-recently-used first to stay inside this budget.
const MAX_SIXEL_CROP_CACHE_BYTES_PER_IMAGE: usize = 4 * 1024 * 1024;
/// Total budget for live sixel rasters and their crop caches. A per-image
/// crop cap alone still lets many images add up without bound.
const MAX_SIXEL_STORE_BYTES: usize = 256 * 1024 * 1024;
/// Bound on store entries so tiny rasters and per-image bookkeeping cannot
/// grow without bound while staying under the byte budget.
const MAX_SIXEL_STORE_IMAGES: usize = 512;

#[derive(Debug, Clone)]
struct SixelCropEntry {
    stamp: u64,
    text: std::sync::Arc<str>,
}

#[derive(Debug, Clone, Default)]
struct SixelImageCache {
    entries: HashMap<PixelRect, SixelCropEntry>,
    by_stamp: BTreeMap<(u64, PixelRect), usize>,
    bytes: usize,
}

impl SixelImageCache {
    fn get(&mut self, rect: &PixelRect, stamp: u64) -> Option<std::sync::Arc<str>> {
        let old_stamp = {
            let entry = self.entries.get(rect)?;
            entry.stamp
        };
        self.by_stamp.remove(&(old_stamp, *rect));
        let entry = self.entries.get_mut(rect)?;
        entry.stamp = stamp;
        self.by_stamp.insert((stamp, *rect), entry.text.len());
        Some(entry.text.clone())
    }
    fn insert(&mut self, rect: PixelRect, text: std::sync::Arc<str>, stamp: u64) {
        if text.len() > MAX_SIXEL_CROP_CACHE_BYTES_PER_IMAGE {
            return;
        }
        if let Some(old) = self.entries.insert(rect, SixelCropEntry { stamp, text }) {
            self.by_stamp.remove(&(old.stamp, rect));
            self.bytes = self.bytes.saturating_sub(old.text.len());
        }
        let text_len = self
            .entries
            .get(&rect)
            .map(|entry| entry.text.len())
            .unwrap_or(0);
        self.by_stamp.insert((stamp, rect), text_len);
        self.bytes += text_len;
        while self.bytes > MAX_SIXEL_CROP_CACHE_BYTES_PER_IMAGE {
            let Some(((old_stamp, old_rect), old_len)) = self.by_stamp.pop_first() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&old_rect) {
                if entry.stamp == old_stamp {
                    self.bytes = self.bytes.saturating_sub(old_len);
                }
            }
        }
    }
    fn clear(&mut self) {
        self.entries.clear();
        self.by_stamp.clear();
        self.bytes = 0;
    }
    #[cfg(test)]
    fn cached_bytes(&self) -> usize {
        self.bytes
    }
}

#[derive(Debug, Clone)]
struct StoredSixelImage {
    image: SixelImage,
    cache: SixelImageCache,
}

impl StoredSixelImage {
    fn base_bytes(image: &SixelImage) -> usize {
        let pixel_bytes = std::mem::size_of::<sixel_image::Pixel>();
        let row_overhead = std::mem::size_of::<Vec<sixel_image::Pixel>>();
        image
            .pixels
            .capacity()
            .saturating_mul(row_overhead)
            .saturating_add(
                image
                    .pixels
                    .iter()
                    .map(|row| row.capacity().saturating_mul(pixel_bytes))
                    .sum::<usize>(),
            )
            .saturating_add(image.color_registers.len().saturating_mul(64))
    }
    fn memory_bytes(&self) -> usize {
        Self::base_bytes(&self.image).saturating_add(self.cache.bytes)
    }
}

#[derive(Debug, Clone)]
pub struct SixelImageStore {
    sixel_images: HashMap<usize, StoredSixelImage>,
    next_image_id: usize,
    next_lru_stamp: u64,
    total_bytes: usize,
    max_bytes: usize,
    max_images: usize,
}

impl Default for SixelImageStore {
    fn default() -> Self {
        Self::with_limits(MAX_SIXEL_STORE_BYTES, MAX_SIXEL_STORE_IMAGES)
    }
}

impl SixelImageStore {
    fn with_limits(max_bytes: usize, max_images: usize) -> Self {
        SixelImageStore {
            sixel_images: HashMap::new(),
            next_image_id: 0,
            next_lru_stamp: 0,
            total_bytes: 0,
            max_bytes,
            max_images,
        }
    }
    /// Admit `image` only if the store can hold it without evicting an image
    /// that is still referenced. Unreferenced images are pruned first because
    /// their store entries are no longer displayed.
    fn insert_image(
        &mut self,
        image_id: usize,
        image: SixelImage,
        referenced_image_ids: &HashSet<usize>,
    ) -> bool {
        let image_bytes = StoredSixelImage::base_bytes(&image);
        if image_bytes > self.max_bytes {
            return false;
        }
        if self.sixel_images.contains_key(&image_id) {
            self.remove_image(image_id);
        }
        if self.would_exceed_budget(image_bytes) {
            self.prune_unreferenced_images(referenced_image_ids);
        }
        let needed_after_derived_eviction = self.max_bytes.saturating_sub(image_bytes);
        if self.would_exceed_budget(image_bytes) {
            self.trim_crops_to(needed_after_derived_eviction);
        }
        if self.would_exceed_budget(image_bytes) {
            return false;
        }
        self.total_bytes = self.total_bytes.saturating_add(image_bytes);
        self.sixel_images.insert(
            image_id,
            StoredSixelImage {
                image,
                cache: SixelImageCache::default(),
            },
        );
        true
    }
    fn would_exceed_budget(&self, incoming_bytes: usize) -> bool {
        self.total_bytes.saturating_add(incoming_bytes) > self.max_bytes
            || self.sixel_images.len() >= self.max_images
    }
    fn prune_unreferenced_images(&mut self, referenced_image_ids: &HashSet<usize>) {
        let unreferenced: Vec<usize> = self
            .sixel_images
            .keys()
            .filter(|image_id| !referenced_image_ids.contains(image_id))
            .copied()
            .collect();
        for image_id in unreferenced {
            self.remove_image(image_id);
        }
    }
    fn remove_image(&mut self, image_id: usize) {
        if let Some(stored) = self.sixel_images.remove(&image_id) {
            self.total_bytes = self.total_bytes.saturating_sub(stored.memory_bytes());
        }
    }
    /// Drop the oldest regenerable crop encodings until the store fits in
    /// `target_bytes`. Base rasters are never dropped here.
    fn trim_crops_to(&mut self, target_bytes: usize) {
        if self.total_bytes <= target_bytes {
            return;
        }
        let mut entries: Vec<(u64, usize, PixelRect, usize)> = Vec::new();
        for (image_id, stored) in &self.sixel_images {
            for (rect, entry) in &stored.cache.entries {
                entries.push((entry.stamp, *image_id, *rect, entry.text.len()));
            }
        }
        entries.sort_unstable_by_key(|(stamp, _, _, _)| *stamp);
        for (stamp, image_id, rect, len) in entries {
            if self.total_bytes <= target_bytes {
                break;
            }
            if let Some(stored) = self.sixel_images.get_mut(&image_id) {
                if stored
                    .cache
                    .entries
                    .get(&rect)
                    .is_some_and(|entry| entry.stamp == stamp)
                {
                    stored.cache.entries.remove(&rect);
                    stored.cache.by_stamp.remove(&(stamp, rect));
                    stored.cache.bytes = stored.cache.bytes.saturating_sub(len);
                    self.total_bytes = self.total_bytes.saturating_sub(len);
                }
            }
        }
    }
    pub fn serialize_image(
        &mut self,
        image_id: usize,
        pixel_x: usize,
        pixel_y: usize,
        pixel_width: usize,
        pixel_height: usize,
    ) -> Option<std::sync::Arc<str>> {
        self.next_lru_stamp += 1;
        let stamp = self.next_lru_stamp;
        let (serialized_image, cache_bytes_before, cache_bytes_after) = {
            let stored = self.sixel_images.get_mut(&image_id)?;
            let cache_bytes_before = stored.cache.bytes;
            let rect = PixelRect::new(pixel_x, pixel_y, pixel_height, pixel_width);
            let serialized_image = if let Some(cached_image) = stored.cache.get(&rect, stamp) {
                cached_image
            } else {
                let serialized_image: std::sync::Arc<str> = stored
                    .image
                    .serialize_range(pixel_x, pixel_y, pixel_width, pixel_height)
                    .into();
                stored.cache.insert(rect, serialized_image.clone(), stamp);
                serialized_image
            };
            (serialized_image, cache_bytes_before, stored.cache.bytes)
        };
        if cache_bytes_after >= cache_bytes_before {
            self.total_bytes = self
                .total_bytes
                .saturating_add(cache_bytes_after - cache_bytes_before);
        } else {
            self.total_bytes = self
                .total_bytes
                .saturating_sub(cache_bytes_before - cache_bytes_after);
        }
        self.trim_crops_to(self.max_bytes);
        Some(serialized_image)
    }
    pub fn image_count(&self) -> usize {
        self.sixel_images.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell_size() -> SizeInPixels {
        SizeInPixels {
            width: 10,
            height: 10,
        }
    }

    #[test]
    fn crop_cache_is_bounded_and_evicts_oldest_first() {
        let mut cache = SixelImageCache::default();
        let cap = MAX_SIXEL_CROP_CACHE_BYTES_PER_IMAGE;
        // Each entry uses 1/8 of the budget; inserting more than 8 crops
        // must evict the oldest instead of growing past the cap.
        let entry_len = cap / 8;
        let mut inserted_rects = Vec::new();
        for i in 0..12usize {
            let rect = PixelRect::new(i, 0, 1, 1);
            let text: std::sync::Arc<str> = "x".repeat(entry_len).into();
            cache.insert(rect, text, i as u64);
            inserted_rects.push(rect);
            assert!(cache.cached_bytes() <= cap);
        }
        for evicted in &inserted_rects[..4] {
            assert!(!cache.entries.contains_key(evicted));
        }
        for kept in &inserted_rects[4..] {
            assert!(cache.entries.contains_key(kept));
        }

        // A hit refreshes the stamp so the hot crop survives the next
        // eviction round.
        let hot = inserted_rects[11];
        let hit = cache.get(&hot, 100).unwrap();
        let another = PixelRect::new(100, 0, 1, 1);
        cache.insert(another, "x".repeat(entry_len).into(), 101);
        assert!(cache.entries.contains_key(&hot));
        assert!(std::sync::Arc::ptr_eq(&hit, &cache.get(&hot, 102).unwrap()));

        // An oversized crop is never retained.
        let huge = PixelRect::new(200, 0, 1, 1);
        cache.insert(huge, "x".repeat(cap + 1).into(), 103);
        assert!(!cache.entries.contains_key(&huge));
        assert!(cache.cached_bytes() <= cap);
        assert_eq!(cache.by_stamp.len(), cache.entries.len());
    }

    #[test]
    fn crop_cache_keeps_its_ordered_index_in_sync() {
        let mut cache = SixelImageCache::default();
        for i in 0..100usize {
            let rect = PixelRect::new(i, 0, 1, 1);
            cache.insert(rect, "x".repeat(8).into(), i as u64);
        }
        assert_eq!(cache.by_stamp.len(), cache.entries.len());
        assert_eq!(cache.by_stamp.len(), 100);

        // Refresh a hit; the old ordered entry must not linger.
        let hot = PixelRect::new(50, 0, 1, 1);
        cache.get(&hot, 1_000).unwrap();
        assert_eq!(cache.by_stamp.len(), cache.entries.len());
        assert_eq!(cache.by_stamp.get(&(1_000, hot)), Some(&8));

        // Replacing an entry also replaces its ordered key.
        cache.insert(hot, "x".repeat(16).into(), 1_001);
        assert_eq!(cache.by_stamp.len(), cache.entries.len());
        assert_eq!(cache.by_stamp.get(&(1_001, hot)), Some(&16));
        assert!(!cache.by_stamp.contains_key(&(1_000, hot)));
    }

    fn store_test_image() -> SixelImage {
        parse_sixel("\"1;1;8;12#0;2;100;0;0!8~-!8~\u{1b}\\").expect("an image")
    }

    #[test]
    fn the_store_refuses_admission_over_its_memory_budget() {
        let image = store_test_image();
        let image_bytes = StoredSixelImage::base_bytes(&image);
        let mut store = SixelImageStore::with_limits(image_bytes * 2, 16);
        let mut referenced = HashSet::new();
        assert!(store.insert_image(1, image.clone(), &referenced));
        referenced.insert(1);
        assert!(store.insert_image(2, image.clone(), &referenced));
        referenced.insert(2);

        assert_eq!(store.total_bytes, image_bytes * 2);
        assert!(!store.insert_image(3, image.clone(), &referenced));
        assert_eq!(store.image_count(), 2);
        assert_eq!(store.total_bytes, image_bytes * 2);

        // Releasing the oldest reference makes room without touching a
        // still-referenced image.
        referenced.remove(&1);
        assert!(store.insert_image(3, image, &referenced));
        assert!(!store.sixel_images.contains_key(&1));
        assert!(store.sixel_images.contains_key(&2));
        assert!(store.sixel_images.contains_key(&3));
        assert_eq!(store.total_bytes, image_bytes * 2);
    }

    #[test]
    fn the_store_refuses_admission_over_its_image_count_budget() {
        let image = store_test_image();
        let mut store = SixelImageStore::with_limits(usize::MAX, 2);
        let mut referenced = HashSet::new();
        assert!(store.insert_image(1, image.clone(), &referenced));
        referenced.insert(1);
        assert!(store.insert_image(2, image.clone(), &referenced));
        referenced.insert(2);
        referenced.insert(3);

        assert!(!store.insert_image(3, image.clone(), &referenced));
        assert_eq!(store.image_count(), 2);

        referenced.remove(&1);
        assert!(store.insert_image(3, image, &referenced));
        assert!(!store.sixel_images.contains_key(&1));
        assert_eq!(store.image_count(), 2);
    }

    #[test]
    fn the_store_bounds_all_crop_caches_together() {
        let image = store_test_image();
        let image_bytes = StoredSixelImage::base_bytes(&image);
        let mut store = SixelImageStore::with_limits(image_bytes + 64, 4);
        let mut referenced = HashSet::new();
        assert!(store.insert_image(1, image, &referenced));
        referenced.insert(1);
        for i in 0..40usize {
            store.serialize_image(1, i % 12, i % 8, 1, 1);
            assert!(
                store.total_bytes <= store.max_bytes,
                "store grew to {} bytes over a {} byte budget",
                store.total_bytes,
                store.max_bytes
            );
        }
        let cache_bytes: usize = store
            .sixel_images
            .values()
            .map(|stored| stored.cache.bytes)
            .sum();
        assert!(cache_bytes <= 64);
    }

    #[test]
    fn a_negative_image_top_uses_the_changed_rect_as_destination_origin() {
        let image_rect = PixelRect {
            x: 0,
            y: -10,
            width: 20,
            height: 20,
        };
        let changed_rect = PixelRect {
            x: 0,
            y: 0,
            width: 20,
            height: 10,
        };

        let chunk =
            sixel_image_chunk_from_rects(1, &image_rect, &changed_rect, 0, &cell_size(), 0, 0)
                .expect("the rectangles intersect");

        assert_eq!(chunk.cell_x, 0);
        assert_eq!(chunk.cell_y, 0);
        assert_eq!(chunk.sixel_image_pixel_x, 0);
        assert_eq!(chunk.sixel_image_pixel_y, 10);
        assert_eq!(chunk.sixel_image_pixel_width, 20);
        assert_eq!(chunk.sixel_image_pixel_height, 10);
    }

    #[test]
    fn an_image_starting_below_the_changed_rect_starts_on_the_correct_row() {
        let image_rect = PixelRect {
            x: 0,
            y: 10,
            width: 20,
            height: 10,
        };
        let changed_rect = PixelRect {
            x: 0,
            y: 0,
            width: 20,
            height: 30,
        };

        let chunk =
            sixel_image_chunk_from_rects(1, &image_rect, &changed_rect, 0, &cell_size(), 0, 3)
                .expect("the rectangles intersect");

        assert_eq!(chunk.cell_y, 4);
        assert_eq!(chunk.sixel_image_pixel_y, 0);
        assert_eq!(chunk.sixel_image_pixel_height, 10);
    }

    fn parse_sixel(body: &str) -> Option<SixelImage> {
        let mut grid = SixelGrid::default();
        grid.start_image(None, vec![], vec![]);
        for byte in body.bytes() {
            grid.handle_byte(byte);
        }
        grid.end_image(1, 0, 0)
    }

    #[test]
    fn an_image_within_the_limits_is_parsed() {
        let image = parse_sixel("\"1;1;8;12#0;2;100;0;0!8~-!8~\u{1b}\\").expect("an image");
        assert_eq!(image.pixel_size(), (12, 8));
        // a wide, single line image right at the width limit
        let wide = parse_sixel("#0;2;100;0;0!10000~\u{1b}\\").expect("an image");
        assert_eq!(wide.pixel_size().1, 10_000);
    }

    #[test]
    fn a_declared_raster_beyond_the_pixel_budget_drops_the_image() {
        for header in ["\"1;1;8000;8000", "\"1;1;10;11000", "\"1;1;11000;10"] {
            let body = format!("{}#0;2;100;0;0~\u{1b}\\", header);
            assert!(parse_sixel(&body).is_none(), "{}", header);
        }
    }

    #[test]
    fn an_absurdly_large_declared_raster_drops_the_image() {
        assert!(parse_sixel("\"1;1;99999;99999#0;2;100;0;0~\u{1b}\\").is_none());
    }

    #[test]
    fn repeats_beyond_the_width_limit_drop_the_image() {
        assert!(parse_sixel("#0;2;100;0;0!99999~\u{1b}\\").is_none());
        // the width accumulates over several runs on the same line
        assert!(parse_sixel("#0;2;100;0;0!6000~!6000~\u{1b}\\").is_none());
        // a carriage return starts the width count over
        assert!(parse_sixel("#0;2;100;0;0!6000~$!6000~\u{1b}\\").is_some());
    }

    #[test]
    fn newlines_beyond_the_height_limit_drop_the_image() {
        let body = format!("#0;2;100;0;0~{}~\u{1b}\\", "-".repeat(1700));
        assert!(parse_sixel(&body).is_none());
    }

    #[test]
    fn an_image_beyond_the_total_pixel_budget_drops_the_image() {
        // each dimension is within bounds, 10000 x 2700 pixels are not
        let body = format!("#0;2;100;0;0{}\u{1b}\\", "!10000~-".repeat(450));
        assert!(parse_sixel(&body).is_none());
    }

    fn parse_sixel_with(
        max_height: Option<usize>,
        dcs: Vec<&[u16]>,
        body: &str,
    ) -> Option<SixelImage> {
        let mut grid = SixelGrid::default();
        grid.start_image(max_height, vec![], dcs);
        for byte in body.bytes() {
            grid.handle_byte(byte);
        }
        grid.end_image(1, 0, 0)
    }

    #[test]
    fn a_transparent_raster_is_not_charged_for_padding_the_buffer_never_gets() {
        // P0;1;q: transparent background. sixel-image does not pad for the
        // declared raster, so only the 800 x 6 pixels drawn exist.
        let body = "\"1;1;800;6000#0;2;100;0;0!800~\u{1b}\\";
        let transparent = parse_sixel_with(None, vec![&[0], &[1]], body)
            .expect("a transparent image with a large declared raster");
        assert_eq!(transparent.pixel_size().1, 800);
        // the same image with an opaque background is padded to 6000 x 6000
        assert!(parse_sixel_with(None, vec![], body).is_none());
    }

    #[test]
    fn a_tall_image_is_clipped_at_the_maximum_height_not_discarded() {
        // 2000 sixel lines, 12000 pixels, but only the first lines are kept
        let body = format!("#0;2;100;0;0{}\u{1b}\\", "~-".repeat(2000));
        assert!(parse_sixel_with(Some(100), vec![], &body).is_some());
        // without a maximum height the same image exceeds the limits
        assert!(parse_sixel_with(None, vec![], &body).is_none());
    }

    #[test]
    fn a_raster_header_after_the_maximum_height_is_ignored() {
        let body = format!("#0;2;100;0;0{}\"1;1;11000;11000~\u{1b}\\", "~-".repeat(5));
        assert!(parse_sixel_with(Some(30), vec![], &body).is_some());
    }

    #[test]
    fn a_dropped_image_does_not_affect_the_next_one() {
        let mut grid = SixelGrid::default();
        grid.start_image(None, vec![], vec![]);
        for byte in "#0;2;100;0;0!99999~".bytes() {
            grid.handle_byte(byte);
        }
        assert!(grid.end_image(1, 0, 0).is_none());
        grid.start_image(None, vec![], vec![]);
        for byte in "#0;2;100;0;0!8~".bytes() {
            grid.handle_byte(byte);
        }
        assert!(grid.end_image(2, 0, 0).is_some());
    }
}

use crate::output::SixelImageChunk;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use sixel_image::{SixelDeserializer, SixelImage};
use sixel_tokenizer::SixelEvent;

use std::fmt::Debug;

use zellij_utils::pane_size::SizeInPixels;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Default)]
pub struct SixelGrid {
    sixel_image_locations: HashMap<usize, PixelRect>,
    previous_cell_size: Option<SizeInPixels>,
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
    currently_parsing: Option<SixelDeserializer>,
    image_ids_to_reap: Vec<usize>,
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
        self.sixel_parser
            .as_mut()
            .unwrap()
            .advance(&byte, |sixel_event| {
                if let Some(currently_parsing) = self.currently_parsing.as_mut() {
                    let _ = currently_parsing.handle_event(sixel_event);
                }
            });
    }
    pub fn handle_event(&mut self, sixel_event: SixelEvent) {
        if let Some(currently_parsing) = self.currently_parsing.as_mut() {
            let _ = currently_parsing.handle_event(sixel_event);
        }
    }
    pub fn is_parsing(&self) -> bool {
        self.sixel_parser.is_some()
    }
    pub fn start_image(
        &mut self,
        max_height_in_pixels: Option<usize>,
        dcs_intermediates: Vec<&u8>,
        dcs_params: Vec<&[u16]>,
    ) {
        self.sixel_parser = Some(sixel_tokenizer::Parser::new());
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
                        if intersecting_rect.x == pixel_rect.x
                            && intersecting_rect.y == pixel_rect.y
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
        if !image_ids.is_empty() {
            Some(image_ids)
        } else {
            None
        }
    }
    pub fn next_image_id(&self) -> usize {
        self.sixel_image_store.borrow().sixel_images.keys().len()
    }
    pub fn new_sixel_image(&mut self, sixel_image_id: usize, sixel_image: SixelImage) {
        self.sixel_image_store
            .borrow_mut()
            .sixel_images
            .insert(sixel_image_id, (sixel_image, HashMap::new()));
    }
    pub fn remove_pixels_from_image(&mut self, image_id: usize, pixel_rect: PixelRect) {
        if let Some((sixel_image, sixel_image_cache)) = self
            .sixel_image_store
            .borrow_mut()
            .sixel_images
            .get_mut(&image_id)
        {
            sixel_image.cut_out(
                pixel_rect.x,
                pixel_rect.y as usize,
                pixel_rect.width,
                pixel_rect.height,
            );
            sixel_image_cache.clear(); // TODO: more intelligent cache clearing
        }
    }
    pub fn reap_images(&mut self, ids_to_reap: Vec<usize>) {
        for id in ids_to_reap {
            drop(self.sixel_image_store.borrow_mut().sixel_images.remove(&id));
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
                    let changed_rect_top_edge = ((line_index + scrollback_size_in_lines)
                        * character_cell_size.height)
                        as isize;
                    let changed_rect_bottom_edge =
                        changed_rect_top_edge + changed_rect_pixel_height as isize;
                    let sixel_image_top_edge = sixel_image_pixel_rect.y;
                    let sixel_image_bottom_edge =
                        sixel_image_pixel_rect.y + sixel_image_pixel_rect.height as isize;

                    let cell_x_in_current_pane =
                        sixel_image_pixel_rect.x / character_cell_size.width;
                    let cell_x = viewport_x_offset + cell_x_in_current_pane;
                    let sixel_image_pixel_width = if sixel_image_pixel_rect.x
                        + sixel_image_pixel_rect.width
                        <= (viewport_width_in_cells * character_cell_size.width)
                    {
                        sixel_image_pixel_rect.width
                    } else {
                        (viewport_width_in_cells * character_cell_size.width)
                            .saturating_sub(sixel_image_pixel_rect.x)
                    };
                    if sixel_image_pixel_width == 0 {
                        continue;
                    }

                    let sixel_image_cell_distance_from_scrollback_top =
                        sixel_image_top_edge as usize / character_cell_size.height;
                    // if the image is above the rect top, this will be 0
                    let sixel_image_cell_distance_from_changed_rect_top =
                        sixel_image_cell_distance_from_scrollback_top
                            .saturating_sub(line_index + scrollback_size_in_lines);
                    let cell_y = viewport_y_offset
                        + line_index
                        + sixel_image_cell_distance_from_changed_rect_top;
                    let sixel_image_pixel_x = 0;
                    // if the image is above the rect top, this will be 0
                    let sixel_image_pixel_y = (changed_rect_top_edge as usize)
                        .saturating_sub(sixel_image_top_edge as usize)
                        as usize;
                    let sixel_image_pixel_height = std::cmp::min(
                        (std::cmp::min(changed_rect_bottom_edge, sixel_image_bottom_edge)
                            - std::cmp::max(changed_rect_top_edge, sixel_image_top_edge))
                            as usize,
                        sixel_image_pixel_rect.height,
                    );

                    if (sixel_image_top_edge >= changed_rect_top_edge
                        && sixel_image_top_edge <= changed_rect_bottom_edge)
                        || (sixel_image_bottom_edge >= changed_rect_top_edge
                            && sixel_image_bottom_edge <= changed_rect_bottom_edge)
                        || (sixel_image_bottom_edge >= changed_rect_bottom_edge
                            && sixel_image_top_edge <= changed_rect_top_edge)
                    {
                        changed_sixel_image_chunks.push(SixelImageChunk {
                            cell_x,
                            cell_y,
                            sixel_image_pixel_x,
                            sixel_image_pixel_y,
                            sixel_image_pixel_width,
                            sixel_image_pixel_height,
                            sixel_image_id,
                        });
                    }
                }
            }
        }
        changed_sixel_image_chunks
    }
}

type SixelImageCache = HashMap<PixelRect, String>;
#[derive(Debug, Clone, Default)]
pub struct SixelImageStore {
    sixel_images: HashMap<usize, (SixelImage, SixelImageCache)>,
}

impl SixelImageStore {
    pub fn serialize_image(
        &mut self,
        image_id: usize,
        pixel_x: usize,
        pixel_y: usize,
        pixel_width: usize,
        pixel_height: usize,
    ) -> Option<String> {
        self.sixel_images
            .get_mut(&image_id)
            .map(|(sixel_image, sixel_image_cache)| {
                if let Some(cached_image) = sixel_image_cache.get(&PixelRect::new(
                    pixel_x,
                    pixel_y,
                    pixel_height,
                    pixel_width,
                )) {
                    cached_image.clone()
                } else {
                    let serialized_image =
                        sixel_image.serialize_range(pixel_x, pixel_y, pixel_width, pixel_height);
                    sixel_image_cache.insert(
                        PixelRect::new(pixel_x, pixel_y, pixel_height, pixel_width),
                        serialized_image.clone(),
                    );
                    serialized_image
                }
            })
    }
    pub fn image_count(&self) -> usize {
        self.sixel_images.len()
    }
}

use std::collections::BTreeMap;
use std::rc::Rc;

use sixel_tokenizer::{ColorCoordinateSystem, Parser, SixelEvent};

use crate::color;
use crate::kitty::Image;

const MAX_DIMENSION: usize = 10_000;
const MAX_IMAGE_BYTES: usize = 104_857_600;
const MAX_CHUNKS: usize = 256;
const MAX_REPEAT: usize = MAX_DIMENSION;
const BAND: usize = 6;

const DEFAULT_REGISTERS: [[usize; 3]; 16] = [
    [0, 0, 0],
    [20, 20, 80],
    [80, 13, 13],
    [20, 80, 20],
    [80, 20, 80],
    [20, 80, 80],
    [80, 80, 20],
    [53, 53, 53],
    [26, 26, 26],
    [33, 33, 60],
    [60, 26, 26],
    [33, 60, 33],
    [60, 33, 60],
    [33, 60, 60],
    [60, 60, 33],
    [80, 80, 80],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
    pub width: u32,
    pub height: u32,
}

impl CellSize {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    fn right(&self) -> u32 {
        self.x.saturating_add(self.width)
    }

    fn bottom(&self) -> u32 {
        self.y.saturating_add(self.height)
    }

    fn intersection(&self, other: &PixelRect) -> Option<PixelRect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= x || bottom <= y {
            return None;
        }
        Some(PixelRect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub left: usize,
    pub right: usize,
}

impl Span {
    pub fn new(line: usize, left: usize, right: usize) -> Self {
        Self { line, left, right }
    }

    fn rect(&self, cell: CellSize) -> PixelRect {
        let x = (self.left as u32).saturating_mul(cell.width);
        let right = (self.right as u32)
            .saturating_add(1)
            .saturating_mul(cell.width);
        PixelRect {
            x,
            y: (self.line as u32).saturating_mul(cell.height),
            width: right.saturating_sub(x),
            height: cell.height,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: u64,
    pub cell_x: usize,
    pub cell_y: usize,
    pub width: u32,
    pub height: u32,
    pub revision: u64,
    pub image: Rc<Image>,
    painted: usize,
}

impl Chunk {
    pub fn rect(&self, cell: CellSize) -> PixelRect {
        PixelRect {
            x: (self.cell_x as u32).saturating_mul(cell.width),
            y: (self.cell_y as u32).saturating_mul(cell.height),
            width: self.width,
            height: self.height,
        }
    }

    fn erase(&mut self, overlap: PixelRect, origin: PixelRect, revision: u64) {
        let local_x = (overlap.x - origin.x) as usize;
        let local_y = (overlap.y - origin.y) as usize;
        let stride = self.width as usize;
        let image = Rc::make_mut(&mut self.image);

        for row in local_y..local_y + overlap.height as usize {
            let start = (row * stride + local_x) * 4;
            let end = start + overlap.width as usize * 4;
            for pixel in image.pixels[start..end].chunks_exact_mut(4) {
                if pixel[3] != 0 {
                    self.painted -= 1;
                }
                pixel.copy_from_slice(&[0, 0, 0, 0]);
            }
        }
        self.revision = revision;
    }
}

pub struct Sixels {
    chunks: BTreeMap<u64, Chunk>,
    pending: Vec<(usize, usize, Image)>,
    cell: CellSize,
    next_id: u64,
    revisions: u64,
    stamp: u64,
}

impl Sixels {
    pub fn new(cell: CellSize) -> Self {
        Self {
            chunks: BTreeMap::new(),
            pending: Vec::new(),
            cell,
            next_id: 0,
            revisions: 0,
            stamp: 0,
        }
    }

    pub fn stamp(&self) -> u64 {
        self.stamp
    }

    pub fn set_cell_size(&mut self, cell: CellSize) {
        if cell == self.cell {
            return;
        }
        self.cell = cell;
        self.clear();
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty() && self.pending.is_empty()
    }

    pub fn chunks(&self) -> impl Iterator<Item = &Chunk> {
        self.chunks.values()
    }

    pub fn resident_ids(&self) -> Vec<u64> {
        self.chunks.keys().copied().collect()
    }

    pub fn clear(&mut self) {
        self.stamp += 1;
        self.chunks.clear();
        self.pending.clear();
    }

    pub fn execute(&mut self, payload: &[u8], cursor: (usize, usize)) {
        let Some(image) = decode(payload) else {
            return;
        };
        self.pending.push((cursor.1, cursor.0, image));
    }

    pub fn cut_spans(&mut self, spans: &[Span]) {
        if self.chunks.is_empty() {
            return;
        }
        for span in spans {
            self.cut(span.rect(self.cell));
        }
    }

    pub fn commit(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        for (cell_x, cell_y, image) in pending {
            let painted = image
                .pixels
                .chunks_exact(4)
                .filter(|pixel| pixel[3] != 0)
                .count();
            self.next_id += 1;
            self.revisions += 1;
            self.stamp += 1;
            let chunk = Chunk {
                id: self.next_id,
                cell_x,
                cell_y,
                width: image.width,
                height: image.height,
                revision: self.revisions,
                image: Rc::new(image),
                painted,
            };
            self.cut(chunk.rect(self.cell));
            self.chunks.insert(chunk.id, chunk);
            while self.chunks.len() > MAX_CHUNKS {
                let oldest = *self.chunks.keys().next().unwrap_or(&0);
                self.chunks.remove(&oldest);
            }
        }
    }

    fn cut(&mut self, rect: PixelRect) {
        let cell = self.cell;
        let mut revisions = self.revisions;
        self.chunks.retain(|_, chunk| {
            let origin = chunk.rect(cell);
            let Some(overlap) = origin.intersection(&rect) else {
                return true;
            };
            revisions += 1;
            chunk.erase(overlap, origin, revisions);
            chunk.painted > 0
        });
        if revisions != self.revisions {
            self.stamp += 1;
        }
        self.revisions = revisions;
    }
}

pub fn decode(payload: &[u8]) -> Option<Image> {
    let mut raster = Raster::new();
    let mut parser = Parser::new();
    for byte in payload {
        parser.advance(byte, |event| raster.event(event));
    }
    for byte in b"\x1b\\" {
        parser.advance(byte, |event| raster.event(event));
    }
    raster.finish()
}

struct Raster {
    registers: BTreeMap<u16, [u8; 3]>,
    current: u16,
    x: usize,
    band_y: usize,
    rows: Vec<Vec<[u8; 4]>>,
    background: [u8; 4],
    declared: Option<(usize, usize)>,
    refused: bool,
}

impl Raster {
    fn new() -> Self {
        let background = {
            let [red, green, blue] = color::DEFAULT_BACKGROUND;
            [red, green, blue, 255]
        };
        Self {
            registers: default_registers(),
            current: 0,
            x: 0,
            band_y: 0,
            rows: Vec::new(),
            background,
            declared: None,
            refused: false,
        }
    }

    fn event(&mut self, event: SixelEvent) {
        if self.refused {
            return;
        }
        match event {
            SixelEvent::Dcs {
                transparent_background,
                ..
            } => {
                if transparent_background == Some(1) {
                    self.background = [0, 0, 0, 0];
                }
            },
            SixelEvent::RasterAttribute {
                ph: Some(width),
                pv: Some(height),
                ..
            } => self.declare(width, height),
            SixelEvent::RasterAttribute { .. } => {},
            SixelEvent::ColorIntroducer {
                color_number,
                color_coordinate_system,
            } => {
                if let Some(system) = color_coordinate_system {
                    self.registers.insert(color_number, resolve(system));
                }
                self.current = color_number;
            },
            SixelEvent::Data { byte } => self.data(byte, 1),
            SixelEvent::Repeat {
                repeat_count,
                byte_to_repeat,
            } => self.data(byte_to_repeat, repeat_count.min(MAX_REPEAT)),
            SixelEvent::GotoBeginningOfLine => self.x = 0,
            SixelEvent::GotoNextLine => {
                self.x = 0;
                self.band_y += BAND;
            },
            SixelEvent::End | SixelEvent::UnknownSequence(_) => {},
        }
    }

    fn declare(&mut self, width: usize, height: usize) {
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            self.refuse(format!("a sixel image of {}x{} px", width, height));
            return;
        }
        if width * height * 4 > MAX_IMAGE_BYTES {
            self.refuse(format!("a sixel image of {} bytes", width * height * 4));
            return;
        }
        self.declared = Some((width, height));
    }

    fn refuse(&mut self, what: String) {
        eprintln!("zellij-window: refusing {}", what);
        self.refused = true;
        self.rows.clear();
    }

    fn data(&mut self, byte: u8, count: usize) {
        if !(0x3f..=0x7e).contains(&byte) {
            return;
        }
        let bits = byte - 0x3f;
        let color = self.color();
        for _ in 0..count {
            if self.x >= MAX_DIMENSION {
                return;
            }
            for bit in 0..BAND {
                if bits & (1 << bit) != 0 {
                    self.paint(self.x, self.band_y + bit, color);
                }
            }
            self.x += 1;
        }
    }

    fn color(&self) -> [u8; 4] {
        let [red, green, blue] = self
            .registers
            .get(&self.current)
            .copied()
            .unwrap_or([0, 0, 0]);
        [red, green, blue, 255]
    }

    fn paint(&mut self, x: usize, y: usize, color: [u8; 4]) {
        if x >= MAX_DIMENSION || y >= MAX_DIMENSION {
            return;
        }
        if let Some((width, height)) = self.declared {
            if x >= width || y >= height {
                return;
            }
        }
        while self.rows.len() <= y {
            self.rows.push(Vec::new());
        }
        let row = &mut self.rows[y];
        if row.len() <= x {
            row.resize(x + 1, self.background);
        }
        row[x] = color;
    }

    fn finish(self) -> Option<Image> {
        if self.refused {
            return None;
        }
        let width = match self.declared {
            Some((width, _)) => width,
            None => self.rows.iter().map(|row| row.len()).max().unwrap_or(0),
        };
        let height = match self.declared {
            Some((_, height)) => height,
            None => self.rows.len(),
        };
        if width == 0 || height == 0 {
            return None;
        }
        if width * height * 4 > MAX_IMAGE_BYTES {
            eprintln!(
                "zellij-window: refusing a sixel image of {} bytes",
                width * height * 4
            );
            return None;
        }

        let mut pixels = Vec::with_capacity(width * height * 4);
        for row in 0..height {
            let painted = self.rows.get(row);
            for column in 0..width {
                let pixel = painted
                    .and_then(|row| row.get(column))
                    .copied()
                    .unwrap_or(self.background);
                pixels.extend_from_slice(&pixel);
            }
        }

        Some(Image {
            width: width as u32,
            height: height as u32,
            pixels,
        })
    }
}

fn default_registers() -> BTreeMap<u16, [u8; 3]> {
    DEFAULT_REGISTERS
        .iter()
        .enumerate()
        .map(|(register, [red, green, blue])| {
            (
                register as u16,
                [percent(*red), percent(*green), percent(*blue)],
            )
        })
        .collect()
}

fn resolve(system: ColorCoordinateSystem) -> [u8; 3] {
    match system {
        ColorCoordinateSystem::RGB(red, green, blue) => {
            [percent(red), percent(green), percent(blue)]
        },
        ColorCoordinateSystem::HLS(hue, lightness, saturation) => {
            hls_to_rgb(hue, lightness, saturation)
        },
    }
}

fn percent(value: usize) -> u8 {
    ((value.min(100) * 255 + 50) / 100) as u8
}

fn hls_to_rgb(hue: usize, lightness: usize, saturation: usize) -> [u8; 3] {
    let hue = ((hue % 360) + 240) % 360;
    let lightness = lightness.min(100) as f32 / 100.0;
    let saturation = saturation.min(100) as f32 / 100.0;

    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue as f32 / 60.0;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (red, green, blue) = match hue / 60 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let base = lightness - chroma / 2.0;

    [
        channel(red + base),
        channel(green + base),
        channel(blue + base),
    ]
}

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPAQUE_BACKGROUND: [u8; 4] = [
        color::DEFAULT_BACKGROUND[0],
        color::DEFAULT_BACKGROUND[1],
        color::DEFAULT_BACKGROUND[2],
        255,
    ];

    fn sixel(body: &str) -> Vec<u8> {
        format!("\x1bP0;1;0q{}\x1b\\", body).into_bytes()
    }

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * image.width + x) * 4) as usize;
        let mut pixel = [0u8; 4];
        pixel.copy_from_slice(&image.pixels[start..start + 4]);
        pixel
    }

    #[test]
    fn a_raster_attribute_sizes_the_canvas() {
        let image = decode(&sixel("\"1;1;4;12#0;2;100;0;0@")).unwrap();
        assert_eq!((image.width, image.height), (4, 12));
        assert_eq!(image.pixels.len(), 4 * 12 * 4);
    }

    #[test]
    fn without_a_raster_attribute_the_data_sizes_the_canvas() {
        let image = decode(&sixel("#0;2;100;100;100~~~")).unwrap();
        assert_eq!((image.width, image.height), (3, 6));
    }

    #[test]
    fn each_data_byte_paints_the_six_pixels_of_its_bits() {
        let image = decode(&sixel("\"1;1;1;6#0;2;100;0;0@")).unwrap();
        assert_eq!(pixel(&image, 0, 0), [255, 0, 0, 255]);
        for row in 1..6 {
            assert_eq!(pixel(&image, 0, row), [0, 0, 0, 0], "row {}", row);
        }
    }

    #[test]
    fn a_data_byte_with_every_bit_set_paints_the_whole_band() {
        let image = decode(&sixel("\"1;1;1;6#0;2;0;100;0\x7e")).unwrap();
        for row in 0..6 {
            assert_eq!(pixel(&image, 0, row), [0, 255, 0, 255], "row {}", row);
        }
    }

    #[test]
    fn a_repeat_paints_the_same_column_run() {
        let image = decode(&sixel("\"1;1;8;6#0;2;0;0;100!5@")).unwrap();
        for column in 0..5 {
            assert_eq!(pixel(&image, column, 0), [0, 0, 255, 255], "{}", column);
        }
        assert_eq!(pixel(&image, 5, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn a_carriage_return_paints_a_second_color_over_the_same_band() {
        let image = decode(&sixel("\"1;1;2;6#0;2;100;0;0@@$#1;2;0;0;100?@")).unwrap();
        assert_eq!(pixel(&image, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&image, 1, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn a_new_line_advances_by_one_band() {
        let image = decode(&sixel("\"1;1;1;12#0;2;100;0;0@-#0;2;100;0;0@")).unwrap();
        assert_eq!(pixel(&image, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&image, 0, 6), [255, 0, 0, 255]);
    }

    #[test]
    fn a_transparent_background_leaves_unpainted_pixels_clear() {
        let image = decode(&sixel("\"1;1;2;6#0;2;100;100;100@")).unwrap();
        assert_eq!(pixel(&image, 1, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn an_opaque_background_fills_unpainted_pixels_with_the_default_background() {
        let stream = b"\x1bP0;0;0q\"1;1;2;6#0;2;100;100;100@\x1b\\";
        let image = decode(stream).unwrap();
        assert_eq!(pixel(&image, 0, 0), [255, 255, 255, 255]);
        assert_eq!(pixel(&image, 1, 0), OPAQUE_BACKGROUND);
    }

    #[test]
    fn a_missing_transparency_parameter_is_opaque_too() {
        let image = decode(b"\x1bPq\"1;1;2;6#0;2;100;100;100@\x1b\\").unwrap();
        assert_eq!(pixel(&image, 1, 0), OPAQUE_BACKGROUND);
    }

    #[test]
    fn rgb_registers_are_percentages_of_full_scale() {
        let image = decode(&sixel("\"1;1;1;6#7;2;50;25;0@")).unwrap();
        assert_eq!(pixel(&image, 0, 0), [128, 64, 0, 255]);
    }

    #[test]
    fn an_hls_register_is_converted_with_the_dec_hue_origin() {
        let blue = decode(&sixel("\"1;1;1;6#1;1;0;50;100@")).unwrap();
        assert_eq!(pixel(&blue, 0, 0), [0, 0, 255, 255]);
        let red = decode(&sixel("\"1;1;1;6#1;1;120;50;100@")).unwrap();
        assert_eq!(pixel(&red, 0, 0), [255, 0, 0, 255]);
        let green = decode(&sixel("\"1;1;1;6#1;1;240;50;100@")).unwrap();
        assert_eq!(pixel(&green, 0, 0), [0, 255, 0, 255]);
    }

    #[test]
    fn an_unsaturated_hls_register_is_a_grey_of_its_lightness() {
        let image = decode(&sixel("\"1;1;1;6#1;1;0;50;0@")).unwrap();
        assert_eq!(pixel(&image, 0, 0), [128, 128, 128, 255]);
    }

    #[test]
    fn an_undefined_register_falls_back_to_the_vt340_palette() {
        let image = decode(&sixel("\"1;1;1;6#3@")).unwrap();
        assert_eq!(pixel(&image, 0, 0), [51, 204, 51, 255]);
    }

    #[test]
    fn a_register_redefined_mid_image_repaints_only_what_follows() {
        let image = decode(&sixel("\"1;1;2;6#0;2;100;0;0@#0;2;0;0;100@")).unwrap();
        assert_eq!(pixel(&image, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&image, 1, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn data_beyond_a_declared_raster_is_clipped_rather_than_growing_it() {
        let image = decode(&sixel("\"1;1;2;6#0;2;100;0;0!8@--#0;2;100;0;0@")).unwrap();
        assert_eq!((image.width, image.height), (2, 6));
    }

    #[test]
    fn an_oversized_raster_attribute_is_refused() {
        assert!(decode(&sixel("\"1;1;20000;20000#0;2;100;0;0@")).is_none());
    }

    #[test]
    fn a_repeat_past_the_dimension_limit_is_clamped_rather_than_allocated() {
        let image = decode(&sixel("#0;2;100;0;0!99999~")).unwrap();
        assert_eq!(image.width as usize, MAX_DIMENSION);
        assert_eq!(image.height, 6);
    }

    #[test]
    fn bands_past_the_dimension_limit_paint_nothing() {
        let mut body = String::from("#0;2;100;0;0@");
        body.push_str(&"-".repeat(MAX_DIMENSION / BAND + 4));
        body.push_str("#0;2;100;0;0@");
        let image = decode(&sixel(&body)).unwrap();
        assert!(image.height as usize <= MAX_DIMENSION, "{}", image.height);
    }

    #[test]
    fn an_undeclared_image_over_the_byte_budget_is_refused() {
        let mut body = format!("#0;2;100;0;0!{}~", MAX_DIMENSION);
        for _ in 0..MAX_IMAGE_BYTES / (MAX_DIMENSION * 4 * BAND) + 1 {
            body.push_str(&format!("-!{}~", MAX_DIMENSION));
        }
        assert!(decode(&sixel(&body)).is_none());
    }

    #[test]
    fn a_register_value_past_full_scale_is_clamped() {
        assert_eq!(percent(200), 255);
        assert_eq!(hls_to_rgb(0, 200, 200), [255, 255, 255]);
    }

    #[test]
    fn an_empty_image_decodes_to_nothing() {
        assert!(decode(&sixel("")).is_none());
        assert!(decode(b"").is_none());
    }

    #[test]
    fn a_malformed_payload_is_refused_rather_than_panicking() {
        for payload in [
            &b"\x1bP0;1;0q#not a register@\x1b\\"[..],
            &b"\x1bP99999999;1;0q@\x1b\\"[..],
            &b"\x1bP0;1;0q\"1;1;;;;;#\x1b\\"[..],
            &b"\x1bP0;1;0q\xff\xfe\x1b\\"[..],
        ] {
            let _ = decode(payload);
        }
    }

    fn cell() -> CellSize {
        CellSize::new(8, 20)
    }

    fn store() -> Sixels {
        Sixels::new(cell())
    }

    fn place(store: &mut Sixels, cell_x: usize, cell_y: usize, body: &str) {
        store.execute(&sixel(body), (cell_y, cell_x));
        store.commit();
    }

    fn opaque(width: usize, height: usize) -> String {
        format!("\"1;1;{};{}#0;2;100;0;0!{}~", width, height, width)
    }

    fn band(width: usize, bands: usize) -> String {
        let mut body = format!("\"1;1;{};{}", width, bands * 6);
        for _ in 0..bands {
            body.push_str(&format!("#0;2;100;0;0!{}~-", width));
        }
        body
    }

    #[test]
    fn a_chunk_is_anchored_at_the_cursor_cell() {
        let mut store = store();
        place(&mut store, 3, 2, &opaque(16, 6));
        let chunk = store.chunks().next().unwrap();
        assert_eq!((chunk.cell_x, chunk.cell_y), (3, 2));
        assert_eq!((chunk.width, chunk.height), (16, 6));
        assert_eq!(
            chunk.rect(cell()),
            PixelRect {
                x: 24,
                y: 40,
                width: 16,
                height: 6
            }
        );
    }

    #[test]
    fn a_chunk_that_covers_an_older_one_replaces_it() {
        let mut store = store();
        place(&mut store, 0, 0, &band(16, 4));
        place(&mut store, 0, 0, &band(16, 4));
        assert_eq!(store.chunks().count(), 1);
    }

    #[test]
    fn a_chunk_that_covers_part_of_an_older_one_erases_only_that_part() {
        let mut store = store();
        place(&mut store, 0, 0, &band(16, 7));
        place(&mut store, 0, 1, &band(16, 1));

        let chunks: Vec<&Chunk> = store.chunks().collect();
        assert_eq!(chunks.len(), 2);
        let older = chunks[0];
        assert_eq!(pixel(&older.image, 0, 19), [255, 0, 0, 255]);
        assert_eq!(pixel(&older.image, 0, 20), [0, 0, 0, 0]);
        assert_eq!(pixel(&older.image, 0, 26), [255, 0, 0, 255]);
    }

    #[test]
    fn an_erase_bumps_the_revision_so_the_texture_is_reuploaded() {
        let mut store = store();
        place(&mut store, 0, 0, &band(16, 7));
        let before = store.chunks().next().unwrap().revision;
        place(&mut store, 0, 1, &band(16, 1));
        let after = store.chunks().next().unwrap().revision;
        assert!(after > before, "{} is not newer than {}", after, before);
    }

    #[test]
    fn a_damaged_line_erases_the_band_it_covers() {
        let mut store = store();
        place(&mut store, 0, 0, &band(16, 7));
        store.cut_spans(&[Span::new(1, 0, 7)]);

        let chunk = store.chunks().next().unwrap();
        assert_eq!(pixel(&chunk.image, 0, 19), [255, 0, 0, 255]);
        assert_eq!(pixel(&chunk.image, 0, 20), [0, 0, 0, 0]);
        assert_eq!(pixel(&chunk.image, 0, 39), [0, 0, 0, 0]);
        assert_eq!(pixel(&chunk.image, 0, 40), [255, 0, 0, 255]);
    }

    #[test]
    fn a_line_outside_the_chunk_leaves_it_alone() {
        let mut store = store();
        place(&mut store, 0, 0, &band(16, 3));
        let revision = store.chunks().next().unwrap().revision;
        store.cut_spans(&[Span::new(4, 0, 7), Span::new(5, 0, 7)]);
        assert_eq!(store.chunks().next().unwrap().revision, revision);
    }

    #[test]
    fn a_chunk_erased_to_nothing_is_dropped() {
        let mut store = store();
        place(&mut store, 0, 0, &band(16, 7));
        store.cut_spans(&[Span::new(0, 0, 7)]);
        assert!(store.chunks().next().is_some());
        store.cut_spans(&[Span::new(1, 0, 7), Span::new(2, 0, 7)]);
        assert!(store.is_empty(), "an emptied chunk stayed resident");
    }

    #[test]
    fn a_transparent_chunk_is_dropped_by_the_lines_it_covers() {
        let mut store = store();
        place(&mut store, 0, 0, "\"1;1;16;20#0;2;100;0;0@");
        store.cut_spans(&[Span::new(0, 0, 1)]);
        assert!(store.is_empty());
    }

    #[test]
    fn residency_is_reported_in_ascending_order() {
        let mut store = store();
        for row in [4, 0, 2] {
            place(&mut store, 0, row, &band(16, 1));
        }
        let ids = store.resident_ids();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn a_cell_size_change_retires_the_chunks_positioned_against_the_old_one() {
        let mut store = store();
        place(&mut store, 0, 0, &band(16, 1));
        store.set_cell_size(CellSize::new(8, 20));
        assert_eq!(store.chunks().count(), 1);
        store.set_cell_size(CellSize::new(16, 40));
        assert!(store.is_empty());
    }

    #[test]
    fn a_clear_drops_committed_and_pending_chunks_alike() {
        let mut store = store();
        place(&mut store, 0, 0, &band(16, 1));
        store.execute(&sixel(&band(16, 1)), (2, 0));
        store.clear();
        store.commit();
        assert!(store.is_empty());
    }

    #[test]
    fn an_undecodable_payload_commits_nothing() {
        let mut store = store();
        store.execute(b"\x1bP0;1;0q\x1b\\", (0, 0));
        store.commit();
        assert!(store.is_empty());
    }

    #[test]
    fn the_chunk_budget_retires_the_oldest_chunks() {
        let mut store = store();
        for row in 0..MAX_CHUNKS + 8 {
            place(&mut store, row % 4, row, &band(4, 1));
        }
        assert_eq!(store.chunks().count(), MAX_CHUNKS);
        assert_eq!(store.resident_ids()[0], 9);
    }

    #[test]
    fn a_zero_cell_dimension_is_clamped_rather_than_dividing_by_it() {
        let cell = CellSize::new(0, 0);
        assert_eq!(cell, CellSize::new(1, 1));
    }
}

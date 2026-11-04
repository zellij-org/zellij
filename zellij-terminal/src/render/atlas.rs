use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use crate::render::sprites;
use crate::text::font::{
    CellMetrics, ClusterRaster, FaceStyle, FontId, FontStack, Glyph, GlyphBitmap, GlyphContent,
    GlyphId, ShapedGlyph,
};

const ATLAS_WIDTH: u32 = 4096;
const INITIAL_HEIGHT: u32 = 256;
const MAX_HEIGHT: u32 = 4096;
const PADDING: u32 = 1;
const MASK_BUDGET_BYTES: usize = 16 << 20;
const COLOR_BUDGET_BYTES: usize = 16 << 20;
const MAX_DIRTY_LOG: usize = 4096;
const OVERFLOW_RETRY: Duration = Duration::from_secs(1);
const MAX_ENTRIES: usize = 32_768;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RasterKey {
    pub font: FontId,
    pub glyph: GlyphId,
    pub budget: u32,
    pub spill: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GlyphKey {
    Char(char),
    Text(Arc<str>),
}

impl GlyphKey {
    pub fn of(text: &str) -> Self {
        let mut characters = text.chars();
        match (characters.next(), characters.next()) {
            (Some(only), None) => GlyphKey::Char(only),
            _ => GlyphKey::Text(Arc::from(text)),
        }
    }

    pub fn shared(text: &Arc<str>) -> Self {
        let mut characters = text.chars();
        match (characters.next(), characters.next()) {
            (Some(only), None) => GlyphKey::Char(only),
            _ => GlyphKey::Text(Arc::clone(text)),
        }
    }

    fn text(&self) -> std::borrow::Cow<'_, str> {
        match self {
            GlyphKey::Char(character) => std::borrow::Cow::Owned(character.to_string()),
            GlyphKey::Text(text) => std::borrow::Cow::Borrowed(text),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterLookup {
    Sprite(RenderedGlyph),
    Blank,
    PerCell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasEntry {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderedGlyph {
    pub entry: AtlasEntry,
    pub content: GlyphContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    Rendered(RenderedGlyph),
    Blank,
    Missing,
}

pub struct GlyphAtlas {
    side: u32,
    height: u32,
    initial_height: u32,
    channels: u32,
    budget: usize,
    coverage: Vec<u8>,
    shelf_top: u32,
    shelf_height: u32,
    shelf_cursor: u32,
    identity: u64,
    revision: u64,
    reshaped_at: u64,
    dirty_log: Vec<(u64, u32, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasStamp {
    pub identity: u64,
    pub revision: u64,
}

fn next_revision() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static REVISIONS: AtomicU64 = AtomicU64::new(1);
    REVISIONS.fetch_add(1, Ordering::Relaxed)
}

impl GlyphAtlas {
    pub fn new(channels: u32) -> Self {
        let budget = if channels == 1 {
            MASK_BUDGET_BYTES
        } else {
            COLOR_BUDGET_BYTES
        };
        let mut atlas = Self::sized(channels, ATLAS_WIDTH, INITIAL_HEIGHT, budget);
        if channels != 1 {
            atlas.release();
        }
        atlas
    }

    fn sized(channels: u32, side: u32, height: u32, budget: usize) -> Self {
        let revision = next_revision();
        Self {
            side,
            height,
            initial_height: height,
            channels,
            budget,
            coverage: vec![0; (side * height * channels) as usize],
            shelf_top: PADDING,
            shelf_height: 0,
            shelf_cursor: PADDING,
            identity: revision,
            revision,
            reshaped_at: revision,
            dirty_log: Vec::new(),
        }
    }

    fn renewed(&self) -> Self {
        let mut renewed = Self::sized(self.channels, self.side, self.initial_height, self.budget);
        if self.height == 0 {
            renewed.release();
        }
        renewed
    }

    fn release(&mut self) {
        self.height = 0;
        self.coverage = Vec::new();
    }

    fn allocate(&mut self) {
        self.height = self.initial_height;
        self.coverage = vec![0; (self.side * self.height * self.channels) as usize];
        self.revision = next_revision();
        self.reshaped_at = self.revision;
        self.dirty_log.clear();
    }

    pub fn dirty_rows_since(&self, revision: u64) -> Option<(u32, u32)> {
        if revision < self.reshaped_at {
            return None;
        }
        let mut rows: Option<(u32, u32)> = None;
        for (changed_at, top, bottom) in self.dirty_log.iter().rev() {
            if *changed_at <= revision {
                break;
            }
            rows = Some(match rows {
                Some((low, high)) => (low.min(*top), high.max(*bottom)),
                None => (*top, *bottom),
            });
        }
        Some(rows.unwrap_or((0, 0)))
    }

    pub fn width(&self) -> u32 {
        self.side
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn channels(&self) -> u32 {
        self.channels
    }

    pub fn coverage(&self) -> &[u8] {
        &self.coverage
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn stamp(&self) -> AtlasStamp {
        AtlasStamp {
            identity: self.identity,
            revision: self.revision,
        }
    }

    fn fits_width(&self, width: u32) -> bool {
        width + PADDING * 2 <= self.side
    }

    pub fn insert(&mut self, bitmap: &GlyphBitmap) -> Result<AtlasEntry> {
        if bitmap.content.channels() != self.channels {
            return Err(anyhow!(
                "a {}-channel glyph does not belong in a {}-channel atlas",
                bitmap.content.channels(),
                self.channels
            ));
        }
        if !self.fits_width(bitmap.width) {
            return Err(anyhow!(
                "glyph of {} px is wider than the {} px atlas",
                bitmap.width,
                self.side
            ));
        }

        if self.height == 0 {
            self.allocate();
        }

        if self.shelf_cursor + bitmap.width + PADDING > self.side {
            self.shelf_top += self.shelf_height + PADDING;
            self.shelf_height = 0;
            self.shelf_cursor = PADDING;
        }

        while self.shelf_top + bitmap.height.max(self.shelf_height) + PADDING > self.height {
            self.grow()?;
        }

        let entry = AtlasEntry {
            x: self.shelf_cursor,
            y: self.shelf_top,
            width: bitmap.width,
            height: bitmap.height,
            left: bitmap.left,
            top: bitmap.top,
        };

        let span = (bitmap.width * self.channels) as usize;
        for row in 0..bitmap.height {
            let source = (row * bitmap.width * self.channels) as usize;
            let target = ((entry.y + row) * self.side + entry.x) as usize * self.channels as usize;
            self.coverage[target..target + span]
                .copy_from_slice(&bitmap.coverage[source..source + span]);
        }

        self.shelf_cursor += bitmap.width + PADDING;
        self.shelf_height = self.shelf_height.max(bitmap.height);
        self.revision = next_revision();
        if self.dirty_log.len() >= MAX_DIRTY_LOG {
            self.dirty_log.clear();
            self.reshaped_at = self.revision;
        } else {
            self.dirty_log
                .push((self.revision, entry.y, entry.y + entry.height));
        }

        Ok(entry)
    }

    fn grow(&mut self) -> Result<()> {
        let grown = self.height * 2;
        if grown > MAX_HEIGHT {
            return Err(anyhow!("glyph atlas exceeded {} px", MAX_HEIGHT));
        }
        let grown_bytes = (self.side * grown * self.channels) as usize;
        if grown_bytes > self.budget {
            return Err(anyhow!(
                "a {} byte glyph atlas exceeds the {} byte budget",
                grown_bytes,
                self.budget
            ));
        }
        self.height = grown;
        self.coverage
            .resize((self.side * self.height * self.channels) as usize, 0);
        self.revision = next_revision();
        self.reshaped_at = self.revision;
        self.dirty_log.clear();
        Ok(())
    }
}

pub struct Atlases<'a> {
    pub mask: &'a GlyphAtlas,
    pub color: &'a GlyphAtlas,
}

pub struct GlyphCache {
    fonts: FontStack,
    mask: GlyphAtlas,
    color: GlyphAtlas,
    entries: HashMap<RasterKey, Option<RenderedGlyph>>,
    clusters: HashMap<(FaceStyle, GlyphKey, u32), ClusterLookup>,
    generation: u64,
    exhausted: bool,
    overflowed_at: Option<Instant>,
    flushes: u64,
}

enum Insertion {
    Placed(RenderedGlyph),
    OutOfRoom,
    Impossible,
}

impl GlyphCache {
    pub fn new(fonts: FontStack) -> Self {
        Self::bounded(fonts, GlyphAtlas::new(1), GlyphAtlas::new(4))
    }

    fn bounded(fonts: FontStack, mask: GlyphAtlas, color: GlyphAtlas) -> Self {
        Self {
            fonts,
            mask,
            color,
            entries: HashMap::new(),
            clusters: HashMap::new(),
            generation: next_revision(),
            exhausted: false,
            overflowed_at: None,
            flushes: 0,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn exhausted(&self) -> bool {
        self.exhausted
    }

    pub fn may_flush(&self, now: Instant) -> bool {
        self.overflowed_at
            .is_none_or(|overflowed_at| now.duration_since(overflowed_at) >= OVERFLOW_RETRY)
    }

    pub fn note_overflow(&mut self, now: Instant) {
        if self.overflowed_at.is_none() {
            report!(
                "the screen holds more distinct glyphs than the glyph atlas can; \
                 some will be drawn blank until it changes"
            );
        }
        self.overflowed_at = Some(now);
    }

    pub fn settled(&mut self) {
        if !self.exhausted {
            self.overflowed_at = None;
        }
    }

    pub fn flush(&mut self) {
        self.mask = self.mask.renewed();
        self.color = self.color.renewed();
        self.entries.clear();
        self.clusters.clear();
        self.generation = next_revision();
        self.exhausted = false;
        self.flushes += 1;
    }

    pub fn metrics(&self) -> CellMetrics {
        self.fonts.metrics()
    }

    pub fn underline_stroke(&self) -> u32 {
        self.fonts.placement().underline_stroke
    }

    pub fn atlases(&self) -> Atlases<'_> {
        Atlases {
            mask: &self.mask,
            color: &self.color,
        }
    }

    pub fn shapes_runs(&self) -> bool {
        self.fonts.shapes_runs()
    }

    pub fn primary_of(&self, style: FaceStyle) -> FontId {
        self.fonts.primary_of(style)
    }

    pub fn face_of(&mut self, style: FaceStyle, character: char) -> Option<FontId> {
        self.fonts.lookup(style, character).map(|glyph| glyph.font)
    }

    pub fn shape(&mut self, font: FontId, text: &str) -> Rc<[ShapedGlyph]> {
        self.fonts.shape(font, text)
    }

    pub fn cluster_glyph(
        &mut self,
        style: FaceStyle,
        key: &GlyphKey,
        budget: u32,
    ) -> ClusterLookup {
        let cache_key = (style, key.clone(), budget);
        if let Some(cached) = self.clusters.get(&cache_key) {
            return *cached;
        }
        if self.exhausted {
            return ClusterLookup::Blank;
        }
        let raster = self.fonts.rasterize_cluster(style, &key.text(), budget);
        let lookup = match raster {
            ClusterRaster::Sprite(bitmap) => match self.insert(&bitmap) {
                Insertion::Placed(rendered) => ClusterLookup::Sprite(rendered),
                Insertion::OutOfRoom => {
                    self.exhausted = true;
                    return ClusterLookup::Blank;
                },
                Insertion::Impossible => ClusterLookup::PerCell,
            },
            ClusterRaster::Blank => ClusterLookup::Blank,
            ClusterRaster::Uncovered => ClusterLookup::PerCell,
        };
        self.clusters.insert(cache_key, lookup);
        if self.entries.len() + self.clusters.len() >= MAX_ENTRIES {
            self.exhausted = true;
        }
        lookup
    }

    pub fn shaped_glyph(&mut self, font: FontId, glyph: GlyphId, budget: u32) -> Lookup {
        self.rendered(Glyph { font, glyph }, budget)
    }

    pub fn glyph_with_room(
        &mut self,
        style: FaceStyle,
        character: char,
        budget: u32,
        spill: u32,
    ) -> Lookup {
        if sprites::is_sprite(character) {
            return self.sprite(character, budget);
        }
        let Some(glyph) = self.fonts.lookup(style, character) else {
            return Lookup::Missing;
        };
        let spill = if self.fonts.is_primary(glyph.font) {
            0
        } else {
            spill
        };
        self.rendered_with_room(glyph, budget, spill)
    }

    fn sprite(&mut self, character: char, budget: u32) -> Lookup {
        let Ok(glyph) = GlyphId::try_from(character as u32) else {
            return Lookup::Missing;
        };
        let key = RasterKey {
            font: FontId::SPRITES,
            glyph,
            budget,
            spill: 0,
        };
        if let Some(cached) = self.entries.get(&key) {
            return match cached {
                Some(rendered) => Lookup::Rendered(*rendered),
                None => Lookup::Blank,
            };
        }
        if self.exhausted {
            return Lookup::Blank;
        }
        let Some(bitmap) = sprites::draw(character, self.fonts.metrics(), budget) else {
            return Lookup::Missing;
        };
        self.place(key, &bitmap)
    }

    fn rendered(&mut self, glyph: Glyph, budget: u32) -> Lookup {
        self.rendered_with_room(glyph, budget, 0)
    }

    fn rendered_with_room(&mut self, glyph: Glyph, budget: u32, spill: u32) -> Lookup {
        let key = RasterKey {
            font: glyph.font,
            glyph: glyph.glyph,
            budget,
            spill,
        };
        if let Some(cached) = self.entries.get(&key) {
            return match cached {
                Some(rendered) => Lookup::Rendered(*rendered),
                None => Lookup::Blank,
            };
        }
        if self.exhausted {
            return Lookup::Blank;
        }

        let Some(bitmap) = self.fonts.rasterize_with_room(glyph, budget, spill) else {
            self.remember(key, None);
            return Lookup::Blank;
        };
        self.place(key, &bitmap)
    }

    fn place(&mut self, key: RasterKey, bitmap: &GlyphBitmap) -> Lookup {
        match self.insert(bitmap) {
            Insertion::Placed(rendered) => {
                self.remember(key, Some(rendered));
                Lookup::Rendered(rendered)
            },
            Insertion::OutOfRoom => {
                self.exhausted = true;
                Lookup::Blank
            },
            Insertion::Impossible => {
                self.remember(key, None);
                Lookup::Blank
            },
        }
    }

    fn remember(&mut self, key: RasterKey, rendered: Option<RenderedGlyph>) {
        self.entries.insert(key, rendered);
        if self.entries.len() + self.clusters.len() >= MAX_ENTRIES {
            self.exhausted = true;
        }
    }

    fn insert(&mut self, bitmap: &GlyphBitmap) -> Insertion {
        let atlas = match bitmap.content {
            GlyphContent::Mask => &mut self.mask,
            GlyphContent::Color => &mut self.color,
        };
        let width_fits = atlas.fits_width(bitmap.width);
        match atlas.insert(bitmap) {
            Ok(entry) => Insertion::Placed(RenderedGlyph {
                entry,
                content: bitmap.content,
            }),
            Err(_) if width_fits => Insertion::OutOfRoom,
            Err(e) => {
                report!("glyph atlas insertion failed: {}", e);
                Insertion::Impossible
            },
        }
    }
}

#[cfg(test)]
pub(crate) mod test_helpers;

#[cfg(test)]
mod tests;

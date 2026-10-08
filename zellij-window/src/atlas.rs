use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};

use crate::font::{
    CellMetrics, FaceStyle, FontId, FontStack, Glyph, GlyphBitmap, GlyphContent, GlyphId,
    ShapedGlyph,
};
use crate::sprites;

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
pub struct GlyphKey {
    pub font: FontId,
    pub glyph: GlyphId,
    pub budget: u32,
    pub spill: u32,
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
    revision: u64,
    reshaped_at: u64,
    dirty_log: Vec<(u64, u32, u32)>,
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
        Self::sized(channels, ATLAS_WIDTH, INITIAL_HEIGHT, budget)
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
            revision,
            reshaped_at: revision,
            dirty_log: Vec::new(),
        }
    }

    fn renewed(&self) -> Self {
        Self::sized(self.channels, self.side, self.initial_height, self.budget)
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

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.coverage.len()
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
    entries: HashMap<GlyphKey, Option<RenderedGlyph>>,
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

    #[cfg(test)]
    pub fn tiny(fonts: FontStack) -> Self {
        const SIDE: u32 = 64;
        Self::bounded(
            fonts,
            GlyphAtlas::sized(1, SIDE, SIDE, (SIDE * SIDE) as usize),
            GlyphAtlas::sized(4, SIDE, SIDE, (SIDE * SIDE * 4) as usize),
        )
    }

    fn bounded(fonts: FontStack, mask: GlyphAtlas, color: GlyphAtlas) -> Self {
        Self {
            fonts,
            mask,
            color,
            entries: HashMap::new(),
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

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.mask.bytes() + self.color.bytes()
    }

    #[cfg(test)]
    pub fn entries(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn flushes(&self) -> u64 {
        self.flushes
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

    #[cfg(test)]
    pub fn nominal_of(&mut self, style: FaceStyle, character: char) -> Option<GlyphId> {
        self.fonts.lookup(style, character).map(|glyph| glyph.glyph)
    }

    pub fn shape(&mut self, font: FontId, text: &str) -> Rc<[ShapedGlyph]> {
        self.fonts.shape(font, text)
    }

    #[cfg(test)]
    pub fn shaped_runs(&self) -> usize {
        self.fonts.shaped_runs()
    }

    pub fn shaped_glyph(&mut self, font: FontId, glyph: GlyphId, budget: u32) -> Lookup {
        self.rendered(Glyph { font, glyph }, budget)
    }

    pub fn glyph(&mut self, style: FaceStyle, character: char, budget: u32) -> Lookup {
        self.glyph_with_room(style, character, budget, 0)
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
        let key = GlyphKey {
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
        let key = GlyphKey {
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

    fn place(&mut self, key: GlyphKey, bitmap: &GlyphBitmap) -> Lookup {
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

    fn remember(&mut self, key: GlyphKey, rendered: Option<RenderedGlyph>) {
        self.entries.insert(key, rendered);
        if self.entries.len() >= MAX_ENTRIES {
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
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT_SIZE;

    fn cache() -> GlyphCache {
        GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap())
    }

    fn bitmap(width: u32, height: u32, fill: u8) -> GlyphBitmap {
        GlyphBitmap {
            left: 0,
            top: 0,
            width,
            height,
            coverage: vec![fill; (width * height) as usize],
            content: GlyphContent::Mask,
        }
    }

    fn color_bitmap(width: u32, height: u32, fill: u8) -> GlyphBitmap {
        GlyphBitmap {
            left: 0,
            top: 0,
            width,
            height,
            coverage: vec![fill; (width * height * 4) as usize],
            content: GlyphContent::Color,
        }
    }

    fn read(atlas: &GlyphAtlas, entry: AtlasEntry) -> Vec<u8> {
        let channels = atlas.channels() as usize;
        (0..entry.height)
            .flat_map(|row| {
                let start = ((entry.y + row) * atlas.width() + entry.x) as usize * channels;
                atlas.coverage()[start..start + entry.width as usize * channels].to_vec()
            })
            .collect()
    }

    fn rendered(lookup: Lookup) -> RenderedGlyph {
        match lookup {
            Lookup::Rendered(rendered) => rendered,
            other => panic!("expected a rendered glyph, got {:?}", other),
        }
    }

    #[test]
    fn inserted_coverage_is_readable_at_the_reported_position() {
        let mut atlas = GlyphAtlas::new(1);
        let entry = atlas.insert(&bitmap(3, 2, 77)).unwrap();
        assert_eq!(read(&atlas, entry), vec![77; 6]);
    }

    #[test]
    fn a_color_atlas_stores_four_bytes_per_pixel() {
        let mut atlas = GlyphAtlas::new(4);
        let entry = atlas.insert(&color_bitmap(2, 2, 9)).unwrap();
        assert_eq!(read(&atlas, entry), vec![9; 16]);
    }

    #[test]
    fn an_atlas_refuses_glyphs_of_the_wrong_channel_count() {
        assert!(GlyphAtlas::new(4).insert(&bitmap(2, 2, 1)).is_err());
        assert!(GlyphAtlas::new(1).insert(&color_bitmap(2, 2, 1)).is_err());
    }

    #[test]
    fn entries_do_not_overlap() {
        let mut atlas = GlyphAtlas::new(1);
        let first = atlas.insert(&bitmap(4, 4, 1)).unwrap();
        let second = atlas.insert(&bitmap(4, 4, 2)).unwrap();
        assert_ne!((first.x, first.y), (second.x, second.y));
        assert_eq!(read(&atlas, first), vec![1; 16]);
        assert_eq!(read(&atlas, second), vec![2; 16]);
    }

    #[test]
    fn a_full_shelf_wraps_onto_the_next_one() {
        let mut atlas = GlyphAtlas::new(1);
        let mut previous = atlas.insert(&bitmap(8, 8, 1)).unwrap();
        let mut wrapped = false;
        for _ in 0..(ATLAS_WIDTH / 8) {
            let entry = atlas.insert(&bitmap(8, 8, 1)).unwrap();
            if entry.y > previous.y {
                wrapped = true;
                break;
            }
            previous = entry;
        }
        assert!(wrapped, "the atlas never opened a second shelf");
    }

    #[test]
    fn the_atlas_grows_in_height_without_moving_earlier_entries() {
        let mut atlas = GlyphAtlas::new(1);
        let first = atlas.insert(&bitmap(4, 4, 42)).unwrap();
        for _ in 0..5000 {
            atlas.insert(&bitmap(16, 16, 7)).unwrap();
        }
        assert!(atlas.height() > INITIAL_HEIGHT);
        assert_eq!(read(&atlas, first), vec![42; 16]);
    }

    #[test]
    fn a_glyph_wider_than_the_atlas_is_rejected() {
        let mut atlas = GlyphAtlas::new(1);
        assert!(atlas.insert(&bitmap(ATLAS_WIDTH, 1, 0)).is_err());
    }

    #[test]
    fn the_revision_advances_only_when_the_atlas_changes() {
        let mut cache = cache();
        let before = cache.atlases().mask.revision();
        rendered(cache.glyph(FaceStyle::Regular, 'x', 1));
        let after = cache.atlases().mask.revision();
        assert!(after > before);
        rendered(cache.glyph(FaceStyle::Regular, 'x', 1));
        assert_eq!(cache.atlases().mask.revision(), after);
    }

    #[test]
    fn a_repeated_lookup_returns_the_same_entry() {
        let mut cache = cache();
        let first = rendered(cache.glyph(FaceStyle::Regular, 'W', 1));
        let second = rendered(cache.glyph(FaceStyle::Regular, 'W', 1));
        assert_eq!(first, second);
    }

    #[test]
    fn styles_are_cached_independently() {
        let mut cache = cache();
        let regular = rendered(cache.glyph(FaceStyle::Regular, 'A', 1)).entry;
        let bold = rendered(cache.glyph(FaceStyle::Bold, 'A', 1)).entry;
        assert_ne!((regular.x, regular.y), (bold.x, bold.y));
    }

    #[test]
    fn budgets_are_cached_independently() {
        let mut cache = cache();
        let wide = rendered(cache.glyph(FaceStyle::Regular, '\u{4f60}', 2)).entry;
        let narrow = rendered(cache.glyph(FaceStyle::Regular, '\u{4f60}', 1)).entry;
        assert_ne!((wide.x, wide.y), (narrow.x, narrow.y));
        assert!(narrow.width < wide.width);
    }

    #[test]
    fn block_and_box_characters_are_drawn_to_fill_the_cell() {
        let mut cache = cache();
        let metrics = cache.metrics();
        for character in ['\u{2588}', '\u{2500}', '\u{2502}', '\u{e0b0}'] {
            let sprite = rendered(cache.glyph(FaceStyle::Regular, character, 1));
            assert_eq!(sprite.content, GlyphContent::Mask);
            assert_eq!(
                (sprite.entry.width, sprite.entry.height),
                (metrics.width, metrics.height),
                "{:?}",
                character
            );
            assert_eq!(
                (sprite.entry.left, sprite.entry.top),
                (0, metrics.baseline as i32)
            );
        }
        let block = rendered(cache.glyph(FaceStyle::Bold, '\u{2588}', 1));
        assert!(read(&cache.mask, block.entry)
            .iter()
            .all(|alpha| *alpha == 255));
    }

    #[test]
    fn a_blank_character_is_distinguished_from_a_missing_one() {
        let mut cache = cache();
        assert_eq!(cache.glyph(FaceStyle::Regular, ' ', 1), Lookup::Blank);
        assert_eq!(
            cache.glyph(FaceStyle::Regular, '\u{10fffd}', 1),
            Lookup::Missing
        );
    }

    #[test]
    fn color_glyphs_land_in_the_color_atlas_alone() {
        let mut cache = cache();
        let before = cache.atlases().mask.revision();
        let emoji = rendered(cache.glyph(FaceStyle::Regular, '\u{1f600}', 2));
        assert_eq!(emoji.content, GlyphContent::Color);
        assert_eq!(cache.atlases().mask.revision(), before);
        assert!(cache.atlases().color.revision() > 0);
    }

    #[test]
    fn a_fallback_glyph_is_a_mask_when_the_face_has_no_color() {
        let mut cache = cache();
        let cjk = rendered(cache.glyph(FaceStyle::Regular, '\u{4f60}', 2));
        assert_eq!(cjk.content, GlyphContent::Mask);
    }

    fn tiny() -> GlyphCache {
        GlyphCache::tiny(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap())
    }

    fn letters() -> impl Iterator<Item = char> {
        (b'!'..=b'~').map(char::from)
    }

    fn fill_until_exhausted(cache: &mut GlyphCache) -> char {
        for letter in letters() {
            cache.glyph(FaceStyle::Regular, letter, 1);
            if cache.exhausted() {
                return letter;
            }
        }
        panic!("a 64 px atlas swallowed every printable ASCII glyph");
    }

    #[test]
    fn an_atlas_that_cannot_grow_past_its_budget_says_so_instead_of_growing() {
        let mut atlas = GlyphAtlas::sized(1, 64, 64, (64 * 64) as usize);
        let mut refused = false;
        for _ in 0..1000 {
            if atlas.insert(&bitmap(16, 16, 3)).is_err() {
                refused = true;
                break;
            }
        }
        assert!(refused, "the atlas grew past the bytes it was allowed");
        assert_eq!(atlas.bytes(), (64 * 64) as usize);
    }

    #[test]
    fn a_full_atlas_reports_itself_exhausted_rather_than_remembering_a_blank() {
        let mut cache = tiny();
        let lost = fill_until_exhausted(&mut cache);
        assert!(cache.exhausted());
        cache.flush();
        assert!(!cache.exhausted());
        let placed = rendered(cache.glyph(FaceStyle::Regular, lost, 1));
        assert!(
            placed.entry.width > 0,
            "the glyph the full atlas turned away must be rasterized after a flush"
        );
    }

    #[test]
    fn a_flush_renews_both_atlases_with_revisions_no_texture_has_seen() {
        let mut cache = tiny();
        fill_until_exhausted(&mut cache);
        let (mask, color, generation, bytes) = (
            cache.atlases().mask.revision(),
            cache.atlases().color.revision(),
            cache.generation(),
            cache.bytes(),
        );
        cache.flush();
        assert!(cache.atlases().mask.revision() > mask);
        assert!(cache.atlases().color.revision() > color);
        assert!(cache.generation() > generation);
        assert_eq!(cache.entries(), 0);
        assert_eq!(
            cache.bytes(),
            bytes,
            "a renewed atlas keeps the geometry it started with"
        );
        assert_eq!(cache.flushes(), 1);
    }

    #[test]
    fn the_cache_stays_inside_its_budget_however_much_text_it_is_shown() {
        let mut cache = tiny();
        let ceiling = cache.bytes();
        for round in 0..40 {
            for letter in letters() {
                cache.glyph(FaceStyle::Regular, letter, 1);
                cache.glyph(FaceStyle::Bold, letter, 1);
                if cache.exhausted() {
                    cache.flush();
                }
            }
            assert_eq!(cache.bytes(), ceiling, "round {}", round);
            assert!(cache.entries() < MAX_ENTRIES, "round {}", round);
        }
    }

    #[test]
    fn a_glyph_too_wide_for_the_atlas_is_told_apart_from_one_that_merely_does_not_fit() {
        let mut cache = tiny();
        assert!(
            matches!(cache.insert(&bitmap(64, 4, 1)), Insertion::Impossible),
            "a glyph wider than the atlas will not fit after a flush either"
        );
        let mut out_of_room = false;
        for _ in 0..100 {
            if matches!(cache.insert(&bitmap(24, 24, 1)), Insertion::OutOfRoom) {
                out_of_room = true;
                break;
            }
        }
        assert!(out_of_room, "the tiny atlas never reported itself full");
    }

    #[test]
    fn only_the_rows_touched_since_an_upload_are_reported_dirty() {
        let mut atlas = GlyphAtlas::new(1);
        let uploaded = atlas.revision();
        assert_eq!(atlas.dirty_rows_since(uploaded), Some((0, 0)));
        let first = atlas.insert(&bitmap(4, 6, 1)).unwrap();
        assert_eq!(
            atlas.dirty_rows_since(uploaded),
            Some((first.y, first.y + 6))
        );
        let seen = atlas.revision();
        for _ in 0..2000 {
            atlas.insert(&bitmap(8, 8, 1)).unwrap();
        }
        let (top, bottom) = atlas.dirty_rows_since(seen).unwrap();
        assert!(top >= first.y && bottom > top);
        assert_eq!(
            atlas.dirty_rows_since(uploaded),
            atlas.dirty_rows_since(uploaded),
        );
    }

    #[test]
    fn growth_forces_a_whole_upload() {
        let mut atlas = GlyphAtlas::new(1);
        let uploaded = atlas.revision();
        while atlas.height() == INITIAL_HEIGHT {
            atlas.insert(&bitmap(64, 64, 1)).unwrap();
        }
        assert_eq!(atlas.dirty_rows_since(uploaded), None);
    }

    #[test]
    fn an_exhausted_cache_stops_rasterizing_until_it_is_flushed() {
        let mut cache = tiny();
        fill_until_exhausted(&mut cache);
        let entries = cache.entries();
        for letter in letters() {
            cache.glyph(FaceStyle::Bold, letter, 1);
        }
        assert_eq!(cache.entries(), entries);
    }

    #[test]
    fn an_overflowing_screen_is_not_flushed_again_straight_away() {
        let mut cache = tiny();
        let now = Instant::now();
        assert!(cache.may_flush(now));
        fill_until_exhausted(&mut cache);
        cache.note_overflow(now);
        assert!(!cache.may_flush(now));
        assert!(cache.may_flush(now + OVERFLOW_RETRY));
        cache.flush();
        cache.settled();
        assert!(cache.may_flush(now));
    }

    fn every_nerd_font_icon() -> impl Iterator<Item = char> {
        ('\u{e000}'..='\u{f8ff}').chain('\u{f0000}'..='\u{f1af0}')
    }

    #[test]
    fn every_nerd_font_icon_fits_in_the_atlas_at_once_at_normal_and_doubled_sizes() {
        for size in [DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE * 2.0] {
            let mut cache = GlyphCache::new(FontStack::embedded(size).unwrap());
            let mut drawn = 0;
            for icon in every_nerd_font_icon() {
                if matches!(
                    cache.glyph_with_room(FaceStyle::Regular, icon, 1, 1),
                    Lookup::Rendered(_)
                ) {
                    drawn += 1;
                }
                assert!(
                    !cache.exhausted(),
                    "the atlas filled up at {:?} after {} icons at size {}",
                    icon,
                    drawn,
                    size
                );
            }
            assert!(
                drawn > 10_000,
                "only {} icons were drawn at size {}",
                drawn,
                size
            );
        }
    }

    #[test]
    fn a_warm_cache_rasterizes_nothing_new_for_a_screen_it_has_seen() {
        let mut cache = cache();
        let icons: Vec<char> = every_nerd_font_icon().take(2000).collect();
        for icon in &icons {
            cache.glyph_with_room(FaceStyle::Regular, *icon, 1, 1);
        }
        let (entries, mask, color) = (
            cache.entries(),
            cache.atlases().mask.revision(),
            cache.atlases().color.revision(),
        );
        for icon in &icons {
            cache.glyph_with_room(FaceStyle::Regular, *icon, 1, 1);
        }
        assert_eq!(cache.entries(), entries);
        assert_eq!(cache.atlases().mask.revision(), mask);
        assert_eq!(cache.atlases().color.revision(), color);
    }

    #[test]
    fn the_entry_table_cannot_grow_without_bound() {
        let mut cache = cache();
        let font = cache.primary_of(FaceStyle::Regular);
        for glyph in 0..MAX_ENTRIES as GlyphId {
            cache.remember(
                GlyphKey {
                    font,
                    glyph,
                    budget: 1,
                    spill: 0,
                },
                None,
            );
        }
        assert!(cache.exhausted());
        cache.flush();
        assert_eq!(cache.entries(), 0);
    }
}

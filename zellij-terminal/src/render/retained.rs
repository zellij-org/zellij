use crate::emulation::color::{Contrast, Srgb};
use crate::emulation::damage::Damage;
use crate::emulation::selection::Selection;
use crate::emulation::terminal::{GraphicsStamp, TerminalState};
use crate::input::links::LinkRun;
use crate::render::atlas::GlyphCache;
use crate::render::scene::SceneInputs;
use crate::render::scene::{
    self, CursorOptions, HeldOut, ImageKey, ImageQuad, PixelRect, RowContext, RowScene, RowScratch,
    Transparency,
};
use crate::text::font::CellMetrics;
use std::time::Instant;

fn next_identity() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static IDENTITIES: AtomicU64 = AtomicU64::new(1);
    IDENTITIES.fetch_add(1, Ordering::Relaxed)
}

pub struct RetainedScene {
    identity: u64,
    width: u32,
    height: u32,
    clear: Srgb,
    transparency: Transparency,
    contrast: Contrast,
    rows: Vec<RowScene>,
    dirty: Vec<bool>,
    everything: bool,
    images: Vec<ImageQuad>,
    held_images: Option<Vec<ImageQuad>>,
    resident_images: Vec<ImageKey>,
    placed: Option<(GraphicsStamp, CellMetrics)>,
    replaced_images: bool,
    held: Vec<HeldOut>,
    held_changed: bool,
    selection: Option<Selection>,
    generation: u64,
    scratch: RowScratch,
    rebuilt_everything: bool,
    rebuilt_rows: Vec<usize>,
}

impl Default for RetainedScene {
    fn default() -> Self {
        Self::new()
    }
}

impl RetainedScene {
    pub fn new() -> Self {
        Self {
            identity: next_identity(),
            width: 0,
            height: 0,
            clear: [0, 0, 0],
            transparency: Transparency::OPAQUE,
            contrast: Contrast::new(1.0),
            rows: Vec::new(),
            dirty: Vec::new(),
            everything: true,
            images: Vec::new(),
            held_images: None,
            resident_images: Vec::new(),
            placed: None,
            replaced_images: true,
            held: Vec::new(),
            held_changed: false,
            selection: None,
            generation: 0,
            scratch: RowScratch::default(),
            rebuilt_everything: true,
            rebuilt_rows: Vec::new(),
        }
    }

    pub fn identity(&self) -> u64 {
        self.identity
    }

    pub fn clear(&self) -> Srgb {
        self.clear
    }

    pub fn transparency(&self) -> Transparency {
        self.transparency
    }

    pub fn set_transparency(&mut self, transparency: Transparency) {
        if self.transparency != transparency {
            self.transparency = transparency;
            self.everything = true;
        }
    }

    pub fn set_contrast(&mut self, minimum: f32) {
        if self.contrast.minimum() != minimum {
            self.contrast = Contrast::new(minimum);
            self.everything = true;
        }
    }

    pub fn forget_contrast(&mut self) {
        self.contrast = Contrast::new(self.contrast.minimum());
    }

    pub fn rows(&self) -> &[RowScene] {
        &self.rows
    }

    pub fn images(&self) -> &[ImageQuad] {
        self.held_images.as_deref().unwrap_or(&self.images)
    }

    pub fn all_images(&self) -> &[ImageQuad] {
        &self.images
    }

    pub fn contrast(&self) -> &Contrast {
        &self.contrast
    }

    pub fn set_held_out(&mut self, held: Vec<HeldOut>) {
        if held == self.held {
            return;
        }
        let spans: Vec<(usize, usize)> = self
            .held
            .iter()
            .chain(held.iter())
            .map(|held| (held.content.y, held.content.rows))
            .collect();
        for (top, rows) in spans {
            for row in top..top + rows {
                self.mark_row(row);
            }
        }
        self.held = held;
        self.held_changed = true;
    }

    pub fn set_selection(&mut self, selection: Option<Selection>) {
        if selection == self.selection {
            return;
        }
        let rows: Vec<usize> = self
            .selection
            .iter()
            .chain(selection.iter())
            .flat_map(Selection::rows)
            .collect();
        for row in rows {
            self.mark_row(row);
        }
        self.selection = selection;
    }

    pub fn resident_images(&self) -> &[ImageKey] {
        &self.resident_images
    }

    pub fn replaced_images(&self) -> bool {
        self.replaced_images
    }

    pub fn rebuilt_everything(&self) -> bool {
        self.rebuilt_everything
    }

    pub fn rebuilt_rows(&self) -> &[usize] {
        &self.rebuilt_rows
    }

    pub fn mark_everything(&mut self) {
        self.everything = true;
    }

    pub fn mark_row(&mut self, row: usize) {
        if self.everything {
            return;
        }
        match self.dirty.get_mut(row) {
            Some(flag) => *flag = true,
            None => self.everything = true,
        }
    }

    pub fn mark(&mut self, damage: &Damage) {
        match damage {
            Damage::Everything => self.mark_everything(),
            Damage::Rows(rows) => rows.iter().for_each(|row| self.mark_row(*row)),
        }
    }

    pub fn refresh(&mut self, inputs: SceneInputs<'_>, cache: &mut GlyphCache) {
        self.refresh_once(inputs, cache);
        let now = Instant::now();
        if cache.exhausted() && cache.may_flush(now) {
            cache.flush();
            self.refresh_once(inputs, cache);
            if cache.exhausted() {
                cache.note_overflow(now);
            }
        }
        cache.settled();
    }

    fn refresh_once(&mut self, inputs: SceneInputs<'_>, cache: &mut GlyphCache) {
        let state = inputs.state;
        let context = RowContext::new(inputs, cache)
            .with_transparency(self.transparency)
            .with_contrast(&self.contrast)
            .with_held_out(&self.held)
            .with_selection(self.selection.as_ref());
        let (width, height) = (context.width(), context.height());
        if self.generation != cache.generation()
            || self.rows.len() != context.size.rows
            || (self.width, self.height) != (width, height)
            || self.clear != context.clear
        {
            self.everything = true;
        }
        self.generation = cache.generation();
        self.width = width;
        self.height = height;
        self.clear = context.clear;
        self.rows.resize_with(context.size.rows, RowScene::default);
        self.dirty.resize(context.size.rows, true);

        let placed = (state.graphics_stamp(), context.metrics);
        self.replaced_images = self.placed != Some(placed);
        if self.replaced_images {
            self.images = scene::images_of(state, context.metrics);
            self.resident_images = scene::resident_images(state);
            self.placed = Some(placed);
        }
        if self.replaced_images || self.held_changed {
            self.held_images = held_images(&self.images, &self.held, context.metrics);
            self.replaced_images = true;
            self.held_changed = false;
        }

        self.rebuilt_rows.clear();
        self.rebuilt_everything = self.everything;
        for row in 0..self.rows.len() {
            if !(self.everything || self.dirty[row]) {
                continue;
            }
            scene::build_row(&context, cache, row, &mut self.scratch, &mut self.rows[row]);
            self.dirty[row] = false;
            if !self.everything {
                self.rebuilt_rows.push(row);
            }
        }
        self.everything = false;
    }
}

fn held_images(
    images: &[ImageQuad],
    held: &[HeldOut],
    metrics: CellMetrics,
) -> Option<Vec<ImageQuad>> {
    if held.is_empty() {
        return None;
    }
    let regions: Vec<PixelRect> = held.iter().flat_map(|held| held.regions(metrics)).collect();
    Some(
        images
            .iter()
            .filter(|image| {
                let area = scene::image_area(image);
                !regions
                    .iter()
                    .any(|region| region.intersection(&area).is_some())
            })
            .cloned()
            .collect(),
    )
}

pub fn blink_damage(state: &TerminalState, cursor: CursorOptions) -> Damage {
    let mut rows: Vec<usize> = state.screen().blinking_rows().collect();
    if state.cursor_is_visible() && cursor.blinks(state.cursor_is_blinking()) {
        rows.push(state.cursor_position().0);
    }
    Damage::Rows(rows)
}

pub fn composing_rows(state: &TerminalState, drawn: Option<usize>) -> Vec<usize> {
    let cursor = state.cursor_position().0;
    match drawn {
        Some(row) if row != cursor => vec![cursor, row],
        _ => vec![cursor],
    }
}

pub fn hover_rows(run: Option<&LinkRun>) -> Vec<usize> {
    run.map(|run| run.spans.iter().map(|(row, _, _)| *row).collect())
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) mod test_helpers;

#[cfg(test)]
mod tests;

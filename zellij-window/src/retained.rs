use crate::atlas::GlyphCache;
use crate::color::{Contrast, Paints, Srgb};
use crate::composition::Preedit;
use crate::font::CellMetrics;
use crate::links::LinkRun;
#[cfg(test)]
use crate::scene::Scene;
use crate::scene::{
    self, BlinkPhase, CursorOptions, HeldOut, ImageKey, ImageQuad, PixelRect, RowContext,
    RowScene, RowScratch, Transparency,
};
use crate::terminal::{GraphicsStamp, TerminalState};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Damage {
    Everything,
    Rows(Vec<usize>),
}

impl Damage {
    #[cfg(test)]
    pub fn of(rows: impl IntoIterator<Item = usize>) -> Self {
        Damage::Rows(rows.into_iter().collect())
    }
}

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
            generation: 0,
            scratch: RowScratch::default(),
            rebuilt_everything: true,
            rebuilt_rows: Vec::new(),
        }
    }

    pub fn identity(&self) -> u64 {
        self.identity
    }

    #[cfg(test)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[cfg(test)]
    pub fn height(&self) -> u32 {
        self.height
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

    #[allow(clippy::too_many_arguments)]
    pub fn refresh(
        &mut self,
        state: &TerminalState,
        cache: &mut GlyphCache,
        phase: BlinkPhase,
        paints: &Paints,
        cursor: CursorOptions,
        hovered_link: Option<&LinkRun>,
        preedit: Option<&Preedit>,
    ) {
        self.refresh_once(state, cache, phase, paints, cursor, hovered_link, preedit);
        let now = Instant::now();
        if cache.exhausted() && cache.may_flush(now) {
            cache.flush();
            self.refresh_once(state, cache, phase, paints, cursor, hovered_link, preedit);
            if cache.exhausted() {
                cache.note_overflow(now);
            }
        }
        cache.settled();
    }

    #[allow(clippy::too_many_arguments)]
    fn refresh_once(
        &mut self,
        state: &TerminalState,
        cache: &mut GlyphCache,
        phase: BlinkPhase,
        paints: &Paints,
        cursor: CursorOptions,
        hovered_link: Option<&LinkRun>,
        preedit: Option<&Preedit>,
    ) {
        let context = RowContext::new(state, cache, phase, paints, cursor, hovered_link, preedit)
            .with_transparency(self.transparency)
            .with_contrast(&self.contrast)
            .with_held_out(&self.held);
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

    #[cfg(test)]
    pub fn flatten(&self) -> Scene {
        let mut scene = Scene {
            width: self.width,
            height: self.height,
            clear: self.clear,
            rects: Vec::new(),
            glyphs: Vec::new(),
            color_glyphs: Vec::new(),
            images: self.images().to_vec(),
            resident_images: self.resident_images.clone(),
        };
        for row in &self.rows {
            scene.rects.extend_from_slice(&row.rects);
            scene.glyphs.extend_from_slice(&row.glyphs);
            scene.color_glyphs.extend_from_slice(&row.color_glyphs);
        }
        scene
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
    let regions: Vec<PixelRect> = held
        .iter()
        .flat_map(|held| held.regions(metrics))
        .collect();
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
mod tests {
    use super::*;
    use crate::font::{FontStack, DEFAULT_FONT_SIZE};
    use crate::screen_buffer::painter::Painter;
    use zellij_utils::structured_render::{
        FrameBuilder, GraphicsMedium, GraphicsRecord, GRAPHICS_FORMAT_RGBA8,
    };

    fn cache() -> GlyphCache {
        GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).expect("no embedded font"))
    }

    fn refreshed(scene: &mut RetainedScene, state: &TerminalState, cache: &mut GlyphCache) {
        scene.refresh(
            state,
            cache,
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            None,
            None,
        );
    }

    fn rebuilt(state: &TerminalState, cache: &mut GlyphCache) -> Scene {
        scene::build_at(
            state,
            cache,
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            None,
            None,
        )
    }

    fn painted() -> TerminalState {
        Painter::state(4, 8, |painter| painter.text(0, 0, "hello"))
    }

    #[test]
    fn a_retained_scene_starts_by_building_every_row() {
        let mut cache = cache();
        let state = painted();
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        assert!(retained.rebuilt_everything());
        assert_eq!(retained.flatten(), rebuilt(&state, &mut cache));
    }

    #[test]
    fn an_unmarked_refresh_rebuilds_nothing() {
        let mut cache = cache();
        let state = painted();
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        refreshed(&mut retained, &state, &mut cache);
        assert!(!retained.rebuilt_everything());
        assert!(retained.rebuilt_rows().is_empty());
        assert_eq!(retained.flatten(), rebuilt(&state, &mut cache));
    }

    #[test]
    fn only_the_rows_a_frame_touched_are_rebuilt() {
        let mut cache = cache();
        let mut state = painted();
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);

        Painter::apply_marking(&mut state, &mut retained, |painter| {
            painter.text(2, 0, "second")
        });
        refreshed(&mut retained, &state, &mut cache);
        assert!(!retained.rebuilt_everything());
        assert_eq!(retained.rebuilt_rows(), [2]);
        assert_eq!(retained.flatten(), rebuilt(&state, &mut cache));
    }

    #[test]
    fn a_new_glyph_cache_rebuilds_every_row() {
        let mut cache = cache();
        let state = painted();
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        refreshed(&mut retained, &state, &mut cache);
        assert!(!retained.rebuilt_everything());

        let mut rebuilt_cache = self::cache();
        refreshed(&mut retained, &state, &mut rebuilt_cache);
        assert!(retained.rebuilt_everything());
    }

    #[test]
    fn a_row_index_past_the_viewport_marks_everything() {
        let mut cache = cache();
        let state = painted();
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        retained.mark(&Damage::of([9]));
        refreshed(&mut retained, &state, &mut cache);
        assert!(retained.rebuilt_everything());
    }

    #[test]
    fn a_cursor_that_moves_marks_the_row_it_left_and_the_row_it_entered() {
        let mut cache = cache();
        let mut state = Painter::state(4, 8, |painter| {
            painter.text(0, 0, "hello");
            painter.cursor(0, 0, crate::screen_buffer::CursorShape::Block);
        });
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);

        Painter::apply_marking(&mut state, &mut retained, |painter| {
            painter.cursor(3, 4, crate::screen_buffer::CursorShape::Block)
        });
        refreshed(&mut retained, &state, &mut cache);
        assert_eq!(retained.rebuilt_rows(), [0, 3]);
        assert_eq!(retained.flatten(), rebuilt(&state, &mut cache));
    }

    #[test]
    fn a_blink_flip_damages_only_the_rows_that_blink() {
        let mut cache = cache();
        let state = Painter::state(4, 8, |painter| {
            painter.text(0, 0, "steady");
            painter.styled(2, 0, "blinks", |cell| {
                cell.attrs |= zellij_utils::structured_render::ATTR_SLOW_BLINK
            });
        });
        let damage = blink_damage(&state, CursorOptions::default());
        assert_eq!(damage, Damage::Rows(vec![2]));

        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        retained.mark(&damage);
        retained.refresh(
            &state,
            &mut cache,
            BlinkPhase::Off,
            &Paints::default(),
            CursorOptions::default(),
            None,
            None,
        );
        assert_eq!(retained.rebuilt_rows(), [2]);
        assert_eq!(
            retained.flatten(),
            scene::build_at(
                &state,
                &mut cache,
                BlinkPhase::Off,
                &Paints::default(),
                CursorOptions::default(),
                None,
                None,
            )
        );
    }

    fn tiny_cache() -> GlyphCache {
        GlyphCache::tiny(FontStack::embedded(DEFAULT_FONT_SIZE).expect("no embedded font"))
    }

    fn crowded() -> crate::terminal::TerminalState {
        Painter::state(4, 40, |painter| {
            painter.text(0, 0, "abcdefghijklmnopqrstuvwxyz0123456789");
            painter.text(1, 0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ!?#$%&*+-=");
        })
    }

    #[test]
    fn a_refresh_that_fills_the_atlas_flushes_it_and_rebuilds_against_the_fresh_one() {
        let mut cache = tiny_cache();
        let state = crowded();
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        assert_eq!(cache.flushes(), 1, "a full atlas must be renewed once");
        assert!(retained.rebuilt_everything());
        assert_eq!(retained.flatten(), rebuilt(&state, &mut tiny_cache()));
    }

    fn overflowing() -> crate::terminal::TerminalState {
        Painter::state(6, 40, |painter| {
            painter.text(0, 0, "abcdefghijklmnopqrstuvwxyz0123456789");
            painter.text(1, 0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ!?#$%&*+-=");
            painter.text(
                2,
                0,
                "\u{4f60}\u{597d}\u{4e16}\u{754c}\u{6f22}\u{5b57}\u{4e2d}\u{6587}\u{65e5}\u{672c}",
            );
            painter.text(
                3,
                0,
                "\u{3042}\u{3044}\u{3046}\u{3048}\u{304a}\u{30a2}\u{30a4}\u{30a6}\u{30a8}\u{30aa}",
            );
        })
    }

    #[test]
    fn a_screen_the_atlas_cannot_hold_is_not_flushed_on_every_refresh() {
        let mut cache = tiny_cache();
        let state = overflowing();
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        assert!(
            cache.exhausted(),
            "the screen must overflow even a freshly flushed tiny atlas for this test to mean anything"
        );
        let flushes = cache.flushes();
        for _ in 0..20 {
            retained.mark(&Damage::Everything);
            refreshed(&mut retained, &state, &mut cache);
        }
        assert_eq!(
            cache.flushes(),
            flushes,
            "an overflowing screen flushed the atlas again straight away"
        );
    }

    #[test]
    fn a_screen_the_atlas_holds_easily_is_never_flushed() {
        let mut cache = tiny_cache();
        let state = Painter::state(4, 8, |painter| painter.text(0, 0, "aaa"));
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        assert_eq!(cache.flushes(), 0);
    }

    fn graphics(state: &mut crate::terminal::TerminalState, records: &[GraphicsRecord]) {
        let size = state.size();
        let mut builder = FrameBuilder::new(size.cols as u16, size.rows as u16, 0);
        builder.extend_graphics(records.iter());
        state
            .apply_frame(&builder.finish())
            .expect("a usable frame");
    }

    fn placement(cell_x: u32) -> GraphicsRecord {
        GraphicsRecord::Placement {
            image_id: 1,
            placement_id: 1,
            cell_x,
            cell_y: 1,
            offset_x: 0,
            offset_y: 0,
            source_x: 0,
            source_y: 0,
            source_width: 2,
            source_height: 2,
            z: 0,
        }
    }

    fn with_an_image(cell_x: u32) -> crate::terminal::TerminalState {
        let mut state = crate::terminal::TerminalState::new(4, 8);
        state.set_cell_size(8, 20);
        graphics(
            &mut state,
            &[
                GraphicsRecord::Residency {
                    id: 1,
                    width: 2,
                    height: 2,
                    format: GRAPHICS_FORMAT_RGBA8,
                    medium: GraphicsMedium::Inline,
                    byte_len: 16,
                    data: [10, 20, 30, 255].repeat(4),
                },
                placement(cell_x),
            ],
        );
        state
    }

    fn see_through() -> Transparency {
        Transparency {
            opacity: 0.8,
            mode: zellij_utils::input::window::OpacityMode::Background,
        }
    }

    fn rects_in_row(scene: &RetainedScene, row: usize) -> usize {
        scene.rows()[row].rects.len()
    }

    #[test]
    fn a_see_through_background_draws_only_the_backgrounds_a_program_chose() {
        use zellij_utils::structured_render::{WireColor, ATTR_REVERSE};
        let mut cache = cache();
        let state = Painter::state(4, 8, |painter| {
            painter.text(0, 0, "plain");
            painter.styled(1, 0, " ", |cell| cell.bg = WireColor::Rgb(0, 0, 0).pack());
            painter.styled(2, 0, " ", |cell| cell.attrs |= ATTR_REVERSE);
            painter.styled(3, 0, " ", |cell| cell.bg = WireColor::Named(1).pack());
        });

        let mut opaque = RetainedScene::new();
        refreshed(&mut opaque, &state, &mut cache);
        assert_eq!(rects_in_row(&opaque, 0), 0);
        assert_eq!(
            rects_in_row(&opaque, 1),
            0,
            "a solid window folds an explicit copy of the default into the clear"
        );

        let mut clear = RetainedScene::new();
        clear.set_transparency(see_through());
        refreshed(&mut clear, &state, &mut cache);
        assert_eq!(
            rects_in_row(&clear, 0),
            0,
            "a default cell must let the clear through"
        );
        assert_eq!(
            rects_in_row(&clear, 1),
            1,
            "an explicit background equal to the default"
        );
        assert_eq!(rects_in_row(&clear, 2), 1, "a reverse-video cell");
        assert_eq!(rects_in_row(&clear, 3), 1, "a colored cell");
    }

    #[test]
    fn hidden_text_on_a_see_through_background_leaves_no_ink() {
        use zellij_utils::structured_render::{
            WireColor, ATTR_HIDDEN, UNDERLINE_SHIFT, UNDERLINE_STRAIGHT,
        };
        let mut cache = cache();
        let state = Painter::state(2, 8, |painter| {
            painter.styled(0, 0, "MM", |cell| {
                cell.attrs |= ATTR_HIDDEN | (UNDERLINE_STRAIGHT << UNDERLINE_SHIFT)
            });
            painter.styled(1, 0, "MM", |cell| {
                cell.attrs |= ATTR_HIDDEN;
                cell.bg = WireColor::Named(1).pack();
            });
        });
        let mut opaque = RetainedScene::new();
        refreshed(&mut opaque, &state, &mut cache);
        assert!(!opaque.rows()[0].glyphs.is_empty() || !opaque.rows()[0].rects.is_empty());

        let mut clear = RetainedScene::new();
        clear.set_transparency(see_through());
        refreshed(&mut clear, &state, &mut cache);
        assert!(clear.rows()[0].glyphs.is_empty(), "hidden glyphs showed");
        assert!(
            clear.rows()[0].rects.is_empty(),
            "a hidden underline showed"
        );
        assert_eq!(clear.rows()[1].rects.len(), 2);
    }

    #[test]
    fn a_change_of_transparency_rebuilds_every_row() {
        let mut cache = cache();
        let state = painted();
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        refreshed(&mut retained, &state, &mut cache);
        assert!(!retained.rebuilt_everything());

        retained.set_transparency(see_through());
        refreshed(&mut retained, &state, &mut cache);
        assert!(retained.rebuilt_everything());
        assert_eq!(retained.transparency(), see_through());

        retained.set_transparency(see_through());
        refreshed(&mut retained, &state, &mut cache);
        assert!(
            !retained.rebuilt_everything(),
            "an unchanged setting rebuilt"
        );

        retained.set_transparency(Transparency {
            mode: zellij_utils::input::window::OpacityMode::Everything,
            ..see_through()
        });
        refreshed(&mut retained, &state, &mut cache);
        assert!(
            retained.rebuilt_everything(),
            "a mode change did not rebuild"
        );
    }

    #[test]
    fn a_graphics_state_that_has_not_moved_keeps_the_image_list_it_emitted() {
        let mut cache = cache();
        let state = with_an_image(1);
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        assert!(retained.replaced_images());
        assert_eq!(retained.images().len(), 1);

        refreshed(&mut retained, &state, &mut cache);
        assert!(
            !retained.replaced_images(),
            "nothing about the graphics changed, so the placements must not be re-emitted"
        );
        assert_eq!(retained.images().len(), 1);
    }

    #[test]
    fn a_placement_that_moves_is_re_emitted() {
        let mut cache = cache();
        let mut state = with_an_image(1);
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        let before = retained.images()[0].x;

        graphics(&mut state, &[placement(4)]);
        refreshed(&mut retained, &state, &mut cache);
        assert!(retained.replaced_images());
        assert_ne!(retained.images()[0].x, before);
    }

    #[test]
    fn a_cell_size_that_changes_re_emits_the_placements() {
        let mut cache = cache();
        let state = with_an_image(1);
        let mut retained = RetainedScene::new();
        refreshed(&mut retained, &state, &mut cache);
        refreshed(&mut retained, &state, &mut cache);
        assert!(!retained.replaced_images());

        let mut zoomed = GlyphCache::new(
            FontStack::embedded(DEFAULT_FONT_SIZE * 2.0).expect("no embedded font"),
        );
        refreshed(&mut retained, &state, &mut zoomed);
        assert!(
            retained.replaced_images(),
            "an image is placed in pixels, so a new cell size moves it"
        );
    }
}

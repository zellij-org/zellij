use std::collections::VecDeque;
use std::time::{Duration, Instant};

use zellij_utils::structured_render::{FrameBuilder, FrameView, WireCell};

use crate::atlas::GlyphCache;
use crate::color::{Contrast, Paints};
use crate::composition::Preedit;
use crate::scene::{
    self, BlinkPhase, CellRect, CursorOptions, GlyphQuad, HeldOut, ImageQuad, PixelRect, Rect,
    RowContext, RowScene, RowScratch, Transparency,
};
use crate::terminal::TerminalState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollSettings {
    pub enabled: bool,
    pub duration: Duration,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ScrollLayer {
    pub rects: Vec<Rect>,
    pub glyphs: Vec<GlyphQuad>,
    pub color_glyphs: Vec<GlyphQuad>,
    pub images: Vec<ImageQuad>,
}

impl ScrollLayer {
    pub fn clear(&mut self) {
        self.rects.clear();
        self.glyphs.clear();
        self.color_glyphs.clear();
        self.images.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
            && self.glyphs.is_empty()
            && self.color_glyphs.is_empty()
            && self.images.is_empty()
    }
}

struct Strip {
    state: TerminalState,
    positions: Vec<isize>,
}

struct PaneAnimation {
    pane: usize,
    content: CellRect,
    covers: Vec<CellRect>,
    grid_cols: usize,
    from: f64,
    since: Instant,
    duration: Duration,
    above: VecDeque<Vec<WireCell>>,
    below: VecDeque<Vec<WireCell>>,
    strip: Option<Strip>,
}

fn ease_out(progress: f64) -> f64 {
    let remaining = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - remaining * remaining * remaining
}

impl PaneAnimation {
    fn offset_at(&self, now: Instant) -> f64 {
        if self.duration.is_zero() {
            return 0.0;
        }
        let elapsed = now.saturating_duration_since(self.since).as_secs_f64();
        let progress = elapsed / self.duration.as_secs_f64();
        self.from * (1.0 - ease_out(progress))
    }

    fn finished_at(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.since) >= self.duration
    }

    fn row_before(&self, before: &TerminalState, row: usize) -> Vec<WireCell> {
        let y = self.content.y + row;
        (0..self.content.cols)
            .map(|col| {
                let x = self.content.x + col;
                if self.covers.iter().any(|cover| cover.contains(y, x)) {
                    WireCell::BLANK
                } else {
                    before.screen().cell(y, x)
                }
            })
            .collect()
    }

    fn shift(&mut self, before: &TerminalState, lines: i64) {
        let rows = self.content.rows;
        let moved = (lines.unsigned_abs() as usize).min(rows);
        if lines > 0 {
            for row in (rows - moved..rows).rev() {
                let cells = self.row_before(before, row);
                self.below.push_front(cells);
            }
            for _ in 0..moved {
                self.above.pop_front();
            }
            self.below.truncate(rows);
        } else {
            for row in 0..moved {
                let cells = self.row_before(before, row);
                self.above.push_front(cells);
            }
            for _ in 0..moved {
                self.below.pop_front();
            }
            self.above.truncate(rows);
        }
        self.strip = None;
    }

    fn strip(&mut self) -> Option<&Strip> {
        if self.strip.is_none() {
            self.strip = self.build_strip();
        }
        self.strip.as_ref()
    }

    fn build_strip(&self) -> Option<Strip> {
        let total = self.above.len() + self.below.len();
        if total == 0 || self.grid_cols == 0 {
            return None;
        }
        let mut builder = FrameBuilder::new(self.grid_cols as u16, total as u16, 0);
        builder.set_full_repaint(true);
        let mut positions = Vec::with_capacity(total);
        let mut index = 0u16;
        for (distance, cells) in self.above.iter().enumerate().rev() {
            builder.push_row(index, self.content.x as u16, cells);
            positions.push(-(distance as isize) - 1);
            index += 1;
        }
        for (distance, cells) in self.below.iter().enumerate() {
            builder.push_row(index, self.content.x as u16, cells);
            positions.push((self.content.rows + distance) as isize);
            index += 1;
        }
        let mut state = TerminalState::new(total, self.grid_cols);
        state.apply_frame(&builder.finish()).ok()?;
        Some(Strip { state, positions })
    }

    #[cfg(test)]
    fn uncovered_rows(&self, now: Instant, cell_height: u32) -> usize {
        if cell_height == 0 {
            return 0;
        }
        let offset = self.offset_at(now);
        let needed = (offset.abs() / cell_height as f64).ceil() as usize;
        let held = if offset < 0.0 {
            self.below.len()
        } else {
            self.above.len()
        };
        needed.saturating_sub(held)
    }
}

#[derive(Default)]
pub struct ScrollAnimations {
    panes: Vec<PaneAnimation>,
    scratch: RowScratch,
    row: RowScene,
}

impl ScrollAnimations {
    pub fn is_active(&self) -> bool {
        !self.panes.is_empty()
    }

    pub fn cancel(&mut self) {
        self.panes.clear();
    }

    pub fn note_frame(
        &mut self,
        before: &TerminalState,
        view: &FrameView<'_>,
        settings: ScrollSettings,
        cell_height: u32,
        now: Instant,
    ) {
        let header = view.header();
        if header.full_repaint() || header.clear() {
            self.cancel();
            return;
        }
        if let Some(geometry) = view.geometry() {
            if &geometry != before.geometry() {
                self.cancel();
                return;
            }
        }
        if !settings.enabled {
            self.cancel();
            return;
        }
        let scroll = view.scroll();
        let geometry = before.geometry();
        let grid_cols = before.size().cols;
        for entry in scroll.entries {
            let index = entry.pane as usize;
            let Some(held) = HeldOut::of(geometry, index) else {
                continue;
            };
            let content = held.content;
            if content.rows == 0 || content.cols == 0 {
                continue;
            }
            let lines = entry.lines as i64;
            let existing = self.panes.iter().position(|pane| pane.pane == index);
            let remaining = existing
                .map(|at| self.panes[at].offset_at(now))
                .unwrap_or(0.0);
            let target = remaining - lines as f64 * cell_height as f64;
            let height = content.rows as f64 * cell_height as f64;
            if lines.unsigned_abs() as usize > content.rows || target.abs() > height {
                if let Some(at) = existing {
                    self.panes.remove(at);
                }
                continue;
            }
            let at = match existing {
                Some(at) if self.panes[at].content == content => at,
                Some(at) => {
                    self.panes.remove(at);
                    self.push(index, content, grid_cols, now, settings.duration)
                },
                None => self.push(index, content, grid_cols, now, settings.duration),
            };
            self.panes[at].covers = held.covers;
            let animation = &mut self.panes[at];
            animation.shift(before, lines);
            animation.from = target;
            animation.since = now;
            animation.duration = settings.duration;
            if target == 0.0 {
                self.panes.remove(at);
            }
        }
    }

    fn push(
        &mut self,
        pane: usize,
        content: CellRect,
        grid_cols: usize,
        now: Instant,
        duration: Duration,
    ) -> usize {
        self.panes.push(PaneAnimation {
            pane,
            content,
            covers: Vec::new(),
            grid_cols,
            from: 0.0,
            since: now,
            duration,
            above: VecDeque::new(),
            below: VecDeque::new(),
            strip: None,
        });
        self.panes.len() - 1
    }

    pub fn retire(&mut self, now: Instant) -> bool {
        let before = self.panes.len();
        self.panes.retain(|pane| !pane.finished_at(now));
        before != self.panes.len()
    }

    #[cfg(test)]
    pub fn offset_of(&self, pane: usize, now: Instant) -> Option<f64> {
        self.panes
            .iter()
            .find(|animation| animation.pane == pane)
            .map(|animation| animation.offset_at(now))
    }

    #[cfg(test)]
    pub fn animating(&self, pane: usize) -> bool {
        self.panes.iter().any(|animation| animation.pane == pane)
    }

    #[cfg(test)]
    pub fn uncovered_rows(&self, pane: usize, now: Instant, cell_height: u32) -> Option<usize> {
        self.panes
            .iter()
            .find(|animation| animation.pane == pane)
            .map(|animation| animation.uncovered_rows(now, cell_height))
    }

    pub fn held_out(&self, state: &TerminalState) -> Vec<HeldOut> {
        self.panes
            .iter()
            .filter_map(|animation| HeldOut::of(state.geometry(), animation.pane))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_layer(
        &mut self,
        layer: &mut ScrollLayer,
        state: &TerminalState,
        cache: &mut GlyphCache,
        phase: BlinkPhase,
        paints: &Paints,
        cursor: CursorOptions,
        preedit: Option<&Preedit>,
        transparency: Transparency,
        contrast: &Contrast,
        images: &[ImageQuad],
        now: Instant,
    ) {
        layer.clear();
        if self.panes.is_empty() {
            return;
        }
        let metrics = cache.metrics();
        let rows = state.size().rows;
        let ScrollAnimations {
            panes,
            scratch,
            row: row_scene,
        } = self;
        for animation in panes.iter_mut() {
            let Some(held) = HeldOut::of(state.geometry(), animation.pane) else {
                continue;
            };
            let regions = held.regions(metrics);
            if regions.is_empty() {
                continue;
            }
            let offset = animation.offset_at(now).round() as i32;
            let content = animation.content;
            let live = RowContext::new(state, cache, phase, paints, cursor, None, preedit)
                .with_transparency(transparency)
                .with_contrast(contrast)
                .with_only(&held);
            for row in content.y..(content.y + content.rows).min(rows) {
                scene::build_row(&live, cache, row, scratch, row_scene);
                push_clipped(layer, row_scene, offset, &regions);
            }
            for image in images {
                let area = scene::image_area(image);
                if !regions
                    .iter()
                    .any(|region| region.intersection(&area).is_some())
                {
                    continue;
                }
                for region in &regions {
                    if let Some(clipped) = clip_image(image, offset, region) {
                        layer.images.push(clipped);
                    }
                }
            }
            let Some(strip) = animation.strip() else {
                continue;
            };
            let context = RowContext::new(
                &strip.state,
                cache,
                phase,
                paints,
                CursorOptions::default(),
                None,
                None,
            )
            .with_transparency(transparency)
            .with_contrast(contrast);
            for (index, position) in strip.positions.iter().enumerate() {
                scene::build_row(&context, cache, index, scratch, row_scene);
                let rows = content.y as isize + position - index as isize;
                let shift = rows as i32 * metrics.height as i32 + offset;
                push_clipped(layer, row_scene, shift, &regions);
            }
        }
    }
}

fn push_clipped(layer: &mut ScrollLayer, row: &RowScene, dy: i32, regions: &[PixelRect]) {
    for region in regions {
        layer.rects.extend(
            row.rects
                .iter()
                .filter_map(|rect| clip_rect(rect, dy, region)),
        );
        layer.glyphs.extend(
            row.glyphs
                .iter()
                .filter_map(|glyph| clip_glyph(glyph, dy, region)),
        );
        layer.color_glyphs.extend(
            row.color_glyphs
                .iter()
                .filter_map(|glyph| clip_glyph(glyph, dy, region)),
        );
    }
}

fn area(x: i32, y: i32, width: u32, height: u32) -> PixelRect {
    PixelRect {
        left: x,
        top: y,
        right: x.saturating_add(width as i32),
        bottom: y.saturating_add(height as i32),
    }
}

pub fn clip_rect(rect: &Rect, dy: i32, region: &PixelRect) -> Option<Rect> {
    let kept = area(rect.x, rect.y + dy, rect.width, rect.height).intersection(region)?;
    Some(Rect {
        x: kept.left,
        y: kept.top,
        width: (kept.right - kept.left) as u32,
        height: (kept.bottom - kept.top) as u32,
        color: rect.color,
    })
}

pub fn clip_glyph(glyph: &GlyphQuad, dy: i32, region: &PixelRect) -> Option<GlyphQuad> {
    let (x, y) = (glyph.x, glyph.y + dy);
    let kept = area(x, y, glyph.entry.width, glyph.entry.height).intersection(region)?;
    let mut entry = glyph.entry;
    entry.x += (kept.left - x) as u32;
    entry.y += (kept.top - y) as u32;
    entry.width = (kept.right - kept.left) as u32;
    entry.height = (kept.bottom - kept.top) as u32;
    Some(GlyphQuad {
        x: kept.left,
        y: kept.top,
        entry,
        color: glyph.color,
    })
}

pub fn clip_image(image: &ImageQuad, dy: i32, region: &PixelRect) -> Option<ImageQuad> {
    let (x, y) = (image.x, image.y + dy);
    let kept = area(x, y, image.width, image.height).intersection(region)?;
    Some(ImageQuad {
        x: kept.left,
        y: kept.top,
        width: (kept.right - kept.left) as u32,
        height: (kept.bottom - kept.top) as u32,
        source_x: image.source_x + (kept.left - x) as u32,
        source_y: image.source_y + (kept.top - y) as u32,
        ..image.clone()
    })
}

#[cfg(test)]
pub(crate) mod sample {
    use super::*;
    use zellij_utils::structured_render::{
        GeometryRecord, PaneRect, ScrollEntry, ScrollRecord, WireColor, PANE_FRAMED,
    };

    pub const ROWS: usize = 12;
    pub const COLS: usize = 20;
    pub const CELL_HEIGHT: u32 = 20;
    pub const DURATION: Duration = Duration::from_millis(100);

    pub fn framed(x: u16, y: u16, cols: u16, rows: u16) -> PaneRect {
        PaneRect {
            x,
            y,
            cols,
            rows,
            top: 1,
            bottom: 1,
            left: 1,
            right: 1,
            flags: PANE_FRAMED,
        }
    }

    pub fn panes() -> GeometryRecord {
        GeometryRecord {
            panes: vec![
                framed(0, 0, 10, 12),
                framed(10, 0, 10, 12),
                framed(4, 3, 8, 4),
            ],
        }
    }

    pub fn line(base: i64, row: usize) -> String {
        format!("{:<8}", format!("L{:03}", base + row as i64))
    }

    pub fn cell_at(base: i64, row: usize, col: usize) -> WireCell {
        let geometry = panes();
        if geometry.panes[2].contains(col as u16, row as u16) {
            return WireCell {
                ch: 'F' as u32,
                bg: WireColor::Named(1).pack(),
                ..WireCell::BLANK
            };
        }
        let scrolled = CellRect::content_of(&geometry.panes[0]);
        if scrolled.contains(row, col) {
            let text = line(base, row - scrolled.y);
            let character = text.chars().nth(col - scrolled.x).unwrap_or(' ');
            return WireCell {
                ch: character as u32,
                bg: WireColor::Named(4).pack(),
                ..WireCell::BLANK
            };
        }
        if CellRect::content_of(&geometry.panes[1]).contains(row, col) {
            return WireCell {
                ch: 'B' as u32,
                ..WireCell::BLANK
            };
        }
        WireCell::BLANK
    }

    pub fn frame(
        seq: u64,
        full: bool,
        geometry: Option<GeometryRecord>,
        base: i64,
        hints: &[(u16, i16)],
    ) -> Vec<u8> {
        let mut builder = FrameBuilder::new(COLS as u16, ROWS as u16, seq);
        builder.set_full_repaint(full);
        builder.set_clear(full);
        for row in 0..ROWS {
            let cells: Vec<WireCell> = (0..COLS).map(|col| cell_at(base, row, col)).collect();
            builder.push_row(row as u16, 0, &cells);
        }
        if let Some(geometry) = geometry {
            builder.push_geometry(&geometry);
        }
        builder.push_scroll(&ScrollRecord {
            entries: hints
                .iter()
                .map(|&(pane, lines)| ScrollEntry { pane, lines })
                .collect(),
        });
        builder.finish()
    }

    pub fn on() -> ScrollSettings {
        ScrollSettings {
            enabled: true,
            duration: DURATION,
        }
    }

    pub struct Scene {
        pub state: TerminalState,
        pub scroll: ScrollAnimations,
        pub base: i64,
        pub seq: u64,
        pub cell_height: u32,
    }

    impl Scene {
        pub fn new() -> Self {
            let mut state = TerminalState::new(ROWS, COLS);
            state
                .apply_frame(&frame(0, true, Some(panes()), 100, &[]))
                .unwrap();
            Self {
                state,
                scroll: ScrollAnimations::default(),
                base: 100,
                seq: 1,
                cell_height: CELL_HEIGHT,
            }
        }

        pub fn scroll_with(&mut self, lines: i16, settings: ScrollSettings, now: Instant) {
            self.base -= lines as i64;
            let bytes = frame(self.seq, false, None, self.base, &[(0, lines)]);
            self.apply(&bytes, settings, now);
        }

        pub fn scroll(&mut self, lines: i16, now: Instant) {
            self.scroll_with(lines, on(), now);
        }

        pub fn apply(&mut self, bytes: &[u8], settings: ScrollSettings, now: Instant) {
            self.seq += 1;
            let scroll = &mut self.scroll;
            let cell_height = self.cell_height;
            self.state
                .apply_frame_with(bytes, |before, view| {
                    scroll.note_frame(before, view, settings, cell_height, now)
                })
                .unwrap();
        }

        pub fn text(cells: &[WireCell]) -> String {
            cells.iter().map(|cell| cell.character()).collect()
        }
    }

    pub fn mid_slide(cache: &mut GlyphCache) -> (crate::retained::RetainedScene, ScrollLayer) {
        let mut scene = Scene::new();
        scene.cell_height = cache.metrics().height;
        let start = Instant::now();
        scene.scroll(3, start);
        let mut retained = crate::retained::RetainedScene::new();
        retained.set_held_out(scene.scroll.held_out(&scene.state));
        retained.refresh(
            &scene.state,
            cache,
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            None,
            None,
        );
        let mut layer = ScrollLayer::default();
        let contrast = Contrast::new(1.0);
        scene.scroll.build_layer(
            &mut layer,
            &scene.state,
            cache,
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            None,
            Transparency::OPAQUE,
            &contrast,
            retained.all_images(),
            start + DURATION / 4,
        );
        (retained, layer)
    }
}

#[cfg(test)]
mod tests {
    use super::sample::*;
    use super::*;
    use crate::font::{FontStack, DEFAULT_FONT_SIZE};
    use crate::retained::RetainedScene;

    #[test]
    fn the_offset_starts_where_the_content_was_and_eases_to_nothing_over_the_duration() {
        let mut scene = Scene::new();
        let start = Instant::now();
        scene.scroll(3, start);
        assert_eq!(scene.scroll.offset_of(0, start), Some(-60.0));
        let halfway = scene.scroll.offset_of(0, start + DURATION / 2).unwrap();
        assert!(halfway > -60.0 && halfway < 0.0, "{}", halfway);
        assert_eq!(scene.scroll.offset_of(0, start + DURATION), Some(0.0));
        assert!(!scene.scroll.animating(1));

        assert!(!scene.scroll.retire(start + DURATION / 2));
        assert!(scene.scroll.retire(start + DURATION));
        assert!(!scene.scroll.is_active());
    }

    #[test]
    fn scrolling_the_other_way_starts_below_and_slides_up() {
        let mut scene = Scene::new();
        let start = Instant::now();
        scene.scroll(-2, start);
        assert_eq!(scene.scroll.offset_of(0, start), Some(40.0));
    }

    #[test]
    fn the_strip_holds_the_rows_that_just_left_nearest_first() {
        let mut scene = Scene::new();
        let start = Instant::now();
        scene.scroll(3, start);
        let animation = &scene.scroll.panes[0];
        let below: Vec<String> = animation
            .below
            .iter()
            .map(|cells| Scene::text(&cells[..4]))
            .collect();
        assert_eq!(below, vec!["L107", "L108", "L109"]);
        assert!(animation.above.is_empty());

        scene.scroll(-5, start + DURATION / 4);
        let animation = &scene.scroll.panes[0];
        let above: Vec<String> = animation
            .above
            .iter()
            .map(|cells| Scene::text(&cells[..3]))
            .collect();
        assert_eq!(above, vec!["L10", "L10", "L09", "L09", "L09"]);
        assert_eq!(Scene::text(&animation.above[3][..4]), "L098");
        assert_eq!(Scene::text(&animation.above[4][..4]), "L097");
        assert!(animation.below.is_empty());
    }

    #[test]
    fn the_strip_covers_every_row_that_enters_in_the_usual_cases() {
        let mut scene = Scene::new();
        let start = Instant::now();
        let check = |scene: &Scene, at: Instant| {
            assert_eq!(
                scene.scroll.uncovered_rows(0, at, CELL_HEIGHT),
                Some(0),
                "rows missing at {:?}",
                at - start
            );
        };
        scene.scroll(3, start);
        for step in 0..10 {
            check(&scene, start + DURATION * step / 10);
        }
        let later = start + DURATION / 3;
        scene.scroll(3, later);
        for step in 0..10 {
            check(&scene, later + DURATION * step / 10);
        }
        let reversed = later + DURATION / 2;
        scene.scroll(-4, reversed);
        for step in 0..10 {
            check(&scene, reversed + DURATION * step / 10);
        }
        let again = reversed + DURATION / 5;
        scene.scroll(-1, again);
        for step in 0..10 {
            check(&scene, again + DURATION * step / 10);
        }
    }

    #[test]
    fn a_hint_during_an_animation_adds_to_the_remaining_offset() {
        let mut scene = Scene::new();
        let start = Instant::now();
        scene.scroll(3, start);
        let later = start + DURATION / 2;
        let remaining = scene.scroll.offset_of(0, later).unwrap();
        scene.scroll(2, later);
        assert_eq!(scene.scroll.offset_of(0, later), Some(remaining - 40.0));
        assert_eq!(scene.scroll.offset_of(0, later + DURATION), Some(0.0));
    }

    #[test]
    fn an_offset_beyond_the_pane_height_drops_the_animation() {
        let mut scene = Scene::new();
        let start = Instant::now();
        scene.scroll(6, start);
        assert!(scene.scroll.is_active());
        scene.scroll(6, start);
        assert!(!scene.scroll.is_active());

        scene.scroll(11, start);
        assert!(!scene.scroll.is_active());
    }

    #[test]
    fn a_frame_that_changes_the_geometry_starts_no_animation() {
        let mut scene = Scene::new();
        let start = Instant::now();
        let mut moved = panes();
        moved.panes[2].x = 5;
        scene.base -= 3;
        let bytes = frame(scene.seq, false, Some(moved), scene.base, &[(0, 3)]);
        scene.apply(&bytes, on(), start);
        assert!(!scene.scroll.is_active());
    }

    #[test]
    fn a_frame_that_changes_the_geometry_ends_a_running_animation() {
        let mut scene = Scene::new();
        let start = Instant::now();
        scene.scroll(3, start);
        let mut moved = panes();
        moved.panes[2].x = 5;
        let bytes = frame(scene.seq, false, Some(moved), scene.base, &[]);
        scene.apply(&bytes, on(), start);
        assert!(!scene.scroll.is_active());
    }

    #[test]
    fn a_repeated_unchanged_geometry_record_does_not_stop_the_animation() {
        let mut scene = Scene::new();
        let start = Instant::now();
        scene.base -= 3;
        let bytes = frame(scene.seq, false, Some(panes()), scene.base, &[(0, 3)]);
        scene.apply(&bytes, on(), start);
        assert!(scene.scroll.is_active());
    }

    #[test]
    fn a_full_repaint_ends_every_animation() {
        let mut scene = Scene::new();
        let start = Instant::now();
        scene.scroll(3, start);
        let bytes = frame(scene.seq, true, None, scene.base, &[]);
        scene.apply(&bytes, on(), start);
        assert!(!scene.scroll.is_active());
    }

    #[test]
    fn with_smooth_scrolling_off_nothing_animates() {
        let mut scene = Scene::new();
        let start = Instant::now();
        let off = ScrollSettings {
            enabled: false,
            ..on()
        };
        scene.scroll_with(3, off, start);
        assert!(!scene.scroll.is_active());
    }

    #[test]
    fn a_wheel_notch_and_a_touchpad_scroll_of_three_lines_animate_alike() {
        let start = Instant::now();
        let mut notch = Scene::new();
        notch.scroll(3, start);
        let mut touchpad = Scene::new();
        touchpad.scroll(1, start);
        touchpad.scroll(1, start);
        touchpad.scroll(1, start);
        for step in 0..=10 {
            let at = start + DURATION * step / 10;
            assert_eq!(
                notch.scroll.offset_of(0, at),
                touchpad.scroll.offset_of(0, at)
            );
        }
    }

    fn cache() -> GlyphCache {
        GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).expect("no embedded font"))
    }

    fn layer_at(scene: &mut Scene, cache: &mut GlyphCache, at: Instant) -> ScrollLayer {
        let mut layer = ScrollLayer::default();
        let contrast = Contrast::new(1.0);
        scene.scroll.build_layer(
            &mut layer,
            &scene.state,
            cache,
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            None,
            Transparency::OPAQUE,
            &contrast,
            &[],
            at,
        );
        layer
    }

    fn inside(outer: &PixelRect, inner: &PixelRect) -> bool {
        inner.left >= outer.left
            && inner.right <= outer.right
            && inner.top >= outer.top
            && inner.bottom <= outer.bottom
    }

    #[test]
    fn the_layer_is_trimmed_to_the_pane_and_kept_out_from_under_the_floating_pane() {
        let mut cache = cache();
        let metrics = cache.metrics();
        let mut scene = Scene::new();
        let start = Instant::now();
        scene.scroll(3, start);
        let layer = layer_at(&mut scene, &mut cache, start + DURATION / 2);
        assert!(!layer.rects.is_empty());
        assert!(!layer.glyphs.is_empty());

        let geometry = panes();
        let content = CellRect::content_of(&geometry.panes[0]).pixels(metrics);
        let floating = CellRect::outer_of(&geometry.panes[2]).pixels(metrics);
        let areas = layer
            .rects
            .iter()
            .map(|rect| area(rect.x, rect.y, rect.width, rect.height))
            .chain(
                layer
                    .glyphs
                    .iter()
                    .chain(layer.color_glyphs.iter())
                    .map(|glyph| area(glyph.x, glyph.y, glyph.entry.width, glyph.entry.height)),
            );
        for piece in areas {
            assert!(inside(&content, &piece), "{:?} leaves the pane", piece);
            assert!(
                floating.intersection(&piece).is_none(),
                "{:?} is drawn under the floating pane",
                piece
            );
        }
    }

    #[test]
    fn the_layer_fills_the_rows_the_new_content_has_not_reached_yet() {
        let mut cache = cache();
        let metrics = cache.metrics();
        let mut scene = Scene::new();
        let start = Instant::now();
        scene.scroll(3, start);
        let layer = layer_at(&mut scene, &mut cache, start);
        let content = CellRect::content_of(&panes().panes[0]).pixels(metrics);
        let bottom_row = PixelRect {
            top: content.bottom - metrics.height as i32,
            ..content
        };
        assert!(
            layer.rects.iter().any(|rect| bottom_row
                .intersection(&area(rect.x, rect.y, rect.width, rect.height))
                .is_some()),
            "the strip must paint the bottom row at the start of the slide"
        );
    }

    #[test]
    fn the_rows_under_an_animation_are_left_to_the_layer() {
        let mut cache = cache();
        let metrics = cache.metrics();
        let mut scene = Scene::new();
        let start = Instant::now();
        scene.scroll(3, start);
        let held = scene.scroll.held_out(&scene.state);
        assert_eq!(held.len(), 1);
        let regions = held[0].regions(metrics);

        let mut retained = RetainedScene::new();
        retained.set_held_out(held);
        retained.refresh(
            &scene.state,
            &mut cache,
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            None,
            None,
        );
        let mut drawn_elsewhere = 0;
        for row in retained.rows() {
            for piece in row
                .rects
                .iter()
                .map(|rect| area(rect.x, rect.y, rect.width, rect.height))
                .chain(
                    row.glyphs
                        .iter()
                        .map(|glyph| area(glyph.x, glyph.y, glyph.entry.width, glyph.entry.height)),
                )
            {
                assert!(
                    regions
                        .iter()
                        .all(|region| region.intersection(&piece).is_none()),
                    "{:?} was drawn in place under the animation",
                    piece
                );
                drawn_elsewhere += 1;
            }
        }
        assert!(drawn_elsewhere > 0, "the other panes must still be drawn");

        retained.set_held_out(Vec::new());
        retained.refresh(
            &scene.state,
            &mut cache,
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            None,
            None,
        );
        assert_eq!(
            retained.flatten(),
            crate::scene::build(&scene.state, &mut cache),
            "ending the animation restores the plain rows"
        );
    }

    #[test]
    fn no_layer_is_built_while_nothing_animates() {
        let mut cache = cache();
        let mut scene = Scene::new();
        let layer = layer_at(&mut scene, &mut cache, Instant::now());
        assert!(layer.is_empty());
        assert!(scene.scroll.held_out(&scene.state).is_empty());
    }

    #[test]
    fn a_glyph_is_trimmed_along_with_its_texture_coordinates() {
        let glyph = GlyphQuad {
            x: 10,
            y: 10,
            entry: crate::atlas::AtlasEntry {
                x: 100,
                y: 200,
                width: 8,
                height: 16,
                left: 0,
                top: 0,
            },
            color: [1, 2, 3],
        };
        let region = PixelRect {
            left: 12,
            top: 0,
            right: 100,
            bottom: 20,
        };
        let clipped = clip_glyph(&glyph, -4, &region).unwrap();
        assert_eq!((clipped.x, clipped.y), (12, 6));
        assert_eq!(
            (
                clipped.entry.x,
                clipped.entry.y,
                clipped.entry.width,
                clipped.entry.height
            ),
            (102, 200, 6, 14)
        );
        assert!(clip_glyph(&glyph, 40, &region).is_none());
    }
}

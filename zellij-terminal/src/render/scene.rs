use std::rc::Rc;
use std::sync::Arc;

use unicode_width::UnicodeWidthStr;
use zellij_utils::structured_render::{
    GeometryRecord, WireCell, WireColor, ATTR_BOLD, ATTR_DIM, ATTR_GROUP_CONTINUE, ATTR_HIDDEN,
    ATTR_ITALIC, ATTR_REVERSE, LINK_NONE, UNDERLINE_SHIFT, UNDERLINE_STRAIGHT,
};

use crate::emulation::color::{self, Contrast, Paints, Srgb};
use crate::emulation::kitty::Image;
use crate::emulation::screen_buffer::{CursorShape, Occupancy, TermSize};
use crate::emulation::selection::Selection;
use crate::emulation::terminal::TerminalState;
use crate::input::composition::Preedit;
use crate::input::links::LinkRun;
use crate::render::atlas::{AtlasEntry, GlyphCache};
use crate::text::font::{CellMetrics, FaceStyle, FontId};
use zellij_utils::input::terminal::OpacityMode;

mod decorations;
mod glyphs;
mod images;
mod margins;
mod runs;
mod units;

use decorations::{push_cursor, push_decorations, push_hollow_cursor, push_hover_underline};
use glyphs::{push_glyphs, spill_room};
use runs::{next_run, push_run, run_key, ShapedRun};
use units::{mark_units, push_unit, UnitCols};

pub use images::{image_area, images_of, resident_images};
pub use margins::{padding_rects, Margins};

const OPAQUE: Srgb = [255, 255, 255];
const WHOLE_GRID: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub color: Srgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphQuad {
    pub x: i32,
    pub y: i32,
    pub entry: AtlasEntry,
    pub color: Srgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImageKey {
    Sixel(u64),
    Kitty(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageQuad {
    pub key: ImageKey,
    pub revision: u64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub source_x: u32,
    pub source_y: u32,
    pub z: i32,
    pub image: Rc<Image>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowScene {
    pub rects: Vec<Rect>,
    pub glyphs: Vec<GlyphQuad>,
    pub color_glyphs: Vec<GlyphQuad>,
}

impl RowScene {
    fn clear(&mut self) {
        self.rects.clear();
        self.glyphs.clear();
        self.color_glyphs.clear();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct CellPaint {
    foreground: Srgb,
    background: Srgb,
    tint: Srgb,
    style: FaceStyle,
    draw_glyph: bool,
    hidden: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct RunKey {
    style: FaceStyle,
    region: usize,
    font: FontId,
}

struct Run {
    start: usize,
    end: usize,
    text: String,
    font: FontId,
}

impl Run {
    fn refilled(&mut self, cells: &[RowCell], start: usize, end: usize, key: RunKey) {
        self.start = start;
        self.end = end;
        self.text.clear();
        self.text.extend(
            cells[start..=end]
                .iter()
                .map(|entry| entry.cell.character()),
        );
        self.font = key.font;
    }
}

struct RowCell {
    cell: WireCell,
    occupancy: Occupancy,
    paint: CellPaint,
    explicit_background: bool,
    on_cursor: bool,
    inverted_by_cursor: bool,
    composing: bool,
    held: bool,
    cluster: bool,
    continuation: bool,
    complex: bool,
    unit_end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRect {
    pub x: usize,
    pub y: usize,
    pub cols: usize,
    pub rows: usize,
}

impl CellRect {
    pub fn content_of(pane: &zellij_utils::structured_render::PaneRect) -> Self {
        Self {
            x: pane.content_x() as usize,
            y: pane.content_y() as usize,
            cols: pane.content_cols() as usize,
            rows: pane.content_rows() as usize,
        }
    }

    pub fn outer_of(pane: &zellij_utils::structured_render::PaneRect) -> Self {
        Self {
            x: pane.x as usize,
            y: pane.y as usize,
            cols: pane.cols as usize,
            rows: pane.rows as usize,
        }
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        row >= self.y && row < self.y + self.rows && col >= self.x && col < self.x + self.cols
    }

    pub fn overlaps(&self, other: &CellRect) -> bool {
        self.x < other.x + other.cols
            && other.x < self.x + self.cols
            && self.y < other.y + other.rows
            && other.y < self.y + self.rows
    }

    pub fn pixels(&self, metrics: CellMetrics) -> PixelRect {
        PixelRect {
            left: (self.x as u32 * metrics.width) as i32,
            top: (self.y as u32 * metrics.height) as i32,
            right: ((self.x + self.cols) as u32 * metrics.width) as i32,
            bottom: ((self.y + self.rows) as u32 * metrics.height) as i32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PixelRect {
    pub fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    pub fn intersection(&self, other: &PixelRect) -> Option<PixelRect> {
        let clipped = PixelRect {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        (!clipped.is_empty()).then_some(clipped)
    }

    pub fn subtract(&self, cut: &PixelRect, out: &mut Vec<PixelRect>) {
        let Some(overlap) = self.intersection(cut) else {
            out.push(*self);
            return;
        };
        let pieces = [
            PixelRect {
                bottom: overlap.top,
                ..*self
            },
            PixelRect {
                top: overlap.bottom,
                ..*self
            },
            PixelRect {
                top: overlap.top,
                bottom: overlap.bottom,
                right: overlap.left,
                ..*self
            },
            PixelRect {
                top: overlap.top,
                bottom: overlap.bottom,
                left: overlap.right,
                ..*self
            },
        ];
        out.extend(pieces.into_iter().filter(|piece| !piece.is_empty()));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldOut {
    pub content: CellRect,
    pub covers: Vec<CellRect>,
}

impl HeldOut {
    pub fn of(geometry: &GeometryRecord, index: usize) -> Option<Self> {
        let pane = geometry.panes.get(index)?;
        let content = CellRect::content_of(pane);
        let covers = geometry.panes[index + 1..]
            .iter()
            .map(CellRect::outer_of)
            .filter(|cover| cover.overlaps(&content))
            .collect();
        Some(Self { content, covers })
    }

    pub fn holds(&self, row: usize, col: usize) -> bool {
        self.content.contains(row, col) && !self.covers.iter().any(|cover| cover.contains(row, col))
    }

    pub fn regions(&self, metrics: CellMetrics) -> Vec<PixelRect> {
        let mut regions = vec![self.content.pixels(metrics)];
        let mut next = Vec::new();
        for cover in &self.covers {
            let cut = cover.pixels(metrics);
            next.clear();
            for region in &regions {
                region.subtract(&cut, &mut next);
            }
            std::mem::swap(&mut regions, &mut next);
        }
        regions
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transparency {
    pub opacity: f32,
    pub mode: OpacityMode,
}

impl Default for Transparency {
    fn default() -> Self {
        Self::OPAQUE
    }
}

impl Transparency {
    pub const OPAQUE: Self = Self {
        opacity: 1.0,
        mode: OpacityMode::Background,
    };

    pub fn see_through_background(&self) -> bool {
        self.mode == OpacityMode::Background && self.opacity < 1.0
    }

    pub fn clear_alpha(&self) -> f32 {
        if self.see_through_background() {
            self.opacity
        } else {
            1.0
        }
    }

    pub fn fade(&self) -> Option<f32> {
        (self.mode == OpacityMode::Everything && self.opacity < 1.0).then_some(self.opacity)
    }
}

pub struct PreeditOverlay {
    row: usize,
    start: usize,
    cells: Vec<(WireCell, Occupancy, Option<Arc<str>>)>,
    cursor: Option<usize>,
}

impl PreeditOverlay {
    fn of(state: &TerminalState, preedit: &Preedit, row: usize, col: usize) -> Option<Self> {
        let size = state.size();
        if row >= size.rows || col >= size.cols {
            return None;
        }
        let base = state.screen().cell(row, col);
        let mut cells: Vec<(WireCell, Occupancy, Option<Arc<str>>)> = Vec::new();
        for column in preedit.columns() {
            if col + cells.len() + column.width > size.cols {
                break;
            }
            let continues = if column.continues {
                ATTR_GROUP_CONTINUE
            } else {
                0
            };
            let head = WireCell {
                ch: column.character() as u32,
                underline_color: 0,
                attrs: (UNDERLINE_STRAIGHT << UNDERLINE_SHIFT) | continues,
                width: column.width as u8,
                link: LINK_NONE,
                ..base
            };
            let mut text = column.text.clone();
            if column.width < UnicodeWidthStr::width(text.as_str()) {
                text.retain(|character| character != '\u{fe0f}');
            }
            let cluster = (text.chars().nth(1).is_some()).then(|| Arc::from(text.as_str()));
            if column.width > 1 {
                cells.push((head, Occupancy::WideHead, cluster));
                for _ in 1..column.width {
                    cells.push((
                        WireCell {
                            ch: ' ' as u32,
                            width: 1,
                            attrs: head.attrs & !ATTR_GROUP_CONTINUE,
                            ..head
                        },
                        Occupancy::WideTail,
                        None,
                    ));
                }
            } else {
                cells.push((head, Occupancy::Single, cluster));
            }
        }
        if cells.is_empty() {
            return None;
        }
        let cursor = preedit
            .cursor_column()
            .map(|offset| (col + offset).min(size.cols - 1));
        Some(Self {
            row,
            start: col,
            cells,
            cursor,
        })
    }

    fn at(&self, row: usize, col: usize) -> Option<&(WireCell, Occupancy, Option<Arc<str>>)> {
        if row != self.row {
            return None;
        }
        col.checked_sub(self.start)
            .and_then(|offset| self.cells.get(offset))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CursorOptions {
    pub shape: Option<CursorShape>,
    pub blink: Option<bool>,
    pub hollow: bool,
}

impl CursorOptions {
    pub fn blinks(&self, asked_for: bool) -> bool {
        !self.hollow && self.blink.unwrap_or(asked_for)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlinkPhase {
    On,
    Off,
}

#[derive(Clone, Copy)]
pub struct SceneInputs<'a> {
    pub state: &'a TerminalState,
    pub phase: BlinkPhase,
    pub paints: &'a Paints,
    pub cursor: CursorOptions,
    pub hovered_link: Option<&'a LinkRun>,
    pub preedit: Option<&'a Preedit>,
}

pub struct RowContext<'a> {
    state: &'a TerminalState,
    paints: &'a Paints,
    hovered_link: Option<&'a LinkRun>,
    phase: BlinkPhase,
    pub metrics: CellMetrics,
    underline_stroke: u32,
    pub size: TermSize,
    pub clear: Srgb,
    cursor_row: usize,
    cursor_col: usize,
    cursor_shape: CursorShape,
    cursor_visible: bool,
    cursor_breaks_runs: Option<(usize, usize)>,
    shaping: bool,
    preedit: Option<PreeditOverlay>,
    transparent_background: bool,
    cursor_hollow: bool,
    contrast: Option<&'a Contrast>,
    held_out: &'a [HeldOut],
    only: Option<&'a HeldOut>,
    selection: Option<&'a Selection>,
}

impl<'a> RowContext<'a> {
    pub fn with_held_out(mut self, held_out: &'a [HeldOut]) -> Self {
        self.held_out = held_out;
        self
    }

    pub fn with_selection(mut self, selection: Option<&'a Selection>) -> Self {
        self.selection = selection;
        self
    }

    fn cluster_at(&self, row: usize, col: usize) -> Option<Arc<str>> {
        if let Some((_, _, cluster)) = self
            .preedit
            .as_ref()
            .and_then(|overlay| overlay.at(row, col))
        {
            return cluster.clone();
        }
        self.state.screen().cluster_at(row, col).cloned()
    }

    fn push_text_to(&self, row: usize, col: usize, out: &mut String) {
        if let Some((cell, _, cluster)) = self
            .preedit
            .as_ref()
            .and_then(|overlay| overlay.at(row, col))
        {
            match cluster {
                Some(text) => out.push_str(text),
                None => out.push(cell.character()),
            }
            return;
        }
        self.state.screen().push_text_to(row, col, out);
    }

    fn selects(&self, row: usize, col: usize) -> bool {
        self.selection
            .is_some_and(|selection| selection.covers(row, col))
    }

    pub fn with_only(mut self, only: &'a HeldOut) -> Self {
        self.only = Some(only);
        self
    }

    fn holds(&self, row: usize, col: usize) -> bool {
        if let Some(only) = self.only {
            if !only.holds(row, col) {
                return true;
            }
        }
        !self.held_out.is_empty() && self.held_out.iter().any(|held| held.holds(row, col))
    }

    pub fn with_transparency(mut self, transparency: Transparency) -> Self {
        self.transparent_background = transparency.see_through_background();
        self
    }

    pub fn with_contrast(mut self, contrast: &'a Contrast) -> Self {
        self.contrast = contrast.is_active().then_some(contrast);
        self
    }

    pub fn new(inputs: SceneInputs<'a>, cache: &GlyphCache) -> Self {
        let SceneInputs {
            state,
            phase,
            paints,
            cursor,
            hovered_link,
            preedit,
        } = inputs;
        let (cursor_row, base_col) = state.cursor_position();
        let blinked_away = phase == BlinkPhase::Off && cursor.blinks(state.cursor_is_blinking());
        let preedit =
            preedit.and_then(|preedit| PreeditOverlay::of(state, preedit, cursor_row, base_col));
        let cursor_col = match preedit.as_ref() {
            Some(overlay) => overlay.cursor.unwrap_or(base_col),
            None => base_col,
        };
        let cursor_visible = state.cursor_is_visible()
            && !blinked_away
            && preedit
                .as_ref()
                .map(|overlay| overlay.cursor.is_some())
                .unwrap_or(true);
        Self {
            state,
            paints,
            hovered_link,
            phase,
            metrics: cache.metrics(),
            underline_stroke: cache.underline_stroke(),
            size: state.size(),
            clear: paints.background,
            cursor_row,
            cursor_col,
            cursor_shape: state.cursor_shape().or(cursor.shape),
            cursor_visible,
            cursor_breaks_runs: state
                .cursor_is_visible()
                .then_some((cursor_row, cursor_col)),
            shaping: cache.shapes_runs(),
            preedit,
            transparent_background: false,
            cursor_hollow: cursor.hollow,
            contrast: None,
            held_out: &[],
            only: None,
            selection: None,
        }
    }

    pub fn width(&self) -> u32 {
        self.metrics.width * self.size.cols as u32
    }

    pub fn height(&self) -> u32 {
        self.metrics.height * self.size.rows as u32
    }
}

#[derive(Default)]
pub struct RowScratch {
    row_paints: Vec<RowCell>,
    run_keys: Vec<Option<RunKey>>,
    shaped: Vec<bool>,
    text: String,
    unit_text: String,
}

pub fn build_row(
    context: &RowContext<'_>,
    cache: &mut GlyphCache,
    row: usize,
    scratch: &mut RowScratch,
    out: &mut RowScene,
) {
    out.clear();
    if row >= context.size.rows {
        return;
    }

    let RowScratch {
        row_paints,
        run_keys,
        shaped,
        text,
        unit_text,
    } = scratch;
    let (state, paints, metrics) = (context.state, context.paints, context.metrics);
    let origin_y = (row as u32 * metrics.height) as i32;

    row_paints.clear();
    for col in 0..context.size.cols {
        let composed = context
            .preedit
            .as_ref()
            .and_then(|overlay| overlay.at(row, col));
        let (cell, occupancy, cluster) = match composed {
            Some((cell, occupancy, cluster)) => (*cell, *occupancy, cluster.is_some()),
            None => (
                state.screen().cell(row, col),
                state.occupancy(row, col),
                state.screen().cluster_at(row, col).is_some(),
            ),
        };
        let on_cursor =
            context.cursor_visible && context.cursor_row == row && context.cursor_col == col;
        let inverted_by_cursor =
            on_cursor && context.cursor_shape == CursorShape::Block && !context.cursor_hollow;

        let blinked_out = context.phase == BlinkPhase::Off
            && composed.is_none()
            && state.blink_at(row, col).is_set()
            && !on_cursor;
        let mut paint = paint_of(cell, occupancy, paints, context.contrast);
        if blinked_out {
            paint.foreground = paint.background;
            paint.hidden = true;
        }
        if inverted_by_cursor {
            let glyph = paint.background;
            paint.background = paints.cursor_over(paint.foreground);
            paint.foreground = match context.contrast {
                Some(contrast) if !paint.hidden => contrast.apply(glyph, paint.background),
                _ => glyph,
            };
        }
        let selected = context.selects(row, col);
        if selected && !inverted_by_cursor {
            std::mem::swap(&mut paint.foreground, &mut paint.background);
        }
        let explicit_background = inverted_by_cursor
            || selected
            || cell.has(ATTR_REVERSE)
            || !matches!(WireColor::unpack(cell.bg), WireColor::Default);
        if context.transparent_background && paint.hidden && !explicit_background {
            paint.draw_glyph = false;
        }

        row_paints.push(RowCell {
            cell,
            occupancy,
            paint,
            explicit_background,
            on_cursor,
            inverted_by_cursor,
            composing: composed.is_some(),
            held: context.holds(row, col),
            cluster,
            continuation: occupancy != Occupancy::WideTail && cell.is_group_continuation(),
            complex: false,
            unit_end: 0,
        });
    }
    mark_units(row_paints);

    shaped.clear();
    shaped.resize(context.size.cols, false);
    if context.shaping {
        run_keys.clear();
        for col in 0..context.size.cols {
            run_keys.push(run_key(
                cache,
                &row_paints[col],
                state.geometry(),
                row,
                col,
                context.cursor_breaks_runs,
            ));
        }
        let mut run = Run {
            start: 0,
            end: 0,
            text: std::mem::take(text),
            font: cache.primary_of(FaceStyle::Regular),
        };
        let mut at = 0;
        while let Some((start, end, key)) = next_run(run_keys, at) {
            run.refilled(row_paints, start, end, key);
            let glyphs = cache.shape(run.font, &run.text);
            let shaped_run = ShapedRun {
                run: &run,
                glyphs: &glyphs,
            };
            push_run(
                out, cache, shaped_run, row_paints, origin_y, metrics, shaped,
            );
            at = end + 1;
        }
        *text = std::mem::take(&mut run.text);
    }

    for col in 0..context.size.cols {
        let RowCell {
            cell,
            paint,
            explicit_background,
            on_cursor,
            inverted_by_cursor,
            composing,
            held,
            complex,
            unit_end,
            ..
        } = row_paints[col];
        if held {
            continue;
        }

        let origin_x = (col as u32 * metrics.width) as i32;
        let fills = if context.transparent_background {
            explicit_background
        } else {
            paint.background != context.clear
        };
        let inkless = context.transparent_background && paint.hidden && !explicit_background;

        if fills {
            out.rects.push(Rect {
                x: origin_x,
                y: origin_y,
                width: metrics.width,
                height: metrics.height,
                color: paint.background,
            });
        }

        if paint.draw_glyph && !shaped[col] {
            if complex {
                if unit_end > col {
                    let unit = UnitCols {
                        row,
                        start: col,
                        end: unit_end,
                    };
                    push_unit(out, context, cache, row_paints, unit_text, unit, origin_y);
                }
            } else {
                let spill = spill_room(row_paints, state.geometry(), row, col);
                push_glyphs(
                    out,
                    cache,
                    &row_paints[col],
                    (origin_x, origin_y),
                    metrics,
                    spill,
                );
            }
        }

        push_decorations(out, context, cell, &paint, (origin_x, origin_y), inkless);

        if !composing && !inkless && context.hovered_link.is_some_and(|run| run.covers(row, col)) {
            push_hover_underline(
                out,
                &paint,
                origin_x,
                origin_y,
                metrics,
                context.underline_stroke,
            );
        }

        if on_cursor && context.cursor_hollow {
            push_hollow_cursor(
                out,
                origin_x,
                origin_y,
                metrics,
                paints.cursor_over(paint.foreground),
            );
        } else if on_cursor && !inverted_by_cursor {
            push_cursor(
                out,
                context.cursor_shape,
                origin_x,
                origin_y,
                metrics,
                paints.beam_cursor(),
            );
        }
    }
}

fn paint_of(
    cell: WireCell,
    occupancy: Occupancy,
    paints: &Paints,
    contrast: Option<&Contrast>,
) -> CellPaint {
    let mut foreground = paints.foreground(cell.fg);
    let mut background = paints.background(cell.bg);
    let mut tint = OPAQUE;

    if cell.has(ATTR_DIM) {
        foreground = color::dim(foreground);
        tint = color::dim(tint);
    }
    if cell.has(ATTR_REVERSE) {
        std::mem::swap(&mut foreground, &mut background);
    }
    let hidden = cell.has(ATTR_HIDDEN);
    if hidden {
        foreground = background;
    } else if let Some(contrast) = contrast {
        foreground = contrast.apply(foreground, background);
    }

    CellPaint {
        foreground,
        background,
        tint,
        style: FaceStyle::of(cell.has(ATTR_BOLD), cell.has(ATTR_ITALIC)),
        draw_glyph: occupancy != Occupancy::WideTail,
        hidden,
    }
}

#[cfg(test)]
pub(crate) mod test_helpers;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ligature_tests;

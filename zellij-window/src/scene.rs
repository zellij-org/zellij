use std::rc::Rc;

use zellij_utils::structured_render::{
    GeometryRecord, WireCell, WireColor, ATTR_BOLD, ATTR_DIM, ATTR_HIDDEN, ATTR_ITALIC,
    ATTR_REVERSE, ATTR_STRIKE, LINK_NONE, UNDERLINE_CURLY, UNDERLINE_DASHED, UNDERLINE_DOTTED,
    UNDERLINE_DOUBLE, UNDERLINE_SHIFT, UNDERLINE_STRAIGHT,
};

use crate::atlas::{AtlasEntry, GlyphCache, Lookup};
use crate::color::{self, Paints, Srgb};
use crate::composition::Preedit;
use crate::font::{CellMetrics, FaceStyle, FontId, GlyphContent, ShapedGlyph};
use crate::kitty::Image;
use crate::links::LinkRun;
use crate::screen_buffer::{CursorShape, Occupancy, TermSize};
use crate::terminal::TerminalState;

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

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub clear: Srgb,
    pub rects: Vec<Rect>,
    pub glyphs: Vec<GlyphQuad>,
    pub color_glyphs: Vec<GlyphQuad>,
    pub images: Vec<ImageQuad>,
    pub resident_images: Vec<ImageKey>,
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
    on_cursor: bool,
    inverted_by_cursor: bool,
    composing: bool,
}

pub struct PreeditOverlay {
    row: usize,
    start: usize,
    cells: Vec<(WireCell, Occupancy)>,
    cursor: Option<usize>,
}

impl PreeditOverlay {
    fn of(state: &TerminalState, preedit: &Preedit, row: usize, col: usize) -> Option<Self> {
        let size = state.size();
        if row >= size.rows || col >= size.cols {
            return None;
        }
        let base = state.screen().cell(row, col);
        let mut cells: Vec<(WireCell, Occupancy)> = Vec::new();
        for column in preedit.columns() {
            if col + cells.len() + column.width > size.cols {
                break;
            }
            let head = WireCell {
                ch: column.character as u32,
                underline_color: 0,
                attrs: UNDERLINE_STRAIGHT << UNDERLINE_SHIFT,
                width: column.width as u8,
                link: LINK_NONE,
                ..base
            };
            if column.width > 1 {
                cells.push((head, Occupancy::WideHead));
                cells.push((
                    WireCell {
                        ch: ' ' as u32,
                        width: 1,
                        ..head
                    },
                    Occupancy::WideTail,
                ));
            } else {
                cells.push((head, Occupancy::Single));
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

    fn at(&self, row: usize, col: usize) -> Option<(WireCell, Occupancy)> {
        if row != self.row {
            return None;
        }
        col.checked_sub(self.start)
            .and_then(|offset| self.cells.get(offset))
            .copied()
    }
}

#[cfg(test)]
pub fn build(state: &TerminalState, cache: &mut GlyphCache) -> Scene {
    build_at(
        state,
        cache,
        BlinkPhase::On,
        &Paints::default(),
        CursorOptions::default(),
        None,
        None,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CursorOptions {
    pub shape: Option<CursorShape>,
    pub blink: Option<bool>,
}

impl CursorOptions {
    pub fn blinks(&self, asked_for: bool) -> bool {
        self.blink.unwrap_or(asked_for)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlinkPhase {
    On,
    Off,
}

#[cfg(test)]
pub fn build_at(
    state: &TerminalState,
    cache: &mut GlyphCache,
    phase: BlinkPhase,
    paints: &Paints,
    cursor: CursorOptions,
    hovered_link: Option<&LinkRun>,
    preedit: Option<&Preedit>,
) -> Scene {
    let scene = build_once(state, cache, phase, paints, cursor, hovered_link, preedit);
    if !cache.exhausted() {
        return scene;
    }
    cache.flush();
    build_once(state, cache, phase, paints, cursor, hovered_link, preedit)
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn build_once(
    state: &TerminalState,
    cache: &mut GlyphCache,
    phase: BlinkPhase,
    paints: &Paints,
    cursor: CursorOptions,
    hovered_link: Option<&LinkRun>,
    preedit: Option<&Preedit>,
) -> Scene {
    let context = RowContext::new(state, cache, phase, paints, cursor, hovered_link, preedit);
    let mut scene = Scene {
        width: context.width(),
        height: context.height(),
        clear: context.clear,
        rects: Vec::new(),
        glyphs: Vec::new(),
        color_glyphs: Vec::new(),
        images: images_of(state, context.metrics),
        resident_images: resident_images(state),
    };

    let mut scratch = RowScratch::default();
    let mut row_scene = RowScene::default();
    for row in 0..context.size.rows {
        build_row(&context, cache, row, &mut scratch, &mut row_scene);
        scene.rects.append(&mut row_scene.rects);
        scene.glyphs.append(&mut row_scene.glyphs);
        scene.color_glyphs.append(&mut row_scene.color_glyphs);
    }

    scene
}

pub struct RowContext<'a> {
    state: &'a TerminalState,
    paints: &'a Paints,
    hovered_link: Option<&'a LinkRun>,
    phase: BlinkPhase,
    pub metrics: CellMetrics,
    pub size: TermSize,
    pub clear: Srgb,
    cursor_row: usize,
    cursor_col: usize,
    cursor_shape: CursorShape,
    cursor_visible: bool,
    cursor_breaks_runs: Option<(usize, usize)>,
    shaping: bool,
    preedit: Option<PreeditOverlay>,
}

impl<'a> RowContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        state: &'a TerminalState,
        cache: &GlyphCache,
        phase: BlinkPhase,
        paints: &'a Paints,
        cursor: CursorOptions,
        hovered_link: Option<&'a LinkRun>,
        preedit: Option<&Preedit>,
    ) -> Self {
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
    } = scratch;
    let (state, paints, metrics) = (context.state, context.paints, context.metrics);
    let origin_y = (row as u32 * metrics.height) as i32;

    row_paints.clear();
    for col in 0..context.size.cols {
        let composed = context
            .preedit
            .as_ref()
            .and_then(|overlay| overlay.at(row, col));
        let (cell, occupancy) = match composed {
            Some(composed) => composed,
            None => (state.screen().cell(row, col), state.occupancy(row, col)),
        };
        let on_cursor =
            context.cursor_visible && context.cursor_row == row && context.cursor_col == col;
        let inverted_by_cursor = on_cursor && context.cursor_shape == CursorShape::Block;

        let blinked_out = context.phase == BlinkPhase::Off
            && composed.is_none()
            && state.blink_at(row, col).is_set()
            && !on_cursor;
        let mut paint = paint_of(cell, occupancy, paints);
        if blinked_out {
            paint.foreground = paint.background;
            paint.hidden = true;
        }
        if inverted_by_cursor {
            let glyph = paint.background;
            paint.background = paints.cursor_over(paint.foreground);
            paint.foreground = glyph;
        }

        row_paints.push(RowCell {
            cell,
            occupancy,
            paint,
            on_cursor,
            inverted_by_cursor,
            composing: composed.is_some(),
        });
    }

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
            push_run(
                out, cache, &run, &glyphs, row_paints, origin_y, metrics, shaped,
            );
            at = end + 1;
        }
        *text = std::mem::take(&mut run.text);
    }

    for col in 0..context.size.cols {
        let RowCell {
            cell,
            occupancy,
            paint,
            on_cursor,
            inverted_by_cursor,
            composing,
        } = row_paints[col];

        let origin_x = (col as u32 * metrics.width) as i32;

        if paint.background != context.clear {
            out.rects.push(Rect {
                x: origin_x,
                y: origin_y,
                width: metrics.width,
                height: metrics.height,
                color: paint.background,
            });
        }

        if paint.draw_glyph && !shaped[col] {
            let spill = spill_room(row_paints, state.geometry(), row, col);
            push_glyphs(
                out, cache, cell, occupancy, &paint, origin_x, origin_y, metrics, spill,
            );
        }

        push_decorations(out, cell, &paint, origin_x, origin_y, metrics, paints);

        if !composing && context.hovered_link.is_some_and(|run| run.covers(row, col)) {
            push_hover_underline(out, &paint, origin_x, origin_y, metrics);
        }

        if on_cursor && !inverted_by_cursor {
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

fn run_key(
    cache: &mut GlyphCache,
    entry: &RowCell,
    geometry: &GeometryRecord,
    row: usize,
    col: usize,
    cursor: Option<(usize, usize)>,
) -> Option<RunKey> {
    if entry.occupancy != Occupancy::Single || !entry.paint.draw_glyph || entry.composing {
        return None;
    }
    let character = entry.cell.character();
    if !is_shapeable(character) {
        return None;
    }
    if cursor == Some((row, col)) {
        return None;
    }
    let region = region_of(geometry, row, col)?;
    let font = cache.face_of(entry.paint.style, character)?;
    if font != cache.primary_of(entry.paint.style) {
        return None;
    }
    Some(RunKey {
        style: entry.paint.style,
        region,
        font,
    })
}

fn is_shapeable(character: char) -> bool {
    character.is_ascii_graphic()
}

fn region_of(geometry: &GeometryRecord, row: usize, col: usize) -> Option<usize> {
    if geometry.panes.is_empty() {
        return Some(WHOLE_GRID);
    }
    let x = u16::try_from(col).ok()?;
    let y = u16::try_from(row).ok()?;
    geometry
        .panes
        .iter()
        .rposition(|pane| pane.content_contains(x, y))
}

fn next_run(keys: &[Option<RunKey>], from: usize) -> Option<(usize, usize, RunKey)> {
    let mut start = from;
    while start < keys.len() {
        let Some(key) = keys[start] else {
            start += 1;
            continue;
        };
        let mut end = start;
        while end + 1 < keys.len() && keys[end + 1] == Some(key) {
            end += 1;
        }
        if end > start {
            return Some((start, end, key));
        }
        start = end + 1;
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn push_run(
    out: &mut RowScene,
    cache: &mut GlyphCache,
    run: &Run,
    glyphs: &[ShapedGlyph],
    cells: &[RowCell],
    origin_y: i32,
    metrics: CellMetrics,
    shaped: &mut [bool],
) {
    let width = run.end - run.start + 1;
    if !tiles_the_run(glyphs, width) {
        return;
    }

    let left = (run.start as u32 * metrics.width) as i32;
    let right = ((run.end + 1) as u32 * metrics.width) as i32;
    let baseline = origin_y + metrics.baseline as i32;
    let one_paint = cells[run.start..=run.end]
        .iter()
        .all(|entry| entry.paint == cells[run.start].paint)
        .then(|| cells[run.start].paint);

    for glyph in glyphs.iter() {
        let Lookup::Rendered(rendered) = cache.shaped_glyph(run.font, glyph.glyph, 1) else {
            continue;
        };
        let pen = left + (glyph.cell as u32 * metrics.width) as i32 + glyph.dx;
        let y = baseline - glyph.dy - rendered.entry.top;
        let x = pen + rendered.entry.left;

        match one_paint {
            Some(paint) => {
                if let Some((x, entry)) = clipped(rendered.entry, x, left, right) {
                    push_run_glyph(out, x, y, entry, rendered.content, &paint);
                }
            },
            None => {
                let touched = touched_cells(x, rendered.entry.width, metrics.width, run);
                for col in touched {
                    let cell_left = (col as u32 * metrics.width) as i32;
                    let cell_right = cell_left + metrics.width as i32;
                    if let Some((x, entry)) = clipped(rendered.entry, x, cell_left, cell_right) {
                        push_run_glyph(out, x, y, entry, rendered.content, &cells[col].paint);
                    }
                }
            },
        }
    }

    for handled in shaped[run.start..=run.end].iter_mut() {
        *handled = true;
    }
}

fn touched_cells(
    x: i32,
    width: u32,
    cell_width: u32,
    run: &Run,
) -> std::ops::RangeInclusive<usize> {
    let first = (x.max(0) as u32 / cell_width) as usize;
    let last = ((x + width as i32 - 1).max(0) as u32 / cell_width) as usize;
    first.max(run.start)..=last.min(run.end)
}

fn push_run_glyph(
    out: &mut RowScene,
    x: i32,
    y: i32,
    entry: AtlasEntry,
    content: GlyphContent,
    paint: &CellPaint,
) {
    let quad = GlyphQuad {
        x,
        y,
        entry,
        color: match content {
            GlyphContent::Mask => paint.foreground,
            GlyphContent::Color => paint.tint,
        },
    };
    match content {
        GlyphContent::Mask => out.glyphs.push(quad),
        GlyphContent::Color if !paint.hidden => out.color_glyphs.push(quad),
        GlyphContent::Color => {},
    }
}

fn tiles_the_run(glyphs: &[ShapedGlyph], cells: usize) -> bool {
    let mut next = 0;
    let mut cluster: Option<usize> = None;
    for glyph in glyphs {
        if cluster == Some(glyph.cell) {
            continue;
        }
        if glyph.cell != next {
            return false;
        }
        next = glyph.cell + glyph.span;
        cluster = Some(glyph.cell);
    }
    next == cells
}

fn clipped(entry: AtlasEntry, x: i32, left: i32, right: i32) -> Option<(i32, AtlasEntry)> {
    let mut entry = entry;
    let mut x = x;
    if x < left {
        let trim = u32::try_from(left - x).ok()?;
        if trim >= entry.width {
            return None;
        }
        entry.x += trim;
        entry.width -= trim;
        x = left;
    }
    let overflow = (x + entry.width as i32) - right;
    if overflow > 0 {
        let trim = u32::try_from(overflow).ok()?;
        if trim >= entry.width {
            return None;
        }
        entry.width -= trim;
    }
    Some((x, entry))
}

#[cfg(test)]
pub fn ligating_sequences(state: &TerminalState, cache: &mut GlyphCache) -> Vec<String> {
    use crate::font::GlyphId;

    let size = state.size();
    let paints = Paints::default();
    let cursor = state.cursor_is_visible().then_some(state.cursor_position());
    let mut found = Vec::new();

    for row in 0..size.rows {
        let cells: Vec<RowCell> = (0..size.cols)
            .map(|col| {
                let cell = state.screen().cell(row, col);
                let occupancy = state.occupancy(row, col);
                RowCell {
                    cell,
                    occupancy,
                    paint: paint_of(cell, occupancy, &paints),
                    on_cursor: false,
                    inverted_by_cursor: false,
                    composing: false,
                }
            })
            .collect();
        let keys: Vec<Option<RunKey>> = (0..size.cols)
            .map(|col| run_key(cache, &cells[col], state.geometry(), row, col, cursor))
            .collect();

        let mut at = 0;
        while let Some((start, end, key)) = next_run(&keys, at) {
            at = end + 1;
            let characters: Vec<char> = cells[start..=end]
                .iter()
                .map(|entry| entry.cell.character())
                .collect();
            let text: String = characters.iter().collect();
            let glyphs = cache.shape(key.font, &text);
            let nominal: Vec<Option<GlyphId>> = characters
                .iter()
                .map(|character| cache.nominal_of(key.style, *character))
                .collect();
            let mut changed = vec![false; characters.len()];
            for glyph in glyphs.iter() {
                let single = glyph.span == 1
                    && glyphs
                        .iter()
                        .filter(|other| other.cell == glyph.cell)
                        .count()
                        == 1;
                if single && nominal.get(glyph.cell) == Some(&Some(glyph.glyph)) {
                    continue;
                }
                for cell in glyph.cell..(glyph.cell + glyph.span).min(changed.len()) {
                    changed[cell] = true;
                }
            }

            let mut from = 0;
            while from < changed.len() {
                if !changed[from] {
                    from += 1;
                    continue;
                }
                let mut to = from;
                while to + 1 < changed.len() && changed[to + 1] {
                    to += 1;
                }
                found.push(characters[from..=to].iter().collect::<String>());
                from = to + 1;
            }
        }
    }

    found.sort();
    found.dedup();
    found
}

pub fn resident_images(state: &TerminalState) -> Vec<ImageKey> {
    let mut resident: Vec<ImageKey> = state
        .sixels()
        .resident_ids()
        .into_iter()
        .map(ImageKey::Sixel)
        .collect();
    resident.extend(
        state
            .graphics()
            .resident_ids()
            .into_iter()
            .map(ImageKey::Kitty),
    );
    resident
}

pub fn clip_to_owning_pane(
    quad: ImageQuad,
    cell_x: usize,
    cell_y: usize,
    geometry: &GeometryRecord,
    metrics: CellMetrics,
) -> Option<ImageQuad> {
    let (Ok(cell_x), Ok(cell_y)) = (u16::try_from(cell_x), u16::try_from(cell_y)) else {
        return Some(quad);
    };
    let Some(pane) = geometry.content_pane_at(cell_x, cell_y) else {
        return Some(quad);
    };
    let left = pane.content_x() as i64 * metrics.width as i64;
    let top = pane.content_y() as i64 * metrics.height as i64;
    let right = left + pane.content_cols() as i64 * metrics.width as i64;
    let bottom = top + pane.content_rows() as i64 * metrics.height as i64;

    let kept_left = (quad.x as i64).max(left);
    let kept_top = (quad.y as i64).max(top);
    let kept_right = (quad.x as i64 + quad.width as i64).min(right);
    let kept_bottom = (quad.y as i64 + quad.height as i64).min(bottom);
    if kept_right <= kept_left || kept_bottom <= kept_top {
        return None;
    }
    Some(ImageQuad {
        x: kept_left as i32,
        y: kept_top as i32,
        width: (kept_right - kept_left) as u32,
        height: (kept_bottom - kept_top) as u32,
        source_x: quad.source_x + (kept_left - quad.x as i64) as u32,
        source_y: quad.source_y + (kept_top - quad.y as i64) as u32,
        ..quad
    })
}

pub fn images_of(state: &TerminalState, metrics: CellMetrics) -> Vec<ImageQuad> {
    let graphics = state.graphics();
    let geometry = state.geometry();
    let mut quads: Vec<ImageQuad> = state
        .sixels()
        .chunks()
        .filter_map(|chunk| {
            clip_to_owning_pane(
                ImageQuad {
                    key: ImageKey::Sixel(chunk.id),
                    revision: chunk.revision,
                    x: (chunk.cell_x as u32 * metrics.width) as i32,
                    y: (chunk.cell_y as u32 * metrics.height) as i32,
                    width: chunk.width,
                    height: chunk.height,
                    source_x: 0,
                    source_y: 0,
                    z: 0,
                    image: Rc::clone(&chunk.image),
                },
                chunk.cell_x,
                chunk.cell_y,
                geometry,
                metrics,
            )
        })
        .collect();
    quads.extend(graphics.placements().filter_map(|placement| {
        let image = graphics.image(placement.image_id)?;
        let revision = graphics.revision(placement.image_id)?;
        clip_to_owning_pane(
            ImageQuad {
                key: ImageKey::Kitty(placement.image_id),
                revision,
                x: (placement.cell_x as u32 * metrics.width + placement.offset_x) as i32,
                y: (placement.cell_y as u32 * metrics.height + placement.offset_y) as i32,
                width: placement.source_width,
                height: placement.source_height,
                source_x: placement.source_x,
                source_y: placement.source_y,
                z: placement.z,
                image: Rc::clone(image),
            },
            placement.cell_x,
            placement.cell_y,
            geometry,
            metrics,
        )
    }));
    quads.sort_by_key(|quad| (quad.z, quad.key));
    quads
}

fn paint_of(cell: WireCell, occupancy: Occupancy, paints: &Paints) -> CellPaint {
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

#[allow(clippy::too_many_arguments)]
fn push_glyphs(
    out: &mut RowScene,
    cache: &mut GlyphCache,
    cell: WireCell,
    occupancy: Occupancy,
    paint: &CellPaint,
    origin_x: i32,
    origin_y: i32,
    metrics: CellMetrics,
    spill: u32,
) {
    let baseline = origin_y + metrics.baseline as i32;
    let budget = if occupancy == Occupancy::WideHead {
        2
    } else {
        1
    };
    let character = cell.character();
    if is_invisible(character) {
        return;
    }
    match cache.glyph_with_room(paint.style, character, budget, spill) {
        Lookup::Rendered(rendered) => {
            let entry = rendered.entry;
            let quad = GlyphQuad {
                x: origin_x + entry.left,
                y: baseline - entry.top,
                entry,
                color: match rendered.content {
                    GlyphContent::Mask => paint.foreground,
                    GlyphContent::Color => paint.tint,
                },
            };
            match rendered.content {
                GlyphContent::Mask => out.glyphs.push(quad),
                GlyphContent::Color if !paint.hidden => out.color_glyphs.push(quad),
                GlyphContent::Color => {},
            }
        },
        Lookup::Blank => {},
        Lookup::Missing => push_tofu(out, paint, origin_x, origin_y, metrics, budget),
    }
}

fn spill_room(cells: &[RowCell], geometry: &GeometryRecord, row: usize, col: usize) -> u32 {
    let Some(next) = cells.get(col + 1) else {
        return 0;
    };
    let spreads = cells[col].occupancy == Occupancy::Single
        && next.occupancy == Occupancy::Single
        && !next.composing
        && next.cell.character() == ' '
        && region_of(geometry, row, col).is_some()
        && region_of(geometry, row, col) == region_of(geometry, row, col + 1);
    u32::from(spreads)
}

fn is_invisible(character: char) -> bool {
    if character == ' ' || character.is_control() {
        return true;
    }
    matches!(character,
        '\u{ad}' | '\u{34f}' | '\u{61c}' | '\u{feff}'
        | '\u{200b}'..='\u{200f}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{2069}'
        | '\u{3000}'
        | '\u{fe00}'..='\u{fe0f}'
        | '\u{e0100}'..='\u{e01ef}')
}

fn push_tofu(
    out: &mut RowScene,
    paint: &CellPaint,
    origin_x: i32,
    origin_y: i32,
    metrics: CellMetrics,
    budget: u32,
) {
    let stroke = metrics.stroke.max(1);
    let inset = stroke as i32;
    let width = (metrics.width * budget).saturating_sub(stroke * 2).max(1);
    let height = metrics
        .height
        .saturating_sub(metrics.height / 4 + stroke * 2)
        .max(1);
    let x = origin_x + inset;
    let y = origin_y + (metrics.height / 8) as i32 + inset;

    for (offset_x, offset_y, width, height) in [
        (0, 0, width, stroke),
        (0, (height - stroke) as i32, width, stroke),
        (0, 0, stroke, height),
        ((width - stroke) as i32, 0, stroke, height),
    ] {
        out.rects.push(Rect {
            x: x + offset_x,
            y: y + offset_y,
            width,
            height,
            color: paint.foreground,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_decorations(
    out: &mut RowScene,
    cell: WireCell,
    paint: &CellPaint,
    origin_x: i32,
    origin_y: i32,
    metrics: CellMetrics,
    paints: &Paints,
) {
    let underline = cell.underline_style();
    let struck = cell.has(ATTR_STRIKE);
    if underline == 0 && !struck {
        return;
    }

    let color = match WireColor::unpack(cell.underline_color) {
        WireColor::Default => paint.foreground,
        _ => paints.foreground(cell.underline_color),
    };
    let stroke = metrics.stroke.max(1);
    let underline_y = origin_y + metrics.underline_top as i32;
    let floor = origin_y + metrics.height as i32 - stroke as i32;
    let mut line = |y: i32, x: i32, width: u32, height: u32| {
        out.rects.push(Rect {
            x,
            y: y.clamp(origin_y, floor),
            width,
            height,
            color,
        })
    };

    match underline {
        UNDERLINE_STRAIGHT => line(underline_y, origin_x, metrics.width, stroke),
        UNDERLINE_DOUBLE => {
            let gap = (stroke * 2) as i32;
            line(underline_y - gap, origin_x, metrics.width, stroke);
            line(underline_y + gap, origin_x, metrics.width, stroke);
        },
        UNDERLINE_DOTTED => {
            for (x, phase) in columns(origin_x, metrics.width, DOTTED_PERIOD * stroke) {
                if phase < stroke {
                    line(underline_y, x, 1, stroke);
                }
            }
        },
        UNDERLINE_DASHED => {
            let dash = (metrics.width / 2).max(2 * stroke);
            for (x, phase) in columns(origin_x, metrics.width, dash * 2) {
                if phase < dash {
                    line(underline_y, x, 1, stroke);
                }
            }
        },
        UNDERLINE_CURLY => {
            let amplitude = (metrics.height / 10).max(2 * stroke) as i32;
            for (x, phase) in columns(origin_x, metrics.width, WAVE.len() as u32 * stroke) {
                let lift = WAVE[(phase / stroke) as usize] * amplitude / 2;
                line(underline_y + lift, x, 1, stroke);
            }
        },
        _ => {},
    }
    if struck {
        let y = origin_y + metrics.strikeout_top as i32;
        line(y, origin_x, metrics.width, stroke);
    }
}

fn push_hover_underline(
    out: &mut RowScene,
    paint: &CellPaint,
    origin_x: i32,
    origin_y: i32,
    metrics: CellMetrics,
) {
    let stroke = metrics.stroke.max(1);
    let floor = origin_y + metrics.height as i32 - stroke as i32;
    out.rects.push(Rect {
        x: origin_x,
        y: (origin_y + metrics.underline_top as i32).clamp(origin_y, floor),
        width: metrics.width,
        height: stroke,
        color: paint.foreground,
    });
}

const DOTTED_PERIOD: u32 = 3;
const WAVE: [i32; 8] = [0, 1, 2, 1, 0, -1, -2, -1];

fn columns(origin_x: i32, width: u32, period: u32) -> impl Iterator<Item = (i32, u32)> {
    let period = period.max(1);
    (0..width).map(move |offset| {
        let x = origin_x + offset as i32;
        (x, x.rem_euclid(period as i32) as u32)
    })
}

fn push_cursor(
    out: &mut RowScene,
    shape: CursorShape,
    origin_x: i32,
    origin_y: i32,
    metrics: CellMetrics,
    color: Srgb,
) {
    let stroke = metrics.stroke.max(1);
    match shape {
        CursorShape::Block | CursorShape::Default => {},
        CursorShape::Underline => out.rects.push(Rect {
            x: origin_x,
            y: origin_y + (metrics.height - stroke) as i32,
            width: metrics.width,
            height: stroke,
            color,
        }),
        CursorShape::Beam => out.rects.push(Rect {
            x: origin_x,
            y: origin_y,
            width: stroke,
            height: metrics.height,
            color,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::Lookup;
    use crate::font::{FontStack, DEFAULT_FONT_SIZE};
    use crate::screen_buffer::painter::{self, Painter, Place};
    use zellij_utils::structured_render::{GraphicsRecord, PaneRect, PANE_SELECTABLE};

    fn cache() -> GlyphCache {
        GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap())
    }

    fn scene_of(rows: usize, cols: usize, paint: impl FnOnce(&mut Painter)) -> Scene {
        scene_at(rows, cols, paint, BlinkPhase::On)
    }

    fn scene_at(
        rows: usize,
        cols: usize,
        paint: impl FnOnce(&mut Painter),
        phase: BlinkPhase,
    ) -> Scene {
        let state = Painter::state(rows, cols, paint);
        build_at(
            &state,
            &mut cache(),
            phase,
            &Paints::default(),
            CursorOptions::default(),
            None,
            None,
        )
    }

    fn scene_painted_with(
        rows: usize,
        cols: usize,
        paint: impl FnOnce(&mut Painter),
        paints: &Paints,
    ) -> Scene {
        let state = Painter::state(rows, cols, paint);
        build_at(
            &state,
            &mut cache(),
            BlinkPhase::On,
            paints,
            CursorOptions::default(),
            None,
            None,
        )
    }

    fn scene_with_cursor(
        rows: usize,
        cols: usize,
        paint: impl FnOnce(&mut Painter),
        cursor: CursorOptions,
        phase: BlinkPhase,
    ) -> Scene {
        let state = Painter::state(rows, cols, paint);
        build_at(
            &state,
            &mut cache(),
            phase,
            &Paints::default(),
            cursor,
            None,
            None,
        )
    }

    fn cursor_block(scene: &Scene) -> Option<&Rect> {
        scene
            .rects
            .iter()
            .find(|rect| rect.width == 8 && rect.height == 20)
    }

    #[test]
    fn a_configured_shape_replaces_the_one_the_session_did_not_ask_for() {
        let scene = scene_with_cursor(
            1,
            4,
            |painter| {
                painter.text(0, 0, "A");
                painter.cursor(0, 0, CursorShape::Default);
            },
            CursorOptions {
                shape: Some(CursorShape::Beam),
                blink: None,
            },
            BlinkPhase::On,
        );
        assert!(cursor_block(&scene).is_none(), "{:?}", scene.rects);
        assert!(
            scene
                .rects
                .iter()
                .any(|rect| rect.width == 1 && rect.height == 20),
            "no beam was drawn: {:?}",
            scene.rects
        );
    }

    #[test]
    fn a_shape_the_session_asked_for_is_not_overridden() {
        let scene = scene_with_cursor(
            1,
            4,
            |painter| {
                painter.text(0, 0, "A");
                painter.cursor(0, 0, CursorShape::Block);
            },
            CursorOptions {
                shape: Some(CursorShape::Beam),
                blink: None,
            },
            BlinkPhase::On,
        );
        assert!(
            cursor_block(&scene).is_some(),
            "an application asking for a block was overruled: {:?}",
            scene.rects
        );
    }

    #[test]
    fn a_blinking_cursor_is_absent_from_the_off_phase_and_present_in_the_on_phase() {
        let blinking = |phase| {
            scene_with_cursor(
                1,
                4,
                |painter| {
                    painter.text(0, 0, "A");
                    painter.cursor(0, 0, CursorShape::Block);
                },
                CursorOptions {
                    shape: None,
                    blink: Some(true),
                },
                phase,
            )
        };
        assert!(cursor_block(&blinking(BlinkPhase::On)).is_some());
        assert!(cursor_block(&blinking(BlinkPhase::Off)).is_none());
    }

    #[test]
    fn a_cursor_told_not_to_blink_stays_through_both_phases() {
        let steady = |phase| {
            scene_with_cursor(
                1,
                4,
                |painter| {
                    painter.text(0, 0, "A");
                    painter.blinking_cursor(0, 0, CursorShape::Block);
                },
                CursorOptions {
                    shape: None,
                    blink: Some(false),
                },
                phase,
            )
        };
        assert!(cursor_block(&steady(BlinkPhase::On)).is_some());
        assert!(
            cursor_block(&steady(BlinkPhase::Off)).is_some(),
            "a cursor the configuration steadied still blinked"
        );
    }

    #[test]
    fn a_cursor_the_session_blinks_blinks_when_nothing_is_configured() {
        let asked = |phase| {
            scene_with_cursor(
                1,
                4,
                |painter| {
                    painter.text(0, 0, "A");
                    painter.blinking_cursor(0, 0, CursorShape::Block);
                },
                CursorOptions::default(),
                phase,
            )
        };
        assert!(cursor_block(&asked(BlinkPhase::On)).is_some());
        assert!(cursor_block(&asked(BlinkPhase::Off)).is_none());
    }

    #[test]
    fn the_scene_is_the_grid_scaled_by_the_cell_size() {
        let scene = scene_of(4, 10, |_| {});
        assert_eq!((scene.width, scene.height), (80, 80));
        assert_eq!(scene.clear, color::DEFAULT_BACKGROUND);
    }

    #[test]
    fn configured_paints_reach_the_clear_the_glyphs_and_the_table() {
        let paints = Paints {
            foreground: [11, 12, 13],
            background: [21, 22, 23],
            cursor: Some([31, 32, 33]),
            ansi: {
                let mut ansi = color::ANSI_16;
                ansi[1] = [41, 42, 43];
                ansi
            },
        };
        let scene = scene_painted_with(
            1,
            4,
            |painter| {
                painter.text(0, 0, "A");
                painter.styled(0, 1, "B", |cell| cell.fg = WireColor::Named(1).pack());
            },
            &paints,
        );
        assert_eq!(scene.clear, [21, 22, 23]);
        assert_eq!(scene.glyphs[0].color, [11, 12, 13]);
        assert_eq!(scene.glyphs[1].color, [41, 42, 43]);
    }

    #[test]
    fn a_configured_cursor_paints_the_beam() {
        let paints = Paints {
            cursor: Some([31, 32, 33]),
            ..Paints::default()
        };
        let scene = scene_painted_with(
            1,
            4,
            |painter| {
                painter.text(0, 0, "A");
                painter.cursor(0, 0, CursorShape::Beam);
            },
            &paints,
        );
        assert!(
            scene.rects.iter().any(|rect| rect.color == [31, 32, 33]),
            "{:?}",
            scene.rects
        );
    }

    #[test]
    fn a_configured_cursor_paints_the_block_the_default_shape_draws() {
        let paints = Paints {
            cursor: Some([31, 32, 33]),
            ..Paints::default()
        };
        let scene = scene_painted_with(
            1,
            4,
            |painter| {
                painter.text(0, 0, "A");
                painter.cursor(0, 0, CursorShape::Block);
            },
            &paints,
        );
        let block = scene
            .rects
            .iter()
            .find(|rect| rect.width == 8 && rect.height == 20)
            .expect("no cursor block");
        assert_eq!(block.color, [31, 32, 33]);
        assert_eq!(
            scene.glyphs[0].color,
            color::DEFAULT_BACKGROUND,
            "the character under the block keeps the cell's background"
        );
    }

    #[test]
    fn an_unconfigured_block_cursor_still_inverts_the_cell() {
        let scene = scene_of(1, 4, |painter| {
            painter.styled(0, 0, "A", |cell| cell.fg = WireColor::Named(1).pack());
            painter.cursor(0, 0, CursorShape::Block);
        });
        let block = scene
            .rects
            .iter()
            .find(|rect| rect.width == 8 && rect.height == 20)
            .expect("no cursor block");
        assert_eq!(block.color, color::ANSI_16[1]);
        assert_eq!(scene.glyphs[0].color, color::DEFAULT_BACKGROUND);
    }

    #[test]
    fn default_background_cells_emit_no_rectangles() {
        let scene = scene_of(2, 4, |painter| painter.text(0, 0, "ab"));
        assert!(
            scene
                .rects
                .iter()
                .all(|rect| rect.width < 8 || rect.color != color::DEFAULT_BACKGROUND),
            "{:?}",
            scene.rects
        );
    }

    #[test]
    fn a_colored_cell_emits_a_background_rectangle_at_its_origin() {
        let scene = scene_of(2, 4, |painter| {
            painter.styled(0, 0, "X", |cell| cell.bg = WireColor::Named(1).pack());
        });
        let rect = scene
            .rects
            .iter()
            .find(|rect| rect.color == color::ANSI_16[1])
            .expect("no red rectangle");
        assert_eq!((rect.x, rect.y), (0, 0));
        assert_eq!((rect.width, rect.height), (8, 20));
    }

    #[test]
    fn glyphs_are_emitted_for_printable_cells_only() {
        let scene = scene_of(1, 6, |painter| painter.text(0, 0, "a b"));
        assert_eq!(scene.glyphs.len(), 2);
        assert!(scene.glyphs[0].x < scene.glyphs[1].x);
    }

    #[test]
    fn inverse_swaps_foreground_and_background() {
        let plain = scene_of(1, 2, |painter| {
            painter.styled(0, 0, "A", |cell| cell.fg = WireColor::Named(1).pack());
        });
        let inverse = scene_of(1, 2, |painter| {
            painter.styled(0, 0, "A", |cell| {
                cell.fg = WireColor::Named(1).pack();
                cell.attrs |= ATTR_REVERSE;
            });
        });
        assert_eq!(plain.glyphs[0].color, color::ANSI_16[1]);
        assert_eq!(inverse.glyphs[0].color, color::DEFAULT_BACKGROUND);
        assert!(inverse
            .rects
            .iter()
            .any(|rect| rect.color == color::ANSI_16[1]));
    }

    #[test]
    fn dim_darkens_the_foreground() {
        let scene = scene_of(1, 2, |painter| {
            painter.styled(0, 0, "A", |cell| {
                cell.fg = WireColor::Named(1).pack();
                cell.attrs |= ATTR_DIM;
            });
        });
        assert_eq!(scene.glyphs[0].color, color::dim(color::ANSI_16[1]));
    }

    #[test]
    fn hidden_paints_the_glyph_in_the_background_color() {
        let scene = scene_of(1, 2, |painter| {
            painter.styled(0, 0, "A", |cell| {
                cell.fg = WireColor::Named(1).pack();
                cell.attrs |= ATTR_HIDDEN;
            });
        });
        assert_eq!(scene.glyphs[0].color, color::DEFAULT_BACKGROUND);
    }

    #[test]
    fn a_blinking_cell_is_painted_out_on_the_off_phase_and_drawn_on_the_on_phase() {
        let blinking = |painter: &mut Painter| {
            painter.styled(0, 0, "A", |cell| {
                cell.fg = WireColor::Named(1).pack();
                cell.attrs |= zellij_utils::structured_render::ATTR_SLOW_BLINK;
            });
        };
        let on = scene_at(1, 2, blinking, BlinkPhase::On);
        let off = scene_at(1, 2, blinking, BlinkPhase::Off);
        assert_eq!(on.glyphs[0].color, color::ANSI_16[1]);
        assert_eq!(off.glyphs[0].color, color::DEFAULT_BACKGROUND);
    }

    #[test]
    fn a_cell_that_does_not_blink_is_unaffected_by_the_phase() {
        let plain = |painter: &mut Painter| {
            painter.styled(0, 0, "A", |cell| cell.fg = WireColor::Named(1).pack());
        };
        let on = scene_at(1, 2, plain, BlinkPhase::On);
        let off = scene_at(1, 2, plain, BlinkPhase::Off);
        assert_eq!(on.glyphs[0].color, off.glyphs[0].color);
    }

    #[test]
    fn the_cursor_cell_keeps_its_inversion_through_the_off_phase() {
        let off = scene_at(
            1,
            2,
            |painter| {
                painter.styled(0, 0, "A", |cell| {
                    cell.fg = WireColor::Named(1).pack();
                    cell.attrs |= zellij_utils::structured_render::ATTR_SLOW_BLINK;
                });
                painter.cursor(0, 0, CursorShape::Block);
            },
            BlinkPhase::Off,
        );
        assert_eq!(off.glyphs[0].color, color::DEFAULT_BACKGROUND);
        assert!(off.rects.iter().any(|rect| rect.color == color::ANSI_16[1]));
    }

    #[test]
    fn build_renders_the_on_phase() {
        let blinking = |painter: &mut Painter| {
            painter.styled(0, 0, "A", |cell| {
                cell.fg = WireColor::Named(1).pack();
                cell.attrs |= zellij_utils::structured_render::ATTR_SLOW_BLINK;
            });
        };
        let state = Painter::state(1, 2, blinking);
        assert_eq!(
            build(&state, &mut cache()).glyphs[0].color,
            scene_at(1, 2, blinking, BlinkPhase::On).glyphs[0].color
        );
    }

    fn entry_of(cache: &mut GlyphCache, style: FaceStyle, character: char) -> AtlasEntry {
        match cache.glyph(style, character, 1) {
            Lookup::Rendered(rendered) => rendered.entry,
            other => panic!("expected a rendered glyph, got {:?}", other),
        }
    }

    #[test]
    fn bold_and_italic_select_different_faces() {
        let mut cache = cache();
        let regular = entry_of(&mut cache, FaceStyle::Regular, 'A');
        let bold = entry_of(&mut cache, FaceStyle::Bold, 'A');
        let state = Painter::state(1, 4, |painter| {
            painter.text(0, 0, "A");
            painter.styled(0, 1, "A", |cell| cell.attrs |= ATTR_BOLD);
        });
        let scene = build(&state, &mut cache);
        assert_eq!(scene.glyphs[0].entry, regular);
        assert_eq!(scene.glyphs[1].entry, bold);
    }

    fn underlined(style: u16) -> impl Fn(&mut Painter) {
        move |painter: &mut Painter| {
            painter.styled(0, 0, "abcd", |cell| {
                cell.attrs |= style << zellij_utils::structured_render::UNDERLINE_SHIFT;
            });
        }
    }

    fn hovered_scene(rows: usize, cols: usize, run: Option<&LinkRun>) -> Scene {
        let state = Painter::state(rows, cols, |painter| {
            painter.links(&[(1, "https://example.com")]);
            painter.linked(0, 0, "ab", 1);
            painter.text(0, 2, "cd");
        });
        build_at(
            &state,
            &mut cache(),
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            run,
            None,
        )
    }

    #[test]
    fn a_link_is_undecorated_until_it_is_hovered() {
        let metrics = cache().metrics();
        let plain = hovered_scene(1, 4, None);
        assert!(!plain
            .rects
            .iter()
            .any(|rect| rect.y == metrics.underline_top as i32));

        let run = LinkRun {
            id: 1,
            uri: "https://example.com".to_owned(),
            spans: vec![(0, 0, 1)],
        };
        let hovered = hovered_scene(1, 4, Some(&run));
        let strokes: Vec<_> = hovered
            .rects
            .iter()
            .filter(|rect| {
                rect.y == metrics.underline_top as i32
                    && rect.width == metrics.width
                    && rect.height == metrics.stroke.max(1)
            })
            .collect();
        assert_eq!(strokes.len(), 2, "{:?}", hovered.rects);
        assert_eq!(strokes[0].x, 0);
        assert_eq!(strokes[1].x, metrics.width as i32);
    }

    #[test]
    fn a_hovered_run_that_wrapped_is_underlined_on_both_of_its_rows() {
        let state = Painter::state(2, 4, |painter| {
            painter.links(&[(1, "https://example.com")]);
            painter.linked(0, 2, "ab", 1);
            painter.linked(1, 0, "cd", 1);
        });
        let run = LinkRun {
            id: 1,
            uri: "https://example.com".to_owned(),
            spans: vec![(0, 2, 3), (1, 0, 1)],
        };
        let scene = build_at(
            &state,
            &mut cache(),
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            Some(&run),
            None,
        );
        let metrics = cache().metrics();
        let strokes: Vec<(i32, i32)> = scene
            .rects
            .iter()
            .filter(|rect| rect.width == metrics.width && rect.height == metrics.stroke.max(1))
            .map(|rect| (rect.x, rect.y))
            .collect();
        let first = metrics.underline_top as i32;
        let second = metrics.height as i32 + first;
        assert_eq!(
            strokes,
            vec![
                (2 * metrics.width as i32, first),
                (3 * metrics.width as i32, first),
                (0, second),
                (metrics.width as i32, second),
            ],
            "only the four cells of the wrapped run, on both rows"
        );
    }

    fn composing(
        state: &TerminalState,
        cache: &mut GlyphCache,
        preedit: Option<&Preedit>,
    ) -> Scene {
        build_at(
            state,
            cache,
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            None,
            preedit,
        )
    }

    fn composing_state() -> TerminalState {
        Painter::state(2, 8, |painter| {
            painter.text(0, 0, "ab");
            painter.text(1, 0, "second");
            painter.cursor(0, 2, CursorShape::Block);
        })
    }

    #[test]
    fn a_preedit_is_drawn_at_the_cursor_and_underlined() {
        let state = composing_state();
        let mut cache = cache();
        let metrics = cache.metrics();
        let preedit = Preedit::new("ni", Some((2, 2)));
        let scene = composing(&state, &mut cache, Some(&preedit));

        for (offset, character) in [(0, 'n'), (1, 'i')] {
            let entry = entry_of(&mut cache, FaceStyle::Regular, character);
            let x = ((2 + offset) * metrics.width) as i32;
            assert!(
                scene
                    .glyphs
                    .iter()
                    .any(|quad| quad.entry == entry && quad.x >= x && quad.x < x + metrics.width as i32),
                "{} is not drawn in the cell the composition occupies",
                character
            );
            assert!(
                scene.rects.iter().any(|rect| rect.x == x
                    && rect.y == metrics.underline_top as i32
                    && rect.width == metrics.width
                    && rect.height == metrics.stroke.max(1)),
                "{} is not underlined as composing text",
                character
            );
        }
    }

    #[test]
    fn the_cells_under_a_preedit_are_untouched() {
        let state = composing_state();
        let mut cache = cache();
        let preedit = Preedit::new("ni", Some((2, 2)));
        let scene = composing(&state, &mut cache, Some(&preedit));
        let plain = composing(&state, &mut cache, None);

        assert_eq!(
            state.screen().cell(0, 2).character(),
            ' ',
            "the pane's own cells must not learn about a composition"
        );
        let metrics = cache.metrics();
        let untouched = |scene: &Scene| -> Vec<i32> {
            scene
                .glyphs
                .iter()
                .filter(|quad| quad.y >= metrics.height as i32)
                .map(|quad| quad.x)
                .collect()
        };
        assert_eq!(
            untouched(&scene),
            untouched(&plain),
            "a composition changed a row it does not sit on"
        );
        let entry = entry_of(&mut cache, FaceStyle::Regular, 'a');
        assert!(
            scene.glyphs.iter().any(|quad| quad.entry == entry),
            "the text the pane painted is still drawn beneath the composition"
        );
    }

    #[test]
    fn the_cursor_follows_the_composition_rather_than_the_pane() {
        let state = composing_state();
        let mut cache = cache();
        let metrics = cache.metrics();
        let preedit = Preedit::new("ni", Some((2, 2)));
        let scene = composing(&state, &mut cache, Some(&preedit));
        let block = |scene: &Scene| -> Vec<i32> {
            scene
                .rects
                .iter()
                .filter(|rect| {
                    rect.height == metrics.height && rect.color == color::DEFAULT_FOREGROUND
                })
                .map(|rect| rect.x)
                .collect()
        };
        assert_eq!(
            block(&scene),
            vec![(4 * metrics.width) as i32],
            "the cursor sits after the composed text, not under it"
        );
    }

    #[test]
    fn a_preedit_without_a_cursor_range_hides_the_cursor() {
        let state = composing_state();
        let mut cache = cache();
        let metrics = cache.metrics();
        let preedit = Preedit::new("ni", None);
        let scene = composing(&state, &mut cache, Some(&preedit));
        assert!(
            !scene.rects.iter().any(|rect| rect.height == metrics.height
                && rect.color == color::DEFAULT_FOREGROUND),
            "winit asks for the cursor to be hidden when it reports no range"
        );
    }

    #[test]
    fn a_wide_composed_character_takes_two_cells() {
        let state = composing_state();
        let mut cache = cache();
        let metrics = cache.metrics();
        let preedit = Preedit::new("你", Some((3, 3)));
        let scene = composing(&state, &mut cache, Some(&preedit));
        let strokes: Vec<i32> = scene
            .rects
            .iter()
            .filter(|rect| {
                rect.y == metrics.underline_top as i32 && rect.height == metrics.stroke.max(1)
            })
            .map(|rect| rect.x)
            .collect();
        assert_eq!(
            strokes,
            vec![(2 * metrics.width) as i32, (3 * metrics.width) as i32],
            "a wide character underlines both of the cells it occupies"
        );
    }

    #[test]
    fn a_preedit_is_clipped_at_the_end_of_its_row() {
        let state = Painter::state(1, 4, |painter| {
            painter.cursor(0, 2, CursorShape::Block);
        });
        let mut cache = cache();
        let metrics = cache.metrics();
        let preedit = Preedit::new("abcd", Some((4, 4)));
        let scene = composing(&state, &mut cache, Some(&preedit));
        let strokes: Vec<i32> = scene
            .rects
            .iter()
            .filter(|rect| {
                rect.y == metrics.underline_top as i32 && rect.height == metrics.stroke.max(1)
            })
            .map(|rect| rect.x)
            .collect();
        assert_eq!(
            strokes,
            vec![(2 * metrics.width) as i32, (3 * metrics.width) as i32],
            "composing text never wraps onto the next row"
        );
    }

    #[test]
    fn underline_emits_a_stroke_at_the_underline_position() {
        let scene = scene_of(1, 2, |painter| {
            painter.styled(0, 0, "A", |cell| {
                cell.attrs |=
                    UNDERLINE_STRAIGHT << zellij_utils::structured_render::UNDERLINE_SHIFT;
            });
        });
        let metrics = cache().metrics();
        assert!(scene
            .rects
            .iter()
            .any(|rect| rect.y == metrics.underline_top as i32
                && rect.width == metrics.width
                && rect.height == metrics.stroke));
    }

    #[test]
    fn a_double_underline_emits_two_strokes() {
        let scene = scene_of(1, 2, |painter| {
            painter.styled(0, 0, "A", |cell| {
                cell.attrs |= UNDERLINE_DOUBLE << zellij_utils::structured_render::UNDERLINE_SHIFT;
            });
        });
        let strokes: Vec<_> = scene
            .rects
            .iter()
            .filter(|rect| rect.width == 8 && rect.height == 1)
            .collect();
        assert_eq!(strokes.len(), 2, "{:?}", scene.rects);
    }

    #[test]
    fn an_underline_color_overrides_the_foreground() {
        let scene = scene_of(1, 2, |painter| {
            painter.styled(0, 0, "A", |cell| {
                cell.attrs |=
                    UNDERLINE_STRAIGHT << zellij_utils::structured_render::UNDERLINE_SHIFT;
                cell.underline_color = WireColor::Indexed(208).pack();
            });
        });
        assert!(scene
            .rects
            .iter()
            .any(|rect| rect.color == color::indexed(208)));
    }

    fn decoration_rows(scene: &Scene) -> Vec<i32> {
        let mut rows: Vec<i32> = scene.rects.iter().map(|rect| rect.y).collect();
        rows.sort_unstable();
        rows.dedup();
        rows
    }

    #[test]
    fn a_dotted_underline_is_one_stroke_on_and_two_off() {
        let scene = scene_of(1, 8, underlined(UNDERLINE_DOTTED));
        let metrics = cache().metrics();
        assert!(!scene.rects.is_empty());
        for rect in &scene.rects {
            assert_eq!(rect.width, 1, "{:?}", rect);
            assert_eq!(rect.x % 3, 0, "the dots left their period: {:?}", rect);
            assert_eq!(rect.y, metrics.underline_top as i32);
        }
    }

    #[test]
    fn a_dotted_pattern_runs_on_across_a_cell_boundary() {
        let scene = scene_of(1, 8, underlined(UNDERLINE_DOTTED));
        let metrics = cache().metrics();
        let columns: Vec<u32> = scene.rects.iter().map(|rect| rect.x as u32).collect();
        let spans: Vec<u32> = columns.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(
            spans.iter().all(|span| *span == 3),
            "the pattern restarted at a cell boundary: {:?}",
            columns
        );
        assert!(columns.iter().any(|x| *x >= metrics.width));
    }

    #[test]
    fn a_dashed_underline_is_half_a_cell_on_and_half_off() {
        let scene = scene_of(1, 8, underlined(UNDERLINE_DASHED));
        let metrics = cache().metrics();
        let dash = metrics.width / 2;
        for rect in &scene.rects {
            assert!(
                rect.x as u32 % metrics.width < dash,
                "a dash landed in the gap: {:?}",
                rect
            );
        }
        assert_eq!(scene.rects.len() as u32, dash * 4);
    }

    #[test]
    fn an_undercurl_swings_evenly_around_the_underline_position() {
        let scene = scene_of(1, 8, underlined(UNDERLINE_CURLY));
        let metrics = cache().metrics();
        let rows = decoration_rows(&scene);
        let centre = metrics.underline_top as i32;
        let amplitude = (metrics.height / 10).max(2 * metrics.stroke) as i32;
        assert_eq!(rows.first().copied(), Some(centre - amplitude));
        assert_eq!(rows.last().copied(), Some(centre + amplitude));
        assert!(rows.contains(&centre));
    }

    #[test]
    fn an_undercurl_completes_one_cycle_per_cell() {
        let scene = scene_of(1, 8, |painter| {
            painter.styled(0, 0, "ab", |cell| {
                cell.attrs |= UNDERLINE_CURLY << zellij_utils::structured_render::UNDERLINE_SHIFT;
            });
        });
        let metrics = cache().metrics();
        let trough = scene
            .rects
            .iter()
            .filter(|rect| rect.y == (metrics.underline_top + 2) as i32)
            .count();
        assert_eq!(trough, 2, "expected one trough per cell");
    }

    #[test]
    fn every_underline_style_stays_inside_its_own_row() {
        let metrics = cache().metrics();
        for style in [
            UNDERLINE_STRAIGHT,
            UNDERLINE_DOUBLE,
            UNDERLINE_CURLY,
            UNDERLINE_DOTTED,
            UNDERLINE_DASHED,
        ] {
            let scene = scene_of(2, 8, underlined(style));
            for rect in &scene.rects {
                assert!(rect.y >= 0, "{} left the frame: {:?}", style, rect);
                assert!(
                    rect.y + rect.height as i32 <= metrics.height as i32,
                    "{} bled into the row below: {:?}",
                    style,
                    rect
                );
            }
        }
        let struck = scene_of(2, 8, |painter| {
            painter.styled(0, 0, "abcd", |cell| cell.attrs |= ATTR_STRIKE);
        });
        for rect in &struck.rects {
            assert!(rect.y >= 0 && rect.y + rect.height as i32 <= metrics.height as i32);
        }
    }

    #[test]
    fn a_doubled_font_size_doubles_the_decoration_geometry() {
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE * 2.0).unwrap());
        let metrics = cache.metrics();
        let state = Painter::state(1, 8, underlined(UNDERLINE_CURLY));
        let scene = build(&state, &mut cache);
        let rows = decoration_rows(&scene);
        let amplitude = (metrics.height / 10).max(2 * metrics.stroke) as i32;
        assert_eq!(amplitude, 4);
        assert_eq!(
            rows.last().copied().unwrap() - rows.first().copied().unwrap(),
            amplitude * 2
        );
    }

    #[test]
    fn strikeout_sits_above_the_underline() {
        let scene = scene_of(1, 2, |painter| {
            painter.styled(0, 0, "A", |cell| cell.attrs |= ATTR_STRIKE);
        });
        let metrics = cache().metrics();
        let stroke = scene
            .rects
            .iter()
            .find(|rect| rect.height == metrics.stroke)
            .unwrap();
        assert_eq!(stroke.y, metrics.strikeout_top as i32);
        assert!(stroke.y < metrics.underline_top as i32);
    }

    #[test]
    fn a_block_cursor_inverts_its_cell() {
        let scene = scene_of(1, 4, |painter| {
            painter.text(0, 0, "A");
            painter.cursor(0, 0, CursorShape::Block);
        });
        let metrics = cache().metrics();
        assert!(scene.rects.iter().any(|rect| rect.x == 0
            && rect.width == metrics.width
            && rect.height == metrics.height
            && rect.color == color::DEFAULT_FOREGROUND));
        assert_eq!(scene.glyphs[0].color, color::DEFAULT_BACKGROUND);
    }

    #[test]
    fn a_beam_cursor_is_a_narrow_full_height_stroke() {
        let scene = scene_of(1, 4, |painter| painter.cursor(0, 0, CursorShape::Beam));
        let metrics = cache().metrics();
        let beam = scene
            .rects
            .iter()
            .find(|rect| rect.height == metrics.height)
            .unwrap();
        assert_eq!(beam.width, metrics.stroke);
        assert_eq!(beam.x, 0);
    }

    #[test]
    fn an_underline_cursor_is_a_stroke_along_the_bottom_of_its_cell() {
        let scene = scene_of(1, 4, |painter| painter.cursor(0, 0, CursorShape::Underline));
        let metrics = cache().metrics();
        let rect = scene.rects.first().expect("no cursor rectangle");
        assert_eq!(rect.width, metrics.width);
        assert_eq!(rect.height, metrics.stroke);
        assert_eq!(rect.y, (metrics.height - metrics.stroke) as i32);
    }

    #[test]
    fn a_hidden_cursor_emits_nothing() {
        let scene = scene_of(1, 4, |_| {});
        assert!(scene.rects.is_empty(), "{:?}", scene.rects);
    }

    #[test]
    fn a_wide_fallback_symbol_spreads_into_a_blank_neighbour_instead_of_shrinking() {
        let glyph_of = |text: &str| {
            let scene = scene_of(1, 4, |painter| painter.text(0, 0, text));
            scene
                .glyphs
                .iter()
                .find(|quad| quad.x < 8)
                .copied()
                .expect("the symbol drew nothing")
        };
        let spread = glyph_of("\u{3012} ");
        let crowded = glyph_of("\u{3012}x");
        assert!(
            spread.entry.height > crowded.entry.height,
            "{:?} vs {:?}",
            spread.entry,
            crowded.entry
        );
        assert!(spread.x >= 0 && spread.x + spread.entry.width as i32 <= 16);
        assert!(crowded.entry.width <= 10, "{:?}", crowded.entry);
    }

    #[test]
    fn nothing_spreads_past_the_last_column_or_into_a_neighbour_with_text() {
        let spill = |text: &str, col: usize| {
            let state = Painter::state(1, 3, |painter| painter.text(0, 0, text));
            let paints = Paints::default();
            let cells: Vec<RowCell> = (0..3)
                .map(|col| {
                    let cell = state.screen().cell(0, col);
                    let occupancy = state.occupancy(0, col);
                    RowCell {
                        cell,
                        occupancy,
                        paint: paint_of(cell, occupancy, &paints),
                        on_cursor: false,
                        inverted_by_cursor: false,
                        composing: false,
                    }
                })
                .collect();
            spill_room(&cells, state.geometry(), 0, col)
        };
        assert_eq!(spill("ab ", 0), 0);
        assert_eq!(spill("a  ", 0), 1);
        assert_eq!(spill("ab ", 2), 0);
    }

    #[test]
    fn a_wide_character_spacer_draws_no_glyph_of_its_own() {
        let scene = scene_of(1, 6, |painter| painter.wide(0, 0, '\u{2500}'));
        assert_eq!(scene.glyphs.len(), 1);
    }

    #[test]
    fn a_cjk_character_is_drawn_once_across_two_cells() {
        let scene = scene_of(1, 6, |painter| painter.wide(0, 0, '\u{4f60}'));
        assert_eq!(scene.glyphs.len(), 1);
        let quad = scene.glyphs[0];
        assert!(quad.entry.width > 8, "{:?}", quad.entry);
        assert!(quad.entry.width <= 16, "{:?}", quad.entry);
        assert!(quad.x >= 0 && quad.x + quad.entry.width as i32 <= 16);
    }

    #[test]
    fn an_emoji_lands_in_the_color_list_rather_than_the_mask_list() {
        let scene = scene_of(1, 6, |painter| painter.wide(0, 0, '\u{1f600}'));
        assert!(scene.glyphs.is_empty(), "{:?}", scene.glyphs);
        assert_eq!(scene.color_glyphs.len(), 1);
        assert_eq!(scene.color_glyphs[0].color, OPAQUE);
    }

    #[test]
    fn a_dimmed_emoji_is_tinted_rather_than_recolored() {
        let scene = scene_of(1, 6, |painter| {
            painter.wide_styled(0, 0, '\u{1f600}', |cell| cell.attrs |= ATTR_DIM);
        });
        assert_eq!(scene.color_glyphs[0].color, color::dim(OPAQUE));
    }

    #[test]
    fn a_hidden_emoji_is_not_drawn_at_all() {
        let scene = scene_of(1, 6, |painter| {
            painter.wide_styled(0, 0, '\u{1f600}', |cell| cell.attrs |= ATTR_HIDDEN);
        });
        assert!(scene.color_glyphs.is_empty(), "{:?}", scene.color_glyphs);
    }

    #[test]
    fn a_codepoint_no_face_carries_is_drawn_as_a_box() {
        let scene = scene_of(1, 6, |painter| painter.text(0, 0, "\u{10fffd}"));
        assert!(scene.glyphs.is_empty(), "{:?}", scene.glyphs);
        assert_eq!(scene.rects.len(), 4, "{:?}", scene.rects);
        assert!(scene
            .rects
            .iter()
            .all(|rect| rect.color == color::DEFAULT_FOREGROUND));
    }

    #[test]
    fn a_variation_selector_is_not_drawn() {
        let scene = scene_of(1, 6, |painter| painter.text(0, 0, "A\u{fe0f}"));
        assert_eq!(scene.glyphs.len(), 1);
        assert!(scene.rects.is_empty(), "{:?}", scene.rects);
    }

    fn image(id: u64, width: u32, height: u32) -> GraphicsRecord {
        painter::residency(id, width, height, [255, 128, 0, 255])
    }

    fn scene_with_graphics(rows: usize, cols: usize, records: &[GraphicsRecord]) -> Scene {
        let state = Painter::state(rows, cols, |painter| painter.graphics(records));
        build(&state, &mut cache())
    }

    #[test]
    fn a_placement_becomes_a_quad_at_the_cell_it_names() {
        let scene = scene_with_graphics(
            4,
            8,
            &[
                image(1, 16, 40),
                Place::of(1, 16, 40).at(1, 2).offset(2, 3).record(),
            ],
        );
        assert_eq!(scene.images.len(), 1);
        let quad = &scene.images[0];
        assert_eq!((quad.x, quad.y), (2 * 8 + 2, 20 + 3));
        assert_eq!((quad.width, quad.height), (16, 40));
        assert_eq!((quad.source_x, quad.source_y), (0, 0));
        assert_eq!(scene.resident_images, vec![ImageKey::Kitty(1)]);
    }

    #[test]
    fn a_cropped_placement_carries_the_source_offset_and_extent() {
        let scene = scene_with_graphics(
            4,
            8,
            &[
                image(1, 16, 40),
                Place::of(1, 16, 40).cropped(4, 6, 8, 10).record(),
            ],
        );
        let quad = &scene.images[0];
        assert_eq!((quad.source_x, quad.source_y), (4, 6));
        assert_eq!((quad.width, quad.height), (8, 10));
    }

    fn clipping_pane(x: u16, y: u16, cols: u16, rows: u16) -> PaneRect {
        PaneRect {
            x,
            y,
            cols,
            rows,
            top: 0,
            bottom: 0,
            left: 0,
            right: 0,
            flags: PANE_SELECTABLE,
        }
    }

    fn quad_at(x: i32, y: i32, width: u32, height: u32) -> ImageQuad {
        ImageQuad {
            key: ImageKey::Kitty(1),
            revision: 0,
            x,
            y,
            width,
            height,
            source_x: 0,
            source_y: 0,
            z: 0,
            image: Rc::new(Image {
                width,
                height,
                pixels: vec![0; (width * height * 4) as usize],
            }),
        }
    }

    fn metrics_of(width: u32, height: u32) -> CellMetrics {
        CellMetrics {
            width,
            height,
            ..cache().metrics()
        }
    }

    #[test]
    fn an_image_inside_its_pane_is_left_alone() {
        let geometry = GeometryRecord {
            panes: vec![clipping_pane(0, 0, 8, 4)],
        };
        let quad = quad_at(16, 20, 16, 20);
        let clipped =
            clip_to_owning_pane(quad.clone(), 2, 1, &geometry, metrics_of(8, 20)).expect("kept");
        assert_eq!(clipped, quad);
    }

    #[test]
    fn an_image_straddling_a_pane_edge_is_cut_at_the_edge() {
        let geometry = GeometryRecord {
            panes: vec![clipping_pane(0, 0, 4, 4), clipping_pane(4, 0, 4, 4)],
        };
        let metrics = metrics_of(8, 20);
        let clipped = clip_to_owning_pane(quad_at(16, 0, 32, 20), 2, 0, &geometry, metrics)
            .expect("the part inside the pane survives");
        assert_eq!(
            (clipped.x, clipped.width),
            (16, 16),
            "an image starting at column 2 of a four-column pane keeps two columns"
        );
        assert_eq!(
            (clipped.source_x, clipped.source_y),
            (0, 0),
            "cutting the right edge does not move the source origin"
        );
        assert_eq!((clipped.y, clipped.height), (0, 20));
    }

    #[test]
    fn an_image_cut_on_its_left_and_top_carries_the_source_origin_with_it() {
        let geometry = GeometryRecord {
            panes: vec![clipping_pane(2, 1, 6, 3)],
        };
        let metrics = metrics_of(8, 20);
        let clipped = clip_to_owning_pane(quad_at(0, 0, 64, 80), 3, 1, &geometry, metrics)
            .expect("the part inside the pane survives");
        assert_eq!((clipped.x, clipped.y), (16, 20));
        assert_eq!((clipped.source_x, clipped.source_y), (16, 20));
        assert_eq!((clipped.width, clipped.height), (48, 60));
    }

    #[test]
    fn an_image_wholly_outside_its_pane_is_dropped() {
        let geometry = GeometryRecord {
            panes: vec![clipping_pane(0, 0, 4, 4)],
        };
        assert_eq!(
            clip_to_owning_pane(quad_at(64, 0, 32, 20), 0, 0, &geometry, metrics_of(8, 20)),
            None
        );
    }

    #[test]
    fn an_image_whose_cell_belongs_to_no_pane_is_left_unclipped() {
        let geometry = GeometryRecord {
            panes: vec![clipping_pane(0, 0, 4, 4)],
        };
        let quad = quad_at(64, 0, 32, 20);
        assert_eq!(
            clip_to_owning_pane(quad.clone(), 8, 0, &geometry, metrics_of(8, 20)),
            Some(quad),
            "a placement the layout does not describe keeps the behaviour it had"
        );
    }

    #[test]
    fn an_image_under_a_floating_pane_is_clipped_to_the_pane_its_origin_falls_in() {
        let geometry = GeometryRecord {
            panes: vec![clipping_pane(0, 0, 8, 4), clipping_pane(4, 0, 4, 4)],
        };
        let metrics = metrics_of(8, 20);
        let under = clip_to_owning_pane(quad_at(0, 0, 64, 20), 0, 0, &geometry, metrics)
            .expect("the tiled pane still owns the image");
        assert_eq!(
            (under.x, under.width),
            (0, 64),
            "the floating pane above does not clip an image the pane below owns"
        );
        let over = clip_to_owning_pane(quad_at(32, 0, 64, 20), 4, 0, &geometry, metrics)
            .expect("the floating pane owns an image placed inside it");
        assert_eq!(
            (over.x, over.width),
            (32, 32),
            "an image placed in the floating pane is cut at the floating pane's edge"
        );
    }

    #[test]
    fn an_image_at_a_pane_edge_is_clipped_on_the_way_into_the_scene() {
        let state = Painter::state(4, 8, |painter| {
            painter.geometry(&GeometryRecord {
                panes: vec![clipping_pane(0, 0, 4, 4), clipping_pane(4, 0, 4, 4)],
            });
            painter.graphics(&[image(1, 64, 20), Place::of(1, 64, 20).at(0, 2).record()]);
        });
        let scene = build(&state, &mut cache());
        assert_eq!(scene.images.len(), 1);
        let quad = &scene.images[0];
        assert_eq!(
            (quad.x, quad.width),
            (2 * 8, 2 * 8),
            "the image is cut at the right edge of the pane it was placed in"
        );
    }

    #[test]
    fn quads_are_ordered_by_z_index() {
        let scene = scene_with_graphics(
            4,
            8,
            &[
                image(1, 8, 20),
                image(2, 8, 20),
                Place::of(1, 8, 20).depth(5).record(),
                Place::of(2, 8, 20).depth(-3).record(),
            ],
        );
        assert_eq!(
            scene.images.iter().map(|quad| quad.z).collect::<Vec<_>>(),
            vec![-3, 5]
        );
        assert_eq!(
            scene.resident_images,
            vec![ImageKey::Kitty(1), ImageKey::Kitty(2)]
        );
    }

    #[test]
    fn a_resident_image_with_no_placement_emits_no_quad() {
        let scene = scene_with_graphics(2, 4, &[image(1, 8, 20)]);
        assert!(scene.images.is_empty());
        assert_eq!(scene.resident_images, vec![ImageKey::Kitty(1)]);
    }

    #[test]
    fn a_deleted_image_leaves_neither_quad_nor_residency() {
        let scene = scene_with_graphics(
            2,
            4,
            &[
                image(1, 8, 20),
                Place::of(1, 8, 20).record(),
                GraphicsRecord::DeleteImage { id: 1 },
            ],
        );
        assert!(scene.images.is_empty());
        assert!(scene.resident_images.is_empty());
    }

    fn sixel(width: usize, height: usize) -> String {
        format!(
            "\u{1b}P0;1;0q\"1;1;{};{}#0;2;100;0;0!{}~\u{1b}\\",
            width, height, width
        )
    }

    #[test]
    fn a_sixel_chunk_becomes_a_quad_at_the_cell_its_record_names() {
        let scene = scene_with_graphics(4, 8, &[painter::sixel_chunk(1, 2, 24, 6, &sixel(24, 6))]);
        assert_eq!(scene.images.len(), 1);
        let quad = &scene.images[0];
        assert_eq!((quad.x, quad.y), (2 * 8, 20));
        assert_eq!((quad.width, quad.height), (24, 6));
        assert_eq!((quad.source_x, quad.source_y), (0, 0));
        assert_eq!(quad.z, 0);
        assert_eq!(scene.resident_images, vec![ImageKey::Sixel(1)]);
    }

    #[test]
    fn a_sixel_chunk_is_drawn_under_the_kitty_images_that_share_its_z_index() {
        let scene = scene_with_graphics(
            4,
            16,
            &[
                image(1, 8, 20),
                Place::of(1, 8, 20).record(),
                painter::sixel_chunk(2, 0, 8, 6, &sixel(8, 6)),
            ],
        );
        assert_eq!(
            scene.images.iter().map(|quad| quad.key).collect::<Vec<_>>(),
            vec![ImageKey::Sixel(1), ImageKey::Kitty(1)]
        );
        assert_eq!(
            scene.resident_images,
            vec![ImageKey::Sixel(1), ImageKey::Kitty(1)]
        );
    }

    #[test]
    fn the_scene_is_deterministic_for_one_frame() {
        let paint = |painter: &mut Painter| {
            painter.styled(0, 0, "heading", |cell| {
                cell.fg = WireColor::Named(3).pack();
                cell.attrs |= ATTR_BOLD;
            });
            painter.styled(1, 0, "body", |cell| {
                cell.attrs |=
                    UNDERLINE_STRAIGHT << zellij_utils::structured_render::UNDERLINE_SHIFT;
            });
        };
        assert_eq!(scene_of(4, 20, paint), scene_of(4, 20, paint));
    }
}

#[cfg(test)]
mod ligature_tests {
    use super::*;
    use crate::font::{FontOptions, FontStack, DEFAULT_FONT_SIZE};
    use crate::screen_buffer::painter::Painter;
    use zellij_utils::structured_render::{PaneRect, PANE_FRAMED, PANE_SELECTABLE};

    fn stack(ligatures: bool) -> FontStack {
        FontStack::build(&FontOptions {
            family: None,
            size: DEFAULT_FONT_SIZE,
            system_fonts: false,
            ligatures,
        })
        .unwrap()
    }

    fn cache_with(ligatures: bool) -> GlyphCache {
        GlyphCache::new(stack(ligatures))
    }

    fn scene_through(
        cache: &mut GlyphCache,
        rows: usize,
        cols: usize,
        paint: impl FnOnce(&mut Painter),
    ) -> Scene {
        let state = Painter::state(rows, cols, paint);
        build_at(
            &state,
            cache,
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            None,
            None,
        )
    }

    fn scene_with(
        ligatures: bool,
        rows: usize,
        cols: usize,
        paint: impl FnOnce(&mut Painter),
    ) -> Scene {
        scene_through(&mut cache_with(ligatures), rows, cols, paint)
    }

    fn nominal_in(cache: &mut GlyphCache, character: char) -> AtlasEntry {
        match cache.glyph(FaceStyle::Regular, character, 1) {
            Lookup::Rendered(rendered) => rendered.entry,
            other => panic!("{:?} did not rasterize: {:?}", character, other),
        }
    }

    fn draws(scene: &Scene, entry: AtlasEntry) -> bool {
        scene.glyphs.iter().any(|quad| quad.entry == entry)
    }

    fn columns(scene: &Scene) -> Vec<i32> {
        let mut columns: Vec<i32> = scene.glyphs.iter().map(|quad| quad.x).collect();
        columns.sort_unstable();
        columns
    }

    fn run(start: usize, end: usize, text: &str, font: FontId) -> Run {
        Run {
            start,
            end,
            text: text.to_owned(),
            font,
        }
    }

    fn uniform_cells(count: usize, character: char) -> Vec<RowCell> {
        let cell = WireCell {
            ch: character as u32,
            ..WireCell::BLANK
        };
        (0..count)
            .map(|_| RowCell {
                cell,
                occupancy: Occupancy::Single,
                paint: paint_of(cell, Occupancy::Single, &Paints::default()),
                on_cursor: false,
                inverted_by_cursor: false,
                composing: false,
            })
            .collect()
    }

    fn empty_scene(_cells: usize, _metrics: CellMetrics) -> RowScene {
        RowScene::default()
    }

    #[test]
    fn an_arrow_becomes_ligature_pieces_over_the_same_two_cells() {
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 4, |painter| painter.text(0, 0, "a->b"));
        let dash = nominal_in(&mut cache, '-');
        let greater = nominal_in(&mut cache, '>');
        let a = nominal_in(&mut cache, 'a');

        let unshaped = scene_with(false, 1, 4, |painter| painter.text(0, 0, "a->b"));
        assert_eq!(
            columns(&scene),
            columns(&unshaped),
            "a ligature moved a glyph off its cell"
        );
        assert_eq!(scene.glyphs.len(), 4);
        assert!(draws(&scene, a), "the letters outside the sequence moved");
        assert!(!draws(&scene, dash), "the hyphen was drawn nominally");
        assert!(
            !draws(&scene, greater),
            "the greater-than was drawn nominally"
        );
    }

    #[test]
    fn with_the_option_off_the_same_text_draws_the_nominal_glyphs() {
        let mut cache = cache_with(false);
        let scene = scene_through(&mut cache, 1, 4, |painter| painter.text(0, 0, "a->b"));
        let dash = nominal_in(&mut cache, '-');
        let greater = nominal_in(&mut cache, '>');

        assert_eq!(scene.glyphs.len(), 4);
        assert!(draws(&scene, dash));
        assert!(draws(&scene, greater));
    }

    #[test]
    fn a_run_without_a_ligating_sequence_renders_exactly_as_it_did_unshaped() {
        for text in ["hello", "&&", "++", "abc123", "x_y", "www"] {
            let plain = scene_with(false, 1, 8, |painter| painter.text(0, 0, text));
            let ligated = scene_with(true, 1, 8, |painter| painter.text(0, 0, text));
            assert_eq!(plain, ligated, "{:?} moved with ligatures on", text);
        }
    }

    #[test]
    fn a_colour_change_inside_a_sequence_keeps_the_ligature_and_paints_each_cell_its_own_colour() {
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 4, |painter| {
            painter.styled(0, 0, "=", |cell| {
                cell.fg = WireColor::Named(1).pack();
            });
            painter.styled(0, 1, ">", |cell| {
                cell.fg = WireColor::Named(6).pack();
            });
        });
        let equals = nominal_in(&mut cache, '=');
        let paints = Paints::default();

        assert!(
            !draws(&scene, equals),
            "a colour change broke the ligature: {:?}",
            scene.glyphs
        );
        assert!(
            scene.glyphs.iter().any(|quad| quad.color == paints.ansi[1]),
            "no piece was painted in the first cell's colour"
        );
        assert!(
            scene.glyphs.iter().any(|quad| quad.color == paints.ansi[6]),
            "no piece was painted in the second cell's colour"
        );
        for quad in &scene.glyphs {
            let cell = if quad.color == paints.ansi[1] { 0 } else { 1 };
            let (left, right) = (cell * 8, (cell + 1) * 8);
            assert!(
                quad.x >= left && quad.x + quad.entry.width as i32 <= right,
                "a piece painted in one cell's colour drew outside that cell: {:?}",
                quad
            );
        }
    }

    #[test]
    fn a_selection_over_half_a_ligature_leaves_the_glyph_whole() {
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 4, |painter| {
            painter.styled(0, 0, "=", |cell| {
                cell.bg = WireColor::Named(4).pack();
            });
            painter.text(0, 1, ">");
        });
        let equals = nominal_in(&mut cache, '=');
        assert!(
            !draws(&scene, equals),
            "a background change broke the ligature"
        );
        assert!(
            scene
                .rects
                .iter()
                .any(|rect| rect.color == Paints::default().ansi[4] && rect.width == 8),
            "the selected cell lost its background"
        );
    }

    #[test]
    fn a_bold_change_inside_a_sequence_breaks_the_run() {
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 4, |painter| {
            painter.text(0, 0, "=");
            painter.styled(0, 1, ">", |cell| {
                cell.attrs |= ATTR_BOLD;
            });
        });
        let equals = nominal_in(&mut cache, '=');
        assert!(
            draws(&scene, equals),
            "the run ligated across a face change"
        );
    }

    #[test]
    fn an_underline_or_strike_change_does_not_break_the_run() {
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 4, |painter| {
            painter.text(0, 0, "=");
            painter.styled(0, 1, ">", |cell| {
                cell.attrs |= ATTR_STRIKE;
            });
        });
        let equals = nominal_in(&mut cache, '=');
        let stroke = cache.metrics().stroke;
        assert!(
            !draws(&scene, equals),
            "a strike on one cell broke the ligature"
        );
        assert!(
            scene
                .rects
                .iter()
                .any(|rect| rect.height == stroke && rect.x == 8),
            "the struck cell lost its line"
        );
    }

    #[test]
    fn a_link_change_does_not_break_the_run() {
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 4, |painter| {
            painter.links(&[(1, "https://example.com")]);
            painter.text(0, 0, "=");
            painter.linked(0, 1, ">", 1);
        });
        let equals = nominal_in(&mut cache, '=');
        assert!(!draws(&scene, equals), "a link boundary broke the ligature");
    }

    fn pane(x: u16, cols: u16, left: u8, flags: u8) -> PaneRect {
        PaneRect {
            x,
            y: 0,
            cols,
            rows: 1,
            top: 0,
            bottom: 0,
            left,
            right: 0,
            flags,
        }
    }

    #[test]
    fn a_pane_boundary_breaks_the_run() {
        let geometry = GeometryRecord {
            panes: vec![
                pane(0, 1, 0, PANE_SELECTABLE),
                pane(1, 3, 0, PANE_SELECTABLE),
            ],
        };
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 4, |painter| {
            painter.geometry(&geometry);
            painter.text(0, 0, "->");
        });
        let dash = nominal_in(&mut cache, '-');
        assert!(
            draws(&scene, dash),
            "the run ligated across a pane boundary"
        );
    }

    #[test]
    fn a_run_inside_one_pane_still_ligates_when_a_geometry_record_is_present() {
        let geometry = GeometryRecord {
            panes: vec![pane(0, 4, 0, PANE_SELECTABLE)],
        };
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 4, |painter| {
            painter.geometry(&geometry);
            painter.text(0, 0, "->");
        });
        let dash = nominal_in(&mut cache, '-');
        assert!(!draws(&scene, dash), "a run inside one pane did not ligate");
    }

    #[test]
    fn a_frame_cell_is_not_part_of_any_run() {
        let geometry = GeometryRecord {
            panes: vec![pane(0, 4, 1, PANE_FRAMED | PANE_SELECTABLE)],
        };
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 4, |painter| {
            painter.geometry(&geometry);
            painter.text(0, 0, "->");
        });
        let dash = nominal_in(&mut cache, '-');
        assert!(draws(&scene, dash), "a frame cell joined a run");
    }

    #[test]
    fn a_wide_spacer_breaks_the_run() {
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 6, |painter| {
            painter.text(0, 0, "-");
            painter.wide(0, 1, '\u{4f60}');
            painter.text(0, 3, ">");
        });
        let dash = nominal_in(&mut cache, '-');
        let greater = nominal_in(&mut cache, '>');
        assert!(draws(&scene, dash) && draws(&scene, greater));
    }

    #[test]
    fn the_cursor_cell_breaks_the_run_and_the_ligature_reforms_when_it_leaves() {
        let mut cache = cache_with(true);
        let under_cursor = scene_through(&mut cache, 1, 4, |painter| {
            painter.text(0, 0, "->");
            painter.cursor(0, 1, CursorShape::Beam);
        });
        let dash = nominal_in(&mut cache, '-');
        assert!(
            draws(&under_cursor, dash),
            "the ligature survived the cursor sitting in it"
        );

        let cursor_away = scene_through(&mut cache, 1, 4, |painter| {
            painter.text(0, 0, "->");
            painter.cursor(0, 3, CursorShape::Beam);
        });
        assert!(
            !draws(&cursor_away, dash),
            "the ligature did not reform once the cursor left"
        );
    }

    #[test]
    fn an_invisible_cursor_leaves_the_ligature_alone() {
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 4, |painter| {
            painter.text(0, 0, "->");
        });
        let dash = nominal_in(&mut cache, '-');
        assert!(!draws(&scene, dash));
    }

    #[test]
    fn a_fallback_face_is_never_shaped_with_the_primary() {
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 6, |painter| {
            painter.text(0, 0, "->");
            painter.wide(0, 2, '\u{6f22}');
        });
        let dash = nominal_in(&mut cache, '-');
        let han = match cache.glyph(FaceStyle::Regular, '\u{6f22}', 2) {
            Lookup::Rendered(rendered) => rendered.entry,
            other => panic!("the fallback glyph did not rasterize: {:?}", other),
        };
        assert!(!draws(&scene, dash), "the ASCII run did not ligate");
        assert!(draws(&scene, han), "the fallback glyph was not drawn");
    }

    #[test]
    fn a_run_of_arabic_is_left_to_the_unshaped_path() {
        let plain = scene_with(false, 1, 6, |painter| {
            painter.text(0, 0, "\u{645}\u{631}\u{62d}\u{628}\u{627}")
        });
        let ligated = scene_with(true, 1, 6, |painter| {
            painter.text(0, 0, "\u{645}\u{631}\u{62d}\u{628}\u{627}")
        });
        assert_eq!(plain, ligated);
    }

    #[test]
    fn a_run_of_cyrillic_is_left_to_the_unshaped_path() {
        let plain = scene_with(false, 1, 6, |painter| painter.text(0, 0, "\u{434}\u{430}"));
        let ligated = scene_with(true, 1, 6, |painter| painter.text(0, 0, "\u{434}\u{430}"));
        assert_eq!(plain, ligated);
    }

    #[test]
    fn a_three_cell_cluster_draws_one_glyph_over_three_cells() {
        let mut cache = cache_with(true);
        let font = cache.primary_of(FaceStyle::Regular);
        let metrics = cache.metrics();
        let nominal = nominal_in(&mut cache, 'M');
        let id = stack(true).lookup(FaceStyle::Regular, 'M').unwrap().glyph;

        let mut scene = empty_scene(4, metrics);
        let mut shaped = vec![false; 4];
        push_run(
            &mut scene,
            &mut cache,
            &run(1, 3, "===", font),
            &[ShapedGlyph {
                cell: 0,
                span: 3,
                glyph: id,
                dx: 0,
                dy: 0,
            }],
            &uniform_cells(4, '='),
            0,
            metrics,
            &mut shaped,
        );

        assert_eq!(scene.glyphs.len(), 1, "{:?}", scene.glyphs);
        assert_eq!(
            scene.glyphs[0].x,
            metrics.width as i32 + nominal.left,
            "the cluster was not placed at its first cell"
        );
        assert_eq!(scene.glyphs[0].entry, nominal);
        assert_eq!(shaped, vec![false, true, true, true]);
    }

    #[test]
    fn a_cluster_mapping_that_does_not_tile_the_run_is_refused() {
        let mut cache = cache_with(true);
        let font = cache.primary_of(FaceStyle::Regular);
        let metrics = cache.metrics();
        let id = stack(true).lookup(FaceStyle::Regular, 'M').unwrap().glyph;
        let mut scene = empty_scene(3, metrics);
        let mut shaped = vec![false; 3];
        push_run(
            &mut scene,
            &mut cache,
            &run(0, 2, "===", font),
            &[ShapedGlyph {
                cell: 0,
                span: 2,
                glyph: id,
                dx: 0,
                dy: 0,
            }],
            &uniform_cells(3, '='),
            0,
            metrics,
            &mut shaped,
        );
        assert!(scene.glyphs.is_empty());
        assert_eq!(shaped, vec![false, false, false]);
    }

    #[test]
    fn a_glyph_wider_than_its_run_is_clipped_to_it() {
        let wide = AtlasEntry {
            x: 100,
            y: 0,
            width: 40,
            height: 10,
            left: -4,
            top: 8,
        };
        assert_eq!(
            clipped(wide, 0, 0, 16),
            Some((0, AtlasEntry { width: 16, ..wide }))
        );
        assert_eq!(
            clipped(wide, -4, 0, 16),
            Some((
                0,
                AtlasEntry {
                    x: 104,
                    width: 16,
                    ..wide
                }
            ))
        );
        assert_eq!(
            clipped(wide, 8, 0, 16),
            Some((8, AtlasEntry { width: 8, ..wide }))
        );
        assert_eq!(clipped(wide, 20, 0, 16), None);
        assert_eq!(clipped(wide, -60, 0, 16), None);
    }

    #[test]
    fn a_glyph_inside_its_run_is_not_clipped() {
        let narrow = AtlasEntry {
            x: 4,
            y: 4,
            width: 6,
            height: 10,
            left: 1,
            top: 8,
        };
        assert_eq!(clipped(narrow, 9, 8, 24), Some((9, narrow)));
    }

    #[test]
    fn the_shape_cache_answers_the_second_time_without_reshaping() {
        let mut fonts = stack(true);
        let font = fonts.primary_of(FaceStyle::Regular);
        assert_eq!(fonts.shaped_runs(), 0);
        let first = fonts.shape(font, "->");
        assert_eq!(fonts.shaped_runs(), 1);
        let second = fonts.shape(font, "->");
        assert_eq!(fonts.shaped_runs(), 1);
        assert_eq!(first, second);
    }

    #[test]
    fn a_font_rebuild_starts_the_shape_cache_empty() {
        let mut fonts = stack(true);
        let font = fonts.primary_of(FaceStyle::Regular);
        fonts.shape(font, "->");
        assert_eq!(fonts.shaped_runs(), 1);
        let rebuilt = stack(true);
        assert_eq!(
            rebuilt.shaped_runs(),
            0,
            "a rebuilt stack inherited shaped runs"
        );
    }

    #[test]
    fn the_toggle_decides_whether_runs_are_shaped_at_all() {
        assert!(cache_with(true).shapes_runs());
        assert!(!cache_with(false).shapes_runs());
    }

    #[test]
    fn the_differential_lane_sees_two_cells_where_the_scene_draws_a_ligature() {
        let state = Painter::state(1, 4, |painter| painter.text(0, 0, "a->b"));
        let mut cache = cache_with(true);
        let scene = build_at(
            &state,
            &mut cache,
            BlinkPhase::On,
            &Paints::default(),
            CursorOptions::default(),
            None,
            None,
        );
        let dash = nominal_in(&mut cache, '-');
        assert!(!draws(&scene, dash), "the sequence did not ligate");

        let projected = crate::equivalence::project_screen_buffer(state.screen());
        assert_eq!(projected.cells[0][1].text, "-");
        assert_eq!(projected.cells[0][2].text, ">");
        assert_eq!(
            projected.cells[0].len(),
            4,
            "shaping changed what the lane counts"
        );
    }

    #[test]
    fn a_hidden_run_draws_its_pieces_in_the_background_colour() {
        let mut cache = cache_with(true);
        let scene = scene_through(&mut cache, 1, 4, |painter| {
            painter.styled(0, 0, "->", |cell| {
                cell.attrs |= ATTR_HIDDEN;
            });
        });
        assert!(scene
            .glyphs
            .iter()
            .all(|quad| quad.color == Paints::default().background));
    }
}

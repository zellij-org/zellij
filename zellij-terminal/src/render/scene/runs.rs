use crate::emulation::screen_buffer::Occupancy;
use crate::render::atlas::{AtlasEntry, GlyphCache, Lookup};
use crate::render::scene::CellPaint;
use crate::render::scene::GlyphQuad;
use crate::render::scene::RowCell;
use crate::render::scene::RowScene;
use crate::render::scene::Run;
use crate::render::scene::RunKey;
use crate::render::scene::WHOLE_GRID;
use crate::text::font::{CellMetrics, GlyphContent, ShapedGlyph};
use zellij_utils::structured_render::GeometryRecord;

pub(super) fn run_key(
    cache: &mut GlyphCache,
    entry: &RowCell,
    geometry: &GeometryRecord,
    row: usize,
    col: usize,
    cursor: Option<(usize, usize)>,
) -> Option<RunKey> {
    if entry.occupancy != Occupancy::Single
        || !entry.paint.draw_glyph
        || entry.composing
        || entry.held
        || entry.complex
    {
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

pub(super) fn is_shapeable(character: char) -> bool {
    character.is_ascii_graphic()
}

pub(super) fn region_of(geometry: &GeometryRecord, row: usize, col: usize) -> Option<usize> {
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

pub(super) fn next_run(keys: &[Option<RunKey>], from: usize) -> Option<(usize, usize, RunKey)> {
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

#[derive(Clone, Copy)]
pub(super) struct ShapedRun<'a> {
    pub(super) run: &'a Run,
    pub(super) glyphs: &'a [ShapedGlyph],
}

pub(super) fn push_run(
    out: &mut RowScene,
    cache: &mut GlyphCache,
    ShapedRun { run, glyphs }: ShapedRun<'_>,
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

pub(super) fn touched_cells(
    x: i32,
    width: u32,
    cell_width: u32,
    run: &Run,
) -> std::ops::RangeInclusive<usize> {
    let first = (x.max(0) as u32 / cell_width) as usize;
    let last = ((x + width as i32 - 1).max(0) as u32 / cell_width) as usize;
    first.max(run.start)..=last.min(run.end)
}

pub(super) fn push_run_glyph(
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

pub(super) fn tiles_the_run(glyphs: &[ShapedGlyph], cells: usize) -> bool {
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

pub(super) fn clipped(
    entry: AtlasEntry,
    x: i32,
    left: i32,
    right: i32,
) -> Option<(i32, AtlasEntry)> {
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

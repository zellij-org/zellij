use crate::emulation::screen_buffer::Occupancy;
use crate::render::atlas::{ClusterLookup, GlyphCache, GlyphKey, RenderedGlyph};
use crate::render::scene::glyphs::is_invisible;
use crate::render::scene::glyphs::push_glyphs;
use crate::render::scene::runs::clipped;
use crate::render::scene::runs::push_run_glyph;
use crate::render::scene::CellPaint;
use crate::render::scene::RowCell;
use crate::render::scene::RowContext;
use crate::render::scene::RowScene;
use crate::text::font::CellMetrics;
use unicode_width::UnicodeWidthStr;

pub(super) fn mark_units(cells: &mut [RowCell]) {
    let cols = cells.len();
    let mut col = 0;
    while col < cols {
        if cells[col].occupancy == Occupancy::WideTail {
            col += 1;
            continue;
        }
        let mut end = col + 1;
        let mut members = 1;
        loop {
            while end < cols && cells[end].occupancy == Occupancy::WideTail {
                end += 1;
            }
            if end < cols && cells[end].continuation {
                end += 1;
                members += 1;
                continue;
            }
            break;
        }
        let complex = members > 1 || cells[col].cluster;
        cells[col].unit_end = end;
        if complex {
            for entry in cells[col..end].iter_mut() {
                entry.complex = true;
            }
        }
        col = end;
    }
}

pub(super) fn same_ink(left: &CellPaint, right: &CellPaint) -> bool {
    left.foreground == right.foreground && left.tint == right.tint && left.hidden == right.hidden
}

#[derive(Debug, Clone, Copy)]
pub(super) struct UnitCols {
    pub(super) row: usize,
    pub(super) start: usize,
    pub(super) end: usize,
}

pub(super) fn push_unit(
    out: &mut RowScene,
    context: &RowContext<'_>,
    cache: &mut GlyphCache,
    cells: &[RowCell],
    text: &mut String,
    UnitCols { row, start, end }: UnitCols,
    origin_y: i32,
) {
    let metrics = context.metrics;
    let end = end.min(cells.len()).max(start + 1);
    let heads: Vec<usize> = (start..end)
        .filter(|col| cells[*col].occupancy != Occupancy::WideTail)
        .collect();
    let key = match heads.as_slice() {
        [only] => match context.cluster_at(row, *only) {
            Some(cluster) => GlyphKey::shared(&cluster),
            None => GlyphKey::Char(cells[*only].cell.character()),
        },
        _ => {
            text.clear();
            for col in &heads {
                context.push_text_to(row, *col, text);
            }
            GlyphKey::of(text)
        },
    };
    let unit_text: std::borrow::Cow<'_, str> = match &key {
        GlyphKey::Char(character) => std::borrow::Cow::Owned(character.to_string()),
        GlyphKey::Text(shared) => std::borrow::Cow::Borrowed(&**shared),
    };
    if unit_text.chars().all(is_invisible) {
        return;
    }
    let columns = end - start;
    let drawn = UnicodeWidthStr::width(&*unit_text).max(1);
    let budget = drawn.min(columns).max(1) as u32;
    let style = cells[start].paint.style;
    match cache.cluster_glyph(style, &key, budget) {
        ClusterLookup::Sprite(rendered) => {
            push_unit_sprite(out, rendered, cells, start, end, origin_y, metrics);
        },
        ClusterLookup::Blank => {},
        ClusterLookup::PerCell => {
            for col in heads {
                push_unit_cell(out, context, cache, cells, row, col, origin_y);
            }
        },
    }
}

pub(super) fn push_unit_cell(
    out: &mut RowScene,
    context: &RowContext<'_>,
    cache: &mut GlyphCache,
    cells: &[RowCell],
    row: usize,
    col: usize,
    origin_y: i32,
) {
    let metrics = context.metrics;
    let entry = &cells[col];
    if entry.held || !entry.paint.draw_glyph {
        return;
    }
    let mut end = col + 1;
    while end < cells.len() && cells[end].occupancy == Occupancy::WideTail {
        end += 1;
    }
    let origin_x = (col as u32 * metrics.width) as i32;
    if let Some(cluster) = context.cluster_at(row, col) {
        let budget = (end - col) as u32;
        match cache.cluster_glyph(entry.paint.style, &GlyphKey::shared(&cluster), budget) {
            ClusterLookup::Sprite(rendered) => {
                push_unit_sprite(out, rendered, cells, col, end, origin_y, metrics);
                return;
            },
            ClusterLookup::Blank => return,
            ClusterLookup::PerCell => {},
        }
    }
    push_glyphs(out, cache, entry, (origin_x, origin_y), metrics, 0);
}

pub(super) fn push_unit_sprite(
    out: &mut RowScene,
    rendered: RenderedGlyph,
    cells: &[RowCell],
    start: usize,
    end: usize,
    origin_y: i32,
    metrics: CellMetrics,
) {
    let baseline = origin_y + metrics.baseline as i32;
    let left = (start as u32 * metrics.width) as i32;
    let x = left + rendered.entry.left;
    let y = baseline - rendered.entry.top;
    let head = &cells[start].paint;
    let one_paint = cells[start..end]
        .iter()
        .all(|entry| !entry.held && same_ink(&entry.paint, head));
    if one_paint {
        let right = (end as u32 * metrics.width) as i32;
        if let Some((x, entry)) = clipped(rendered.entry, x, left, right) {
            push_run_glyph(out, x, y, entry, rendered.content, head);
        }
        return;
    }
    for col in start..end {
        if cells[col].held {
            continue;
        }
        let cell_left = (col as u32 * metrics.width) as i32;
        let cell_right = cell_left + metrics.width as i32;
        if let Some((x, entry)) = clipped(rendered.entry, x, cell_left, cell_right) {
            push_run_glyph(out, x, y, entry, rendered.content, &cells[col].paint);
        }
    }
}

use crate::emulation::screen_buffer::Occupancy;
use crate::render::atlas::{GlyphCache, Lookup};
use crate::render::scene::runs::region_of;
use crate::render::scene::CellPaint;
use crate::render::scene::GlyphQuad;
use crate::render::scene::Rect;
use crate::render::scene::RowCell;
use crate::render::scene::RowScene;
use crate::text::font::{CellMetrics, GlyphContent};
use zellij_utils::structured_render::GeometryRecord;

pub(super) fn push_glyphs(
    out: &mut RowScene,
    cache: &mut GlyphCache,
    entry: &RowCell,
    (origin_x, origin_y): (i32, i32),
    metrics: CellMetrics,
    spill: u32,
) {
    let (cell, occupancy, paint) = (entry.cell, entry.occupancy, &entry.paint);
    let baseline = origin_y + metrics.baseline as i32;
    let budget = if occupancy == Occupancy::WideHead {
        (cell.width as u32).clamp(2, 3)
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

pub(super) fn spill_room(
    cells: &[RowCell],
    geometry: &GeometryRecord,
    row: usize,
    col: usize,
) -> u32 {
    let Some(next) = cells.get(col + 1) else {
        return 0;
    };
    let spreads = cells[col].occupancy == Occupancy::Single
        && next.occupancy == Occupancy::Single
        && !next.composing
        && !next.held
        && !next.complex
        && next.cell.character() == ' '
        && region_of(geometry, row, col).is_some()
        && region_of(geometry, row, col) == region_of(geometry, row, col + 1);
    u32::from(spreads)
}

pub(super) fn is_invisible(character: char) -> bool {
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

pub(super) fn push_tofu(
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

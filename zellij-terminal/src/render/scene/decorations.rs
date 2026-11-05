use crate::emulation::color::Srgb;
use crate::emulation::screen_buffer::CursorShape;
use crate::render::scene::CellPaint;
use crate::render::scene::Rect;
use crate::render::scene::RowContext;
use crate::render::scene::RowScene;
use crate::text::font::CellMetrics;
use zellij_utils::structured_render::{
    WireCell, WireColor, ATTR_STRIKE, UNDERLINE_CURLY, UNDERLINE_DASHED, UNDERLINE_DOTTED,
    UNDERLINE_DOUBLE, UNDERLINE_STRAIGHT,
};

pub(super) fn push_decorations(
    out: &mut RowScene,
    context: &RowContext<'_>,
    cell: WireCell,
    paint: &CellPaint,
    (origin_x, origin_y): (i32, i32),
    inkless: bool,
) {
    let (metrics, underline_stroke) = (context.metrics, context.underline_stroke);
    let (paints, contrast) = (context.paints, context.contrast);
    let underline = cell.underline_style();
    let struck = cell.has(ATTR_STRIKE);
    if underline == 0 && !struck {
        return;
    }

    let color = match WireColor::unpack(cell.underline_color) {
        WireColor::Default if inkless => return,
        WireColor::Default => paint.foreground,
        _ => {
            let underline = paints.foreground(cell.underline_color);
            match contrast {
                Some(contrast) if !paint.hidden => contrast.apply(underline, paint.background),
                _ => underline,
            }
        },
    };
    let strike = metrics.stroke.max(1);
    let stroke = underline_stroke.max(1);
    let underline_y = origin_y + metrics.underline_top as i32;
    let bottom = origin_y + metrics.height as i32;
    let mut line = |y: i32, x: i32, width: u32, height: u32| {
        out.rects.push(Rect {
            x,
            y: y.clamp(origin_y, (bottom - height as i32).max(origin_y)),
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
        line(y, origin_x, metrics.width, strike);
    }
}

pub(super) fn push_hover_underline(
    out: &mut RowScene,
    paint: &CellPaint,
    origin_x: i32,
    origin_y: i32,
    metrics: CellMetrics,
    underline_stroke: u32,
) {
    let stroke = underline_stroke.max(1);
    let floor = origin_y + metrics.height as i32 - stroke as i32;
    out.rects.push(Rect {
        x: origin_x,
        y: (origin_y + metrics.underline_top as i32).clamp(origin_y, floor),
        width: metrics.width,
        height: stroke,
        color: paint.foreground,
    });
}

pub(super) const DOTTED_PERIOD: u32 = 3;
pub(super) const WAVE: [i32; 8] = [0, 1, 2, 1, 0, -1, -2, -1];

pub(super) fn columns(origin_x: i32, width: u32, period: u32) -> impl Iterator<Item = (i32, u32)> {
    let period = period.max(1);
    (0..width).map(move |offset| {
        let x = origin_x + offset as i32;
        (x, x.rem_euclid(period as i32) as u32)
    })
}

pub(super) fn push_hollow_cursor(
    out: &mut RowScene,
    origin_x: i32,
    origin_y: i32,
    metrics: CellMetrics,
    color: Srgb,
) {
    let stroke = metrics.stroke.max(1).min(metrics.width).min(metrics.height);
    let (width, height) = (metrics.width, metrics.height);
    for (x, y, w, h) in [
        (0, 0, width, stroke),
        (0, (height - stroke) as i32, width, stroke),
        (0, 0, stroke, height),
        ((width - stroke) as i32, 0, stroke, height),
    ] {
        out.rects.push(Rect {
            x: origin_x + x,
            y: origin_y + y,
            width: w,
            height: h,
            color,
        });
    }
}

pub(super) fn push_cursor(
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

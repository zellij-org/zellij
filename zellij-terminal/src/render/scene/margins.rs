use crate::emulation::color::{Paints, Srgb};
use crate::emulation::terminal::TerminalState;
use crate::render::scene::paint_of;
use crate::render::scene::Rect;
use crate::render::scene::Transparency;
use crate::text::font::CellMetrics;
use zellij_utils::structured_render::{WireColor, ATTR_REVERSE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Margins {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

pub fn padding_rects(
    state: &TerminalState,
    paints: &Paints,
    metrics: CellMetrics,
    transparency: Transparency,
    margins: Margins,
) -> Vec<Rect> {
    let size = state.size();
    let mut rects: Vec<Rect> = Vec::new();
    if size.rows == 0 || size.cols == 0 {
        return rects;
    }
    let edge = |row: usize, col: usize| -> Option<Srgb> {
        let cell = state.screen().cell(row, col);
        let paint = paint_of(cell, state.occupancy(row, col), paints, None);
        let explicit =
            cell.has(ATTR_REVERSE) || !matches!(WireColor::unpack(cell.bg), WireColor::Default);
        let fills = if transparency.see_through_background() {
            explicit
        } else {
            paint.background != paints.background
        };
        fills.then_some(paint.background)
    };
    let (width, height) = (metrics.width as i32, metrics.height as i32);
    let (grid_width, grid_height) = (width * size.cols as i32, height * size.rows as i32);
    let (left, top) = (margins.left as i32, margins.top as i32);
    let (last_row, last_col) = (size.rows - 1, size.cols - 1);

    let mut push = |color: Option<Srgb>, x: i32, y: i32, w: u32, h: u32| {
        let Some(color) = color else {
            return;
        };
        if w == 0 || h == 0 {
            return;
        }
        if let Some(previous) = rects.last_mut() {
            if previous.color == color
                && previous.x == x
                && previous.width == w
                && previous.y + previous.height as i32 == y
            {
                previous.height += h;
                return;
            }
            if previous.color == color
                && previous.y == y
                && previous.height == h
                && previous.x + previous.width as i32 == x
            {
                previous.width += w;
                return;
            }
        }
        rects.push(Rect {
            x,
            y,
            width: w,
            height: h,
            color,
        });
    };

    for row in 0..size.rows {
        let y = row as i32 * height;
        push(edge(row, 0), -left, y, margins.left, metrics.height);
    }
    for row in 0..size.rows {
        let y = row as i32 * height;
        push(
            edge(row, last_col),
            grid_width,
            y,
            margins.right,
            metrics.height,
        );
    }
    push(edge(0, 0), -left, -top, margins.left, margins.top);
    for col in 0..size.cols {
        push(
            edge(0, col),
            col as i32 * width,
            -top,
            metrics.width,
            margins.top,
        );
    }
    push(
        edge(0, last_col),
        grid_width,
        -top,
        margins.right,
        margins.top,
    );
    push(
        edge(last_row, 0),
        -left,
        grid_height,
        margins.left,
        margins.bottom,
    );
    for col in 0..size.cols {
        push(
            edge(last_row, col),
            col as i32 * width,
            grid_height,
            metrics.width,
            margins.bottom,
        );
    }
    push(
        edge(last_row, last_col),
        grid_width,
        grid_height,
        margins.right,
        margins.bottom,
    );
    rects
}

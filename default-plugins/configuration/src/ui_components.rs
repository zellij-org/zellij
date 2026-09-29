use crate::WIDTH_BREAKPOINTS;
use std::cell::Cell;
use zellij_tile::prelude::*;

thread_local! {
    static ORIGIN: Cell<(usize, usize)> = Cell::new((0, 0));
    static CLOSE_REQUESTED: Cell<bool> = Cell::new(false);
    static CLOSE_DIRECTLY: Cell<bool> = Cell::new(true);
}

pub fn set_origin(x: usize, y: usize) {
    ORIGIN.with(|origin| origin.set((x, y)));
}

fn origin() -> (usize, usize) {
    ORIGIN.with(|origin| origin.get())
}

pub fn set_close_directly(close_directly: bool) {
    CLOSE_DIRECTLY.with(|c| c.set(close_directly));
}

pub fn request_close() {
    if CLOSE_DIRECTLY.with(|c| c.get()) {
        close_self();
    } else {
        CLOSE_REQUESTED.with(|c| c.set(true));
    }
}

pub fn take_close_request() -> bool {
    CLOSE_REQUESTED.with(|c| c.replace(false))
}

pub fn print_text_with_coordinates(
    text: Text,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
) {
    let (origin_x, origin_y) = origin();
    zellij_tile::prelude::print_text_with_coordinates(
        text,
        x + origin_x,
        y + origin_y,
        width,
        height,
    );
}

pub fn print_nested_list_with_coordinates(
    items: Vec<NestedListItem>,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
) {
    let (origin_x, origin_y) = origin();
    zellij_tile::prelude::print_nested_list_with_coordinates(
        items,
        x + origin_x,
        y + origin_y,
        width,
        height,
    );
}

pub fn info_line(
    rows: usize,
    cols: usize,
    ui_size: usize,
    notification: Option<String>,
    warning_text: Option<String>,
    widths: Option<(usize, usize, usize)>,
) {
    let top_coordinates = if rows > 14 {
        (rows.saturating_sub(ui_size) / 2) + 14
    } else {
        (rows.saturating_sub(ui_size) / 2) + 10
    };
    let left_padding = if let Some(widths) = widths {
        if cols >= widths.0 {
            cols.saturating_sub(widths.0) / 2
        } else if cols >= widths.1 {
            cols.saturating_sub(widths.1) / 2
        } else {
            cols.saturating_sub(widths.2) / 2
        }
    } else {
        if cols >= WIDTH_BREAKPOINTS.0 {
            cols.saturating_sub(WIDTH_BREAKPOINTS.0) / 2
        } else {
            cols.saturating_sub(WIDTH_BREAKPOINTS.1) / 2
        }
    };
    if let Some(notification) = notification {
        print_text_with_coordinates(
            Text::from(notification).color_range(3, ..),
            left_padding,
            top_coordinates,
            None,
            None,
        );
    } else if let Some(warning_text) = warning_text {
        print_text_with_coordinates(
            Text::from(warning_text).color_range(3, ..),
            left_padding,
            top_coordinates,
            None,
            None,
        );
    }
}

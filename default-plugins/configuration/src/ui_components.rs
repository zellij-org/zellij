use std::cell::Cell;
use zellij_tile::prelude::*;

thread_local! {
    static CLOSE_REQUESTED: Cell<bool> = Cell::new(false);
    static CLOSE_DIRECTLY: Cell<bool> = Cell::new(true);
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

pub fn print_link(link: &str, x: usize, y: usize, color: Option<PaletteColor>, hovered: bool) {
    let color = match color {
        Some(PaletteColor::Rgb((r, g, b))) => format!("\u{1b}[38;2;{};{};{}m", r, g, b),
        Some(PaletteColor::EightBit(index)) => format!("\u{1b}[38;5;{}m", index),
        None => String::new(),
    };
    let hover = if hovered { "\u{1b}[3;4m" } else { "" };
    print!(
        "\u{1b}[{};{}H\u{1b}[0m\u{1b}[1m{}{}{}\u{1b}[0m",
        y + 1,
        x + 1,
        color,
        hover,
        link
    );
}

//! Some general utility functions.

use std::{iter, str::from_utf8};

use strip_ansi_escapes::strip;

use colors_transform::{Color, Rgb};
use zellij_tile::data::{Palette, PaletteSource, Theme};

fn ansi_len(s: &str) -> usize {
    from_utf8(&strip(s.as_bytes()).unwrap())
        .unwrap()
        .chars()
        .count()
}

pub fn adjust_to_size(s: &str, rows: usize, columns: usize) -> String {
    s.lines()
        .map(|l| {
            let actual_len = ansi_len(l);
            if actual_len > columns {
                let mut line = String::from(l);
                line.truncate(columns);
                line
            } else {
                [l, &str::repeat(" ", columns - ansi_len(l))].concat()
            }
        })
        .chain(iter::repeat(str::repeat(" ", columns)))
        .take(rows)
        .collect::<Vec<_>>()
        .join("\n\r")
}

// Colors
pub mod colors {
    pub const WHITE: (u8, u8, u8) = (238, 238, 238);
    pub const GREEN: (u8, u8, u8) = (175, 255, 0);
    pub const GRAY: (u8, u8, u8) = (68, 68, 68);
    pub const BRIGHT_GRAY: (u8, u8, u8) = (138, 138, 138);
    pub const RED: (u8, u8, u8) = (135, 0, 0);
    pub const ORANGE: (u8, u8, u8) = (215, 95, 0);
    pub const BLACK: (u8, u8, u8) = (0, 0, 0);
}

pub fn hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let rgb = Rgb::from_hex_str(hex)
        .expect("The passed argument must be a valid hex color")
        .as_tuple();
    (rgb.0 as u8, rgb.1 as u8, rgb.2 as u8)
}

pub fn default_palette() -> Palette {
    Palette {
        source: PaletteSource::Default,
        theme: Theme::Dark,
        fg: colors::BRIGHT_GRAY,
        bg: colors::GRAY,
        black: colors::BLACK,
        red: colors::RED,
        green: colors::GREEN,
        yellow: colors::GRAY,
        blue: colors::GRAY,
        magenta: colors::GRAY,
        cyan: colors::GRAY,
        white: colors::WHITE,
        orange: colors::ORANGE,
    }
}

// Dark magic
pub fn detect_theme(bg: (u8, u8, u8)) -> Theme {
    let (r, g, b) = bg;
    // HSP, P stands for perceived brightness
    let hsp: f64 = (0.299 * (r as f64 * r as f64)
        + 0.587 * (g as f64 * g as f64)
        + 0.114 * (b as f64 * b as f64))
        .sqrt();
    match hsp > 127.5 {
        true => Theme::Light,
        false => Theme::Dark,
    }
}

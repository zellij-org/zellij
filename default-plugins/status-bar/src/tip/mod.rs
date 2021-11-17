pub mod data;
pub mod utils;

use crate::LinePart;
use zellij_tile::prelude::*;

pub type TipFn = fn(Palette) -> LinePart;

#[derive(Debug)]
pub struct TipFnMap {
    pub short: TipFn,
    pub medium: TipFn,
    pub full: TipFn,
}

#[macro_export]
macro_rules! palette_match {
    ($palette_color:expr) => {
        match $palette_color {
            PaletteColor::Rgb((r, g, b)) => RGB(r, g, b),
            PaletteColor::EightBit(color) => Fixed(color),
        }
    };
}

#[macro_export]
macro_rules! strings {
    ($ANSIStrings:expr) => {{
        let strings: &[ANSIString<'static>] = $ANSIStrings;

        let ansi_strings = ANSIStrings(strings);

        LinePart {
            part: format!("{}", ansi_strings),
            len: unstyled_len(&ansi_strings),
        }
    }};
}

use ansi_term::{
    unstyled_len, ANSIString, ANSIStrings,
    Color::{Fixed, RGB},
    Style,
};

use crate::LinePart;
use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;

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

pub fn move_focus_hjkl_tab_switch_full(palette: Palette) -> LinePart {
    // Tip: When changing focus with Alt + <←↓↑→> moving off screen left/right focuses the next tab.
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("When changing focus with "),
        Style::new().fg(orange_color).bold().paint("Alt"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<←↓↑→>"),
        Style::new().paint(" moving off screen left/right focuses the next tab."),
    ])
}

pub fn move_focus_hjkl_tab_switch_medium(palette: Palette) -> LinePart {
    // Tip: Changing focus with Alt + <←↓↑→> off screen focuses the next tab.
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);
    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("Changing focus with "),
        Style::new().fg(orange_color).bold().paint("Alt"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<←↓↑→>"),
        Style::new().paint(" off screen focuses the next tab."),
    ])
}

pub fn move_focus_hjkl_tab_switch_short(palette: Palette) -> LinePart {
    // Alt + <←↓↑→> off screen edge focuses next tab.
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().fg(orange_color).bold().paint(" Alt"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<←↓↑→>"),
        Style::new().paint(" off screen edge focuses next tab."),
    ])
}

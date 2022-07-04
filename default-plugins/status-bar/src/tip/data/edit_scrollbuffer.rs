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

pub fn edit_scrollbuffer_full(palette: Palette) -> LinePart {
    // Tip: Search through the scrollbuffer using your default $EDITOR with
    // Ctrl + <s> + <e>
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("Search through the scrollbuffer using your default "),
        Style::new().fg(green_color).bold().paint("$EDITOR"),
        Style::new().paint(" with "),
        Style::new().fg(orange_color).bold().paint("Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<s>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<e>"),
    ])
}

pub fn edit_scrollbuffer_medium(palette: Palette) -> LinePart {
    // Tip: Search the scrollbuffer using your $EDITOR with
    // Ctrl + <s> + <e>
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("Search the scrollbuffer using your "),
        Style::new().fg(green_color).bold().paint("$EDITOR"),
        Style::new().paint(" with "),
        Style::new().fg(orange_color).bold().paint("Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<s>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<e>"),
    ])
}

pub fn edit_scrollbuffer_short(palette: Palette) -> LinePart {
    // Search using $EDITOR with
    // Ctrl + <s> + <e>
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Search using "),
        Style::new().fg(green_color).bold().paint("$EDITOR"),
        Style::new().paint(" with "),
        Style::new().fg(orange_color).bold().paint("Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<s>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<e>"),
    ])
}

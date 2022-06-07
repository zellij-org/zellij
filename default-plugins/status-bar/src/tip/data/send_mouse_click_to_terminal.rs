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

pub fn mouse_click_to_terminal_full(palette: Palette) -> LinePart {
    // Tip: SHIFT + <mouse-click> bypasses Zellij and sends the mouse click directly to the terminal
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().fg(orange_color).bold().paint("SHIFT"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<mouse-click>"),
        Style::new().paint(" bypasses Zellij and sends the mouse click directly to the terminal."),
    ])
}

pub fn mouse_click_to_terminal_medium(palette: Palette) -> LinePart {
    // Tip: SHIFT + <mouse-click> sends the click directly to the terminal
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);
    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().fg(orange_color).bold().paint("SHIFT"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<mouse-click>"),
        Style::new().paint(" sends the click directly to the terminal."),
    ])
}

pub fn mouse_click_to_terminal_short(palette: Palette) -> LinePart {
    // Tip: SHIFT + <mouse-click>  => sends click to terminal.
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().fg(orange_color).bold().paint("SHIFT"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<mouse-click>"),
        Style::new().paint(" => sends click to terminal."),
    ])
}

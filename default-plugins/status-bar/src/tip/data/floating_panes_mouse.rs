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

pub fn floating_panes_mouse_full(palette: Palette) -> LinePart {
    // Tip: Toggle floating panes with Ctrl + <p> + <w> and move them with keyboard or mouse
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("Toggle floating panes with "),
        Style::new().fg(orange_color).bold().paint("Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<p>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<w>"),
        Style::new().paint(" and move them with keyboard or mouse"),
    ])
}

pub fn floating_panes_mouse_medium(palette: Palette) -> LinePart {
    // Tip: Toggle floating panes with Ctrl + <p> + <w>
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);
    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("Toggle floating panes with "),
        Style::new().fg(orange_color).bold().paint("Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<p>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<w>"),
    ])
}

pub fn floating_panes_mouse_short(palette: Palette) -> LinePart {
    // Ctrl + <p> + <w> => floating panes
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().fg(orange_color).bold().paint(" Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<p>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<w>"),
        Style::new().paint(" => floating panes"),
    ])
}

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

pub fn compact_layout_full(palette: Palette) -> LinePart {
    // Tip: UI taking up too much space? Start Zellij with
    // zellij -l compact or remove pane frames with Ctrl + <p> + <z>
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("UI taking up too much space? Start Zellij with "),
        Style::new().fg(green_color).bold().paint("zellij -l compact"),
        Style::new().paint(" or remove pane frames with "),
        Style::new().fg(orange_color).bold().paint("Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<p>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<z>"),
    ])
}

pub fn compact_layout_medium(palette: Palette) -> LinePart {
    // Tip: To save screen space, start Zellij with
    // zellij -l compact or remove pane frames with Ctrl + <p> + <z>
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("To save screen space, start Zellij with "),
        Style::new().fg(green_color).bold().paint("zellij -l compact"),
        Style::new().paint(" or remove frames with "),
        Style::new().fg(orange_color).bold().paint("Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<p>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<z>"),
    ])
}

pub fn compact_layout_short(palette: Palette) -> LinePart {
    // Save screen space, start Zellij with
    // zellij -l compact or remove pane frames with Ctrl + <p> + <z>
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Save screen space, start with: "),
        Style::new().fg(green_color).bold().paint("zellij -l compact"),
        Style::new().paint(" or remove frames with "),
        Style::new().fg(orange_color).bold().paint("Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<p>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<z>"),
    ])
}

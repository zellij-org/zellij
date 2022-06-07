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

pub fn sync_tab_full(palette: Palette) -> LinePart {
    // Tip: Sync a tab and write keyboard input to all panes with Ctrl + <t> + <s>
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("Sync a tab and write keyboard input to all its panes with "),
        Style::new().fg(orange_color).bold().paint("Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<t>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<s>"),
    ])
}

pub fn sync_tab_medium(palette: Palette) -> LinePart {
    // Tip: Sync input to panes in a tab with Ctrl + <t> + <s>
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("Sync input to panes in a tab with "),
        Style::new().fg(orange_color).bold().paint("Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<t>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<s>"),
    ])
}

pub fn sync_tab_short(palette: Palette) -> LinePart {
    // Sync input in a tab with Ctrl + <t> + <s>
    let green_color = palette_match!(palette.green);
    let orange_color = palette_match!(palette.orange);

    strings!(&[
        Style::new().paint(" Sync input in a tab with "),
        Style::new().fg(orange_color).bold().paint("Ctrl"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<t>"),
        Style::new().paint(" + "),
        Style::new().fg(green_color).bold().paint("<s>"),
    ])
}

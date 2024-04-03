use ansi_term::{
    unstyled_len, ANSIString, ANSIStrings,
    Color::{Fixed, RGB},
    Style,
};

use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;

use crate::LinePart;

macro_rules! strings {
    ($ANSIStrings:expr) => {{
        let strings: &[ANSIString] = $ANSIStrings;

        let ansi_strings = ANSIStrings(strings);

        LinePart {
            part: format!("{}", ansi_strings),
            len: unstyled_len(&ansi_strings),
        }
    }};
}

pub fn move_tabs_full(help: &ModeInfo) -> LinePart {
    // Tip: Wrong order of tabs? You can move them to left and right with:
    // Alt + i (left) and Alt + o (right)
    let green_color = palette_match!(help.style.styling.text_unselected[3]);

    let bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("Wrong order of tabs? You can move them to left and right with: "),
        Style::new().fg(green_color).bold().paint("Alt + i"),
        Style::new().paint(" (left) and "),
        Style::new().fg(green_color).bold().paint("Alt + o"),
        Style::new().paint(" (right)"),
    ];
    strings!(&bits)
}

pub fn move_tabs_medium(help: &ModeInfo) -> LinePart {
    // Tip: You can move tabs to left and right with:
    // Alt + i (left) and Alt + o (right)
    let green_color = palette_match!(help.style.styling.text_unselected[3]);

    let bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("You can move tabs to left and right with: "),
        Style::new().fg(green_color).bold().paint("Alt + i"),
        Style::new().paint(" (left) and "),
        Style::new().fg(green_color).bold().paint("Alt + o"),
        Style::new().paint(" (right)"),
    ];
    strings!(&bits)
}

pub fn move_tabs_short(help: &ModeInfo) -> LinePart {
    // Move tabs with: Alt + i (left) and Alt + o (right)
    let green_color = palette_match!(help.style.styling.text_unselected[3]);

    let bits = vec![
        Style::new().paint(" Move tabs with: "),
        Style::new().fg(green_color).bold().paint("Alt + i"),
        Style::new().paint(" (left) and "),
        Style::new().fg(green_color).bold().paint("Alt + o"),
        Style::new().paint(" (right)"),
    ];
    strings!(&bits)
}

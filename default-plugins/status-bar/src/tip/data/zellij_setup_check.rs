use ansi_term::Color::{Fixed, RGB};
use ansi_term::Style;

use crate::{ansi_strings, LinePart};
use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;

pub fn zellij_setup_check_full(help: &ModeInfo) -> LinePart {
    // Tip: Having issues with Zellij? Try running "zellij setup --check"
    let orange_color = palette_match!(help.style.colors.orange);

    ansi_strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("Having issues with Zellij? Try running "),
        Style::new()
            .fg(orange_color)
            .bold()
            .paint("zellij setup --check"),
    ])
}

pub fn zellij_setup_check_medium(help: &ModeInfo) -> LinePart {
    // Tip: Run "zellij setup --check" to find issues
    let orange_color = palette_match!(help.style.colors.orange);

    ansi_strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().paint("Run "),
        Style::new()
            .fg(orange_color)
            .bold()
            .paint("zellij setup --check"),
        Style::new().paint(" to find issues"),
    ])
}

pub fn zellij_setup_check_short(help: &ModeInfo) -> LinePart {
    // Run "zellij setup --check" to find issues
    let orange_color = palette_match!(help.style.colors.orange);

    ansi_strings!(&[
        Style::new().paint(" Run "),
        Style::new()
            .fg(orange_color)
            .bold()
            .paint("zellij setup --check"),
        Style::new().paint(" to find issues"),
    ])
}

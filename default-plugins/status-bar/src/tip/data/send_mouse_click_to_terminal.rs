use ansi_term::Color::{Fixed, RGB};
use ansi_term::Style;

use crate::{ansi_strings, LinePart};
use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;

pub fn mouse_click_to_terminal_full(help: &ModeInfo) -> LinePart {
    // Tip: SHIFT + <mouse-click> bypasses Zellij and sends the mouse click directly to the terminal
    let green_color = palette_match!(help.style.colors.green);
    let orange_color = palette_match!(help.style.colors.orange);

    ansi_strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().fg(orange_color).bold().paint("Shift"),
        Style::new().paint(" + <"),
        Style::new().fg(green_color).bold().paint("mouse-click"),
        Style::new().paint("> bypasses Zellij and sends the mouse click directly to the terminal."),
    ])
}

pub fn mouse_click_to_terminal_medium(help: &ModeInfo) -> LinePart {
    // Tip: SHIFT + <mouse-click> sends the click directly to the terminal
    let green_color = palette_match!(help.style.colors.green);
    let orange_color = palette_match!(help.style.colors.orange);
    ansi_strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().fg(orange_color).bold().paint("Shift"),
        Style::new().paint(" + <"),
        Style::new().fg(green_color).bold().paint("mouse-click"),
        Style::new().paint("> sends the click directly to the terminal."),
    ])
}

pub fn mouse_click_to_terminal_short(help: &ModeInfo) -> LinePart {
    // Tip: SHIFT + <mouse-click>  => sends click to terminal.
    let green_color = palette_match!(help.style.colors.green);
    let orange_color = palette_match!(help.style.colors.orange);

    ansi_strings!(&[
        Style::new().paint(" Tip: "),
        Style::new().fg(orange_color).bold().paint("Shift"),
        Style::new().paint(" + <"),
        Style::new().fg(green_color).bold().paint("mouse-click"),
        Style::new().paint("> => sends click to terminal."),
    ])
}

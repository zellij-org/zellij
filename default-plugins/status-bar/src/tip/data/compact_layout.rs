use ansi_term::Color::{Fixed, RGB};
use ansi_term::{ANSIString, Style};

use crate::{action_key, style_key_with_modifier};
use crate::{ansi_strings, LinePart};
use zellij_tile::prelude::{actions::Action, *};
use zellij_tile_utils::palette_match;

pub fn compact_layout_full(help: &ModeInfo) -> LinePart {
    // Tip: UI taking up too much space? Start Zellij with
    // zellij -l compact or remove pane frames with Ctrl + <p> + <z>
    let green_color = palette_match!(help.style.colors.green);

    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("UI taking up too much space? Start Zellij with "),
        Style::new()
            .fg(green_color)
            .bold()
            .paint("zellij -l compact"),
        Style::new().paint(" or remove pane frames with "),
    ];
    bits.extend(add_keybinds(help));
    ansi_strings!(&bits)
}

pub fn compact_layout_medium(help: &ModeInfo) -> LinePart {
    // Tip: To save screen space, start Zellij with
    // zellij -l compact or remove pane frames with Ctrl + <p> + <z>
    let green_color = palette_match!(help.style.colors.green);

    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("To save screen space, start Zellij with "),
        Style::new()
            .fg(green_color)
            .bold()
            .paint("zellij -l compact"),
        Style::new().paint(" or remove frames with "),
    ];
    bits.extend(add_keybinds(help));
    ansi_strings!(&bits)
}

pub fn compact_layout_short(help: &ModeInfo) -> LinePart {
    // Save screen space, start Zellij with
    // zellij -l compact or remove pane frames with Ctrl + <p> + <z>
    let green_color = palette_match!(help.style.colors.green);

    let mut bits = vec![
        Style::new().paint(" Save screen space, start with: "),
        Style::new()
            .fg(green_color)
            .bold()
            .paint("zellij -l compact"),
        Style::new().paint(" or remove frames with "),
    ];
    bits.extend(add_keybinds(help));
    ansi_strings!(&bits)
}

fn add_keybinds(help: &ModeInfo) -> Vec<ANSIString> {
    let to_pane = action_key(
        &help.get_mode_keybinds(),
        &[Action::SwitchToMode(InputMode::Pane)],
    );
    let pane_frames = action_key(
        &help.get_keybinds_for_mode(InputMode::Pane),
        &[
            Action::TogglePaneFrames,
            Action::SwitchToMode(InputMode::Normal),
        ],
    );

    if pane_frames.is_empty() {
        return vec![Style::new().bold().paint("UNBOUND")];
    }

    let mut bits = vec![];
    bits.extend(style_key_with_modifier(&to_pane, &help.style.colors, None));
    bits.push(Style::new().paint(", "));
    bits.extend(style_key_with_modifier(
        &pane_frames,
        &help.style.colors,
        None,
    ));
    bits
}

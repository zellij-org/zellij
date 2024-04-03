use ansi_term::{
    unstyled_len, ANSIString, ANSIStrings,
    Color::{Fixed, RGB},
    Style,
};

use crate::{action_key, style_key_with_modifier, LinePart};
use zellij_tile::prelude::{actions::Action, *};
use zellij_tile_utils::palette_match;

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

pub fn edit_scrollbuffer_full(help: &ModeInfo) -> LinePart {
    // Tip: Search through the scrollbuffer using your default $EDITOR with
    // Ctrl + <s> + <e>
    let green_color = palette_match!(help.style.colors.text_unselected[1]);

    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("Search through the scrollbuffer using your default "),
        Style::new().fg(green_color).bold().paint("$EDITOR"),
        Style::new().paint(" with "),
    ];
    bits.extend(add_keybinds(help));
    strings!(&bits)
}

pub fn edit_scrollbuffer_medium(help: &ModeInfo) -> LinePart {
    // Tip: Search the scrollbuffer using your $EDITOR with
    // Ctrl + <s> + <e>
    let green_color = palette_match!(help.style.colors.text_unselected[1]);

    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("Search the scrollbuffer using your "),
        Style::new().fg(green_color).bold().paint("$EDITOR"),
        Style::new().paint(" with "),
    ];
    bits.extend(add_keybinds(help));
    strings!(&bits)
}

pub fn edit_scrollbuffer_short(help: &ModeInfo) -> LinePart {
    // Search using $EDITOR with
    // Ctrl + <s> + <e>
    let green_color = palette_match!(help.style.colors.text_unselected[1]);

    let mut bits = vec![
        Style::new().paint(" Search using "),
        Style::new().fg(green_color).bold().paint("$EDITOR"),
        Style::new().paint(" with "),
    ];
    bits.extend(add_keybinds(help));
    strings!(&bits)
}

fn add_keybinds(help: &ModeInfo) -> Vec<ANSIString> {
    let to_pane = action_key(
        &help.get_mode_keybinds(),
        &[Action::SwitchToMode(InputMode::Scroll)],
    );
    let edit_buffer = action_key(
        &help.get_keybinds_for_mode(InputMode::Scroll),
        &[
            Action::EditScrollback,
            Action::SwitchToMode(InputMode::Normal),
        ],
    );

    if edit_buffer.is_empty() {
        return vec![Style::new().bold().paint("UNBOUND")];
    }

    let mut bits = vec![];
    bits.extend(style_key_with_modifier(&to_pane, &help.style.colors, None));
    bits.push(Style::new().paint(", "));
    bits.extend(style_key_with_modifier(
        &edit_buffer,
        &help.style.colors,
        None,
    ));
    bits
}

use ansi_term::{unstyled_len, ANSIString, ANSIStrings, Style};

use crate::{action_key, action_key_group, style_key_with_modifier, LinePart};
use zellij_tile::prelude::{
    actions::{Action, Direction},
    *,
};

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

pub fn move_focus_hjkl_tab_switch_full(help: &ModeInfo) -> LinePart {
    // Tip: When changing focus with Alt + <←↓↑→> moving off screen left/right focuses the next tab.
    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("When changing focus with "),
    ];
    bits.extend(add_keybinds(help));
    bits.push(Style::new().paint(" moving off screen left/right focuses the next tab."));
    strings!(&bits)
}

pub fn move_focus_hjkl_tab_switch_medium(help: &ModeInfo) -> LinePart {
    // Tip: Changing focus with Alt + <←↓↑→> off screen focuses the next tab.
    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("Changing focus with "),
    ];
    bits.extend(add_keybinds(help));
    bits.push(Style::new().paint(" off screen focuses the next tab."));
    strings!(&bits)
}

pub fn move_focus_hjkl_tab_switch_short(help: &ModeInfo) -> LinePart {
    // Alt + <←↓↑→> off screen edge focuses next tab.
    let mut bits = add_keybinds(help);
    bits.push(Style::new().paint(" off screen edge focuses next tab."));
    strings!(&bits)
}

fn add_keybinds(help: &ModeInfo) -> Vec<ANSIString> {
    let to_pane = action_key(
        &help.get_mode_keybinds(),
        &[Action::SwitchToMode(InputMode::Pane)],
    );
    let pane_frames = action_key_group(
        &help.get_keybinds_for_mode(InputMode::Normal),
        &[
            &[Action::MoveFocus(Direction::Left)],
            &[Action::MoveFocus(Direction::Down)],
        ],
    );

    let mut bits = vec![];
    bits.extend(style_key_with_modifier(&to_pane, &help.style.colors));
    bits.push(Style::new().paint(", "));
    bits.extend(style_key_with_modifier(&pane_frames, &help.style.colors));
    bits
}

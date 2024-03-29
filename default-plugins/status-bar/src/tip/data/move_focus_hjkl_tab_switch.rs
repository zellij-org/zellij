use ansi_term::{unstyled_len, ANSIString, ANSIStrings, Style};

use crate::{action_key_group, style_key_with_modifier, LinePart};
use zellij_tile::prelude::{actions::Action, *};

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
    let pane_keymap = help.get_keybinds_for_mode(InputMode::Pane);
    let move_focus_keys = action_key_group(
        &pane_keymap,
        &[
            &[Action::MoveFocusOrTab(Direction::Left)],
            &[Action::MoveFocusOrTab(Direction::Right)],
        ],
    );

    // Let's see if we have some pretty groups in common here
    let mut arrows = vec![];
    let mut letters = vec![];
    for key in move_focus_keys.into_iter() {
        let key_str = key.to_string();
        if key_str.contains('←')
            || key_str.contains('↓')
            || key_str.contains('↑')
            || key_str.contains('→')
        {
            arrows.push(key);
        } else {
            letters.push(key);
        }
    }
    let arrows = style_key_with_modifier(&arrows, &help.style.styling, None);
    let letters = style_key_with_modifier(&letters, &help.style.styling, None);
    if arrows.is_empty() && letters.is_empty() {
        vec![Style::new().bold().paint("UNBOUND")]
    } else if arrows.is_empty() || letters.is_empty() {
        arrows.into_iter().chain(letters.into_iter()).collect()
    } else {
        arrows
            .into_iter()
            .chain(vec![Style::new().paint(" or ")].into_iter())
            .chain(letters.into_iter())
            .collect()
    }
}

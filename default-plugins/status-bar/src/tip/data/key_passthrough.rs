use ansi_term::{unstyled_len, ANSIString, ANSIStrings, Style};

use crate::LinePart;
use crate::{action_key, style_key_with_modifier};
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

pub fn key_passthrough_full(help: &ModeInfo) -> LinePart {
    // Tip: Do your zellij keybindings collide with applications?
    // Press Ctrl + <z> to pass keys through directly
    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("Do your zellij keybindings collide with applications? Press "),
    ];
    bits.extend(add_keybinds(help));
    bits.push(Style::new().paint(" to pass keys through directly"));
    strings!(&bits)
}

pub fn key_passthrough_medium(help: &ModeInfo) -> LinePart {
    // Tip: To send keys to applications without zellij interfering, prefix them with Ctrl + <z>
    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new()
            .paint("To send keys to applications without zellij interfering, prefix them with "),
    ];
    bits.extend(add_keybinds(help));
    strings!(&bits)
}

pub fn key_passthrough_short(help: &ModeInfo) -> LinePart {
    // Send keys to applications directly: Prefix with Ctrl + <z>
    let mut bits = vec![Style::new().paint("Send keys to applications directly: Prefix with ")];
    bits.extend(add_keybinds(help));
    strings!(&bits)
}

fn add_keybinds(help: &ModeInfo) -> Vec<ANSIString> {
    let passthrough = action_key(
        &help.get_keybinds_for_mode(InputMode::Normal),
        &[Action::SwitchToMode(InputMode::Passthrough)],
    );

    if passthrough.is_empty() {
        return vec![Style::new().bold().paint("UNBOUND")];
    }

    style_key_with_modifier(&passthrough, &help.style.colors)
}

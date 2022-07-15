use ansi_term::{
    unstyled_len, ANSIString, ANSIStrings,
    Style,
};

use crate::{LinePart, action_key, style_key_with_modifier};
use zellij_tile::prelude::{*, actions::Action};

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

pub fn floating_panes_mouse_full(help: &ModeInfo) -> LinePart {
    // Tip: Toggle floating panes with Ctrl + <p> + <w> and move them with keyboard or mouse
    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("Toggle floating panes with "),
    ];
    bits.extend(add_keybinds(help));
    bits.push(Style::new().paint(" and move them with keyboard or mouse"));
    strings!(&bits)
}

pub fn floating_panes_mouse_medium(help: &ModeInfo) -> LinePart {
    // Tip: Toggle floating panes with Ctrl + <p> + <w>
    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("Toggle floating panes with "),
    ];
    bits.extend(add_keybinds(help));
    strings!(&bits)
}

pub fn floating_panes_mouse_short(help: &ModeInfo) -> LinePart {
    // Ctrl + <p> + <w> => floating panes
    let mut bits = add_keybinds(help);
    bits.push(Style::new().paint(" => floating panes"));
    strings!(&bits)
}

fn add_keybinds(help: &ModeInfo) -> Vec<ANSIString> {
    let to_pane = action_key(
        &help.get_mode_keybinds(),
        &[Action::SwitchToMode(InputMode::Pane)],
    );
    let pane_frames = action_key(
        &help.get_keybinds_for_mode(InputMode::Pane),
        &[Action::ToggleFloatingPanes, Action::SwitchToMode(InputMode::Normal)],
    );

    let mut bits = vec![];
    bits.extend(style_key_with_modifier(&to_pane, &help.style.colors));
    bits.push(Style::new().paint(", "));
    bits.extend(style_key_with_modifier(&pane_frames, &help.style.colors));

    if bits.len() < 2 {
        // No keybindings available
        bits = vec![Style::new().bold().paint("UNBOUND")];
    }
    bits
}

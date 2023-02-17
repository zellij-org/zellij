use ansi_term::{unstyled_len, ANSIString, ANSIStrings, Style};

use crate::{action_key, style_key_with_modifier, LinePart};
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

pub fn sync_tab_full(help: &ModeInfo) -> LinePart {
    // Tip: Sync a tab and write keyboard input to all panes with Ctrl + <t> + <s>
    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("Sync a tab and write keyboard input to all its panes with "),
    ];
    bits.extend(add_keybinds(help));
    strings!(&bits)
}

pub fn sync_tab_medium(help: &ModeInfo) -> LinePart {
    // Tip: Sync input to panes in a tab with Ctrl + <t> + <s>
    let mut bits = vec![
        Style::new().paint(" Tip: "),
        Style::new().paint("Sync input to panes in a tab with "),
    ];
    bits.extend(add_keybinds(help));
    strings!(&bits)
}

pub fn sync_tab_short(help: &ModeInfo) -> LinePart {
    // Sync input in a tab with Ctrl + <t> + <s>
    let mut bits = vec![Style::new().paint(" Sync input in a tab with ")];
    bits.extend(add_keybinds(help));
    strings!(&bits)
}

fn add_keybinds(help: &ModeInfo) -> Vec<ANSIString> {
    let to_tab = action_key(
        &help.get_mode_keybinds(),
        &[Action::SwitchToMode(InputMode::Tab)],
    );
    let sync_tabs = action_key(
        &help.get_keybinds_for_mode(InputMode::Tab),
        &[
            Action::ToggleActiveSyncTab,
            Action::SwitchToMode(InputMode::Normal),
        ],
    );

    if sync_tabs.is_empty() {
        return vec![Style::new().bold().paint("UNBOUND")];
    }

    let mut bits = vec![];
    bits.extend(style_key_with_modifier(&to_tab, &help.style.colors, None));
    bits.push(Style::new().paint(", "));
    bits.extend(style_key_with_modifier(
        &sync_tabs,
        &help.style.colors,
        None,
    ));
    bits
}

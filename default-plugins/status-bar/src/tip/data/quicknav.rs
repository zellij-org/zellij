use ansi_term::{unstyled_len, ANSIString, ANSIStrings, Style};

use crate::{action_key, action_key_group, style_key_with_modifier, LinePart};
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

pub fn quicknav_full(help: &ModeInfo) -> LinePart {
    let groups = add_keybinds(help);

    let mut bits = vec![Style::new().paint(" Tip: ")];
    bits.extend(groups.new_pane);
    bits.push(Style::new().paint(" => open new pane. "));
    bits.extend(groups.move_focus);
    bits.push(Style::new().paint(" => navigate between panes. "));
    bits.extend(groups.resize);
    bits.push(Style::new().paint(" => increase/decrease pane size."));
    strings!(&bits)
}

pub fn quicknav_medium(help: &ModeInfo) -> LinePart {
    let groups = add_keybinds(help);

    let mut bits = vec![Style::new().paint(" Tip: ")];
    bits.extend(groups.new_pane);
    bits.push(Style::new().paint(" => new pane. "));
    bits.extend(groups.move_focus);
    bits.push(Style::new().paint(" => navigate. "));
    bits.extend(groups.resize);
    bits.push(Style::new().paint(" => resize pane."));
    strings!(&bits)
}

pub fn quicknav_short(help: &ModeInfo) -> LinePart {
    let groups = add_keybinds(help);

    let mut bits = vec![Style::new().paint(" QuickNav: ")];
    bits.extend(groups.new_pane);
    bits.push(Style::new().paint(" / "));
    bits.extend(groups.move_focus);
    bits.push(Style::new().paint(" / "));
    bits.extend(groups.resize);
    strings!(&bits)
}

struct Keygroups<'a> {
    new_pane: Vec<ANSIString<'a>>,
    move_focus: Vec<ANSIString<'a>>,
    resize: Vec<ANSIString<'a>>,
}

fn add_keybinds(help: &ModeInfo) -> Keygroups {
    let normal_keymap = help.get_mode_keybinds();
    let new_pane_keys = action_key(&normal_keymap, &[Action::NewPane(None, None)]);
    let new_pane = if new_pane_keys.is_empty() {
        vec![Style::new().bold().paint("UNBOUND")]
    } else {
        style_key_with_modifier(&new_pane_keys, &help.style.colors, None)
    };

    let mut resize_keys = action_key_group(
        &normal_keymap,
        &[
            &[Action::Resize(Resize::Increase, None)],
            &[Action::Resize(Resize::Decrease, None)],
        ],
    );
    if resize_keys.contains(&KeyWithModifier::new(BareKey::Char('=')).with_alt_modifier())
        && resize_keys.contains(&KeyWithModifier::new(BareKey::Char('+')).with_alt_modifier())
    {
        resize_keys.retain(|k| k != &KeyWithModifier::new(BareKey::Char('=')).with_alt_modifier())
    }
    let resize = if resize_keys.is_empty() {
        vec![Style::new().bold().paint("UNBOUND")]
    } else {
        style_key_with_modifier(&resize_keys, &help.style.colors, None)
    };

    let move_focus_keys = action_key_group(
        &normal_keymap,
        &[
            &[Action::MoveFocus(Direction::Left)],
            &[Action::MoveFocusOrTab(Direction::Left)],
            &[Action::MoveFocus(Direction::Down)],
            &[Action::MoveFocus(Direction::Up)],
            &[Action::MoveFocus(Direction::Right)],
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
    let arrows = style_key_with_modifier(&arrows, &help.style.colors, None);
    let letters = style_key_with_modifier(&letters, &help.style.colors, None);
    let move_focus = if arrows.is_empty() && letters.is_empty() {
        vec![Style::new().bold().paint("UNBOUND")]
    } else if arrows.is_empty() || letters.is_empty() {
        arrows.into_iter().chain(letters.into_iter()).collect()
    } else {
        arrows
            .into_iter()
            .chain(vec![Style::new().paint(" or ")].into_iter())
            .chain(letters.into_iter())
            .collect()
    };

    Keygroups {
        new_pane,
        move_focus,
        resize,
    }
}

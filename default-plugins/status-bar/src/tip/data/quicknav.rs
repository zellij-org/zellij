use ansi_term::{unstyled_len, ANSIString, ANSIStrings, Style};

use crate::{action_key, action_key_group, style_key_with_modifier, LinePart};
use zellij_tile::prelude::{
    actions::{Action, Direction, ResizeDirection},
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
    bits.push(Style::new().paint(" / "));
    strings!(&bits)
}

struct Keygroups<'a> {
    new_pane: Vec<ANSIString<'a>>,
    move_focus: Vec<ANSIString<'a>>,
    resize: Vec<ANSIString<'a>>,
}

fn add_keybinds(help: &ModeInfo) -> Keygroups {
    let normal_keymap = help.get_keybinds_for_mode(InputMode::Normal);
    let new_pane = action_key(&normal_keymap, &[Action::NewPane(None)]);
    let move_focus = action_key_group(
        &normal_keymap,
        &[
            &[Action::MoveFocus(Direction::Left)],
            &[Action::MoveFocus(Direction::Down)],
            &[Action::MoveFocus(Direction::Up)],
            &[Action::MoveFocus(Direction::Right)],
        ],
    );
    let resize = action_key_group(
        &normal_keymap,
        &[
            &[Action::Resize(ResizeDirection::Increase)],
            &[Action::Resize(ResizeDirection::Decrease)],
        ],
    );

    Keygroups {
        new_pane: style_key_with_modifier(&new_pane, &help.style.colors),
        move_focus: style_key_with_modifier(&move_focus, &help.style.colors),
        resize: style_key_with_modifier(&resize, &help.style.colors),
    }
}

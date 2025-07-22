use ansi_term::{ANSIString, ANSIStrings};
use ansi_term::{
    Color::{Fixed, RGB},
    Style,
};
use std::collections::HashMap;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zellij_tile_utils::palette_match;

use crate::first_line::{to_char, KeyAction, KeyMode, KeyShortcut};
use crate::second_line::{system_clipboard_error, text_copied_hint};
use crate::{action_key, action_key_group, color_elements, MORE_MSG, TO_NORMAL};
use crate::{ColoredElements, LinePart};
use unicode_width::UnicodeWidthStr;

pub fn one_line_ui(
    help: &ModeInfo,
    tab_info: Option<&TabInfo>,
    mut max_len: usize,
    separator: &str,
    base_mode_is_locked: bool,
    text_copied_to_clipboard_destination: Option<CopyDestination>,
    clipboard_failure: bool,
) -> LinePart {
    if let Some(text_copied_to_clipboard_destination) = text_copied_to_clipboard_destination {
        return text_copied_hint(text_copied_to_clipboard_destination);
    }
    if clipboard_failure {
        return system_clipboard_error(&help.style.colors);
    }
    let mut line_part_to_render = LinePart::default();
    let mut append = |line_part: &LinePart, max_len: &mut usize| {
        line_part_to_render.append(line_part);
        *max_len = max_len.saturating_sub(line_part.len);
    };

    render_mode_key_indicators(help, max_len, separator, base_mode_is_locked)
        .map(|mode_key_indicators| append(&mode_key_indicators, &mut max_len))
        .and_then(|_| match help.mode {
            InputMode::Normal | InputMode::Locked => render_secondary_info(help, tab_info, max_len)
                .map(|secondary_info| append(&secondary_info, &mut max_len)),
            _ => add_keygroup_separator(help, max_len)
                .map(|key_group_separator| append(&key_group_separator, &mut max_len))
                .and_then(|_| keybinds(help, max_len))
                .map(|keybinds| append(&keybinds, &mut max_len)),
        });
    line_part_to_render
}

fn to_base_mode(base_mode: InputMode) -> Action {
    Action::SwitchToMode(base_mode)
}

fn base_mode_locked_mode_indicators(help: &ModeInfo) -> HashMap<InputMode, Vec<KeyShortcut>> {
    let locked_binds = &help.get_keybinds_for_mode(InputMode::Locked);
    let normal_binds = &help.get_keybinds_for_mode(InputMode::Normal);
    let pane_binds = &help.get_keybinds_for_mode(InputMode::Pane);
    let tab_binds = &help.get_keybinds_for_mode(InputMode::Tab);
    let resize_binds = &help.get_keybinds_for_mode(InputMode::Resize);
    let move_binds = &help.get_keybinds_for_mode(InputMode::Move);
    let scroll_binds = &help.get_keybinds_for_mode(InputMode::Scroll);
    let session_binds = &help.get_keybinds_for_mode(InputMode::Session);
    HashMap::from([
        (
            InputMode::Locked,
            vec![KeyShortcut::new(
                KeyMode::Unselected,
                KeyAction::Unlock,
                to_char(action_key(
                    locked_binds,
                    &[Action::SwitchToMode(InputMode::Normal)],
                )),
            )],
        ),
        (
            InputMode::Normal,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Locked)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Pane,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Pane)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Tab,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Tab)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Resize,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Resize)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Move,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Move)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Search,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Scroll)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Session,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Session)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Quit,
                    to_char(action_key(normal_binds, &[Action::Quit])),
                ),
            ],
        ),
        (
            InputMode::Pane,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(
                        pane_binds,
                        &[Action::SwitchToMode(InputMode::Locked)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Pane,
                    to_char(action_key(
                        pane_binds,
                        &[Action::SwitchToMode(InputMode::Normal)],
                    )),
                ),
            ],
        ),
        (
            InputMode::Tab,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(
                        tab_binds,
                        &[Action::SwitchToMode(InputMode::Locked)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Tab,
                    to_char(action_key(
                        tab_binds,
                        &[Action::SwitchToMode(InputMode::Normal)],
                    )),
                ),
            ],
        ),
        (
            InputMode::Resize,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(
                        resize_binds,
                        &[Action::SwitchToMode(InputMode::Locked)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Resize,
                    to_char(action_key(
                        resize_binds,
                        &[Action::SwitchToMode(InputMode::Normal)],
                    )),
                ),
            ],
        ),
        (
            InputMode::Move,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(
                        move_binds,
                        &[Action::SwitchToMode(InputMode::Locked)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Move,
                    to_char(action_key(
                        move_binds,
                        &[Action::SwitchToMode(InputMode::Normal)],
                    )),
                ),
            ],
        ),
        (
            InputMode::Scroll,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(
                        scroll_binds,
                        &[Action::SwitchToMode(InputMode::Locked)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Search,
                    to_char(action_key(
                        scroll_binds,
                        &[Action::SwitchToMode(InputMode::Normal)],
                    )),
                ),
            ],
        ),
        (
            InputMode::Session,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(
                        session_binds,
                        &[Action::SwitchToMode(InputMode::Locked)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Session,
                    to_char(action_key(
                        session_binds,
                        &[Action::SwitchToMode(InputMode::Normal)],
                    )),
                ),
            ],
        ),
    ])
}

fn base_mode_normal_mode_indicators(help: &ModeInfo) -> HashMap<InputMode, Vec<KeyShortcut>> {
    let locked_binds = &help.get_keybinds_for_mode(InputMode::Locked);
    let normal_binds = &help.get_keybinds_for_mode(InputMode::Normal);
    let pane_binds = &help.get_keybinds_for_mode(InputMode::Pane);
    let tab_binds = &help.get_keybinds_for_mode(InputMode::Tab);
    let resize_binds = &help.get_keybinds_for_mode(InputMode::Resize);
    let move_binds = &help.get_keybinds_for_mode(InputMode::Move);
    let scroll_binds = &help.get_keybinds_for_mode(InputMode::Scroll);
    let session_binds = &help.get_keybinds_for_mode(InputMode::Session);
    HashMap::from([
        (
            InputMode::Locked,
            vec![KeyShortcut::new(
                KeyMode::Selected,
                KeyAction::Lock,
                to_char(action_key(
                    locked_binds,
                    &[Action::SwitchToMode(InputMode::Normal)],
                )),
            )],
        ),
        (
            InputMode::Normal,
            vec![
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Lock,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Locked)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Pane,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Pane)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Tab,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Tab)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Resize,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Resize)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Move,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Move)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Search,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Scroll)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Session,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Session)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Quit,
                    to_char(action_key(normal_binds, &[Action::Quit])),
                ),
            ],
        ),
        (
            InputMode::Pane,
            vec![KeyShortcut::new(
                KeyMode::Selected,
                KeyAction::Pane,
                to_char(action_key(
                    pane_binds,
                    &[Action::SwitchToMode(InputMode::Normal)],
                )),
            )],
        ),
        (
            InputMode::Tab,
            vec![KeyShortcut::new(
                KeyMode::Selected,
                KeyAction::Tab,
                to_char(action_key(
                    tab_binds,
                    &[Action::SwitchToMode(InputMode::Normal)],
                )),
            )],
        ),
        (
            InputMode::Resize,
            vec![KeyShortcut::new(
                KeyMode::Selected,
                KeyAction::Resize,
                to_char(action_key(
                    resize_binds,
                    &[Action::SwitchToMode(InputMode::Normal)],
                )),
            )],
        ),
        (
            InputMode::Move,
            vec![KeyShortcut::new(
                KeyMode::Selected,
                KeyAction::Move,
                to_char(action_key(
                    move_binds,
                    &[Action::SwitchToMode(InputMode::Normal)],
                )),
            )],
        ),
        (
            InputMode::Scroll,
            vec![KeyShortcut::new(
                KeyMode::Selected,
                KeyAction::Search,
                to_char(action_key(
                    scroll_binds,
                    &[Action::SwitchToMode(InputMode::Normal)],
                )),
            )],
        ),
        (
            InputMode::Session,
            vec![KeyShortcut::new(
                KeyMode::Selected,
                KeyAction::Session,
                to_char(action_key(
                    session_binds,
                    &[Action::SwitchToMode(InputMode::Normal)],
                )),
            )],
        ),
    ])
}

fn render_mode_key_indicators(
    help: &ModeInfo,
    max_len: usize,
    separator: &str,
    base_mode_is_locked: bool,
) -> Option<LinePart> {
    let mut line_part_to_render = LinePart::default();
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let colored_elements = color_elements(help.style.colors, !supports_arrow_fonts);
    let default_keys = if base_mode_is_locked {
        base_mode_locked_mode_indicators(help)
    } else {
        base_mode_normal_mode_indicators(help)
    };
    match common_modifiers_in_all_modes(&default_keys) {
        Some(modifiers) => {
            if let Some(default_keys) = default_keys.get(&help.mode) {
                let keys_without_common_modifiers: Vec<KeyShortcut> = default_keys
                    .iter()
                    .map(|key_shortcut| {
                        let key = key_shortcut
                            .get_key()
                            .map(|k| k.strip_common_modifiers(&modifiers));
                        let mode = key_shortcut.get_mode();
                        let action = key_shortcut.get_action();
                        KeyShortcut::new(mode, action, key)
                    })
                    .collect();
                render_common_modifiers(
                    &colored_elements,
                    help,
                    &modifiers,
                    &mut line_part_to_render,
                    separator,
                );

                let full_shortcut_list =
                    full_inline_keys_modes_shortcut_list(&keys_without_common_modifiers, help);

                if line_part_to_render.len + full_shortcut_list.len <= max_len {
                    line_part_to_render.append(&full_shortcut_list);
                } else {
                    let shortened_shortcut_list = shortened_inline_keys_modes_shortcut_list(
                        &keys_without_common_modifiers,
                        help,
                    );
                    if line_part_to_render.len + shortened_shortcut_list.len <= max_len {
                        line_part_to_render.append(&shortened_shortcut_list);
                    }
                }
            }
        },
        None => {
            if let Some(default_keys) = default_keys.get(&help.mode) {
                let full_shortcut_list = full_modes_shortcut_list(&default_keys, help);
                if line_part_to_render.len + full_shortcut_list.len <= max_len {
                    line_part_to_render.append(&full_shortcut_list);
                } else {
                    let shortened_shortcut_list =
                        shortened_modes_shortcut_list(&default_keys, help);
                    if line_part_to_render.len + shortened_shortcut_list.len <= max_len {
                        line_part_to_render.append(&shortened_shortcut_list);
                    }
                }
            }
        },
    }
    if line_part_to_render.len <= max_len {
        Some(line_part_to_render)
    } else {
        None
    }
}

fn full_inline_keys_modes_shortcut_list(
    keys_without_common_modifiers: &Vec<KeyShortcut>,
    help: &ModeInfo,
) -> LinePart {
    let mut full_shortcut_list = LinePart::default();
    for key in keys_without_common_modifiers {
        let is_selected = key.is_selected();
        let shortcut = add_shortcut_with_inline_key(
            help,
            &key.full_text(),
            key.key
                .as_ref()
                .map(|k| vec![k.clone()])
                .unwrap_or_else(|| vec![]),
            is_selected,
        );
        full_shortcut_list.append(&shortcut);
    }
    full_shortcut_list
}

fn shortened_inline_keys_modes_shortcut_list(
    keys_without_common_modifiers: &Vec<KeyShortcut>,
    help: &ModeInfo,
) -> LinePart {
    let mut shortened_shortcut_list = LinePart::default();
    for key in keys_without_common_modifiers {
        let is_selected = key.is_selected();
        let shortcut = add_shortcut_with_key_only(
            help,
            key.key
                .as_ref()
                .map(|k| vec![k.clone()])
                .unwrap_or_else(|| vec![]),
            is_selected,
        );
        shortened_shortcut_list.append(&shortcut);
    }
    shortened_shortcut_list
}

fn full_modes_shortcut_list(default_keys: &Vec<KeyShortcut>, help: &ModeInfo) -> LinePart {
    let mut full_shortcut_list = LinePart::default();
    for key in default_keys {
        let is_selected = key.is_selected();
        full_shortcut_list.append(&add_shortcut(
            help,
            &key.full_text(),
            &key.key
                .as_ref()
                .map(|k| vec![k.clone()])
                .unwrap_or_else(|| vec![]),
            is_selected,
            Some(3),
        ));
    }
    full_shortcut_list
}

fn shortened_modes_shortcut_list(default_keys: &Vec<KeyShortcut>, help: &ModeInfo) -> LinePart {
    let mut shortened_shortcut_list = LinePart::default();
    for key in default_keys {
        let is_selected = key.is_selected();
        shortened_shortcut_list.append(&add_shortcut(
            help,
            &key.short_text(),
            &key.key
                .as_ref()
                .map(|k| vec![k.clone()])
                .unwrap_or_else(|| vec![]),
            is_selected,
            Some(3),
        ));
    }
    shortened_shortcut_list
}

fn common_modifiers_in_all_modes(
    key_shortcuts: &HashMap<InputMode, Vec<KeyShortcut>>,
) -> Option<Vec<KeyModifier>> {
    let Some(mut common_modifiers) = key_shortcuts.iter().next().and_then(|k| {
        k.1.iter()
            .next()
            .and_then(|k| k.get_key().map(|k| k.key_modifiers.clone()))
    }) else {
        return None;
    };
    for (_mode, key_shortcuts) in key_shortcuts {
        if key_shortcuts.is_empty() {
            return None;
        }
        let Some(mut common_modifiers_for_mode) = key_shortcuts
            .iter()
            .next()
            .unwrap()
            .get_key()
            .map(|k| k.key_modifiers.clone())
        else {
            return None;
        };
        for key in key_shortcuts {
            let Some(key) = key.get_key() else {
                return None;
            };
            common_modifiers_for_mode = common_modifiers_for_mode
                .intersection(&key.key_modifiers)
                .cloned()
                .collect();
        }
        common_modifiers = common_modifiers
            .intersection(&common_modifiers_for_mode)
            .cloned()
            .collect();
    }
    if common_modifiers.is_empty() {
        return None;
    }
    Some(common_modifiers.into_iter().collect())
}

fn render_common_modifiers(
    palette: &ColoredElements,
    mode_info: &ModeInfo,
    common_modifiers: &Vec<KeyModifier>,
    line_part_to_render: &mut LinePart,
    separator: &str,
) {
    let prefix_text = if mode_info.capabilities.arrow_fonts {
        // Add extra space in simplified ui
        format!(
            " {} + ",
            common_modifiers
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("-")
        )
    } else {
        format!(
            " {} +",
            common_modifiers
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("-")
        )
    };

    let suffix_separator = palette.superkey_suffix_separator.paint(separator);
    line_part_to_render.part = format!(
        "{}{}{}",
        line_part_to_render.part,
        serialize_text(&Text::new(&prefix_text).opaque()),
        suffix_separator
    );
    line_part_to_render.len += prefix_text.chars().count() + separator.chars().count();
}

fn render_secondary_info(
    help: &ModeInfo,
    tab_info: Option<&TabInfo>,
    max_len: usize,
) -> Option<LinePart> {
    let mut secondary_info = LinePart::default();
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let colored_elements = color_elements(help.style.colors, !supports_arrow_fonts);
    let secondary_keybinds = secondary_keybinds(&help, tab_info, max_len);
    secondary_info.append(&secondary_keybinds);
    let remaining_space = max_len.saturating_sub(secondary_info.len).saturating_sub(1); // 1 for the end padding of the line
    let mut padding = String::new();
    let mut padding_len = 0;
    for _ in 0..remaining_space {
        padding.push_str(&ANSIStrings(&[colored_elements.superkey_prefix.paint(" ")]).to_string());
        padding_len += 1;
    }
    secondary_info.part = format!("{}{}", padding, secondary_info.part);
    secondary_info.len += padding_len;
    if secondary_info.len <= max_len {
        Some(secondary_info)
    } else {
        None
    }
}

fn should_show_focus_and_resize_shortcuts(tab_info: Option<&TabInfo>) -> bool {
    let Some(tab_info) = tab_info else {
        return false;
    };
    let are_floating_panes_visible = tab_info.are_floating_panes_visible;
    if are_floating_panes_visible {
        tab_info.selectable_floating_panes_count > 1
    } else {
        tab_info.selectable_tiled_panes_count > 1
    }
}

fn secondary_keybinds(help: &ModeInfo, tab_info: Option<&TabInfo>, max_len: usize) -> LinePart {
    let mut secondary_info = LinePart::default();
    let binds = &help.get_mode_keybinds();
    let should_show_focus_and_resize_shortcuts = should_show_focus_and_resize_shortcuts(tab_info);
    // New Pane
    let new_pane_action_key = action_key(binds, &[Action::NewPane(None, None, false)]);
    let mut new_pane_key_to_display = new_pane_action_key
        .iter()
        .find(|k| k.is_key_with_alt_modifier(BareKey::Char('n')))
        .or_else(|| new_pane_action_key.iter().next());
    let new_pane_key_to_display =
        if let Some(new_pane_key_to_display) = new_pane_key_to_display.take() {
            vec![new_pane_key_to_display.clone()]
        } else {
            vec![]
        };

    // Resize
    let resize_increase_action_key = action_key(binds, &[Action::Resize(Resize::Increase, None)]);
    let resize_increase_key = resize_increase_action_key
        .iter()
        .find(|k| k.bare_key == BareKey::Char('+'))
        .or_else(|| resize_increase_action_key.iter().next());
    let resize_decrease_action_key = action_key(binds, &[Action::Resize(Resize::Decrease, None)]);
    let resize_decrease_key = resize_decrease_action_key
        .iter()
        .find(|k| k.bare_key == BareKey::Char('-'))
        .or_else(|| resize_increase_action_key.iter().next());
    let mut resize_shortcuts = vec![];
    if let Some(resize_increase_key) = resize_increase_key {
        resize_shortcuts.push(resize_increase_key.clone());
    }
    if let Some(resize_decrease_key) = resize_decrease_key {
        resize_shortcuts.push(resize_decrease_key.clone());
    }

    // Move focus
    let mut move_focus_shortcuts: Vec<KeyWithModifier> = vec![];

    // Left
    let move_focus_left_action_key = action_key(binds, &[Action::MoveFocusOrTab(Direction::Left)]);
    let move_focus_left_key = move_focus_left_action_key
        .iter()
        .find(|k| k.bare_key == BareKey::Left)
        .or_else(|| move_focus_left_action_key.iter().next());
    if let Some(move_focus_left_key) = move_focus_left_key {
        move_focus_shortcuts.push(move_focus_left_key.clone());
    }
    // Down
    let move_focus_left_action_key = action_key(binds, &[Action::MoveFocus(Direction::Down)]);
    let move_focus_left_key = move_focus_left_action_key
        .iter()
        .find(|k| k.bare_key == BareKey::Down)
        .or_else(|| move_focus_left_action_key.iter().next());
    if let Some(move_focus_left_key) = move_focus_left_key {
        move_focus_shortcuts.push(move_focus_left_key.clone());
    }
    // Up
    let move_focus_left_action_key = action_key(binds, &[Action::MoveFocus(Direction::Up)]);
    let move_focus_left_key = move_focus_left_action_key
        .iter()
        .find(|k| k.bare_key == BareKey::Up)
        .or_else(|| move_focus_left_action_key.iter().next());
    if let Some(move_focus_left_key) = move_focus_left_key {
        move_focus_shortcuts.push(move_focus_left_key.clone());
    }
    // Right
    let move_focus_left_action_key = action_key(binds, &[Action::MoveFocusOrTab(Direction::Right)]);
    let move_focus_left_key = move_focus_left_action_key
        .iter()
        .find(|k| k.bare_key == BareKey::Right)
        .or_else(|| move_focus_left_action_key.iter().next());
    if let Some(move_focus_left_key) = move_focus_left_key {
        move_focus_shortcuts.push(move_focus_left_key.clone());
    }

    let toggle_floating_action_key = action_key(binds, &[Action::ToggleFloatingPanes]);
    let mut toggle_floating_action_key = toggle_floating_action_key
        .iter()
        .find(|k| k.is_key_with_alt_modifier(BareKey::Char('f')))
        .or_else(|| toggle_floating_action_key.iter().next());
    let toggle_floating_key_to_display =
        if let Some(toggle_floating_key_to_display) = toggle_floating_action_key.take() {
            vec![toggle_floating_key_to_display.clone()]
        } else {
            vec![]
        };
    let are_floating_panes_visible = tab_info
        .map(|t| t.are_floating_panes_visible)
        .unwrap_or(false);

    let common_modifiers = get_common_modifiers(
        [
            new_pane_key_to_display.clone(),
            move_focus_shortcuts.clone(),
            resize_shortcuts.clone(),
            toggle_floating_key_to_display.clone(),
        ]
        .iter()
        .flatten()
        .collect(),
    );
    let no_common_modifier = common_modifiers.is_empty();

    if no_common_modifier {
        secondary_info.append(&add_shortcut(
            help,
            "New Pane",
            &new_pane_key_to_display,
            false,
            Some(0),
        ));
        if should_show_focus_and_resize_shortcuts {
            secondary_info.append(&add_shortcut(
                help,
                "Change Focus",
                &move_focus_shortcuts,
                false,
                Some(0),
            ));
            secondary_info.append(&add_shortcut(
                help,
                "Resize",
                &resize_shortcuts,
                false,
                Some(0),
            ));
        }
        secondary_info.append(&add_shortcut(
            help,
            "Floating",
            &toggle_floating_key_to_display,
            are_floating_panes_visible,
            Some(0),
        ));
    } else {
        let modifier_str = text_as_line_part_with_emphasis(
            format!(
                "{} + ",
                common_modifiers
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join("-")
            ),
            0,
        );
        secondary_info.append(&modifier_str);
        let new_pane_key_to_display: Vec<KeyWithModifier> = new_pane_key_to_display
            .iter()
            .map(|k| k.strip_common_modifiers(&common_modifiers))
            .collect();
        let move_focus_shortcuts: Vec<KeyWithModifier> = move_focus_shortcuts
            .iter()
            .map(|k| k.strip_common_modifiers(&common_modifiers))
            .collect();
        let resize_shortcuts: Vec<KeyWithModifier> = resize_shortcuts
            .iter()
            .map(|k| k.strip_common_modifiers(&common_modifiers))
            .collect();
        let toggle_floating_key_to_display: Vec<KeyWithModifier> = toggle_floating_key_to_display
            .iter()
            .map(|k| k.strip_common_modifiers(&common_modifiers))
            .collect();
        secondary_info.append(&add_shortcut_with_inline_key(
            help,
            "New Pane",
            new_pane_key_to_display,
            false,
        ));
        if should_show_focus_and_resize_shortcuts {
            secondary_info.append(&add_shortcut_with_inline_key(
                help,
                "Change Focus",
                move_focus_shortcuts,
                false,
            ));
            secondary_info.append(&add_shortcut_with_inline_key(
                help,
                "Resize",
                resize_shortcuts,
                false,
            ));
        }
        secondary_info.append(&add_shortcut_with_inline_key(
            help,
            "Floating",
            toggle_floating_key_to_display,
            are_floating_panes_visible,
        ));
    }

    if secondary_info.len <= max_len {
        secondary_info
    } else {
        let mut short_line = LinePart::default();
        if no_common_modifier {
            short_line.append(&add_shortcut(
                help,
                "New",
                &new_pane_key_to_display,
                false,
                Some(0),
            ));
            if should_show_focus_and_resize_shortcuts {
                short_line.append(&add_shortcut(
                    help,
                    "Focus",
                    &move_focus_shortcuts,
                    false,
                    Some(0),
                ));
                short_line.append(&add_shortcut(
                    help,
                    "Resize",
                    &resize_shortcuts,
                    false,
                    Some(0),
                ));
            }
            short_line.append(&add_shortcut(
                help,
                "Floating",
                &toggle_floating_key_to_display,
                are_floating_panes_visible,
                Some(0),
            ));
        } else {
            let modifier_str = text_as_line_part_with_emphasis(
                format!(
                    "{} + ",
                    common_modifiers
                        .iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<_>>()
                        .join("-")
                ),
                0,
            );
            short_line.append(&modifier_str);
            let new_pane_key_to_display: Vec<KeyWithModifier> = new_pane_key_to_display
                .iter()
                .map(|k| k.strip_common_modifiers(&common_modifiers))
                .collect();
            let move_focus_shortcuts: Vec<KeyWithModifier> = move_focus_shortcuts
                .iter()
                .map(|k| k.strip_common_modifiers(&common_modifiers))
                .collect();
            let resize_shortcuts: Vec<KeyWithModifier> = resize_shortcuts
                .iter()
                .map(|k| k.strip_common_modifiers(&common_modifiers))
                .collect();
            let toggle_floating_key_to_display: Vec<KeyWithModifier> =
                toggle_floating_key_to_display
                    .iter()
                    .map(|k| k.strip_common_modifiers(&common_modifiers))
                    .collect();
            short_line.append(&add_shortcut_with_inline_key(
                help,
                "New",
                new_pane_key_to_display,
                false,
            ));
            if should_show_focus_and_resize_shortcuts {
                short_line.append(&add_shortcut_with_inline_key(
                    help,
                    "Focus",
                    move_focus_shortcuts,
                    false,
                ));
                short_line.append(&add_shortcut_with_inline_key(
                    help,
                    "Resize",
                    resize_shortcuts,
                    false,
                ));
            }
            short_line.append(&add_shortcut_with_inline_key(
                help,
                "Floating",
                toggle_floating_key_to_display,
                are_floating_panes_visible,
            ));
        }
        if short_line.len <= max_len {
            short_line
        } else if max_len >= 3 {
            let part = serialize_text(
                &Text::new(format!("{:>width$}", "...", width = max_len))
                    .color_range(0, ..)
                    .opaque(),
            );
            let len = max_len;
            LinePart { part, len }
        } else {
            LinePart {
                part: "".to_owned(),
                len: 0,
            }
        }
    }
}

fn text_as_line_part_with_emphasis(text: String, emphases_index: usize) -> LinePart {
    let part = serialize_text(&Text::new(&text).color_range(emphases_index, ..).opaque());
    LinePart {
        part,
        len: text.width(),
    }
}

fn keybinds(help: &ModeInfo, max_width: usize) -> Option<LinePart> {
    let full_shortcut_list = full_shortcut_list(help);
    if full_shortcut_list.len <= max_width {
        return Some(full_shortcut_list);
    }
    let shortened_shortcut_list = shortened_shortcut_list(help);
    if shortened_shortcut_list.len <= max_width {
        return Some(shortened_shortcut_list);
    }
    Some(best_effort_shortcut_list(help, max_width))
}

fn add_shortcut(
    help: &ModeInfo,
    text: &str,
    keys: &Vec<KeyWithModifier>,
    selected: bool,
    key_color_index: Option<usize>,
) -> LinePart {
    let mut ret = LinePart::default();
    if keys.is_empty() {
        return ret;
    }

    ret.append(&style_key_with_modifier(&keys, key_color_index)); // TODO: alternate
                                                                  //
    let ribbon = if selected {
        serialize_ribbon(&Text::new(format!("{}", text)).selected())
    } else {
        serialize_ribbon(&Text::new(format!("{}", text)))
    };
    ret.part = format!("{}{}", ret.part, ribbon);
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    ret.len += if supports_arrow_fonts {
        text.width() + 4 // padding and arrow fonts
    } else {
        text.width() + 2 // padding
    };
    ret
}

fn add_shortcut_with_inline_key(
    help: &ModeInfo,
    text: &str,
    key: Vec<KeyWithModifier>,
    is_selected: bool,
) -> LinePart {
    let capabilities = help.capabilities;

    let mut ret = LinePart::default();
    if key.is_empty() {
        return ret;
    }

    let key_separator = match key
        .iter()
        .map(|k| k.to_string())
        .collect::<Vec<_>>()
        .join("")
        .as_str()
    {
        "HJKL" => "",
        "hjkl" => "",
        "←↓↑→" => "",
        "←→" => "",
        "↓↑" => "",
        "[]" => "",
        "+-" => "",
        _ => "|",
    };

    let key_string = format!(
        "{}",
        key.iter()
            .map(|k| k.to_string())
            .collect::<Vec<_>>()
            .join(key_separator)
    );

    let ribbon = if is_selected {
        serialize_ribbon(
            &Text::new(format!("<{}> {}", key_string, text))
                .color_range(0, 1..key_string.width() + 1)
                .selected(),
        )
    } else {
        serialize_ribbon(
            &Text::new(format!("<{}> {}", key_string, text))
                .color_range(0, 1..key_string.width() + 1),
        )
    };
    ret.part = ribbon;
    let supports_arrow_fonts = !capabilities.arrow_fonts;
    ret.len += if supports_arrow_fonts {
        text.width() + key_string.width() + 7 // padding, group boundaries and arrow fonts
    } else {
        text.width() + key_string.width() + 5 // padding and group boundaries
    };

    ret
}

fn add_shortcut_with_key_only(
    help: &ModeInfo,
    key: Vec<KeyWithModifier>,
    is_selected: bool,
) -> LinePart {
    let mut ret = LinePart::default();
    if key.is_empty() {
        return ret;
    }

    let key_string = format!(
        "{}",
        key.iter()
            .map(|k| k.to_string())
            .collect::<Vec<_>>()
            .join("-")
    );

    let ribbon = if is_selected {
        serialize_ribbon(
            &Text::new(format!("{}", key_string))
                .color_range(0, ..)
                .selected(),
        )
    } else {
        serialize_ribbon(&Text::new(format!("{}", key_string)).color_range(0, ..))
    };
    ret.part = ribbon;
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    ret.len += if supports_arrow_fonts {
        key_string.width() + 4 // 4 => arrow fonts + padding
    } else {
        key_string.width() + 2 // 2 => padding
    };
    ret
}

fn add_keygroup_separator(help: &ModeInfo, max_len: usize) -> Option<LinePart> {
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let separator = if supports_arrow_fonts {
        crate::ARROW_SEPARATOR
    } else {
        " "
    };
    let palette = help.style.colors;

    let mut ret = LinePart::default();

    let separator_color = palette_match!(palette.text_unselected.emphasis_0);
    let bg_color = palette_match!(palette.ribbon_selected.base);
    let mut bits: Vec<ANSIString> = vec![];
    let mode_help_text = match help.mode {
        InputMode::RenamePane => Some("RENAMING PANE"),
        InputMode::RenameTab => Some("RENAMING TAB"),
        InputMode::EnterSearch => Some("ENTERING SEARCH TERM"),
        InputMode::Search => Some("SEARCHING"),
        _ => None,
    };
    if let Some(mode_help_text) = mode_help_text {
        bits.push(
            Style::new()
                .fg(separator_color)
                .on(bg_color)
                .bold()
                .paint(format!(" {} ", mode_help_text)),
        );
        ret.len += mode_help_text.width() + 2; // 2 => padding
    }
    bits.push(
        Style::new()
            .fg(bg_color)
            .on(separator_color)
            .bold()
            .paint(format!("{}", separator)),
    );
    bits.push(
        Style::new()
            .fg(separator_color)
            .on(separator_color)
            .bold()
            .paint(format!(" ")),
    );
    bits.push(
        Style::new()
            .fg(separator_color)
            .on(bg_color)
            .bold()
            .paint(format!("{}", separator)),
    );
    ret.part = format!("{}{}", ret.part, ANSIStrings(&bits));
    ret.len += 3; // padding and arrow fonts

    if ret.len <= max_len {
        Some(ret)
    } else {
        None
    }
}

fn full_shortcut_list(help: &ModeInfo) -> LinePart {
    match help.mode {
        InputMode::Normal => LinePart::default(),
        InputMode::Locked => LinePart::default(),
        _ => full_shortcut_list_nonstandard_mode(help),
    }
}

fn full_shortcut_list_nonstandard_mode(help: &ModeInfo) -> LinePart {
    let mut line_part = LinePart::default();
    let keys_and_hints = get_keys_and_hints(help);

    for (long, _short, keys) in keys_and_hints.into_iter() {
        line_part.append(&add_shortcut(help, &long, &keys.to_vec(), false, Some(2)));
    }
    line_part
}

#[rustfmt::skip]
fn get_keys_and_hints(mi: &ModeInfo) -> Vec<(String, String, Vec<KeyWithModifier>)> {
    use Action as A;
    use InputMode as IM;
    use Direction as Dir;
    use actions::SearchDirection as SDir;
    use actions::SearchOption as SOpt;

    let mut old_keymap = mi.get_mode_keybinds();
    let s = |string: &str| string.to_string();

    // Find a keybinding to get back to "Normal" input mode. In this case we prefer '\n' over other
    // choices. Do it here before we dedupe the keymap below!
    let base_mode = mi.base_mode;
    let to_basemode_keys = base_mode.map(|b| action_key(&old_keymap, &[to_base_mode(b)])).unwrap_or_else(|| action_key(&old_keymap, &[TO_NORMAL]));
    let to_basemode_key = if to_basemode_keys.contains(&KeyWithModifier::new(BareKey::Enter)) {
        vec![KeyWithModifier::new(BareKey::Enter)]
    } else {
        // Yield `vec![key]` if `to_normal_keys` has at least one key, or an empty vec otherwise.
        to_basemode_keys.into_iter().take(1).collect()
    };

    // Sort and deduplicate the keybindings first. We sort after the `Key`s, and deduplicate by
    // their `Action` vectors. An unstable sort is fine here because if the user maps anything to
    // the same key again, anything will happen...
    old_keymap.sort_unstable_by(|(keya, _), (keyb, _)| keya.partial_cmp(keyb).unwrap());

    let mut known_actions: Vec<Vec<Action>> = vec![];
    let mut km = vec![];
    for (key, acvec) in old_keymap {
        if known_actions.contains(&acvec) {
            // This action is known already
            continue;
        } else {
            known_actions.push(acvec.to_vec());
            km.push((key, acvec));
        }
    }

    if mi.mode == IM::Pane { vec![
        (s("New"), s("New"), single_action_key(&km, &[A::NewPane(None, None, false), TO_NORMAL])),
        (s("Change Focus"), s("Move"),
            action_key_group(&km, &[&[A::MoveFocus(Dir::Left)], &[A::MoveFocus(Dir::Down)],
                &[A::MoveFocus(Dir::Up)], &[A::MoveFocus(Dir::Right)]])),
        (s("Close"), s("Close"), single_action_key(&km, &[A::CloseFocus, TO_NORMAL])),
        (s("Rename"), s("Rename"),
            single_action_key(&km, &[A::SwitchToMode(IM::RenamePane), A::PaneNameInput(vec![0])])),
        (s("Toggle Fullscreen"), s("Fullscreen"), single_action_key(&km, &[A::ToggleFocusFullscreen, TO_NORMAL])),
        (s("Toggle Floating"), s("Floating"),
            single_action_key(&km, &[A::ToggleFloatingPanes, TO_NORMAL])),
        (s("Toggle Embed"), s("Embed"), single_action_key(&km, &[A::TogglePaneEmbedOrFloating, TO_NORMAL])),
        (s("Split Right"), s("Right"), single_action_key(&km, &[A::NewPane(Some(Direction::Right), None, false), TO_NORMAL])),
        (s("Split Down"), s("Down"), single_action_key(&km, &[A::NewPane(Some(Direction::Down), None, false), TO_NORMAL])),
        (s("Stack"), s("Stack"), single_action_key(&km, &[A::NewStackedPane(None, None), TO_NORMAL])),
        (s("Select pane"), s("Select"), to_basemode_key),
    ]} else if mi.mode == IM::Tab {
        // With the default bindings, "Move focus" for tabs is tricky: It binds all the arrow keys
        // to moving tabs focus (left/up go left, right/down go right). Since we sort the keys
        // above and then dedpulicate based on the actions, we will end up with LeftArrow for
        // "left" and DownArrow for "right". What we really expect is to see LeftArrow and
        // RightArrow.
        // FIXME: So for lack of a better idea we just check this case manually here.
        let old_keymap = mi.get_mode_keybinds();
        let focus_keys_full: Vec<KeyWithModifier> = action_key_group(&old_keymap,
            &[&[A::GoToPreviousTab], &[A::GoToNextTab]]);
        let focus_keys = if focus_keys_full.contains(&KeyWithModifier::new(BareKey::Left))
            && focus_keys_full.contains(&KeyWithModifier::new(BareKey::Right)) {
            vec![KeyWithModifier::new(BareKey::Left), KeyWithModifier::new(BareKey::Right)]
        } else {
            action_key_group(&km, &[&[A::GoToPreviousTab], &[A::GoToNextTab]])
        };

        vec![
        (s("New"), s("New"), single_action_key(&km, &[A::NewTab(None, vec![], None, None, None, true, None), TO_NORMAL])),
        (s("Change focus"), s("Move"), focus_keys),
        (s("Close"), s("Close"), single_action_key(&km, &[A::CloseTab, TO_NORMAL])),
        (s("Rename"), s("Rename"),
            single_action_key(&km, &[A::SwitchToMode(IM::RenameTab), A::TabNameInput(vec![0])])),
        (s("Sync"), s("Sync"), single_action_key(&km, &[A::ToggleActiveSyncTab, TO_NORMAL])),
        (s("Break pane to new tab"), s("Break out"), single_action_key(&km, &[A::BreakPane, TO_NORMAL])),
        (s("Break pane left/right"), s("Break"), action_key_group(&km, &[
            &[Action::BreakPaneLeft, TO_NORMAL],
            &[Action::BreakPaneRight, TO_NORMAL],
        ])),
        (s("Toggle"), s("Toggle"), single_action_key(&km, &[A::ToggleTab])),
        (s("Select pane"), s("Select"), to_basemode_key),
    ]} else if mi.mode == IM::Resize { vec![
        (s("Increase/Decrease size"), s("Increase/Decrease"),
            action_key_group(&km, &[
                &[A::Resize(Resize::Increase, None)],
                &[A::Resize(Resize::Decrease, None)]
            ])),
        (s("Increase to"), s("Increase"), action_key_group(&km, &[
            &[A::Resize(Resize::Increase, Some(Dir::Left))],
            &[A::Resize(Resize::Increase, Some(Dir::Down))],
            &[A::Resize(Resize::Increase, Some(Dir::Up))],
            &[A::Resize(Resize::Increase, Some(Dir::Right))]
            ])),
        (s("Decrease from"), s("Decrease"), action_key_group(&km, &[
            &[A::Resize(Resize::Decrease, Some(Dir::Left))],
            &[A::Resize(Resize::Decrease, Some(Dir::Down))],
            &[A::Resize(Resize::Decrease, Some(Dir::Up))],
            &[A::Resize(Resize::Decrease, Some(Dir::Right))]
            ])),
        (s("Select pane"), s("Select"), to_basemode_key),
    ]} else if mi.mode == IM::Move { vec![
        (s("Switch Location"), s("Move"), action_key_group(&km, &[
            &[Action::MovePane(Some(Dir::Left))], &[Action::MovePane(Some(Dir::Down))],
            &[Action::MovePane(Some(Dir::Up))], &[Action::MovePane(Some(Dir::Right))]])),
        (s("When done"), s("Back"), to_basemode_key),
    ]} else if mi.mode == IM::Scroll { vec![
        (s("Enter search term"), s("Search"),
            action_key(&km, &[A::SwitchToMode(IM::EnterSearch), A::SearchInput(vec![0])])),
        (s("Scroll"), s("Scroll"),
            action_key_group(&km, &[&[Action::ScrollDown], &[Action::ScrollUp]])),
        (s("Scroll page"), s("Scroll"),
            action_key_group(&km, &[&[Action::PageScrollDown], &[Action::PageScrollUp]])),
        (s("Scroll half page"), s("Scroll"),
            action_key_group(&km, &[&[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]])),
        (s("Edit scrollback in default editor"), s("Edit"),
            single_action_key(&km, &[Action::EditScrollback, TO_NORMAL])),
        (s("Select pane"), s("Select"), to_basemode_key),
    ]} else if mi.mode == IM::EnterSearch { vec![
        (s("When done"), s("Done"), action_key(&km, &[A::SwitchToMode(IM::Search)])),
        (s("Cancel"), s("Cancel"),
            action_key(&km, &[A::SearchInput(vec![27]), A::SwitchToMode(IM::Scroll)])),
    ]} else if mi.mode == IM::Search { vec![
        (s("Enter Search term"), s("Search"),
            action_key(&km, &[A::SwitchToMode(IM::EnterSearch), A::SearchInput(vec![0])])),
        (s("Scroll"), s("Scroll"),
            action_key_group(&km, &[&[Action::ScrollDown], &[Action::ScrollUp]])),
        (s("Scroll page"), s("Scroll"),
            action_key_group(&km, &[&[Action::PageScrollDown], &[Action::PageScrollUp]])),
        (s("Scroll half page"), s("Scroll"),
            action_key_group(&km, &[&[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]])),
        (s("Search down"), s("Down"), action_key(&km, &[A::Search(SDir::Down)])),
        (s("Search up"), s("Up"), action_key(&km, &[A::Search(SDir::Up)])),
        (s("Case sensitive"), s("Case"),
            action_key(&km, &[A::SearchToggleOption(SOpt::CaseSensitivity)])),
        (s("Wrap"), s("Wrap"),
            action_key(&km, &[A::SearchToggleOption(SOpt::Wrap)])),
        (s("Whole words"), s("Whole"),
            action_key(&km, &[A::SearchToggleOption(SOpt::WholeWord)])),
    ]} else if mi.mode == IM::Session { vec![
        (s("Detach"), s("Detach"), action_key(&km, &[Action::Detach])),
        (s("Session Manager"), s("Manager"), session_manager_key(&km)),
        (s("Share"), s("Share"), share_key(&km)),
        (s("Configure"), s("Config"), configuration_key(&km)),
        (s("Plugin Manager"), s("Plugins"), plugin_manager_key(&km)),
        (s("About"), s("About"), about_key(&km)),
        (s("Select pane"), s("Select"), to_basemode_key),
    ]} else if mi.mode == IM::Tmux { vec![
        (s("Move focus"), s("Move"), action_key_group(&km, &[
            &[A::MoveFocus(Dir::Left)], &[A::MoveFocus(Dir::Down)],
            &[A::MoveFocus(Dir::Up)], &[A::MoveFocus(Dir::Right)]])),
        (s("Split down"), s("Down"), action_key(&km, &[A::NewPane(Some(Dir::Down), None, false), TO_NORMAL])),
        (s("Split right"), s("Right"), action_key(&km, &[A::NewPane(Some(Dir::Right), None, false), TO_NORMAL])),
        (s("Fullscreen"), s("Fullscreen"), action_key(&km, &[A::ToggleFocusFullscreen, TO_NORMAL])),
        (s("New tab"), s("New"), action_key(&km, &[A::NewTab(None, vec![], None, None, None, true, None), TO_NORMAL])),
        (s("Rename tab"), s("Rename"),
            action_key(&km, &[A::SwitchToMode(IM::RenameTab), A::TabNameInput(vec![0])])),
        (s("Previous Tab"), s("Previous"), action_key(&km, &[A::GoToPreviousTab, TO_NORMAL])),
        (s("Next Tab"), s("Next"), action_key(&km, &[A::GoToNextTab, TO_NORMAL])),
        (s("Select pane"), s("Select"), to_basemode_key),
    ]} else if matches!(mi.mode, IM::RenamePane | IM::RenameTab) { vec![
        (s("When done"), s("Done"), to_basemode_key),
    ]} else { vec![] }
}

fn shortened_shortcut_list_nonstandard_mode(help: &ModeInfo) -> LinePart {
    let mut line_part = LinePart::default();
    let keys_and_hints = get_keys_and_hints(help);

    for (_, short, keys) in keys_and_hints.into_iter() {
        line_part.append(&add_shortcut(help, &short, &keys.to_vec(), false, Some(2)));
    }
    line_part
}

fn shortened_shortcut_list(help: &ModeInfo) -> LinePart {
    match help.mode {
        InputMode::Normal => LinePart::default(),
        InputMode::Locked => LinePart::default(),
        _ => shortened_shortcut_list_nonstandard_mode(help),
    }
}

fn best_effort_shortcut_list(help: &ModeInfo, max_len: usize) -> LinePart {
    let mut line_part = LinePart::default();
    let keys_and_hints = get_keys_and_hints(help);
    for (_, short, keys) in keys_and_hints.into_iter() {
        let shortcut = add_shortcut(help, &short, &keys.to_vec(), false, Some(2));
        if line_part.len + shortcut.len + MORE_MSG.chars().count() > max_len {
            line_part.part = format!("{}{}", line_part.part, MORE_MSG);
            line_part.len += MORE_MSG.chars().count();
            break;
        } else {
            line_part.append(&shortcut);
        }
    }
    line_part
}

fn single_action_key(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    action: &[Action],
) -> Vec<KeyWithModifier> {
    let mut matching = keymap.iter().find_map(|(key, acvec)| {
        if acvec.iter().next() == action.iter().next() {
            Some(key.clone())
        } else {
            None
        }
    });
    if let Some(matching) = matching.take() {
        vec![matching]
    } else {
        vec![]
    }
}

fn session_manager_key(keymap: &[(KeyWithModifier, Vec<Action>)]) -> Vec<KeyWithModifier> {
    let mut matching = keymap.iter().find_map(|(key, acvec)| {
        let has_match = acvec
            .iter()
            .find(|a| a.launches_plugin("session-manager"))
            .is_some();
        if has_match {
            Some(key.clone())
        } else {
            None
        }
    });
    if let Some(matching) = matching.take() {
        vec![matching]
    } else {
        vec![]
    }
}

fn share_key(keymap: &[(KeyWithModifier, Vec<Action>)]) -> Vec<KeyWithModifier> {
    let mut matching = keymap.iter().find_map(|(key, acvec)| {
        let has_match = acvec
            .iter()
            .find(|a| a.launches_plugin("zellij:share"))
            .is_some();
        if has_match {
            Some(key.clone())
        } else {
            None
        }
    });
    if let Some(matching) = matching.take() {
        vec![matching]
    } else {
        vec![]
    }
}

fn plugin_manager_key(keymap: &[(KeyWithModifier, Vec<Action>)]) -> Vec<KeyWithModifier> {
    let mut matching = keymap.iter().find_map(|(key, acvec)| {
        let has_match = acvec
            .iter()
            .find(|a| a.launches_plugin("plugin-manager"))
            .is_some();
        if has_match {
            Some(key.clone())
        } else {
            None
        }
    });
    if let Some(matching) = matching.take() {
        vec![matching]
    } else {
        vec![]
    }
}

fn about_key(keymap: &[(KeyWithModifier, Vec<Action>)]) -> Vec<KeyWithModifier> {
    let mut matching = keymap.iter().find_map(|(key, acvec)| {
        let has_match = acvec
            .iter()
            .find(|a| a.launches_plugin("zellij:about"))
            .is_some();
        if has_match {
            Some(key.clone())
        } else {
            None
        }
    });
    if let Some(matching) = matching.take() {
        vec![matching]
    } else {
        vec![]
    }
}

fn configuration_key(keymap: &[(KeyWithModifier, Vec<Action>)]) -> Vec<KeyWithModifier> {
    let mut matching = keymap.iter().find_map(|(key, acvec)| {
        let has_match = acvec
            .iter()
            .find(|a| a.launches_plugin("configuration"))
            .is_some();
        if has_match {
            Some(key.clone())
        } else {
            None
        }
    });
    if let Some(matching) = matching.take() {
        vec![matching]
    } else {
        vec![]
    }
}

fn style_key_with_modifier(keyvec: &[KeyWithModifier], color_index: Option<usize>) -> LinePart {
    if keyvec.is_empty() {
        return LinePart::default();
    }

    let common_modifiers = get_common_modifiers(keyvec.iter().collect());

    let no_common_modifier = common_modifiers.is_empty();
    let modifier_str = common_modifiers
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join("-");

    // Prints the keys
    let key = keyvec
        .iter()
        .map(|key| {
            if no_common_modifier || keyvec.len() == 1 {
                format!("{}", key)
            } else {
                format!("{}", key.strip_common_modifiers(&common_modifiers))
            }
        })
        .collect::<Vec<String>>();

    // Special handling of some pre-defined keygroups
    let key_string = key.join("");
    let key_separator = match &key_string[..] {
        "HJKL" => "",
        "hjkl" => "",
        "←↓↑→" => "",
        "←→" => "",
        "↓↑" => "",
        "[]" => "",
        _ => "|",
    };

    if no_common_modifier || key.len() == 1 {
        let key_string_text = format!(" {} ", key.join(key_separator));
        let text = if let Some(color_index) = color_index {
            Text::new(&key_string_text)
                .color_range(color_index, ..)
                .opaque()
        } else {
            Text::new(&key_string_text).opaque()
        };
        LinePart {
            part: serialize_text(&text),
            len: key_string_text.width(),
        }
    } else {
        let key_string_without_modifier = format!("{}", key.join(key_separator));
        let key_string_text = format!(" {} <{}> ", modifier_str, key_string_without_modifier);
        let text = if let Some(color_index) = color_index {
            Text::new(&key_string_text)
                .color_range(color_index, ..modifier_str.width() + 1)
                .color_range(
                    color_index,
                    modifier_str.width() + 3
                        ..modifier_str.width() + 3 + key_string_without_modifier.width(),
                )
                .opaque()
        } else {
            Text::new(&key_string_text).opaque()
        };
        LinePart {
            part: serialize_text(&text),
            len: key_string_text.width(),
        }
    }
}

fn get_common_modifiers(mut keyvec: Vec<&KeyWithModifier>) -> Vec<KeyModifier> {
    if keyvec.is_empty() {
        return vec![];
    }
    let mut common_modifiers = keyvec.pop().unwrap().key_modifiers.clone();
    for key in keyvec {
        common_modifiers = common_modifiers
            .intersection(&key.key_modifiers)
            .cloned()
            .collect();
    }
    common_modifiers.into_iter().collect()
}

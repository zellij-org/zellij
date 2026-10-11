use ansi_term::{ANSIString, ANSIStrings};
use ansi_term::{
    Color::{Fixed, RGB},
    Style,
};
use std::collections::HashMap;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zellij_tile_utils::{palette_match, style};

use crate::click_actions::{actions_for_key, ClickRegion};
use crate::status_bar::first_line::{to_char, KeyAction, KeyMode, KeyShortcut};
use crate::status_bar::hover::is_hovered;
use crate::status_bar::second_line::{
    ascended_to_host_session_hint, descended_into_nested_session_hint, system_clipboard_error,
    text_copied_hint,
};
use crate::status_bar::{action_key, action_key_group, color_elements, MORE_MSG, TO_NORMAL};
use crate::status_bar::{ColoredElements, LinePart};
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
    if help.session_dimmed.unwrap_or(false) {
        return descended_into_nested_session_hint(help, max_len);
    }
    if help.session_ascended.unwrap_or(false) {
        return ascended_to_host_session_hint(help, max_len);
    }
    if let Some(text_copied_to_clipboard_destination) = text_copied_to_clipboard_destination {
        return text_copied_hint(text_copied_to_clipboard_destination);
    }
    if clipboard_failure {
        return system_clipboard_error(&help.style.colors);
    }
    let mut line_part_to_render = LinePart::default();

    let left_part = render_mode_key_indicators(help, max_len, separator, base_mode_is_locked);
    if let Some(left) = left_part {
        line_part_to_render.append(&left);
        max_len = max_len.saturating_sub(left.len);
        match help.mode {
            InputMode::Normal | InputMode::Locked => {
                if let Some(secondary_info) = render_secondary_info(help, tab_info, max_len) {
                    line_part_to_render.append(&secondary_info);
                }
            },
            _ => {
                if let Some(key_group_separator) = add_keygroup_separator(help, max_len) {
                    line_part_to_render.append(&key_group_separator);
                    max_len = max_len.saturating_sub(key_group_separator.len);
                    if let Some(keybinds) = keybinds(help, max_len) {
                        line_part_to_render.append(&keybinds);
                    }
                }
            },
        }
    }
    line_part_to_render
}

fn to_base_mode(base_mode: InputMode) -> Action {
    Action::SwitchToMode {
        input_mode: base_mode,
    }
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
                    &[Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    }],
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
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Locked,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Pane,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Pane,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Tab,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Tab,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Resize,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Resize,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Move,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Move,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Search,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Scroll,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Session,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Session,
                        }],
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
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Locked,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Pane,
                    to_char(action_key(
                        pane_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Normal,
                        }],
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
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Locked,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Tab,
                    to_char(action_key(
                        tab_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Normal,
                        }],
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
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Locked,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Resize,
                    to_char(action_key(
                        resize_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Normal,
                        }],
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
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Locked,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Move,
                    to_char(action_key(
                        move_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Normal,
                        }],
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
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Locked,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Search,
                    to_char(action_key(
                        scroll_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Normal,
                        }],
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
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Locked,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Session,
                    to_char(action_key(
                        session_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Normal,
                        }],
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
                    &[Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    }],
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
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Locked,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Pane,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Pane,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Tab,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Tab,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Resize,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Resize,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Move,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Move,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Search,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Scroll,
                        }],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Session,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode {
                            input_mode: InputMode::Session,
                        }],
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
                    &[Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    }],
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
                    &[Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    }],
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
                    &[Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    }],
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
                    &[Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    }],
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
                    &[Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    }],
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
                    &[Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    }],
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
    let colored_elements = color_elements(
        help.style.colors,
        !supports_arrow_fonts,
        help.session_dimmed.unwrap_or(false),
    );
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

                let full_shortcut_list = full_inline_keys_modes_shortcut_list(
                    &keys_without_common_modifiers,
                    help,
                    &modifiers,
                );

                if line_part_to_render.len + full_shortcut_list.len <= max_len {
                    line_part_to_render.append(&full_shortcut_list);
                } else {
                    let shortened_shortcut_list = shortened_inline_keys_modes_shortcut_list(
                        &keys_without_common_modifiers,
                        help,
                        &modifiers,
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
    stripped_modifiers: &[KeyModifier],
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
            stripped_modifiers,
        );
        full_shortcut_list.append(&shortcut);
    }
    full_shortcut_list
}

fn shortened_inline_keys_modes_shortcut_list(
    keys_without_common_modifiers: &Vec<KeyShortcut>,
    help: &ModeInfo,
    stripped_modifiers: &[KeyModifier],
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
            stripped_modifiers,
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
    let prefix_text_len = prefix_text.len();

    let prefix = if mode_info.session_dimmed.unwrap_or(false) {
        serialize_text(&Text::from(prefix_text).disabled().opaque())
    } else {
        serialize_text(&Text::from(prefix_text).opaque())
    };
    let suffix_separator = palette.superkey_suffix_separator.paint(separator);
    line_part_to_render.part =
        format!("{}{}{}", line_part_to_render.part, prefix, suffix_separator);
    line_part_to_render.len += prefix_text_len + separator.chars().count();
}

fn render_secondary_info(
    help: &ModeInfo,
    tab_info: Option<&TabInfo>,
    max_len: usize,
) -> Option<LinePart> {
    let mut secondary_info = LinePart::default();
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let colored_elements = color_elements(
        help.style.colors,
        !supports_arrow_fonts,
        help.session_dimmed.unwrap_or(false),
    );
    let secondary_keybinds = secondary_keybinds(&help, tab_info, max_len);
    secondary_info.append(&secondary_keybinds);
    let remaining_space = max_len.saturating_sub(secondary_info.len).saturating_sub(1);
    let mut padding = String::new();
    let mut padding_len = 0;
    for _ in 0..remaining_space {
        padding.push_str(&ANSIStrings(&[colored_elements.superkey_prefix.paint(" ")]).to_string());
        padding_len += 1;
    }
    secondary_info.prepend_padding(&padding, padding_len);
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
    let binds = &help.get_mode_keybinds();
    let should_show_focus_and_resize_shortcuts = should_show_focus_and_resize_shortcuts(tab_info);
    let new_pane_action_key = action_key(
        binds,
        &[Action::NewPane {
            direction: None,
            pane_name: None,
            start_suppressed: false,
        }],
    );
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

    let resize_increase_action_key = action_key(
        binds,
        &[Action::Resize {
            resize: Resize::Increase,
            direction: None,
        }],
    );
    let resize_increase_key = resize_increase_action_key
        .iter()
        .find(|k| k.bare_key == BareKey::Char('+'))
        .or_else(|| resize_increase_action_key.iter().next());
    let resize_decrease_action_key = action_key(
        binds,
        &[Action::Resize {
            resize: Resize::Decrease,
            direction: None,
        }],
    );
    let resize_decrease_key = resize_decrease_action_key
        .iter()
        .find(|k| k.bare_key == BareKey::Char('-'))
        .or_else(|| resize_decrease_action_key.iter().next());
    let mut resize_shortcuts = vec![];
    if let Some(resize_increase_key) = resize_increase_key {
        resize_shortcuts.push(resize_increase_key.clone());
    }
    if let Some(resize_decrease_key) = resize_decrease_key {
        resize_shortcuts.push(resize_decrease_key.clone());
    }

    let mut move_focus_shortcuts: Vec<KeyWithModifier> = vec![];

    let move_focus_left_action_key = action_key(
        binds,
        &[Action::MoveFocusOrTab {
            direction: Direction::Left,
        }],
    );
    let move_focus_left_key = move_focus_left_action_key
        .iter()
        .find(|k| k.bare_key == BareKey::Left)
        .or_else(|| move_focus_left_action_key.iter().next());
    if let Some(move_focus_left_key) = move_focus_left_key {
        move_focus_shortcuts.push(move_focus_left_key.clone());
    }
    let move_focus_left_action_key = action_key(
        binds,
        &[Action::MoveFocus {
            direction: Direction::Down,
        }],
    );
    let move_focus_left_key = move_focus_left_action_key
        .iter()
        .find(|k| k.bare_key == BareKey::Down)
        .or_else(|| move_focus_left_action_key.iter().next());
    if let Some(move_focus_left_key) = move_focus_left_key {
        move_focus_shortcuts.push(move_focus_left_key.clone());
    }
    let move_focus_left_action_key = action_key(
        binds,
        &[Action::MoveFocus {
            direction: Direction::Up,
        }],
    );
    let move_focus_left_key = move_focus_left_action_key
        .iter()
        .find(|k| k.bare_key == BareKey::Up)
        .or_else(|| move_focus_left_action_key.iter().next());
    if let Some(move_focus_left_key) = move_focus_left_key {
        move_focus_shortcuts.push(move_focus_left_key.clone());
    }
    let move_focus_left_action_key = action_key(
        binds,
        &[Action::MoveFocusOrTab {
            direction: Direction::Right,
        }],
    );
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
    let hint_line = |new_pane_text: &str, focus_text: &str| -> LinePart {
        let mut line = LinePart::default();
        if no_common_modifier {
            line.append(&add_shortcut(
                help,
                new_pane_text,
                &new_pane_key_to_display,
                false,
                Some(0),
            ));
            if should_show_focus_and_resize_shortcuts {
                line.append(&add_shortcut(
                    help,
                    focus_text,
                    &move_focus_shortcuts,
                    false,
                    Some(0),
                ));
                line.append(&add_shortcut(
                    help,
                    "Resize",
                    &resize_shortcuts,
                    false,
                    Some(0),
                ));
            }
            line.append(&add_shortcut(
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
                help.session_dimmed.unwrap_or(false),
            );
            line.append(&modifier_str);
            let strip = |keys: &Vec<KeyWithModifier>| -> Vec<KeyWithModifier> {
                keys.iter()
                    .map(|k| k.strip_common_modifiers(&common_modifiers))
                    .collect()
            };
            line.append(&add_shortcut_with_inline_key(
                help,
                new_pane_text,
                strip(&new_pane_key_to_display),
                false,
                &common_modifiers,
            ));
            if should_show_focus_and_resize_shortcuts {
                line.append(&add_shortcut_with_inline_key(
                    help,
                    focus_text,
                    strip(&move_focus_shortcuts),
                    false,
                    &common_modifiers,
                ));
                line.append(&add_shortcut_with_inline_key(
                    help,
                    "Resize",
                    strip(&resize_shortcuts),
                    false,
                    &common_modifiers,
                ));
            }
            line.append(&add_shortcut_with_inline_key(
                help,
                "Floating",
                strip(&toggle_floating_key_to_display),
                are_floating_panes_visible,
                &common_modifiers,
            ));
        }
        line
    };

    let secondary_info = hint_line("New Pane", "Change Focus");
    if secondary_info.len <= max_len {
        return secondary_info;
    }
    let short_line = hint_line("New", "Focus");
    if short_line.len <= max_len {
        short_line
    } else if max_len >= 3 {
        let overflow_text = Text::from(format!("{:>width$}", "...", width = max_len));
        let part = if help.session_dimmed.unwrap_or(false) {
            serialize_text(&overflow_text.disabled().opaque())
        } else {
            serialize_text(&overflow_text.color_range(0, ..).opaque())
        };
        let len = max_len;
        LinePart {
            part,
            len,
            ..Default::default()
        }
    } else {
        LinePart::default()
    }
}

fn text_as_line_part_with_emphasis(text: String, emphases_index: usize, dimmed: bool) -> LinePart {
    let text_width = text.width();
    let part = if dimmed {
        serialize_text(&Text::from(text).disabled().opaque())
    } else {
        serialize_text(&Text::from(text).color_range(emphases_index, ..).opaque())
    };
    LinePart {
        part,
        len: text_width,
        ..Default::default()
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

fn hovered_ribbon_wrap(body: String, palette: Styling, supports_arrow_fonts: bool) -> String {
    let ribbon_bg = palette.ribbon_unselected.emphasis_1;
    let outer_bg = palette.text_unselected.background;
    let arrow = if supports_arrow_fonts {
        crate::status_bar::ARROW_SEPARATOR
    } else {
        ""
    };
    let left = style!(outer_bg, ribbon_bg).paint(arrow).to_string();
    let right = style!(ribbon_bg, outer_bg).paint(arrow).to_string();
    format!("{}{}{}", left, body, right)
}

fn key_targets(
    help: &ModeInfo,
    keys: &[KeyWithModifier],
    stripped_modifiers: &[KeyModifier],
) -> Vec<Option<Vec<Action>>> {
    let keybinds = help.get_mode_keybinds();
    keys.iter()
        .map(|key| actions_for_key(&keybinds, key, stripped_modifiers))
        .collect()
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct HintHover {
    whole: bool,
    key: Option<usize>,
}

impl HintHover {
    fn any(&self) -> bool {
        self.whole || self.key.is_some()
    }
}

fn target_is_hovered(target: &Option<Vec<Action>>) -> bool {
    target
        .as_ref()
        .map(|actions| is_hovered(actions))
        .unwrap_or(false)
}

fn hint_hover(help: &ModeInfo, targets: &[Option<Vec<Action>>]) -> HintHover {
    if help.session_dimmed.unwrap_or(false) {
        return HintHover::default();
    }
    if targets.len() == 1 {
        HintHover {
            whole: target_is_hovered(&targets[0]),
            key: None,
        }
    } else {
        HintHover {
            whole: false,
            key: targets.iter().position(target_is_hovered),
        }
    }
}

fn hint_clicks(
    targets: &[Option<Vec<Action>>],
    key_ranges: &[(usize, usize)],
    total_len: usize,
) -> Vec<ClickRegion> {
    if targets.len() == 1 {
        return targets[0]
            .clone()
            .map(|actions| vec![ClickRegion::new(0, total_len, actions)])
            .unwrap_or_default();
    }
    targets
        .iter()
        .zip(key_ranges.iter())
        .filter_map(|(target, (start, end))| {
            target
                .clone()
                .map(|actions| ClickRegion::new(*start, *end, actions))
        })
        .collect()
}

fn label_ranges(
    labels: &[String],
    separator_width: usize,
    first_col: usize,
) -> Vec<(usize, usize)> {
    let mut ranges = vec![];
    let mut col = first_col;
    for (index, label) in labels.iter().enumerate() {
        if index > 0 {
            col += separator_width;
        }
        let end = col + label.width();
        ranges.push((col, end));
        col = end;
    }
    ranges
}

fn inline_key_separator(joined_keys: &str) -> &'static str {
    match joined_keys {
        "HJKL" => "",
        "hjkl" => "",
        "←↓↑→" => "",
        "←→" => "",
        "↓↑" => "",
        "[]" => "",
        "+-" => "",
        _ => "|",
    }
}

fn ribbon_content_offset(help: &ModeInfo) -> usize {
    if !help.capabilities.arrow_fonts {
        2
    } else {
        1
    }
}

fn hovered_inline_ribbon(
    help: &ModeInfo,
    text: &str,
    labels: &[String],
    key_separator: &str,
    hovered_key: Option<usize>,
) -> String {
    let palette = help.style.colors;
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let ribbon_bg = palette.ribbon_unselected.emphasis_1;
    let base = style!(palette.ribbon_unselected.base, ribbon_bg).bold();
    let emph = style!(palette.ribbon_unselected.emphasis_0, ribbon_bg).bold();
    let mut body = base.paint(" <").to_string();
    for (index, label) in labels.iter().enumerate() {
        if index > 0 {
            body.push_str(&base.paint(key_separator).to_string());
        }
        let label_style = if hovered_key == Some(index) {
            style!(
                palette.ribbon_selected.base,
                palette.ribbon_selected.background
            )
            .bold()
        } else {
            emph
        };
        body.push_str(&label_style.paint(label.clone()).to_string());
    }
    body.push_str(&base.paint(format!("> {} ", text)).to_string());
    hovered_ribbon_wrap(body, palette, supports_arrow_fonts)
}
fn add_shortcut(
    help: &ModeInfo,
    text: &str,
    keys: &Vec<KeyWithModifier>,
    selected: bool,
    key_color_index: Option<usize>,
) -> LinePart {
    let dimmed = help.session_dimmed.unwrap_or(false);
    let mut ret = LinePart::default();
    if keys.is_empty() {
        return ret;
    }

    let targets = key_targets(help, keys, &[]);
    let hover = hint_hover(help, &targets);
    let (key_part, key_ranges) = style_key_with_modifier(&keys, key_color_index, dimmed, hover.key);
    ret.append(&key_part);
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let ribbon = if hover.any() {
        let palette = help.style.colors;
        let body = style!(
            palette.ribbon_unselected.base,
            palette.ribbon_unselected.emphasis_1
        )
        .bold()
        .paint(format!(" {} ", text))
        .to_string();
        hovered_ribbon_wrap(body, palette, supports_arrow_fonts)
    } else if dimmed {
        serialize_ribbon(&Text::from(format!("{}", text)).disabled())
    } else if selected {
        serialize_ribbon(&Text::from(format!("{}", text)).selected())
    } else {
        serialize_ribbon(&Text::from(format!("{}", text)))
    };
    ret.part = format!("{}{}", ret.part, ribbon);
    ret.len += if supports_arrow_fonts {
        text.width() + 4
    } else {
        text.width() + 2
    };
    ret.clicks = hint_clicks(&targets, &key_ranges, ret.len);
    ret
}

fn add_shortcut_with_inline_key(
    help: &ModeInfo,
    text: &str,
    key: Vec<KeyWithModifier>,
    is_selected: bool,
    stripped_modifiers: &[KeyModifier],
) -> LinePart {
    let capabilities = help.capabilities;

    let mut ret = LinePart::default();
    if key.is_empty() {
        return ret;
    }

    let targets = key_targets(help, &key, stripped_modifiers);
    let hover = hint_hover(help, &targets);
    let labels: Vec<String> = key.iter().map(|k| k.to_string()).collect();
    let key_separator = inline_key_separator(&labels.join(""));
    let key_string = labels.join(key_separator);

    let ribbon = if hover.any() {
        hovered_inline_ribbon(help, text, &labels, key_separator, hover.key)
    } else if help.session_dimmed.unwrap_or(false) {
        serialize_ribbon(&Text::from(format!("<{}> {}", key_string, text)).disabled())
    } else if is_selected {
        serialize_ribbon(
            &Text::from(format!("<{}> {}", key_string, text))
                .color_range(0, 1..key_string.width() + 1)
                .selected(),
        )
    } else {
        serialize_ribbon(
            &Text::from(format!("<{}> {}", key_string, text))
                .color_range(0, 1..key_string.width() + 1),
        )
    };
    ret.part = ribbon;
    let supports_arrow_fonts = !capabilities.arrow_fonts;
    ret.len += if supports_arrow_fonts {
        text.width() + key_string.width() + 7
    } else {
        text.width() + key_string.width() + 5
    };
    let key_ranges = label_ranges(
        &labels,
        key_separator.width(),
        ribbon_content_offset(help) + 1,
    );
    ret.clicks = hint_clicks(&targets, &key_ranges, ret.len);

    ret
}

fn add_shortcut_with_key_only(
    help: &ModeInfo,
    key: Vec<KeyWithModifier>,
    is_selected: bool,
    stripped_modifiers: &[KeyModifier],
) -> LinePart {
    let mut ret = LinePart::default();
    if key.is_empty() {
        return ret;
    }

    let targets = key_targets(help, &key, stripped_modifiers);
    let hover = hint_hover(help, &targets);
    let labels: Vec<String> = key.iter().map(|k| k.to_string()).collect();
    let key_string = labels.join("-");
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;

    let ribbon = if hover.any() {
        let palette = help.style.colors;
        let body = style!(
            palette.ribbon_unselected.emphasis_0,
            palette.ribbon_unselected.emphasis_1
        )
        .bold()
        .paint(format!(" {} ", key_string))
        .to_string();
        hovered_ribbon_wrap(body, palette, supports_arrow_fonts)
    } else if help.session_dimmed.unwrap_or(false) {
        serialize_ribbon(&Text::from(format!("{}", key_string)).disabled())
    } else if is_selected {
        serialize_ribbon(
            &Text::from(format!("{}", key_string))
                .color_range(0, ..)
                .selected(),
        )
    } else {
        serialize_ribbon(&Text::from(format!("{}", key_string)).color_range(0, ..))
    };
    ret.part = ribbon;
    ret.len += if supports_arrow_fonts {
        key_string.width() + 4
    } else {
        key_string.width() + 2
    };
    let key_ranges = label_ranges(&labels, 1, ribbon_content_offset(help));
    ret.clicks = hint_clicks(&targets, &key_ranges, ret.len);
    ret
}

fn add_keygroup_separator(help: &ModeInfo, max_len: usize) -> Option<LinePart> {
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let separator = if supports_arrow_fonts {
        crate::status_bar::ARROW_SEPARATOR
    } else {
        " "
    };
    let palette = help.style.colors;

    if help.session_dimmed.unwrap_or(false) {
        let mut ret = LinePart::default();
        let dim_style = Style::new()
            .fg(palette_match!(palette.text_unselected.base))
            .on(palette_match!(palette.text_unselected.background))
            .italic();
        let mut bits: Vec<ANSIString> = vec![];
        let mode_help_text = match help.mode {
            InputMode::RenamePane => Some("RENAMING PANE"),
            InputMode::RenameTab => Some("RENAMING TAB"),
            InputMode::EnterSearch => Some("ENTERING SEARCH TERM"),
            InputMode::Search => Some("SEARCHING"),
            _ => None,
        };
        if let Some(mode_help_text) = mode_help_text {
            bits.push(dim_style.paint(format!(" {} ", mode_help_text)));
            ret.len += mode_help_text.width() + 2;
        }
        bits.push(dim_style.paint(format!("{}", separator)));
        bits.push(dim_style.paint(format!(" ")));
        bits.push(dim_style.paint(format!("{}", separator)));
        ret.part = format!("{}{}", ret.part, ANSIStrings(&bits));
        ret.len += 3;

        if ret.len <= max_len {
            return Some(ret);
        } else {
            return None;
        }
    }

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
        ret.len += mode_help_text.width() + 2;
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
    ret.len += 3;

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

    let base_mode = mi.base_mode;
    let to_basemode_keys = base_mode.map(|b| action_key(&old_keymap, &[to_base_mode(b)])).unwrap_or_else(|| action_key(&old_keymap, &[TO_NORMAL]));
    let to_basemode_key = if to_basemode_keys.contains(&KeyWithModifier::new(BareKey::Enter)) {
        vec![KeyWithModifier::new(BareKey::Enter)]
    } else {
        to_basemode_keys.into_iter().take(1).collect()
    };

    old_keymap.sort_unstable_by(|(keya, _), (keyb, _)| keya.partial_cmp(keyb).unwrap());

    let mut known_actions: Vec<Vec<Action>> = vec![];
    let mut km = vec![];
    for (key, acvec) in old_keymap {
        if known_actions.contains(&acvec) {
            continue;
        } else {
            known_actions.push(acvec.to_vec());
            km.push((key, acvec));
        }
    }

    if mi.mode == IM::Pane { vec![
        (s("New"), s("New"), single_action_key(&km, &[A::NewPane{direction: None, pane_name: None, start_suppressed: false}, TO_NORMAL])),
        (s("Change Focus"), s("Move"),
            action_key_group(&km, &[&[A::MoveFocus{direction: Dir::Left}], &[A::MoveFocus{direction: Dir::Down}],
                &[A::MoveFocus{direction: Dir::Up}], &[A::MoveFocus{direction: Dir::Right}]])),
        (s("Close"), s("Close"), single_action_key(&km, &[A::CloseFocus, TO_NORMAL])),
        (s("Rename"), s("Rename"),
            single_action_key(&km, &[A::SwitchToMode{input_mode: IM::RenamePane}, A::PaneNameInput{input: vec![0]}])),
        (s("Toggle Fullscreen"), s("Fullscreen"), single_action_key(&km, &[A::ToggleFocusFullscreen, TO_NORMAL])),
        (s("Toggle Floating"), s("Floating"),
            single_action_key(&km, &[A::ToggleFloatingPanes, TO_NORMAL])),
        (s("Toggle Embed"), s("Embed"), single_action_key(&km, &[A::TogglePaneEmbedOrFloating, TO_NORMAL])),
        (s("Split Right"), s("Right"), single_action_key(&km, &[A::NewPane{direction: Some(Direction::Right), pane_name: None, start_suppressed: false}, TO_NORMAL])),
        (s("Split Down"), s("Down"), single_action_key(&km, &[A::NewPane{direction: Some(Direction::Down), pane_name: None, start_suppressed: false}, TO_NORMAL])),
        (s("Stack"), s("Stack"), single_action_key(&km, &[A::NewStackedPane{command: None, pane_name: None, near_current_pane: false, no_focus: false, tab_id: None}, TO_NORMAL])),
        (s("Select pane"), s("Select"), to_basemode_key),
    ]} else if mi.mode == IM::Tab {
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
        (s("New"), s("New"), single_action_key(&km, &[A::NewTab{
            tiled_layout: None,
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: true,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        }, TO_NORMAL])),
        (s("Change focus"), s("Move"), focus_keys),
        (s("Close"), s("Close"), single_action_key(&km, &[A::CloseTab, TO_NORMAL])),
        (s("Rename"), s("Rename"),
            single_action_key(&km, &[A::SwitchToMode{input_mode: IM::RenameTab}, A::TabNameInput{input: vec![0]}])),
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
                &[A::Resize{resize: Resize::Increase, direction: None}],
                &[A::Resize{resize: Resize::Decrease, direction: None}]
            ])),
        (s("Increase to"), s("Increase"), action_key_group(&km, &[
            &[A::Resize{resize: Resize::Increase, direction: Some(Dir::Left)}],
            &[A::Resize{resize: Resize::Increase, direction: Some(Dir::Down)}],
            &[A::Resize{resize: Resize::Increase, direction: Some(Dir::Up)}],
            &[A::Resize{resize: Resize::Increase, direction: Some(Dir::Right)}]
            ])),
        (s("Decrease from"), s("Decrease"), action_key_group(&km, &[
            &[A::Resize{resize: Resize::Decrease, direction: Some(Dir::Left)}],
            &[A::Resize{resize: Resize::Decrease, direction: Some(Dir::Down)}],
            &[A::Resize{resize: Resize::Decrease, direction: Some(Dir::Up)}],
            &[A::Resize{resize: Resize::Decrease, direction: Some(Dir::Right)}]
            ])),
        (s("Select pane"), s("Select"), to_basemode_key),
    ]} else if mi.mode == IM::Move { vec![
        (s("Switch Location"), s("Move"), action_key_group(&km, &[
            &[Action::MovePane{direction: Some(Dir::Left)}], &[Action::MovePane{direction: Some(Dir::Down)}],
            &[Action::MovePane{direction: Some(Dir::Up)}], &[Action::MovePane{direction: Some(Dir::Right)}]])),
        (s("When done"), s("Back"), to_basemode_key),
    ]} else if mi.mode == IM::Scroll { vec![
        (s("Enter search term"), s("Search"),
            action_key(&km, &[A::SwitchToMode{input_mode: IM::EnterSearch}, A::SearchInput{input: vec![0]}])),
        (s("Scroll"), s("Scroll"), action_key_group(&km, &[
            &[Action::ScrollDown], &[Action::ScrollUp],
            &[Action::PageScrollDown], &[Action::PageScrollUp],
            &[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]])),
        (s("Edit scrollback in default editor"), s("Edit"),
            single_action_key(&km, &[Action::EditScrollback { ansi: false }, TO_NORMAL])),
        (s("Select pane"), s("Select"), to_basemode_key),
        (s("Scroll commands"), s("Commands"),
            action_key_group(&km, &[&[Action::ScrollToPreviousPrompt], &[Action::ScrollToNextPrompt]])),
    ]} else if mi.mode == IM::EnterSearch { vec![
        (s("When done"), s("Done"), action_key(&km, &[A::SwitchToMode{input_mode: IM::Search}])),
        (s("Cancel"), s("Cancel"),
            action_key(&km, &[A::SearchInput{input: vec![27]}, A::SwitchToMode{input_mode: IM::Scroll}])),
    ]} else if mi.mode == IM::Search { vec![
        (s("Enter Search term"), s("Search"),
            action_key(&km, &[A::SwitchToMode{input_mode: IM::EnterSearch}, A::SearchInput{input: vec![0]}])),
        (s("Scroll"), s("Scroll"),
            action_key_group(&km, &[&[Action::ScrollDown], &[Action::ScrollUp]])),
        (s("Scroll page"), s("Scroll"),
            action_key_group(&km, &[&[Action::PageScrollDown], &[Action::PageScrollUp]])),
        (s("Scroll half page"), s("Scroll"),
            action_key_group(&km, &[&[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]])),
        (s("Search down"), s("Down"), action_key(&km, &[A::Search{direction: SDir::Down}])),
        (s("Search up"), s("Up"), action_key(&km, &[A::Search{direction: SDir::Up}])),
        (s("Case sensitive"), s("Case"),
            action_key(&km, &[A::SearchToggleOption{option: SOpt::CaseSensitivity}])),
        (s("Wrap"), s("Wrap"),
            action_key(&km, &[A::SearchToggleOption{option: SOpt::Wrap}])),
        (s("Whole words"), s("Whole"),
            action_key(&km, &[A::SearchToggleOption{option: SOpt::WholeWord}])),
    ]} else if mi.mode == IM::Session { vec![
        (s("Detach"), s("Detach"), action_key(&km, &[Action::Detach])),
        (s("Session Manager"), s("Manager"), session_manager_key(&km)),
        (s("Share"), s("Share"), share_key(&km)),
        (s("Configure"), s("Config"), configuration_key(&km)),
        (s("Layout Manager"), s("Layouts"), layout_manager_key(&km)),
        (s("Plugin Manager"), s("Plugins"), plugin_manager_key(&km)),
        (s("About"), s("About"), about_key(&km)),
        (s("Select pane"), s("Select"), to_basemode_key),
    ]} else if mi.mode == IM::Tmux { vec![
        (s("Move focus"), s("Move"), action_key_group(&km, &[
            &[A::MoveFocus{direction: Dir::Left}], &[A::MoveFocus{direction: Dir::Down}],
            &[A::MoveFocus{direction: Dir::Up}], &[A::MoveFocus{direction: Dir::Right}]])),
        (s("Split down"), s("Down"), action_key(&km, &[A::NewPane{direction: Some(Dir::Down), pane_name: None, start_suppressed: false}, TO_NORMAL])),
        (s("Split right"), s("Right"), action_key(&km, &[A::NewPane{direction: Some(Dir::Right), pane_name: None, start_suppressed: false}, TO_NORMAL])),
        (s("Fullscreen"), s("Fullscreen"), action_key(&km, &[A::ToggleFocusFullscreen, TO_NORMAL])),
        (s("New tab"), s("New"), action_key(&km, &[A::NewTab{
            tiled_layout: None,
            floating_layouts: vec![],
            swap_tiled_layouts: None,
            swap_floating_layouts: None,
            tab_name: None,
            should_change_focus_to_new_tab: true,
            cwd: None,
            initial_panes: None,
            first_pane_unblock_condition: None,
        }, TO_NORMAL])),
        (s("Rename tab"), s("Rename"),
            action_key(&km, &[A::SwitchToMode{input_mode: IM::RenameTab}, A::TabNameInput{input: vec![0]}])),
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

fn layout_manager_key(keymap: &[(KeyWithModifier, Vec<Action>)]) -> Vec<KeyWithModifier> {
    let mut matching = keymap.iter().find_map(|(key, acvec)| {
        let has_match = acvec
            .iter()
            .find(|a| a.launches_plugin("zellij:layout-manager"))
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

fn styled_key_piece(
    text: &str,
    colored: bool,
    hovered: bool,
    color_index: Option<usize>,
    dimmed: bool,
) -> String {
    let piece = Text::from(text);
    let piece = if dimmed {
        piece.disabled()
    } else if colored {
        match color_index {
            Some(color_index) => piece.color_range(color_index, ..),
            None => piece,
        }
    } else {
        piece
    };
    let piece = if hovered && !dimmed {
        piece.selected()
    } else {
        piece
    };
    serialize_text(&piece.opaque())
}

fn style_key_with_modifier(
    keyvec: &[KeyWithModifier],
    color_index: Option<usize>,
    dimmed: bool,
    hovered_key: Option<usize>,
) -> (LinePart, Vec<(usize, usize)>) {
    if keyvec.is_empty() {
        return (LinePart::default(), vec![]);
    }

    let common_modifiers = get_common_modifiers(keyvec.iter().collect());

    let no_common_modifier = common_modifiers.is_empty();
    let modifier_str = common_modifiers
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join("-");

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

    let mut part = String::new();
    let mut len = 0;
    let mut ranges = vec![];
    let mut push = |text: &str, colored: bool, hovered: bool| -> (usize, usize) {
        let start = len;
        part.push_str(&styled_key_piece(
            text,
            colored,
            hovered,
            color_index,
            dimmed,
        ));
        len += text.width();
        (start, len)
    };

    if no_common_modifier || key.len() == 1 {
        push(" ", true, false);
        for (index, label) in key.iter().enumerate() {
            if index > 0 && !key_separator.is_empty() {
                push(key_separator, true, false);
            }
            ranges.push(push(label, true, hovered_key == Some(index)));
        }
        push(" ", true, false);
    } else {
        push(&format!(" {}", modifier_str), true, false);
        push(" <", false, false);
        for (index, label) in key.iter().enumerate() {
            if index > 0 && !key_separator.is_empty() {
                push(key_separator, true, false);
            }
            ranges.push(push(label, true, hovered_key == Some(index)));
        }
        push("> ", false, false);
    }
    (
        LinePart {
            part,
            len,
            ..Default::default()
        },
        ranges,
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status_bar::hover::with_hovered;

    fn key(bare_key: BareKey) -> KeyWithModifier {
        KeyWithModifier::new(bare_key)
    }

    fn alt(bare_key: BareKey) -> KeyWithModifier {
        KeyWithModifier::new(bare_key).with_alt_modifier()
    }

    fn ctrl(c: char) -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Char(c)).with_ctrl_modifier()
    }

    fn to_mode(input_mode: InputMode) -> Vec<Action> {
        vec![Action::SwitchToMode { input_mode }]
    }

    fn resize(resize: Resize, direction: Option<Direction>) -> Vec<Action> {
        vec![Action::Resize { resize, direction }]
    }

    fn normal_keybinds(increase: char, decrease: char) -> Vec<(KeyWithModifier, Vec<Action>)> {
        vec![
            (ctrl('g'), to_mode(InputMode::Locked)),
            (ctrl('p'), to_mode(InputMode::Pane)),
            (ctrl('t'), to_mode(InputMode::Tab)),
            (ctrl('n'), to_mode(InputMode::Resize)),
            (ctrl('h'), to_mode(InputMode::Move)),
            (ctrl('s'), to_mode(InputMode::Scroll)),
            (ctrl('o'), to_mode(InputMode::Session)),
            (ctrl('q'), vec![Action::Quit]),
            (
                alt(BareKey::Char('n')),
                vec![Action::NewPane {
                    direction: None,
                    pane_name: None,
                    start_suppressed: false,
                }],
            ),
            (
                alt(BareKey::Left),
                vec![Action::MoveFocusOrTab {
                    direction: Direction::Left,
                }],
            ),
            (
                alt(BareKey::Down),
                vec![Action::MoveFocus {
                    direction: Direction::Down,
                }],
            ),
            (
                alt(BareKey::Up),
                vec![Action::MoveFocus {
                    direction: Direction::Up,
                }],
            ),
            (
                alt(BareKey::Right),
                vec![Action::MoveFocusOrTab {
                    direction: Direction::Right,
                }],
            ),
            (alt(BareKey::Char(increase)), resize(Resize::Increase, None)),
            (alt(BareKey::Char(decrease)), resize(Resize::Decrease, None)),
            (alt(BareKey::Char('f')), vec![Action::ToggleFloatingPanes]),
        ]
    }

    fn resize_keybinds() -> Vec<(KeyWithModifier, Vec<Action>)> {
        vec![
            (ctrl('n'), to_mode(InputMode::Normal)),
            (key(BareKey::Char('+')), resize(Resize::Increase, None)),
            (key(BareKey::Char('-')), resize(Resize::Decrease, None)),
            (
                key(BareKey::Left),
                resize(Resize::Increase, Some(Direction::Left)),
            ),
            (
                key(BareKey::Down),
                resize(Resize::Increase, Some(Direction::Down)),
            ),
            (
                key(BareKey::Up),
                resize(Resize::Increase, Some(Direction::Up)),
            ),
            (
                key(BareKey::Right),
                resize(Resize::Increase, Some(Direction::Right)),
            ),
            (
                key(BareKey::Char('H')),
                resize(Resize::Decrease, Some(Direction::Left)),
            ),
        ]
    }

    fn mode_info(mode: InputMode, increase: char, decrease: char) -> ModeInfo {
        ModeInfo {
            mode,
            base_mode: Some(InputMode::Normal),
            keybinds: vec![
                (InputMode::Normal, normal_keybinds(increase, decrease)),
                (InputMode::Resize, resize_keybinds()),
            ],
            ..Default::default()
        }
    }

    fn tab_with_two_panes() -> TabInfo {
        TabInfo {
            active: true,
            selectable_tiled_panes_count: 2,
            ..Default::default()
        }
    }

    fn render(mode_info: &ModeInfo, hovered: Option<Vec<Action>>) -> LinePart {
        let tab = tab_with_two_panes();
        with_hovered(hovered, || {
            one_line_ui(mode_info, Some(&tab), 300, "", false, None, false)
        })
    }

    fn region_for<'a>(line: &'a LinePart, actions: &[Action]) -> Option<&'a ClickRegion> {
        line.clicks
            .iter()
            .find(|region| region.actions.as_slice() == actions)
    }

    fn assert_regions_are_sane(line: &LinePart) {
        let mut regions: Vec<&ClickRegion> = line.clicks.iter().collect();
        regions.sort_by_key(|region| region.start);
        for pair in regions.windows(2) {
            assert!(pair[0].end <= pair[1].start, "{:?}", regions);
        }
        for region in regions {
            assert!(region.start < region.end);
            assert!(region.end <= line.len);
        }
    }

    #[test]
    fn every_mode_label_in_normal_mode_is_clickable() {
        let line = render(&mode_info(InputMode::Normal, '+', '-'), None);
        assert_regions_are_sane(&line);
        for mode in [
            InputMode::Locked,
            InputMode::Pane,
            InputMode::Tab,
            InputMode::Resize,
            InputMode::Move,
            InputMode::Scroll,
            InputMode::Session,
        ] {
            assert!(
                region_for(&line, &to_mode(mode)).is_some(),
                "{:?} has no click region in {:?}",
                mode,
                line.clicks
            );
        }
        assert!(region_for(&line, &[Action::Quit]).is_some());
    }

    #[test]
    fn each_arrow_and_plus_minus_in_normal_mode_has_its_own_region() {
        let line = render(&mode_info(InputMode::Normal, '+', '-'), None);
        for actions in [
            vec![Action::MoveFocusOrTab {
                direction: Direction::Left,
            }],
            vec![Action::MoveFocus {
                direction: Direction::Down,
            }],
            vec![Action::MoveFocus {
                direction: Direction::Up,
            }],
            vec![Action::MoveFocusOrTab {
                direction: Direction::Right,
            }],
            resize(Resize::Increase, None),
            resize(Resize::Decrease, None),
        ] {
            let region = region_for(&line, &actions)
                .unwrap_or_else(|| panic!("{:?} has no region in {:?}", actions, line.clicks));
            assert_eq!(region.end - region.start, 1, "{:?}", region);
        }
        assert!(region_for(&line, &[Action::ToggleFloatingPanes]).is_some());
        assert!(region_for(
            &line,
            &[Action::NewPane {
                direction: None,
                pane_name: None,
                start_suppressed: false
            }]
        )
        .is_some());
    }

    #[test]
    fn the_decrease_key_is_found_even_when_it_is_not_minus() {
        let line = render(&mode_info(InputMode::Normal, '=', '_'), None);
        assert!(region_for(&line, &resize(Resize::Increase, None)).is_some());
        assert!(
            region_for(&line, &resize(Resize::Decrease, None)).is_some(),
            "{:?}",
            line.clicks
        );
    }

    #[test]
    fn resize_mode_hints_map_arrows_and_plus_minus_to_their_resize_actions() {
        let line = render(&mode_info(InputMode::Resize, '+', '-'), None);
        assert_regions_are_sane(&line);
        for direction in [
            Direction::Left,
            Direction::Down,
            Direction::Up,
            Direction::Right,
        ] {
            let region = region_for(&line, &resize(Resize::Increase, Some(direction)))
                .unwrap_or_else(|| panic!("{:?} missing in {:?}", direction, line.clicks));
            assert_eq!(region.end - region.start, 1);
        }
        assert!(region_for(&line, &resize(Resize::Increase, None)).is_some());
        assert!(region_for(&line, &resize(Resize::Decrease, None)).is_some());
        assert!(region_for(&line, &resize(Resize::Decrease, Some(Direction::Left))).is_some());
        assert!(region_for(&line, &to_mode(InputMode::Normal)).is_some());
    }

    #[test]
    fn hovering_changes_only_the_look_of_the_line() {
        let mode_info = mode_info(InputMode::Normal, '+', '-');
        let plain = render(&mode_info, None);
        for hovered in [
            resize(Resize::Increase, None),
            to_mode(InputMode::Pane),
            vec![Action::ToggleFloatingPanes],
        ] {
            let hovered_line = render(&mode_info, Some(hovered.clone()));
            assert_ne!(plain.part, hovered_line.part, "{:?}", hovered);
            assert_eq!(plain.len, hovered_line.len);
            assert_eq!(plain.clicks, hovered_line.clicks);
        }
        let unrelated = render(&mode_info, Some(vec![Action::Detach]));
        assert_eq!(plain.part, unrelated.part);
    }

    #[test]
    fn hovering_in_resize_mode_highlights_the_hovered_arrow() {
        let mode_info = mode_info(InputMode::Resize, '+', '-');
        let plain = render(&mode_info, None);
        let hovered = render(
            &mode_info,
            Some(resize(Resize::Increase, Some(Direction::Up))),
        );
        assert_ne!(plain.part, hovered.part);
        assert_eq!(plain.clicks, hovered.clicks);
    }

    #[test]
    fn hovering_highlights_only_the_hovered_arrow_and_the_ribbon() {
        let mode_info = mode_info(InputMode::Resize, '+', '-');
        let arrows = vec![
            key(BareKey::Left),
            key(BareKey::Down),
            key(BareKey::Up),
            key(BareKey::Right),
        ];
        let (plain_keys, _) = style_key_with_modifier(&arrows, Some(2), false, None);
        let (up_highlighted, _) = style_key_with_modifier(&arrows, Some(2), false, Some(2));
        let plain = render(&mode_info, None);
        let hovered = render(
            &mode_info,
            Some(resize(Resize::Increase, Some(Direction::Up))),
        );
        assert!(plain.part.contains(&plain_keys.part));
        assert!(!hovered.part.contains(&plain_keys.part));
        assert!(hovered.part.contains(&up_highlighted.part));
        assert!(plain
            .part
            .contains(&serialize_ribbon(&Text::from("Increase to"))));
        assert!(!hovered
            .part
            .contains(&serialize_ribbon(&Text::from("Increase to"))));
    }

    #[test]
    fn hovering_a_single_key_hint_leaves_its_key_text_alone() {
        let mode_info = mode_info(InputMode::Resize, '+', '-');
        let enter_like = vec![ctrl('n')];
        let (plain_key, _) = style_key_with_modifier(&enter_like, Some(2), false, None);
        let hovered = render(&mode_info, Some(to_mode(InputMode::Normal)));
        assert!(hovered.part.contains(&plain_key.part));
    }

    #[test]
    fn padding_before_the_secondary_hints_moves_their_regions() {
        let mode_info = mode_info(InputMode::Normal, '+', '-');
        let tab = tab_with_two_panes();
        let narrow = one_line_ui(&mode_info, Some(&tab), 200, "", false, None, false);
        let wide = one_line_ui(&mode_info, Some(&tab), 260, "", false, None, false);
        let floating = vec![Action::ToggleFloatingPanes];
        let narrow_start = region_for(&narrow, &floating).unwrap().start;
        let wide_start = region_for(&wide, &floating).unwrap().start;
        assert_eq!(wide_start, narrow_start + 60);
        let pane = to_mode(InputMode::Pane);
        assert_eq!(
            region_for(&narrow, &pane).unwrap().start,
            region_for(&wide, &pane).unwrap().start
        );
    }
}

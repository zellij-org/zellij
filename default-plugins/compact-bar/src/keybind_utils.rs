use crate::action_types::ActionType;
use std::collections::HashSet;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

pub struct KeybindProcessor;

impl KeybindProcessor {
    /// Find predetermined actions based on predicates while maintaining order
    pub fn find_predetermined_actions<F>(
        mode_info: &ModeInfo,
        mode: InputMode,
        predicates: Vec<F>,
    ) -> Vec<(String, String)>
    where
        F: Fn(&Action) -> bool,
    {
        let mut result = Vec::new();
        let keybinds = mode_info.get_keybinds_for_mode(mode);
        let mut processed_action_types = HashSet::new();

        // Iterate through predicates in order to maintain the desired sequence
        for predicate in predicates {
            // Find the first matching action for this predicate
            let mut found_match = false;
            for (_key, actions) in &keybinds {
                if let Some(first_action) = actions.first() {
                    if predicate(first_action) {
                        let action_type = ActionType::from_action(first_action);

                        // Skip if we've already processed this action type
                        if processed_action_types.contains(&action_type) {
                            found_match = true;
                            break;
                        }

                        let mut matching_keys = Vec::new();

                        // Find all keys that match this action type (including different directions)
                        for (inner_key, inner_actions) in &keybinds {
                            if let Some(inner_first_action) = inner_actions.first() {
                                if ActionType::from_action(inner_first_action) == action_type {
                                    matching_keys.push(format!("{}", inner_key));
                                }
                            }
                        }

                        if !matching_keys.is_empty() {
                            let description = action_type.description();
                            let should_add_brackets_to_keys = mode != InputMode::Normal;

                            // Check if this is switching to normal mode
                            // let is_switching_to_locked = matches!(first_action, Action::SwitchToMode(InputMode::Normal));
                            let is_switching_to_locked =
                                matches!(first_action, Action::SwitchToMode(InputMode::Locked));

                            let grouped_keys = Self::group_key_sets(
                                &matching_keys,
                                should_add_brackets_to_keys,
                                is_switching_to_locked,
                            );
                            result.push((grouped_keys, description));
                            processed_action_types.insert(action_type);
                        }

                        found_match = true;
                        break;
                    }
                }
            }

            // If we found a match for this predicate, we've processed it
            if found_match {
                continue;
            }
        }

        result
    }

    /// Group keys into sets and separate different key types with '|'
    fn group_key_sets(
        keys: &[String],
        should_add_brackets_to_keys: bool,
        is_switching_to_locked: bool,
    ) -> String {
        if keys.is_empty() {
            return String::new();
        }

        // Filter out Esc and Enter keys when switching to normal mode, but only if other keys exist
        let filtered_keys: Vec<String> = if is_switching_to_locked {
            let non_esc_enter_keys: Vec<String> = keys
                .iter()
                .filter(|k| k.as_str() != "ESC" && k.as_str() != "ENTER")
                .cloned()
                .collect();

            if non_esc_enter_keys.is_empty() {
                // If no other keys exist, keep the original keys
                keys.to_vec()
            } else {
                // Use filtered keys (without Esc/Enter)
                non_esc_enter_keys
            }
        } else {
            keys.to_vec()
        };

        if filtered_keys.len() == 1 {
            return if should_add_brackets_to_keys {
                format!("<{}>", filtered_keys[0])
            } else {
                filtered_keys[0].clone()
            };
        }

        // Group keys by type
        let mut arrow_keys = Vec::new();
        let mut hjkl_lower = Vec::new();
        let mut hjkl_upper = Vec::new();
        let mut square_bracket_keys = Vec::new();
        let mut plus_minus_keys = Vec::new();
        let mut pgup_pgdown = Vec::new();
        let mut other_keys = Vec::new();

        for key in &filtered_keys {
            match key.as_str() {
                "Left" | "←" => arrow_keys.push("←"),
                "Down" | "↓" => arrow_keys.push("↓"),
                "Up" | "↑" => arrow_keys.push("↑"),
                "Right" | "→" => arrow_keys.push("→"),
                "h" => hjkl_lower.push("h"),
                "j" => hjkl_lower.push("j"),
                "k" => hjkl_lower.push("k"),
                "l" => hjkl_lower.push("l"),
                "H" => hjkl_upper.push("H"),
                "J" => hjkl_upper.push("J"),
                "K" => hjkl_upper.push("K"),
                "L" => hjkl_upper.push("L"),
                "[" => square_bracket_keys.push("["),
                "]" => square_bracket_keys.push("]"),
                "+" => plus_minus_keys.push("+"),
                "-" => plus_minus_keys.push("-"),
                "=" => plus_minus_keys.push("="),
                "PgUp" => pgup_pgdown.push("PgUp"),
                "PgDn" => pgup_pgdown.push("PgDn"),
                _ => {
                    if should_add_brackets_to_keys {
                        other_keys.push(format!("<{}>", key));
                    } else {
                        other_keys.push(key.clone());
                    }
                },
            }
        }

        let mut groups = Vec::new();

        // Add hjkl group if present (prioritize hjkl over arrows)
        if !hjkl_lower.is_empty() {
            Self::sort_hjkl(&mut hjkl_lower);
            groups.push(Self::format_key_group(
                &hjkl_lower,
                should_add_brackets_to_keys,
                false,
            ));
        }

        // Add HJKL group if present
        if !hjkl_upper.is_empty() {
            Self::sort_hjkl_upper(&mut hjkl_upper);
            groups.push(Self::format_key_group(
                &hjkl_upper,
                should_add_brackets_to_keys,
                false,
            ));
        }

        // Add arrow keys group if present
        if !arrow_keys.is_empty() {
            Self::sort_arrows(&mut arrow_keys);
            groups.push(Self::format_key_group(
                &arrow_keys,
                should_add_brackets_to_keys,
                false,
            ));
        }

        if !square_bracket_keys.is_empty() {
            Self::sort_square_brackets(&mut square_bracket_keys);
            groups.push(Self::format_key_group(
                &square_bracket_keys,
                should_add_brackets_to_keys,
                false,
            ));
        }

        if !plus_minus_keys.is_empty() {
            Self::sort_plus_minus(&mut plus_minus_keys);
            groups.push(Self::format_key_group(
                &plus_minus_keys,
                should_add_brackets_to_keys,
                false,
            ));
        }

        if !pgup_pgdown.is_empty() {
            Self::sort_pgup_pgdown(&mut pgup_pgdown);
            groups.push(Self::format_key_group(
                &pgup_pgdown,
                should_add_brackets_to_keys,
                true,
            ));
        }

        // Add other keys with / separator
        if !other_keys.is_empty() {
            groups.push(other_keys.join("/"));
        }

        groups.join("/")
    }

    fn sort_hjkl(keys: &mut Vec<&str>) {
        keys.sort_by(|a, b| {
            let order = ["h", "j", "k", "l"];
            let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
            let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
            pos_a.cmp(&pos_b)
        });
    }

    fn sort_hjkl_upper(keys: &mut Vec<&str>) {
        keys.sort_by(|a, b| {
            let order = ["H", "J", "K", "L"];
            let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
            let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
            pos_a.cmp(&pos_b)
        });
    }

    fn sort_arrows(keys: &mut Vec<&str>) {
        keys.sort();
        keys.dedup();
        keys.sort_by(|a, b| {
            let order = ["←", "↓", "↑", "→"];
            let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
            let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
            pos_a.cmp(&pos_b)
        });
    }

    fn sort_square_brackets(keys: &mut Vec<&str>) {
        keys.sort_by(|a, b| {
            let order = ["[", "]"];
            let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
            let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
            pos_a.cmp(&pos_b)
        });
    }

    fn sort_plus_minus(keys: &mut Vec<&str>) {
        keys.sort_by(|a, b| {
            let order = ["+", "-"];
            let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
            let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
            pos_a.cmp(&pos_b)
        });
        // Remove "=" if both "+" and "=" are present
        if keys.contains(&"+") && keys.contains(&"=") {
            keys.retain(|k| k != &"=");
        }
    }

    fn sort_pgup_pgdown(keys: &mut Vec<&str>) {
        keys.sort_by(|a, b| {
            let order = ["PgUp", "PgDn"];
            let pos_a = order.iter().position(|&x| &x == a).unwrap_or(usize::MAX);
            let pos_b = order.iter().position(|&x| &x == b).unwrap_or(usize::MAX);
            pos_a.cmp(&pos_b)
        });
    }

    fn format_key_group(
        keys: &[&str],
        should_add_brackets: bool,
        use_pipe_separator: bool,
    ) -> String {
        let separator = if use_pipe_separator { "|" } else { "" };
        let joined = keys.join(separator);

        if should_add_brackets {
            format!("<{}>", joined)
        } else {
            joined
        }
    }

    /// Get predetermined actions for a specific mode
    pub fn get_predetermined_actions(
        mode_info: &ModeInfo,
        mode: InputMode,
    ) -> Vec<(String, String)> {
        match mode {
            InputMode::Locked => {
                let ordered_predicates = vec![|action: &Action| {
                    matches!(action, Action::SwitchToMode(InputMode::Normal))
                }];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Normal => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Locked)),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Pane)),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Tab)),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Resize)),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Move)),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Scroll)),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::Session)),
                    |action: &Action| matches!(action, Action::Quit),
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Pane => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::NewPane(None, None, false)),
                    |action: &Action| matches!(action, Action::MoveFocus(Direction::Left)),
                    |action: &Action| matches!(action, Action::MoveFocus(Direction::Down)),
                    |action: &Action| matches!(action, Action::MoveFocus(Direction::Up)),
                    |action: &Action| matches!(action, Action::MoveFocus(Direction::Right)),
                    |action: &Action| matches!(action, Action::CloseFocus),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::RenamePane)),
                    |action: &Action| matches!(action, Action::ToggleFocusFullscreen),
                    |action: &Action| matches!(action, Action::ToggleFloatingPanes),
                    |action: &Action| matches!(action, Action::TogglePaneEmbedOrFloating),
                    |action: &Action| {
                        matches!(action, Action::NewPane(Some(Direction::Right), None, false))
                    },
                    |action: &Action| {
                        matches!(action, Action::NewPane(Some(Direction::Down), None, false))
                    },
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Tab => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::GoToPreviousTab),
                    |action: &Action| matches!(action, Action::GoToNextTab),
                    |action: &Action| {
                        matches!(action, Action::NewTab(None, _, None, None, None, true))
                    },
                    |action: &Action| matches!(action, Action::CloseTab),
                    |action: &Action| matches!(action, Action::SwitchToMode(InputMode::RenameTab)),
                    |action: &Action| matches!(action, Action::TabNameInput(_)),
                    |action: &Action| matches!(action, Action::ToggleActiveSyncTab),
                    |action: &Action| matches!(action, Action::BreakPane),
                    |action: &Action| matches!(action, Action::BreakPaneLeft),
                    |action: &Action| matches!(action, Action::BreakPaneRight),
                    |action: &Action| matches!(action, Action::ToggleTab),
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Resize => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::Resize(Resize::Increase, None)),
                    |action: &Action| matches!(action, Action::Resize(Resize::Decrease, None)),
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize(Resize::Increase, Some(Direction::Left))
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize(Resize::Increase, Some(Direction::Down))
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize(Resize::Increase, Some(Direction::Up))
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize(Resize::Increase, Some(Direction::Right))
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize(Resize::Decrease, Some(Direction::Left))
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize(Resize::Decrease, Some(Direction::Down))
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize(Resize::Decrease, Some(Direction::Up))
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize(Resize::Decrease, Some(Direction::Right))
                        )
                    },
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Move => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::MovePane(Some(Direction::Left))),
                    |action: &Action| matches!(action, Action::MovePane(Some(Direction::Down))),
                    |action: &Action| matches!(action, Action::MovePane(Some(Direction::Up))),
                    |action: &Action| matches!(action, Action::MovePane(Some(Direction::Right))),
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Scroll => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::ScrollDown),
                    |action: &Action| matches!(action, Action::ScrollUp),
                    |action: &Action| matches!(action, Action::HalfPageScrollDown),
                    |action: &Action| matches!(action, Action::HalfPageScrollUp),
                    |action: &Action| matches!(action, Action::PageScrollDown),
                    |action: &Action| matches!(action, Action::PageScrollUp),
                    |action: &Action| {
                        matches!(action, Action::SwitchToMode(InputMode::EnterSearch))
                    },
                    |action: &Action| matches!(action, Action::EditScrollback),
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Search => {
                let ordered_predicates = vec![
                    |action: &Action| {
                        matches!(action, Action::SwitchToMode(InputMode::EnterSearch))
                    },
                    |action: &Action| matches!(action, Action::SearchInput(_)),
                    |action: &Action| matches!(action, Action::ScrollDown),
                    |action: &Action| matches!(action, Action::ScrollUp),
                    |action: &Action| matches!(action, Action::PageScrollDown),
                    |action: &Action| matches!(action, Action::PageScrollUp),
                    |action: &Action| matches!(action, Action::HalfPageScrollDown),
                    |action: &Action| matches!(action, Action::HalfPageScrollUp),
                    |action: &Action| {
                        matches!(action, Action::Search(actions::SearchDirection::Down))
                    },
                    |action: &Action| {
                        matches!(action, Action::Search(actions::SearchDirection::Up))
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SearchToggleOption(actions::SearchOption::CaseSensitivity)
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SearchToggleOption(actions::SearchOption::Wrap)
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SearchToggleOption(actions::SearchOption::WholeWord)
                        )
                    },
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Session => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::Detach),
                    |action: &Action| action.launches_plugin("session-manager"),
                    |action: &Action| action.launches_plugin("plugin-manager"),
                    |action: &Action| action.launches_plugin("configuration"),
                    |action: &Action| action.launches_plugin("zellij:about"),
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::EnterSearch
            | InputMode::RenameTab
            | InputMode::RenamePane
            | InputMode::Prompt
            | InputMode::Tmux => Vec::new(),
        }
    }
}

use crate::compact_bar::action_types::ActionType;
use std::collections::HashSet;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct TooltipKeyChunk {
    pub text: String,
    pub actions: Option<Vec<Action>>,
}

impl TooltipKeyChunk {
    fn plain(text: &str) -> Self {
        TooltipKeyChunk {
            text: text.to_owned(),
            actions: None,
        }
    }
    fn key(text: String, actions: Vec<Action>) -> Self {
        TooltipKeyChunk {
            text,
            actions: Some(actions),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TooltipEntry {
    pub chunks: Vec<TooltipKeyChunk>,
    pub description: String,
}

impl TooltipEntry {
    pub fn key_text(&self) -> String {
        self.chunks.iter().map(|c| c.text.as_str()).collect()
    }

    pub fn description_actions(&self) -> Option<Vec<Action>> {
        let mut actions = self.chunks.iter().filter_map(|c| c.actions.as_ref());
        let first = actions.next()?;
        if actions.all(|other| other == first) {
            Some(first.clone())
        } else {
            None
        }
    }

    pub fn chunk_ranges(&self) -> Vec<(usize, usize, &TooltipKeyChunk)> {
        let mut ranges = vec![];
        let mut start = 0;
        for chunk in &self.chunks {
            let end = start + chunk.text.chars().count();
            ranges.push((start, end, chunk));
            start = end;
        }
        ranges
    }
}

pub struct KeybindProcessor;

impl KeybindProcessor {
    pub fn find_predetermined_actions<F>(
        mode_info: &ModeInfo,
        mode: InputMode,
        predicates: Vec<F>,
    ) -> Vec<TooltipEntry>
    where
        F: Fn(&Action) -> bool,
    {
        let mut result = Vec::new();
        let keybinds = mode_info.get_keybinds_for_mode(mode);
        let mut processed_action_types = HashSet::new();

        for predicate in predicates {
            for (_key, actions) in &keybinds {
                if let Some(first_action) = actions.first() {
                    if predicate(first_action) {
                        let action_type = ActionType::from_action(first_action);

                        if processed_action_types.contains(&action_type) {
                            break;
                        }

                        let mut matching_keys = Vec::new();

                        for (inner_key, inner_actions) in &keybinds {
                            if let Some(inner_first_action) = inner_actions.first() {
                                if ActionType::from_action(inner_first_action) == action_type {
                                    matching_keys
                                        .push((format!("{}", inner_key), inner_actions.clone()));
                                }
                            }
                        }

                        if !matching_keys.is_empty() {
                            let description = action_type.description();
                            let should_add_brackets_to_keys = mode != InputMode::Normal;

                            let is_switching_to_locked = matches!(
                                first_action,
                                Action::SwitchToMode {
                                    input_mode: InputMode::Locked
                                }
                            );

                            let chunks = Self::group_key_chunks(
                                &matching_keys,
                                should_add_brackets_to_keys,
                                is_switching_to_locked,
                            );
                            result.push(TooltipEntry {
                                chunks,
                                description,
                            });
                            processed_action_types.insert(action_type);
                        }

                        break;
                    }
                }
            }
        }

        result
    }

    pub fn group_key_chunks(
        keys: &[(String, Vec<Action>)],
        should_add_brackets_to_keys: bool,
        is_switching_to_locked: bool,
    ) -> Vec<TooltipKeyChunk> {
        if keys.is_empty() {
            return Vec::new();
        }

        let filtered_keys: Vec<(String, Vec<Action>)> = if is_switching_to_locked {
            let non_esc_enter_keys: Vec<(String, Vec<Action>)> = keys
                .iter()
                .filter(|(k, _)| k.as_str() != "ESC" && k.as_str() != "ENTER")
                .cloned()
                .collect();

            if non_esc_enter_keys.is_empty() {
                keys.to_vec()
            } else {
                non_esc_enter_keys
            }
        } else {
            keys.to_vec()
        };

        let bracketed = |text: &str| -> String {
            if should_add_brackets_to_keys {
                format!("<{}>", text)
            } else {
                text.to_owned()
            }
        };

        if filtered_keys.len() == 1 {
            let (key, actions) = &filtered_keys[0];
            return vec![TooltipKeyChunk::key(bracketed(key), actions.clone())];
        }

        let mut arrow_keys: Vec<(&'static str, Vec<Action>)> = Vec::new();
        let mut hjkl_lower: Vec<(&'static str, Vec<Action>)> = Vec::new();
        let mut hjkl_upper: Vec<(&'static str, Vec<Action>)> = Vec::new();
        let mut square_bracket_keys: Vec<(&'static str, Vec<Action>)> = Vec::new();
        let mut plus_minus_keys: Vec<(&'static str, Vec<Action>)> = Vec::new();
        let mut pgup_pgdown: Vec<(&'static str, Vec<Action>)> = Vec::new();
        let mut other_keys: Vec<TooltipKeyChunk> = Vec::new();

        for (key, actions) in &filtered_keys {
            let actions = actions.clone();
            match key.as_str() {
                "Left" | "←" => arrow_keys.push(("←", actions)),
                "Down" | "↓" => arrow_keys.push(("↓", actions)),
                "Up" | "↑" => arrow_keys.push(("↑", actions)),
                "Right" | "→" => arrow_keys.push(("→", actions)),
                "h" => hjkl_lower.push(("h", actions)),
                "j" => hjkl_lower.push(("j", actions)),
                "k" => hjkl_lower.push(("k", actions)),
                "l" => hjkl_lower.push(("l", actions)),
                "H" => hjkl_upper.push(("H", actions)),
                "J" => hjkl_upper.push(("J", actions)),
                "K" => hjkl_upper.push(("K", actions)),
                "L" => hjkl_upper.push(("L", actions)),
                "[" => square_bracket_keys.push(("[", actions)),
                "]" => square_bracket_keys.push(("]", actions)),
                "+" => plus_minus_keys.push(("+", actions)),
                "-" => plus_minus_keys.push(("-", actions)),
                "=" => plus_minus_keys.push(("=", actions)),
                "PgUp" => pgup_pgdown.push(("PgUp", actions)),
                "PgDn" => pgup_pgdown.push(("PgDn", actions)),
                _ => other_keys.push(TooltipKeyChunk::key(bracketed(key), actions)),
            }
        }

        let mut groups: Vec<Vec<TooltipKeyChunk>> = Vec::new();
        let mut add_group =
            |mut keys: Vec<(&'static str, Vec<Action>)>, order: &[&str], separator: &str| {
                if keys.is_empty() {
                    return;
                }
                Self::sort_by_order(&mut keys, order);
                if order.contains(&"+")
                    && keys.iter().any(|(k, _)| *k == "+")
                    && keys.iter().any(|(k, _)| *k == "=")
                {
                    keys.retain(|(k, _)| *k != "=");
                }
                let mut chunks = Vec::new();
                if should_add_brackets_to_keys {
                    chunks.push(TooltipKeyChunk::plain("<"));
                }
                for (index, (key, actions)) in keys.into_iter().enumerate() {
                    if index > 0 && !separator.is_empty() {
                        chunks.push(TooltipKeyChunk::plain(separator));
                    }
                    chunks.push(TooltipKeyChunk::key(key.to_owned(), actions));
                }
                if should_add_brackets_to_keys {
                    chunks.push(TooltipKeyChunk::plain(">"));
                }
                groups.push(chunks);
            };

        add_group(hjkl_lower, &["h", "j", "k", "l"], "");
        add_group(hjkl_upper, &["H", "J", "K", "L"], "");
        add_group(arrow_keys, &["←", "↓", "↑", "→"], "");
        add_group(square_bracket_keys, &["[", "]"], "");
        add_group(plus_minus_keys, &["+", "-"], "");
        add_group(pgup_pgdown, &["PgUp", "PgDn"], "|");

        if !other_keys.is_empty() {
            let mut chunks = Vec::new();
            for (index, chunk) in other_keys.into_iter().enumerate() {
                if index > 0 {
                    chunks.push(TooltipKeyChunk::plain("/"));
                }
                chunks.push(chunk);
            }
            groups.push(chunks);
        }

        let mut result = Vec::new();
        for (index, group) in groups.into_iter().enumerate() {
            if index > 0 {
                result.push(TooltipKeyChunk::plain("/"));
            }
            result.extend(group);
        }
        result
    }

    fn sort_by_order(keys: &mut Vec<(&'static str, Vec<Action>)>, order: &[&str]) {
        let mut seen = HashSet::new();
        keys.retain(|(key, _)| seen.insert(*key));
        keys.sort_by_key(|(key, _)| {
            order
                .iter()
                .position(|candidate| candidate == key)
                .unwrap_or(usize::MAX)
        });
    }

    pub fn get_tooltip_entries(mode_info: &ModeInfo, mode: InputMode) -> Vec<TooltipEntry> {
        match mode {
            InputMode::Locked => {
                let ordered_predicates = vec![|action: &Action| {
                    matches!(
                        action,
                        Action::SwitchToMode {
                            input_mode: InputMode::Normal
                        }
                    )
                }];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Normal => {
                let ordered_predicates = vec![
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SwitchToMode {
                                input_mode: InputMode::Locked
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SwitchToMode {
                                input_mode: InputMode::Pane
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SwitchToMode {
                                input_mode: InputMode::Tab
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SwitchToMode {
                                input_mode: InputMode::Resize
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SwitchToMode {
                                input_mode: InputMode::Move
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SwitchToMode {
                                input_mode: InputMode::Scroll
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SwitchToMode {
                                input_mode: InputMode::Session
                            }
                        )
                    },
                    |action: &Action| matches!(action, Action::Quit),
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Pane => {
                let ordered_predicates = vec![
                    |action: &Action| {
                        matches!(
                            action,
                            Action::NewPane {
                                direction: None,
                                pane_name: None,
                                start_suppressed: false
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::MoveFocus {
                                direction: Direction::Left
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::MoveFocus {
                                direction: Direction::Down
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::MoveFocus {
                                direction: Direction::Up
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::MoveFocus {
                                direction: Direction::Right
                            }
                        )
                    },
                    |action: &Action| matches!(action, Action::CloseFocus),
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SwitchToMode {
                                input_mode: InputMode::RenamePane
                            }
                        )
                    },
                    |action: &Action| matches!(action, Action::ToggleFocusFullscreen),
                    |action: &Action| matches!(action, Action::ToggleFloatingPanes),
                    |action: &Action| matches!(action, Action::TogglePaneEmbedOrFloating),
                    |action: &Action| {
                        matches!(
                            action,
                            Action::NewStackedPane {
                                command: None,
                                pane_name: None,
                                near_current_pane: false,
                                ..
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::NewPane {
                                direction: Some(Direction::Right),
                                pane_name: None,
                                start_suppressed: false
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::NewPane {
                                direction: Some(Direction::Down),
                                pane_name: None,
                                start_suppressed: false
                            }
                        )
                    },
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Tab => {
                let ordered_predicates = vec![
                    |action: &Action| matches!(action, Action::GoToPreviousTab),
                    |action: &Action| matches!(action, Action::GoToNextTab),
                    |action: &Action| {
                        matches!(
                            action,
                            Action::NewTab {
                                tiled_layout: None,
                                floating_layouts: _,
                                swap_tiled_layouts: None,
                                swap_floating_layouts: None,
                                tab_name: None,
                                should_change_focus_to_new_tab: true,
                                cwd: None,
                                initial_panes: _,
                                first_pane_unblock_condition: _,
                            }
                        )
                    },
                    |action: &Action| matches!(action, Action::CloseTab),
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SwitchToMode {
                                input_mode: InputMode::RenameTab
                            }
                        )
                    },
                    |action: &Action| matches!(action, Action::TabNameInput { .. }),
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
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize {
                                resize: Resize::Increase,
                                direction: None
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize {
                                resize: Resize::Decrease,
                                direction: None
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize {
                                resize: Resize::Increase,
                                direction: Some(Direction::Left)
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize {
                                resize: Resize::Increase,
                                direction: Some(Direction::Down)
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize {
                                resize: Resize::Increase,
                                direction: Some(Direction::Up)
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize {
                                resize: Resize::Increase,
                                direction: Some(Direction::Right)
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize {
                                resize: Resize::Decrease,
                                direction: Some(Direction::Left)
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize {
                                resize: Resize::Decrease,
                                direction: Some(Direction::Down)
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize {
                                resize: Resize::Decrease,
                                direction: Some(Direction::Up)
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Resize {
                                resize: Resize::Decrease,
                                direction: Some(Direction::Right)
                            }
                        )
                    },
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Move => {
                let ordered_predicates = vec![
                    |action: &Action| {
                        matches!(
                            action,
                            Action::MovePane {
                                direction: Some(Direction::Left)
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::MovePane {
                                direction: Some(Direction::Down)
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::MovePane {
                                direction: Some(Direction::Up)
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::MovePane {
                                direction: Some(Direction::Right)
                            }
                        )
                    },
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
                        matches!(
                            action,
                            Action::SwitchToMode {
                                input_mode: InputMode::EnterSearch
                            }
                        )
                    },
                    |action: &Action| matches!(action, Action::EditScrollback { .. }),
                ];
                Self::find_predetermined_actions(mode_info, mode, ordered_predicates)
            },
            InputMode::Search => {
                let ordered_predicates = vec![
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SwitchToMode {
                                input_mode: InputMode::EnterSearch
                            }
                        )
                    },
                    |action: &Action| matches!(action, Action::SearchInput { .. }),
                    |action: &Action| matches!(action, Action::ScrollDown),
                    |action: &Action| matches!(action, Action::ScrollUp),
                    |action: &Action| matches!(action, Action::PageScrollDown),
                    |action: &Action| matches!(action, Action::PageScrollUp),
                    |action: &Action| matches!(action, Action::HalfPageScrollDown),
                    |action: &Action| matches!(action, Action::HalfPageScrollUp),
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Search {
                                direction: actions::SearchDirection::Down
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::Search {
                                direction: actions::SearchDirection::Up
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SearchToggleOption {
                                option: actions::SearchOption::CaseSensitivity
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SearchToggleOption {
                                option: actions::SearchOption::Wrap
                            }
                        )
                    },
                    |action: &Action| {
                        matches!(
                            action,
                            Action::SearchToggleOption {
                                option: actions::SearchOption::WholeWord
                            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn focus(direction: Direction) -> Vec<Action> {
        vec![Action::MoveFocus { direction }]
    }

    fn texts(chunks: &[TooltipKeyChunk]) -> Vec<&str> {
        chunks.iter().map(|c| c.text.as_str()).collect()
    }

    #[test]
    fn arrows_are_grouped_but_each_keeps_its_own_action() {
        let keys = vec![
            ("Right".to_owned(), focus(Direction::Right)),
            ("Left".to_owned(), focus(Direction::Left)),
            ("Up".to_owned(), focus(Direction::Up)),
            ("Down".to_owned(), focus(Direction::Down)),
            ("h".to_owned(), focus(Direction::Left)),
        ];
        let chunks = KeybindProcessor::group_key_chunks(&keys, true, false);
        assert_eq!(
            texts(&chunks),
            vec!["<", "h", ">", "/", "<", "←", "↓", "↑", "→", ">"]
        );
        assert_eq!(chunks[5].actions, Some(focus(Direction::Left)));
        assert_eq!(chunks[8].actions, Some(focus(Direction::Right)));
        assert_eq!(chunks[0].actions, None);
        let entry = TooltipEntry {
            chunks,
            description: "Move focus".to_owned(),
        };
        assert_eq!(entry.key_text(), "<h>/<←↓↑→>");
        assert_eq!(entry.description_actions(), None);
    }

    #[test]
    fn plus_minus_drop_equals_and_keep_separate_actions() {
        let increase = vec![Action::Resize {
            resize: Resize::Increase,
            direction: None,
        }];
        let decrease = vec![Action::Resize {
            resize: Resize::Decrease,
            direction: None,
        }];
        let keys = vec![
            ("=".to_owned(), increase.clone()),
            ("-".to_owned(), decrease.clone()),
            ("+".to_owned(), increase.clone()),
        ];
        let chunks = KeybindProcessor::group_key_chunks(&keys, true, false);
        assert_eq!(texts(&chunks), vec!["<", "+", "-", ">"]);
        assert_eq!(chunks[1].actions, Some(increase));
        assert_eq!(chunks[2].actions, Some(decrease));
    }

    #[test]
    fn a_single_key_is_one_clickable_chunk_and_its_description_is_clickable_too() {
        let lock = vec![Action::SwitchToMode {
            input_mode: InputMode::Locked,
        }];
        let keys = vec![
            ("Ctrl g".to_owned(), lock.clone()),
            ("ESC".to_owned(), lock.clone()),
        ];
        let chunks = KeybindProcessor::group_key_chunks(&keys, false, true);
        assert_eq!(texts(&chunks), vec!["Ctrl g"]);
        let entry = TooltipEntry {
            chunks,
            description: "Lock".to_owned(),
        };
        assert_eq!(entry.description_actions(), Some(lock));
        assert_eq!(entry.chunk_ranges()[0].0, 0);
        assert_eq!(entry.chunk_ranges()[0].1, 6);
    }

    #[test]
    fn page_keys_and_other_keys_use_their_separators() {
        let keys = vec![
            ("PgDn".to_owned(), vec![Action::PageScrollDown]),
            ("PgUp".to_owned(), vec![Action::PageScrollUp]),
            ("x".to_owned(), vec![Action::PageScrollDown]),
        ];
        let chunks = KeybindProcessor::group_key_chunks(&keys, true, false);
        let entry = TooltipEntry {
            chunks,
            description: "Scroll page".to_owned(),
        };
        assert_eq!(entry.key_text(), "<PgUp|PgDn>/<x>");
    }
}

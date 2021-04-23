use super::actions::{Action, Direction};
use super::data::{InputMode, Key};
use std::collections::HashMap;

// const fn now does not support PartialEq/Eq, we have to implement our own compare fn
const fn compare_key(l: &Key, r: &Key) -> bool {
    matches!(
        (l, r),
        (Key::Backspace, Key::Backspace)
            | (Key::Left, Key::Left)
            | (Key::Right, Key::Right)
            | (Key::Up, Key::Up)
            | (Key::Down, Key::Down)
            | (Key::Home, Key::Home)
            | (Key::End, Key::End)
            | (Key::PageUp, Key::PageUp)
            | (Key::PageDown, Key::PageDown)
            | (Key::Delete, Key::Delete)
            | (Key::Insert, Key::Insert)
            | (Key::Esc, Key::Esc)
            | (Key::BackTab, Key::BackTab)
    )
}

const fn get_key_order(key: &Key) -> Option<i32> {
    const V: &[(Key, i32)] = &[
        (Key::Left, 0),
        (Key::Right, 0),
        (Key::Up, 1),
        (Key::Down, 1),
        (Key::PageUp, 2),
        (Key::PageDown, 2),
    ];
    let mut i = 0;
    while i < V.len() {
        let (k, o) = V[i];
        if compare_key(&k, key) {
            return Some(o);
        }
        i += 1;
    }
    None
}

/// Get a prior key from keybinds
/// many keys may be mapped to one action, e.g. kj/↑↓
/// but we do not want to show all of them in help info,
/// so just pickup one primary key.
fn get_major_key_by_action(keybinds: &HashMap<Key, Vec<Action>>, action: &[Action]) -> Key {
    let mut key = Key::Null;
    for (k, actions) in keybinds {
        if actions == action {
            if key == Key::Null {
                // old key is null
                key = *k;
            } else if let Some(new_order) = get_key_order(k) {
                if let Some(old_order) = get_key_order(&key) {
                    if new_order < old_order {
                        // old key has lower order (larger number) than new one
                        key = *k;
                    }
                } else {
                    // old key does not have order, new key have order
                    // then use new keybind
                    key = *k;
                }
            }
        }
    }
    key
}

fn get_key_map_string(key_config: &HashMap<Key, Vec<Action>>, actions: &[&[Action]]) -> String {
    let map = actions
        .iter()
        .map(|&actions| get_major_key_by_action(&key_config, actions))
        .map(|key| key.to_string())
        .collect::<Vec<_>>();
    let should_split = map.iter().any(|s| s.chars().count() > 1);
    map.into_iter().fold(String::new(), |s0, s| {
        if !s0.is_empty() && should_split {
            format!("{}/{}", s0, s)
        } else {
            format!("{}{}", s0, s)
        }
    })
}

pub fn get_mode_info(
    mode: InputMode,
    key_config: &HashMap<Key, Vec<Action>>,
) -> Vec<(String, String)> {
    let mut keybinds: Vec<(String, String)> = vec![];
    match mode {
        InputMode::Normal | InputMode::Locked => {}
        InputMode::Resize => {
            let key_map = get_key_map_string(
                &key_config,
                &[
                    &[Action::Resize(Direction::Left)],
                    &[Action::Resize(Direction::Down)],
                    &[Action::Resize(Direction::Up)],
                    &[Action::Resize(Direction::Right)],
                ],
            );
            keybinds.push((key_map, "Resize".to_string()));
        }
        InputMode::Pane => {
            let key_map = get_key_map_string(
                &key_config,
                &[
                    &[Action::MoveFocus(Direction::Left)],
                    &[Action::MoveFocus(Direction::Down)],
                    &[Action::MoveFocus(Direction::Up)],
                    &[Action::MoveFocus(Direction::Right)],
                ],
            );
            keybinds.push((key_map, "Move focus".to_string()));
            keybinds.push((
                get_major_key_by_action(&key_config, &[Action::SwitchFocus]).to_string(),
                "Next".to_string(),
            ));
            keybinds.push((
                get_major_key_by_action(&key_config, &[Action::NewPane(None)]).to_string(),
                "New".to_string(),
            ));
            keybinds.push((
                get_major_key_by_action(&key_config, &[Action::NewPane(Some(Direction::Down))])
                    .to_string(),
                "Down split".to_string(),
            ));
            keybinds.push((
                get_major_key_by_action(&key_config, &[Action::NewPane(Some(Direction::Right))])
                    .to_string(),
                "Right split".to_string(),
            ));
            keybinds.push((
                get_major_key_by_action(&key_config, &[Action::CloseFocus]).to_string(),
                "Close".to_string(),
            ));
            keybinds.push((
                get_major_key_by_action(&key_config, &[Action::ToggleFocusFullscreen]).to_string(),
                "Fullscreen".to_string(),
            ));
        }
        InputMode::Tab => {
            let key_map = get_key_map_string(
                &key_config,
                &[&[Action::GoToPreviousTab], &[Action::GoToNextTab]],
            );
            keybinds.push((key_map, "Move focus".to_string()));
            keybinds.push((
                get_major_key_by_action(&key_config, &[Action::NewTab]).to_string(),
                "New".to_string(),
            ));
            keybinds.push((
                get_major_key_by_action(&key_config, &[Action::CloseTab]).to_string(),
                "Close".to_string(),
            ));
            keybinds.push((
                get_major_key_by_action(
                    &key_config,
                    &[
                        Action::SwitchToMode(InputMode::RenameTab),
                        Action::TabNameInput(vec![0]),
                    ],
                )
                .to_string(),
                "Rename".to_string(),
            ));
        }
        InputMode::Scroll => {
            let key_map =
                get_key_map_string(&key_config, &[&[Action::ScrollUp], &[Action::ScrollDown]]);
            keybinds.push((key_map, "Scroll".to_string()));
            let key_map = get_key_map_string(
                &key_config,
                &[&[Action::PageScrollUp], &[Action::PageScrollDown]],
            );
            keybinds.push((key_map, "Scroll Page".to_string()));
        }
        InputMode::RenameTab => {
            keybinds.push(("Enter".to_string(), "when done".to_string()));
        }
    }
    keybinds
}

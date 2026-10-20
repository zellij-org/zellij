use std::cell::RefCell;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

pub const SWITCH_HINT_MS: u64 = 5000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionHints {
    pub previous: Option<String>,
    pub has_items: bool,
    pub transient: Option<String>,
    pub pane_note: Option<String>,
}

thread_local! {
    static SESSION_HINTS: RefCell<SessionHints> = RefCell::new(SessionHints::default());
}

pub fn with_session_hints<T>(hints: SessionHints, render: impl FnOnce() -> T) -> T {
    SESSION_HINTS.with(|h| *h.borrow_mut() = hints);
    let result = render();
    SESSION_HINTS.with(|h| *h.borrow_mut() = SessionHints::default());
    result
}

pub fn session_hints() -> SessionHints {
    SESSION_HINTS.with(|h| h.borrow().clone())
}

pub fn first_action_keys(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    action: &Action,
) -> Vec<KeyWithModifier> {
    keymap
        .iter()
        .filter(|(_, actions)| actions.first().map(|a| a == action).unwrap_or(false))
        .map(|(key, _)| key.clone())
        .collect()
}

const MAX_MODE_STEPS: usize = 3;

fn mode_switch_target(actions: &[Action]) -> Option<InputMode> {
    let mut target = None;
    for action in actions {
        match action {
            Action::SwitchToMode { input_mode } => target = Some(*input_mode),
            _ => return None,
        }
    }
    target
}

pub fn keys_to_action(mode_info: &ModeInfo, action: &Action) -> Option<Vec<KeyWithModifier>> {
    let mut visited = vec![mode_info.mode];
    let mut frontier: Vec<(InputMode, Vec<KeyWithModifier>)> = vec![(mode_info.mode, vec![])];
    for _ in 0..=MAX_MODE_STEPS {
        let mut next = vec![];
        for (mode, path) in &frontier {
            let keymap = mode_info.get_keybinds_for_mode(*mode);
            if let Some(key) = first_action_keys(&keymap, action).into_iter().next() {
                let mut keys = path.clone();
                keys.push(key);
                return Some(keys);
            }
            for (key, actions) in keymap {
                if let Some(target) = mode_switch_target(&actions) {
                    if !visited.contains(&target) {
                        visited.push(target);
                        let mut keys = path.clone();
                        keys.push(key);
                        next.push((target, keys));
                    }
                }
            }
        }
        if next.is_empty() {
            return None;
        }
        frontier = next;
    }
    None
}

pub fn back_keys_text(mode_info: &ModeInfo) -> Option<String> {
    let keys = keys_to_action(mode_info, &Action::ToggleSession)?;
    Some(
        keys.iter()
            .map(|key| key.to_string())
            .collect::<Vec<_>>()
            .join(", "),
    )
}

pub fn compute_session_hints(
    history: Option<&SessionHistoryInfo>,
    global: Option<&GlobalAttention>,
    focused_pane: Option<&PaneInfo>,
    back_keys: Option<&str>,
    now_ms: u64,
) -> SessionHints {
    let previous = history.and_then(|h| h.previous.clone());
    let pane_note = focused_pane.and_then(|pane| {
        if pane.attention.is_muted() {
            Some("pane muted".to_owned())
        } else if pane.attention.watched {
            Some("watching this pane".to_owned())
        } else {
            None
        }
    });
    let keys = back_keys.filter(|keys| !keys.is_empty());
    let transient = history.and_then(|history| {
        let from = history.arrived_from.as_ref()?;
        let recent = now_ms.saturating_sub(history.arrived_at_ms) < SWITCH_HINT_MS;
        if let Some(SessionArrival::Attention { pane_id }) = history.arrival.as_ref() {
            let cleared = global
                .map(|g| {
                    g.find(&g.current_session, pane_id)
                        .map(|item| !item.is_counted())
                        .unwrap_or(true)
                })
                .unwrap_or(false);
            let still_there = focused_pane
                .map(|pane| !pane.is_plugin && PaneId::Terminal(pane.id) == *pane_id)
                .unwrap_or(false);
            if cleared && still_there && !recent {
                return previous.as_ref().map(|previous| match keys {
                    Some(keys) => format!(
                        "Done · {} back to {}",
                        keys,
                        sanitize_display_text(previous)
                    ),
                    None => "Done".to_owned(),
                });
            }
        }
        if recent {
            let from = sanitize_display_text(from);
            Some(match keys {
                Some(keys) => format!("Switched from {} · {} back", from, keys),
                None => format!("Switched from {}", from),
            })
        } else {
            None
        }
    });
    SessionHints {
        previous,
        has_items: global
            .map(|g| !g.counted_items().is_empty())
            .unwrap_or(false),
        transient,
        pane_note,
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn history(arrival: SessionArrival, at: u64) -> SessionHistoryInfo {
        SessionHistoryInfo {
            current: "here".to_owned(),
            previous: Some("api".to_owned()),
            back: vec!["api".to_owned()],
            forward: vec![],
            arrived_from: Some("api".to_owned()),
            arrival: Some(arrival),
            arrived_at_ms: at,
        }
    }

    #[test]
    fn a_switch_is_announced_briefly() {
        let history = history(SessionArrival::Switch, 1_000);
        let hints = compute_session_hints(Some(&history), None, None, Some("Ctrl o, Tab"), 2_000);
        assert_eq!(
            hints.transient.as_deref(),
            Some("Switched from api · Ctrl o, Tab back")
        );
        assert_eq!(hints.previous.as_deref(), Some("api"));
        let later = compute_session_hints(Some(&history), None, None, Some("Ctrl o, Tab"), 9_000);
        assert_eq!(later.transient, None);
    }

    #[test]
    fn done_is_shown_once_the_item_jumped_to_clears() {
        let history = history(
            SessionArrival::Attention {
                pane_id: PaneId::Terminal(3),
            },
            1_000,
        );
        let global = GlobalAttention::build("here", vec![("here".to_owned(), Default::default())]);
        let pane = PaneInfo {
            id: 3,
            is_focused: true,
            ..Default::default()
        };
        let hints = compute_session_hints(
            Some(&history),
            Some(&global),
            Some(&pane),
            Some("Tab"),
            20_000,
        );
        assert_eq!(hints.transient.as_deref(), Some("Done · Tab back to api"));
    }

    fn key(text: &str) -> KeyWithModifier {
        KeyWithModifier::from_str(text).unwrap()
    }

    fn to_mode(mode: InputMode) -> Vec<Action> {
        vec![Action::SwitchToMode { input_mode: mode }]
    }

    fn mode_info(
        mode: InputMode,
        keybinds: Vec<(InputMode, Vec<(&str, Vec<Action>)>)>,
    ) -> ModeInfo {
        ModeInfo {
            mode,
            keybinds: keybinds
                .into_iter()
                .map(|(mode, binds)| {
                    (
                        mode,
                        binds
                            .into_iter()
                            .map(|(k, actions)| (key(k), actions))
                            .collect(),
                    )
                })
                .collect(),
            ..Default::default()
        }
    }

    fn session_mode() -> (InputMode, Vec<(&'static str, Vec<Action>)>) {
        (
            InputMode::Session,
            vec![(
                "Tab",
                vec![
                    Action::ToggleSession,
                    Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    },
                ],
            )],
        )
    }

    #[test]
    fn the_back_keys_go_through_session_mode_from_normal() {
        let info = mode_info(
            InputMode::Normal,
            vec![
                (
                    InputMode::Normal,
                    vec![("Ctrl o", to_mode(InputMode::Session))],
                ),
                session_mode(),
            ],
        );
        assert_eq!(back_keys_text(&info).as_deref(), Some("Ctrl o, TAB"));
    }

    #[test]
    fn a_locked_base_mode_lists_every_step() {
        let info = mode_info(
            InputMode::Locked,
            vec![
                (
                    InputMode::Locked,
                    vec![("Ctrl g", to_mode(InputMode::Normal))],
                ),
                (InputMode::Normal, vec![("o", to_mode(InputMode::Session))]),
                session_mode(),
            ],
        );
        assert_eq!(back_keys_text(&info).as_deref(), Some("Ctrl g, o, TAB"));
    }

    #[test]
    fn a_direct_binding_wins_over_the_way_through_session_mode() {
        let info = mode_info(
            InputMode::Normal,
            vec![
                (
                    InputMode::Normal,
                    vec![
                        ("Ctrl o", to_mode(InputMode::Session)),
                        ("Alt t", vec![Action::ToggleSession]),
                    ],
                ),
                session_mode(),
            ],
        );
        assert_eq!(back_keys_text(&info).as_deref(), Some("Alt t"));
    }

    #[test]
    fn in_session_mode_only_the_toggle_key_is_shown() {
        let info = mode_info(
            InputMode::Session,
            vec![
                (
                    InputMode::Normal,
                    vec![("Ctrl o", to_mode(InputMode::Session))],
                ),
                session_mode(),
            ],
        );
        assert_eq!(back_keys_text(&info).as_deref(), Some("TAB"));
    }

    #[test]
    fn keys_with_side_effects_are_not_used_as_steps() {
        let info = mode_info(
            InputMode::Normal,
            vec![
                (
                    InputMode::Normal,
                    vec![(
                        "w",
                        vec![
                            Action::Detach,
                            Action::SwitchToMode {
                                input_mode: InputMode::Session,
                            },
                        ],
                    )],
                ),
                session_mode(),
            ],
        );
        assert_eq!(back_keys_text(&info), None);
    }

    #[test]
    fn without_a_way_back_no_key_is_promised() {
        let info = mode_info(InputMode::Normal, vec![(InputMode::Normal, vec![])]);
        assert_eq!(back_keys_text(&info), None);
        let history = history(SessionArrival::Switch, 1_000);
        let hints = compute_session_hints(Some(&history), None, None, None, 2_000);
        assert_eq!(hints.transient.as_deref(), Some("Switched from api"));
    }

    #[test]
    fn muted_and_watched_panes_are_mentioned() {
        let mut pane = PaneInfo::default();
        pane.attention.muted = Some(MuteScope::Pane);
        let hints = compute_session_hints(None, None, Some(&pane), None, 0);
        assert_eq!(hints.pane_note.as_deref(), Some("pane muted"));
        let mut pane = PaneInfo::default();
        pane.attention.watched = true;
        let hints = compute_session_hints(None, None, Some(&pane), None, 0);
        assert_eq!(hints.pane_note.as_deref(), Some("watching this pane"));
    }
}

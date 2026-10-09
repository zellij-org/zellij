use zellij_utils::data::{BareKey, InputMode, KeyWithModifier};
use zellij_utils::input::actions::Action;
use zellij_utils::input::keybinds::Keybinds;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupScroll {
    Up(usize),
    Down(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopupActionRule {
    ToPopup,
    ScrollPopup(PopupScroll),
    ClosePopup,
    Underneath,
    Ignore,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PopupKeyAction {
    Route(Action),
    KeyToPopup {
        key: KeyWithModifier,
        raw_bytes: Vec<u8>,
        is_kitty_keyboard_protocol: bool,
    },
    ScrollPopup(PopupScroll),
    ClosePopup,
}

const POPUP_HALF_PAGE_LINES: usize = 5;
const POPUP_PAGE_LINES: usize = 10;
const POPUP_EDGE_LINES: usize = 1000;

fn is_text_entry_mode(input_mode: &InputMode) -> bool {
    matches!(
        input_mode,
        InputMode::RenamePane | InputMode::RenameTab | InputMode::EnterSearch
    )
}

pub(crate) fn popup_action_rule(action: &Action) -> PopupActionRule {
    use PopupActionRule::*;
    match action {
        Action::Write { .. } | Action::WriteChars { .. } => ToPopup,
        Action::ScrollUp | Action::ScrollUpAt { .. } => ScrollPopup(PopupScroll::Up(1)),
        Action::ScrollDown | Action::ScrollDownAt { .. } => ScrollPopup(PopupScroll::Down(1)),
        Action::HalfPageScrollUp => ScrollPopup(PopupScroll::Up(POPUP_HALF_PAGE_LINES)),
        Action::HalfPageScrollDown => ScrollPopup(PopupScroll::Down(POPUP_HALF_PAGE_LINES)),
        Action::PageScrollUp => ScrollPopup(PopupScroll::Up(POPUP_PAGE_LINES)),
        Action::PageScrollDown => ScrollPopup(PopupScroll::Down(POPUP_PAGE_LINES)),
        Action::ScrollToTop => ScrollPopup(PopupScroll::Up(POPUP_EDGE_LINES)),
        Action::ScrollToBottom => ScrollPopup(PopupScroll::Down(POPUP_EDGE_LINES)),
        Action::CloseFocus => ClosePopup,
        Action::SearchInput { .. }
        | Action::Search { .. }
        | Action::SearchToggleOption { .. }
        | Action::EditScrollback { .. }
        | Action::ScrollToPreviousPrompt
        | Action::ScrollToNextPrompt
        | Action::SelectCommandAtScrollPosition
        | Action::CopyLastCommandOutput
        | Action::ClearScreen
        | Action::Copy
        | Action::Paste { pane_id: None, .. }
        | Action::DumpScreen { pane_id: None, .. }
        | Action::ToggleFocusFullscreen
        | Action::ToggleFocusNoUiFullscreen
        | Action::TogglePaneEmbedOrFloating
        | Action::TogglePanePinned
        | Action::Resize { .. }
        | Action::MovePane { .. }
        | Action::MovePaneBackwards
        | Action::MoveTab { .. }
        | Action::BreakPane
        | Action::BreakPaneRight
        | Action::BreakPaneLeft
        | Action::PaneNameInput { .. }
        | Action::UndoRenamePane
        | Action::TabNameInput { .. }
        | Action::UndoRenameTab
        | Action::TogglePaneInGroup
        | Action::ToggleGroupMarking => Ignore,
        Action::SwitchToMode { input_mode } | Action::SwitchModeForAllClients { input_mode }
            if is_text_entry_mode(input_mode) =>
        {
            Ignore
        },
        Action::SkipConfirm { action } => popup_action_rule(action),
        _ => Underneath,
    }
}

fn key_to_popup(
    key: &KeyWithModifier,
    raw_bytes: Vec<u8>,
    is_kitty_keyboard_protocol: bool,
) -> PopupKeyAction {
    PopupKeyAction::KeyToPopup {
        key: key.clone(),
        raw_bytes,
        is_kitty_keyboard_protocol,
    }
}

fn always_reaches_popup(key: &KeyWithModifier) -> bool {
    key.has_no_modifiers() && matches!(key.bare_key, BareKey::Esc | BareKey::Enter)
}

pub(crate) fn key_actions_with_popup_open(
    actions: Vec<Action>,
    key: &KeyWithModifier,
    raw_bytes: Vec<u8>,
    is_kitty_keyboard_protocol: bool,
) -> Vec<PopupKeyAction> {
    let mut key_sent_to_popup = false;
    actions
        .into_iter()
        .filter_map(|action| match popup_action_rule(&action) {
            PopupActionRule::ToPopup => {
                if key_sent_to_popup {
                    None
                } else {
                    key_sent_to_popup = true;
                    Some(key_to_popup(
                        key,
                        raw_bytes.clone(),
                        is_kitty_keyboard_protocol,
                    ))
                }
            },
            PopupActionRule::Underneath => Some(PopupKeyAction::Route(action)),
            PopupActionRule::ScrollPopup(scroll) => Some(PopupKeyAction::ScrollPopup(scroll)),
            PopupActionRule::ClosePopup => Some(PopupKeyAction::ClosePopup),
            PopupActionRule::Ignore => None,
        })
        .collect()
}

pub(crate) fn actions_for_key(
    keybinds: &Keybinds,
    current_mode: &InputMode,
    default_mode: InputMode,
    key: &KeyWithModifier,
    raw_bytes: Vec<u8>,
    is_kitty_keyboard_protocol: bool,
    in_key_passthrough: bool,
) -> Vec<Action> {
    if in_key_passthrough {
        return vec![Action::Write {
            key_with_modifier: Some(key.clone()),
            bytes: raw_bytes,
            is_kitty_keyboard_protocol,
        }];
    }
    keybinds.get_actions_for_key_in_mode_or_default_action(
        current_mode,
        key,
        raw_bytes,
        default_mode,
        is_kitty_keyboard_protocol,
    )
}

pub(crate) fn key_dispatch(
    keybinds: &Keybinds,
    current_mode: &InputMode,
    default_mode: InputMode,
    key: &KeyWithModifier,
    raw_bytes: Vec<u8>,
    is_kitty_keyboard_protocol: bool,
    in_key_passthrough: bool,
    has_focused_popup: bool,
    popup_takes_all_keys: bool,
) -> Vec<PopupKeyAction> {
    if has_focused_popup
        && (in_key_passthrough || popup_takes_all_keys || always_reaches_popup(key))
    {
        return vec![key_to_popup(key, raw_bytes, is_kitty_keyboard_protocol)];
    }
    let actions = actions_for_key(
        keybinds,
        current_mode,
        default_mode,
        key,
        raw_bytes.clone(),
        is_kitty_keyboard_protocol,
        in_key_passthrough,
    );
    if has_focused_popup {
        key_actions_with_popup_open(actions, key, raw_bytes, is_kitty_keyboard_protocol)
    } else {
        actions.into_iter().map(PopupKeyAction::Route).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn key(c: char) -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Char(c))
    }

    fn ctrl(c: char) -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Char(c)).with_ctrl_modifier()
    }

    fn switch_to(input_mode: InputMode) -> Action {
        Action::SwitchToMode { input_mode }
    }

    fn keybinds() -> Keybinds {
        let mut normal = HashMap::new();
        normal.insert(ctrl('g'), vec![switch_to(InputMode::Locked)]);
        normal.insert(ctrl('p'), vec![switch_to(InputMode::Pane)]);
        normal.insert(ctrl('d'), vec![Action::Detach]);
        let mut locked = HashMap::new();
        locked.insert(ctrl('g'), vec![switch_to(InputMode::Normal)]);
        let mut pane = HashMap::new();
        pane.insert(
            key('f'),
            vec![Action::ToggleFocusFullscreen, switch_to(InputMode::Normal)],
        );
        pane.insert(
            key('x'),
            vec![Action::CloseFocus, switch_to(InputMode::Normal)],
        );
        pane.insert(
            key('c'),
            vec![
                switch_to(InputMode::RenamePane),
                Action::PaneNameInput { input: vec![0] },
            ],
        );
        pane.insert(
            key('h'),
            vec![Action::MoveFocus {
                direction: zellij_utils::data::Direction::Left,
            }],
        );
        let mut map = HashMap::new();
        map.insert(InputMode::Normal, normal);
        map.insert(InputMode::Locked, locked);
        map.insert(InputMode::Pane, pane);
        Keybinds(map)
    }

    fn dispatch(mode: InputMode, key: KeyWithModifier) -> Vec<PopupKeyAction> {
        key_dispatch(
            &keybinds(),
            &mode,
            InputMode::Normal,
            &key,
            vec![],
            false,
            false,
            true,
            false,
        )
    }

    fn write(key: KeyWithModifier) -> PopupKeyAction {
        PopupKeyAction::Route(Action::Write {
            key_with_modifier: Some(key),
            bytes: vec![],
            is_kitty_keyboard_protocol: false,
        })
    }

    fn to_popup(key: KeyWithModifier) -> PopupKeyAction {
        PopupKeyAction::KeyToPopup {
            key,
            raw_bytes: vec![],
            is_kitty_keyboard_protocol: false,
        }
    }

    fn keybinds_with_esc_and_enter_bound() -> Keybinds {
        let esc = KeyWithModifier::new(BareKey::Esc);
        let enter = KeyWithModifier::new(BareKey::Enter);
        let alt_enter = KeyWithModifier::new(BareKey::Enter).with_alt_modifier();
        let mut keybinds = keybinds();
        let normal = keybinds.0.get_mut(&InputMode::Normal).unwrap();
        normal.insert(esc.clone(), vec![switch_to(InputMode::Locked)]);
        normal.insert(enter.clone(), vec![switch_to(InputMode::Locked)]);
        normal.insert(alt_enter, vec![Action::Detach]);
        let pane = keybinds.0.get_mut(&InputMode::Pane).unwrap();
        pane.insert(esc, vec![switch_to(InputMode::Normal)]);
        pane.insert(enter, vec![switch_to(InputMode::Normal)]);
        keybinds
    }

    fn dispatch_with(
        keybinds: &Keybinds,
        mode: InputMode,
        key: KeyWithModifier,
        has_focused_popup: bool,
    ) -> Vec<PopupKeyAction> {
        key_dispatch(
            keybinds,
            &mode,
            InputMode::Normal,
            &key,
            vec![],
            false,
            false,
            has_focused_popup,
            false,
        )
    }

    #[test]
    fn bare_esc_and_enter_always_reach_the_popup_even_when_bound() {
        let keybinds = keybinds_with_esc_and_enter_bound();
        let esc = KeyWithModifier::new(BareKey::Esc);
        let enter = KeyWithModifier::new(BareKey::Enter);
        for mode in [InputMode::Normal, InputMode::Pane] {
            assert_eq!(
                dispatch_with(&keybinds, mode, esc.clone(), true),
                vec![to_popup(esc.clone())],
                "{:?}",
                mode
            );
            assert_eq!(
                dispatch_with(&keybinds, mode, enter.clone(), true),
                vec![to_popup(enter.clone())],
                "{:?}",
                mode
            );
        }
    }

    #[test]
    fn modified_enter_and_other_bound_keys_keep_their_binding_while_a_popup_is_open() {
        let keybinds = keybinds_with_esc_and_enter_bound();
        let alt_enter = KeyWithModifier::new(BareKey::Enter).with_alt_modifier();
        assert_eq!(
            dispatch_with(&keybinds, InputMode::Normal, alt_enter, true),
            vec![PopupKeyAction::Route(Action::Detach)]
        );
        assert_eq!(
            dispatch_with(&keybinds, InputMode::Normal, ctrl('p'), true),
            vec![PopupKeyAction::Route(switch_to(InputMode::Pane))]
        );
    }

    #[test]
    fn bound_esc_and_enter_run_their_action_without_a_popup() {
        let keybinds = keybinds_with_esc_and_enter_bound();
        let esc = KeyWithModifier::new(BareKey::Esc);
        let enter = KeyWithModifier::new(BareKey::Enter);
        assert_eq!(
            dispatch_with(&keybinds, InputMode::Pane, esc, false),
            vec![PopupKeyAction::Route(switch_to(InputMode::Normal))]
        );
        assert_eq!(
            dispatch_with(&keybinds, InputMode::Normal, enter, false),
            vec![PopupKeyAction::Route(switch_to(InputMode::Locked))]
        );
    }

    #[test]
    fn a_key_bound_to_several_writes_reaches_the_popup_once() {
        let mut keybinds = keybinds();
        keybinds.0.get_mut(&InputMode::Normal).unwrap().insert(
            ctrl('w'),
            vec![
                Action::WriteChars { chars: "a".into() },
                Action::WriteChars { chars: "b".into() },
            ],
        );
        assert_eq!(
            dispatch_with(&keybinds, InputMode::Normal, ctrl('w'), true),
            vec![to_popup(ctrl('w'))]
        );
    }

    #[test]
    fn bound_keys_run_their_action_while_a_popup_is_open() {
        assert_eq!(
            dispatch(InputMode::Normal, ctrl('g')),
            vec![PopupKeyAction::Route(switch_to(InputMode::Locked))]
        );
        assert_eq!(
            dispatch(InputMode::Normal, ctrl('d')),
            vec![PopupKeyAction::Route(Action::Detach)]
        );
        assert_eq!(
            dispatch(InputMode::Pane, key('h')),
            vec![PopupKeyAction::Route(Action::MoveFocus {
                direction: zellij_utils::data::Direction::Left
            })]
        );
    }

    #[test]
    fn unbound_keys_in_the_base_mode_reach_the_popup() {
        assert_eq!(
            dispatch(InputMode::Normal, key('a')),
            vec![to_popup(key('a'))]
        );
        let esc = KeyWithModifier::new(BareKey::Esc);
        assert_eq!(
            dispatch(InputMode::Normal, esc.clone()),
            vec![to_popup(esc)]
        );
    }

    #[test]
    fn locked_mode_sends_every_key_but_the_unlock_key_to_the_popup() {
        assert_eq!(
            dispatch(InputMode::Locked, ctrl('p')),
            vec![to_popup(ctrl('p'))]
        );
        assert_eq!(
            dispatch(InputMode::Locked, key('x')),
            vec![to_popup(key('x'))]
        );
        assert_eq!(
            dispatch(InputMode::Locked, ctrl('g')),
            vec![PopupKeyAction::Route(switch_to(InputMode::Normal))]
        );
    }

    #[test]
    fn unbound_keys_in_a_non_base_mode_do_not_reach_the_popup() {
        assert_eq!(
            dispatch(InputMode::Pane, key('q')),
            vec![PopupKeyAction::Route(Action::NoOp)]
        );
    }

    #[test]
    fn close_pane_closes_the_popup_and_pane_only_actions_are_dropped() {
        assert_eq!(
            dispatch(InputMode::Pane, key('x')),
            vec![
                PopupKeyAction::ClosePopup,
                PopupKeyAction::Route(switch_to(InputMode::Normal))
            ]
        );
        assert_eq!(
            dispatch(InputMode::Pane, key('f')),
            vec![PopupKeyAction::Route(switch_to(InputMode::Normal))]
        );
        assert_eq!(dispatch(InputMode::Pane, key('c')), vec![]);
    }

    #[test]
    fn a_popup_that_takes_all_keys_gets_bound_keys_in_every_mode() {
        let down = KeyWithModifier::new(BareKey::Down);
        for mode in [InputMode::Normal, InputMode::Pane, InputMode::Locked] {
            for key in [ctrl('p'), key('h'), down.clone(), ctrl('g')] {
                let actions = key_dispatch(
                    &keybinds(),
                    &mode,
                    InputMode::Normal,
                    &key,
                    vec![],
                    false,
                    false,
                    true,
                    true,
                );
                assert_eq!(actions, vec![to_popup(key.clone())], "{:?} {:?}", mode, key);
            }
        }
    }

    #[test]
    fn key_passthrough_sends_bound_keys_to_the_popup() {
        let actions = key_dispatch(
            &keybinds(),
            &InputMode::Normal,
            InputMode::Normal,
            &ctrl('g'),
            vec![],
            false,
            true,
            true,
            false,
        );
        assert_eq!(actions, vec![to_popup(ctrl('g'))]);
        let actions = key_dispatch(
            &keybinds(),
            &InputMode::Normal,
            InputMode::Normal,
            &ctrl('g'),
            vec![],
            false,
            true,
            false,
            false,
        );
        assert_eq!(actions, vec![write(ctrl('g'))]);
    }

    #[test]
    fn without_a_popup_every_action_is_routed_as_is() {
        let actions = key_dispatch(
            &keybinds(),
            &InputMode::Pane,
            InputMode::Normal,
            &key('f'),
            vec![],
            false,
            false,
            false,
            false,
        );
        assert_eq!(
            actions,
            vec![
                PopupKeyAction::Route(Action::ToggleFocusFullscreen),
                PopupKeyAction::Route(switch_to(InputMode::Normal))
            ]
        );
    }

    #[test]
    fn action_rules_while_a_popup_is_open() {
        use PopupActionRule::*;
        let cases = vec![
            (Action::WriteChars { chars: "a".into() }, ToPopup),
            (Action::ScrollUp, ScrollPopup(PopupScroll::Up(1))),
            (
                Action::PageScrollDown,
                ScrollPopup(PopupScroll::Down(POPUP_PAGE_LINES)),
            ),
            (Action::SearchInput { input: vec![1] }, Ignore),
            (Action::CloseFocus, ClosePopup),
            (Action::GoToNextTab, Underneath),
            (
                Action::NewPane {
                    direction: None,
                    pane_name: None,
                    start_suppressed: false,
                },
                Underneath,
            ),
            (Action::Detach, Underneath),
            (Action::Quit, Underneath),
            (Action::FocusNextPane, Underneath),
            (switch_to(InputMode::Pane), Underneath),
            (switch_to(InputMode::RenameTab), Ignore),
            (switch_to(InputMode::EnterSearch), Ignore),
            (Action::ToggleFocusFullscreen, Ignore),
            (Action::TogglePaneEmbedOrFloating, Ignore),
            (Action::TogglePanePinned, Ignore),
            (
                Action::Resize {
                    resize: zellij_utils::data::Resize::Increase,
                    direction: None,
                },
                Ignore,
            ),
            (Action::MovePane { direction: None }, Ignore),
            (Action::TogglePaneInGroup, Ignore),
            (Action::TabNameInput { input: vec![0] }, Ignore),
            (
                Action::SkipConfirm {
                    action: Box::new(Action::CloseFocus),
                },
                ClosePopup,
            ),
        ];
        for (action, rule) in cases {
            assert_eq!(popup_action_rule(&action), rule, "{:?}", action);
        }
    }
}

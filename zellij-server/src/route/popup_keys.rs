use zellij_utils::data::{InputMode, KeyWithModifier};
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

pub(crate) fn key_actions_with_popup_open(actions: Vec<Action>) -> Vec<PopupKeyAction> {
    actions
        .into_iter()
        .filter_map(|action| match popup_action_rule(&action) {
            PopupActionRule::ToPopup | PopupActionRule::Underneath => {
                Some(PopupKeyAction::Route(action))
            },
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
) -> Vec<PopupKeyAction> {
    let actions = actions_for_key(
        keybinds,
        current_mode,
        default_mode,
        key,
        raw_bytes,
        is_kitty_keyboard_protocol,
        in_key_passthrough,
    );
    if has_focused_popup && !in_key_passthrough {
        key_actions_with_popup_open(actions)
    } else {
        actions.into_iter().map(PopupKeyAction::Route).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use zellij_utils::data::BareKey;

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
        pane.insert(key('x'), vec![Action::CloseFocus, switch_to(InputMode::Normal)]);
        pane.insert(
            key('c'),
            vec![
                switch_to(InputMode::RenamePane),
                Action::PaneNameInput { input: vec![0] },
            ],
        );
        pane.insert(key('h'), vec![Action::MoveFocus {
            direction: zellij_utils::data::Direction::Left,
        }]);
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
        )
    }

    fn write(key: KeyWithModifier) -> PopupKeyAction {
        PopupKeyAction::Route(Action::Write {
            key_with_modifier: Some(key),
            bytes: vec![],
            is_kitty_keyboard_protocol: false,
        })
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
        assert_eq!(dispatch(InputMode::Normal, key('a')), vec![write(key('a'))]);
        let esc = KeyWithModifier::new(BareKey::Esc);
        assert_eq!(dispatch(InputMode::Normal, esc.clone()), vec![write(esc)]);
    }

    #[test]
    fn locked_mode_sends_every_key_but_the_unlock_key_to_the_popup() {
        assert_eq!(dispatch(InputMode::Locked, ctrl('p')), vec![write(ctrl('p'))]);
        assert_eq!(dispatch(InputMode::Locked, key('x')), vec![write(key('x'))]);
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
    fn key_passthrough_is_unchanged_by_popups() {
        let actions = key_dispatch(
            &keybinds(),
            &InputMode::Normal,
            InputMode::Normal,
            &ctrl('g'),
            vec![],
            false,
            true,
            true,
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

use serde::{Deserialize, Serialize};

use crate::data::{BareKey, KeyModifier, KeyWithModifier};
use crate::position::Position;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
/// A mouse event can have any number of buttons (including no
/// buttons) pressed or released.
pub struct MouseEvent {
    /// A mouse event can current be a Press, Release, or Motion.
    /// Future events could consider double-click and triple-click.
    pub event_type: MouseEventType,

    // Mouse buttons associated with this event.
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub wheel_up: bool,
    pub wheel_down: bool,

    // Keyboard modifier flags can be encoded with events too.  They
    // are not often passed on the wire (instead used for
    // selection/copy-paste and changing terminal properties
    // on-the-fly at the user-facing terminal), but alt-mouseclick
    // usually passes through and is testable on vttest.  termwiz
    // already exposes them too.
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,

    /// The coordinates are zero-based.
    pub position: Position,
}

/// A mouse related event
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub enum MouseEventType {
    /// A mouse button was pressed.
    Press,
    /// A mouse button was released.
    Release,
    /// A mouse button is held over the given coordinates.
    Motion,
}

impl MouseEvent {
    pub fn new() -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: false,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position: Position::new(0, 0),
        };
        event
    }
    pub fn new_buttonless_motion(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: false,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_left_press_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_right_press_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: true,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_middle_press_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: true,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_middle_release_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Release,
            left: false,
            right: false,
            middle: true,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_left_release_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Release,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_left_motion_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_right_release_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Release,
            left: false,
            right: true,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_right_motion_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: false,
            right: true,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_middle_motion_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: false,
            right: false,
            middle: true,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_left_press_with_alt_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: true,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_right_press_with_alt_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: true,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: true,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_left_press_with_ctrl_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: true,
            position,
        };
        event
    }
    pub fn new_left_motion_with_ctrl_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: true,
            position,
        };
        event
    }
    pub fn new_left_release_with_ctrl_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Release,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: true,
            position,
        };
        event
    }
    pub fn new_left_motion_with_alt_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            shift: false,
            alt: true,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_scroll_up_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: true,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_scroll_down_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: true,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_ctrl_scroll_up_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: true,
            wheel_down: false,
            shift: false,
            alt: false,
            ctrl: true,
            position,
        };
        event
    }
    pub fn new_ctrl_scroll_down_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: true,
            shift: false,
            alt: false,
            ctrl: true,
            position,
        };
        event
    }
    /// Converts a modifier+scroll mouse event into a KeyWithModifier for keybinding matching.
    /// Returns None for non-scroll events or scroll events without modifiers.
    pub fn to_key_with_modifier(&self) -> Option<KeyWithModifier> {
        let bare_key = if self.wheel_up {
            BareKey::ScrollUp
        } else if self.wheel_down {
            BareKey::ScrollDown
        } else {
            return None;
        };

        let has_modifier = self.ctrl || self.alt || self.shift;
        if !has_modifier {
            return None;
        }

        let mut key = KeyWithModifier::new(bare_key);
        if self.ctrl {
            key.key_modifiers.insert(KeyModifier::Ctrl);
        }
        if self.alt {
            key.key_modifiers.insert(KeyModifier::Alt);
        }
        if self.shift {
            key.key_modifiers.insert(KeyModifier::Shift);
        }
        Some(key)
    }
}

impl Default for MouseEvent {
    fn default() -> Self {
        MouseEvent::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{BareKey, KeyModifier, KeyWithModifier};

    #[test]
    fn ctrl_scroll_up_converts_to_key_with_modifier() {
        let event = MouseEvent::new_ctrl_scroll_up_event(Position::new(0, 0));
        let key = event.to_key_with_modifier();
        assert_eq!(
            key,
            Some(KeyWithModifier::new(BareKey::ScrollUp).with_ctrl_modifier()),
        );
    }

    #[test]
    fn ctrl_scroll_down_converts_to_key_with_modifier() {
        let event = MouseEvent::new_ctrl_scroll_down_event(Position::new(0, 0));
        let key = event.to_key_with_modifier();
        assert_eq!(
            key,
            Some(KeyWithModifier::new(BareKey::ScrollDown).with_ctrl_modifier()),
        );
    }

    #[test]
    fn alt_scroll_up_converts_to_key_with_modifier() {
        let mut event = MouseEvent::new_scroll_up_event(Position::new(0, 0));
        event.alt = true;
        let key = event.to_key_with_modifier();
        assert_eq!(
            key,
            Some(KeyWithModifier::new(BareKey::ScrollUp).with_alt_modifier()),
        );
    }

    #[test]
    fn shift_scroll_down_converts_to_key_with_modifier() {
        let mut event = MouseEvent::new_scroll_down_event(Position::new(0, 0));
        event.shift = true;
        let key = event.to_key_with_modifier();
        assert_eq!(
            key,
            Some(KeyWithModifier::new(BareKey::ScrollDown).with_shift_modifier()),
        );
    }

    #[test]
    fn ctrl_alt_scroll_up_converts_with_both_modifiers() {
        let mut event = MouseEvent::new_ctrl_scroll_up_event(Position::new(0, 0));
        event.alt = true;
        let key = event.to_key_with_modifier();
        assert_eq!(
            key,
            Some(
                KeyWithModifier::new(BareKey::ScrollUp)
                    .with_ctrl_modifier()
                    .with_alt_modifier()
            ),
        );
    }

    #[test]
    fn plain_scroll_up_returns_none() {
        let event = MouseEvent::new_scroll_up_event(Position::new(0, 0));
        let key = event.to_key_with_modifier();
        assert_eq!(key, None);
    }

    #[test]
    fn plain_scroll_down_returns_none() {
        let event = MouseEvent::new_scroll_down_event(Position::new(0, 0));
        let key = event.to_key_with_modifier();
        assert_eq!(key, None);
    }

    #[test]
    fn left_click_returns_none() {
        let event = MouseEvent::new_left_press_event(Position::new(0, 0));
        let key = event.to_key_with_modifier();
        assert_eq!(key, None);
    }

    #[test]
    fn motion_event_returns_none() {
        let event = MouseEvent::new_buttonless_motion(Position::new(0, 0));
        let key = event.to_key_with_modifier();
        assert_eq!(key, None);
    }

    #[test]
    fn ctrl_scroll_matches_configured_keybinding() {
        use crate::input::config::Config;
        use crate::data::InputMode;
        use crate::input::actions::Action;

        let config_contents = r#"
            keybinds {
                shared_except "locked" {
                    bind "Ctrl ScrollUp" { GoToPreviousTab; }
                    bind "Ctrl ScrollDown" { GoToNextTab; }
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();

        // Simulate a Ctrl+ScrollUp mouse event
        let event = MouseEvent::new_ctrl_scroll_up_event(Position::new(5, 10));
        let key = event.to_key_with_modifier().unwrap();
        let action = config
            .keybinds
            .get_actions_for_key_in_mode(&InputMode::Normal, &key);
        assert_eq!(
            action,
            Some(&vec![Action::GoToPreviousTab]),
            "Ctrl+ScrollUp mouse event resolves to GoToPreviousTab via keybinding"
        );

        // Simulate a Ctrl+ScrollDown mouse event
        let event = MouseEvent::new_ctrl_scroll_down_event(Position::new(5, 10));
        let key = event.to_key_with_modifier().unwrap();
        let action = config
            .keybinds
            .get_actions_for_key_in_mode(&InputMode::Normal, &key);
        assert_eq!(
            action,
            Some(&vec![Action::GoToNextTab]),
            "Ctrl+ScrollDown mouse event resolves to GoToNextTab via keybinding"
        );
    }

    #[test]
    fn plain_scroll_does_not_match_modifier_keybinding() {
        use crate::input::config::Config;

        let config_contents = r#"
            keybinds {
                normal {
                    bind "Ctrl ScrollUp" { GoToPreviousTab; }
                }
            }
        "#;
        let _config = Config::from_kdl(config_contents, None).unwrap();

        // Plain scroll (no modifier) should return None, not matching keybindings
        let event = MouseEvent::new_scroll_up_event(Position::new(5, 10));
        assert_eq!(
            event.to_key_with_modifier(),
            None,
            "Plain scroll does not produce a keybinding key"
        );
    }
}

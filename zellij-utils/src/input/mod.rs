pub mod actions;
pub mod command;
pub mod config;
pub mod keybinds;
pub mod layout;
pub mod options;
pub mod permission;
pub mod plugins;
pub mod theme;

// Can't use this in wasm due to dependency on the `termwiz` crate.
#[cfg(not(target_family = "wasm"))]
pub mod mouse;

#[cfg(not(target_family = "wasm"))]
pub use not_wasm::*;

#[cfg(not(target_family = "wasm"))]
mod not_wasm {
    use crate::{
        data::{CharOrArrow, Direction, InputMode, Key, KeyWithModifier, BareKey, ModeInfo, PluginCapabilities},
        envs,
        ipc::ClientAttributes,
    };
    use termwiz::input::{InputEvent, InputParser, KeyCode, KeyEvent, Modifiers};

    use super::keybinds::Keybinds;

    /// Creates a [`ModeInfo`] struct indicating the current [`InputMode`] and its keybinds
    /// (as pairs of [`String`]s).
    pub fn get_mode_info(
        mode: InputMode,
        attributes: &ClientAttributes,
        capabilities: PluginCapabilities,
    ) -> ModeInfo {
        let keybinds = attributes.keybinds.to_keybinds_vec();
        let session_name = envs::get_session_name().ok();

        ModeInfo {
            mode,
            keybinds,
            style: attributes.style,
            capabilities,
            session_name,
        }
    }

    pub fn parse_keys(input_bytes: &[u8]) -> Vec<KeyWithModifier> {
        let mut ret = vec![];
        let mut input_parser = InputParser::new(); // this is the termwiz InputParser
        let maybe_more = false;
        let parse_input_event = |input_event: InputEvent| {
            if let InputEvent::Key(key_event) = input_event {
                ret.push(cast_termwiz_key(key_event, input_bytes, None));
            }
        };
        input_parser.parse(input_bytes, parse_input_event, maybe_more);
        ret
    }

    fn key_is_bound(key: &KeyWithModifier, keybinds: &Keybinds, mode: &InputMode) -> bool {
        keybinds
            .get_actions_for_key_in_mode(mode, key)
            .map_or(false, |actions| !actions.is_empty())
    }

    // FIXME: This is an absolutely cursed function that should be destroyed as soon
    // as an alternative that doesn't touch zellij-tile can be developed...
    pub fn cast_termwiz_key(
        event: KeyEvent,
        raw_bytes: &[u8],
        keybinds_mode: Option<(&Keybinds, &InputMode)>,
    ) -> KeyWithModifier {
        let modifiers = event.modifiers;

        // *** THIS IS WHERE WE SHOULD WORK AROUND ISSUES WITH TERMWIZ ***
        if raw_bytes == [8] {
            return KeyWithModifier::new(BareKey::Char('h')).with_ctrl_modifier();
        };

        if raw_bytes == [10] {
            if let Some((keybinds, mode)) = keybinds_mode {
                let ctrl_j = KeyWithModifier::new(BareKey::Char('j')).with_ctrl_modifier();
                if key_is_bound(&ctrl_j, keybinds, mode) {
                    return ctrl_j;
                }
            }
        }

        match event.key {
            KeyCode::Char(c) => {
                if modifiers.contains(Modifiers::CTRL) {
                    KeyWithModifier::new(BareKey::Char(c.to_lowercase().next().unwrap_or_default())).with_ctrl_modifier()
                    // Key::Ctrl(c.to_lowercase().next().unwrap_or_default())
                } else if modifiers.contains(Modifiers::ALT) {
                    KeyWithModifier::new(BareKey::Char(c.to_lowercase().next().unwrap_or_default())).with_alt_modifier()
                    // Key::Alt(CharOrArrow::Char(c))
                } else {
                    KeyWithModifier::new(BareKey::Char(c.to_lowercase().next().unwrap_or_default()))
                    // Key::Char(c)
                }
            },
            KeyCode::Backspace => KeyWithModifier::new(BareKey::Backspace),
            // KeyCode::Backspace => Key::Backspace,
            KeyCode::LeftArrow | KeyCode::ApplicationLeftArrow => {
                if modifiers.contains(Modifiers::ALT) {
                    KeyWithModifier::new(BareKey::Left).with_alt_modifier()
                    // Key::Alt(CharOrArrow::Direction(Direction::Left))
                } else {
                    KeyWithModifier::new(BareKey::Left)
                    // Key::Left
                }
            },
            KeyCode::RightArrow | KeyCode::ApplicationRightArrow => {
                if modifiers.contains(Modifiers::ALT) {
                    // Key::Alt(CharOrArrow::Direction(Direction::Right))
                    KeyWithModifier::new(BareKey::Right).with_alt_modifier()
                    // Key::Alt(CharOrArrow::Direction(Direction::Right))
                } else {
                    KeyWithModifier::new(BareKey::Right)
                    // Key::Right
                }
            },
            KeyCode::UpArrow | KeyCode::ApplicationUpArrow => {
                if modifiers.contains(Modifiers::ALT) {
                    // Key::Alt(CharOrArrow::Direction(Direction::Up))
                    KeyWithModifier::new(BareKey::Up).with_alt_modifier()
                } else {
                    KeyWithModifier::new(BareKey::Up)
                    // Key::Up
                }
            },
            KeyCode::DownArrow | KeyCode::ApplicationDownArrow => {
                if modifiers.contains(Modifiers::ALT) {
                    // Key::Alt(CharOrArrow::Direction(Direction::Down))
                    KeyWithModifier::new(BareKey::Down).with_alt_modifier()
                } else {
                    KeyWithModifier::new(BareKey::Down)
                    // Key::Down
                }
            },
            KeyCode::Home => KeyWithModifier::new(BareKey::Home),
            KeyCode::End => KeyWithModifier::new(BareKey::End),
            KeyCode::PageUp => KeyWithModifier::new(BareKey::PageUp),
            KeyCode::PageDown => KeyWithModifier::new(BareKey::PageDown),
            KeyCode::Tab => KeyWithModifier::new(BareKey::Tab),
            KeyCode::Delete => KeyWithModifier::new(BareKey::Delete),
            KeyCode::Insert => KeyWithModifier::new(BareKey::Insert),
            KeyCode::Function(n) => {
                if modifiers.contains(Modifiers::ALT) {
                    KeyWithModifier::new(BareKey::F(n)).with_alt_modifier()
                    // Key::AltF(n)
                } else if modifiers.contains(Modifiers::CTRL) {
                    KeyWithModifier::new(BareKey::F(n)).with_ctrl_modifier()
                    // Key::CtrlF(n)
                } else {
                    KeyWithModifier::new(BareKey::F(n))
                    // Key::F(n)
                }
            },
            KeyCode::Escape => KeyWithModifier::new(BareKey::Esc),
            KeyCode::Enter => KeyWithModifier::new(BareKey::Enter),
            _ => KeyWithModifier::new(BareKey::Esc), // there are other keys we can implement here, but we might need additional terminal support to implement them, not just exhausting this enum
        }
    }
}

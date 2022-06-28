//! The way terminal input is handled.
pub mod actions;
pub mod command;
pub mod config;
pub mod keybinds;
pub mod layout;
pub mod options;
pub mod plugins;
pub mod theme;

// Can't use this in wasm due to dependency on the `termwiz` crate.
#[cfg(not(target_family = "wasm"))]
pub mod mouse;

use crate::{
    data::{InputMode, Key, ModeInfo, PluginCapabilities, Style},
    envs,
    input::keybinds::Keybinds,
};

/// Creates a [`ModeInfo`] struct indicating the current [`InputMode`] and its keybinds
/// (as pairs of [`String`]s).
pub fn get_mode_info(mode: InputMode, style: Style, capabilities: PluginCapabilities) -> ModeInfo {
    // FIXME: Need access to the real keybindings here
    let keybinds = Keybinds::default().get_mode_keybinds(&mode).to_cloned_vec();
    let session_name = envs::get_session_name().ok();

    ModeInfo {
        mode,
        keybinds,
        style,
        capabilities,
        session_name,
    }
}

#[cfg(not(target_family = "wasm"))]
pub use not_wasm::*;

#[cfg(not(target_family = "wasm"))]
mod not_wasm {
    use super::*;
    use crate::data::{CharOrArrow, Direction};
    use termwiz::input::{InputEvent, InputParser, KeyCode, KeyEvent, Modifiers};

    pub fn parse_keys(input_bytes: &[u8]) -> Vec<Key> {
        let mut ret = vec![];
        let mut input_parser = InputParser::new(); // this is the termwiz InputParser
        let maybe_more = false;
        let parse_input_event = |input_event: InputEvent| {
            if let InputEvent::Key(key_event) = input_event {
                ret.push(cast_termwiz_key(key_event, input_bytes));
            }
        };
        input_parser.parse(input_bytes, parse_input_event, maybe_more);
        ret
    }

    // FIXME: This is an absolutely cursed function that should be destroyed as soon
    // as an alternative that doesn't touch zellij-tile can be developed...
    pub fn cast_termwiz_key(event: KeyEvent, raw_bytes: &[u8]) -> Key {
        let modifiers = event.modifiers;

        // *** THIS IS WHERE WE SHOULD WORK AROUND ISSUES WITH TERMWIZ ***
        if raw_bytes == [8] {
            return Key::Ctrl('h');
        };

        match event.key {
            KeyCode::Char(c) => {
                if modifiers.contains(Modifiers::CTRL) {
                    Key::Ctrl(c.to_lowercase().next().unwrap_or_default())
                } else if modifiers.contains(Modifiers::ALT) {
                    Key::Alt(CharOrArrow::Char(c))
                } else {
                    Key::Char(c)
                }
            },
            KeyCode::Backspace => Key::Backspace,
            KeyCode::LeftArrow | KeyCode::ApplicationLeftArrow => {
                if modifiers.contains(Modifiers::ALT) {
                    Key::Alt(CharOrArrow::Direction(Direction::Left))
                } else {
                    Key::Left
                }
            },
            KeyCode::RightArrow | KeyCode::ApplicationRightArrow => {
                if modifiers.contains(Modifiers::ALT) {
                    Key::Alt(CharOrArrow::Direction(Direction::Right))
                } else {
                    Key::Right
                }
            },
            KeyCode::UpArrow | KeyCode::ApplicationUpArrow => {
                if modifiers.contains(Modifiers::ALT) {
                    //Key::AltPlusUpArrow
                    Key::Alt(CharOrArrow::Direction(Direction::Up))
                } else {
                    Key::Up
                }
            },
            KeyCode::DownArrow | KeyCode::ApplicationDownArrow => {
                if modifiers.contains(Modifiers::ALT) {
                    Key::Alt(CharOrArrow::Direction(Direction::Down))
                } else {
                    Key::Down
                }
            },
            KeyCode::Home => Key::Home,
            KeyCode::End => Key::End,
            KeyCode::PageUp => Key::PageUp,
            KeyCode::PageDown => Key::PageDown,
            KeyCode::Tab => Key::BackTab, // TODO: ???
            KeyCode::Delete => Key::Delete,
            KeyCode::Insert => Key::Insert,
            KeyCode::Function(n) => Key::F(n),
            KeyCode::Escape => Key::Esc,
            KeyCode::Enter => Key::Char('\n'),
            _ => Key::Esc, // there are other keys we can implement here, but we might need additional terminal support to implement them, not just exhausting this enum
        }
    }
}

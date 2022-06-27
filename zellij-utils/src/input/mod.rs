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
    envs,
    data::{InputMode, Key, ModeInfo, PluginCapabilities, Style},
};

/// Creates a [`ModeInfo`] struct indicating the current [`InputMode`] and its keybinds
/// (as pairs of [`String`]s).
pub fn get_mode_info(mode: InputMode, style: Style, capabilities: PluginCapabilities) -> ModeInfo {
    let keybinds = match mode {
        InputMode::Normal | InputMode::Locked | InputMode::Prompt => Vec::new(),
        InputMode::Resize => vec![
            ("←↓↑→".to_string(), "Resize".to_string()),
            ("+-".to_string(), "Increase/Decrease size".to_string()),
        ],
        InputMode::Move => vec![
            ("←↓↑→".to_string(), "Move".to_string()),
            ("n/Tab".to_string(), "Next Pane".to_string()),
        ],
        InputMode::Pane => vec![
            ("←↓↑→".to_string(), "Move focus".to_string()),
            ("n".to_string(), "New".to_string()),
            ("d".to_string(), "Down split".to_string()),
            ("r".to_string(), "Right split".to_string()),
            ("x".to_string(), "Close".to_string()),
            ("f".to_string(), "Fullscreen".to_string()),
            ("z".to_string(), "Frames".to_string()),
            ("c".to_string(), "Rename".to_string()),
            ("w".to_string(), "Floating Toggle".to_string()),
            ("e".to_string(), "Embed Pane".to_string()),
            ("p".to_string(), "Next".to_string()),
        ],
        InputMode::Tab => vec![
            ("←↓↑→".to_string(), "Move focus".to_string()),
            ("n".to_string(), "New".to_string()),
            ("x".to_string(), "Close".to_string()),
            ("r".to_string(), "Rename".to_string()),
            ("s".to_string(), "Sync".to_string()),
            ("Tab".to_string(), "Toggle".to_string()),
        ],
        InputMode::Scroll => vec![
            ("↓↑".to_string(), "Scroll".to_string()),
            ("PgDn/PgUp".to_string(), "Scroll Page".to_string()),
            ("d/u".to_string(), "Scroll Half Page".to_string()),
            (
                "e".to_string(),
                "Edit Scrollback in Default Editor".to_string(),
            ),
            ("s".to_string(), "Enter search term".to_string()),
        ],
        InputMode::EnterSearch => vec![("Enter".to_string(), "when done".to_string())],
        InputMode::Search => vec![
            ("↓↑".to_string(), "Scroll".to_string()),
            ("PgUp/PgDn".to_string(), "Scroll Page".to_string()),
            ("u/d".to_string(), "Scroll Half Page".to_string()),
            ("n".to_string(), "Search down".to_string()),
            ("p".to_string(), "Search up".to_string()),
            ("c".to_string(), "Case sensitivity".to_string()),
            ("w".to_string(), "Wrap".to_string()),
            ("o".to_string(), "Whole words".to_string()),
        ],
        InputMode::RenameTab => vec![("Enter".to_string(), "when done".to_string())],
        InputMode::RenamePane => vec![("Enter".to_string(), "when done".to_string())],
        InputMode::Session => vec![("d".to_string(), "Detach".to_string())],
        InputMode::Tmux => vec![
            ("←↓↑→".to_string(), "Move focus".to_string()),
            ("\"".to_string(), "Split Down".to_string()),
            ("%".to_string(), "Split Right".to_string()),
            ("z".to_string(), "Fullscreen".to_string()),
            ("c".to_string(), "New Tab".to_string()),
            (",".to_string(), "Rename Tab".to_string()),
            ("p".to_string(), "Previous Tab".to_string()),
            ("n".to_string(), "Next Tab".to_string()),
        ],
    };

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

//! The way terminal input is handled.

pub mod actions;
pub mod command;
pub mod config;
pub mod keybinds;
pub mod layout;
pub mod mouse;
pub mod options;
pub mod plugins;
pub mod theme;

use termion::input::TermRead;
use zellij_tile::data::{InputMode, Key, ModeInfo, Palette, PluginCapabilities};

/// Creates a [`ModeInfo`] struct indicating the current [`InputMode`] and its keybinds
/// (as pairs of [`String`]s).
pub fn get_mode_info(
    mode: InputMode,
    palette: Palette,
    capabilities: PluginCapabilities,
) -> ModeInfo {
    let keybinds = match mode {
        InputMode::Normal | InputMode::Locked => Vec::new(),
        InputMode::Resize => vec![("←↓↑→".to_string(), "Resize".to_string())],
        InputMode::Pane => vec![
            ("←↓↑→".to_string(), "Move focus".to_string()),
            ("p".to_string(), "Next".to_string()),
            ("n".to_string(), "New".to_string()),
            ("d".to_string(), "Down split".to_string()),
            ("r".to_string(), "Right split".to_string()),
            ("x".to_string(), "Close".to_string()),
            ("f".to_string(), "Fullscreen".to_string()),
            ("z".to_string(), "Frames".to_string()),
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
            ("PgUp/PgDn".to_string(), "Scroll Page".to_string()),
        ],
        InputMode::RenameTab => vec![("Enter".to_string(), "when done".to_string())],
        InputMode::Session => vec![("d".to_string(), "Detach".to_string())],
    };

    let session_name = std::env::var("ZELLIJ_SESSION_NAME").ok();

    ModeInfo {
        mode,
        keybinds,
        palette,
        capabilities,
        session_name,
    }
}

pub fn parse_keys(input_bytes: &[u8]) -> Vec<Key> {
    input_bytes.keys().flatten().map(cast_termion_key).collect()
}

// FIXME: This is an absolutely cursed function that should be destroyed as soon
// as an alternative that doesn't touch zellij-tile can be developed...
pub fn cast_termion_key(event: termion::event::Key) -> Key {
    match event {
        termion::event::Key::Backspace => Key::Backspace,
        termion::event::Key::Left => Key::Left,
        termion::event::Key::Right => Key::Right,
        termion::event::Key::Up => Key::Up,
        termion::event::Key::Down => Key::Down,
        termion::event::Key::Home => Key::Home,
        termion::event::Key::End => Key::End,
        termion::event::Key::PageUp => Key::PageUp,
        termion::event::Key::PageDown => Key::PageDown,
        termion::event::Key::BackTab => Key::BackTab,
        termion::event::Key::Delete => Key::Delete,
        termion::event::Key::Insert => Key::Insert,
        termion::event::Key::F(n) => Key::F(n),
        termion::event::Key::Char(c) => Key::Char(c),
        termion::event::Key::Alt(c) => Key::Alt(c),
        termion::event::Key::Ctrl(c) => Key::Ctrl(c),
        termion::event::Key::Null => Key::Null,
        termion::event::Key::Esc => Key::Esc,
        _ => {
            unimplemented!("Encountered an unknown key!")
        }
    }
}

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

use crate::envs;
use zellij_tile::data::{InputMode, Key, ModeInfo, Palette, PluginCapabilities};

/// Creates a [`ModeInfo`] struct indicating the current [`InputMode`] and its keybinds
/// (as pairs of [`String`]s).
pub fn get_mode_info(
    mode: InputMode,
    palette: Palette,
    capabilities: PluginCapabilities,
) -> ModeInfo {
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
            ("p".to_string(), "Next".to_string()),
            ("n".to_string(), "New".to_string()),
            ("d".to_string(), "Down split".to_string()),
            ("r".to_string(), "Right split".to_string()),
            ("x".to_string(), "Close".to_string()),
            ("f".to_string(), "Fullscreen".to_string()),
            ("z".to_string(), "Frames".to_string()),
            ("c".to_string(), "Rename".to_string()),
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
            ("u/d".to_string(), "Scroll Half Page".to_string()),
        ],
        InputMode::RenameTab => vec![("Enter".to_string(), "when done".to_string())],
        InputMode::RenamePane => vec![("Enter".to_string(), "when done".to_string())],
        InputMode::Session => vec![("d".to_string(), "Detach".to_string())],
    };

    let session_name = envs::get_session_name().ok();

    ModeInfo {
        mode,
        keybinds,
        palette,
        capabilities,
        session_name,
    }
}

pub fn parse_keys(input_bytes: &[u8]) -> Vec<Key> {
    input_bytes
        .keys()
        .flatten()
        .map(cast_crossterm_key)
        .collect()
}

// FIXME: This is an absolutely cursed function that should be destroyed as soon
// as an alternative that doesn't touch zellij-tile can be developed...
pub fn cast_crossterm_key(event: crossterm::event::KeyEvent) -> Key {
    match event.code {
        crossterm::event::KeyCode::Backspace => Key::Backspace,
        crossterm::event::KeyCode::Left => Key::Left,
        crossterm::event::KeyCode::Right => Key::Right,
        crossterm::event::KeyCode::Up => Key::Up,
        crossterm::event::KeyCode::Down => Key::Down,
        crossterm::event::KeyCode::Home => Key::Home,
        crossterm::event::KeyCode::End => Key::End,
        crossterm::event::KeyCode::PageUp => Key::PageUp,
        crossterm::event::KeyCode::PageDown => Key::PageDown,
        crossterm::event::KeyCode::BackTab => Key::BackTab,
        crossterm::event::KeyCode::Delete => Key::Delete,
        crossterm::event::KeyCode::Insert => Key::Insert,
        crossterm::event::KeyCode::F(n) => Key::F(n),
        crossterm::event::KeyCode::Char(c) if event.modifiers.contains(crossterm::event::KeyModifiers::ALT) => Key::Alt(c),
        crossterm::event::KeyCode::Char(c) if event.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => Key::Ctrl(c),
        crossterm::event::KeyCode::Char(c) => Key::Char(c),
        crossterm::event::KeyCode::Null => Key::Null,
        crossterm::event::KeyCode::Esc => Key::Esc,
        _ => {
            unimplemented!("Encountered an unknown key!")
        }
    }
}

//! The way terminal input is handled.

pub mod actions;
pub mod config;
pub mod keybinds;
pub mod options;

use termion::input::TermRead;
use zellij_tile::data::{InputMode, Key, ModeInfo, Palette, PluginCapabilities};

/// Creates a [`Help`] struct indicating the current [`InputMode`] and its keybinds
/// (as pairs of [`String`]s).
// TODO this should probably be automatically generated in some way
pub fn get_mode_info(
    mode: InputMode,
    palette: Palette,
    capabilities: PluginCapabilities,
) -> ModeInfo {
    let mut keybinds: Vec<(String, String)> = vec![];
    match mode {
        InputMode::Normal | InputMode::Locked => {}
        InputMode::Resize => {
            keybinds.push(("←↓↑→".to_string(), "Resize".to_string()));
        }
        InputMode::Pane => {
            keybinds.push(("←↓↑→".to_string(), "Move focus".to_string()));
            keybinds.push(("p".to_string(), "Next".to_string()));
            keybinds.push(("n".to_string(), "New".to_string()));
            keybinds.push(("d".to_string(), "Down split".to_string()));
            keybinds.push(("r".to_string(), "Right split".to_string()));
            keybinds.push(("x".to_string(), "Close".to_string()));
            keybinds.push(("f".to_string(), "Fullscreen".to_string()));
        }
        InputMode::Tab => {
            keybinds.push(("←↓↑→".to_string(), "Move focus".to_string()));
            keybinds.push(("n".to_string(), "New".to_string()));
            keybinds.push(("x".to_string(), "Close".to_string()));
            keybinds.push(("r".to_string(), "Rename".to_string()));
            keybinds.push(("s".to_string(), "Sync".to_string()));
        }
        InputMode::Scroll => {
            keybinds.push(("↓↑".to_string(), "Scroll".to_string()));
            keybinds.push(("PgUp/PgDn".to_string(), "Scroll Page".to_string()));
        }
        InputMode::RenameTab => {
            keybinds.push(("Enter".to_string(), "when done".to_string()));
        }
        InputMode::Session => {
            keybinds.push(("d".to_string(), "Detach".to_string()));
        }
        InputMode::Search => {
            keybinds.push(("c".to_string(), "Change search text".to_string()));
        }
        InputMode::SearchInputTab => {
            keybinds.push(("Enter".to_string(), "when done".to_string()));
        }
    }
    ModeInfo {
        mode,
        keybinds,
        palette,
        capabilities,
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

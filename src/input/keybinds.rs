// This module is for mapping particular input keys to their corresponding actions.

use super::actions::{Action, Direction};
use super::handler::InputMode;

use std::collections::HashMap;

use strum::IntoEnumIterator;
use termion::event::Key;

type Keybinds = HashMap<InputMode, ModeKeybinds>;
type ModeKeybinds = HashMap<Key, Action>;

/// Populate the default hashmap of keybinds
/// @@@khs26 What about an input config file?
pub fn get_default_keybinds() -> Result<Keybinds, String> {
    let mut defaults = Keybinds::new();

    for mode in InputMode::iter() {
        defaults.insert(mode, get_defaults_for_mode(&mode)?);
    }

    Ok(defaults)
}

fn get_defaults_for_mode(mode: &InputMode) -> Result<ModeKeybinds, String> {
    let mut defaults = ModeKeybinds::new();

    match *mode {
        InputMode::Normal => {
            // Ctrl+G -> Command Mode
            defaults.insert(Key::Ctrl('g'), Action::SwitchToMode(InputMode::Command));
        }
        command_mode @ InputMode::Command | command_mode @ InputMode::CommandPersistent => {
            match command_mode {
                InputMode::Command => {
                    // Ctrl+G -> Command Mode (Persistent)
                    defaults.insert(
                        Key::Ctrl('g'),
                        Action::SwitchToMode(InputMode::CommandPersistent),
                    );
                }
                InputMode::CommandPersistent => {
                    // Ctrl+G -> Command Mode (Persistent)
                    defaults.insert(Key::Ctrl('g'), Action::SwitchToMode(InputMode::Normal));
                }
                _ => unreachable!(),
            }
            // Esc -> Normal Mode
            defaults.insert(Key::Esc, Action::SwitchToMode(InputMode::Normal));
            // Resize commands
            defaults.insert(Key::Char('j'), Action::Resize(Direction::Down));
            defaults.insert(Key::Char('k'), Action::Resize(Direction::Up));
            defaults.insert(Key::Char('h'), Action::Resize(Direction::Left));
            defaults.insert(Key::Char('l'), Action::Resize(Direction::Right));
            // Move pane commands
            defaults.insert(Key::Char('u'), Action::MoveFocus(Direction::Down));
            defaults.insert(Key::Char('i'), Action::MoveFocus(Direction::Up));
            defaults.insert(Key::Char('y'), Action::MoveFocus(Direction::Left));
            defaults.insert(Key::Char('o'), Action::MoveFocus(Direction::Right));
            // Switch focus
            // @@@ Currently just tab through panes - use right for this
            defaults.insert(Key::Char('p'), Action::SwitchFocus(Direction::Right));
            // Scroll
            defaults.insert(Key::PageUp, Action::ScrollUp);
            defaults.insert(Key::PageDown, Action::ScrollDown);
            // Tab controls
            defaults.insert(Key::Char('1'), Action::NewTab);
            defaults.insert(Key::Char('2'), Action::GoToNextTab);
            defaults.insert(Key::Char('3'), Action::GoToPreviousTab);
            defaults.insert(Key::Char('4'), Action::CloseTab);
            // New pane
            defaults.insert(Key::Char('z'), Action::NewPane(None));
            defaults.insert(Key::Char('b'), Action::NewPane(Some(Direction::Down)));
            defaults.insert(Key::Char('n'), Action::NewPane(Some(Direction::Right)));
            // Toggle focus fullscreen
            defaults.insert(Key::Char('e'), Action::ToggleFocusFullscreen);
            // Close pane
            defaults.insert(Key::Char('x'), Action::CloseFocus);
            // Close Mosaic
            defaults.insert(Key::Char('q'), Action::Quit);
        }
    }

    Ok(defaults)
}

pub fn key_to_action(key: &Key, input: Vec<u8>, mode: &InputMode, keybinds: &Keybinds) -> Action {
    if let Some(mode_keybinds) = keybinds.get(mode) {
        mode_keybinds
            .get(key)
            .cloned()
            .unwrap_or(Action::Write(input))
    } else {
        // Unrecognized mode - panic?
        panic!("Unrecognized mode: {:?}", mode);
    }
}

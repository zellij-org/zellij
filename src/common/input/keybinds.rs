// This module is for mapping particular input keys to their corresponding actions.

use super::actions::{Action, Direction};
use super::handler::InputMode;

use std::collections::HashMap;

use strum::IntoEnumIterator;
use termion::event::Key;

type Keybinds = HashMap<InputMode, ModeKeybinds>;
type ModeKeybinds = HashMap<Key, Action>;

/// Populates the default hashmap of keybinds.
/// @@@khs26 What about an input config file?
pub fn get_default_keybinds() -> Result<Keybinds, String> {
    let mut defaults = Keybinds::new();

    for mode in InputMode::iter() {
        defaults.insert(mode, get_defaults_for_mode(&mode)?);
    }

    Ok(defaults)
}

/// Returns the default keybinds for a givent [`InputMode`].
fn get_defaults_for_mode(mode: &InputMode) -> Result<ModeKeybinds, String> {
    let mut defaults = ModeKeybinds::new();

    match *mode {
        InputMode::Normal => {
            defaults.insert(Key::Ctrl('g'), Action::SwitchToMode(InputMode::Command));
        }
        InputMode::Command => {
            defaults.insert(Key::Char('r'), Action::SwitchToMode(InputMode::Resize));
            defaults.insert(Key::Char('p'), Action::SwitchToMode(InputMode::Pane));
            defaults.insert(Key::Char('t'), Action::SwitchToMode(InputMode::Tab));
            defaults.insert(Key::Char('s'), Action::SwitchToMode(InputMode::Scroll));
            defaults.insert(Key::Ctrl('g'), Action::TogglePersistentMode);
            defaults.insert(Key::Esc, Action::SwitchToMode(InputMode::Normal));
            defaults.insert(Key::Char('q'), Action::Quit);
        }
        InputMode::Resize => {
            defaults.insert(Key::Char('h'), Action::Resize(Direction::Left));
            defaults.insert(Key::Char('j'), Action::Resize(Direction::Down));
            defaults.insert(Key::Char('k'), Action::Resize(Direction::Up));
            defaults.insert(Key::Char('l'), Action::Resize(Direction::Right));

            defaults.insert(Key::Left, Action::Resize(Direction::Left));
            defaults.insert(Key::Down, Action::Resize(Direction::Down));
            defaults.insert(Key::Up, Action::Resize(Direction::Up));
            defaults.insert(Key::Right, Action::Resize(Direction::Right));

            defaults.insert(Key::Ctrl('b'), Action::Resize(Direction::Left));
            defaults.insert(Key::Ctrl('n'), Action::Resize(Direction::Down));
            defaults.insert(Key::Ctrl('p'), Action::Resize(Direction::Up));
            defaults.insert(Key::Ctrl('f'), Action::Resize(Direction::Right));

            defaults.insert(Key::Char('q'), Action::Quit);
            defaults.insert(Key::Ctrl('g'), Action::TogglePersistentMode);
            defaults.insert(Key::Esc, Action::SwitchToMode(InputMode::Normal));
        }
        InputMode::Pane => {
            defaults.insert(Key::Char('h'), Action::MoveFocus(Direction::Left));
            defaults.insert(Key::Char('j'), Action::MoveFocus(Direction::Down));
            defaults.insert(Key::Char('k'), Action::MoveFocus(Direction::Up));
            defaults.insert(Key::Char('l'), Action::MoveFocus(Direction::Right));

            defaults.insert(Key::Left, Action::MoveFocus(Direction::Left));
            defaults.insert(Key::Down, Action::MoveFocus(Direction::Down));
            defaults.insert(Key::Up, Action::MoveFocus(Direction::Up));
            defaults.insert(Key::Right, Action::MoveFocus(Direction::Right));

            defaults.insert(Key::Ctrl('b'), Action::MoveFocus(Direction::Left));
            defaults.insert(Key::Ctrl('n'), Action::MoveFocus(Direction::Down));
            defaults.insert(Key::Ctrl('p'), Action::MoveFocus(Direction::Up));
            defaults.insert(Key::Ctrl('f'), Action::MoveFocus(Direction::Right));

            defaults.insert(Key::Char('p'), Action::SwitchFocus(Direction::Right));
            defaults.insert(Key::Char('n'), Action::NewPane(None));
            defaults.insert(Key::Char('d'), Action::NewPane(Some(Direction::Down)));
            defaults.insert(Key::Char('r'), Action::NewPane(Some(Direction::Right)));
            defaults.insert(Key::Char('x'), Action::CloseFocus);

            defaults.insert(Key::Char('f'), Action::ToggleFocusFullscreen);

            defaults.insert(Key::Char('q'), Action::Quit);
            defaults.insert(Key::Ctrl('g'), Action::TogglePersistentMode);
            defaults.insert(Key::Esc, Action::SwitchToMode(InputMode::Normal));
        }
        InputMode::Tab => {
            defaults.insert(Key::Char('h'), Action::GoToPreviousTab);
            defaults.insert(Key::Char('j'), Action::GoToNextTab);
            defaults.insert(Key::Char('k'), Action::GoToPreviousTab);
            defaults.insert(Key::Char('l'), Action::GoToNextTab);

            defaults.insert(Key::Left, Action::GoToPreviousTab);
            defaults.insert(Key::Down, Action::GoToNextTab);
            defaults.insert(Key::Up, Action::GoToPreviousTab);
            defaults.insert(Key::Right, Action::GoToNextTab);

            defaults.insert(Key::Ctrl('b'), Action::GoToPreviousTab);
            defaults.insert(Key::Ctrl('n'), Action::GoToNextTab);
            defaults.insert(Key::Ctrl('p'), Action::GoToPreviousTab);
            defaults.insert(Key::Ctrl('f'), Action::GoToNextTab);

            defaults.insert(Key::Char('n'), Action::NewTab);
            defaults.insert(Key::Char('x'), Action::CloseTab);

            defaults.insert(Key::Char('q'), Action::Quit);
            defaults.insert(Key::Ctrl('g'), Action::TogglePersistentMode);
            defaults.insert(Key::Esc, Action::SwitchToMode(InputMode::Normal));
        }
        InputMode::Scroll => {
            defaults.insert(Key::Char('j'), Action::ScrollDown);
            defaults.insert(Key::Char('k'), Action::ScrollUp);

            defaults.insert(Key::Down, Action::ScrollDown);
            defaults.insert(Key::Up, Action::ScrollUp);

            defaults.insert(Key::Ctrl('n'), Action::ScrollDown);
            defaults.insert(Key::Ctrl('p'), Action::ScrollUp);

            defaults.insert(Key::Char('q'), Action::Quit);
            defaults.insert(Key::Ctrl('g'), Action::TogglePersistentMode);
            defaults.insert(Key::Esc, Action::SwitchToMode(InputMode::Normal));
        }
        InputMode::Exiting => {}
    }

    Ok(defaults)
}

/// Converts a [`Key`] terminal event to an [`Action`] according to the current
/// [`InputMode`] and [`Keybinds`].
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

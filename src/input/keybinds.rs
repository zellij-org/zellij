// This module is for mapping particular input keys to their corresponding actions.

use super::actions::{Action, Direction};
use super::handler::InputMode;
use super::hotkeys::BaseInputKey;
use super::hotkeys::InputKey;
use super::hotkeys::ModifierKey;

use std::collections::{HashMap, HashSet};

use strum::IntoEnumIterator;

type Keybinds = HashMap<InputKey, Action>;

/// Populate the default hashmap of keybinds
/// @@@khs26 What about an input config file?
fn get_defaults() -> Result<HashMap<InputMode, Keybinds>, String> {
    let mut defaults = HashMap::new();

    for mode in InputMode::iter() {
        defaults.insert(mode, get_defaults_for_mode(&mode)?);
    }

    //@@@khs26

    /*Quit,
    ToMode(InputMode),
    Resize(Direction),
    SwitchFocus(Direction),
    ScrollUp,
    ScrollDown,
    ToggleFocusFullscreen,
    NewPane(Direction),*/

    Ok(defaults)
}

fn get_defaults_for_mode(mode: &InputMode) -> Result<Keybinds, String> {
    let mut defaults = Keybinds::new();

    match *mode {
        InputMode::Normal => {
            // Ctrl+G -> Command Mode
            defaults.insert(
                InputKey::new(
                    BaseInputKey::G,
                    [ModifierKey::Control].iter().cloned().collect(),
                ),
                Action::ToMode(InputMode::Command),
            );
        }
        InputMode::Command => {
            // Ctrl+G -> Command Mode (Persistent)
            defaults.insert(
                InputKey::new(
                    BaseInputKey::G,
                    [ModifierKey::Control].iter().cloned().collect(),
                ),
                Action::ToMode(InputMode::CommandPersistent),
            );
            defaults.insert(
                InputKey::new(BaseInputKey::J, HashSet::new()),
                Action::Resize(Direction::Down),
            );
        }
        InputMode::CommandPersistent => {}
        InputMode::Exiting => {}
    }

    Ok(defaults)
}

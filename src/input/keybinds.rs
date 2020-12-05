// This module is for mapping particular input keys to their corresponding actions.

use super::hotkeys::InputKey;
use super::actions::{Action, Direction};

use std::collections::HashMap;

type Keybinds = HashMap<InputKey, Action>;

/// Populate the default hashmap of keybinds
/// @@@khs26 What about an input config file?
fn get_defaults() -> Result<Keybinds, String> {
    let defaults = Keybinds::new();

    //@@@khs26

    Ok(defaults)
}
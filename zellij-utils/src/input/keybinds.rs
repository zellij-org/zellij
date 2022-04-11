//! Mapping of inputs to sequences of actions.
use std::collections::HashMap;

use super::actions::Action;
use super::config;

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use zellij_tile::data::*;

/// Used in the config struct
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Keybinds(HashMap<InputMode, ModeKeybinds>);
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ModeKeybinds(HashMap<Key, Vec<Action>>);

/// List of keys, for which to disable their respective default actions
/// `All` is a catch all, and will disable the default actions for all keys.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(untagged)]
enum Unbind {
    // This is the correct order, don't rearrange!
    // Suspected Bug in the untagged macro.
    // 1. Keys
    Keys(Vec<Key>),
    // 2. All
    All(bool),
}

impl Default for Keybinds {
    // Use once per codepath
    // TODO investigate why
    fn default() -> Keybinds {
        Self::from_default_assets()
    }
}

impl Keybinds {
    pub fn new() -> Keybinds {
        Keybinds(HashMap::<InputMode, ModeKeybinds>::new())
    }

    fn from_default_assets() -> Keybinds {
        config::Config::from_default_assets()
            .expect("Keybinds from default assets Error!")
            .keybinds
    }

    /// Unbind [`Key`] bindings respective to their mode
    fn unbind_mode(&self, unbind: HashMap<InputMode, Unbind>) -> Keybinds {
        let mut keybinds = Keybinds::new();

        for mode in InputMode::iter() {
            if let Some(unbind) = unbind.get(&mode) {
                match unbind {
                    Unbind::All(true) => {}
                    Unbind::Keys(keys) => {
                        if let Some(defaults) = self.0.get(&mode) {
                            keybinds
                                .0
                                .insert(mode, defaults.clone().unbind_keys(keys.to_vec()));
                        }
                    }
                    Unbind::All(false) => {
                        if let Some(defaults) = self.0.get(&mode) {
                            keybinds.0.insert(mode, defaults.clone());
                        }
                    }
                }
            } else if let Some(defaults) = self.0.get(&mode) {
                keybinds.0.insert(mode, defaults.clone());
            }
        }
        keybinds
    }

    /// Merges two Keybinds structs into one Keybinds struct
    /// `other` overrides the ModeKeybinds of `self`.
    pub fn merge_keybinds(&self, other: Keybinds) -> Keybinds {
        let mut keybinds = Keybinds::new();

        for mode in InputMode::iter() {
            let mut mode_keybinds = ModeKeybinds::new();
            if let Some(keybind) = self.0.get(&mode) {
                mode_keybinds.0.extend(keybind.0.clone());
            };
            if let Some(keybind) = other.0.get(&mode) {
                mode_keybinds.0.extend(keybind.0.clone());
            }
            if !mode_keybinds.0.is_empty() {
                keybinds.0.insert(mode, mode_keybinds);
            }
        }
        keybinds
    }

    /// Converts a [`Key`] terminal event to a sequence of [`Action`]s according to the current
    /// [`InputMode`] and [`Keybinds`].
    pub fn key_to_actions(
        key: &Key,
        raw_bytes: Vec<u8>,
        mode: &InputMode,
        keybinds: &Keybinds,
    ) -> Vec<Action> {
        let mode_keybind_or_action = |action: Action| {
            keybinds
                .0
                .get(mode)
                .unwrap_or({
                    // create a dummy mode to recover from
                    &ModeKeybinds::new()
                })
                .0
                .get(key)
                .cloned()
                .unwrap_or_else(|| vec![action])
        };
        match *mode {
            InputMode::Normal | InputMode::Locked => {
                mode_keybind_or_action(Action::Write(raw_bytes))
            }
            InputMode::RenameTab => mode_keybind_or_action(Action::TabNameInput(raw_bytes)),
            InputMode::RenamePane => mode_keybind_or_action(Action::PaneNameInput(raw_bytes)),
            _ => mode_keybind_or_action(Action::NoOp),
        }
    }
}

impl ModeKeybinds {
    fn new() -> ModeKeybinds {
        ModeKeybinds(HashMap::<Key, Vec<Action>>::new())
    }

    /// Merges `self` with `other`, if keys are the same, `other` overwrites.
    fn merge(self, other: ModeKeybinds) -> ModeKeybinds {
        let mut merged = self;
        merged.0.extend(other.0);
        merged
    }

    /// Remove [`Key`]'s from [`ModeKeybinds`]
    fn unbind_keys(self, unbind: Vec<Key>) -> Self {
        let mut keymap = self;
        for key in unbind {
            keymap.0.remove(&key);
        }
        keymap
    }
}

impl Default for Unbind {
    fn default() -> Unbind {
        Unbind::All(false)
    }
}

// The unit test location.
#[cfg(test)]
#[path = "./unit/keybinds_test.rs"]
mod keybinds_test;

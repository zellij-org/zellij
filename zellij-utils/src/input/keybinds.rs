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

/// Intermediate struct used for deserialisation
/// Used in the config file.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct KeybindsFromYaml {
    #[serde(flatten)]
    keybinds: HashMap<InputMode, Vec<KeyActionUnbind>>,
    #[serde(default)]
    unbind: Unbind,
}

/// Intermediate enum used for deserialisation
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
enum KeyActionUnbind {
    KeyAction(KeyActionFromYaml),
    Unbind(UnbindFromYaml),
}

/// Intermediate struct used for deserialisation
#[derive(Clone, Debug, PartialEq, Deserialize)]
struct KeyActionUnbindFromYaml {
    keybinds: Vec<KeyActionFromYaml>,
    unbind: Unbind,
}

/// Intermediate struct used for deserialisation
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct KeyActionFromYaml {
    action: Vec<Action>,
    key: Vec<Key>,
}

/// Intermediate struct used for deserialisation
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
struct UnbindFromYaml {
    unbind: Unbind,
}

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

    /// Entrypoint from the config module
    pub fn get_default_keybinds_with_config(from_yaml: Option<KeybindsFromYaml>) -> Keybinds {
        let default_keybinds = match from_yaml.clone() {
            Some(keybinds) => match keybinds.unbind {
                Unbind::All(true) => Keybinds::new(),
                Unbind::All(false) | Unbind::Keys(_) => Keybinds::unbind(keybinds),
            },
            None => Keybinds::default(),
        };

        if let Some(keybinds) = from_yaml {
            default_keybinds.merge_keybinds(Keybinds::from(keybinds))
        } else {
            default_keybinds
        }
    }

    /// Unbinds the default keybindings in relation to their mode
    fn unbind(from_yaml: KeybindsFromYaml) -> Keybinds {
        let mut keybind_config = Self::new();
        let mut unbind_config: HashMap<InputMode, Unbind> = HashMap::new();
        let keybinds_from_yaml = from_yaml.keybinds;

        for mode in InputMode::iter() {
            if let Some(keybinds) = keybinds_from_yaml.get(&mode) {
                for keybind in keybinds.iter() {
                    match keybind {
                        KeyActionUnbind::Unbind(unbind) => {
                            unbind_config.insert(mode, unbind.unbind.clone());
                        }
                        KeyActionUnbind::KeyAction(key_action_from_yaml) => {
                            keybind_config
                                .0
                                .insert(mode, ModeKeybinds::from(key_action_from_yaml.clone()));
                        }
                    }
                }
            }
        }

        let mut default = Self::default().unbind_mode(unbind_config);

        // Toplevel Unbinds
        if let Unbind::Keys(_) = from_yaml.unbind {
            let mut unbind_config: HashMap<InputMode, Unbind> = HashMap::new();
            for mode in InputMode::iter() {
                unbind_config.insert(mode, from_yaml.unbind.clone());
            }
            default = default.unbind_mode(unbind_config);
        };

        default.merge_keybinds(keybind_config)
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
    fn merge_keybinds(&self, other: Keybinds) -> Keybinds {
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
                .unwrap_or_else(|| unreachable!("Unrecognized mode: {:?}", mode))
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

impl From<KeybindsFromYaml> for Keybinds {
    fn from(keybinds_from_yaml: KeybindsFromYaml) -> Keybinds {
        let mut keybinds = Keybinds::new();

        for mode in InputMode::iter() {
            let mut mode_keybinds = ModeKeybinds::new();
            for key_action in keybinds_from_yaml.keybinds.get(&mode).iter() {
                for keybind in key_action.iter() {
                    mode_keybinds = mode_keybinds.merge(ModeKeybinds::from(keybind.clone()));
                }
            }
            keybinds.0.insert(mode, mode_keybinds);
        }
        keybinds
    }
}

/// For each [`Key`] assigned to [`Action`]s,
/// map the [`Action`]s to the [`Key`]
impl From<KeyActionFromYaml> for ModeKeybinds {
    fn from(key_action: KeyActionFromYaml) -> ModeKeybinds {
        let actions = key_action.action;

        ModeKeybinds(
            key_action
                .key
                .into_iter()
                .map(|k| (k, actions.clone()))
                .collect::<HashMap<Key, Vec<Action>>>(),
        )
    }
}

impl From<KeyActionUnbind> for ModeKeybinds {
    fn from(key_action_unbind: KeyActionUnbind) -> ModeKeybinds {
        match key_action_unbind {
            KeyActionUnbind::KeyAction(key_action) => ModeKeybinds::from(key_action),
            KeyActionUnbind::Unbind(_) => ModeKeybinds::new(),
        }
    }
}

impl From<Vec<KeyActionFromYaml>> for ModeKeybinds {
    fn from(key_action_from_yaml: Vec<KeyActionFromYaml>) -> ModeKeybinds {
        let mut mode_keybinds = ModeKeybinds::new();

        for keybind in key_action_from_yaml {
            for key in keybind.key {
                mode_keybinds.0.insert(key, keybind.action.clone());
            }
        }
        mode_keybinds
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

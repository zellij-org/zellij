//! Mapping of inputs to sequences of actions.
use std::str::FromStr;
use kdl::{KdlDocument, KdlValue, KdlNode};
use std::collections::{BTreeMap, HashMap};

use super::actions::Action;
use super::config:: {self, ConfigError};
use crate::data::{InputMode, Key, KeybindsVec};

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use std::fmt;

/// Used in the config struct
#[derive(Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct Keybinds(pub HashMap<InputMode, HashMap<Key, Vec<Action>>>);

impl fmt::Debug for Keybinds {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut stable_sorted = BTreeMap::new();
        for (mode, keybinds) in self.0.iter() {
            let mut stable_sorted_mode_keybinds = BTreeMap::new();
            for (key, actions) in keybinds {
                stable_sorted_mode_keybinds.insert(key, actions);
            }
            stable_sorted.insert(mode, stable_sorted_mode_keybinds);
        }
        write!(f, "{:#?}", stable_sorted)
    }
}


// pub struct Keybinds(HashMap<InputMode, ModeKeybinds>);
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ModeKeybinds(BTreeMap<Key, Vec<Action>>);

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

impl Keybinds {
    pub fn get_actions_for_key_in_mode(&self, mode: &InputMode, key: &Key) -> Option<&Vec<Action>> {
        self.0.get(mode)
            .and_then(|normal_mode_keybindings| normal_mode_keybindings.get(key))
    }
    pub fn get_actions_for_key_in_mode_or_default_action(&self, mode: &InputMode, key: &Key, raw_bytes: Vec<u8>) -> Vec<Action> {
        self.0.get(mode)
            .and_then(|normal_mode_keybindings| normal_mode_keybindings.get(key))
            .cloned()
            .unwrap_or_else(|| {
                vec![self.default_action_for_mode(mode, raw_bytes)]
            })
    }
    pub fn get_input_mode_mut(&mut self, input_mode: &InputMode) -> &mut HashMap<Key, Vec<Action>> {
        self.0.entry(*input_mode).or_insert_with(HashMap::new)
    }
    pub fn default_action_for_mode(&self, mode: &InputMode, raw_bytes: Vec<u8>) -> Action {
        match *mode {
            InputMode::Normal | InputMode::Locked => Action::Write(raw_bytes),
            InputMode::RenameTab => Action::TabNameInput(raw_bytes),
            InputMode::RenamePane => Action::PaneNameInput(raw_bytes),
            InputMode::EnterSearch => Action::SearchInput(raw_bytes),
            _ => Action::NoOp,
        }
    }
    pub fn to_keybinds_vec(&self) -> KeybindsVec {
        let mut ret = vec![];
        for (mode, mode_binds) in &self.0 {
            let mut mode_binds_vec: Vec<(Key, Vec<Action>)> = vec![];
            for (key, actions) in mode_binds {
                mode_binds_vec.push((key.clone(), actions.clone()));
            }
            ret.push((*mode, mode_binds_vec))
        }
        ret
    }
}

impl ModeKeybinds {
    fn new() -> ModeKeybinds {
        ModeKeybinds(BTreeMap::<Key, Vec<Action>>::new())
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

    pub fn to_cloned_vec(&self) -> Vec<(Key, Vec<Action>)> {
        self.0
            .iter()
            .map(|(key, vac)| (*key, vac.clone()))
            .collect()
    }
}

// impl From<KeybindsFromYaml> for Keybinds {
//     fn from(keybinds_from_yaml: KeybindsFromYaml) -> Keybinds {
//         let mut keybinds = Keybinds::new();
//
//         for mode in InputMode::iter() {
//             let mut mode_keybinds = ModeKeybinds::new();
//             if let Some(key_action) = keybinds_from_yaml.keybinds.get(&mode) {
//                 for keybind in key_action {
//                     mode_keybinds = mode_keybinds.merge(ModeKeybinds::from(keybind.clone()));
//                 }
//             }
//             keybinds.0.insert(mode, mode_keybinds);
//         }
//         keybinds
//     }
// }

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
                .collect::<BTreeMap<Key, Vec<Action>>>(),
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

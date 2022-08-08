//! Mapping of inputs to sequences of actions.
use std::collections::HashMap;
use std::str::FromStr;
use kdl::{KdlDocument, KdlValue, KdlNode};

use super::actions::Action;
use super::config:: ConfigError;
use crate::input::{InputMode, Key};
use crate::{kdl_arg_is_truthy, kdl_children_or_error, kdl_string_arguments, kdl_argument_values, kdl_children, kdl_name, keys_from_kdl, actions_from_kdl, kdl_children_nodes_or_error};

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

/// Used in the config struct
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Default)]
pub struct Keybinds(pub HashMap<InputMode, HashMap<Key, Vec<Action>>>);
// pub struct Keybinds(HashMap<InputMode, ModeKeybinds>);
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

impl Keybinds {
    fn bind_actions_for_each_key(key_block: &KdlNode, input_mode_keybinds: &mut HashMap<Key, Vec<Action>>) -> Result<(), ConfigError>{
        let keys: Vec<Key> = keys_from_kdl!(key_block);
        let actions: Vec<Action> = actions_from_kdl!(key_block);
        for key in keys {
            input_mode_keybinds.insert(key, actions.clone());
        }
        Ok(())
    }
    fn unbind_keys(key_block: &KdlNode, input_mode_keybinds: &mut HashMap<Key, Vec<Action>>) -> Result<(), ConfigError>{
        let keys: Vec<Key> = keys_from_kdl!(key_block);
        for key in keys {
            input_mode_keybinds.remove(&key);
        }
        Ok(())
    }
    fn unbind_keys_in_all_modes(global_unbind: &KdlNode, keybinds_from_config: &mut Keybinds) -> Result<(), ConfigError> {
        let keys: Vec<Key> = keys_from_kdl!(global_unbind);
        for mode in keybinds_from_config.0.values_mut() {
            for key in &keys {
                mode.remove(&key);
            }
        }
        Ok(())
    }
    fn input_mode_keybindings <'a>(mode: &KdlNode, keybinds_from_config: &'a mut Keybinds) -> Result<&'a mut HashMap<Key, Vec<Action>>, ConfigError> {
        let mode_name = kdl_name!(mode);
        let input_mode = InputMode::from_str(mode_name)?;
        let input_mode_keybinds = keybinds_from_config.get_input_mode_mut(&input_mode);
        let clear_defaults_for_mode = kdl_arg_is_truthy!(mode, "clear-defaults");
        if clear_defaults_for_mode {
            input_mode_keybinds.clear();
        }
        Ok(input_mode_keybinds)
    }
    pub fn from_kdl(kdl_keybinds: &KdlNode, base_keybinds: Keybinds) -> Result<Self, ConfigError> {
        let clear_defaults = kdl_arg_is_truthy!(kdl_keybinds, "clear-defaults");
        let mut keybinds_from_config = if clear_defaults { Keybinds::default() } else { base_keybinds };
        for mode in kdl_children_nodes_or_error!(kdl_keybinds, "keybindings with no children") {
            if kdl_name!(mode) == "unbind" {
                continue;
            }
            let mut input_mode_keybinds = Keybinds::input_mode_keybindings(mode, &mut keybinds_from_config)?;
            let bind_nodes = kdl_children_nodes_or_error!(mode, "no keybinding block for mode").iter().filter(|n| kdl_name!(n) == "bind");
            let unbind_nodes = kdl_children_nodes_or_error!(mode, "no keybinding block for mode").iter().filter(|n| kdl_name!(n) == "unbind");
            for key_block in bind_nodes {
                Keybinds::bind_actions_for_each_key(key_block, &mut input_mode_keybinds)?;
            }
            // we loop twice so that the unbinds always happen after the binds
            for key_block in unbind_nodes {
                Keybinds::unbind_keys(key_block, &mut input_mode_keybinds)?;
            }
        }
        if let Some(global_unbind) = kdl_keybinds.children().and_then(|c| c.get("unbind")) {
            Keybinds::unbind_keys_in_all_modes(global_unbind, &mut keybinds_from_config)?;
        };
        Ok(keybinds_from_config)
    }
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
            _ => Action::NoOp,
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

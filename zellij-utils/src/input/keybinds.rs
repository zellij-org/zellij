use std::collections::{BTreeMap, HashMap};

use super::actions::Action;
use crate::data::{InputMode, Key, KeybindsVec};

use serde::{Deserialize, Serialize};
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

impl Keybinds {
    pub fn get_actions_for_key_in_mode(&self, mode: &InputMode, key: &Key) -> Option<&Vec<Action>> {
        self.0
            .get(mode)
            .and_then(|normal_mode_keybindings| normal_mode_keybindings.get(key))
    }
    pub fn get_actions_for_key_in_mode_or_default_action(
        &self,
        mode: &InputMode,
        key: &Key,
        raw_bytes: Vec<u8>,
    ) -> Vec<Action> {
        self.0
            .get(mode)
            .and_then(|normal_mode_keybindings| normal_mode_keybindings.get(key))
            .cloned()
            .unwrap_or_else(|| vec![self.default_action_for_mode(mode, raw_bytes)])
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
    pub fn merge(&mut self, mut other: Keybinds) {
        for (other_input_mode, mut other_input_mode_keybinds) in other.0.drain() {
            let input_mode_keybinds = self
                .0
                .entry(other_input_mode)
                .or_insert_with(|| Default::default());
            for (other_action, other_action_keybinds) in other_input_mode_keybinds.drain() {
                input_mode_keybinds.insert(other_action, other_action_keybinds);
            }
        }
    }
}

// The unit test location.
#[cfg(test)]
#[path = "./unit/keybinds_test.rs"]
mod keybinds_test;

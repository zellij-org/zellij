//! Mapping of inputs to sequences of actions.
use std::collections::HashMap;

use super::actions::{Action, Direction};
use super::handler::InputMode;

use serde::Deserialize;
use strum::IntoEnumIterator;
use termion::event::Key;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Keybinds(HashMap<InputMode, ModeKeybinds>);
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ModeKeybinds(HashMap<Key, Vec<Action>>);

/// Intermediate struct for configuration.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct MultipleKeybinds(HashMap<InputMode, Vec<ModeKeybind>>);
/// Intermediate struct for configuration.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
struct MultipleModeKeybinds(HashMap<Vec<Key>, Vec<Action>>);

/// Intermediate enum for configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
enum ModeKeybind {
    ModeKeybinds(ModeKeybinds),
    MultipleModeKeybinds(MultipleModeKeybinds),
}

impl Default for Keybinds {
    fn default() -> Keybinds {
        Keybinds::get_default_keybinds().expect("Something is wrong with the default Keybinds")
    }
}

impl Keybinds {
    pub fn new() -> Keybinds {
        Keybinds::from(HashMap::<InputMode, ModeKeybinds>::new())
    }

    pub fn get_default_keybinds() -> Result<Keybinds, String> {
        let mut defaults = Keybinds::new();

        for mode in InputMode::iter() {
            defaults
                .0
                .insert(mode, Keybinds::get_defaults_for_mode(&mode)?);
        }

        Ok(defaults)
    }

    pub fn get_default_keybinds_with_config(keybinds: Option<MultipleKeybinds>) -> Keybinds {
        let default_keybinds = Keybinds::default();
        if let Some(keybinds) = keybinds {
            default_keybinds.merge_keybinds(Keybinds::from(keybinds))
        } else {
            default_keybinds
        }
    }

    /// Merges two Keybinds structs into one Keybinds struct
    /// `other` overrides the ModeKeybinds of `self`.
    fn merge_keybinds(&self, other: Keybinds) -> Keybinds {
        let mut keybinds = Keybinds::default();

        for mode in InputMode::iter() {
            let mut mode_keybinds: ModeKeybinds = if let Some(keybind) = self.0.get(&mode) {
                keybind.clone()
            } else {
                ModeKeybinds::default()
            };
            if let Some(keybind) = other.0.get(&mode) {
                mode_keybinds.0.extend(keybind.0.clone());
            }
            keybinds.0.insert(mode, mode_keybinds);
        }
        keybinds
    }

    /// Returns the default keybinds for a given [`InputMode`].
    fn get_defaults_for_mode(mode: &InputMode) -> Result<ModeKeybinds, String> {
        let mut defaults = HashMap::new();

        match *mode {
            InputMode::Normal => {
                defaults.insert(
                    Key::Ctrl('g'),
                    vec![Action::SwitchToMode(InputMode::Command)],
                );
            }
            InputMode::Command => {
                defaults.insert(
                    Key::Char('r'),
                    vec![Action::SwitchToMode(InputMode::Resize)],
                );
                defaults.insert(Key::Char('p'), vec![Action::SwitchToMode(InputMode::Pane)]);
                defaults.insert(Key::Char('t'), vec![Action::SwitchToMode(InputMode::Tab)]);
                defaults.insert(
                    Key::Char('s'),
                    vec![Action::SwitchToMode(InputMode::Scroll)],
                );
                defaults.insert(
                    Key::Ctrl('g'),
                    vec![Action::SwitchToMode(InputMode::Normal)],
                );
                defaults.insert(Key::Esc, vec![Action::SwitchToMode(InputMode::Normal)]);
                defaults.insert(Key::Char('q'), vec![Action::Quit]);
            }
            InputMode::Resize => {
                defaults.insert(Key::Char('h'), vec![Action::Resize(Direction::Left)]);
                defaults.insert(Key::Char('j'), vec![Action::Resize(Direction::Down)]);
                defaults.insert(Key::Char('k'), vec![Action::Resize(Direction::Up)]);
                defaults.insert(Key::Char('l'), vec![Action::Resize(Direction::Right)]);

                defaults.insert(Key::Left, vec![Action::Resize(Direction::Left)]);
                defaults.insert(Key::Down, vec![Action::Resize(Direction::Down)]);
                defaults.insert(Key::Up, vec![Action::Resize(Direction::Up)]);
                defaults.insert(Key::Right, vec![Action::Resize(Direction::Right)]);

                defaults.insert(Key::Ctrl('b'), vec![Action::Resize(Direction::Left)]);
                defaults.insert(Key::Ctrl('n'), vec![Action::Resize(Direction::Down)]);
                defaults.insert(Key::Ctrl('p'), vec![Action::Resize(Direction::Up)]);
                defaults.insert(Key::Ctrl('f'), vec![Action::Resize(Direction::Right)]);

                defaults.insert(Key::Char('q'), vec![Action::Quit]);
                defaults.insert(
                    Key::Ctrl('g'),
                    vec![Action::SwitchToMode(InputMode::Normal)],
                );
                defaults.insert(Key::Esc, vec![Action::SwitchToMode(InputMode::Command)]);
            }
            InputMode::Pane => {
                defaults.insert(Key::Char('h'), vec![Action::MoveFocus(Direction::Left)]);
                defaults.insert(Key::Char('j'), vec![Action::MoveFocus(Direction::Down)]);
                defaults.insert(Key::Char('k'), vec![Action::MoveFocus(Direction::Up)]);
                defaults.insert(Key::Char('l'), vec![Action::MoveFocus(Direction::Right)]);

                defaults.insert(Key::Left, vec![Action::MoveFocus(Direction::Left)]);
                defaults.insert(Key::Down, vec![Action::MoveFocus(Direction::Down)]);
                defaults.insert(Key::Up, vec![Action::MoveFocus(Direction::Up)]);
                defaults.insert(Key::Right, vec![Action::MoveFocus(Direction::Right)]);

                defaults.insert(Key::Ctrl('b'), vec![Action::MoveFocus(Direction::Left)]);
                defaults.insert(Key::Ctrl('n'), vec![Action::MoveFocus(Direction::Down)]);
                defaults.insert(Key::Ctrl('p'), vec![Action::MoveFocus(Direction::Up)]);
                defaults.insert(Key::Ctrl('f'), vec![Action::MoveFocus(Direction::Right)]);

                defaults.insert(Key::Char('p'), vec![Action::SwitchFocus(Direction::Right)]);
                defaults.insert(Key::Char('n'), vec![Action::NewPane(None)]);
                defaults.insert(Key::Char('d'), vec![Action::NewPane(Some(Direction::Down))]);
                defaults.insert(
                    Key::Char('r'),
                    vec![Action::NewPane(Some(Direction::Right))],
                );
                defaults.insert(Key::Char('x'), vec![Action::CloseFocus]);

                defaults.insert(Key::Char('f'), vec![Action::ToggleFocusFullscreen]);

                defaults.insert(Key::Char('q'), vec![Action::Quit]);
                defaults.insert(
                    Key::Ctrl('g'),
                    vec![Action::SwitchToMode(InputMode::Normal)],
                );
                defaults.insert(Key::Esc, vec![Action::SwitchToMode(InputMode::Command)]);
            }
            InputMode::Tab => {
                defaults.insert(Key::Char('h'), vec![Action::GoToPreviousTab]);
                defaults.insert(Key::Char('j'), vec![Action::GoToNextTab]);
                defaults.insert(Key::Char('k'), vec![Action::GoToPreviousTab]);
                defaults.insert(Key::Char('l'), vec![Action::GoToNextTab]);

                defaults.insert(Key::Left, vec![Action::GoToPreviousTab]);
                defaults.insert(Key::Down, vec![Action::GoToNextTab]);
                defaults.insert(Key::Up, vec![Action::GoToPreviousTab]);
                defaults.insert(Key::Right, vec![Action::GoToNextTab]);

                defaults.insert(Key::Ctrl('b'), vec![Action::GoToPreviousTab]);
                defaults.insert(Key::Ctrl('n'), vec![Action::GoToNextTab]);
                defaults.insert(Key::Ctrl('p'), vec![Action::GoToPreviousTab]);
                defaults.insert(Key::Ctrl('f'), vec![Action::GoToNextTab]);

                defaults.insert(Key::Char('n'), vec![Action::NewTab]);
                defaults.insert(Key::Char('x'), vec![Action::CloseTab]);

                defaults.insert(Key::Char('q'), vec![Action::Quit]);
                defaults.insert(
                    Key::Ctrl('g'),
                    vec![Action::SwitchToMode(InputMode::Normal)],
                );
                for i in '1'..='9' {
                    defaults.insert(Key::Char(i), vec![Action::GoToTab(i.to_digit(10).unwrap())]);
                }
                defaults.insert(Key::Esc, vec![Action::SwitchToMode(InputMode::Command)]);
            }
            InputMode::Scroll => {
                defaults.insert(Key::Char('j'), vec![Action::ScrollDown]);
                defaults.insert(Key::Char('k'), vec![Action::ScrollUp]);

                defaults.insert(Key::Down, vec![Action::ScrollDown]);
                defaults.insert(Key::Up, vec![Action::ScrollUp]);

                defaults.insert(Key::Ctrl('n'), vec![Action::ScrollDown]);
                defaults.insert(Key::Ctrl('p'), vec![Action::ScrollUp]);

                defaults.insert(Key::Char('q'), vec![Action::Quit]);
                defaults.insert(
                    Key::Ctrl('g'),
                    vec![Action::SwitchToMode(InputMode::Normal)],
                );
                defaults.insert(Key::Esc, vec![Action::SwitchToMode(InputMode::Command)]);
            }
        }

        Ok(ModeKeybinds::from(defaults))
    }

    /// Converts a [`Key`] terminal event to a sequence of [`Action`]s according to the current
    /// [`InputMode`] and [`Keybinds`].
    pub fn key_to_actions(
        key: &Key,
        input: Vec<u8>,
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
            InputMode::Normal => mode_keybind_or_action(Action::Write(input)),
            _ => mode_keybind_or_action(Action::NoOp),
        }
    }
}

impl ModeKeybinds {
    pub fn new() -> ModeKeybinds {
        ModeKeybinds::from(HashMap::<Key, Vec<Action>>::new())
    }

    /// Merges `self` with `other`, if keys are the same, `other` overwrites.
    pub fn merge(self, other: ModeKeybinds) -> ModeKeybinds {
        let mut merged = self;
        merged.0.extend(other.0.clone());
        merged
    }
}

/// Converts from the `MultipleKeybinds` struct to a `Keybinds` struct.
/// TODO @a-kenji Error on conflicting keybinds?
/// Atm the Lower Keybind will take precedence and will overwrite the
/// one further up.
impl From<MultipleKeybinds> for Keybinds {
    fn from(multiple: MultipleKeybinds) -> Keybinds {
        let mut keybinds = Keybinds::new();

        for mode in InputMode::iter() {
            let mut mode_keybinds = ModeKeybinds::new();
            for mode_keybind in multiple.0.get(&mode).iter() {
                for keybind in mode_keybind.iter() {
                    match keybind {
                        ModeKeybind::ModeKeybinds(k) => {
                            mode_keybinds = mode_keybinds.clone().merge(k.clone());
                        }
                        ModeKeybind::MultipleModeKeybinds(k) => {
                            mode_keybinds =
                                mode_keybinds.clone().merge(ModeKeybinds::from(k.clone()));
                        }
                    }
                }
            }
            keybinds.0.insert(mode, mode_keybinds);
        }
        keybinds
    }
}

impl From<HashMap<InputMode, ModeKeybinds>> for Keybinds {
    fn from(map: HashMap<InputMode, ModeKeybinds>) -> Keybinds {
        Keybinds(map)
    }
}
impl From<HashMap<Key, Vec<Action>>> for ModeKeybinds {
    fn from(map: HashMap<Key, Vec<Action>>) -> ModeKeybinds {
        ModeKeybinds(map)
    }
}

impl From<MultipleModeKeybinds> for ModeKeybinds {
    fn from(multiple: MultipleModeKeybinds) -> ModeKeybinds {
        let single: HashMap<Key, Vec<Action>> = multiple
            .0
            .into_iter()
            .flat_map(|(k, v)| (k.into_iter().map(move |k| (k, v.clone()))))
            .collect();
        ModeKeybinds::from(single)
    }
}

// The unit test location.
#[cfg(test)]
#[path = "./ut/keybinds_test.rs"]
mod keybinds_test;

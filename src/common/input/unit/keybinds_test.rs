use super::super::keybinds::*;
use zellij_tile::{actions::Action, data::Key};

#[test]
fn merge_keybinds_merges_different_keys() {
    let mut mode_keybinds_self = ModeKeybinds::new();
    mode_keybinds_self.0.insert(Key::F(1), vec![Action::NoOp]);
    let mut mode_keybinds_other = ModeKeybinds::new();
    mode_keybinds_other
        .0
        .insert(Key::Backspace, vec![Action::NoOp]);

    let mut mode_keybinds_expected = ModeKeybinds::new();
    mode_keybinds_expected
        .0
        .insert(Key::F(1), vec![Action::NoOp]);
    mode_keybinds_expected
        .0
        .insert(Key::Backspace, vec![Action::NoOp]);

    let mode_keybinds_merged = mode_keybinds_self.merge(mode_keybinds_other);

    assert_eq!(mode_keybinds_expected, mode_keybinds_merged);
}

#[test]
fn merge_mode_keybinds_overwrites_same_keys() {
    let mut mode_keybinds_self = ModeKeybinds::new();
    mode_keybinds_self.0.insert(Key::F(1), vec![Action::NoOp]);
    let mut mode_keybinds_other = ModeKeybinds::new();
    mode_keybinds_other
        .0
        .insert(Key::F(1), vec![Action::GoToTab(1)]);

    let mut mode_keybinds_expected = ModeKeybinds::new();
    mode_keybinds_expected
        .0
        .insert(Key::F(1), vec![Action::GoToTab(1)]);

    let mode_keybinds_merged = mode_keybinds_self.merge(mode_keybinds_other);

    assert_eq!(mode_keybinds_expected, mode_keybinds_merged);
}

#[test]
fn merge_keybinds_merges() {
    let mut mode_keybinds_self = ModeKeybinds::new();
    mode_keybinds_self.0.insert(Key::F(1), vec![Action::NoOp]);
    let mut mode_keybinds_other = ModeKeybinds::new();
    mode_keybinds_other
        .0
        .insert(Key::Backspace, vec![Action::NoOp]);
    let mut keybinds_self = Keybinds::new();
    keybinds_self
        .0
        .insert(InputMode::Normal, mode_keybinds_self.clone());
    let mut keybinds_other = Keybinds::new();
    keybinds_other
        .0
        .insert(InputMode::Resize, mode_keybinds_other.clone());
    let mut keybinds_expected = Keybinds::new();
    keybinds_expected
        .0
        .insert(InputMode::Normal, mode_keybinds_self);
    keybinds_expected
        .0
        .insert(InputMode::Resize, mode_keybinds_other);

    assert_eq!(
        keybinds_expected,
        keybinds_self.merge_keybinds(keybinds_other)
    )
}

#[test]
fn merge_keybinds_overwrites_same_keys() {
    let mut mode_keybinds_self = ModeKeybinds::new();
    mode_keybinds_self.0.insert(Key::F(1), vec![Action::NoOp]);
    mode_keybinds_self.0.insert(Key::F(2), vec![Action::NoOp]);
    mode_keybinds_self.0.insert(Key::F(3), vec![Action::NoOp]);
    let mut mode_keybinds_other = ModeKeybinds::new();
    mode_keybinds_other
        .0
        .insert(Key::F(1), vec![Action::GoToTab(1)]);
    mode_keybinds_other
        .0
        .insert(Key::F(2), vec![Action::GoToTab(2)]);
    mode_keybinds_other
        .0
        .insert(Key::F(3), vec![Action::GoToTab(3)]);
    let mut keybinds_self = Keybinds::new();
    keybinds_self
        .0
        .insert(InputMode::Normal, mode_keybinds_self.clone());
    let mut keybinds_other = Keybinds::new();
    keybinds_other
        .0
        .insert(InputMode::Normal, mode_keybinds_other.clone());
    let mut keybinds_expected = Keybinds::new();
    keybinds_expected
        .0
        .insert(InputMode::Normal, mode_keybinds_other);

    assert_eq!(
        keybinds_expected,
        keybinds_self.merge_keybinds(keybinds_other)
    )
}

#[test]
fn from_keyaction_from_yaml_to_mode_keybindings() {
    let actions = vec![Action::NoOp, Action::GoToTab(1)];
    let keyaction = KeyActionFromYaml {
        action: actions.clone(),
        key: vec![Key::F(1), Key::Backspace, Key::Char('t')],
    };

    let mut expected = ModeKeybinds::new();
    expected.0.insert(Key::F(1), actions.clone());
    expected.0.insert(Key::Backspace, actions.clone());
    expected.0.insert(Key::Char('t'), actions);

    assert_eq!(expected, ModeKeybinds::from(keyaction));
}

#[test]
fn toplevel_unbind_unbinds_all() {
    let from_yaml = KeybindsFromYaml {
        unbind: Unbind::All(true),
        keybinds: HashMap::new(),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));

    assert_eq!(keybinds_from_yaml, Keybinds::new());
}

fn no_unbind_unbinds_none() {
    let from_yaml = KeybindsFromYaml {
        unbind: Unbind::All(false),
        keybinds: HashMap::new(),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));

    assert_eq!(keybinds_from_yaml, Keybinds::new());
}

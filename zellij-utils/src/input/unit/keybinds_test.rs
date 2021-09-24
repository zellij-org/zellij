use super::super::actions::*;
use super::super::keybinds::*;
use zellij_tile::data::Key;

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
        .insert(InputMode::Normal, mode_keybinds_self);
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

#[test]
fn no_unbind_unbinds_none() {
    let from_yaml = KeybindsFromYaml {
        unbind: Unbind::All(false),
        keybinds: HashMap::new(),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));

    assert_eq!(keybinds_from_yaml, Keybinds::default());
}

#[test]
fn last_keybind_is_taken() {
    let actions_1 = vec![Action::NoOp, Action::NewTab(None)];
    let keyaction_1 = KeyActionFromYaml {
        action: actions_1,
        key: vec![Key::F(1), Key::Backspace, Key::Char('t')],
    };
    let actions_2 = vec![Action::GoToTab(1)];
    let keyaction_2 = KeyActionFromYaml {
        action: actions_2.clone(),
        key: vec![Key::F(1), Key::Backspace, Key::Char('t')],
    };

    let mut expected = ModeKeybinds::new();
    expected.0.insert(Key::F(1), actions_2.clone());
    expected.0.insert(Key::Backspace, actions_2.clone());
    expected.0.insert(Key::Char('t'), actions_2);

    assert_eq!(expected, ModeKeybinds::from(vec![keyaction_1, keyaction_2]));
}

#[test]
fn last_keybind_overwrites() {
    let actions_1 = vec![Action::NoOp, Action::NewTab(None)];
    let keyaction_1 = KeyActionFromYaml {
        action: actions_1.clone(),
        key: vec![Key::F(1), Key::Backspace, Key::Char('t')],
    };
    let actions_2 = vec![Action::GoToTab(1)];
    let keyaction_2 = KeyActionFromYaml {
        action: actions_2.clone(),
        key: vec![Key::F(1), Key::Char('t')],
    };

    let mut expected = ModeKeybinds::new();
    expected.0.insert(Key::F(1), actions_2.clone());
    expected.0.insert(Key::Backspace, actions_1);
    expected.0.insert(Key::Char('t'), actions_2);

    assert_eq!(expected, ModeKeybinds::from(vec![keyaction_1, keyaction_2]));
}

#[test]
fn unbind_single_mode() {
    let unbind = Unbind::All(true);
    let unbind_from_yaml = UnbindFromYaml { unbind };
    let key_action_unbinds = vec![KeyActionUnbind::Unbind(unbind_from_yaml)];

    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbinds);

    let keybinds_from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::All(false),
    };

    let keybinds = Keybinds::unbind(keybinds_from_yaml);
    let result = keybinds.0.get(&InputMode::Normal);
    assert!(result.is_none());
}

#[test]
fn unbind_multiple_modes() {
    let unbind = Unbind::All(true);
    let unbind_from_yaml = UnbindFromYaml { unbind };
    let key_action_unbinds = vec![KeyActionUnbind::Unbind(unbind_from_yaml)];

    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbinds.clone());
    keys.insert(InputMode::Pane, key_action_unbinds);

    let keybinds_from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::All(false),
    };

    let keybinds = Keybinds::unbind(keybinds_from_yaml);
    let normal = keybinds.0.get(&InputMode::Normal);
    let pane = keybinds.0.get(&InputMode::Pane);
    assert!(normal.is_none());
    assert!(pane.is_none());
}

#[test]
fn unbind_single_keybind_single_mode() {
    let unbind = Unbind::Keys(vec![Key::Alt('n')]);
    let unbind_from_yaml = UnbindFromYaml { unbind };
    let key_action_unbinds = vec![KeyActionUnbind::Unbind(unbind_from_yaml)];

    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbinds);

    let keybinds_from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::All(false),
    };

    let keybinds = Keybinds::unbind(keybinds_from_yaml);
    let mode_keybinds = keybinds.0.get(&InputMode::Normal);
    let result = mode_keybinds
        .expect("Mode shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    assert!(result.is_none());
}

#[test]
fn unbind_single_keybind_multiple_modes() {
    let unbind_n = Unbind::Keys(vec![Key::Alt('n')]);
    let unbind_h = Unbind::Keys(vec![Key::Alt('h')]);
    let unbind_from_yaml_n = UnbindFromYaml { unbind: unbind_n };
    let unbind_from_yaml_h = UnbindFromYaml { unbind: unbind_h };
    let key_action_unbinds_n = vec![KeyActionUnbind::Unbind(unbind_from_yaml_n)];
    let key_action_unbinds_h = vec![KeyActionUnbind::Unbind(unbind_from_yaml_h)];

    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbinds_n);
    keys.insert(InputMode::Pane, key_action_unbinds_h);

    let keybinds_from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::All(false),
    };

    let keybinds = Keybinds::unbind(keybinds_from_yaml);
    let normal = keybinds.0.get(&InputMode::Normal);
    let pane = keybinds.0.get(&InputMode::Pane);
    let result_normal = normal
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_pane = pane.expect("Mode shouldn't be empty").0.get(&Key::Alt('h'));
    assert!(result_normal.is_none());
    assert!(result_pane.is_none());
}

#[test]
fn unbind_multiple_keybinds_single_mode() {
    let unbind = Unbind::Keys(vec![Key::Alt('n'), Key::Ctrl('p')]);
    let unbind_from_yaml = UnbindFromYaml { unbind };
    let key_action_unbinds = vec![KeyActionUnbind::Unbind(unbind_from_yaml)];

    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbinds);

    let keybinds_from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::All(false),
    };

    let keybinds = Keybinds::unbind(keybinds_from_yaml);
    let mode_keybinds = keybinds.0.get(&InputMode::Normal);
    let result_n = mode_keybinds
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_p = mode_keybinds
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Ctrl('p'));
    assert!(result_n.is_none());
    assert!(result_p.is_none());
}

#[test]
fn unbind_multiple_keybinds_multiple_modes() {
    let unbind_normal = Unbind::Keys(vec![Key::Alt('n'), Key::Ctrl('p')]);
    let unbind_resize = Unbind::Keys(vec![Key::Char('h'), Key::Ctrl('r')]);
    let unbind_from_yaml_normal = UnbindFromYaml {
        unbind: unbind_normal,
    };
    let unbind_from_yaml_resize = UnbindFromYaml {
        unbind: unbind_resize,
    };
    let key_action_unbinds_normal = vec![KeyActionUnbind::Unbind(unbind_from_yaml_normal)];
    let key_action_unbinds_resize = vec![KeyActionUnbind::Unbind(unbind_from_yaml_resize)];

    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbinds_normal);
    keys.insert(InputMode::Resize, key_action_unbinds_resize);

    let keybinds_from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::All(false),
    };

    let keybinds = Keybinds::unbind(keybinds_from_yaml);
    let mode_keybinds_normal = keybinds.0.get(&InputMode::Normal);
    let mode_keybinds_resize = keybinds.0.get(&InputMode::Resize);
    let result_normal_1 = mode_keybinds_normal
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_normal_2 = mode_keybinds_normal
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Ctrl('p'));
    let result_resize_1 = mode_keybinds_resize
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Char('h'));
    let result_resize_2 = mode_keybinds_resize
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Ctrl('r'));
    assert!(result_normal_1.is_none());
    assert!(result_resize_1.is_none());
    assert!(result_normal_2.is_none());
    assert!(result_resize_2.is_none());
}

#[test]
fn unbind_multiple_keybinds_all_modes() {
    let unbind = Unbind::Keys(vec![Key::Alt('n'), Key::Alt('h')]);
    let keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    let keybinds_from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind,
    };

    let keybinds = Keybinds::unbind(keybinds_from_yaml);
    let mode_keybinds_normal = keybinds.0.get(&InputMode::Normal);
    let mode_keybinds_resize = keybinds.0.get(&InputMode::Resize);
    let result_normal_1 = mode_keybinds_normal
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_normal_2 = mode_keybinds_normal
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Ctrl('h'));
    let result_resize_1 = mode_keybinds_resize
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Char('n'));
    let result_resize_2 = mode_keybinds_resize
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Ctrl('h'));
    assert!(result_normal_1.is_none());
    assert!(result_resize_1.is_none());
    assert!(result_normal_2.is_none());
    assert!(result_resize_2.is_none());
}

#[test]
fn unbind_all_toplevel_single_key_single_mode() {
    let unbind = Unbind::Keys(vec![Key::Alt('h')]);
    let unbind_from_yaml = UnbindFromYaml { unbind };
    let key_action_unbinds_normal = vec![KeyActionUnbind::Unbind(unbind_from_yaml)];
    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbinds_normal);
    let from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::All(true),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));
    assert_eq!(keybinds_from_yaml, Keybinds::new());
}

#[test]
fn unbind_all_toplevel_single_key_multiple_modes() {
    let unbind_n = Unbind::Keys(vec![Key::Alt('n')]);
    let unbind_h = Unbind::Keys(vec![Key::Alt('h')]);
    let unbind_from_yaml_n = UnbindFromYaml { unbind: unbind_n };
    let unbind_from_yaml_h = UnbindFromYaml { unbind: unbind_h };
    let key_action_unbinds_normal = vec![KeyActionUnbind::Unbind(unbind_from_yaml_n)];
    let key_action_unbinds_pane = vec![KeyActionUnbind::Unbind(unbind_from_yaml_h)];
    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbinds_normal);
    keys.insert(InputMode::Pane, key_action_unbinds_pane);
    let from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::All(true),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));
    assert_eq!(keybinds_from_yaml, Keybinds::new());
}

#[test]
fn unbind_all_toplevel_multiple_key_multiple_modes() {
    let unbind_n = Unbind::Keys(vec![Key::Alt('n'), Key::Ctrl('p')]);
    let unbind_h = Unbind::Keys(vec![Key::Alt('h'), Key::Ctrl('t')]);
    let unbind_from_yaml_n = UnbindFromYaml { unbind: unbind_n };
    let unbind_from_yaml_h = UnbindFromYaml { unbind: unbind_h };
    let key_action_unbinds_normal = vec![KeyActionUnbind::Unbind(unbind_from_yaml_n)];
    let key_action_unbinds_pane = vec![KeyActionUnbind::Unbind(unbind_from_yaml_h)];
    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbinds_normal);
    keys.insert(InputMode::Pane, key_action_unbinds_pane);
    let from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::All(true),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));
    assert_eq!(keybinds_from_yaml, Keybinds::new());
}

#[test]
fn unbind_all_toplevel_all_key_multiple_modes() {
    let unbind = Unbind::All(true);
    let unbind_from_yaml = UnbindFromYaml { unbind };
    let key_action_unbinds_normal = vec![KeyActionUnbind::Unbind(unbind_from_yaml.clone())];
    let key_action_unbinds_pane = vec![KeyActionUnbind::Unbind(unbind_from_yaml)];
    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbinds_normal);
    keys.insert(InputMode::Pane, key_action_unbinds_pane);
    let from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::All(true),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));
    assert_eq!(keybinds_from_yaml, Keybinds::new());
}

#[test]
fn unbind_single_keybind_all_modes() {
    let keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    let from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::Keys(vec![Key::Alt('n')]),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));

    let result_normal = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_pane = keybinds_from_yaml
        .0
        .get(&InputMode::Pane)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_resize = keybinds_from_yaml
        .0
        .get(&InputMode::Resize)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_tab = keybinds_from_yaml
        .0
        .get(&InputMode::Tab)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));

    assert!(result_normal.is_none());
    assert!(result_pane.is_none());
    assert!(result_resize.is_none());
    assert!(result_tab.is_none());
}

#[test]
fn unbind_single_toplevel_single_key_single_mode_identical() {
    let unbind = Unbind::Keys(vec![Key::Alt('n')]);
    let unbind_from_yaml = UnbindFromYaml { unbind };
    let key_action_unbind = vec![KeyActionUnbind::Unbind(unbind_from_yaml)];
    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbind);
    let from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::Keys(vec![Key::Alt('n')]),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));

    let result_normal = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_pane = keybinds_from_yaml
        .0
        .get(&InputMode::Pane)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_resize = keybinds_from_yaml
        .0
        .get(&InputMode::Resize)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_tab = keybinds_from_yaml
        .0
        .get(&InputMode::Tab)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));

    assert!(result_normal.is_none());
    assert!(result_pane.is_none());
    assert!(result_resize.is_none());
    assert!(result_tab.is_none());
}

#[test]
fn unbind_single_toplevel_single_key_single_mode_differing() {
    let unbind = Unbind::Keys(vec![Key::Alt('l')]);
    let unbind_from_yaml = UnbindFromYaml { unbind };
    let key_action_unbind = vec![KeyActionUnbind::Unbind(unbind_from_yaml)];
    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbind);
    let from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::Keys(vec![Key::Alt('n')]),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));

    let result_normal_n = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_normal_l = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('l'));
    let result_resize_n = keybinds_from_yaml
        .0
        .get(&InputMode::Resize)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_resize_l = keybinds_from_yaml
        .0
        .get(&InputMode::Resize)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('l'));

    assert!(result_normal_n.is_none());
    assert!(result_normal_l.is_none());
    assert!(result_resize_n.is_none());
    assert!(result_resize_l.is_some());
}

#[test]
fn unbind_single_toplevel_single_key_multiple_modes() {
    let unbind = Unbind::Keys(vec![Key::Alt('l')]);
    let unbind_from_yaml = UnbindFromYaml { unbind };
    let key_action_unbind = vec![KeyActionUnbind::Unbind(unbind_from_yaml)];
    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbind.clone());
    keys.insert(InputMode::Pane, key_action_unbind);
    let from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::Keys(vec![Key::Alt('n')]),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));

    let result_normal_n = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_normal_l = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('l'));
    let result_pane_n = keybinds_from_yaml
        .0
        .get(&InputMode::Pane)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_pane_l = keybinds_from_yaml
        .0
        .get(&InputMode::Pane)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('l'));

    assert!(result_normal_n.is_none());
    assert!(result_normal_l.is_none());
    assert!(result_pane_n.is_none());
    assert!(result_pane_l.is_none());
}

#[test]
fn unbind_single_toplevel_multiple_keys_single_mode() {
    let unbind = Unbind::Keys(vec![
        Key::Alt('l'),
        Key::Alt('h'),
        Key::Alt('j'),
        Key::Alt('k'),
    ]);
    let unbind_from_yaml = UnbindFromYaml { unbind };
    let key_action_unbind = vec![KeyActionUnbind::Unbind(unbind_from_yaml)];
    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbind.clone());
    keys.insert(InputMode::Pane, key_action_unbind);
    let from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::Keys(vec![Key::Alt('n')]),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));

    let result_normal_n = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_normal_l = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('l'));
    let result_normal_k = keybinds_from_yaml
        .0
        .get(&InputMode::Pane)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('k'));
    let result_normal_h = keybinds_from_yaml
        .0
        .get(&InputMode::Pane)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('h'));

    assert!(result_normal_n.is_none());
    assert!(result_normal_l.is_none());
    assert!(result_normal_h.is_none());
    assert!(result_normal_k.is_none());
}

#[test]
fn unbind_single_toplevel_multiple_keys_multiple_modes() {
    let unbind_normal = Unbind::Keys(vec![Key::Alt('l'), Key::Ctrl('p')]);
    let unbind_from_yaml_normal = UnbindFromYaml {
        unbind: unbind_normal,
    };
    let key_action_unbind_normal = vec![KeyActionUnbind::Unbind(unbind_from_yaml_normal)];
    let unbind = Unbind::Keys(vec![Key::Alt('l'), Key::Alt('k')]);
    let unbind_from_yaml = UnbindFromYaml { unbind };
    let key_action_unbind = vec![KeyActionUnbind::Unbind(unbind_from_yaml)];
    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbind_normal);
    keys.insert(InputMode::Pane, key_action_unbind);
    let from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::Keys(vec![Key::Alt('n')]),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));

    let result_normal_n = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_normal_p = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Ctrl('p'));
    let result_normal_l = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('l'));
    let result_pane_p = keybinds_from_yaml
        .0
        .get(&InputMode::Pane)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Ctrl('p'));
    let result_pane_n = keybinds_from_yaml
        .0
        .get(&InputMode::Pane)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('n'));
    let result_pane_l = keybinds_from_yaml
        .0
        .get(&InputMode::Pane)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Alt('l'));

    assert!(result_normal_n.is_none());
    assert!(result_normal_l.is_none());
    assert!(result_normal_p.is_none());
    assert!(result_pane_n.is_none());
    assert!(result_pane_p.is_some());
    assert!(result_pane_l.is_none());
}

#[test]
fn uppercase_and_lowercase_are_distinct() {
    let key_action_n = KeyActionFromYaml {
        key: vec![Key::Char('n')],
        action: vec![Action::NewTab(None)],
    };
    let key_action_large_n = KeyActionFromYaml {
        key: vec![Key::Char('N')],
        action: vec![Action::NewPane(None)],
    };

    let key_action_unbind = vec![
        KeyActionUnbind::KeyAction(key_action_n),
        KeyActionUnbind::KeyAction(key_action_large_n),
    ];
    let mut keys = HashMap::<InputMode, Vec<KeyActionUnbind>>::new();
    keys.insert(InputMode::Normal, key_action_unbind);
    let from_yaml = KeybindsFromYaml {
        keybinds: keys,
        unbind: Unbind::All(false),
    };

    let keybinds_from_yaml = Keybinds::get_default_keybinds_with_config(Some(from_yaml));
    let result_n = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Char('n'));
    let result_large_n = keybinds_from_yaml
        .0
        .get(&InputMode::Normal)
        .expect("ModeKeybinds shouldn't be empty")
        .0
        .get(&Key::Char('N'));

    assert!(result_n.is_some());
    assert!(result_large_n.is_some());
}

use super::super::actions::*;
use super::super::keybinds::*;
use termion::event::Key;

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
    keybinds_expected.0.insert(
        InputMode::Normal,
        mode_keybinds_self
    );
    keybinds_expected.0.insert(
        InputMode::Resize,
        mode_keybinds_other
    );

    assert_eq!(
        keybinds_expected,
        keybinds_self.merge_keybinds(keybinds_other)
    )
}

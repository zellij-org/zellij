use super::super::actions::*;
use super::super::keybinds::*;
use crate::data::{BareKey, Direction, KeyWithModifier};
use crate::input::config::Config;
use insta::assert_snapshot;
use strum::IntoEnumIterator;

#[test]
fn can_define_keybindings_in_configfile() {
    let config_contents = r#"
        keybinds {
            normal {
                bind "Ctrl g" { SwitchToMode "Locked"; }
            }
        }
    "#;
    let config = Config::from_kdl(config_contents, None).unwrap();
    let ctrl_g_normal_mode_action = config.keybinds.get_actions_for_key_in_mode(
        &InputMode::Normal,
        &KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier(),
    );
    assert_eq!(
        ctrl_g_normal_mode_action,
        Some(&vec![Action::SwitchToMode(InputMode::Locked)]),
        "Keybinding successfully defined in config"
    );
}

#[test]
fn can_define_multiple_keybinds_for_same_action() {
    let config_contents = r#"
        keybinds {
            normal {
                bind "Alt h" "Alt Left" { MoveFocusOrTab "Left"; }
            }
        }
    "#;
    let config = Config::from_kdl(config_contents, None).unwrap();
    let alt_h_normal_mode_action = config.keybinds.get_actions_for_key_in_mode(
        &InputMode::Normal,
        &KeyWithModifier::new(BareKey::Left).with_alt_modifier(),
    );
    let alt_left_normal_mode_action = config.keybinds.get_actions_for_key_in_mode(
        &InputMode::Normal,
        &KeyWithModifier::new(BareKey::Char('h')).with_alt_modifier(),
    );
    assert_eq!(
        alt_h_normal_mode_action,
        Some(&vec![Action::MoveFocusOrTab(Direction::Left)]),
        "First keybinding successfully defined in config"
    );
    assert_eq!(
        alt_left_normal_mode_action,
        Some(&vec![Action::MoveFocusOrTab(Direction::Left)]),
        "Second keybinding successfully defined in config"
    );
}

#[test]
fn can_define_series_of_actions_for_same_keybinding() {
    let config_contents = r#"
        keybinds {
            pane {
                bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
            }
        }
    "#;
    let config = Config::from_kdl(config_contents, None).unwrap();
    let z_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('z')));
    assert_eq!(
        z_in_pane_mode,
        Some(&vec![
            Action::TogglePaneFrames,
            Action::SwitchToMode(InputMode::Normal)
        ]),
        "Action series successfully defined"
    );
}

#[test]
fn keybindings_bind_order_is_preserved() {
    let config_contents = r#"
        keybinds {
            pane {
                bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                bind "z" { SwitchToMode "Resize"; }
            }
        }
    "#;
    let config = Config::from_kdl(config_contents, None).unwrap();
    let z_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('z')));
    assert_eq!(
        z_in_pane_mode,
        Some(&vec![Action::SwitchToMode(InputMode::Resize)]),
        "Second keybinding was applied"
    );
}

#[test]
fn uppercase_and_lowercase_keybindings_are_distinct() {
    let config_contents = r#"
        keybinds {
            pane {
                bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                bind "Z" { SwitchToMode "Resize"; }
            }
        }
    "#;
    let config = Config::from_kdl(config_contents, None).unwrap();
    let z_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('z')));
    let uppercase_z_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('Z')));
    assert_eq!(
        z_in_pane_mode,
        Some(&vec![
            Action::TogglePaneFrames,
            Action::SwitchToMode(InputMode::Normal)
        ]),
        "Lowercase z successfully bound"
    );
    assert_eq!(
        uppercase_z_in_pane_mode,
        Some(&vec![Action::SwitchToMode(InputMode::Resize)]),
        "Uppercase z successfully bound"
    );
}

#[test]
fn can_override_keybindings() {
    let default_config_contents = r#"
        keybinds {
            pane {
                bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
            }
        }
    "#;
    let config_contents = r#"
        keybinds {
            pane {
                bind "z" { SwitchToMode "Resize"; }
            }
        }
    "#;
    let default_config = Config::from_kdl(default_config_contents, None).unwrap();
    let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
    let z_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('z')));
    assert_eq!(
        z_in_pane_mode,
        Some(&vec![Action::SwitchToMode(InputMode::Resize)]),
        "Keybinding from config overrode keybinding from default config"
    );
}

#[test]
fn can_add_to_default_keybindings() {
    // this test just makes sure keybindings defined in a custom config are added to different
    // keybindings defined in the default config
    let default_config_contents = r#"
        keybinds {
            pane {
                bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
            }
        }
    "#;
    let config_contents = r#"
        keybinds {
            pane {
                bind "r" { SwitchToMode "Resize"; }
            }
        }
    "#;
    let default_config = Config::from_kdl(default_config_contents, None).unwrap();
    let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
    let z_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('z')));
    let r_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('r')));
    assert_eq!(
        z_in_pane_mode,
        Some(&vec![
            Action::TogglePaneFrames,
            Action::SwitchToMode(InputMode::Normal)
        ]),
        "Keybinding from default config bound"
    );
    assert_eq!(
        r_in_pane_mode,
        Some(&vec![Action::SwitchToMode(InputMode::Resize)]),
        "Keybinding from custom config bound as well"
    );
}

#[test]
fn can_clear_default_keybindings() {
    let default_config_contents = r#"
        keybinds {
            normal {
                bind "Ctrl g" { SwitchToMode "Locked"; }
            }
            pane {
                bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
            }
        }
    "#;
    let config_contents = r#"
        keybinds clear-defaults=true {
            normal {
                bind "Ctrl r" { SwitchToMode "Locked"; }
            }
            pane {
                bind "r" { SwitchToMode "Resize"; }
            }
        }
    "#;
    let default_config = Config::from_kdl(default_config_contents, None).unwrap();
    let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
    let ctrl_g_normal_mode_action = config.keybinds.get_actions_for_key_in_mode(
        &InputMode::Normal,
        &KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier(),
    );
    let z_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('z')));
    let ctrl_r_in_normal_mode = config.keybinds.get_actions_for_key_in_mode(
        &InputMode::Normal,
        &KeyWithModifier::new(BareKey::Char('r')).with_ctrl_modifier(),
    );
    let r_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('r')));
    assert_eq!(
        ctrl_g_normal_mode_action, None,
        "Keybinding from normal mode in default config cleared"
    );
    assert_eq!(
        z_in_pane_mode, None,
        "Keybinding from pane mode in default config cleared"
    );
    assert_eq!(
        r_in_pane_mode,
        Some(&vec![Action::SwitchToMode(InputMode::Resize)]),
        "Keybinding from pane mode in custom config still bound"
    );
    assert_eq!(
        ctrl_r_in_normal_mode,
        Some(&vec![Action::SwitchToMode(InputMode::Locked)]),
        "Keybinding from normal mode in custom config still bound"
    );
}

#[test]
fn can_clear_default_keybindings_per_single_mode() {
    let default_config_contents = r#"
        keybinds {
            normal {
                bind "Ctrl g" { SwitchToMode "Locked"; }
            }
            pane {
                bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
            }
        }
    "#;
    let config_contents = r#"
        keybinds {
            pane clear-defaults=true {
                bind "r" { SwitchToMode "Resize"; }
            }
        }
    "#;
    let default_config = Config::from_kdl(default_config_contents, None).unwrap();
    let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
    let ctrl_g_normal_mode_action = config.keybinds.get_actions_for_key_in_mode(
        &InputMode::Normal,
        &KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier(),
    );
    let z_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('z')));
    let r_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('r')));
    assert_eq!(
        ctrl_g_normal_mode_action,
        Some(&vec![Action::SwitchToMode(InputMode::Locked)]),
        "Keybind in different mode from default config not cleared"
    );
    assert_eq!(
        z_in_pane_mode, None,
        "Keybinding from pane mode in default config cleared"
    );
    assert_eq!(
        r_in_pane_mode,
        Some(&vec![Action::SwitchToMode(InputMode::Resize)]),
        "Keybinding from pane mode in custom config still bound"
    );
}

#[test]
fn can_unbind_multiple_keys_globally() {
    let default_config_contents = r#"
        keybinds {
            normal {
                bind "Ctrl g" { SwitchToMode "Locked"; }
            }
            pane {
                bind "Ctrl g" { SwitchToMode "Locked"; }
                bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                bind "r" { TogglePaneFrames; }
            }
        }
    "#;
    let config_contents = r#"
        keybinds {
            unbind "Ctrl g" "z"
            pane {
                bind "t" { SwitchToMode "Tab"; }
            }
        }
    "#;
    let default_config = Config::from_kdl(default_config_contents, None).unwrap();
    let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
    let ctrl_g_normal_mode_action = config.keybinds.get_actions_for_key_in_mode(
        &InputMode::Normal,
        &KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier(),
    );
    let ctrl_g_pane_mode_action = config.keybinds.get_actions_for_key_in_mode(
        &InputMode::Pane,
        &KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier(),
    );
    let r_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('r')));
    let z_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('z')));
    let t_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('t')));
    assert_eq!(
        ctrl_g_normal_mode_action, None,
        "First keybind uncleared in one mode"
    );
    assert_eq!(
        ctrl_g_pane_mode_action, None,
        "First keybind uncleared in another mode"
    );
    assert_eq!(z_in_pane_mode, None, "Second keybind cleared as well");
    assert_eq!(
        r_in_pane_mode,
        Some(&vec![Action::TogglePaneFrames]),
        "Unrelated keybinding in default config still bound"
    );
    assert_eq!(
        t_in_pane_mode,
        Some(&vec![Action::SwitchToMode(InputMode::Tab)]),
        "Keybinding from custom config still bound"
    );
}

#[test]
fn can_unbind_multiple_keys_per_single_mode() {
    let default_config_contents = r#"
        keybinds {
            normal {
                bind "Ctrl g" { SwitchToMode "Locked"; }
            }
            pane {
                bind "Ctrl g" { SwitchToMode "Locked"; }
                bind "z" { TogglePaneFrames; SwitchToMode "Normal"; }
                bind "r" { TogglePaneFrames; }
            }
        }
    "#;
    let config_contents = r#"
        keybinds {
            pane {
                unbind "Ctrl g" "z"
                bind "t" { SwitchToMode "Tab"; }
            }
        }
    "#;
    let default_config = Config::from_kdl(default_config_contents, None).unwrap();
    let config = Config::from_kdl(config_contents, Some(default_config)).unwrap();
    let ctrl_g_normal_mode_action = config.keybinds.get_actions_for_key_in_mode(
        &InputMode::Normal,
        &KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier(),
    );
    let ctrl_g_pane_mode_action = config.keybinds.get_actions_for_key_in_mode(
        &InputMode::Pane,
        &KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier(),
    );
    let r_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('r')));
    let z_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('z')));
    let t_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('t')));
    assert_eq!(
        ctrl_g_normal_mode_action,
        Some(&vec![Action::SwitchToMode(InputMode::Locked)]),
        "Keybind in different mode not cleared"
    );
    assert_eq!(
        ctrl_g_pane_mode_action, None,
        "First Keybind cleared in its mode"
    );
    assert_eq!(
        z_in_pane_mode, None,
        "Second keybind cleared in its mode as well"
    );
    assert_eq!(
        r_in_pane_mode,
        Some(&vec![Action::TogglePaneFrames]),
        "Unrelated keybinding in default config still bound"
    );
    assert_eq!(
        t_in_pane_mode,
        Some(&vec![Action::SwitchToMode(InputMode::Tab)]),
        "Keybinding from custom config still bound"
    );
}

#[test]
fn can_define_shared_keybinds_for_all_modes() {
    let config_contents = r#"
        keybinds {
            shared {
                bind "Ctrl g" { SwitchToMode "Locked"; }
            }
        }
    "#;
    let config = Config::from_kdl(config_contents, None).unwrap();
    for mode in InputMode::iter() {
        let action_in_mode = config.keybinds.get_actions_for_key_in_mode(
            &mode,
            &KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier(),
        );
        assert_eq!(
            action_in_mode,
            Some(&vec![Action::SwitchToMode(InputMode::Locked)]),
            "Keybind bound in mode"
        );
    }
}

#[test]
fn can_define_shared_keybinds_with_exclusion() {
    let config_contents = r#"
        keybinds {
            shared_except "locked" {
                bind "Ctrl g" { SwitchToMode "Locked"; }
            }
        }
    "#;
    let config = Config::from_kdl(config_contents, None).unwrap();
    for mode in InputMode::iter() {
        let action_in_mode = config.keybinds.get_actions_for_key_in_mode(
            &mode,
            &KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier(),
        );
        if mode == InputMode::Locked {
            assert_eq!(action_in_mode, None, "Keybind unbound in excluded mode");
        } else {
            assert_eq!(
                action_in_mode,
                Some(&vec![Action::SwitchToMode(InputMode::Locked)]),
                "Keybind bound in mode"
            );
        }
    }
}

#[test]
fn can_define_shared_keybinds_with_inclusion() {
    let config_contents = r#"
        keybinds {
            shared_among "normal" "resize" "pane" {
                bind "Ctrl g" { SwitchToMode "Locked"; }
            }
        }
    "#;
    let config = Config::from_kdl(config_contents, None).unwrap();
    for mode in InputMode::iter() {
        let action_in_mode = config.keybinds.get_actions_for_key_in_mode(
            &mode,
            &KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier(),
        );
        if mode == InputMode::Normal || mode == InputMode::Resize || mode == InputMode::Pane {
            assert_eq!(
                action_in_mode,
                Some(&vec![Action::SwitchToMode(InputMode::Locked)]),
                "Keybind bound in included mode"
            );
        } else {
            assert_eq!(action_in_mode, None, "Keybind unbound in other modes");
        }
    }
}

#[test]
fn keybindings_unbinds_happen_after_binds() {
    let config_contents = r#"
        keybinds {
            pane {
                unbind "z"
                bind "z" { SwitchToMode "Resize"; }
            }
        }
    "#;
    let config = Config::from_kdl(config_contents, None).unwrap();
    let z_in_pane_mode = config
        .keybinds
        .get_actions_for_key_in_mode(&InputMode::Pane, &KeyWithModifier::new(BareKey::Char('z')));
    assert_eq!(z_in_pane_mode, None, "Key was ultimately unbound");
}

#[test]
fn error_received_on_unknown_input_mode() {
    let config_contents = r#"
        keybinds {
            i_do_not_exist {
                bind "z" { SwitchToMode "Resize"; }
            }
        }
    "#;
    let config_error = Config::from_kdl(config_contents, None).unwrap_err();
    assert_snapshot!(format!("{:?}", config_error));
}

#[test]
fn error_received_on_unknown_key_instruction() {
    let config_contents = r#"
        keybinds {
            pane {
                i_am_not_bind_or_unbind
                bind "z" { SwitchToMode "Resize"; }
            }
        }
    "#;
    let config_error = Config::from_kdl(config_contents, None).unwrap_err();
    assert_snapshot!(format!("{:?}", config_error));
}

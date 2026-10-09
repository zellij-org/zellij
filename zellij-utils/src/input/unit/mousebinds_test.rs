use super::super::actions::Action;
use super::super::config::Config;
use super::super::config_blocks::{
    mousebind_change_kdl, mousebind_change_kdl_for, mousebinding_entries,
};
use super::super::config_file_edit::{apply_edits, edits_between, written_file_matches};
use super::super::keybind_presets::{save_keybinds_as_preset, MousebindChanges};
use super::super::mousebinds::*;
use super::super::options::Options;
use crate::data::{
    Direction, InputMode, KeyModifier, KeybindingSource, MouseButton, MouseTarget, MouseTrigger,
};
use kdl::KdlDocument;
use std::collections::BTreeSet;
use strum::IntoEnumIterator;

fn defaults() -> Config {
    Config::from_default_assets().unwrap()
}

fn user_config(text: &str) -> Config {
    Config::from_kdl(text, Some(defaults())).unwrap()
}

fn modifiers(names: &[KeyModifier]) -> BTreeSet<KeyModifier> {
    names.iter().copied().collect()
}

fn resolved(
    config: &Config,
    mode: InputMode,
    button: MouseButton,
    held: &[KeyModifier],
    click_count: u8,
    on_frame: bool,
) -> Option<MouseBinding> {
    config
        .mousebinds
        .resolve(mode, button, &modifiers(held), click_count, on_frame)
        .map(|(_, binding)| binding.clone())
}

fn behaviour(
    config: &Config,
    mode: InputMode,
    button: MouseButton,
    held: &[KeyModifier],
    click_count: u8,
    on_frame: bool,
) -> Option<MouseBehaviour> {
    resolved(config, mode, button, held, click_count, on_frame).and_then(|b| b.as_behaviour())
}

fn changes(text: &str) -> MousebindChanges {
    let document: KdlDocument = text.parse().unwrap();
    MousebindChanges::from_kdl(document.get("mousebinds").unwrap(), &Options::default()).unwrap()
}

fn parse_error(text: &str) -> String {
    let document: KdlDocument = text.parse().unwrap();
    format!(
        "{:?}",
        MousebindChanges::from_kdl(document.get("mousebinds").unwrap(), &Options::default())
            .unwrap_err()
    )
}

#[test]
fn triggers_parse_modifiers_click_counts_and_buttons() {
    let trigger = MouseTrigger::parse("Ctrl Alt Double Left", MouseTarget::Frame).unwrap();
    assert_eq!(trigger.button, MouseButton::Left);
    assert_eq!(
        trigger.modifiers,
        modifiers(&[KeyModifier::Ctrl, KeyModifier::Alt])
    );
    assert_eq!(trigger.click_count, 2);
    assert_eq!(trigger.target, MouseTarget::Frame);
    assert_eq!(trigger.to_kdl(), "Ctrl Alt Double Left");
    assert_eq!(
        MouseTrigger::parse("triple middle", MouseTarget::Any)
            .unwrap()
            .to_kdl(),
        "Triple Middle"
    );
    assert_eq!(
        MouseTrigger::parse("Shift ScrollUp", MouseTarget::Any)
            .unwrap()
            .button,
        MouseButton::ScrollUp
    );
}

#[test]
fn super_double_scrolls_and_unknown_buttons_are_rejected() {
    assert!(MouseTrigger::parse("Super Left", MouseTarget::Any)
        .unwrap_err()
        .contains("Super"));
    assert!(MouseTrigger::parse("Double ScrollUp", MouseTarget::Any)
        .unwrap_err()
        .contains("Double"));
    assert!(MouseTrigger::parse("Ctrl Button4", MouseTarget::Any)
        .unwrap_err()
        .contains("Unknown mouse button"));
    assert!(
        parse_error("mousebinds {\n normal {\n bind \"Left\" on=\"edge\" { Click; }\n }\n}")
            .contains("'on' must be")
    );
}

#[test]
fn the_default_preset_reproduces_the_built_in_mouse_behaviour() {
    let config = defaults();
    for mode in [InputMode::Normal, InputMode::Locked, InputMode::Pane] {
        let b = |button, held: &[KeyModifier], count, on_frame| {
            behaviour(&config, mode, button, held, count, on_frame)
        };
        assert_eq!(
            b(MouseButton::Left, &[], 1, false),
            Some(MouseBehaviour::Click)
        );
        assert_eq!(
            b(MouseButton::Left, &[], 1, true),
            Some(MouseBehaviour::Click)
        );
        assert_eq!(
            b(MouseButton::Left, &[KeyModifier::Shift], 1, false),
            Some(MouseBehaviour::Click)
        );
        assert_eq!(
            b(MouseButton::Left, &[], 2, true),
            Some(MouseBehaviour::ToggleFullscreen)
        );
        assert_eq!(
            b(MouseButton::Left, &[], 2, false),
            Some(MouseBehaviour::Select)
        );
        assert_eq!(
            b(MouseButton::Left, &[], 3, false),
            Some(MouseBehaviour::Select)
        );
        assert_eq!(
            b(MouseButton::Left, &[], 3, true),
            Some(MouseBehaviour::ToggleFullscreen)
        );
        assert_eq!(
            b(MouseButton::Left, &[KeyModifier::Ctrl], 1, true),
            Some(MouseBehaviour::ResizePane)
        );
        assert_eq!(b(MouseButton::Left, &[KeyModifier::Ctrl], 1, false), None);
        assert_eq!(
            b(MouseButton::Left, &[KeyModifier::Alt], 1, false),
            Some(MouseBehaviour::GroupToggle)
        );
        assert_eq!(
            b(MouseButton::Right, &[KeyModifier::Alt], 1, false),
            Some(MouseBehaviour::Ungroup)
        );
        assert_eq!(
            b(MouseButton::Right, &[], 1, false),
            Some(MouseBehaviour::ContextMenu)
        );
        assert_eq!(b(MouseButton::Middle, &[], 1, false), None);
        assert_eq!(
            b(MouseButton::ScrollUp, &[], 1, false),
            Some(MouseBehaviour::Scroll(3))
        );
        assert_eq!(
            b(MouseButton::ScrollUp, &[KeyModifier::Ctrl], 1, false),
            Some(MouseBehaviour::ResizeScroll(
                ResizeScrollDirection::Increase
            ))
        );
        assert_eq!(
            b(MouseButton::ScrollDown, &[KeyModifier::Alt], 1, false),
            Some(MouseBehaviour::ScrollToPrompt(PromptDirection::Next))
        );
        assert_eq!(
            b(MouseButton::ScrollLeft, &[], 1, false),
            Some(MouseBehaviour::ScrollColumns(4))
        );
    }
}

#[test]
fn the_unlock_first_preset_has_the_same_mouse_bindings_as_the_default_preset() {
    let unlock_first = user_config("keybinds preset=\"unlock-first\"");
    assert_eq!(unlock_first.mousebinds, defaults().mousebinds);
}

#[test]
fn a_users_block_is_layered_on_the_preset() {
    let config = user_config(
        "mousebinds {\n    normal {\n        bind \"Alt Middle\" { NewTab; }\n        unbind \"Right\"\n    }\n    locked clear-defaults=true {\n    }\n}\n",
    );
    assert!(matches!(
        resolved(
            &config,
            InputMode::Normal,
            MouseButton::Middle,
            &[KeyModifier::Alt],
            1,
            false
        )
        .map(|binding| binding.kind),
        Some(MouseBindingKind::Actions(_))
    ));
    assert_eq!(
        behaviour(
            &config,
            InputMode::Normal,
            MouseButton::Right,
            &[],
            1,
            false
        ),
        None
    );
    assert_eq!(
        behaviour(&config, InputMode::Pane, MouseButton::Right, &[], 1, false),
        Some(MouseBehaviour::ContextMenu)
    );
    assert!(config
        .mousebinds
        .0
        .get(&InputMode::Locked)
        .map(|bindings| bindings.is_empty())
        .unwrap_or(true));
}

#[test]
fn a_whole_block_clearing_the_defaults_leaves_only_the_users_bindings() {
    let config = user_config(
        "mousebinds clear-defaults=true {\n    shared {\n        bind \"Left\" { Click; }\n    }\n}\n",
    );
    assert_eq!(
        behaviour(&config, InputMode::Normal, MouseButton::Left, &[], 1, false),
        Some(MouseBehaviour::Click)
    );
    assert_eq!(
        behaviour(
            &config,
            InputMode::Normal,
            MouseButton::ScrollUp,
            &[],
            1,
            false
        ),
        None
    );
}

#[test]
fn leader_values_are_used_in_mouse_triggers() {
    let config = user_config("keybinds primary=\"Alt\" secondary=\"Ctrl\"");
    assert_eq!(
        behaviour(
            &config,
            InputMode::Normal,
            MouseButton::Left,
            &[KeyModifier::Alt],
            1,
            true
        ),
        Some(MouseBehaviour::ResizePane)
    );
    assert_eq!(
        behaviour(
            &config,
            InputMode::Normal,
            MouseButton::Left,
            &[KeyModifier::Ctrl],
            1,
            false
        ),
        Some(MouseBehaviour::GroupToggle)
    );
}

#[test]
fn a_leader_that_resolves_to_super_skips_only_that_binding() {
    let config = user_config("keybinds primary=\"Super\"");
    assert_eq!(config.keybinds_layers.active.error, None);
    assert_eq!(
        behaviour(&config, InputMode::Normal, MouseButton::Left, &[], 1, false),
        Some(MouseBehaviour::Click)
    );
    assert_eq!(
        behaviour(
            &config,
            InputMode::Normal,
            MouseButton::ScrollUp,
            &[KeyModifier::Alt],
            1,
            false
        ),
        Some(MouseBehaviour::ScrollToPrompt(PromptDirection::Previous))
    );
}

#[test]
fn a_preset_without_mouse_bindings_uses_the_default_ones() {
    let dir = tempfile::tempdir().unwrap();
    let keybinds_dir = dir.path().join("keybinds");
    std::fs::create_dir_all(&keybinds_dir).unwrap();
    std::fs::write(
        keybinds_dir.join("fixed.kdl"),
        "keybinds {\n    normal {\n        bind \"Ctrl y\" { SwitchToMode \"Locked\"; }\n    }\n}\n",
    )
    .unwrap();
    let mut base = defaults();
    base.keybinds_layers.config_dir = Some(dir.path().to_path_buf());
    let config = Config::from_kdl("keybinds preset=\"fixed\"", Some(base)).unwrap();
    assert_eq!(config.keybinds_layers.active.error, None);
    assert_eq!(config.mousebinds, defaults().mousebinds);
}

#[test]
fn a_preset_with_its_own_mouse_bindings_replaces_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let keybinds_dir = dir.path().join("keybinds");
    std::fs::create_dir_all(&keybinds_dir).unwrap();
    std::fs::write(
        keybinds_dir.join("mouse.kdl"),
        "keybinds {\n}\nmousebinds {\n    shared {\n        bind \"Left\" { FocusPane; }\n    }\n}\n",
    )
    .unwrap();
    let mut base = defaults();
    base.keybinds_layers.config_dir = Some(dir.path().to_path_buf());
    let config = Config::from_kdl("keybinds preset=\"mouse\"", Some(base)).unwrap();
    assert_eq!(
        behaviour(&config, InputMode::Normal, MouseButton::Left, &[], 1, false),
        Some(MouseBehaviour::FocusPane)
    );
    assert_eq!(
        behaviour(
            &config,
            InputMode::Normal,
            MouseButton::Right,
            &[],
            1,
            false
        ),
        None
    );
}

#[test]
fn a_layout_block_goes_on_top_of_the_users_block() {
    let user =
        user_config("mousebinds {\n    normal {\n        bind \"Middle\" { Ignore; }\n    }\n}\n");
    let config = Config::from_layout_kdl(
        "mousebinds {\n    normal {\n        bind \"Middle\" { PassToApp; }\n    }\n}\n",
        Some(user),
    )
    .unwrap();
    assert_eq!(
        behaviour(
            &config,
            InputMode::Normal,
            MouseButton::Middle,
            &[],
            1,
            false
        ),
        Some(MouseBehaviour::PassToApp)
    );
    assert!(!config.keybinds_layers.layout_mouse.is_empty());
}

#[test]
fn double_and_triple_clicks_fall_back_to_fewer_clicks_and_any_place() {
    let config = user_config(
        "mousebinds clear-defaults=true {\n    normal {\n        bind \"Middle\" { FocusPane; }\n        bind \"Double Middle\" on=\"frame\" { ToggleFullscreen; }\n    }\n}\n",
    );
    let b = |count, on_frame| {
        behaviour(
            &config,
            InputMode::Normal,
            MouseButton::Middle,
            &[],
            count,
            on_frame,
        )
    };
    assert_eq!(b(3, true), Some(MouseBehaviour::ToggleFullscreen));
    assert_eq!(b(2, false), Some(MouseBehaviour::FocusPane));
    assert_eq!(b(1, true), Some(MouseBehaviour::FocusPane));
}

#[test]
fn behaviours_must_be_alone_and_scrolling_needs_the_wheel() {
    assert!(
        parse_error("mousebinds {\n normal {\n bind \"Left\" { Click; NewTab; }\n }\n}")
            .contains("must be the only item")
    );
    assert!(
        parse_error("mousebinds {\n normal {\n bind \"Left\" { Scroll 3; }\n }\n}")
            .contains("can only be bound to")
    );
}

#[test]
fn move_pane_with_a_direction_is_the_normal_action() {
    let parsed = changes("mousebinds {\n normal {\n bind \"Alt Middle\" { MovePane \"left\"; }\n bind \"Middle\" { MovePane; }\n }\n}");
    let normal = parsed.modes.get(&InputMode::Normal).unwrap();
    let alt_middle =
        MouseTrigger::new(MouseButton::Middle).with_modifiers(modifiers(&[KeyModifier::Alt]));
    assert_eq!(
        normal.bind.get(&alt_middle).map(|b| b.kind.clone()),
        Some(MouseBindingKind::Actions(vec![Action::MovePane {
            direction: Some(Direction::Left)
        }]))
    );
    assert_eq!(
        normal
            .bind
            .get(&MouseTrigger::new(MouseButton::Middle))
            .and_then(|b| b.as_behaviour()),
        Some(MouseBehaviour::MovePane)
    );
}

#[test]
fn mouse_changes_round_trip_through_kdl() {
    let text = "mousebinds {\n    shared_except \"locked\" {\n        bind \"Ctrl Double Left\" on=\"frame\" app_first=true { ToggleFullscreen; }\n    }\n    pane clear-defaults=true {\n        bind \"Right\" { NewTab; }\n    }\n    normal {\n        unbind \"Middle\" on=\"content\"\n    }\n    unbind \"ScrollLeft\"\n}\n";
    let parsed = changes(text);
    let written = parsed.to_kdl().unwrap().to_string();
    assert_eq!(changes(&written), parsed);
}

#[test]
fn a_mouse_binding_change_is_written_as_a_mousebinds_block() {
    let trigger = MouseTrigger::parse("Alt Middle", MouseTarget::Frame).unwrap();
    let text = mousebind_change_kdl(
        InputMode::Normal,
        &trigger,
        Some(&["NewTab".to_owned()]),
        Some(false),
    )
    .unwrap();
    assert_eq!(
        text,
        "mousebinds {\n    normal {\n        bind \"Alt Middle\" on=\"frame\" app_first=false { NewTab; }\n    }\n}\n"
    );
    let unbind = mousebind_change_kdl(InputMode::Pane, &trigger, None, None).unwrap();
    assert_eq!(
        unbind,
        "mousebinds {\n    pane {\n        unbind \"Alt Middle\" on=\"frame\"\n    }\n}\n"
    );
}

#[test]
fn a_mouse_binding_change_for_every_mode_is_written_as_a_shared_block() {
    let trigger = MouseTrigger::parse("Middle", MouseTarget::Any).unwrap();
    let text =
        mousebind_change_kdl_for(None, &trigger, Some(&["NewTab".to_owned()]), None).unwrap();
    assert_eq!(
        text,
        "mousebinds {\n    shared {\n        bind \"Middle\" { NewTab; }\n    }\n}\n"
    );
    let parsed = changes(&text);
    for mode in InputMode::iter() {
        assert!(
            parsed.modes[&mode].bind.contains_key(&trigger),
            "{:?} is missing the binding",
            mode
        );
    }
    let unbind = mousebind_change_kdl_for(None, &trigger, None, None).unwrap();
    assert_eq!(
        unbind,
        "mousebinds {\n    shared {\n        unbind \"Middle\"\n    }\n}\n"
    );
    let parsed = changes(&unbind);
    for mode in InputMode::iter() {
        assert!(parsed.modes[&mode].unbind.contains(&trigger), "{:?}", mode);
    }
    assert!(parsed
        .to_kdl()
        .unwrap()
        .to_string()
        .contains("unbind \"Middle\""));
}

#[test]
fn mouse_binding_entries_name_where_they_come_from() {
    let saved = defaults();
    let current = user_config(
        "mousebinds {\n    normal {\n        bind \"Alt Middle\" { NewTab; }\n        unbind \"Right\"\n    }\n}\n",
    );
    let entries = mousebinding_entries(&saved, &current);
    let normal: Vec<_> = entries
        .iter()
        .filter(|entry| entry.mode == InputMode::Normal)
        .collect();
    let alt_middle = normal
        .iter()
        .find(|entry| entry.trigger.to_kdl() == "Alt Middle")
        .unwrap();
    assert_eq!(alt_middle.source, KeybindingSource::User);
    assert!(alt_middle.unsaved);
    assert_eq!(alt_middle.preset_actions, Some(vec!["Ignore".to_owned()]));
    let right = normal
        .iter()
        .find(|entry| entry.trigger.to_kdl() == "Right")
        .unwrap();
    assert!(right.unbound);
    assert_eq!(right.preset_actions, Some(vec!["ContextMenu".to_owned()]));
    let left = normal
        .iter()
        .find(|entry| entry.trigger.to_kdl() == "Left")
        .unwrap();
    assert_eq!(left.source, KeybindingSource::Preset);
    assert!(left.app_first);
    assert!(!left.unsaved);
}

#[test]
fn saving_writes_the_users_mouse_bindings_and_reads_them_back() {
    let file = "// mine\nmouse_mode true\n";
    let saved = user_config(file);
    let runtime = Config::from_kdl(
        "mousebinds {\n    normal {\n        bind \"Alt Middle\" { NewTab; }\n    }\n}\n",
        Some(saved.clone()),
    )
    .unwrap();
    let edits = edits_between(&saved, &runtime);
    let text = apply_edits(Some(file), &edits, &runtime).unwrap();
    written_file_matches(&text, &runtime, &edits).unwrap();
    assert!(text.starts_with("// mine\nmouse_mode true\n"));
    assert!(text.contains("bind \"Alt Middle\" { NewTab; }"));
    let reverted = user_config(&text);
    let edits = edits_between(&reverted, &saved);
    let text = apply_edits(Some(&text), &edits, &saved).unwrap();
    assert!(!text.contains("mousebinds"));
}

#[test]
fn save_as_preset_keeps_the_users_mouse_bindings() {
    let dir = tempfile::tempdir().unwrap();
    let config = user_config(
        "mousebinds {\n    normal {\n        bind \"Alt Middle\" { NewTab; }\n    }\n}\n",
    );
    let path = save_keybinds_as_preset(
        None,
        &config.keybinds_layers.user,
        &config.keybinds_layers.user_mouse,
        None,
        "mine",
        dir.path(),
    )
    .unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains("mousebinds"));
    assert!(text.contains("Alt Middle"));
}

#[test]
fn mouse_events_map_to_buttons_and_modifiers() {
    use super::super::mouse::MouseEvent;
    use crate::position::Position;
    let event = MouseEvent::new_ctrl_scroll_up_event(Position::new(0, 0));
    assert_eq!(mouse_button_of_event(&event), Some(MouseButton::ScrollUp));
    assert_eq!(
        mouse_modifiers_of_event(&event),
        modifiers(&[KeyModifier::Ctrl])
    );
    let event = MouseEvent::new_left_press_with_alt_event(Position::new(0, 0));
    assert_eq!(mouse_button_of_event(&event), Some(MouseButton::Left));
    assert_eq!(
        mouse_modifiers_of_event(&event),
        modifiers(&[KeyModifier::Alt])
    );
}

fn with_command_line_preset(mut config: Config, preset: &str) -> Config {
    let options = Options {
        keybinds_preset: Some(preset.to_owned()),
        ..Default::default()
    };
    config.apply_command_line_keybinds(&options);
    config
}

#[test]
fn a_command_line_preset_ignores_a_users_block_that_clears_the_defaults() {
    let config = user_config(
        "keybinds preset=\"unlock-first\"\nmousebinds clear-defaults=true {\n    shared {\n        bind \"Left\" { FocusPane; }\n    }\n}\n",
    );
    assert_eq!(
        behaviour(&config, InputMode::Normal, MouseButton::Left, &[], 1, false),
        Some(MouseBehaviour::FocusPane)
    );
    let config = with_command_line_preset(config, "default");
    assert_eq!(config.mousebinds, defaults().mousebinds);
}

#[test]
fn a_command_line_preset_keeps_user_and_layout_additions() {
    let user = user_config(
        "mousebinds {\n    normal {\n        bind \"Alt Middle\" { NewTab; }\n    }\n}\n",
    );
    let config = Config::from_layout_kdl(
        "mousebinds {\n    normal {\n        bind \"Middle\" { Ignore; }\n    }\n}\n",
        Some(user),
    )
    .unwrap();
    let config = with_command_line_preset(config, "unlock-first");
    assert!(matches!(
        resolved(
            &config,
            InputMode::Normal,
            MouseButton::Middle,
            &[KeyModifier::Alt],
            1,
            false
        )
        .map(|binding| binding.kind),
        Some(MouseBindingKind::Actions(_))
    ));
    assert_eq!(
        behaviour(
            &config,
            InputMode::Normal,
            MouseButton::Middle,
            &[],
            1,
            false
        ),
        Some(MouseBehaviour::Ignore)
    );
    assert_eq!(
        behaviour(
            &config,
            InputMode::Normal,
            MouseButton::Right,
            &[],
            1,
            false
        ),
        Some(MouseBehaviour::ContextMenu)
    );
}

use super::super::actions::Action;
use super::super::config::Config;
use super::super::keybind_presets::*;
use super::super::keybinds::Keybinds;
use super::super::options::Options;
use crate::data::{InputMode, KeyWithModifier, KeybindPresetSource};
use kdl::KdlDocument;
use std::collections::BTreeMap;
use std::path::Path;

const EXPECTED_DEFAULT_KEYBINDS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/test-fixtures/keybind-presets/expected-default-keybinds.kdl"
));

const EXPECTED_UNLOCK_FIRST_KEYBINDS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/test-fixtures/keybind-presets/expected-unlock-first-keybinds.kdl"
));

fn keybinds_from_fixture(fixture: &str) -> Keybinds {
    let document: KdlDocument = fixture.parse().unwrap();
    Keybinds::from_kdl(
        document.get("keybinds").unwrap(),
        Keybinds::default(),
        &Options::default(),
    )
    .unwrap()
}

fn today_default_keybinds() -> Keybinds {
    keybinds_from_fixture(EXPECTED_DEFAULT_KEYBINDS)
}

fn user_config(text: &str) -> Config {
    Config::from_kdl(text, Some(Config::from_default_assets().unwrap())).unwrap()
}

fn kdl_string(text: &str) -> String {
    kdl::KdlValue::String(text.to_owned()).to_string()
}

fn user_config_in_dir(text: &str, config_dir: &Path) -> Config {
    let mut base = Config::from_default_assets().unwrap();
    base.keybinds_layers.config_dir = Some(config_dir.to_path_buf());
    Config::from_kdl(text, Some(base)).unwrap()
}

fn key(text: &str) -> KeyWithModifier {
    text.parse().unwrap()
}

fn actions_for(config: &Config, mode: InputMode, key_text: &str) -> Option<Vec<Action>> {
    config
        .keybinds
        .get_actions_for_key_in_mode(&mode, &key(key_text))
        .cloned()
}

fn switch_to(mode: InputMode) -> Vec<Action> {
    vec![Action::SwitchToMode { input_mode: mode }]
}

fn write_preset(dir: &Path, name: &str, text: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(format!("{}.kdl", name)), text).unwrap();
}

const HARD_CODED_PRESET: &str = r#"
preset {
    name "Fixed"
    description "Keys without placeholders"
}
keybinds {
    normal {
        bind "Ctrl y" { SwitchToMode "Locked"; }
    }
    locked {
        bind "Ctrl y" { SwitchToMode "Normal"; }
    }
}
"#;

const PLACEHOLDER_PRESET: &str = r#"
preset {
    name "Leaders"
    defaults {
        primary "Ctrl"
        secondary "Alt"
    }
}
keybinds {
    normal {
        bind "{primary} p" { SwitchToMode "Pane"; }
        bind "{secondary} n" { NewPane; }
    }
}
"#;

#[test]
fn the_builtin_default_preset_with_its_default_values_equals_todays_keybindings() {
    let config = Config::from_default_assets().unwrap();
    assert_eq!(*config.keybinds, today_default_keybinds());
    assert_eq!(config.keybinds_layers.active.info.name, "default");
    assert_eq!(config.keybinds_layers.active.error, None);
}

#[test]
fn the_builtin_unlock_first_preset_with_its_default_values_equals_todays_unlock_first_keybindings()
{
    let config = user_config("keybinds preset=\"unlock-first\"");
    assert_eq!(
        *config.keybinds,
        keybinds_from_fixture(EXPECTED_UNLOCK_FIRST_KEYBINDS)
    );
    assert_eq!(config.options.default_mode, Some(InputMode::Locked));
}

#[test]
fn an_old_config_without_a_keybinds_block_gets_todays_keys() {
    let config = user_config("mouse_mode false");
    assert_eq!(*config.keybinds, today_default_keybinds());
}

#[test]
fn an_old_keybinds_block_without_clear_defaults_adds_to_todays_keys() {
    let config = user_config(
        r#"
        keybinds {
            normal {
                bind "Alt y" { NewTab; }
            }
            pane {
                unbind "x"
            }
        }
        "#,
    );
    assert_eq!(
        actions_for(&config, InputMode::Normal, "Alt y").map(|a| a.len()),
        Some(1)
    );
    assert_eq!(actions_for(&config, InputMode::Pane, "x"), None);
    assert_eq!(
        config.keybinds.0.get(&InputMode::Tab),
        today_default_keybinds().0.get(&InputMode::Tab)
    );
}

#[test]
fn an_old_block_with_clear_defaults_gives_exactly_its_own_keys() {
    let config = user_config(
        r#"
        keybinds clear-defaults=true {
            normal {
                bind "Ctrl r" { SwitchToMode "Locked"; }
            }
        }
        "#,
    );
    let mut expected = Keybinds::default();
    expected
        .get_input_mode_mut(&InputMode::Normal)
        .insert(key("Ctrl r"), switch_to(InputMode::Locked));
    assert_eq!(*config.keybinds, expected);
}

#[test]
fn a_per_mode_clear_defaults_clears_only_that_mode_of_the_preset() {
    let config = user_config(
        r#"
        keybinds {
            pane clear-defaults=true {
                bind "r" { SwitchToMode "Resize"; }
            }
        }
        "#,
    );
    let pane = config.keybinds.0.get(&InputMode::Pane).unwrap();
    assert_eq!(pane.len(), 1);
    assert_eq!(pane.get(&key("r")), Some(&switch_to(InputMode::Resize)));
    assert_eq!(
        config.keybinds.0.get(&InputMode::Tab),
        today_default_keybinds().0.get(&InputMode::Tab)
    );
}

#[test]
fn a_layout_keybinds_block_is_applied_on_top_of_the_user_keybindings() {
    let config = user_config(
        r#"
        keybinds {
            normal {
                bind "Alt y" { SwitchToMode "Tab"; }
            }
        }
        "#,
    );
    let config = Config::from_layout_kdl(
        r#"
        keybinds {
            normal {
                bind "Alt y" { SwitchToMode "Pane"; }
                bind "Alt z" { SwitchToMode "Move"; }
            }
        }
        "#,
        Some(config),
    )
    .unwrap();
    assert_eq!(
        actions_for(&config, InputMode::Normal, "Alt y"),
        Some(switch_to(InputMode::Pane))
    );
    assert_eq!(
        actions_for(&config, InputMode::Normal, "Alt z"),
        Some(switch_to(InputMode::Move))
    );
}

#[test]
fn a_layout_preset_overrides_the_config_preset_and_keeps_user_changes_on_top() {
    let config = user_config(
        r#"
        keybinds preset="default" primary="Alt" secondary="Ctrl" {
            locked {
                bind "Alt u" { SwitchToMode "Normal"; }
            }
        }
        "#,
    );
    let config = Config::from_layout_kdl(
        "keybinds preset=\"unlock-first\" unlock=\"Ctrl u\"",
        Some(config),
    )
    .unwrap();
    assert_eq!(config.keybinds_layers.active.info.name, "unlock-first");
    assert_eq!(
        actions_for(&config, InputMode::Locked, "Ctrl u"),
        Some(switch_to(InputMode::Normal))
    );
    assert_eq!(actions_for(&config, InputMode::Locked, "Ctrl g"), None);
    assert_eq!(
        actions_for(&config, InputMode::Locked, "Alt n"),
        Some(vec![Action::NewPane {
            direction: None,
            pane_name: None,
            start_suppressed: false,
        }]),
        "the config's leader values belong to another preset and are not used"
    );
    assert_eq!(
        actions_for(&config, InputMode::Locked, "Alt u"),
        Some(switch_to(InputMode::Normal))
    );
    assert_eq!(
        config.keybinds_layers.user.preset.as_deref(),
        Some("default")
    );
}

#[test]
fn a_bare_name_is_looked_up_in_the_keybinds_folder_before_the_builtins() {
    let config_dir = tempfile::tempdir().unwrap();
    write_preset(
        &config_dir.path().join("keybinds"),
        "unlock-first",
        HARD_CODED_PRESET,
    );
    let config = user_config_in_dir("keybinds preset=\"unlock-first\"", config_dir.path());
    assert_eq!(
        config.keybinds_layers.active.info.source,
        KeybindPresetSource::Folder
    );
    assert_eq!(
        actions_for(&config, InputMode::Normal, "Ctrl y"),
        Some(switch_to(InputMode::Locked))
    );
    assert_eq!(actions_for(&config, InputMode::Locked, "Ctrl g"), None);
}

#[test]
fn a_bare_name_missing_from_the_folder_uses_the_builtin() {
    let config_dir = tempfile::tempdir().unwrap();
    write_preset(
        &config_dir.path().join("keybinds"),
        "other",
        HARD_CODED_PRESET,
    );
    let config = user_config_in_dir("keybinds preset=\"unlock-first\"", config_dir.path());
    assert_eq!(
        config.keybinds_layers.active.info.source,
        KeybindPresetSource::BuiltIn
    );
    assert_eq!(
        actions_for(&config, InputMode::Locked, "Ctrl g"),
        Some(switch_to(InputMode::Normal))
    );
}

#[test]
fn the_keybinds_dir_option_moves_the_folder() {
    let config_dir = tempfile::tempdir().unwrap();
    let elsewhere = config_dir.path().join("elsewhere");
    write_preset(&elsewhere, "mine", HARD_CODED_PRESET);
    let config = user_config_in_dir(
        &format!(
            "keybinds_dir {}\nkeybinds preset=\"mine\"",
            kdl_string(&elsewhere.display().to_string())
        ),
        config_dir.path(),
    );
    assert_eq!(config.keybinds_layers.active.error, None);
    assert_eq!(
        actions_for(&config, InputMode::Normal, "Ctrl y"),
        Some(switch_to(InputMode::Locked))
    );
}

#[test]
fn a_value_with_an_extension_or_a_slash_is_a_path_relative_to_the_config_dir() {
    let config_dir = tempfile::tempdir().unwrap();
    write_preset(
        &config_dir.path().join("presets"),
        "fixed",
        HARD_CODED_PRESET,
    );
    let absolute = config_dir.path().join("presets").join("fixed.kdl");
    for value in [
        "presets/fixed.kdl".to_owned(),
        absolute.display().to_string(),
    ] {
        let config = user_config_in_dir(
            &format!("keybinds preset={}", kdl_string(&value)),
            config_dir.path(),
        );
        assert_eq!(
            config.keybinds_layers.active.info.source,
            KeybindPresetSource::File
        );
        assert_eq!(
            actions_for(&config, InputMode::Normal, "Ctrl y"),
            Some(switch_to(InputMode::Locked))
        );
    }
}

#[test]
fn a_missing_preset_falls_back_to_the_default_preset_with_an_error() {
    let config = user_config("keybinds preset=\"does-not-exist\"");
    assert_eq!(*config.keybinds, today_default_keybinds());
    assert!(config
        .keybinds_layers
        .active
        .error
        .as_deref()
        .unwrap()
        .contains("does-not-exist"));
}

#[test]
fn placeholders_take_the_block_attributes_first() {
    let config = user_config("keybinds primary=\"Alt\" secondary=\"Ctrl\"");
    assert_eq!(
        actions_for(&config, InputMode::Normal, "Alt p"),
        Some(switch_to(InputMode::Pane))
    );
    assert_eq!(
        actions_for(&config, InputMode::Normal, "Alt n"),
        Some(switch_to(InputMode::Resize))
    );
    assert!(actions_for(&config, InputMode::Normal, "Ctrl n").is_some());
    assert_eq!(
        actions_for(&config, InputMode::Scroll, "Ctrl c"),
        Some(vec![
            Action::ScrollToBottom,
            Action::SwitchToMode {
                input_mode: InputMode::Normal
            }
        ]),
        "keys written without placeholders stay as they are"
    );
}

#[test]
fn placeholders_fall_back_to_the_header_defaults() {
    let preset = KeybindPreset::from_kdl(PLACEHOLDER_PRESET).unwrap();
    let mut requested = BTreeMap::new();
    requested.insert("primary".to_owned(), "Super".to_owned());
    let values = preset.leader_values(&requested).unwrap();
    assert_eq!(values.get("primary").map(|s| s.as_str()), Some("Super"));
    assert_eq!(values.get("secondary").map(|s| s.as_str()), Some("Alt"));
    let keybinds = preset.keybinds(&values, &Options::default()).unwrap();
    let normal = keybinds.0.get(&InputMode::Normal).unwrap();
    assert!(normal.contains_key(&key("Super p")));
    assert!(normal.contains_key(&key("Alt n")));
}

fn bind_lines(text: &str) -> usize {
    text.lines()
        .filter(|line| line.trim_start().starts_with("bind "))
        .count()
}

#[test]
fn a_placeholder_without_any_value_is_an_error() {
    let preset = KeybindPreset::from_kdl(
        r#"
        keybinds {
            normal {
                bind "{unlock}" { SwitchToMode "Locked"; }
            }
        }
        "#,
    )
    .unwrap();
    let error = preset.leader_values(&BTreeMap::new()).unwrap_err();
    assert!(error.contains("{unlock}"), "{}", error);
}

#[test]
fn an_empty_value_for_a_used_placeholder_is_an_error_and_falls_back_to_the_default_preset() {
    let config = user_config("keybinds primary=\"\"");
    assert_eq!(*config.keybinds, today_default_keybinds());
    assert!(config
        .keybinds_layers
        .active
        .error
        .as_deref()
        .unwrap()
        .contains("empty"));
}

#[test]
fn equal_primary_and_secondary_values_are_an_error() {
    let config = user_config("keybinds primary=\"Alt Ctrl\" secondary=\"Ctrl Alt\"");
    assert_eq!(*config.keybinds, today_default_keybinds());
    assert!(config
        .keybinds_layers
        .active
        .error
        .as_deref()
        .unwrap()
        .contains("must differ"));
}

#[test]
fn an_unknown_placeholder_is_an_error() {
    let error = KeybindPreset::from_kdl(
        r#"
        keybinds {
            normal {
                bind "{tertiary} g" { SwitchToMode "Locked"; }
            }
        }
        "#,
    )
    .unwrap_err();
    assert!(error.contains("tertiary"), "{}", error);
}

#[test]
fn a_hard_coded_preset_ignores_leader_values() {
    let config_dir = tempfile::tempdir().unwrap();
    write_preset(
        &config_dir.path().join("keybinds"),
        "fixed",
        HARD_CODED_PRESET,
    );
    let config = user_config_in_dir(
        "keybinds preset=\"fixed\" primary=\"\" secondary=\"Alt\" unlock=\"Ctrl u\"",
        config_dir.path(),
    );
    assert_eq!(config.keybinds_layers.active.error, None);
    assert!(config.keybinds_layers.active.info.placeholders.is_empty());
    assert_eq!(
        actions_for(&config, InputMode::Normal, "Ctrl y"),
        Some(switch_to(InputMode::Locked))
    );
}

#[test]
fn choosing_another_preset_forgets_the_leader_values_of_the_previous_one() {
    let config = user_config("keybinds primary=\"Alt\" secondary=\"Ctrl\"");
    let config = Config::from_kdl("keybinds preset=\"unlock-first\"", Some(config)).unwrap();
    assert_eq!(config.keybinds_layers.user.primary, None);
    assert_eq!(config.keybinds_layers.user.secondary, None);
    assert_eq!(config.keybinds_layers.active.error, None);
}

#[test]
fn the_config_file_keeps_only_the_attributes_and_the_users_own_changes() {
    let config = user_config(
        r#"
        keybinds preset="unlock-first" unlock="Ctrl u" {
            normal {
                bind "Alt y" { SwitchToMode "Tab"; }
            }
            unbind "Alt f"
        }
        "#,
    );
    let text = config.to_string(false);
    let document: KdlDocument = text.parse().unwrap();
    let keybinds = document.get("keybinds").unwrap();
    assert_eq!(
        keybinds.get("preset").and_then(|e| e.value().as_string()),
        Some("unlock-first")
    );
    assert_eq!(
        keybinds.get("unlock").and_then(|e| e.value().as_string()),
        Some("Ctrl u")
    );
    assert!(keybinds.get("clear-defaults").is_none());
    assert_eq!(bind_lines(&text), 1, "{}", text);
    assert!(
        document.get("default_mode").is_none(),
        "the preset's default mode is not written: {}",
        text
    );
    let reloaded = Config::from_kdl(&text, None).unwrap();
    assert_eq!(reloaded.keybinds, config.keybinds);
    assert_eq!(reloaded.keybinds_layers.user, config.keybinds_layers.user);
    assert_eq!(reloaded.options.default_mode, Some(InputMode::Locked));
}

#[test]
fn the_config_file_keeps_clear_defaults_only_when_the_user_block_had_it() {
    let cleared = user_config(
        r#"
        keybinds clear-defaults=true {
            normal {
                bind "Ctrl r" { SwitchToMode "Locked"; }
            }
            pane clear-defaults=true {
                bind "r" { SwitchToMode "Resize"; }
            }
        }
        "#,
    );
    let text = cleared.to_string(false);
    assert!(text.contains("keybinds clear-defaults=true"), "{}", text);
    let reloaded = Config::from_kdl(&text, None).unwrap();
    assert_eq!(reloaded.keybinds, cleared.keybinds);

    let not_cleared = user_config("mouse_mode false");
    let text = not_cleared.to_string(false);
    assert!(!text.contains("keybinds"), "{}", text);
}

#[test]
fn per_mode_clear_defaults_and_unbinds_round_trip() {
    let config = user_config(
        r#"
        keybinds {
            pane clear-defaults=true {
                bind "r" { SwitchToMode "Resize"; }
            }
            tab {
                unbind "x"
            }
        }
        "#,
    );
    let text = config.to_string(false);
    let reloaded = Config::from_kdl(&text, None).unwrap();
    assert_eq!(reloaded.keybinds, config.keybinds, "{}", text);
    assert_eq!(reloaded.keybinds_layers.user, config.keybinds_layers.user);
}

#[test]
fn rebinding_keys_is_recorded_as_a_user_change() {
    let mut config = user_config("keybinds preset=\"unlock-first\"");
    config.keybinds_layers.user.changes.bind(
        InputMode::Locked,
        key("Alt y"),
        switch_to(InputMode::Tab),
    );
    config.resolve_keybinds();
    let text = config.to_string(false);
    assert!(text.contains("Alt y"), "{}", text);
    assert_eq!(bind_lines(&text), 1, "{}", text);
}

#[test]
fn listing_presets_shows_folder_and_builtin_presets_and_broken_files() {
    let dir = tempfile::tempdir().unwrap();
    write_preset(dir.path(), "fixed", HARD_CODED_PRESET);
    write_preset(dir.path(), "leaders", PLACEHOLDER_PRESET);
    write_preset(dir.path(), "broken", "keybinds {");
    let (presets, errors) = list_keybind_presets(Some(dir.path()), &[]);
    let names: Vec<&str> = presets.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["default", "unlock-first", "fixed", "leaders"]);
    let fixed = presets.iter().find(|p| p.name == "fixed").unwrap();
    assert_eq!(fixed.source, KeybindPresetSource::Folder);
    assert_eq!(
        fixed.description.as_deref(),
        Some("Keys without placeholders")
    );
    assert!(fixed.placeholders.is_empty());
    let leaders = presets.iter().find(|p| p.name == "leaders").unwrap();
    assert_eq!(leaders.placeholders, vec!["primary", "secondary"]);
    let unlock_first = presets.iter().find(|p| p.name == "unlock-first").unwrap();
    assert_eq!(
        unlock_first.placeholders,
        vec!["primary", "secondary", "unlock"]
    );
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].name, "broken");
}

#[test]
fn copying_a_preset_writes_it_under_a_new_name() {
    let dir = tempfile::tempdir().unwrap();
    let keybinds_dir = dir.path().join("keybinds");
    let path = copy_keybind_preset_to_folder("unlock-first", "mine", &keybinds_dir, None).unwrap();
    assert_eq!(path, keybinds_dir.join("mine.kdl"));
    let preset = KeybindPreset::from_kdl(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(preset.display_name.as_deref(), Some("mine"));
    assert!(copy_keybind_preset_to_folder("default", "mine", &keybinds_dir, None).is_err());
    assert!(copy_keybind_preset_to_folder("default", "no/slashes", &keybinds_dir, None).is_err());
}

#[test]
fn dump_keybinds_prints_the_builtin_presets() {
    assert_eq!(
        crate::setup::specified_keybinds_text("default").as_deref(),
        Some(DEFAULT_KEYBIND_PRESET_ASSET)
    );
    assert_eq!(
        crate::setup::specified_keybinds_text("unlock-first").as_deref(),
        Some(UNLOCK_FIRST_KEYBIND_PRESET_ASSET)
    );
    assert_eq!(
        crate::setup::specified_keybinds_text("does-not-exist"),
        None
    );
}

#[test]
fn an_empty_keybinds_block_with_only_attributes_is_valid() {
    let config = user_config("keybinds preset=\"default\" {\n}");
    assert_eq!(*config.keybinds, today_default_keybinds());
}

fn command_line_options(preset: Option<&str>, primary: Option<&str>) -> Options {
    Options {
        keybinds_preset: preset.map(|p| p.to_owned()),
        keybinds_primary: primary.map(|p| p.to_owned()),
        ..Default::default()
    }
}

#[test]
fn a_command_line_preset_beats_the_config_and_the_layout() {
    let config = user_config("keybinds preset=\"default\"");
    let mut config = Config::from_layout_kdl("keybinds preset=\"default\"", Some(config)).unwrap();
    config.apply_command_line_keybinds(&command_line_options(Some("unlock-first"), None));
    assert_eq!(config.keybinds_layers.active.info.name, "unlock-first");
    assert_eq!(
        *config.keybinds,
        keybinds_from_fixture(EXPECTED_UNLOCK_FIRST_KEYBINDS)
    );
    assert_eq!(config.options.default_mode, Some(InputMode::Locked));
    let snapshot = config.keybinds_selection_snapshot();
    assert!(snapshot.set_on_command_line);
    assert!(!snapshot.set_by_layout);
}

#[test]
fn command_line_leader_values_beat_the_config_values_for_the_same_preset() {
    let mut config = user_config("keybinds primary=\"Super\" secondary=\"Ctrl\"");
    config.apply_command_line_keybinds(&command_line_options(None, Some("Alt")));
    assert_eq!(
        config
            .keybinds_layers
            .active
            .values
            .get("primary")
            .map(|v| v.as_str()),
        Some("Alt")
    );
    assert_eq!(
        config
            .keybinds_layers
            .active
            .values
            .get("secondary")
            .map(|v| v.as_str()),
        Some("Ctrl")
    );
}

#[test]
fn config_leader_values_are_not_used_for_a_different_command_line_preset() {
    let mut config = user_config("keybinds primary=\"Super\" secondary=\"Ctrl\"");
    config.apply_command_line_keybinds(&command_line_options(Some("unlock-first"), None));
    assert_eq!(
        config
            .keybinds_layers
            .active
            .values
            .get("primary")
            .map(|v| v.as_str()),
        Some("Ctrl")
    );
    assert_eq!(
        config
            .keybinds_layers
            .active
            .values
            .get("secondary")
            .map(|v| v.as_str()),
        Some("Alt")
    );
}

#[test]
fn command_line_keybinds_are_neither_written_nor_read_from_the_config_file() {
    let mut config = user_config("mouse_mode false");
    config.apply_command_line_keybinds(&command_line_options(Some("unlock-first"), Some("Alt")));
    let text = config.to_string(false);
    assert!(!text.contains("unlock-first"), "{}", text);
    assert!(!text.contains("keybinds_preset"), "{}", text);
    assert!(
        !text
            .lines()
            .any(|line| line.trim_start().starts_with("default_mode")),
        "{}",
        text
    );
    let read = user_config("keybinds_preset \"unlock-first\"\nkeybinds_primary \"Alt\"");
    assert_eq!(read.options.keybinds_preset, None);
    assert_eq!(read.options.keybinds_primary, None);
    assert_eq!(read.keybinds_layers.active.info.name, "default");
}

#[test]
fn clearing_command_line_keybinds_returns_to_the_config_preset() {
    let mut config = user_config("mouse_mode false");
    config.apply_command_line_keybinds(&command_line_options(Some("unlock-first"), None));
    config.clear_command_line_keybinds();
    config.resolve_keybinds();
    assert_eq!(*config.keybinds, today_default_keybinds());
    assert_eq!(config.options.default_mode, None);
}

#[test]
fn a_relative_command_line_preset_path_is_resolved_from_the_current_directory() {
    let mut options = command_line_options(Some("presets/mine.kdl"), None);
    command_line_preset_relative_to(&mut options, Path::new("/work"));
    assert_eq!(
        options.keybinds_preset,
        Some(
            Path::new("/work")
                .join("presets/mine.kdl")
                .display()
                .to_string()
        )
    );
    let mut options = command_line_options(Some("unlock-first"), None);
    command_line_preset_relative_to(&mut options, Path::new("/work"));
    assert_eq!(options.keybinds_preset.as_deref(), Some("unlock-first"));
}

#[test]
fn keybinds_preset_flags_are_taken_from_zellij_options_and_attach_options() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(keybinds_preset_flags_are_parsed)
        .unwrap()
        .join()
        .unwrap();
}

fn keybinds_preset_flags_are_parsed() {
    use crate::cli::CliArgs;
    use crate::setup::Setup;
    use clap::Parser;
    let from_options = CliArgs::try_parse_from([
        "zellij",
        "options",
        "--keybinds-preset",
        "unlock-first",
        "--keybinds-unlock",
        "Ctrl u",
    ])
    .unwrap();
    let (config, _, config_options, _, _) = Setup::from_cli_args(&from_options).unwrap();
    assert_eq!(
        config_options.keybinds_preset.as_deref(),
        Some("unlock-first")
    );
    assert_eq!(config_options.keybinds_unlock.as_deref(), Some("Ctrl u"));
    assert_eq!(config.keybinds_layers.active.info.name, "unlock-first");
    assert_eq!(
        config
            .keybinds_layers
            .active
            .values
            .get("unlock")
            .map(|v| v.as_str()),
        Some("Ctrl u")
    );
    let from_attach = CliArgs::try_parse_from([
        "zellij",
        "attach",
        "some-session",
        "options",
        "--keybinds-preset",
        "unlock-first",
    ])
    .unwrap();
    let (_, _, config_options, _, _) = Setup::from_cli_args(&from_attach).unwrap();
    assert_eq!(
        config_options.keybinds_preset.as_deref(),
        Some("unlock-first")
    );
}

#[test]
fn preset_headers_carry_example_keys_for_the_settings_screen() {
    let (presets, _) = list_keybind_presets(None, &[]);
    let unlock_first = presets.iter().find(|p| p.name == "unlock-first").unwrap();
    assert_eq!(
        unlock_first.examples,
        vec![
            ("{unlock} + p".to_owned(), "to enter PANE mode".to_owned()),
            ("{unlock} + t".to_owned(), "to enter TAB mode".to_owned()),
        ]
    );
    let error =
        KeybindPreset::from_kdl("preset {\n example \"Ctrl p\"\n}\nkeybinds {\n}").unwrap_err();
    assert!(error.contains("two strings"), "{}", error);
}

#[test]
fn a_command_line_preset_overrides_a_config_block_that_clears_the_defaults() {
    let mut config = user_config(
        r#"
        keybinds clear-defaults=true {
            normal {
                bind "Ctrl r" { SwitchToMode "Locked"; }
            }
        }
        "#,
    );
    config.apply_command_line_keybinds(&command_line_options(Some("default"), None));
    assert_eq!(*config.keybinds, today_default_keybinds());
    assert!(config
        .to_string(false)
        .contains("keybinds clear-defaults=true"));
    config.clear_command_line_keybinds();
    config.resolve_keybinds();
    assert_eq!(
        actions_for(&config, InputMode::Normal, "Ctrl r"),
        Some(switch_to(InputMode::Locked))
    );
    assert_eq!(actions_for(&config, InputMode::Normal, "Ctrl p"), None);
}

#[test]
fn a_command_line_preset_keeps_config_additions_that_do_not_clear_the_defaults() {
    let mut config = user_config(
        r#"
        keybinds {
            normal {
                bind "Alt y" { SwitchToMode "Tab"; }
            }
        }
        "#,
    );
    config.apply_command_line_keybinds(&command_line_options(Some("unlock-first"), None));
    assert_eq!(
        actions_for(&config, InputMode::Normal, "Alt y"),
        Some(switch_to(InputMode::Tab))
    );
}

#[test]
fn a_command_line_preset_overrides_the_config_default_mode_without_changing_the_file() {
    let mut config = user_config("default_mode \"locked\"");
    config.apply_command_line_keybinds(&command_line_options(Some("default"), None));
    assert_eq!(config.options.default_mode, Some(InputMode::Normal));
    let text = config.to_string(false);
    assert!(text.contains("default_mode \"locked\""), "{}", text);
    assert_eq!(
        crate::input::config_settings::setting_value(&config, crate::data::SettingKey::DefaultMode),
        Some("locked".to_owned())
    );
    config.clear_command_line_keybinds();
    config.resolve_keybinds();
    assert_eq!(config.options.default_mode, Some(InputMode::Locked));

    let mut config = user_config("default_mode \"normal\"");
    config.apply_command_line_keybinds(&command_line_options(Some("unlock-first"), None));
    assert_eq!(config.options.default_mode, Some(InputMode::Locked));
}

#[test]
fn an_explicit_command_line_default_mode_beats_the_command_line_preset() {
    let mut config = user_config("default_mode \"locked\"");
    config.options.default_mode = Some(InputMode::Pane);
    config.apply_command_line_keybinds(&command_line_options(Some("unlock-first"), None));
    assert_eq!(config.options.default_mode, Some(InputMode::Pane));
}

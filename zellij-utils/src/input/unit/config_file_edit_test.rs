use super::super::actions::Action;
use super::super::config::Config;
use super::super::config_file_edit::*;
use super::super::config_settings::config_settings_test::every_setting_set;
use super::super::config_settings::{settings_set_in_file, unset_setting};
use super::super::keybind_presets::KeybindChanges;
use crate::data::{InputMode, KeyWithModifier, SettingKey};
use std::str::FromStr;

fn defaults() -> Config {
    Config::from_default_assets().unwrap()
}

fn load(text: &str) -> Config {
    Config::from_kdl(text, Some(defaults())).unwrap()
}

fn reconfigure(config: &Config, kdl: &str) -> Config {
    Config::from_kdl(kdl, Some(config.clone())).unwrap()
}

fn saved_text_after(file: &str, runtime: impl Fn(&Config) -> Config) -> String {
    let saved = load(file);
    let runtime = runtime(&saved);
    let edits = edits_between(&saved, &runtime);
    let text = apply_edits(Some(file), &edits, &runtime.keybinds_layers.user.changes).unwrap();
    written_file_matches(&text, &runtime, &edits).unwrap();
    text
}

fn new_tab() -> Vec<Action> {
    load("keybinds { normal { bind \"Alt u\" { NewTab; }; }; }")
        .keybinds_layers
        .user
        .changes
        .modes[&InputMode::Normal]
        .bind[&key("Alt u")]
        .clone()
}

fn key(text: &str) -> KeyWithModifier {
    KeyWithModifier::from_str(text).unwrap()
}

fn rebind(
    config: &Config,
    unbind: &[(InputMode, &str)],
    bind: &[(InputMode, &str, Vec<Action>)],
) -> Config {
    let mut changes = KeybindChanges::default();
    for (mode, key_text) in unbind {
        changes.unbind(*mode, key(key_text));
    }
    for (mode, key_text, actions) in bind {
        changes.bind(*mode, key(key_text), actions.clone());
    }
    let mut rebound = config.clone();
    rebound.keybinds_layers.user.changes.compose(changes);
    rebound.resolve_keybinds();
    rebound
}

const COMMENTED_FILE: &str = "// my settings
// are here

mouse_mode false // I like it off

plugins {
    // a plugin I wrote
    my-plugin location=\"file:/tmp/plugin.wasm\"
}

/* block comment */
pane_frames true
";

#[test]
fn an_unchanged_file_is_written_back_byte_for_byte() {
    let saved = load(COMMENTED_FILE);
    let text = apply_edits(Some(COMMENTED_FILE), &[], &KeybindChanges::default()).unwrap();
    assert_eq!(text, COMMENTED_FILE);
    assert!(edits_between(&saved, &saved).is_empty());
}

#[test]
fn a_changed_value_is_edited_in_place_keeping_comments_and_unrelated_nodes() {
    let text = saved_text_after(COMMENTED_FILE, |saved| reconfigure(saved, "mouse_mode true"));
    assert_eq!(text, COMMENTED_FILE.replace("mouse_mode false", "mouse_mode true"));
}

#[test]
fn a_new_value_is_added_at_the_end_of_the_top_level() {
    let text = saved_text_after(COMMENTED_FILE, |saved| {
        reconfigure(saved, "theme \"nord\"\nscroll_buffer_size 2000")
    });
    let expected = format!(
        "{}{}",
        COMMENTED_FILE, "theme \"nord\"\nscroll_buffer_size 2000\n"
    );
    assert_eq!(text, expected);
}

#[test]
fn a_file_without_a_final_newline_gets_the_new_value_on_its_own_line() {
    let file = "mouse_mode false";
    let text = saved_text_after(file, |saved| reconfigure(saved, "pane_frames false"));
    assert_eq!(text, "mouse_mode false\npane_frames false\n");
}

#[test]
fn a_value_reset_to_the_default_is_removed_only_when_the_file_sets_it() {
    let text = saved_text_after(COMMENTED_FILE, |saved| {
        let mut runtime = saved.clone();
        unset_setting(&mut runtime, SettingKey::MouseMode);
        runtime
    });
    assert_eq!(
        text,
        "// my settings
// are here


plugins {
    // a plugin I wrote
    my-plugin location=\"file:/tmp/plugin.wasm\"
}

/* block comment */
pane_frames true
"
    );
    let file = "pane_frames true\n";
    let saved = load(file);
    let mut runtime = reconfigure(&saved, "copy_on_select false");
    let edits = edits_between(&load(file), &runtime);
    assert_eq!(
        apply_edits(Some(file), &edits, &runtime.keybinds_layers.user.changes).unwrap(),
        "pane_frames true\ncopy_on_select false\n"
    );
    unset_setting(&mut runtime, SettingKey::CopyOnSelect);
    let edits = edits_between(&saved, &runtime);
    assert!(edits.is_empty());
}

#[test]
fn a_reset_value_set_twice_in_the_file_is_removed_everywhere() {
    let file = "theme \"nord\"\nmouse_mode true\ntheme \"dracula\"\n";
    let text = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        unset_setting(&mut runtime, SettingKey::Theme);
        runtime
    });
    assert_eq!(text, "mouse_mode true\n");
}

#[test]
fn pane_frame_settings_go_inside_ui_and_pane_frames_creating_them_when_missing() {
    let text = saved_text_after("mouse_mode true\n", |saved| {
        reconfigure(saved, "ui { pane_frames { rounded_corners true; }; }")
    });
    assert_eq!(
        text,
        "mouse_mode true\nui {\n    pane_frames {\n        rounded_corners true\n    }\n}\n"
    );
}

#[test]
fn pane_frame_settings_join_an_existing_block_with_its_indentation() {
    let file = "ui {\n  // frames\n  pane_frames {\n    hide_session_name true\n  }\n}\n";
    let text = saved_text_after(file, |saved| {
        reconfigure(saved, "ui { pane_frames { rounded_corners true; }; }")
    });
    assert_eq!(
        text,
        "ui {\n  // frames\n  pane_frames {\n    hide_session_name true\n    rounded_corners true\n  }\n}\n"
    );
}

#[test]
fn a_ui_block_without_pane_frames_gets_one() {
    let file = "ui {\n}\n";
    let text = saved_text_after(file, |saved| {
        reconfigure(saved, "ui { pane_frames { border_style \"heavy\"; }; }")
    });
    assert_eq!(
        text,
        "ui {\n    pane_frames {\n        border_style \"heavy\"\n    }\n}\n"
    );
}

#[test]
fn web_client_settings_go_inside_web_client() {
    let file = "web_client {\n    font \"monospace\"\n}\n";
    let text = saved_text_after(file, |saved| {
        reconfigure(saved, "web_client { font_size 14; font \"Fira Code\"; }")
    });
    assert_eq!(
        text,
        "web_client {\n    font \"Fira Code\"\n    font_size 14\n}\n"
    );
    let text = saved_text_after("", |saved| {
        reconfigure(saved, "web_client { cursor_blink true; }")
    });
    assert_eq!(text, "web_client {\n    cursor_blink true\n}\n");
}

#[test]
fn a_minimal_file_is_created_when_none_exists() {
    let saved = defaults();
    let runtime = reconfigure(
        &saved,
        "mouse_mode false\nkeybinds preset=\"unlock-first\"\nui { pane_frames { rounded_corners true; }; }",
    );
    let edits = edits_between(&saved, &runtime);
    let text = apply_edits(None, &edits, &runtime.keybinds_layers.user.changes).unwrap();
    assert_eq!(
        text,
        "mouse_mode false\nui {\n    pane_frames {\n        rounded_corners true\n    }\n}\nkeybinds preset=\"unlock-first\"\n"
    );
    written_file_matches(&text, &runtime, &edits).unwrap();
}

#[test]
fn every_setting_round_trips_through_a_new_file() {
    let saved = defaults();
    let mut runtime = every_setting_set();
    runtime.keybinds_layers.user = Default::default();
    runtime.resolve_keybinds();
    let edits = edits_between(&saved, &runtime);
    let text = apply_edits(None, &edits, &runtime.keybinds_layers.user.changes).unwrap();
    written_file_matches(&text, &runtime, &edits).unwrap();
    let set_in_file = settings_set_in_file(&text);
    for key in SettingKey::all() {
        if key != SettingKey::Keybinds {
            assert!(set_in_file.contains(&key), "{} was not written", key);
        }
    }
}

#[test]
fn keybinds_attributes_are_added_changed_and_removed_in_place() {
    let file = "// keys\nkeybinds preset=\"default\" secondary=\"Alt\" {\n    normal {\n        bind \"Alt y\" { NewTab; }\n    }\n}\n";
    let text = saved_text_after(file, |saved| {
        let mut runtime = reconfigure(saved, "keybinds preset=\"unlock-first\" primary=\"Ctrl\"");
        runtime.keybinds_layers.user.secondary = None;
        runtime.resolve_keybinds();
        runtime
    });
    assert_eq!(
        text,
        "// keys\nkeybinds preset=\"unlock-first\" primary=\"Ctrl\" {\n    normal {\n        bind \"Alt y\" { NewTab; }\n    }\n}\n"
    );
}

#[test]
fn a_rebound_key_is_added_to_its_mode_block() {
    let file = "keybinds {\n    // my normal keys\n    normal {\n        bind \"Alt y\" { NewTab; }\n    }\n}\n";
    let text = saved_text_after(file, |saved| {
        rebind(
            saved,
            &[],
            &[(InputMode::Normal, "Alt u", new_tab())],
        )
    });
    assert_eq!(
        text,
        "keybinds {\n    // my normal keys\n    normal {\n        bind \"Alt y\" { NewTab; }\n        bind \"Alt u\" { NewTab; }\n    }\n}\n"
    );
}

#[test]
fn an_unbound_key_creates_its_mode_block_and_keeps_clear_defaults() {
    let file = "keybinds clear-defaults=true {\n    normal {\n        bind \"Alt y\" \"Alt u\" { NewTab; }\n    }\n}\n";
    let text = saved_text_after(file, |saved| {
        rebind(saved, &[(InputMode::Normal, "Alt y"), (InputMode::Locked, "Ctrl g")], &[])
    });
    assert_eq!(
        text,
        "keybinds clear-defaults=true {\n    normal {\n        bind \"Alt u\" { NewTab; }\n        unbind \"Alt y\"\n    }\n    locked {\n        unbind \"Ctrl g\"\n    }\n}\n"
    );
}

#[test]
fn a_key_back_to_the_preset_loses_only_its_own_bind_or_unbind() {
    let file = "keybinds {\n    normal {\n        bind \"Alt y\" { NewTab; }\n        unbind \"Alt n\"\n        bind \"Alt u\" { NewTab; }\n    }\n}\n";
    let text = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        let normal = runtime
            .keybinds_layers
            .user
            .changes
            .modes
            .get_mut(&InputMode::Normal)
            .unwrap();
        normal.bind.remove(&key("Alt y"));
        normal.unbind.remove(&key("Alt n"));
        runtime.resolve_keybinds();
        runtime
    });
    assert_eq!(
        text,
        "keybinds {\n    normal {\n        bind \"Alt u\" { NewTab; }\n    }\n}\n"
    );
}

#[test]
fn a_key_back_to_the_preset_in_one_mode_splits_a_shared_block() {
    let file = "keybinds {\n    shared_among \"normal\" \"locked\" {\n        bind \"Alt y\" { NewTab; }\n    }\n}\n";
    let text = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        runtime
            .keybinds_layers
            .user
            .changes
            .modes
            .get_mut(&InputMode::Normal)
            .unwrap()
            .bind
            .remove(&key("Alt y"));
        runtime.resolve_keybinds();
        runtime
    });
    assert_eq!(
        text,
        "keybinds {\n    shared_among \"normal\" \"locked\" {\n    }\n    locked {\n        bind \"Alt y\" { NewTab; }\n    }\n}\n"
    );
}

#[test]
fn switching_away_from_a_clear_defaults_block_replaces_its_keys() {
    let file = "keybinds clear-defaults=true {\n    normal {\n        bind \"Alt y\" { NewTab; }\n    }\n}\ndefault_mode \"locked\"\n";
    let text = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        unset_setting(&mut runtime, SettingKey::Keybinds);
        unset_setting(&mut runtime, SettingKey::DefaultMode);
        reconfigure(&runtime, "keybinds preset=\"default\"")
    });
    assert_eq!(text, "keybinds preset=\"default\"\n");
}

#[test]
fn a_changed_context_menu_section_is_replaced_and_the_rest_kept() {
    let file = "context_menu {\n    // my tab items\n    tab {\n        item \"Hi\" { NewTab; }\n    }\n}\n";
    let text = saved_text_after(file, |saved| {
        reconfigure(saved, "context_menu { bar clear-defaults=true { item \"Only\" { NewTab; }; }; }")
    });
    assert_eq!(
        text,
        "context_menu {\n    // my tab items\n    tab {\n        item \"Hi\" { NewTab; }\n    }\n    bar clear-defaults=true {\n        item \"Only\" { NewTab; }\n    }\n}\n"
    );
}

fn temp_config(contents: Option<&str>) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.kdl");
    if let Some(contents) = contents {
        std::fs::write(&path, contents).unwrap();
    }
    (dir, path)
}

#[test]
fn saving_writes_a_backup_and_returns_the_new_saved_config() {
    let file = "// keep me\nmouse_mode false\n";
    let (_dir, path) = temp_config(Some(file));
    let saved = load(file);
    let runtime = reconfigure(&saved, "mouse_mode true");
    match save_config_in_place(&path, Some(file), &saved, &saved, &runtime, false) {
        SaveOutcome::Written {
            file_contents,
            saved_config,
        } => {
            assert_eq!(file_contents.as_deref(), Some("// keep me\nmouse_mode true\n"));
            assert_eq!(saved_config.options.mouse_mode, Some(true));
        },
        other => panic!("unexpected outcome {:?}", other),
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "// keep me\nmouse_mode true\n"
    );
    assert_eq!(
        std::fs::read_to_string(Config::backup_file_path(&path)).unwrap(),
        file
    );
}

#[test]
fn saving_without_a_file_creates_a_minimal_one_without_a_backup() {
    let (_dir, path) = temp_config(None);
    let saved = defaults();
    let runtime = reconfigure(&saved, "mouse_mode false");
    let outcome = save_config_in_place(&path, None, &saved, &saved, &runtime, false);
    assert!(matches!(outcome, SaveOutcome::Written { .. }));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "mouse_mode false\n"
    );
    assert!(!Config::backup_file_path(&path).exists());
}

#[test]
fn an_outside_change_is_detected_and_overwrite_edits_the_current_file() {
    let file = "mouse_mode false\n";
    let (_dir, path) = temp_config(Some(file));
    let saved = load(file);
    let runtime = reconfigure(&saved, "pane_frames false");
    let outside = "// edited elsewhere\nmouse_mode false\ntheme \"nord\"\n";
    std::fs::write(&path, outside).unwrap();
    assert_eq!(
        save_config_in_place(&path, Some(file), &saved, &saved, &runtime, false),
        SaveOutcome::ChangedOutside
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), outside);
    let outcome = save_config_in_place(&path, Some(file), &saved, &saved, &runtime, true);
    assert!(matches!(outcome, SaveOutcome::Written { .. }));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("{}pane_frames false\n", outside)
    );
}

#[test]
fn a_file_that_does_not_read_back_is_restored_from_the_backup() {
    let file = "keybinds {\n    normal {\n        not_a_bind \"x\"\n    }\n}\n";
    let (_dir, path) = temp_config(Some(file));
    let saved = defaults();
    let runtime = reconfigure(&saved, "mouse_mode false");
    let outcome = save_config_in_place(&path, Some(file), &saved, &saved, &runtime, false);
    assert_eq!(outcome, SaveOutcome::Failed(Some(path.clone())));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), file);
    assert_eq!(
        std::fs::read_to_string(Config::backup_file_path(&path)).unwrap(),
        file
    );
}

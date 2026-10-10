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
    let text = apply_edits(Some(file), &edits, &runtime).unwrap();
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
    let text = apply_edits(Some(COMMENTED_FILE), &[], &saved).unwrap();
    assert_eq!(text, COMMENTED_FILE);
    assert!(edits_between(&saved, &saved).is_empty());
}

#[test]
fn a_changed_value_is_edited_in_place_keeping_comments_and_unrelated_nodes() {
    let text = saved_text_after(COMMENTED_FILE, |saved| {
        reconfigure(saved, "mouse_mode true")
    });
    assert_eq!(
        text,
        COMMENTED_FILE.replace("mouse_mode false", "mouse_mode true")
    );
}

#[test]
fn nested_session_ancestor_tab_highlight_is_saved_updated_and_removed() {
    let file = "// keep this comment\nnested_session_handling \"ask\"\n";
    let added = saved_text_after(file, |saved| {
        reconfigure(saved, "nested_session_ancestor_tab_highlight false")
    });
    assert_eq!(
        added,
        format!("{file}nested_session_ancestor_tab_highlight false\n")
    );
    let updated = saved_text_after(&added, |saved| {
        reconfigure(saved, "nested_session_ancestor_tab_highlight true")
    });
    assert_eq!(
        updated,
        format!("{file}nested_session_ancestor_tab_highlight true\n")
    );
    let removed = saved_text_after(&updated, |saved| {
        let mut runtime = saved.clone();
        unset_setting(&mut runtime, SettingKey::NestedSessionAncestorTabHighlight);
        runtime
    });
    assert_eq!(removed, file);
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
        apply_edits(Some(file), &edits, &runtime).unwrap(),
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
    let text = apply_edits(None, &edits, &runtime).unwrap();
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
    let text = apply_edits(None, &edits, &runtime).unwrap();
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
        rebind(saved, &[], &[(InputMode::Normal, "Alt u", new_tab())])
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
        rebind(
            saved,
            &[(InputMode::Normal, "Alt y"), (InputMode::Locked, "Ctrl g")],
            &[],
        )
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

fn actions_of(kdl: &str) -> Vec<Action> {
    load(&format!(
        "keybinds {{ normal {{ bind \"Alt u\" {{ {} }}; }}; }}",
        kdl
    ))
    .keybinds_layers
    .user
    .changes
    .modes[&InputMode::Normal]
        .bind[&key("Alt u")]
        .clone()
}

fn back_to_preset(config: &Config, mode: InputMode, key_text: &str) -> Config {
    let mut runtime = config.clone();
    if let Some(mode_changes) = runtime.keybinds_layers.user.changes.modes.get_mut(&mode) {
        mode_changes.bind.remove(&key(key_text));
        mode_changes.unbind.remove(&key(key_text));
    }
    runtime.resolve_keybinds();
    runtime
}

#[test]
fn a_key_back_to_the_preset_in_one_mode_keeps_the_shared_among_block() {
    let file = "keybinds {\n    shared_among \"normal\" \"locked\" {\n        bind \"Alt y\" { NewTab; }\n    }\n}\n";
    let text = saved_text_after(file, |saved| {
        back_to_preset(saved, InputMode::Normal, "Alt y")
    });
    assert_eq!(
        text,
        "keybinds {\n    shared_among \"normal\" \"locked\" {\n        bind \"Alt y\" { NewTab; }\n    }\n    normal {\n        unbind \"Alt y\"\n    }\n}\n"
    );
}

#[test]
fn a_key_rebound_in_one_mode_adds_an_override_and_keeps_the_shared_block() {
    let file = "keybinds {\n    shared {\n        bind \"Alt y\" { NewTab; }\n    }\n}\n";
    let text = saved_text_after(file, |saved| {
        rebind(
            saved,
            &[],
            &[(InputMode::Normal, "Alt y", actions_of("Detach;"))],
        )
    });
    assert_eq!(
        text,
        "keybinds {\n    shared {\n        bind \"Alt y\" { NewTab; }\n    }\n    normal {\n        bind \"Alt y\" { Detach; }\n    }\n}\n"
    );
}

#[test]
fn a_key_unbound_in_one_mode_adds_an_unbind_and_keeps_the_shared_except_block() {
    let file = "keybinds {\n    shared_except \"locked\" {\n        bind \"Alt y\" { NewTab; }\n    }\n}\n";
    let text = saved_text_after(file, |saved| {
        rebind(saved, &[(InputMode::Pane, "Alt y")], &[])
    });
    assert_eq!(
        text,
        "keybinds {\n    shared_except \"locked\" {\n        bind \"Alt y\" { NewTab; }\n    }\n    pane {\n        unbind \"Alt y\"\n    }\n}\n"
    );
}

#[test]
fn a_mode_override_that_is_no_longer_needed_is_removed() {
    let file = "keybinds {\n    shared {\n        bind \"Alt y\" { NewTab; }\n    }\n    normal {\n        bind \"Alt y\" { Detach; }\n    }\n}\n";
    let text = saved_text_after(file, |saved| {
        rebind(saved, &[], &[(InputMode::Normal, "Alt y", new_tab())])
    });
    assert_eq!(
        text,
        "keybinds {\n    shared {\n        bind \"Alt y\" { NewTab; }\n    }\n    normal {\n    }\n}\n"
    );
}

#[test]
fn a_key_bound_in_one_mode_moves_a_top_level_unbind_into_a_shared_block() {
    let file = "keybinds {\n    unbind \"Alt y\"\n}\n";
    let text = saved_text_after(file, |saved| {
        rebind(
            saved,
            &[],
            &[(InputMode::Normal, "Alt y", actions_of("Detach;"))],
        )
    });
    assert_eq!(
        text,
        "keybinds {\n    shared {\n        unbind \"Alt y\"\n    }\n    normal {\n        bind \"Alt y\" { Detach; }\n    }\n}\n"
    );
}

#[test]
fn a_key_bound_in_one_mode_keeps_a_top_level_unbind_of_another_key() {
    let file = "keybinds {\n    unbind \"Alt y\" \"Alt z\"\n}\n";
    let text = saved_text_after(file, |saved| {
        rebind(saved, &[], &[(InputMode::Locked, "Alt y", new_tab())])
    });
    assert!(
        text.starts_with("keybinds {\n    unbind \"Alt z\"\n"),
        "{}",
        text
    );
    assert!(
        text.contains("shared {\n        unbind \"Alt y\"\n    }"),
        "{}",
        text
    );
    assert!(
        text.contains("locked {\n        bind \"Alt y\" { NewTab; }\n    }"),
        "{}",
        text
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
        reconfigure(
            saved,
            "context_menu { bar clear-defaults=true { item \"Only\" { NewTab; }; }; }",
        )
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
            changed_outside,
        } => {
            assert!(!changed_outside);
            assert_eq!(
                file_contents.as_deref(),
                Some("// keep me\nmouse_mode true\n")
            );
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

#[test]
fn a_plugin_alias_is_added_changed_and_removed_keeping_comments() {
    let added = saved_text_after(COMMENTED_FILE, |saved| {
        reconfigure(saved, "plugins { other location=\"zellij:strider\"; }")
    });
    assert_eq!(
        added,
        COMMENTED_FILE.replace(
            "    my-plugin location=\"file:/tmp/plugin.wasm\"\n",
            "    my-plugin location=\"file:/tmp/plugin.wasm\"\n    other location=\"zellij:strider\"\n"
        )
    );
    let changed = saved_text_after(COMMENTED_FILE, |saved| {
        reconfigure(
            saved,
            "plugins { my-plugin location=\"file:/tmp/other.wasm\"; }",
        )
    });
    assert_eq!(
        changed,
        COMMENTED_FILE.replace("/tmp/plugin.wasm", "/tmp/other.wasm")
    );
    let removed = saved_text_after(COMMENTED_FILE, |saved| {
        let mut runtime = saved.clone();
        runtime.plugins.aliases.remove("my-plugin");
        runtime
    });
    assert!(!removed.contains("my-plugin"));
    assert!(
        removed.starts_with("// my settings\n// are here\n\nmouse_mode false // I like it off\n")
    );
    assert!(removed.ends_with("/* block comment */\npane_frames true\n"));
}

#[test]
fn an_overridden_built_in_alias_back_at_its_default_is_removed_from_the_file() {
    let file = "plugins {\n    tab-bar location=\"zellij:strider\"\n}\n";
    let text = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        runtime.plugins.aliases.insert(
            "tab-bar".to_owned(),
            defaults().plugins.aliases["tab-bar"].clone(),
        );
        runtime
    });
    assert_eq!(text, "plugins {\n}\n");
}

#[test]
fn env_variables_are_added_changed_and_removed_in_place() {
    let file = "env {\n    // mine\n    A \"1\" // first\n}\n";
    let changed = saved_text_after(file, |saved| {
        reconfigure(saved, "env { A \"2\"; B \"3\"; }")
    });
    assert_eq!(
        changed,
        "env {\n    // mine\n    A \"2\" // first\n    B \"3\"\n}\n"
    );
    let removed = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        runtime.env = crate::envs::EnvironmentVariables::default();
        runtime
    });
    assert_eq!(removed, "");
    let edits = edits_between(&load(file), &reconfigure(&load(file), "env { B \"3\"; }"));
    assert_eq!(
        edits,
        vec![ConfigEdit::EnvVar {
            name: "B".to_owned(),
            value: Some("3".to_owned())
        }]
    );
}

#[test]
fn load_plugins_are_added_reordered_and_removed_keeping_comments() {
    let file = "load_plugins {\n    // first\n    \"zellij:strider\"\n    \"zellij:about\"\n}\n";
    let reordered = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        runtime.background_plugins.swap(0, 1);
        runtime
    });
    assert_eq!(
        reordered,
        "load_plugins {\n    \"zellij:about\"\n    // first\n    \"zellij:strider\"\n}\n"
    );
    let added = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        let mut more = load("load_plugins { \"zellij:bars\"; }").background_plugins;
        runtime.background_plugins.append(&mut more);
        runtime
    });
    assert_eq!(
        added,
        "load_plugins {\n    // first\n    \"zellij:strider\"\n    \"zellij:about\"\n    \"zellij:bars\"\n}\n"
    );
    let removed = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        runtime.background_plugins.remove(1);
        runtime
    });
    assert_eq!(
        removed,
        "load_plugins {\n    // first\n    \"zellij:strider\"\n}\n"
    );
}

#[test]
fn context_menu_items_are_edited_one_by_one_inside_their_section() {
    let file = "context_menu {\n    pane clear-defaults=true {\n        // mine\n        item \"A\" { NewTab; }\n        item \"B\" { NewPane; }\n    }\n}\n";
    let added = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        let mut extra = load(
            "context_menu { pane clear-defaults=true { separator; item \"C\" { Detach; }; }; }",
        )
        .context_menu
        .pane;
        runtime.context_menu.pane.append(&mut extra);
        runtime
    });
    assert_eq!(
        added,
        "context_menu {\n    pane clear-defaults=true {\n        // mine\n        item \"A\" { NewTab; }\n        item \"B\" { NewPane; }\n        separator\n        item \"C\" { Detach; }\n    }\n}\n"
    );
    let moved = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        runtime.context_menu.pane.swap(0, 1);
        runtime
    });
    assert_eq!(
        moved,
        "context_menu {\n    pane clear-defaults=true {\n        item \"B\" { NewPane; }\n        // mine\n        item \"A\" { NewTab; }\n    }\n}\n"
    );
    let removed = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        runtime.context_menu.pane.remove(1);
        runtime
    });
    assert_eq!(
        removed,
        "context_menu {\n    pane clear-defaults=true {\n        // mine\n        item \"A\" { NewTab; }\n    }\n}\n"
    );
    let restored = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        runtime.context_menu.pane = defaults().context_menu.pane.clone();
        runtime
    });
    assert_eq!(restored, "context_menu {\n}\n");
}

const BAR_FILE: &str =
    "context_menu {\n    bar {\n        // mine\n        item \"Mine\" { Detach; }\n    }\n}\n";

fn menu_item(label: &str) -> crate::data::ContextMenuEntry {
    crate::data::ContextMenuEntry::item(label, new_tab().into_iter().map(Into::into).collect())
}

#[test]
fn a_default_item_removed_from_a_merged_section_is_written_as_remove() {
    let text = saved_text_after(BAR_FILE, |saved| {
        let mut runtime = saved.clone();
        runtime.context_menu.bar.remove(0);
        runtime
    });
    assert_eq!(
        text,
        "context_menu {\n    bar {\n        // mine\n        item \"Mine\" { Detach; }\n        remove \"New pane\"\n    }\n}\n"
    );
}

#[test]
fn an_item_added_to_a_merged_section_is_written_without_clear_defaults() {
    let text = saved_text_after(BAR_FILE, |saved| {
        let mut runtime = saved.clone();
        runtime.context_menu.bar.push(menu_item("Extra"));
        runtime
    });
    assert_eq!(
        text,
        "context_menu {\n    bar {\n        // mine\n        item \"Mine\" { Detach; }\n        item \"Extra\" { NewTab; }\n    }\n}\n"
    );
    let text = saved_text_after(BAR_FILE, |saved| {
        let mut runtime = saved.clone();
        runtime.context_menu.bar.insert(1, menu_item("Middle"));
        runtime
    });
    assert!(
        text.contains("item \"Middle\" after=\"New pane\" { NewTab; }"),
        "{}",
        text
    );
    assert!(!text.contains("clear-defaults"), "{}", text);
}

#[test]
fn a_changed_default_item_is_written_as_an_item_with_its_label() {
    let text = saved_text_after(BAR_FILE, |saved| {
        let mut runtime = saved.clone();
        runtime.context_menu.bar[1] =
            crate::data::ContextMenuEntry::item("New tab", vec![Action::Detach.into()]);
        runtime
    });
    assert_eq!(
        text,
        "context_menu {\n    bar {\n        // mine\n        item \"Mine\" { Detach; }\n        item \"New tab\" { Detach; }\n    }\n}\n"
    );
}

#[test]
fn a_moved_default_item_and_a_new_separator_are_placed_relative_to_other_items() {
    let moved = saved_text_after(BAR_FILE, |saved| {
        let mut runtime = saved.clone();
        runtime.context_menu.bar.swap(0, 1);
        runtime
    });
    assert!(
        moved.contains("item \"New pane\" after=\"New tab\" {"),
        "{}",
        moved
    );
    assert!(
        moved.contains("        // mine\n        item \"Mine\" { Detach; }\n"),
        "{}",
        moved
    );
    assert!(!moved.contains("clear-defaults"), "{}", moved);
    let separated = saved_text_after(BAR_FILE, |saved| {
        let mut runtime = saved.clone();
        runtime
            .context_menu
            .bar
            .insert(1, crate::data::ContextMenuEntry::Separator);
        runtime
    });
    assert!(
        separated.contains("separator after=\"New pane\""),
        "{}",
        separated
    );
    assert!(!separated.contains("clear-defaults"), "{}", separated);
}

#[test]
fn a_changed_own_item_in_a_merged_section_keeps_its_comment_and_other_statements() {
    let file = "context_menu {\n    bar {\n        remove \"Gone\"\n        // mine\n        item \"Mine\" { Detach; }\n    }\n}\n";
    let text = saved_text_after(file, |saved| {
        let mut runtime = saved.clone();
        let last = runtime.context_menu.bar.len() - 1;
        runtime.context_menu.bar[last] = menu_item("Mine");
        runtime
    });
    assert_eq!(
        text,
        "context_menu {\n    bar {\n        remove \"Gone\"\n        // mine\n        item \"Mine\" { NewTab; }\n    }\n}\n"
    );
}

#[test]
fn a_merged_section_back_at_the_defaults_is_removed() {
    let text = saved_text_after(BAR_FILE, |saved| {
        let mut runtime = saved.clone();
        runtime.context_menu.bar = defaults().context_menu.bar.clone();
        runtime
    });
    assert_eq!(text, "context_menu {\n}\n");
}

#[test]
fn a_file_without_merge_statements_reads_as_before() {
    let file = "context_menu {\n    bar {\n        item \"New tab\" { Detach; }\n        separator\n        item \"Mine\" { Detach; }\n    }\n}\n";
    let config = load(file);
    let mut expected = defaults().context_menu.bar.clone();
    expected[1] = crate::data::ContextMenuEntry::item("New tab", vec![Action::Detach.into()]);
    expected.push(crate::data::ContextMenuEntry::Separator);
    expected.push(crate::data::ContextMenuEntry::item(
        "Mine",
        vec![Action::Detach.into()],
    ));
    assert_eq!(config.context_menu.bar, expected);
}

const THEME_FILE: &str = "themes {\n    // my theme\n    mine {\n        text_unselected {\n            base 1 2 3 // fg\n            emphasis_0 4\n            emphasis_1 5\n            emphasis_2 6\n            emphasis_3 7\n        }\n    }\n}\n";

#[test]
fn a_theme_colour_is_changed_in_place() {
    let text = saved_text_after(THEME_FILE, |saved| {
        let mut runtime = saved.clone();
        let mut theme = runtime.themes.get_theme("mine").unwrap().clone();
        theme.palette.text_unselected.base = crate::data::PaletteColor::Rgb((9, 8, 7));
        runtime.themes.insert("mine".to_owned(), theme);
        runtime
    });
    assert_eq!(
        text,
        THEME_FILE.replace("base 1 2 3 // fg", "base 9 8 7 // fg")
    );
}

#[test]
fn a_new_theme_is_added_and_a_deleted_one_removed() {
    let added = saved_text_after(THEME_FILE, |saved| {
        let mut runtime = saved.clone();
        let theme = runtime.themes.get_theme("mine").unwrap().clone();
        runtime.themes.insert("copy".to_owned(), theme);
        runtime
    });
    assert!(
        added.starts_with(&THEME_FILE[..THEME_FILE.len() - 2]),
        "{}",
        added
    );
    assert!(
        added.contains("    copy {\n        text_unselected {"),
        "{}",
        added
    );
    let removed = saved_text_after(THEME_FILE, |saved| {
        let mut runtime = saved.clone();
        runtime.themes.remove("mine");
        runtime
    });
    assert_eq!(removed, "");
}

#[test]
fn every_new_edit_keeps_an_unrelated_file_byte_for_byte_when_nothing_changed() {
    for file in [COMMENTED_FILE, THEME_FILE] {
        let saved = load(file);
        assert!(edits_between(&saved, &saved).is_empty());
    }
}

fn written(outcome: SaveOutcome) -> (Option<String>, bool) {
    match outcome {
        SaveOutcome::Written {
            file_contents,
            changed_outside,
            ..
        } => (file_contents, changed_outside),
        other => panic!("unexpected outcome {:?}", other),
    }
}

#[test]
fn an_overwrite_save_keeps_an_env_variable_added_outside() {
    let file = "env {\n    A \"1\"\n}\n";
    let (_dir, path) = temp_config(Some(file));
    let saved = load(file);
    let runtime = reconfigure(&saved, "env { B \"2\"; }");
    let outside = "env {\n    A \"1\"\n    C \"3\"\n}\n";
    std::fs::write(&path, outside).unwrap();
    let (contents, changed_outside) = written(save_config_in_place(
        &path,
        Some(file),
        &saved,
        &saved,
        &runtime,
        true,
    ));
    assert!(changed_outside);
    let expected = "env {\n    A \"1\"\n    C \"3\"\n    B \"2\"\n}\n";
    assert_eq!(contents.as_deref(), Some(expected));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
}

#[test]
fn an_overwrite_save_keeps_a_menu_item_added_outside_to_the_same_section() {
    let file = "context_menu {\n    pane clear-defaults=true {\n        item \"A\" { NewTab; }\n        item \"B\" { NewTab; }\n    }\n}\n";
    let (_dir, path) = temp_config(Some(file));
    let saved = load(file);
    let mut runtime = saved.clone();
    runtime.context_menu.pane.push(menu_item("C"));
    let outside = "context_menu {\n    pane clear-defaults=true {\n        item \"A\" { NewTab; }\n        item \"X\" { Detach; }\n        item \"B\" { NewTab; }\n    }\n}\n";
    std::fs::write(&path, outside).unwrap();
    let (_, changed_outside) = written(save_config_in_place(
        &path,
        Some(file),
        &saved,
        &saved,
        &runtime,
        true,
    ));
    assert!(changed_outside);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "context_menu {\n    pane clear-defaults=true {\n        item \"A\" { NewTab; }\n        item \"X\" { Detach; }\n        item \"B\" { NewTab; }\n        item \"C\" { NewTab; }\n    }\n}\n"
    );
}

#[test]
fn an_overwrite_save_of_a_merged_section_keeps_an_item_added_outside() {
    let (_dir, path) = temp_config(Some(BAR_FILE));
    let saved = load(BAR_FILE);
    let mut runtime = saved.clone();
    runtime.context_menu.bar.remove(0);
    let outside = BAR_FILE.replace(
        "        item \"Mine\" { Detach; }\n",
        "        item \"Mine\" { Detach; }\n        item \"Other\" { NewTab; }\n",
    );
    std::fs::write(&path, &outside).unwrap();
    let (_, changed_outside) = written(save_config_in_place(
        &path,
        Some(BAR_FILE),
        &saved,
        &saved,
        &runtime,
        true,
    ));
    assert!(changed_outside);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "context_menu {\n    bar {\n        // mine\n        item \"Mine\" { Detach; }\n        item \"Other\" { NewTab; }\n        remove \"New pane\"\n    }\n}\n"
    );
}

#[test]
fn an_overwrite_save_keeps_a_plugin_loaded_outside() {
    let file = "load_plugins {\n    \"zellij:strider\"\n}\n";
    let (_dir, path) = temp_config(Some(file));
    let saved = load(file);
    let mut runtime = saved.clone();
    let mut more = load("load_plugins { \"zellij:bars\"; }").background_plugins;
    runtime.background_plugins.append(&mut more);
    let outside = "load_plugins {\n    \"zellij:strider\"\n    \"zellij:about\"\n}\n";
    std::fs::write(&path, outside).unwrap();
    let (_, changed_outside) = written(save_config_in_place(
        &path,
        Some(file),
        &saved,
        &saved,
        &runtime,
        true,
    ));
    assert!(changed_outside);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "load_plugins {\n    \"zellij:strider\"\n    \"zellij:bars\"\n    \"zellij:about\"\n}\n"
    );
}

#[test]
fn a_backup_this_code_did_not_make_is_moved_aside_instead_of_overwritten() {
    let (_dir, path) = temp_config(None);
    let backup = Config::backup_file_path(&path);
    let first_copy = backup.with_file_name("config.kdl.bak.1");
    let file = format!(
        "//\n// THIS FILE WAS AUTOGENERATED BY ZELLIJ, THE PREVIOUS FILE AT THIS LOCATION WAS COPIED TO: {}\n//\n\nmouse_mode false\n",
        backup.display()
    );
    std::fs::write(&path, &file).unwrap();
    std::fs::write(&backup, "my original config\n").unwrap();
    let saved = load(&file);
    let runtime = reconfigure(&saved, "mouse_mode true");
    let (contents, _) = written(save_config_in_place(
        &path,
        Some(&file),
        &saved,
        &saved,
        &runtime,
        false,
    ));
    assert_eq!(
        std::fs::read_to_string(&first_copy).unwrap(),
        "my original config\n"
    );
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), file);
    let contents = contents.unwrap();
    assert!(
        contents.contains(&format!("COPIED TO: {}\n", first_copy.display())),
        "{}",
        contents
    );
    assert!(contents.contains("mouse_mode true"), "{}", contents);
    let saved = load(&contents);
    let runtime = reconfigure(&saved, "mouse_mode false");
    written(save_config_in_place(
        &path,
        Some(&contents),
        &saved,
        &saved,
        &runtime,
        false,
    ));
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), contents);
    assert_eq!(
        std::fs::read_to_string(&first_copy).unwrap(),
        "my original config\n"
    );
    assert!(!backup.with_file_name("config.kdl.bak.2").exists());
}

#[test]
fn a_comment_inside_a_one_line_empty_block_is_kept_when_the_first_child_is_added() {
    let text = saved_text_after("env { /* my vars */ }\n", |saved| {
        reconfigure(saved, "env { BAR \"2\"; }")
    });
    assert_eq!(text, "env { /* my vars */\n    BAR \"2\"\n}\n");
    let text = saved_text_after("ui { pane_frames { /* keep */ }; }\n", |saved| {
        reconfigure(saved, "ui { pane_frames { rounded_corners true; }; }")
    });
    assert!(text.contains("/* keep */"), "{}", text);
    assert!(text.contains("rounded_corners true"), "{}", text);
}

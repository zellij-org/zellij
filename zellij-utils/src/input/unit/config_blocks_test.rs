use super::super::config::Config;
use super::super::config_blocks::*;
use super::super::config_file_edit::{apply_list_edits, list_edits, ListEdit};
use crate::data::{
    InputMode, KeyWithModifier, KeybindingSource, PaletteColor, ThemeSource, DEFAULT_STYLES,
};
use std::str::FromStr;

fn defaults() -> Config {
    Config::from_default_assets().unwrap()
}

fn load(text: &str) -> Config {
    Config::from_kdl(text, Some(defaults())).unwrap()
}

fn key(text: &str) -> KeyWithModifier {
    KeyWithModifier::from_str(text).unwrap()
}

#[test]
fn colours_are_read_in_every_accepted_format() {
    assert_eq!(
        parse_colour("#fff"),
        Some(PaletteColor::Rgb((255, 255, 255)))
    );
    assert_eq!(
        parse_colour("#102030"),
        Some(PaletteColor::Rgb((16, 32, 48)))
    );
    assert_eq!(
        parse_colour("16 32 48"),
        Some(PaletteColor::Rgb((16, 32, 48)))
    );
    assert_eq!(
        parse_colour("16, 32, 48"),
        Some(PaletteColor::Rgb((16, 32, 48)))
    );
    assert_eq!(parse_colour("208"), Some(PaletteColor::EightBit(208)));
    assert_eq!(parse_colour("300"), None);
    assert_eq!(parse_colour("#12"), None);
    assert_eq!(parse_colour("#gggggg"), None);
    assert_eq!(colour_text(&PaletteColor::Rgb((16, 32, 48))), "#102030");
    assert_eq!(colour_text(&PaletteColor::EightBit(7)), "7");
}

#[test]
fn colours_are_also_read_as_bare_hex_and_rgb_calls() {
    assert_eq!(
        parse_colour("6fde21"),
        Some(PaletteColor::Rgb((111, 222, 33)))
    );
    assert_eq!(
        parse_colour("123456"),
        Some(PaletteColor::Rgb((0x12, 0x34, 0x56)))
    );
    assert_eq!(
        parse_colour("rgb(111, 222, 33)"),
        Some(PaletteColor::Rgb((111, 222, 33)))
    );
    assert_eq!(
        parse_colour("RGB(1 2 3)"),
        Some(PaletteColor::Rgb((1, 2, 3)))
    );
    assert_eq!(parse_colour("rgb(5)"), None);
    assert_eq!(parse_colour("rgb(300, 0, 0)"), None);
    assert_eq!(parse_colour("111 222 333"), None);
    assert_eq!(parse_colour("12345"), None);
}

fn sample_palette() -> crate::data::Styling {
    let mut styling = DEFAULT_STYLES;
    set_slot_colour(
        &mut styling,
        "text_unselected",
        "base",
        PaletteColor::Rgb((1, 2, 3)),
    );
    styling
}

fn read_theme(path: &std::path::Path, name: &str) -> Option<crate::input::theme::Theme> {
    crate::input::theme::Themes::from_path(path.to_path_buf())
        .ok()?
        .get_theme(name)
        .cloned()
}

#[test]
fn a_theme_file_is_created_once_under_the_theme_name() {
    let dir = tempfile::tempdir().unwrap();
    let theme_dir = dir.path().join("themes");
    let palette = sample_palette();
    let path = create_theme_file(&theme_dir, "mine", &palette, None).unwrap();
    assert_eq!(path, theme_dir.join("mine.kdl"));
    let theme = read_theme(&path, "mine").unwrap();
    assert!(theme.sourced_from_external_file);
    assert_eq!(styling_colours(&theme.palette), styling_colours(&palette));
    let again = create_theme_file(&theme_dir, "mine", &palette, None).unwrap_err();
    assert!(again.contains("already exists"));
    assert!(again.contains("mine.kdl"));
    assert!(create_theme_file(&theme_dir, "no/slashes", &palette, None)
        .unwrap_err()
        .contains("cannot be a file name"));
    assert!(create_theme_file(&theme_dir, "", &palette, None).is_err());
}

#[test]
fn a_theme_folder_that_is_a_file_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let not_a_folder = dir.path().join("themes");
    std::fs::write(&not_a_folder, "").unwrap();
    let error = create_theme_file(&not_a_folder, "mine", &sample_palette(), None).unwrap_err();
    assert!(error.contains("not a folder"));
}

const TWO_THEMES: &str = "themes {\n    first {\n        fg 1\n        bg 2\n        red 3\n        green 4\n        blue 5\n        yellow 6\n        magenta 7\n        orange 8\n        cyan 9\n        black 10\n        white 11\n    }\n    second {\n        fg 11\n        bg 10\n        red 9\n        green 8\n        blue 7\n        yellow 6\n        magenta 5\n        orange 4\n        cyan 3\n        black 2\n        white 1\n    }\n}\n";

#[test]
fn updating_a_theme_file_changes_only_that_theme() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("both.kdl");
    std::fs::write(&path, TWO_THEMES).unwrap();
    let second_before = read_theme(&path, "second").unwrap();
    let palette = sample_palette();
    update_theme_file(&path, "first", &palette).unwrap();
    assert_eq!(
        styling_colours(&read_theme(&path, "first").unwrap().palette),
        styling_colours(&palette)
    );
    assert_eq!(read_theme(&path, "second").unwrap(), second_before);
    let missing = update_theme_file(&path, "third", &palette).unwrap_err();
    assert!(missing.contains("does not define the theme third"));
    assert!(update_theme_file(&dir.path().join("gone.kdl"), "first", &palette).is_err());
}

#[test]
fn deleting_the_last_theme_of_a_file_deletes_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("both.kdl");
    std::fs::write(&path, TWO_THEMES).unwrap();
    assert_eq!(delete_theme_from_file(&path, "first"), Ok(false));
    assert!(read_theme(&path, "first").is_none());
    assert!(read_theme(&path, "second").is_some());
    assert!(delete_theme_from_file(&path, "first").is_err());
    assert_eq!(delete_theme_from_file(&path, "second"), Ok(true));
    assert!(!path.exists());
}

#[test]
fn theme_files_are_found_by_theme_name() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("both.kdl"), TWO_THEMES).unwrap();
    std::fs::write(dir.path().join("broken.kdl"), "themes {").unwrap();
    std::fs::write(dir.path().join("notes.txt"), TWO_THEMES).unwrap();
    let files = theme_files(dir.path());
    assert_eq!(files.len(), 2);
    assert_eq!(files.get("first"), Some(&dir.path().join("both.kdl")));
    assert_eq!(files.get("second"), Some(&dir.path().join("both.kdl")));
}

#[test]
fn theme_folder_themes_carry_their_file_and_colours() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("both.kdl"), TWO_THEMES).unwrap();
    let mut config = load("");
    config.options.theme_dir = Some(dir.path().to_path_buf());
    assert_eq!(theme_dir(&config), Some(dir.path().to_path_buf()));
    config.themes = config
        .themes
        .merge(crate::input::theme::Themes::from_dir(dir.path().to_path_buf()).unwrap());
    let blocks = config_blocks(&config, true, None);
    let first = blocks.theme("first").unwrap();
    assert_eq!(first.source, ThemeSource::ThemeFolder);
    assert_eq!(
        first.file_path.as_deref(),
        Some(dir.path().join("both.kdl").display().to_string().as_str())
    );
    assert_eq!(first.colours.len(), theme_slots().len());
    let saved_blocks = config_blocks(&config, false, None);
    assert!(saved_blocks.theme("first").is_none());
}

#[test]
fn styling_survives_the_colour_list() {
    let mut styling = DEFAULT_STYLES;
    styling.frame_unselected = Some(styling.frame_highlight);
    set_slot_colour(
        &mut styling,
        "ribbon_selected",
        "emphasis_2",
        PaletteColor::EightBit(9),
    );
    set_slot_colour(
        &mut styling,
        "multiplayer_user_colors",
        "player_3",
        PaletteColor::Rgb((1, 2, 3)),
    );
    let colours = styling_colours(&styling);
    assert_eq!(colours.len(), theme_slots().len());
    assert_eq!(styling_from_colours(&colours), styling);
    let mut without_frame = DEFAULT_STYLES;
    without_frame.frame_unselected = None;
    assert_eq!(
        styling_from_colours(&styling_colours(&without_frame)),
        without_frame
    );
}

#[test]
fn replacing_blocks_removes_entries_that_merging_cannot() {
    let mut config = load(
        "env { A \"1\"; B \"2\"; }\nplugins { mine location=\"zellij:strider\"; }\nthemes { mine { fg 1; bg 2; red 3; green 4; blue 5; yellow 6; magenta 7; orange 8; cyan 9; black 10; white 11; }; }",
    );
    replace_config_blocks(
        &mut config,
        "env {\n}\nthemes {\n}\nplugins {\n    other location=\"zellij:strider\"\n}",
    )
    .unwrap();
    assert!(config.env.inner().is_empty());
    assert!(config.themes.get_theme("mine").is_none());
    assert!(config.plugins.aliases.contains_key("other"));
    assert!(!config.plugins.aliases.contains_key("mine"));
    assert!(!config.plugins.aliases.contains_key("tab-bar"));
    assert!(replace_config_blocks(&mut config, "options {}").is_err());
}

#[test]
fn block_kdl_from_entries_reads_back_to_the_same_entries() {
    let config = load(
        "env { EDITOR \"vim\"; }\nload_plugins { \"zellij:strider\" { cwd \"/tmp\"; size \"3\"; }; }\ncontext_menu { pane clear-defaults=true { item \"Tab\" { NewTab; }; separator; item \"Go\" { SwitchToMode \"locked\"; }; }; }\nthemes { mine { fg 1; bg 2; red 3; green 4; blue 5; yellow 6; magenta 7; orange 8; cyan 9; black 10; white 11; }; }",
    );
    let blocks = config_blocks(&config, false, None);
    let mut rebuilt = Config::default();
    let pane = blocks.menu_section("pane");
    let text = format!(
        "{}\n{}\n{}\n{}\n{}",
        env_kdl(&blocks.env),
        load_plugins_kdl(&blocks.load_plugins),
        plugin_aliases_kdl(&blocks.plugin_aliases),
        menu_section_kdl("pane", &pane).unwrap(),
        themes_kdl(&blocks.themes)
    );
    replace_config_blocks(&mut rebuilt, &text).unwrap();
    let rebuilt_blocks = config_blocks(&rebuilt, false, None);
    assert_eq!(rebuilt_blocks.env, blocks.env);
    assert_eq!(rebuilt_blocks.load_plugins, blocks.load_plugins);
    assert_eq!(rebuilt_blocks.plugin_aliases, blocks.plugin_aliases);
    assert_eq!(rebuilt_blocks.menu_section("pane"), pane);
    assert_eq!(rebuilt_blocks.themes, blocks.themes);
    assert_eq!(blocks.themes[0].source, ThemeSource::ConfigFile);
    assert_eq!(pane.len(), 3);
    assert!(pane[1].is_separator());
}

#[test]
fn a_keybinding_change_is_written_as_a_keybinds_block() {
    let text = keybind_change_kdl(
        InputMode::Normal,
        &key("Alt y"),
        Some(&["NewTab".to_owned(), "SwitchToMode \"locked\"".to_owned()]),
    )
    .unwrap();
    let config = load(&text);
    let actions =
        &config.keybinds_layers.user.changes.modes[&InputMode::Normal].bind[&key("Alt y")];
    assert_eq!(
        action_texts(actions),
        vec!["NewTab", "SwitchToMode \"locked\""]
    );
    let unbind = keybind_change_kdl(InputMode::Pane, &key("x"), None).unwrap();
    let config = load(&unbind);
    assert!(config.keybinds_layers.user.changes.modes[&InputMode::Pane]
        .unbind
        .contains(&key("x")));
}

#[test]
fn keybindings_name_where_they_come_from() {
    let file = "keybinds {\n    shared_except \"locked\" {\n        bind \"Alt y\" { NewTab; }\n    }\n    normal {\n        bind \"Alt u\" { NewTab; }\n        unbind \"Alt n\"\n    }\n}\n";
    let saved = load(file);
    let mut current = saved.clone();
    current
        .keybinds_layers
        .user
        .changes
        .bind(InputMode::Pane, key("Alt k"), vec![]);
    current.resolve_keybinds();
    let layout = Config::from_layout_kdl(
        "keybinds { tab { bind \"Alt l\" { NewTab; }; }; }",
        Some(current.clone()),
    )
    .unwrap();
    let entries = keybinding_entries(&saved, &layout, Some(file));
    let find = |mode: InputMode, text: &str| {
        entries
            .iter()
            .find(|entry| entry.mode == mode && entry.key == key(text))
            .cloned()
            .unwrap_or_else(|| panic!("{:?} {} missing", mode, text))
    };
    assert_eq!(
        find(InputMode::Normal, "Alt y").source,
        KeybindingSource::Shared("shared_except \"locked\"".to_owned())
    );
    assert_eq!(
        find(InputMode::Normal, "Alt u").source,
        KeybindingSource::User
    );
    let unbound = find(InputMode::Normal, "Alt n");
    assert!(unbound.unbound);
    assert!(unbound.preset_actions.is_some());
    assert!(!unbound.unsaved);
    assert_eq!(
        find(InputMode::Normal, "Alt h").source,
        KeybindingSource::Preset
    );
    assert_eq!(
        find(InputMode::Tab, "Alt l").source,
        KeybindingSource::Layout
    );
    let added = find(InputMode::Pane, "Alt k");
    assert!(added.unsaved);
    assert_eq!(added.source, KeybindingSource::User);
    assert!(entries
        .iter()
        .all(|entry| !(entry.mode == InputMode::Locked && entry.key == key("Alt y"))));
}

#[test]
fn list_edits_turn_one_list_into_another() {
    let cases: Vec<(Vec<u8>, Vec<u8>)> = vec![
        (vec![1, 2, 3], vec![1, 2, 3, 4]),
        (vec![1, 2, 3], vec![1, 3]),
        (vec![1, 2, 3], vec![3, 1, 2]),
        (vec![1, 2, 3], vec![1, 5, 3]),
        (vec![1, 2, 3], vec![4, 5]),
        (vec![], vec![1]),
        (vec![1, 2], vec![]),
        (vec![1, 2, 3, 4], vec![2, 1, 4, 3, 5]),
    ];
    for (old, new) in cases {
        let edits = list_edits(&old, &new);
        let mut edited = old.clone();
        apply_list_edits(&mut edited, &edits);
        assert_eq!(edited, new, "{:?} -> {:?} with {:?}", old, new, edits);
    }
    assert_eq!(
        list_edits(&[1, 2, 3], &[2, 1, 3]),
        vec![ListEdit::Move { from: 0, to: 1 }]
    );
    assert_eq!(
        list_edits(&[1, 2, 3], &[1, 9, 3]),
        vec![ListEdit::Replace { index: 1, item: 9 }]
    );
}

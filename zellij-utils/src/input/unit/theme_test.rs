use super::super::theme::*;
use crate::consts::{DEFAULT_LIGHT_THEME_NAME, DEFAULT_THEME_NAME, ZELLIJ_DEFAULT_THEMES};
use crate::data::{PaletteColor, DEFAULT_STYLES};
use insta::assert_snapshot;
use std::path::{Path, PathBuf};

const THEMES_WITHOUT_TERMINAL_COLORS: [&str; 7] = [
    "ansi",
    "ao",
    "blade-runner",
    "classic",
    "cyber-noir",
    "menace",
    "retro-wave",
];

fn bundled_themes() -> Themes {
    let mut all = Themes::default();
    for file in ZELLIJ_DEFAULT_THEMES.files() {
        let contents = file
            .contents_utf8()
            .unwrap_or_else(|| panic!("{} is not utf8", file.path().display()));
        let themes = Themes::from_string(&contents.to_string(), true)
            .unwrap_or_else(|e| panic!("{} does not parse: {}", file.path().display(), e));
        all = all.merge(themes);
    }
    all
}

fn theme_test_dir(theme: String) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme_dir = root.join("src/input/unit/fixtures/themes");
    theme_dir.join(theme)
}

#[test]
fn dracula_theme_from_file() {
    let path = theme_test_dir("dracula.kdl".into());
    let theme = Themes::from_path(path).unwrap();
    assert_snapshot!(format!("{:#?}", theme));
}

#[test]
fn every_bundled_theme_parses() {
    assert_eq!(
        bundled_themes().inner().len(),
        ZELLIJ_DEFAULT_THEMES.files().count(),
        "a bundled file defines a number of themes other than one"
    );
}

#[test]
fn every_bundled_terminal_colors_block_holds_sixteen_entries_and_no_palette_index() {
    for (name, theme) in bundled_themes().inner() {
        let Some(table) = theme.terminal_colors else {
            continue;
        };
        assert_eq!(
            table.declared(),
            16,
            "{} declares {} of the sixteen terminal colors",
            name,
            table.declared()
        );
        for slot in 0..16 {
            assert!(
                matches!(table.get(slot), Some(PaletteColor::Rgb(_))),
                "{} declares {:?} in terminal color {}, which must be an rgb or hex value",
                name,
                table.get(slot),
                TERMINAL_COLOR_NAMES[slot]
            );
        }
    }
}

#[test]
fn the_bundled_themes_carrying_no_terminal_colors_are_the_ones_with_no_canonical_table() {
    let mut without: Vec<String> = bundled_themes()
        .inner()
        .iter()
        .filter(|(_, theme)| theme.terminal_colors.is_none())
        .map(|(name, _)| name.clone())
        .collect();
    without.sort();
    assert_eq!(
        without, THEMES_WITHOUT_TERMINAL_COLORS,
        "a bundled theme gained or lost a terminal_colors block without a stated source"
    );
}

#[test]
fn the_bundled_default_theme_matches_the_compiled_in_one() {
    let bundled = bundled_themes()
        .get_theme(DEFAULT_THEME_NAME)
        .copied()
        .expect("the default theme is bundled");
    assert_eq!(
        bundled.palette, DEFAULT_STYLES,
        "assets/themes/default.kdl and DEFAULT_STYLES have drifted apart"
    );
}

fn relative_luminance(color: PaletteColor) -> f64 {
    let (r, g, b) = match color {
        PaletteColor::Rgb(rgb) => rgb,
        PaletteColor::EightBit(index) => panic!("color {} is not an rgb value", index),
    };
    let channel = |c: u8| {
        let c = c as f64 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn contrast(a: PaletteColor, b: PaletteColor) -> f64 {
    let (a, b) = (relative_luminance(a), relative_luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn assert_contrast(theme: &str, what: &str, fg: PaletteColor, bg: PaletteColor, least: f64) {
    let ratio = contrast(fg, bg);
    assert!(
        ratio >= least,
        "{}: {} has a contrast ratio of {:.2}, below the required {:.1}",
        theme,
        what,
        ratio,
        least
    );
}

#[test]
fn the_bundled_default_themes_are_readable() {
    let themes = bundled_themes();
    for name in [DEFAULT_THEME_NAME, DEFAULT_LIGHT_THEME_NAME] {
        let theme = themes.get_theme(name).expect("theme is bundled");
        let styling = theme.palette;
        let canvas = styling.text_unselected.background;

        let text_fields = [
            ("text_unselected", styling.text_unselected),
            ("text_selected", styling.text_selected),
            ("table_title", styling.table_title),
            ("table_cell_unselected", styling.table_cell_unselected),
            ("table_cell_selected", styling.table_cell_selected),
            ("list_unselected", styling.list_unselected),
            ("list_selected", styling.list_selected),
        ];
        for (field, declaration) in text_fields {
            let on_own_background = [
                ("base", declaration.base),
                ("emphasis_0", declaration.emphasis_0),
                ("emphasis_1", declaration.emphasis_1),
                ("emphasis_2", declaration.emphasis_2),
                ("emphasis_3", declaration.emphasis_3),
            ];
            for (part, color) in on_own_background {
                assert_contrast(
                    name,
                    &format!("{}.{} on its background", field, part),
                    color,
                    declaration.background,
                    4.5,
                );
            }
        }

        let ribbon = styling.ribbon_unselected;
        for (part, color) in [("base", ribbon.base), ("emphasis_0", ribbon.emphasis_0)] {
            assert_contrast(
                name,
                &format!("ribbon_unselected.{} on its background", part),
                color,
                ribbon.background,
                4.5,
            );
            assert_contrast(
                name,
                &format!("ribbon_unselected.{} on the hover fill", part),
                color,
                ribbon.emphasis_1,
                4.5,
            );
        }

        let ribbon = styling.ribbon_selected;
        for (part, color) in [
            ("base", ribbon.base),
            ("emphasis_0", ribbon.emphasis_0),
            ("emphasis_2", ribbon.emphasis_2),
            ("emphasis_3", ribbon.emphasis_3),
        ] {
            assert_contrast(
                name,
                &format!("ribbon_selected.{} on its background", part),
                color,
                ribbon.background,
                4.5,
            );
        }

        let frame_unselected = styling
            .frame_unselected
            .expect("the default themes color unfocused frames");
        let lines = [
            ("frame_unselected.base", frame_unselected.base),
            ("frame_selected.base", styling.frame_selected.base),
            ("frame_highlight.base", styling.frame_highlight.base),
            (
                "frame_highlight.emphasis_0",
                styling.frame_highlight.emphasis_0,
            ),
            (
                "frame_highlight.emphasis_1",
                styling.frame_highlight.emphasis_1,
            ),
            ("exit_code_success.base", styling.exit_code_success.base),
            ("exit_code_error.base", styling.exit_code_error.base),
        ];
        for (what, color) in lines {
            assert_contrast(name, &format!("{} on the canvas", what), color, canvas, 3.0);
        }

        let players = styling.multiplayer_user_colors;
        let players = [
            ("player_1", players.player_1),
            ("player_2", players.player_2),
            ("player_3", players.player_3),
            ("player_4", players.player_4),
            ("player_5", players.player_5),
            ("player_6", players.player_6),
            ("player_7", players.player_7),
            ("player_8", players.player_8),
            ("player_9", players.player_9),
            ("player_10", players.player_10),
        ];
        for (what, color) in players {
            assert_contrast(
                name,
                &format!("{} as a frame color", what),
                color,
                canvas,
                3.0,
            );
            assert_contrast(
                name,
                &format!("the cursor marker on {}", what),
                PaletteColor::Rgb((0, 0, 0)),
                color,
                4.5,
            );
        }

        let terminal_colors = theme
            .terminal_colors
            .expect("the default themes declare terminal colors");
        for slot in [1, 2, 3, 4, 5, 6, 9, 10, 11, 12, 13, 14] {
            let color = terminal_colors
                .get(slot)
                .expect("the default themes declare every terminal color");
            assert_contrast(
                name,
                &format!("the {} terminal color", TERMINAL_COLOR_NAMES[slot]),
                color,
                canvas,
                4.5,
            );
        }
    }
}

#[test]
fn no_theme_is_err() {
    let path = theme_test_dir("nonexistent.kdl".into());
    let theme = Themes::from_path(path);
    assert!(theme.is_err());
}

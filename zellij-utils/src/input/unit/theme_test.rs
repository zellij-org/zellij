use super::super::theme::*;
use insta::assert_snapshot;
use std::path::{Path, PathBuf};

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
fn no_theme_is_err() {
    let path = theme_test_dir("nonexistent.kdl".into());
    let theme = Themes::from_path(path);
    assert!(theme.is_err());
}

fn theme_with_fg(fg: &str) -> Result<Themes, crate::input::config::ConfigError> {
    let text = format!(
        "themes {{ t {{ fg {}; bg 2; red 3; green 4; blue 5; yellow 6; magenta 7; orange 8; cyan 9; black 10; white 11; }}; }}",
        fg
    );
    Themes::from_string(&text, false)
}

#[test]
fn quoted_rgb_and_bare_hex_colors_read_like_the_number_form() {
    let numbers = theme_with_fg("111 222 33").unwrap();
    assert_eq!(theme_with_fg("\"111 222 33\"").unwrap(), numbers);
    assert_eq!(theme_with_fg("\"6fde21\"").unwrap(), numbers);
    assert_eq!(theme_with_fg("\"#6fde21\"").unwrap(), numbers);
}

#[test]
fn color_values_outside_0_to_255_are_rejected() {
    assert!(theme_with_fg("111 222 333").is_err());
    assert!(theme_with_fg("-1 0 0").is_err());
    assert!(theme_with_fg("256").is_err());
    assert!(theme_with_fg("255").is_ok());
    assert!(theme_with_fg("\"not a color\"").is_err());
}

use super::super::theme::*;
use std::path::PathBuf;

fn theme_test_dir(theme: String) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme_dir = root.join("src/input/unit/fixtures/themes");
    theme_dir.join(theme)
}

#[test]
fn dracula_theme_is_ok() {
    let path = theme_test_dir("dracula.yaml".into());
    let theme = ThemesFromYaml::from_path(&path);
    assert!(theme.is_ok());
}

#[test]
fn no_theme_is_err() {
    let path = theme_test_dir("nonexistent.yaml".into());
    let theme = ThemesFromYaml::from_path(&path);
    assert!(theme.is_err());
}

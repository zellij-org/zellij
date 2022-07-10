use super::super::theme::*;

fn theme_test_dir(theme: String) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme_dir = root.join("src/input/unit/fixtures/themes");
    theme_dir.join(theme)
}

#[test]
fn default_theme_is_ok() {
    let path = theme_test_dir("default.yaml".into());
    let theme = Theme::from_path(&path);
    assert!(theme.is_ok());
}

#[test]
fn no_theme_is_err() {
    let path = theme_test_dir("nonexistent.yaml".into());
    let theme = Theme::from_path(&path);
    assert!(theme.is_err());
}

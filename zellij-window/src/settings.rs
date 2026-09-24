use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use zellij_utils::data::{HostTerminalThemeMode, ThemeHue};
use zellij_utils::home;
use zellij_utils::input::config::Config;
use zellij_utils::input::theme::{Theme, Themes};
use zellij_utils::input::window::WindowConfig;
use zellij_utils::setup::get_default_themes;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    pub section: WindowConfig,
    pub theme: Option<Theme>,
    pub theme_dark: Option<Theme>,
    pub theme_light: Option<Theme>,
    pub explicit_hue: Option<ThemeHue>,
}

impl Settings {
    pub fn theme(&self, mode: Option<HostTerminalThemeMode>) -> Option<Theme> {
        match (self.theme_dark, self.theme_light) {
            (Some(dark), Some(light)) => match mode.unwrap_or_else(|| self.default_mode()) {
                HostTerminalThemeMode::Dark => Some(dark),
                HostTerminalThemeMode::Light => Some(light),
            },
            _ => self.theme,
        }
    }

    fn default_mode(&self) -> HostTerminalThemeMode {
        match self.explicit_hue {
            Some(ThemeHue::Light) => HostTerminalThemeMode::Light,
            _ => HostTerminalThemeMode::Dark,
        }
    }
}

pub fn load(config_file_path: Option<&Path>) -> Result<Settings> {
    let Some(path) = config_file_path else {
        return Ok(Settings::default());
    };
    let defaults = Config::from_default_assets()
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("the built-in default configuration is unreadable")?;
    let mut config = if path.exists() {
        Config::from_path(&path.to_path_buf(), Some(defaults))
            .map_err(|e| anyhow::anyhow!("{}", e))
            .with_context(|| format!("failed to read the configuration at {:?}", path))?
    } else {
        defaults
    };

    config.themes = config.themes.merge(get_default_themes());
    if let Some(theme_dir) = theme_dir(&config, path) {
        let themes = Themes::from_dir(theme_dir.clone())
            .map_err(|e| anyhow::anyhow!("{}", e))
            .with_context(|| format!("failed to read the themes in {:?}", theme_dir))?;
        config.themes = config.themes.merge(themes);
    }

    Ok(Settings {
        theme: config.theme(config.options.theme.as_ref()),
        theme_dark: config.theme_dark(),
        theme_light: config.theme_light(),
        explicit_hue: config.options.explicit_theme_hue,
        section: config.window,
    })
}

fn theme_dir(config: &Config, config_file_path: &Path) -> Option<PathBuf> {
    config
        .options
        .theme_dir
        .clone()
        .or_else(|| home::get_theme_dir(config_file_path.parent().map(Path::to_path_buf)))
        .filter(|dir| dir.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::data::PaletteColor;

    fn written(config: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.kdl");
        std::fs::write(&path, config).unwrap();
        (dir, path)
    }

    fn theme(name: &str, base: u8) -> String {
        format!(
            r#"
            {name} {{
                text_unselected {{
                    base {base} {base} {base}
                    background {base} {base} {base}
                    emphasis_0 0 0 0
                    emphasis_1 0 0 0
                    emphasis_2 0 0 0
                    emphasis_3 0 0 0
                }}
            }}
            "#
        )
    }

    fn dark_and_light(options: &str) -> Settings {
        let (_dir, path) = written(&format!(
            "{}\nthemes {{\n{}\n{}\n}}\n",
            options,
            theme("window-dark", 1),
            theme("window-light", 2)
        ));
        load(Some(&path)).unwrap()
    }

    #[test]
    fn no_config_path_at_all_reads_nothing_from_disk() {
        assert_eq!(load(None).unwrap(), Settings::default());
    }

    #[test]
    fn a_config_file_that_does_not_exist_leaves_every_option_unset() {
        let dir = tempfile::TempDir::new().unwrap();
        let settings = load(Some(&dir.path().join("config.kdl"))).unwrap();
        assert_eq!(settings.section, WindowConfig::default());
    }

    #[test]
    fn a_config_without_the_section_leaves_every_option_unset() {
        let (_dir, path) = written("default_mode \"locked\"\n");
        assert_eq!(load(Some(&path)).unwrap().section, WindowConfig::default());
    }

    #[test]
    fn the_section_is_read_out_of_the_standard_config_file() {
        let (_dir, path) = written("window {\n font \"Iosevka Term\"\n font_size 18\n}\n");
        let settings = load(Some(&path)).unwrap();
        assert_eq!(settings.section.font.as_deref(), Some("Iosevka Term"));
        assert_eq!(settings.section.font_size, Some(18.0));
    }

    #[test]
    fn an_unknown_section_is_ignored() {
        let (_dir, path) = written(
            "unknown {\n font \"Iosevka Term\"\n font_size 18\n}\nwindow {\n font_size 19\n}\n",
        );
        let settings = load(Some(&path)).expect("an unknown section is ignored, not an error");
        assert_eq!(settings.section.font, None);
        assert_eq!(settings.section.font_size, Some(19.0));
    }

    #[test]
    fn an_unreadable_config_is_an_error_rather_than_a_silent_default() {
        let (_dir, path) = written("window {\n font_size \"enormous\"\n}\n");
        assert!(load(Some(&path)).is_err());
    }

    #[test]
    fn the_configured_theme_travels_beside_the_section() {
        let (_dir, path) = written(&format!(
            "theme \"window-test\"\nthemes {{\n{}\n}}\n",
            theme("window-test", 3)
        ));
        let settings = load(Some(&path)).unwrap();
        let theme = settings.theme(None).expect("no theme resolved");
        assert_eq!(
            theme.palette.text_unselected.base,
            PaletteColor::Rgb((3, 3, 3))
        );
    }

    #[test]
    fn a_themes_terminal_colors_are_read_beside_its_slots() {
        let (_dir, path) = written(
            r##"
            theme "window-test"
            themes {
                window-test {
                    text_unselected {
                        base 3 3 3
                        background 3 3 3
                        emphasis_0 0 0 0
                        emphasis_1 0 0 0
                        emphasis_2 0 0 0
                        emphasis_3 0 0 0
                    }
                    terminal_colors {
                        red 9 9 9
                        bright_white "#ffffff"
                    }
                }
            }
            "##,
        );
        let table = load(Some(&path))
            .unwrap()
            .theme(None)
            .expect("no theme resolved")
            .terminal_colors
            .expect("no terminal colors resolved");
        assert_eq!(table.get(1), Some(PaletteColor::Rgb((9, 9, 9))));
        assert_eq!(table.get(15), Some(PaletteColor::Rgb((255, 255, 255))));
    }

    #[test]
    fn a_theme_that_is_not_configured_falls_back_to_the_bundled_default() {
        let (_dir, path) = written("window {\n font_size 18\n}\n");
        let settings = load(Some(&path)).unwrap();
        assert_eq!(
            settings.theme(Some(HostTerminalThemeMode::Dark)),
            settings.theme,
            "the dark default is the theme a session without options resolves"
        );
        assert_eq!(
            settings
                .theme(Some(HostTerminalThemeMode::Light))
                .expect("no light theme resolved")
                .palette
                .text_unselected
                .background,
            PaletteColor::Rgb((246, 244, 251)),
            "a light host palette resolves the bundled light default"
        );
    }

    #[test]
    fn a_dark_and_light_pair_is_chosen_by_the_reported_mode() {
        let settings = dark_and_light("theme_dark \"window-dark\"\ntheme_light \"window-light\"");
        let base = |mode| settings.theme(mode).unwrap().palette.text_unselected.base;
        assert_eq!(
            base(Some(HostTerminalThemeMode::Dark)),
            PaletteColor::Rgb((1, 1, 1))
        );
        assert_eq!(
            base(Some(HostTerminalThemeMode::Light)),
            PaletteColor::Rgb((2, 2, 2))
        );
    }

    #[test]
    fn a_pair_with_no_mode_reported_yet_starts_dark_as_the_session_does() {
        let settings = dark_and_light("theme_dark \"window-dark\"\ntheme_light \"window-light\"");
        assert_eq!(
            settings.theme(None).unwrap().palette.text_unselected.base,
            PaletteColor::Rgb((1, 1, 1))
        );
    }

    #[test]
    fn a_pinned_hue_decides_the_mode_before_anything_is_reported() {
        let settings = dark_and_light(
            "theme_dark \"window-dark\"\ntheme_light \"window-light\"\nexplicit_theme_hue \"light\"",
        );
        assert_eq!(
            settings.theme(None).unwrap().palette.text_unselected.base,
            PaletteColor::Rgb((2, 2, 2))
        );
    }

    #[test]
    fn half_a_pair_leaves_the_static_theme_authoritative_as_the_session_does() {
        let settings = dark_and_light("theme \"window-light\"\ntheme_dark \"window-dark\"");
        for mode in [
            None,
            Some(HostTerminalThemeMode::Dark),
            Some(HostTerminalThemeMode::Light),
        ] {
            assert_eq!(
                settings.theme(mode).unwrap().palette.text_unselected.base,
                PaletteColor::Rgb((2, 2, 2)),
                "{:?} moved a theme the session would not have moved",
                mode
            );
        }
    }
}

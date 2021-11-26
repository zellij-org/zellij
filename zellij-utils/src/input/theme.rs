use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::options::Options;
use zellij_tile::data::{Palette, PaletteColor};

/// Intermediate deserialization of themes
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ThemesFromYaml(HashMap<String, Theme>);

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
struct Theme {
    #[serde(flatten)]
    palette: PaletteFromYaml,
}

/// Intermediate deserialization struct
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct PaletteFromYaml {
    pub fg: PaletteColorFromYaml,
    pub bg: PaletteColorFromYaml,
    pub black: PaletteColorFromYaml,
    pub red: PaletteColorFromYaml,
    pub green: PaletteColorFromYaml,
    pub yellow: PaletteColorFromYaml,
    pub blue: PaletteColorFromYaml,
    pub magenta: PaletteColorFromYaml,
    pub cyan: PaletteColorFromYaml,
    pub white: PaletteColorFromYaml,
    pub orange: PaletteColorFromYaml,
}

/// Intermediate deserialization enum
// This is here in order to make the untagged enum work
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum PaletteColorFromYaml {
    Rgb((u8, u8, u8)),
    EightBit(u8),
}

impl Default for PaletteColorFromYaml {
    fn default() -> Self {
        PaletteColorFromYaml::EightBit(0)
    }
}

impl ThemesFromYaml {
    pub fn theme_config(self, opts: &Options) -> Option<Palette> {
        let mut from_yaml = self;
        match &opts.theme {
            Some(theme) => from_yaml.from_default_theme(theme.to_owned()),
            None => from_yaml.from_default_theme("default".into()),
        }
    }

    fn get_theme(&mut self, theme: String) -> Option<Theme> {
        self.0.remove(&theme)
    }

    fn from_default_theme(&mut self, theme: String) -> Option<Palette> {
        self.clone()
            .get_theme(theme)
            .map(|t| Palette::from(t.palette))
    }

    /// Merges two Theme structs into one Theme struct
    /// `other` overrides the Theme of `self`.
    pub fn merge(&self, other: Self) -> Self {
        let mut theme = self.0.clone();
        theme.extend(other.0);
        Self(theme)
    }
}

impl From<PaletteFromYaml> for Palette {
    fn from(yaml: PaletteFromYaml) -> Self {
        Palette {
            fg: yaml.fg.into(),
            bg: yaml.fg.into(),
            black: yaml.black.into(),
            red: yaml.red.into(),
            green: yaml.green.into(),
            yellow: yaml.yellow.into(),
            blue: yaml.blue.into(),
            magenta: yaml.magenta.into(),
            cyan: yaml.cyan.into(),
            white: yaml.white.into(),
            orange: yaml.orange.into(),
            ..Palette::default()
        }
    }
}

impl From<PaletteColorFromYaml> for PaletteColor {
    fn from(yaml: PaletteColorFromYaml) -> Self {
        match yaml {
            PaletteColorFromYaml::Rgb(color) => PaletteColor::Rgb(color),
            PaletteColorFromYaml::EightBit(color) => PaletteColor::EightBit(color),
        }
    }
}

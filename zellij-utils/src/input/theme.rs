use anyhow::Context;
use colorsys::Rgb;
use std::str::FromStr;
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

#[derive(Debug, Clone, PartialEq)]
#[derive(knuffel::Decode)]
pub struct ThemeFromKdl {
    #[knuffel(argument)]
    name: String,
    #[knuffel(child)]
    palette: PaletteFromYaml,
}

/// Intermediate deserialization struct
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[derive(knuffel::Decode)]
pub struct PaletteFromYaml {
    #[knuffel(child, unwrap(argument, str))]
    pub fg: PaletteColorFromYaml,
    #[knuffel(child, unwrap(argument, str))]
    pub bg: PaletteColorFromYaml,
    #[knuffel(child, unwrap(argument, str))]
    pub black: PaletteColorFromYaml,
    #[knuffel(child, unwrap(argument, str))]
    pub gray: PaletteColorFromYaml,
    #[knuffel(child, unwrap(argument, str))]
    pub red: PaletteColorFromYaml,
    #[knuffel(child, unwrap(argument, str))]
    pub green: PaletteColorFromYaml,
    #[knuffel(child, unwrap(argument, str))]
    pub yellow: PaletteColorFromYaml,
    #[knuffel(child, unwrap(argument, str))]
    pub blue: PaletteColorFromYaml,
    #[knuffel(child, unwrap(argument, str))]
    pub magenta: PaletteColorFromYaml,
    #[knuffel(child, unwrap(argument, str))]
    pub cyan: PaletteColorFromYaml,
    #[knuffel(child, unwrap(argument, str))]
    pub white: PaletteColorFromYaml,
    #[knuffel(child, unwrap(argument, str))]
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

impl FromStr for PaletteColorFromYaml {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with('#') {
            Ok(PaletteColorFromYaml::Rgb(Rgb::from_hex_str(s)?.into()))
        } else {
            Ok(PaletteColorFromYaml::EightBit(
                s.parse()
                .context("expected either RGB prefixed by `#` or 8bit integer")?
            ))
        }
    }
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

impl From<Vec<ThemeFromKdl>> for ThemesFromYaml {
    fn from(src: Vec<ThemeFromKdl>) -> ThemesFromYaml {
        ThemesFromYaml(
            src.into_iter()
            .map(|theme| (theme.name, Theme { palette: theme.palette }))
            .collect()
        )
    }
}

impl From<PaletteFromYaml> for Palette {
    fn from(yaml: PaletteFromYaml) -> Self {
        Palette {
            fg: yaml.fg.into(),
            bg: yaml.bg.into(),
            black: yaml.black.into(),
            gray: yaml.gray.into(),
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

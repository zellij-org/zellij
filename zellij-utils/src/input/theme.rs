use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{collections::HashMap, fmt};

use super::options::Options;
use crate::shared::detect_theme_hue;
use zellij_tile::data::{Palette, PaletteColor};

/// Intermediate deserialization of themes
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ThemesFromYaml(HashMap<String, Theme>);

#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct UiConfigFromYaml {
    pub pane_frames: FrameConfigFromYaml,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct FrameConfigFromYaml {
    pub rounded_corners: bool,
}

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
    Hex(HexColor),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HexColor(u8, u8, u8);

impl From<HexColor> for (u8, u8, u8) {
    fn from(e: HexColor) -> (u8, u8, u8) {
        let HexColor(r, g, b) = e;
        (r, g, b)
    }
}

pub struct HexColorVisitor();

impl<'de> Visitor<'de> for HexColorVisitor {
    type Value = HexColor;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a hex color in the format #RGB or #RRGGBB")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        if let Some(stripped) = s.strip_prefix('#') {
            return self.visit_str(stripped);
        }

        if s.len() == 3 {
            Ok(HexColor(
                u8::from_str_radix(&s[0..1], 16).map_err(E::custom)? * 0x11,
                u8::from_str_radix(&s[1..2], 16).map_err(E::custom)? * 0x11,
                u8::from_str_radix(&s[2..3], 16).map_err(E::custom)? * 0x11,
            ))
        } else if s.len() == 6 {
            Ok(HexColor(
                u8::from_str_radix(&s[0..2], 16).map_err(E::custom)?,
                u8::from_str_radix(&s[2..4], 16).map_err(E::custom)?,
                u8::from_str_radix(&s[4..6], 16).map_err(E::custom)?,
            ))
        } else {
            Err(Error::custom(
                "Hex color must be of form \"#RGB\" or \"#RRGGBB\"",
            ))
        }
    }
}

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(HexColorVisitor())
    }
}
impl Serialize for HexColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(format!("{:02X}{:02X}{:02X}", self.0, self.1, self.2).as_str())
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

impl From<PaletteFromYaml> for Palette {
    fn from(yaml: PaletteFromYaml) -> Self {
        Palette {
            fg: yaml.fg.into(),
            bg: yaml.bg.into(),
            black: yaml.black.into(),
            red: yaml.red.into(),
            green: yaml.green.into(),
            yellow: yaml.yellow.into(),
            blue: yaml.blue.into(),
            magenta: yaml.magenta.into(),
            cyan: yaml.cyan.into(),
            white: yaml.white.into(),
            orange: yaml.orange.into(),
            theme_hue: detect_theme_hue(yaml.bg.into()),
            ..Palette::default()
        }
    }
}

impl From<PaletteColorFromYaml> for PaletteColor {
    fn from(yaml: PaletteColorFromYaml) -> Self {
        match yaml {
            PaletteColorFromYaml::Rgb(color) => PaletteColor::Rgb(color),
            PaletteColorFromYaml::EightBit(color) => PaletteColor::EightBit(color),
            PaletteColorFromYaml::Hex(color) => PaletteColor::Rgb(color.into()),
        }
    }
}

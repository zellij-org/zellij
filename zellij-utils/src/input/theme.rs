use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{collections::{HashMap, BTreeMap}, fmt};

use kdl::KdlNode;
use crate::{entry_count, kdl_entries_as_i64, kdl_first_entry_as_string, kdl_first_entry_as_i64, kdl_children_or_error, kdl_name, kdl_children_nodes_or_error, kdl_get_child, kdl_children_property_first_arg_as_bool};

use super::options::Options;
use super::config::ConfigError;
use crate::data::{Palette, PaletteColor};
use crate::shared::detect_theme_hue;

/// Intermediate deserialization of themes
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ThemesFromYaml(HashMap<String, Theme>);

#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct UiConfig {
    pub pane_frames: FrameConfig,
}

impl UiConfig {
    pub fn merge(&self, other: UiConfig) -> Self {
        let mut merged = self.clone();
        merged.pane_frames = merged.pane_frames.merge(other.pane_frames);
        merged
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct FrameConfig {
    pub rounded_corners: bool,
}

impl FrameConfig {
    pub fn merge(&self, other: FrameConfig) -> Self {
        let mut merged = self.clone();
        merged.rounded_corners = other.rounded_corners;
        merged
    }
}

#[derive(Clone, PartialEq, Default)]
pub struct Themes(HashMap<String, Theme>);

impl fmt::Debug for Themes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut stable_sorted = BTreeMap::new();
        for (theme_name, theme) in self.0.iter() {
            stable_sorted.insert(theme_name, theme);
        }
        write!(f, "{:#?}", stable_sorted)
    }
}

impl Themes {
    pub fn from_data(theme_data: HashMap<String, Theme>) -> Self {
        Themes(theme_data)
    }
    pub fn merge(&self, mut other: Themes) -> Self {
        let mut merged = self.clone();
        for (name, theme) in other.0.drain() {
            merged.0.insert(name, theme);
        }
        merged
    }
    pub fn get_theme(&self, theme_name: &str) -> Option<&Theme> {
        self.0.get(theme_name)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Theme {
    #[serde(flatten)]
    pub palette: Palette,
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

    #[allow(clippy::wrong_self_convention)]
    fn from_default_theme(&mut self, theme: String) -> Option<Palette> {
        self.clone()
            .get_theme(theme)
            .map(|t| t.palette)
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

use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
};

use crate::data::Styling;

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
    pub hide_session_name: bool,
}

impl FrameConfig {
    pub fn merge(&self, other: FrameConfig) -> Self {
        let mut merged = self.clone();
        merged.rounded_corners = other.rounded_corners;
        merged.hide_session_name = other.hide_session_name;
        merged
    }
}

#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
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
    pub fn insert(&mut self, theme_name: String, theme: Theme) {
        self.0.insert(theme_name, theme);
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
    pub fn inner(&self) -> &HashMap<String, Theme> {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Theme {
    pub sourced_from_external_file: bool,
    #[serde(flatten)]
    pub palette: Styling,
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

#[cfg(test)]
#[path = "./unit/theme_test.rs"]
mod theme_test;

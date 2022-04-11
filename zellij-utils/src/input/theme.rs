use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::options::Options;
use crate::shared::detect_theme_hue;
use zellij_tile::data::{Palette, PaletteColor};

/// Intermediate deserialization of themes
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Themes(HashMap<String, Theme>);

#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct UiConfig {
    pub pane_frames: FrameConfig,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct FrameConfig {
    pub rounded_corners: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
struct Theme {
    #[serde(flatten)]
    palette: Palette,
}

impl Themes {
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

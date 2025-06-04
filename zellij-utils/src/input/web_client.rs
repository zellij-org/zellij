use kdl::{KdlDocument, KdlNode, KdlValue};
use serde::{Deserialize, Serialize};

use crate::{
    data::PaletteColor, kdl_children_or_error, kdl_get_child, kdl_get_child_entry_string_value,
};

use super::config::ConfigError;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebClientTheme {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub black: Option<String>,
    pub blue: Option<String>,
    pub bright_black: Option<String>,
    pub bright_blue: Option<String>,
    pub bright_cyan: Option<String>,
    pub bright_green: Option<String>,
    pub bright_magenta: Option<String>,
    pub bright_red: Option<String>,
    pub bright_white: Option<String>,
    pub bright_yellow: Option<String>,
    pub cursor: Option<String>,
    pub cursor_accent: Option<String>,
    pub cyan: Option<String>,
    pub green: Option<String>,
    pub magenta: Option<String>,
    pub red: Option<String>,
    pub selection_background: Option<String>,
    pub selection_foreground: Option<String>,
    pub selection_inactive_background: Option<String>,
    pub white: Option<String>,
    pub yellow: Option<String>,
}

impl WebClientTheme {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let mut theme = WebClientTheme::default();
        let colors = kdl_children_or_error!(kdl, "empty theme");

        // Helper function to extract colors
        let extract_color = |name: &str| -> Result<Option<String>, ConfigError> {
            if colors.get(name).is_some() {
                let color = PaletteColor::try_from((name, colors))?;
                Ok(Some(color.as_rgb_str()))
            } else {
                Ok(None)
            }
        };

        theme.background = extract_color("background")?;
        theme.foreground = extract_color("foreground")?;
        theme.black = extract_color("black")?;
        theme.blue = extract_color("blue")?;
        theme.bright_black = extract_color("bright_black")?;
        theme.bright_blue = extract_color("bright_blue")?;
        theme.bright_cyan = extract_color("bright_cyan")?;
        theme.bright_green = extract_color("bright_green")?;
        theme.bright_magenta = extract_color("bright_magenta")?;
        theme.bright_red = extract_color("bright_red")?;
        theme.bright_white = extract_color("bright_white")?;
        theme.bright_yellow = extract_color("bright_yellow")?;
        theme.cursor = extract_color("cursor")?;
        theme.cursor_accent = extract_color("cursor_accent")?;
        theme.cyan = extract_color("cyan")?;
        theme.green = extract_color("green")?;
        theme.magenta = extract_color("magenta")?;
        theme.red = extract_color("red")?;
        theme.selection_background = extract_color("selection_background")?;
        theme.selection_foreground = extract_color("selection_foreground")?;
        theme.selection_inactive_background = extract_color("selection_inactive_background")?;
        theme.white = extract_color("white")?;
        theme.yellow = extract_color("yellow")?;

        Ok(theme)
    }

    pub fn to_kdl(&self) -> KdlNode {
        let mut theme_node = KdlNode::new("theme");
        let mut theme_children = KdlDocument::new();

        if let Some(background) = &self.background {
            let mut background_node = KdlNode::new("background");
            background_node.push(KdlValue::String(background.clone()));
            theme_children.nodes_mut().push(background_node);
        }

        if let Some(foreground) = &self.foreground {
            let mut foreground_node = KdlNode::new("foreground");
            foreground_node.push(KdlValue::String(foreground.clone()));
            theme_children.nodes_mut().push(foreground_node);
        }

        theme_node.set_children(theme_children);
        theme_node
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebClientConfig {
    pub font: String,
    pub theme: Option<WebClientTheme>,
}

impl Default for WebClientConfig {
    fn default() -> Self {
        WebClientConfig {
            font: "monospace".to_string(),
            theme: None,
        }
    }
}

impl WebClientConfig {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let mut web_client_config = WebClientConfig::default();

        if let Some(font) = kdl_get_child_entry_string_value!(kdl, "font") {
            web_client_config.font = font.to_owned();
        }

        if let Some(theme_node) = kdl_get_child!(kdl, "theme") {
            web_client_config.theme = Some(WebClientTheme::from_kdl(theme_node)?);
        }

        Ok(web_client_config)
    }

    pub fn to_kdl(&self) -> KdlNode {
        let mut web_client_node = KdlNode::new("web_client");
        let mut web_client_children = KdlDocument::new();
        let mut font_node = KdlNode::new("font");
        font_node.push(KdlValue::String(self.font.clone()));
        web_client_children.nodes_mut().push(font_node);
        web_client_node.set_children(web_client_children);
        web_client_node
    }

    pub fn merge(&self, other: WebClientConfig) -> Self {
        let mut merged = self.clone();
        merged.font = other.font;
        merged.theme = other.theme;
        merged
    }
}

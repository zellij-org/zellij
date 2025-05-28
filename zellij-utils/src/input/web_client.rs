use kdl::{KdlDocument, KdlNode, KdlValue};
use serde::{Deserialize, Serialize};

use crate::{kdl_get_child, kdl_get_child_entry_string_value};

use super::config::ConfigError;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebClientTheme {
    pub background: Option<String>,
    pub foreground: Option<String>,
}

impl WebClientTheme {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let mut theme = WebClientTheme::default();

        if let Some(background) = kdl_get_child_entry_string_value!(kdl, "background") {
            // TODO: color parsing
            theme.background = Some(background.to_owned());
        }

        if let Some(foreground) = kdl_get_child_entry_string_value!(kdl, "foreground") {
            // TODO: color parsing
            theme.foreground = Some(foreground.to_owned());
        }

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

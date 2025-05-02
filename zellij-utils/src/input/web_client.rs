use kdl::{KdlDocument, KdlNode, KdlValue};
use serde::{Deserialize, Serialize};

use crate::kdl_get_child_entry_string_value;

use super::config::ConfigError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebClientConfig {
    pub font: String,
}

impl Default for WebClientConfig {
    fn default() -> Self {
        WebClientConfig {
            font: "monospace".to_string(),
        }
    }
}

impl WebClientConfig {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let mut web_client_config = WebClientConfig::default();

        if let Some(font) = kdl_get_child_entry_string_value!(kdl, "font") {
            web_client_config.font = font.to_owned();
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
        merged
    }
}

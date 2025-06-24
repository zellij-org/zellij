use kdl::{KdlDocument, KdlNode, KdlValue};
use serde::{Deserialize, Serialize};

use crate::{
    data::PaletteColor, kdl_children_or_error, kdl_first_entry_as_string, kdl_get_child,
    kdl_get_child_entry_bool_value, kdl_get_child_entry_string_value,
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
        macro_rules! add_color_nodes {
            ($theme_children:expr, $self:expr, $($field:ident),+ $(,)?) => {
                $(
                    if let Some(color) = &$self.$field {
                        let node = PaletteColor::from_rgb_str(color).to_kdl(stringify!($field));
                        $theme_children.nodes_mut().push(node);
                    }
                )+
            };
        }
        let mut theme_node = KdlNode::new("theme");
        let mut theme_children = KdlDocument::new();

        add_color_nodes!(
            theme_children,
            self,
            background,
            foreground,
            black,
            blue,
            bright_black,
            bright_blue,
            bright_cyan,
            bright_green,
            bright_magenta,
            bright_red,
            bright_white,
            bright_yellow,
            cursor,
            cursor_accent,
            cyan,
            green,
            magenta,
            red,
            selection_background,
            selection_foreground,
            selection_inactive_background,
            white,
            yellow,
        );

        theme_node.set_children(theme_children);
        theme_node
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CursorInactiveStyle {
    Outline,
    Block,
    Bar,
    Underline,
    NoStyle,
}

impl std::fmt::Display for CursorInactiveStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CursorInactiveStyle::Block => write!(f, "block"),
            CursorInactiveStyle::Bar => write!(f, "bar"),
            CursorInactiveStyle::Underline => write!(f, "underline"),
            CursorInactiveStyle::Outline => write!(f, "outline"),
            CursorInactiveStyle::NoStyle => write!(f, "none"),
        }
    }
}

impl CursorInactiveStyle {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        match kdl_first_entry_as_string!(kdl) {
            Some("block") => Ok(CursorInactiveStyle::Block),
            Some("bar") => Ok(CursorInactiveStyle::Bar),
            Some("underline") => Ok(CursorInactiveStyle::Underline),
            Some("outline") => Ok(CursorInactiveStyle::Outline),
            Some("no_style") => Ok(CursorInactiveStyle::NoStyle),
            _ => Err(ConfigError::new_kdl_error(
                format!("Must be 'block', 'bar', 'underline', 'outline' or 'no_style'"),
                kdl.span().offset(),
                kdl.span().len(),
            )),
        }
    }
    pub fn to_kdl(&self) -> KdlNode {
        let mut cursor_inactive_style_node = KdlNode::new("cursor_inactive_style");
        match self {
            CursorInactiveStyle::Block => {
                cursor_inactive_style_node.push(KdlValue::String("block".to_owned()));
            },
            CursorInactiveStyle::Bar => {
                cursor_inactive_style_node.push(KdlValue::String("bar".to_owned()));
            },
            CursorInactiveStyle::Underline => {
                cursor_inactive_style_node.push(KdlValue::String("underline".to_owned()));
            },
            CursorInactiveStyle::Outline => {
                cursor_inactive_style_node.push(KdlValue::String("outline".to_owned()));
            },
            CursorInactiveStyle::NoStyle => {
                cursor_inactive_style_node.push(KdlValue::String("no_style".to_owned()));
            },
        }
        cursor_inactive_style_node
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
}

impl std::fmt::Display for CursorStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CursorStyle::Block => write!(f, "block"),
            CursorStyle::Bar => write!(f, "bar"),
            CursorStyle::Underline => write!(f, "underline"),
        }
    }
}

impl CursorStyle {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        match kdl_first_entry_as_string!(kdl) {
            Some("block") => Ok(CursorStyle::Block),
            Some("bar") => Ok(CursorStyle::Bar),
            Some("underline") => Ok(CursorStyle::Underline),
            _ => Err(ConfigError::new_kdl_error(
                format!("Must be 'block', 'bar' or 'underline'"),
                kdl.span().offset(),
                kdl.span().len(),
            )),
        }
    }
    pub fn to_kdl(&self) -> KdlNode {
        let mut cursor_style_node = KdlNode::new("cursor_style");
        match self {
            CursorStyle::Block => {
                cursor_style_node.push(KdlValue::String("block".to_owned()));
            },
            CursorStyle::Bar => {
                cursor_style_node.push(KdlValue::String("bar".to_owned()));
            },
            CursorStyle::Underline => {
                cursor_style_node.push(KdlValue::String("underline".to_owned()));
            },
        }
        cursor_style_node
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebClientConfig {
    pub font: String,
    pub theme: Option<WebClientTheme>,
    pub cursor_blink: bool,
    pub cursor_inactive_style: Option<CursorInactiveStyle>,
    pub cursor_style: Option<CursorStyle>,
    pub mac_option_is_meta: bool,
}

impl Default for WebClientConfig {
    fn default() -> Self {
        WebClientConfig {
            font: "monospace".to_string(),
            theme: None,
            cursor_blink: false,
            cursor_inactive_style: None,
            cursor_style: None,
            mac_option_is_meta: true, // TODO: yes? no?
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

        if let Some(cursor_blink) = kdl_get_child_entry_bool_value!(kdl, "cursor_blink") {
            web_client_config.cursor_blink = cursor_blink;
        }

        if let Some(cursor_inactive_style_node) = kdl_get_child!(kdl, "cursor_inactive_style") {
            web_client_config.cursor_inactive_style =
                Some(CursorInactiveStyle::from_kdl(cursor_inactive_style_node)?);
        }

        if let Some(cursor_style_node) = kdl_get_child!(kdl, "cursor_style") {
            web_client_config.cursor_style = Some(CursorStyle::from_kdl(cursor_style_node)?);
        }

        if let Some(mac_option_is_meta) = kdl_get_child_entry_bool_value!(kdl, "mac_option_is_meta")
        {
            web_client_config.mac_option_is_meta = mac_option_is_meta;
        }

        Ok(web_client_config)
    }

    pub fn to_kdl(&self) -> KdlNode {
        let mut web_client_node = KdlNode::new("web_client");
        let mut web_client_children = KdlDocument::new();

        let mut font_node = KdlNode::new("font");
        font_node.push(KdlValue::String(self.font.clone()));
        web_client_children.nodes_mut().push(font_node);

        if let Some(theme_node) = self.theme.as_ref().map(|t| t.to_kdl()) {
            web_client_children.nodes_mut().push(theme_node);
        }

        if self.cursor_blink {
            // this defaults to false, so we only need to add it if it's true
            let mut cursor_blink_node = KdlNode::new("cursor_blink");
            cursor_blink_node.push(KdlValue::Bool(true));
            web_client_children.nodes_mut().push(cursor_blink_node);
        }

        if let Some(cursor_inactive_style_node) =
            self.cursor_inactive_style.as_ref().map(|c| c.to_kdl())
        {
            web_client_children
                .nodes_mut()
                .push(cursor_inactive_style_node);
        }

        if let Some(cursor_style_node) = self.cursor_style.as_ref().map(|c| c.to_kdl()) {
            web_client_children.nodes_mut().push(cursor_style_node);
        }

        if !self.mac_option_is_meta {
            // this defaults to true, so we only need to add it if it's false
            let mut mac_option_is_meta_node = KdlNode::new("mac_option_is_meta");
            mac_option_is_meta_node.push(KdlValue::Bool(false));
            web_client_children
                .nodes_mut()
                .push(mac_option_is_meta_node);
        }

        web_client_node.set_children(web_client_children);
        web_client_node
    }

    pub fn merge(&self, other: WebClientConfig) -> Self {
        let mut merged = self.clone();
        merged.font = other.font;
        merged.theme = other.theme;
        merged.cursor_blink = other.cursor_blink;
        merged.cursor_inactive_style = other.cursor_inactive_style;
        merged.cursor_style = other.cursor_style;
        merged.mac_option_is_meta = other.mac_option_is_meta;
        merged
    }
}

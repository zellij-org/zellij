use std::str::FromStr;

use kdl::{KdlDocument, KdlNode, KdlValue};
use serde::{Deserialize, Serialize};

use crate::data::{KeyWithModifier, PaletteColor};
use crate::input::theme::TERMINAL_COLOR_NAMES;
use crate::kdl::palette_color_refusing_index;
use crate::{kdl_get_child, kdl_get_child_entry_bool_value, kdl_get_child_entry_string_value};

use super::config::ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
}

impl CursorStyle {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let named = kdl
            .entries()
            .iter()
            .next()
            .and_then(|entry| entry.value().as_string());
        match named.map(str::to_ascii_lowercase).as_deref() {
            Some("block") => Ok(CursorStyle::Block),
            Some("bar") | Some("beam") => Ok(CursorStyle::Bar),
            Some("underline") => Ok(CursorStyle::Underline),
            _ => Err(ConfigError::new_kdl_error(
                "cursor_style must be \"block\", \"bar\" or \"underline\"".to_owned(),
                kdl.span().offset(),
                kdl.span().len(),
            )),
        }
    }

    pub fn to_kdl(&self) -> KdlNode {
        let mut node = KdlNode::new("cursor_style");
        node.push(KdlValue::String(
            match self {
                CursorStyle::Block => "block",
                CursorStyle::Bar => "bar",
                CursorStyle::Underline => "underline",
            }
            .to_owned(),
        ));
        node
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BellMode {
    Visual,
    Audible,
    Both,
    None,
}

impl BellMode {
    pub fn rings(&self) -> bool {
        matches!(self, BellMode::Audible | BellMode::Both)
    }

    pub fn attends(&self) -> bool {
        matches!(self, BellMode::Visual | BellMode::Both)
    }

    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let named = kdl
            .entries()
            .iter()
            .next()
            .and_then(|entry| entry.value().as_string());
        match named.map(str::to_ascii_lowercase).as_deref() {
            Some("visual") => Ok(BellMode::Visual),
            Some("audible") => Ok(BellMode::Audible),
            Some("both") => Ok(BellMode::Both),
            Some("none") => Ok(BellMode::None),
            _ => Err(ConfigError::new_kdl_error(
                "bell must be \"visual\", \"audible\", \"both\" or \"none\"".to_owned(),
                kdl.span().offset(),
                kdl.span().len(),
            )),
        }
    }

    pub fn to_kdl(&self) -> KdlNode {
        let mut node = KdlNode::new("bell");
        node.push(KdlValue::String(
            match self {
                BellMode::Visual => "visual",
                BellMode::Audible => "audible",
                BellMode::Both => "both",
                BellMode::None => "none",
            }
            .to_owned(),
        ));
        node
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationMode {
    Desktop,
    Attention,
    None,
}

impl NotificationMode {
    pub fn to_desktop(&self) -> bool {
        matches!(self, NotificationMode::Desktop)
    }

    pub fn attends(&self) -> bool {
        matches!(
            self,
            NotificationMode::Desktop | NotificationMode::Attention
        )
    }

    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let named = kdl
            .entries()
            .iter()
            .next()
            .and_then(|entry| entry.value().as_string());
        match named.map(str::to_ascii_lowercase).as_deref() {
            Some("desktop") => Ok(NotificationMode::Desktop),
            Some("attention") => Ok(NotificationMode::Attention),
            Some("none") => Ok(NotificationMode::None),
            _ => Err(ConfigError::new_kdl_error(
                "notifications must be \"desktop\", \"attention\" or \"none\"".to_owned(),
                kdl.span().offset(),
                kdl.span().len(),
            )),
        }
    }

    pub fn to_kdl(&self) -> KdlNode {
        let mut node = KdlNode::new("notifications");
        node.push(KdlValue::String(
            match self {
                NotificationMode::Desktop => "desktop",
                NotificationMode::Attention => "attention",
                NotificationMode::None => "none",
            }
            .to_owned(),
        ));
        node
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
pub enum StartupMode {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
}

impl StartupMode {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let named = kdl
            .entries()
            .iter()
            .next()
            .and_then(|entry| entry.value().as_string());
        match named.map(str::to_ascii_lowercase).as_deref() {
            Some("windowed") => Ok(StartupMode::Windowed),
            Some("maximized") => Ok(StartupMode::Maximized),
            Some("fullscreen") => Ok(StartupMode::Fullscreen),
            _ => Err(ConfigError::new_kdl_error(
                "startup_mode must be \"windowed\", \"maximized\" or \"fullscreen\"".to_owned(),
                kdl.span().offset(),
                kdl.span().len(),
            )),
        }
    }

    pub fn to_kdl(&self) -> KdlNode {
        let mut node = KdlNode::new("startup_mode");
        node.push(KdlValue::String(
            match self {
                StartupMode::Windowed => "windowed",
                StartupMode::Maximized => "maximized",
                StartupMode::Fullscreen => "fullscreen",
            }
            .to_owned(),
        ));
        node
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpacityMode {
    #[default]
    Background,
    Everything,
}

impl OpacityMode {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let named = kdl
            .entries()
            .iter()
            .next()
            .and_then(|entry| entry.value().as_string());
        match named.map(str::to_ascii_lowercase).as_deref() {
            Some("background") => Ok(OpacityMode::Background),
            Some("everything") => Ok(OpacityMode::Everything),
            _ => Err(ConfigError::new_kdl_error(
                "opacity_mode must be \"background\" or \"everything\"".to_owned(),
                kdl.span().offset(),
                kdl.span().len(),
            )),
        }
    }

    pub fn to_kdl(&self) -> KdlNode {
        let mut node = KdlNode::new("opacity_mode");
        node.push(KdlValue::String(
            match self {
                OpacityMode::Background => "background",
                OpacityMode::Everything => "everything",
            }
            .to_owned(),
        ));
        node
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowTheme {
    pub foreground: Option<PaletteColor>,
    pub background: Option<PaletteColor>,
    pub cursor: Option<PaletteColor>,
    pub ansi: [Option<PaletteColor>; 16],
}

impl WindowTheme {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let mut theme = WindowTheme::default();
        let Some(colors) = kdl.children() else {
            return Ok(theme);
        };

        let named = |name: &str| -> Result<Option<PaletteColor>, ConfigError> {
            match colors.get(name) {
                Some(_) => Ok(Some(PaletteColor::try_from((name, colors))?)),
                None => Ok(None),
            }
        };

        theme.foreground = named("foreground")?;
        theme.background = named("background")?;
        theme.cursor = named("cursor")?;
        for (slot, name) in TERMINAL_COLOR_NAMES.iter().enumerate() {
            if colors.get(name).is_none() {
                continue;
            }
            theme.ansi[slot] = Some(palette_color_refusing_index(name, colors)?);
        }

        Ok(theme)
    }

    pub fn to_kdl(&self) -> Option<KdlNode> {
        let mut node = KdlNode::new("theme");
        let mut children = KdlDocument::new();

        for (name, color) in [
            ("foreground", self.foreground),
            ("background", self.background),
            ("cursor", self.cursor),
        ] {
            if let Some(color) = color {
                children.nodes_mut().push(color.to_kdl(name));
            }
        }
        for (slot, name) in TERMINAL_COLOR_NAMES.iter().enumerate() {
            if let Some(color) = self.ansi[slot] {
                children.nodes_mut().push(color.to_kdl(name));
            }
        }

        if children.nodes().is_empty() {
            return None;
        }
        node.set_children(children);
        Some(node)
    }

    pub fn merge(&self, other: WindowTheme) -> Self {
        let mut merged = self.clone();
        merged.foreground = other.foreground.or(merged.foreground);
        merged.background = other.background.or(merged.background);
        merged.cursor = other.cursor.or(merged.cursor);
        for slot in 0..merged.ansi.len() {
            merged.ansi[slot] = other.ansi[slot].or(merged.ansi[slot]);
        }
        merged
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowConfig {
    pub font: Option<String>,
    pub font_size: Option<f32>,
    pub system_fonts: Option<bool>,
    pub ligatures: Option<bool>,
    pub cursor_style: Option<CursorStyle>,
    pub cursor_blink: Option<bool>,
    pub paste_keys: Option<Vec<KeyWithModifier>>,
    pub zoom_in_keys: Option<Vec<KeyWithModifier>>,
    pub zoom_out_keys: Option<Vec<KeyWithModifier>>,
    pub zoom_reset_keys: Option<Vec<KeyWithModifier>>,
    pub middle_click_paste: Option<bool>,
    pub open_links: Option<bool>,
    pub bell: Option<BellMode>,
    pub notifications: Option<NotificationMode>,
    pub startup_mode: Option<StartupMode>,
    pub opacity: Option<f32>,
    pub opacity_mode: Option<OpacityMode>,
    pub blur: Option<bool>,
    pub theme: Option<WindowTheme>,
}

impl WindowConfig {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let mut window = WindowConfig::default();

        if let Some(font) = kdl_get_child_entry_string_value!(kdl, "font") {
            window.font = Some(font.to_owned());
        }
        if let Some(font_size) = kdl_get_child!(kdl, "font_size") {
            window.font_size = Some(font_size_from_kdl(font_size)?);
        }
        if let Some(system_fonts) = kdl_get_child_entry_bool_value!(kdl, "system_fonts") {
            window.system_fonts = Some(system_fonts);
        }
        if let Some(ligatures) = kdl_get_child_entry_bool_value!(kdl, "ligatures") {
            window.ligatures = Some(ligatures);
        }
        if let Some(cursor_style) = kdl_get_child!(kdl, "cursor_style") {
            window.cursor_style = Some(CursorStyle::from_kdl(cursor_style)?);
        }
        if let Some(cursor_blink) = kdl_get_child_entry_bool_value!(kdl, "cursor_blink") {
            window.cursor_blink = Some(cursor_blink);
        }
        if let Some(paste_keys) = kdl_get_child!(kdl, "paste_keys") {
            window.paste_keys = Some(keys_from_kdl(paste_keys)?);
        }
        if let Some(zoom_in_keys) = kdl_get_child!(kdl, "zoom_in_keys") {
            window.zoom_in_keys = Some(keys_from_kdl(zoom_in_keys)?);
        }
        if let Some(zoom_out_keys) = kdl_get_child!(kdl, "zoom_out_keys") {
            window.zoom_out_keys = Some(keys_from_kdl(zoom_out_keys)?);
        }
        if let Some(zoom_reset_keys) = kdl_get_child!(kdl, "zoom_reset_keys") {
            window.zoom_reset_keys = Some(keys_from_kdl(zoom_reset_keys)?);
        }
        if let Some(middle_click_paste) = kdl_get_child_entry_bool_value!(kdl, "middle_click_paste")
        {
            window.middle_click_paste = Some(middle_click_paste);
        }
        if let Some(open_links) = kdl_get_child_entry_bool_value!(kdl, "open_links") {
            window.open_links = Some(open_links);
        }
        if let Some(bell) = kdl_get_child!(kdl, "bell") {
            window.bell = Some(BellMode::from_kdl(bell)?);
        }
        if let Some(notifications) = kdl_get_child!(kdl, "notifications") {
            window.notifications = Some(NotificationMode::from_kdl(notifications)?);
        }
        if let Some(startup_mode) = kdl_get_child!(kdl, "startup_mode") {
            window.startup_mode = Some(StartupMode::from_kdl(startup_mode)?);
        }
        if let Some(opacity) = kdl_get_child!(kdl, "opacity") {
            window.opacity = Some(opacity_from_kdl(opacity)?);
        }
        if let Some(opacity_mode) = kdl_get_child!(kdl, "opacity_mode") {
            window.opacity_mode = Some(OpacityMode::from_kdl(opacity_mode)?);
        }
        if let Some(blur) = kdl_get_child_entry_bool_value!(kdl, "blur") {
            window.blur = Some(blur);
        }
        if let Some(theme) = kdl_get_child!(kdl, "theme") {
            window.theme = Some(WindowTheme::from_kdl(theme)?);
        }

        Ok(window)
    }

    pub fn to_kdl(&self) -> Option<KdlNode> {
        let mut node = KdlNode::new("window");
        let mut children = KdlDocument::new();

        if let Some(font) = &self.font {
            let mut font_node = KdlNode::new("font");
            font_node.push(KdlValue::String(font.clone()));
            children.nodes_mut().push(font_node);
        }
        if let Some(font_size) = self.font_size {
            let mut font_size_node = KdlNode::new("font_size");
            if font_size.fract() == 0.0 {
                font_size_node.push(KdlValue::Base10(font_size as i64));
            } else {
                font_size_node.push(KdlValue::Base10Float(font_size as f64));
            }
            children.nodes_mut().push(font_size_node);
        }
        if let Some(system_fonts) = self.system_fonts {
            let mut system_fonts_node = KdlNode::new("system_fonts");
            system_fonts_node.push(KdlValue::Bool(system_fonts));
            children.nodes_mut().push(system_fonts_node);
        }
        if let Some(ligatures) = self.ligatures {
            let mut ligatures_node = KdlNode::new("ligatures");
            ligatures_node.push(KdlValue::Bool(ligatures));
            children.nodes_mut().push(ligatures_node);
        }
        if let Some(cursor_style) = self.cursor_style {
            children.nodes_mut().push(cursor_style.to_kdl());
        }
        if let Some(cursor_blink) = self.cursor_blink {
            let mut cursor_blink_node = KdlNode::new("cursor_blink");
            cursor_blink_node.push(KdlValue::Bool(cursor_blink));
            children.nodes_mut().push(cursor_blink_node);
        }
        for (name, keys) in [
            ("paste_keys", &self.paste_keys),
            ("zoom_in_keys", &self.zoom_in_keys),
            ("zoom_out_keys", &self.zoom_out_keys),
            ("zoom_reset_keys", &self.zoom_reset_keys),
        ] {
            let Some(keys) = keys else {
                continue;
            };
            let mut keys_node = KdlNode::new(name);
            for key in keys {
                keys_node.push(KdlValue::String(key.to_kdl()));
            }
            children.nodes_mut().push(keys_node);
        }
        if let Some(middle_click_paste) = self.middle_click_paste {
            let mut middle_click_paste_node = KdlNode::new("middle_click_paste");
            middle_click_paste_node.push(KdlValue::Bool(middle_click_paste));
            children.nodes_mut().push(middle_click_paste_node);
        }
        if let Some(open_links) = self.open_links {
            let mut open_links_node = KdlNode::new("open_links");
            open_links_node.push(KdlValue::Bool(open_links));
            children.nodes_mut().push(open_links_node);
        }
        if let Some(bell) = self.bell {
            children.nodes_mut().push(bell.to_kdl());
        }
        if let Some(notifications) = self.notifications {
            children.nodes_mut().push(notifications.to_kdl());
        }
        if let Some(startup_mode) = self.startup_mode {
            children.nodes_mut().push(startup_mode.to_kdl());
        }
        if let Some(opacity) = self.opacity {
            let mut opacity_node = KdlNode::new("opacity");
            if opacity.fract() == 0.0 {
                opacity_node.push(KdlValue::Base10(opacity as i64));
            } else {
                opacity_node.push(KdlValue::Base10Float(opacity as f64));
            }
            children.nodes_mut().push(opacity_node);
        }
        if let Some(opacity_mode) = self.opacity_mode {
            children.nodes_mut().push(opacity_mode.to_kdl());
        }
        if let Some(blur) = self.blur {
            let mut blur_node = KdlNode::new("blur");
            blur_node.push(KdlValue::Bool(blur));
            children.nodes_mut().push(blur_node);
        }
        if let Some(theme) = self.theme.as_ref().and_then(|theme| theme.to_kdl()) {
            children.nodes_mut().push(theme);
        }

        if children.nodes().is_empty() {
            return None;
        }
        node.set_children(children);
        Some(node)
    }

    pub fn merge(&self, other: WindowConfig) -> Self {
        let mut merged = self.clone();
        merged.font = other.font.or(merged.font);
        merged.font_size = other.font_size.or(merged.font_size);
        merged.system_fonts = other.system_fonts.or(merged.system_fonts);
        merged.ligatures = other.ligatures.or(merged.ligatures);
        merged.cursor_style = other.cursor_style.or(merged.cursor_style);
        merged.cursor_blink = other.cursor_blink.or(merged.cursor_blink);
        merged.paste_keys = other.paste_keys.or(merged.paste_keys);
        merged.zoom_in_keys = other.zoom_in_keys.or(merged.zoom_in_keys);
        merged.zoom_out_keys = other.zoom_out_keys.or(merged.zoom_out_keys);
        merged.zoom_reset_keys = other.zoom_reset_keys.or(merged.zoom_reset_keys);
        merged.middle_click_paste = other.middle_click_paste.or(merged.middle_click_paste);
        merged.open_links = other.open_links.or(merged.open_links);
        merged.bell = other.bell.or(merged.bell);
        merged.notifications = other.notifications.or(merged.notifications);
        merged.startup_mode = other.startup_mode.or(merged.startup_mode);
        merged.opacity = other.opacity.or(merged.opacity);
        merged.opacity_mode = other.opacity_mode.or(merged.opacity_mode);
        merged.blur = other.blur.or(merged.blur);
        merged.theme = match (merged.theme, other.theme) {
            (Some(mine), Some(theirs)) => Some(mine.merge(theirs)),
            (mine, theirs) => theirs.or(mine),
        };
        merged
    }
}

fn font_size_from_kdl(node: &KdlNode) -> Result<f32, ConfigError> {
    let error = |message: String| {
        ConfigError::new_kdl_error(message, node.span().offset(), node.span().len())
    };
    let value = node
        .entries()
        .iter()
        .next()
        .map(|entry| entry.value())
        .ok_or_else(|| error("font_size needs a value".to_owned()))?;
    let size = match value {
        KdlValue::Base10Float(size) => *size,
        other => other
            .as_i64()
            .map(|size| size as f64)
            .ok_or_else(|| error("font_size must be a number".to_owned()))?,
    };
    if !(size.is_finite() && size > 0.0) {
        return Err(error(format!("font_size {} is not a usable size", size)));
    }
    Ok(size as f32)
}

fn opacity_from_kdl(node: &KdlNode) -> Result<f32, ConfigError> {
    let error = |message: String| {
        ConfigError::new_kdl_error(message, node.span().offset(), node.span().len())
    };
    let value = node
        .entries()
        .iter()
        .next()
        .map(|entry| entry.value())
        .ok_or_else(|| error("opacity needs a value between 0.0 and 1.0".to_owned()))?;
    let opacity = match value {
        KdlValue::Base10Float(opacity) => *opacity,
        other => other.as_i64().map(|opacity| opacity as f64).ok_or_else(|| {
            error("opacity must be a number between 0.0 and 1.0".to_owned())
        })?,
    };
    if !(opacity.is_finite() && (0.0..=1.0).contains(&opacity)) {
        return Err(error(format!(
            "opacity {} is outside the range 0.0 to 1.0",
            opacity
        )));
    }
    Ok(opacity as f32)
}

fn keys_from_kdl(node: &KdlNode) -> Result<Vec<KeyWithModifier>, ConfigError> {
    let mut keys = Vec::new();
    for entry in node.entries() {
        let error = |message: String| {
            ConfigError::new_kdl_error(message, entry.span().offset(), entry.span().len())
        };
        let text = entry
            .value()
            .as_string()
            .ok_or_else(|| error("a key must be given as a string".to_owned()))?;
        let key = KeyWithModifier::from_str(text)
            .map_err(|e| error(format!("{:?} is not a key: {}", text, e)))?;
        keys.push(key);
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{BareKey, KeyModifier};

    fn section(kdl: &str) -> Result<WindowConfig, ConfigError> {
        let document: KdlDocument = kdl.parse()?;
        let node = document.get("window").expect("no window section");
        WindowConfig::from_kdl(node)
    }

    #[test]
    fn an_absent_section_is_every_option_unset() {
        assert_eq!(WindowConfig::default(), WindowConfig::default());
        assert!(WindowConfig::default().to_kdl().is_none());
    }

    #[test]
    fn an_empty_section_sets_nothing() {
        assert_eq!(section("window {\n}").unwrap(), WindowConfig::default());
    }

    #[test]
    fn every_option_round_trips_through_kdl() {
        let parsed = section(
            r##"
            window {
                font "Iosevka Term"
                font_size 18
                system_fonts false
                ligatures false
                cursor_style "bar"
                cursor_blink true
                paste_keys "Ctrl Shift v" "Shift Insert"
                zoom_in_keys "Ctrl =" "Ctrl +"
                zoom_out_keys "Ctrl -"
                zoom_reset_keys "Ctrl 0"
                middle_click_paste false
                open_links false
                bell "both"
                notifications "desktop"
                startup_mode "fullscreen"
                opacity 0.85
                opacity_mode "everything"
                blur true
                theme {
                    foreground "#e5e5e5"
                    background 0 0 0
                    cursor "#ff00ff"
                    red 205 0 0
                    bright_white "#ffffff"
                }
            }
            "##,
        )
        .unwrap();

        assert_eq!(parsed.font.as_deref(), Some("Iosevka Term"));
        assert_eq!(parsed.font_size, Some(18.0));
        assert_eq!(parsed.system_fonts, Some(false));
        assert_eq!(parsed.ligatures, Some(false));
        assert_eq!(parsed.cursor_style, Some(CursorStyle::Bar));
        assert_eq!(parsed.cursor_blink, Some(true));
        assert_eq!(
            parsed.paste_keys,
            Some(vec![
                KeyWithModifier::new(BareKey::Char('v'))
                    .with_ctrl_modifier()
                    .with_shift_modifier(),
                KeyWithModifier::new(BareKey::Insert).with_shift_modifier(),
            ])
        );
        assert_eq!(
            parsed.zoom_in_keys,
            Some(vec![
                KeyWithModifier::new(BareKey::Char('=')).with_ctrl_modifier(),
                KeyWithModifier::new(BareKey::Char('+')).with_ctrl_modifier(),
            ])
        );
        assert_eq!(
            parsed.zoom_out_keys,
            Some(vec![
                KeyWithModifier::new(BareKey::Char('-')).with_ctrl_modifier()
            ])
        );
        assert_eq!(
            parsed.zoom_reset_keys,
            Some(vec![
                KeyWithModifier::new(BareKey::Char('0')).with_ctrl_modifier()
            ])
        );
        assert_eq!(parsed.middle_click_paste, Some(false));
        assert_eq!(parsed.open_links, Some(false));
        assert_eq!(parsed.bell, Some(BellMode::Both));
        assert_eq!(parsed.notifications, Some(NotificationMode::Desktop));
        assert_eq!(parsed.startup_mode, Some(StartupMode::Fullscreen));
        assert_eq!(parsed.opacity, Some(0.85));
        assert_eq!(parsed.opacity_mode, Some(OpacityMode::Everything));
        assert_eq!(parsed.blur, Some(true));
        let theme = parsed.theme.clone().unwrap();
        assert_eq!(theme.foreground, Some(PaletteColor::Rgb((229, 229, 229))));
        assert_eq!(theme.background, Some(PaletteColor::Rgb((0, 0, 0))));
        assert_eq!(theme.cursor, Some(PaletteColor::Rgb((255, 0, 255))));
        assert_eq!(theme.ansi[1], Some(PaletteColor::Rgb((205, 0, 0))));
        assert_eq!(theme.ansi[15], Some(PaletteColor::Rgb((255, 255, 255))));
        assert_eq!(theme.ansi[0], None);

        let emitted = parsed.to_kdl().unwrap().to_string();
        let reparsed = section(&emitted).unwrap();
        assert_eq!(reparsed, parsed);
    }

    #[test]
    fn a_fractional_font_size_survives_the_round_trip() {
        let parsed = section("window {\n font_size 13.5\n}").unwrap();
        assert_eq!(parsed.font_size, Some(13.5));
        let reparsed = section(&parsed.to_kdl().unwrap().to_string()).unwrap();
        assert_eq!(reparsed.font_size, Some(13.5));
    }

    #[test]
    fn paste_keys_with_no_values_claims_no_chords() {
        assert_eq!(
            section("window {\n paste_keys\n}").unwrap().paste_keys,
            Some(Vec::new())
        );
    }

    #[test]
    fn opening_links_can_be_switched_off() {
        assert_eq!(
            section("window {\n open_links false\n}")
                .unwrap()
                .open_links,
            Some(false)
        );
        assert_eq!(
            section("window {\n open_links true\n}").unwrap().open_links,
            Some(true)
        );
        assert_eq!(section("window {\n}").unwrap().open_links, None);
    }

    #[test]
    fn a_key_that_does_not_parse_is_a_config_error() {
        assert!(section("window {\n paste_keys \"Ctrl Shift nonsense\"\n}").is_err());
    }

    #[test]
    fn a_palette_index_is_refused_for_an_ansi_slot() {
        let err = section("window {\n theme {\n red 9\n }\n}").unwrap_err();
        assert!(format!("{:?}", err).contains("red"), "{:?}", err);
    }

    #[test]
    fn a_palette_index_is_accepted_for_the_derived_slots() {
        let parsed = section("window {\n theme {\n foreground 4\n }\n}").unwrap();
        assert_eq!(
            parsed.theme.unwrap().foreground,
            Some(PaletteColor::EightBit(4))
        );
    }

    #[test]
    fn a_font_size_that_is_not_a_usable_number_is_refused() {
        assert!(section("window {\n font_size 0\n}").is_err());
        assert!(section("window {\n font_size \"large\"\n}").is_err());
    }

    #[test]
    fn a_later_section_supersedes_an_earlier_one_field_by_field() {
        let first =
            section("window {\n font \"A\"\n font_size 12\n theme {\n red 1 1 1\n }\n}").unwrap();
        let second = section("window {\n font_size 20\n theme {\n green 2 2 2\n }\n}").unwrap();
        let merged = first.merge(second);
        assert_eq!(merged.font.as_deref(), Some("A"));
        assert_eq!(merged.startup_mode, None);
        let maximized = section("window {\n startup_mode \"maximized\"\n}").unwrap();
        let windowed = section("window {\n startup_mode \"windowed\"\n}").unwrap();
        assert_eq!(
            maximized.merge(windowed.clone()).startup_mode,
            Some(StartupMode::Windowed)
        );
        assert_eq!(
            maximized.merge(WindowConfig::default()).startup_mode,
            Some(StartupMode::Maximized)
        );
        assert_eq!(merged.font_size, Some(20.0));
        let theme = merged.theme.unwrap();
        assert_eq!(theme.ansi[1], Some(PaletteColor::Rgb((1, 1, 1))));
        assert_eq!(theme.ansi[2], Some(PaletteColor::Rgb((2, 2, 2))));
    }

    #[test]
    fn every_cursor_style_is_named_the_way_the_web_client_names_it() {
        for (text, expected) in [
            ("block", CursorStyle::Block),
            ("bar", CursorStyle::Bar),
            ("underline", CursorStyle::Underline),
            ("beam", CursorStyle::Bar),
            ("BAR", CursorStyle::Bar),
        ] {
            let parsed = section(&format!("window {{\n cursor_style \"{}\"\n}}", text)).unwrap();
            assert_eq!(parsed.cursor_style, Some(expected), "{}", text);
        }
    }

    #[test]
    fn a_cursor_blink_preference_is_three_valued() {
        assert_eq!(
            section("window {\n cursor_blink true\n}")
                .unwrap()
                .cursor_blink,
            Some(true)
        );
        assert_eq!(
            section("window {\n cursor_blink false\n}")
                .unwrap()
                .cursor_blink,
            Some(false)
        );
        assert_eq!(
            section("window {\n}").unwrap().cursor_blink,
            None,
            "an absent key must stay distinguishable from a refusal to blink"
        );
    }

    #[test]
    fn a_cursor_style_that_is_not_a_shape_is_a_config_error() {
        assert!(section("window {\n cursor_style \"wedge\"\n}").is_err());
        assert!(section("window {\n cursor_style\n}").is_err());
    }

    #[test]
    fn the_section_travels_with_the_configuration_it_lives_in() {
        use crate::input::config::Config;

        let config = Config::from_kdl(
            "window {\n font \"Configured\"\n cursor_style \"bar\"\n}\n",
            None,
        )
        .unwrap();
        assert_eq!(config.window.font.as_deref(), Some("Configured"));

        let mut base = Config::default();
        base.merge(config).unwrap();
        assert_eq!(base.window.font.as_deref(), Some("Configured"));
        assert_eq!(base.window.cursor_style, Some(CursorStyle::Bar));
    }

    #[test]
    fn a_configuration_that_never_names_the_section_writes_nothing_back() {
        use crate::input::config::Config;

        let config = Config::from_kdl("default_mode \"locked\"\n", None).unwrap();
        assert_eq!(config.window, WindowConfig::default());
        assert!(
            !config.to_string(false).contains("window"),
            "an untouched section was written back into the configuration"
        );
    }

    #[test]
    fn a_section_a_binary_does_not_know_is_ignored_rather_than_refused() {
        use crate::input::config::Config;

        let config = Config::from_kdl(
            "window {\n font \"Configured\"\n}\nsomething_from_the_future {\n whatever true\n}\n",
            None,
        )
        .expect("an unknown top-level section must not refuse the whole configuration");
        assert_eq!(config.window.font.as_deref(), Some("Configured"));
    }

    #[test]
    fn every_bell_mode_is_named_and_says_what_it_does() {
        for (text, expected, rings, attends) in [
            ("visual", BellMode::Visual, false, true),
            ("audible", BellMode::Audible, true, false),
            ("both", BellMode::Both, true, true),
            ("none", BellMode::None, false, false),
            ("BOTH", BellMode::Both, true, true),
        ] {
            let parsed = section(&format!("window {{\n bell \"{}\"\n}}", text)).unwrap();
            assert_eq!(parsed.bell, Some(expected), "{}", text);
            assert_eq!(expected.rings(), rings, "{}", text);
            assert_eq!(expected.attends(), attends, "{}", text);
        }
        assert!(section("window {\n bell \"loud\"\n}").is_err());
        assert!(section("window {\n bell\n}").is_err());
        assert_eq!(
            section("window {\n}").unwrap().bell,
            None,
            "an absent key must stay distinguishable from a refusal to ring"
        );
    }

    #[test]
    fn every_notification_mode_is_named_and_says_what_it_does() {
        for (text, expected, to_desktop, attends) in [
            ("desktop", NotificationMode::Desktop, true, true),
            ("attention", NotificationMode::Attention, false, true),
            ("none", NotificationMode::None, false, false),
            ("DESKTOP", NotificationMode::Desktop, true, true),
        ] {
            let parsed = section(&format!("window {{\n notifications \"{}\"\n}}", text)).unwrap();
            assert_eq!(parsed.notifications, Some(expected), "{}", text);
            assert_eq!(expected.to_desktop(), to_desktop, "{}", text);
            assert_eq!(expected.attends(), attends, "{}", text);
        }
        assert!(section("window {\n notifications \"popup\"\n}").is_err());
        assert!(section("window {\n notifications\n}").is_err());
        assert_eq!(
            section("window {\n}").unwrap().notifications,
            None,
            "an absent key must leave the window manager in charge, as it always was"
        );
    }

    #[test]
    fn every_startup_mode_is_named_and_an_unknown_one_is_refused() {
        for (text, expected) in [
            ("windowed", StartupMode::Windowed),
            ("maximized", StartupMode::Maximized),
            ("fullscreen", StartupMode::Fullscreen),
            ("FullScreen", StartupMode::Fullscreen),
        ] {
            let parsed = section(&format!("window {{\n startup_mode \"{}\"\n}}", text)).unwrap();
            assert_eq!(parsed.startup_mode, Some(expected), "{}", text);
            let reparsed = section(&parsed.to_kdl().unwrap().to_string()).unwrap();
            assert_eq!(reparsed.startup_mode, Some(expected), "{}", text);
        }
        assert!(section("window {\n startup_mode \"minimized\"\n}").is_err());
        assert!(section("window {\n startup_mode\n}").is_err());
        assert_eq!(section("window {\n}").unwrap().startup_mode, None);
        assert_eq!(StartupMode::default(), StartupMode::Windowed);
    }

    #[test]
    fn opacity_accepts_the_unit_range_and_nothing_else() {
        for (text, expected) in [("0.8", 0.8f32), ("0", 0.0), ("1", 1.0), ("1.0", 1.0)] {
            let parsed = section(&format!("window {{\n opacity {}\n}}", text)).unwrap();
            assert_eq!(parsed.opacity, Some(expected), "{}", text);
            let reparsed = section(&parsed.to_kdl().unwrap().to_string()).unwrap();
            assert_eq!(reparsed.opacity, Some(expected), "{}", text);
        }
        for text in ["-0.1", "1.5", "2", "\"high\"", ""] {
            let err = section(&format!("window {{\n opacity {}\n}}", text)).unwrap_err();
            assert!(format!("{:?}", err).contains("opacity"), "{}: {:?}", text, err);
        }
    }

    #[test]
    fn every_opacity_mode_is_named_and_an_unknown_one_lists_the_valid_ones() {
        for (text, expected) in [
            ("background", OpacityMode::Background),
            ("everything", OpacityMode::Everything),
            ("Everything", OpacityMode::Everything),
        ] {
            let parsed = section(&format!("window {{\n opacity_mode \"{}\"\n}}", text)).unwrap();
            assert_eq!(parsed.opacity_mode, Some(expected), "{}", text);
        }
        let err = section("window {\n opacity_mode \"sideways\"\n}").unwrap_err();
        let message = format!("{:?}", err);
        assert!(
            message.contains("background") && message.contains("everything"),
            "{}",
            message
        );
        assert!(section("window {\n opacity_mode\n}").is_err());
        assert_eq!(OpacityMode::default(), OpacityMode::Background);
    }

    #[test]
    fn blur_parses_and_an_absent_see_through_setting_stays_unset() {
        assert_eq!(section("window {\n blur true\n}").unwrap().blur, Some(true));
        assert_eq!(
            section("window {\n blur false\n}").unwrap().blur,
            Some(false)
        );
        let empty = section("window {\n}").unwrap();
        assert_eq!(empty.opacity, None);
        assert_eq!(empty.opacity_mode, None);
        assert_eq!(empty.blur, None);
    }

    #[test]
    fn a_later_section_supersedes_the_see_through_settings_field_by_field() {
        let first =
            section("window {\n opacity 0.5\n opacity_mode \"everything\"\n blur true\n}")
                .unwrap();
        let second = section("window {\n opacity 0.9\n}").unwrap();
        let merged = first.merge(second);
        assert_eq!(merged.opacity, Some(0.9));
        assert_eq!(merged.opacity_mode, Some(OpacityMode::Everything));
        assert_eq!(merged.blur, Some(true));
        let unblurred = merged.merge(section("window {\n blur false\n}").unwrap());
        assert_eq!(unblurred.blur, Some(false));
        assert_eq!(unblurred.opacity, Some(0.9));
    }

    #[test]
    fn zoom_chords_with_no_values_claim_nothing() {
        let parsed = section("window {\n zoom_in_keys\n zoom_out_keys\n zoom_reset_keys\n}")
            .expect("empty zoom lists must parse");
        assert_eq!(parsed.zoom_in_keys, Some(Vec::new()));
        assert_eq!(parsed.zoom_out_keys, Some(Vec::new()));
        assert_eq!(parsed.zoom_reset_keys, Some(Vec::new()));
    }

    #[test]
    fn a_zoom_chord_that_does_not_parse_is_a_config_error() {
        assert!(section("window {\n zoom_in_keys \"Ctrl nonsense\"\n}").is_err());
    }

    #[test]
    fn a_modifierless_key_is_accepted() {
        let parsed = section("window {\n paste_keys \"F5\"\n}").unwrap();
        assert_eq!(
            parsed.paste_keys,
            Some(vec![KeyWithModifier::new(BareKey::F(5))])
        );
        assert!(parsed.paste_keys.unwrap()[0]
            .key_modifiers
            .iter()
            .next()
            .is_none());
        let _ = KeyModifier::Ctrl;
    }
}

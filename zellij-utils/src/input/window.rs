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
    Windowed,
    Maximized,
    Fullscreen,
    #[default]
    Remember,
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
            Some("remember") => Ok(StartupMode::Remember),
            _ => Err(ConfigError::new_kdl_error(
                "startup_mode must be \"windowed\", \"maximized\", \"fullscreen\" or \"remember\""
                    .to_owned(),
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
                StartupMode::Remember => "remember",
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaddingColor {
    #[default]
    Background,
    Extend,
}

impl PaddingColor {
    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let named = kdl
            .entries()
            .iter()
            .next()
            .and_then(|entry| entry.value().as_string());
        match named.map(str::to_ascii_lowercase).as_deref() {
            Some("background") => Ok(PaddingColor::Background),
            Some("extend") => Ok(PaddingColor::Extend),
            _ => Err(ConfigError::new_kdl_error(
                "padding_color must be \"background\" or \"extend\"".to_owned(),
                kdl.span().offset(),
                kdl.span().len(),
            )),
        }
    }

    pub fn to_kdl(&self) -> KdlNode {
        let mut node = KdlNode::new("padding_color");
        node.push(KdlValue::String(
            match self {
                PaddingColor::Background => "background",
                PaddingColor::Extend => "extend",
            }
            .to_owned(),
        ));
        node
    }
}

pub const FONT_WEIGHT_NAMES: &[(&str, u16)] = &[
    ("thin", 100),
    ("hairline", 100),
    ("extralight", 200),
    ("ultralight", 200),
    ("light", 300),
    ("regular", 400),
    ("normal", 400),
    ("medium", 500),
    ("semibold", 600),
    ("demibold", 600),
    ("bold", 700),
    ("extrabold", 800),
    ("ultrabold", 800),
    ("black", 900),
    ("heavy", 900),
];

pub const MIN_CELL_SCALE: f32 = 0.5;
pub const MAX_CELL_SCALE: f32 = 3.0;
pub const MAX_PIXEL_ADJUSTMENT: i32 = 64;
pub const MAX_PADDING: f32 = 1000.0;
pub const MIN_INITIAL_CELLS: u16 = 2;
pub const MAX_INITIAL_CELLS: u16 = 1000;

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
    pub padding: Option<f32>,
    pub padding_top: Option<f32>,
    pub padding_right: Option<f32>,
    pub padding_bottom: Option<f32>,
    pub padding_left: Option<f32>,
    pub padding_balance: Option<bool>,
    pub padding_color: Option<PaddingColor>,
    pub line_height: Option<f32>,
    pub cell_width: Option<f32>,
    pub baseline_offset: Option<i32>,
    pub underline_offset: Option<i32>,
    pub underline_thickness: Option<i32>,
    pub initial_columns: Option<u16>,
    pub initial_rows: Option<u16>,
    pub font_weight: Option<u16>,
    pub font_features: Option<Vec<String>>,
    pub fullscreen_keys: Option<Vec<KeyWithModifier>>,
    pub confirm_close: Option<bool>,
    pub hide_pointer_while_typing: Option<bool>,
    pub cursor_unfocused_hollow: Option<bool>,
    pub minimum_contrast: Option<f32>,
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
        if let Some(fullscreen_keys) = kdl_get_child!(kdl, "fullscreen_keys") {
            window.fullscreen_keys = Some(keys_from_kdl(fullscreen_keys)?);
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
        for (name, slot) in [
            ("padding", &mut window.padding),
            ("padding_top", &mut window.padding_top),
            ("padding_right", &mut window.padding_right),
            ("padding_bottom", &mut window.padding_bottom),
            ("padding_left", &mut window.padding_left),
        ] {
            if let Some(node) = kdl_get_child!(kdl, name) {
                *slot = Some(padding_from_kdl(node, name)?);
            }
        }
        if let Some(padding_balance) = kdl_get_child_entry_bool_value!(kdl, "padding_balance") {
            window.padding_balance = Some(padding_balance);
        }
        if let Some(padding_color) = kdl_get_child!(kdl, "padding_color") {
            window.padding_color = Some(PaddingColor::from_kdl(padding_color)?);
        }
        for (name, slot) in [
            ("line_height", &mut window.line_height),
            ("cell_width", &mut window.cell_width),
        ] {
            if let Some(node) = kdl_get_child!(kdl, name) {
                *slot = Some(cell_scale_from_kdl(node, name)?);
            }
        }
        for (name, slot) in [
            ("baseline_offset", &mut window.baseline_offset),
            ("underline_offset", &mut window.underline_offset),
            ("underline_thickness", &mut window.underline_thickness),
        ] {
            if let Some(node) = kdl_get_child!(kdl, name) {
                *slot = Some(pixel_adjustment_from_kdl(node, name)?);
            }
        }
        for (name, slot) in [
            ("initial_columns", &mut window.initial_columns),
            ("initial_rows", &mut window.initial_rows),
        ] {
            if let Some(node) = kdl_get_child!(kdl, name) {
                *slot = Some(initial_cells_from_kdl(node, name)?);
            }
        }
        if let Some(font_weight) = kdl_get_child!(kdl, "font_weight") {
            window.font_weight = Some(font_weight_from_kdl(font_weight)?);
        }
        if let Some(font_features) = kdl_get_child!(kdl, "font_features") {
            window.font_features = Some(font_features_from_kdl(font_features)?);
        }
        if let Some(confirm_close) = kdl_get_child_entry_bool_value!(kdl, "confirm_close") {
            window.confirm_close = Some(confirm_close);
        }
        if let Some(hide) = kdl_get_child_entry_bool_value!(kdl, "hide_pointer_while_typing") {
            window.hide_pointer_while_typing = Some(hide);
        }
        if let Some(hollow) = kdl_get_child_entry_bool_value!(kdl, "cursor_unfocused_hollow") {
            window.cursor_unfocused_hollow = Some(hollow);
        }
        if let Some(minimum_contrast) = kdl_get_child!(kdl, "minimum_contrast") {
            window.minimum_contrast = Some(minimum_contrast_from_kdl(minimum_contrast)?);
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
            ("fullscreen_keys", &self.fullscreen_keys),
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
        for (name, value) in [
            ("padding", self.padding),
            ("padding_top", self.padding_top),
            ("padding_right", self.padding_right),
            ("padding_bottom", self.padding_bottom),
            ("padding_left", self.padding_left),
        ] {
            if let Some(value) = value {
                children.nodes_mut().push(number_node(name, value));
            }
        }
        if let Some(padding_balance) = self.padding_balance {
            let mut padding_balance_node = KdlNode::new("padding_balance");
            padding_balance_node.push(KdlValue::Bool(padding_balance));
            children.nodes_mut().push(padding_balance_node);
        }
        if let Some(padding_color) = self.padding_color {
            children.nodes_mut().push(padding_color.to_kdl());
        }
        for (name, value) in [
            ("line_height", self.line_height),
            ("cell_width", self.cell_width),
        ] {
            if let Some(value) = value {
                children.nodes_mut().push(number_node(name, value));
            }
        }
        for (name, value) in [
            ("baseline_offset", self.baseline_offset),
            ("underline_offset", self.underline_offset),
            ("underline_thickness", self.underline_thickness),
        ] {
            if let Some(value) = value {
                let mut adjustment_node = KdlNode::new(name);
                adjustment_node.push(KdlValue::Base10(value as i64));
                children.nodes_mut().push(adjustment_node);
            }
        }
        for (name, value) in [
            ("initial_columns", self.initial_columns),
            ("initial_rows", self.initial_rows),
        ] {
            if let Some(value) = value {
                let mut cells_node = KdlNode::new(name);
                cells_node.push(KdlValue::Base10(value as i64));
                children.nodes_mut().push(cells_node);
            }
        }
        if let Some(font_weight) = self.font_weight {
            let mut font_weight_node = KdlNode::new("font_weight");
            font_weight_node.push(KdlValue::Base10(font_weight as i64));
            children.nodes_mut().push(font_weight_node);
        }
        if let Some(font_features) = &self.font_features {
            let mut font_features_node = KdlNode::new("font_features");
            for feature in font_features {
                font_features_node.push(KdlValue::String(feature.clone()));
            }
            children.nodes_mut().push(font_features_node);
        }
        for (name, value) in [
            ("confirm_close", self.confirm_close),
            ("hide_pointer_while_typing", self.hide_pointer_while_typing),
            ("cursor_unfocused_hollow", self.cursor_unfocused_hollow),
        ] {
            if let Some(value) = value {
                let mut flag_node = KdlNode::new(name);
                flag_node.push(KdlValue::Bool(value));
                children.nodes_mut().push(flag_node);
            }
        }
        if let Some(minimum_contrast) = self.minimum_contrast {
            children
                .nodes_mut()
                .push(number_node("minimum_contrast", minimum_contrast));
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
        merged.padding = other.padding.or(merged.padding);
        merged.padding_top = other.padding_top.or(merged.padding_top);
        merged.padding_right = other.padding_right.or(merged.padding_right);
        merged.padding_bottom = other.padding_bottom.or(merged.padding_bottom);
        merged.padding_left = other.padding_left.or(merged.padding_left);
        merged.padding_balance = other.padding_balance.or(merged.padding_balance);
        merged.padding_color = other.padding_color.or(merged.padding_color);
        merged.line_height = other.line_height.or(merged.line_height);
        merged.cell_width = other.cell_width.or(merged.cell_width);
        merged.baseline_offset = other.baseline_offset.or(merged.baseline_offset);
        merged.underline_offset = other.underline_offset.or(merged.underline_offset);
        merged.underline_thickness = other.underline_thickness.or(merged.underline_thickness);
        merged.initial_columns = other.initial_columns.or(merged.initial_columns);
        merged.initial_rows = other.initial_rows.or(merged.initial_rows);
        merged.font_weight = other.font_weight.or(merged.font_weight);
        merged.font_features = other.font_features.or(merged.font_features);
        merged.fullscreen_keys = other.fullscreen_keys.or(merged.fullscreen_keys);
        merged.confirm_close = other.confirm_close.or(merged.confirm_close);
        merged.hide_pointer_while_typing = other
            .hide_pointer_while_typing
            .or(merged.hide_pointer_while_typing);
        merged.cursor_unfocused_hollow = other
            .cursor_unfocused_hollow
            .or(merged.cursor_unfocused_hollow);
        merged.minimum_contrast = other.minimum_contrast.or(merged.minimum_contrast);
        merged
    }
}

fn number_node(name: &str, value: f32) -> KdlNode {
    let mut node = KdlNode::new(name);
    if value.fract() == 0.0 {
        node.push(KdlValue::Base10(value as i64));
    } else {
        node.push(KdlValue::Base10Float(value as f64));
    }
    node
}

fn number_from_kdl(node: &KdlNode, name: &str) -> Result<f64, ConfigError> {
    let error = |message: String| {
        ConfigError::new_kdl_error(message, node.span().offset(), node.span().len())
    };
    let value = node
        .entries()
        .iter()
        .next()
        .map(|entry| entry.value())
        .ok_or_else(|| error(format!("{} needs a value", name)))?;
    let number = match value {
        KdlValue::Base10Float(number) => *number,
        other => other
            .as_i64()
            .map(|number| number as f64)
            .ok_or_else(|| error(format!("{} must be a number", name)))?,
    };
    if !number.is_finite() {
        return Err(error(format!("{} must be a finite number", name)));
    }
    Ok(number)
}

fn integer_from_kdl(node: &KdlNode, name: &str) -> Result<i64, ConfigError> {
    let error = |message: String| {
        ConfigError::new_kdl_error(message, node.span().offset(), node.span().len())
    };
    let value = node
        .entries()
        .iter()
        .next()
        .map(|entry| entry.value())
        .ok_or_else(|| error(format!("{} needs a value", name)))?;
    value
        .as_i64()
        .ok_or_else(|| error(format!("{} must be a whole number", name)))
}

fn padding_from_kdl(node: &KdlNode, name: &str) -> Result<f32, ConfigError> {
    let padding = number_from_kdl(node, name)?;
    if !(0.0..=MAX_PADDING as f64).contains(&padding) {
        return Err(ConfigError::new_kdl_error(
            format!(
                "{} {} is outside the range 0 to {} pixels",
                name, padding, MAX_PADDING
            ),
            node.span().offset(),
            node.span().len(),
        ));
    }
    Ok(padding as f32)
}

fn cell_scale_from_kdl(node: &KdlNode, name: &str) -> Result<f32, ConfigError> {
    let scale = number_from_kdl(node, name)?;
    if !(MIN_CELL_SCALE as f64..=MAX_CELL_SCALE as f64).contains(&scale) {
        return Err(ConfigError::new_kdl_error(
            format!(
                "{} {} is outside the range {} to {} (a multiple of the font's own cell)",
                name, scale, MIN_CELL_SCALE, MAX_CELL_SCALE
            ),
            node.span().offset(),
            node.span().len(),
        ));
    }
    Ok(scale as f32)
}

fn pixel_adjustment_from_kdl(node: &KdlNode, name: &str) -> Result<i32, ConfigError> {
    let adjustment = integer_from_kdl(node, name)?;
    let limit = MAX_PIXEL_ADJUSTMENT as i64;
    if !(-limit..=limit).contains(&adjustment) {
        return Err(ConfigError::new_kdl_error(
            format!(
                "{} {} is outside the range -{} to {} pixels",
                name, adjustment, limit, limit
            ),
            node.span().offset(),
            node.span().len(),
        ));
    }
    Ok(adjustment as i32)
}

fn initial_cells_from_kdl(node: &KdlNode, name: &str) -> Result<u16, ConfigError> {
    let cells = integer_from_kdl(node, name)?;
    if !(MIN_INITIAL_CELLS as i64..=MAX_INITIAL_CELLS as i64).contains(&cells) {
        return Err(ConfigError::new_kdl_error(
            format!(
                "{} {} is outside the range {} to {}",
                name, cells, MIN_INITIAL_CELLS, MAX_INITIAL_CELLS
            ),
            node.span().offset(),
            node.span().len(),
        ));
    }
    Ok(cells as u16)
}

fn font_weight_from_kdl(node: &KdlNode) -> Result<u16, ConfigError> {
    let error = |message: String| {
        ConfigError::new_kdl_error(message, node.span().offset(), node.span().len())
    };
    let names = || {
        FONT_WEIGHT_NAMES
            .iter()
            .map(|(name, _)| format!("\"{}\"", name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let value = node
        .entries()
        .iter()
        .next()
        .map(|entry| entry.value())
        .ok_or_else(|| error("font_weight needs a value".to_owned()))?;
    if let Some(name) = value.as_string() {
        let lowered = name.to_ascii_lowercase().replace(['-', '_', ' '], "");
        return FONT_WEIGHT_NAMES
            .iter()
            .find(|(known, _)| *known == lowered)
            .map(|(_, weight)| *weight)
            .ok_or_else(|| {
                error(format!(
                    "font_weight {:?} is not a weight; use a number from 100 to 900 or one of {}",
                    name,
                    names()
                ))
            });
    }
    let weight = value.as_i64().ok_or_else(|| {
        error(format!(
            "font_weight must be a number from 100 to 900 or one of {}",
            names()
        ))
    })?;
    if !(100..=900).contains(&weight) {
        return Err(error(format!(
            "font_weight {} is outside the range 100 to 900",
            weight
        )));
    }
    Ok(weight as u16)
}

pub fn is_font_feature(text: &str) -> bool {
    let tag = text.strip_prefix(['+', '-']).unwrap_or(text);
    tag.len() == 4 && tag.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

pub const MIN_CONTRAST: f32 = 1.0;
pub const MAX_CONTRAST: f32 = 21.0;

fn minimum_contrast_from_kdl(node: &KdlNode) -> Result<f32, ConfigError> {
    let contrast = number_from_kdl(node, "minimum_contrast")?;
    if !(MIN_CONTRAST as f64..=MAX_CONTRAST as f64).contains(&contrast) {
        return Err(ConfigError::new_kdl_error(
            format!(
                "minimum_contrast {} is outside the range {} to {}",
                contrast, MIN_CONTRAST, MAX_CONTRAST
            ),
            node.span().offset(),
            node.span().len(),
        ));
    }
    Ok(contrast as f32)
}

fn font_features_from_kdl(node: &KdlNode) -> Result<Vec<String>, ConfigError> {
    let mut features = Vec::new();
    for entry in node.entries() {
        let error = |message: String| {
            ConfigError::new_kdl_error(message, entry.span().offset(), entry.span().len())
        };
        let text = entry
            .value()
            .as_string()
            .ok_or_else(|| error("a font feature must be given as a string".to_owned()))?;
        if !is_font_feature(text) {
            return Err(error(format!(
                "{:?} is not a font feature; use a four-character OpenType tag such as \"ss01\", \
                 optionally prefixed with - to turn it off",
                text
            )));
        }
        features.push(text.to_owned());
    }
    Ok(features)
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
        other => other
            .as_i64()
            .map(|opacity| opacity as f64)
            .ok_or_else(|| error("opacity must be a number between 0.0 and 1.0".to_owned()))?,
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
            ("remember", StartupMode::Remember),
            ("Remember", StartupMode::Remember),
        ] {
            let parsed = section(&format!("window {{\n startup_mode \"{}\"\n}}", text)).unwrap();
            assert_eq!(parsed.startup_mode, Some(expected), "{}", text);
            let reparsed = section(&parsed.to_kdl().unwrap().to_string()).unwrap();
            assert_eq!(reparsed.startup_mode, Some(expected), "{}", text);
        }
        let message = format!(
            "{:?}",
            section("window {\n startup_mode \"minimized\"\n}").unwrap_err()
        );
        for valid in ["windowed", "maximized", "fullscreen", "remember"] {
            assert!(
                message.contains(valid),
                "{} missing from {}",
                valid,
                message
            );
        }
        assert!(section("window {\n startup_mode\n}").is_err());
        assert_eq!(section("window {\n}").unwrap().startup_mode, None);
        assert_eq!(StartupMode::default(), StartupMode::Remember);
    }

    #[test]
    fn remember_round_trips_through_kdl() {
        let parsed = section("window {\n startup_mode \"remember\"\n}").unwrap();
        let emitted = parsed.to_kdl().unwrap().to_string();
        assert!(emitted.contains("startup_mode \"remember\""), "{}", emitted);
        assert_eq!(section(&emitted).unwrap(), parsed);
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
            assert!(
                format!("{:?}", err).contains("opacity"),
                "{}: {:?}",
                text,
                err
            );
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
            section("window {\n opacity 0.5\n opacity_mode \"everything\"\n blur true\n}").unwrap();
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

    #[test]
    fn every_layout_option_round_trips_through_kdl() {
        let parsed = section(
            r##"
            window {
                padding 8
                padding_top 4.5
                padding_right 12
                padding_bottom 0
                padding_left 6
                padding_balance true
                padding_color "extend"
                line_height 1.25
                cell_width 1
                baseline_offset -2
                underline_offset 1
                underline_thickness 2
                initial_columns 100
                initial_rows 30
                font_weight "medium"
                font_features "ss01" "zero" "-calt" "+dlig"
            }
            "##,
        )
        .unwrap();

        assert_eq!(parsed.padding, Some(8.0));
        assert_eq!(parsed.padding_top, Some(4.5));
        assert_eq!(parsed.padding_right, Some(12.0));
        assert_eq!(parsed.padding_bottom, Some(0.0));
        assert_eq!(parsed.padding_left, Some(6.0));
        assert_eq!(parsed.padding_balance, Some(true));
        assert_eq!(parsed.padding_color, Some(PaddingColor::Extend));
        assert_eq!(parsed.line_height, Some(1.25));
        assert_eq!(parsed.cell_width, Some(1.0));
        assert_eq!(parsed.baseline_offset, Some(-2));
        assert_eq!(parsed.underline_offset, Some(1));
        assert_eq!(parsed.underline_thickness, Some(2));
        assert_eq!(parsed.initial_columns, Some(100));
        assert_eq!(parsed.initial_rows, Some(30));
        assert_eq!(parsed.font_weight, Some(500));
        assert_eq!(
            parsed.font_features,
            Some(vec![
                "ss01".to_owned(),
                "zero".to_owned(),
                "-calt".to_owned(),
                "+dlig".to_owned()
            ])
        );

        let emitted = parsed.to_kdl().unwrap().to_string();
        assert_eq!(section(&emitted).unwrap(), parsed, "{}", emitted);
    }

    #[test]
    fn layout_options_are_unset_in_an_empty_section() {
        let empty = section("window {\n}").unwrap();
        assert_eq!(empty.padding, None);
        assert_eq!(empty.padding_balance, None);
        assert_eq!(empty.padding_color, None);
        assert_eq!(empty.line_height, None);
        assert_eq!(empty.cell_width, None);
        assert_eq!(empty.font_weight, None);
        assert_eq!(empty.font_features, None);
        assert_eq!(PaddingColor::default(), PaddingColor::Background);
    }

    #[test]
    fn padding_must_not_be_negative() {
        for name in [
            "padding",
            "padding_top",
            "padding_right",
            "padding_bottom",
            "padding_left",
        ] {
            let err = section(&format!("window {{\n {} -1\n}}", name)).unwrap_err();
            assert!(format!("{:?}", err).contains(name), "{}: {:?}", name, err);
            assert!(section(&format!("window {{\n {} \"wide\"\n}}", name)).is_err());
            assert!(section(&format!("window {{\n {}\n}}", name)).is_err());
        }
    }

    #[test]
    fn line_height_and_cell_width_refuse_multipliers_that_collapse_the_cell() {
        for name in ["line_height", "cell_width"] {
            for text in ["0", "0.2", "-1", "4", "\"tall\""] {
                let err = section(&format!("window {{\n {} {}\n}}", name, text)).unwrap_err();
                assert!(
                    format!("{:?}", err).contains(name),
                    "{} {}: {:?}",
                    name,
                    text,
                    err
                );
            }
            for text in ["0.5", "1", "1.2", "3"] {
                assert!(
                    section(&format!("window {{\n {} {}\n}}", name, text)).is_ok(),
                    "{} {}",
                    name,
                    text
                );
            }
        }
    }

    #[test]
    fn pixel_adjustments_are_whole_numbers_within_bounds() {
        for name in ["baseline_offset", "underline_offset", "underline_thickness"] {
            assert!(section(&format!("window {{\n {} 1.5\n}}", name)).is_err());
            assert!(section(&format!("window {{\n {} 65\n}}", name)).is_err());
            assert!(section(&format!("window {{\n {} -65\n}}", name)).is_err());
            assert!(section(&format!("window {{\n {} -3\n}}", name)).is_ok());
        }
    }

    #[test]
    fn initial_cells_share_the_saved_state_bounds() {
        assert!(section("window {\n initial_columns 1\n}").is_err());
        assert!(section("window {\n initial_rows 1001\n}").is_err());
        assert!(section("window {\n initial_rows 24.5\n}").is_err());
        assert_eq!(
            section("window {\n initial_rows 24\n}")
                .unwrap()
                .initial_rows,
            Some(24)
        );
    }

    #[test]
    fn font_weight_accepts_numbers_and_names_and_refuses_the_rest() {
        for (text, expected) in [
            ("100", 100),
            ("350", 350),
            ("900", 900),
            ("\"thin\"", 100),
            ("\"Light\"", 300),
            ("\"regular\"", 400),
            ("\"normal\"", 400),
            ("\"medium\"", 500),
            ("\"semi-bold\"", 600),
            ("\"bold\"", 700),
            ("\"extra_bold\"", 800),
            ("\"black\"", 900),
        ] {
            let parsed = section(&format!("window {{\n font_weight {}\n}}", text)).unwrap();
            assert_eq!(parsed.font_weight, Some(expected), "{}", text);
        }
        let message = format!(
            "{:?}",
            section("window {\n font_weight \"chunky\"\n}").unwrap_err()
        );
        assert!(
            message.contains("font_weight") && message.contains("medium"),
            "{}",
            message
        );
        assert!(section("window {\n font_weight 50\n}").is_err());
        assert!(section("window {\n font_weight 1000\n}").is_err());
        assert!(section("window {\n font_weight\n}").is_err());
    }

    #[test]
    fn a_malformed_font_feature_is_refused() {
        for text in [
            "\"ss1\"",
            "\"ss011\"",
            "\"--calt\"",
            "\"\"",
            "4",
            "\"ss 1\"",
        ] {
            let err = section(&format!("window {{\n font_features {}\n}}", text)).unwrap_err();
            assert!(
                format!("{:?}", err).contains("font feature"),
                "{}: {:?}",
                text,
                err
            );
        }
        assert_eq!(
            section("window {\n font_features\n}")
                .unwrap()
                .font_features,
            Some(Vec::new())
        );
    }

    #[test]
    fn an_unknown_padding_color_lists_the_valid_ones() {
        let message = format!(
            "{:?}",
            section("window {\n padding_color \"mirror\"\n}").unwrap_err()
        );
        assert!(
            message.contains("background") && message.contains("extend"),
            "{}",
            message
        );
    }

    #[test]
    fn every_ui_option_round_trips_through_kdl() {
        let parsed = section(
            "window {\n fullscreen_keys \"F11\" \"Alt Enter\"\n confirm_close false\n \
             hide_pointer_while_typing true\n cursor_unfocused_hollow false\n \
             minimum_contrast 4.5\n}",
        )
        .unwrap();
        assert_eq!(
            parsed.fullscreen_keys,
            Some(vec![
                KeyWithModifier::new(BareKey::F(11)),
                KeyWithModifier::new(BareKey::Enter).with_alt_modifier(),
            ])
        );
        assert_eq!(parsed.confirm_close, Some(false));
        assert_eq!(parsed.hide_pointer_while_typing, Some(true));
        assert_eq!(parsed.cursor_unfocused_hollow, Some(false));
        assert_eq!(parsed.minimum_contrast, Some(4.5));
        let emitted = parsed.to_kdl().unwrap().to_string();
        assert_eq!(section(&emitted).unwrap(), parsed, "{}", emitted);

        let empty = section("window {\n}").unwrap();
        assert_eq!(empty.fullscreen_keys, None);
        assert_eq!(empty.confirm_close, None);
        assert_eq!(empty.hide_pointer_while_typing, None);
        assert_eq!(empty.cursor_unfocused_hollow, None);
        assert_eq!(empty.minimum_contrast, None);
    }

    #[test]
    fn minimum_contrast_is_a_ratio_from_one_to_twenty_one() {
        for text in ["1", "7", "21", "1.5"] {
            assert!(
                section(&format!("window {{\n minimum_contrast {}\n}}", text)).is_ok(),
                "{}",
                text
            );
        }
        for text in ["0.9", "0", "-3", "22", "\"high\"", ""] {
            let err = section(&format!("window {{\n minimum_contrast {}\n}}", text)).unwrap_err();
            assert!(
                format!("{:?}", err).contains("minimum_contrast"),
                "{}: {:?}",
                text,
                err
            );
        }
    }

    #[test]
    fn a_fullscreen_chord_that_does_not_parse_is_a_config_error() {
        assert!(section("window {\n fullscreen_keys \"Ctrl nonsense\"\n}").is_err());
        assert_eq!(
            section("window {\n fullscreen_keys\n}")
                .unwrap()
                .fullscreen_keys,
            Some(Vec::new())
        );
    }

    #[test]
    fn a_later_section_supersedes_layout_options_field_by_field() {
        let first = section("window {\n padding 4\n line_height 1.5\n}").unwrap();
        let second = section("window {\n padding_left 9\n line_height 1.1\n}").unwrap();
        let merged = first.merge(second);
        assert_eq!(merged.padding, Some(4.0));
        assert_eq!(merged.padding_left, Some(9.0));
        assert_eq!(merged.line_height, Some(1.1));
    }
}

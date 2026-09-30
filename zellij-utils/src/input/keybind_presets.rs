use super::actions::Action;
use super::config::{Config, ConfigError};
use super::keybinds::Keybinds;
use super::options::Options;
use crate::data::{
    InputMode, KeyWithModifier, KeybindPresetInfo, KeybindPresetSource, KeybindPresetWithError,
    KeybindsSelectionSnapshot,
};
use crate::home::{find_default_config_dir, get_keybinds_dir};
use kdl::{KdlDocument, KdlEntry, KdlNode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

pub const DEFAULT_KEYBIND_PRESET: &str = "default";
pub const LEADER_PLACEHOLDERS: [&str; 3] = ["primary", "secondary", "unlock"];

pub const DEFAULT_KEYBIND_PRESET_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/",
    "assets/keybinds/default.kdl"
));

pub const UNLOCK_FIRST_KEYBIND_PRESET_ASSET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/",
    "assets/keybinds/unlock-first.kdl"
));

pub const BUILTIN_KEYBIND_PRESETS: [(&str, &str); 2] = [
    (DEFAULT_KEYBIND_PRESET, DEFAULT_KEYBIND_PRESET_ASSET),
    ("unlock-first", UNLOCK_FIRST_KEYBIND_PRESET_ASSET),
];

pub fn builtin_keybind_preset_text(name: &str) -> Option<&'static str> {
    crate::distribution::builtin_keybind_preset(name)
        .map(|preset| preset.preset)
        .or_else(|| {
            BUILTIN_KEYBIND_PRESETS
                .iter()
                .find(|(builtin_name, _)| *builtin_name == name)
                .map(|(_, text)| *text)
        })
}

pub fn builtin_keybind_preset_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = BUILTIN_KEYBIND_PRESETS
        .iter()
        .map(|(name, _)| *name)
        .collect();
    for preset in crate::distribution::builtin_keybind_presets() {
        if !names.contains(&preset.name) {
            names.push(preset.name);
        }
    }
    names
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ModeKeybindChanges {
    pub clear_defaults: bool,
    pub bind: BTreeMap<KeyWithModifier, Vec<Action>>,
    pub unbind: BTreeSet<KeyWithModifier>,
}

impl ModeKeybindChanges {
    pub fn is_empty(&self) -> bool {
        !self.clear_defaults && self.bind.is_empty() && self.unbind.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct KeybindChanges {
    pub clear_defaults: bool,
    pub modes: BTreeMap<InputMode, ModeKeybindChanges>,
}

impl KeybindChanges {
    pub fn is_empty(&self) -> bool {
        !self.clear_defaults && self.modes.values().all(|mode| mode.is_empty())
    }
    pub fn has_keybindings(&self) -> bool {
        self.modes.values().any(|mode| !mode.is_empty())
    }
    fn mode_mut(&mut self, mode: InputMode) -> &mut ModeKeybindChanges {
        self.modes.entry(mode).or_default()
    }
    pub fn clear_all(&mut self) {
        *self = KeybindChanges {
            clear_defaults: true,
            modes: BTreeMap::new(),
        };
    }
    pub fn clear_mode(&mut self, mode: InputMode) {
        self.modes.insert(
            mode,
            ModeKeybindChanges {
                clear_defaults: true,
                ..Default::default()
            },
        );
    }
    pub fn bind(&mut self, mode: InputMode, key: KeyWithModifier, actions: Vec<Action>) {
        let mode_changes = self.mode_mut(mode);
        mode_changes.unbind.remove(&key);
        mode_changes.bind.insert(key, actions);
    }
    pub fn unbind(&mut self, mode: InputMode, key: KeyWithModifier) {
        let mode_changes = self.mode_mut(mode);
        mode_changes.bind.remove(&key);
        mode_changes.unbind.insert(key);
    }
    pub fn compose(&mut self, other: KeybindChanges) {
        if other.clear_defaults {
            *self = other;
            return;
        }
        for (mode, other_mode_changes) in other.modes {
            if other_mode_changes.clear_defaults {
                self.modes.insert(mode, other_mode_changes);
                continue;
            }
            for key in other_mode_changes.unbind {
                self.unbind(mode, key);
            }
            for (key, actions) in other_mode_changes.bind {
                self.bind(mode, key, actions);
            }
        }
    }
    pub fn apply(&self, keybinds: &mut Keybinds) {
        if self.clear_defaults {
            keybinds.0.clear();
        }
        for (mode, mode_changes) in &self.modes {
            if mode_changes.clear_defaults || !mode_changes.bind.is_empty() {
                let keys_in_mode = keybinds.get_input_mode_mut(mode);
                if mode_changes.clear_defaults {
                    keys_in_mode.clear();
                }
                for key in &mode_changes.unbind {
                    keys_in_mode.remove(key);
                }
                for (key, actions) in &mode_changes.bind {
                    keys_in_mode.insert(key.clone(), actions.clone());
                }
            } else if let Some(keys_in_mode) = keybinds.0.get_mut(mode) {
                for key in &mode_changes.unbind {
                    keys_in_mode.remove(key);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct KeybindsSelection {
    pub preset: Option<String>,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub unlock: Option<String>,
    pub changes: KeybindChanges,
}

impl KeybindsSelection {
    pub fn is_empty(&self) -> bool {
        self.preset.is_none()
            && self.primary.is_none()
            && self.secondary.is_none()
            && self.unlock.is_none()
            && self.changes.is_empty()
    }
    pub fn leader(&self, placeholder: &str) -> Option<&String> {
        match placeholder {
            "primary" => self.primary.as_ref(),
            "secondary" => self.secondary.as_ref(),
            "unlock" => self.unlock.as_ref(),
            _ => None,
        }
    }
    pub fn set_leader(&mut self, placeholder: &str, value: Option<String>) {
        match placeholder {
            "primary" => self.primary = value,
            "secondary" => self.secondary = value,
            "unlock" => self.unlock = value,
            _ => {},
        }
    }
    pub fn effective_preset(&self) -> &str {
        self.preset.as_deref().unwrap_or(DEFAULT_KEYBIND_PRESET)
    }
    pub fn merge(&mut self, other: KeybindsSelection) {
        if let Some(preset) = other.preset.clone() {
            if self.effective_preset() != preset {
                for placeholder in LEADER_PLACEHOLDERS {
                    self.set_leader(placeholder, None);
                }
            }
            self.preset = Some(preset);
        }
        for placeholder in LEADER_PLACEHOLDERS {
            if let Some(value) = other.leader(placeholder).cloned() {
                self.set_leader(placeholder, Some(value));
            }
        }
        self.changes.compose(other.changes);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeybindsLayer {
    User,
    Layout,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ActiveKeybindPreset {
    pub info: KeybindPresetInfo,
    pub values: BTreeMap<String, String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct KeybindsLayers {
    pub user: KeybindsSelection,
    pub layout: KeybindsSelection,
    pub config_dir: Option<PathBuf>,
    pub keybinds_dir_override: Option<PathBuf>,
    pub injected_default_mode: Option<InputMode>,
    pub config_default_mode: Option<InputMode>,
    pub active: ActiveKeybindPreset,
}

impl KeybindsLayers {
    pub fn layer_mut(&mut self, layer: KeybindsLayer) -> &mut KeybindsSelection {
        match layer {
            KeybindsLayer::User => &mut self.user,
            KeybindsLayer::Layout => &mut self.layout,
        }
    }
    pub fn effective_preset(&self) -> String {
        self.layout
            .preset
            .clone()
            .unwrap_or_else(|| self.user.effective_preset().to_owned())
    }
    pub fn requested_leader_values(&self, preset: &str) -> BTreeMap<String, String> {
        let user_values_apply = self.user.effective_preset() == preset;
        let layout_values_apply = self
            .layout
            .preset
            .as_deref()
            .unwrap_or_else(|| self.user.effective_preset())
            == preset;
        let mut values = BTreeMap::new();
        for placeholder in LEADER_PLACEHOLDERS {
            let layout_value = if layout_values_apply {
                self.layout.leader(placeholder).cloned()
            } else {
                None
            };
            let value = layout_value.or_else(|| {
                if user_values_apply {
                    self.user.leader(placeholder).cloned()
                } else {
                    None
                }
            });
            if let Some(value) = value {
                values.insert(placeholder.to_owned(), value);
            }
        }
        values
    }
}

#[derive(Debug, Clone)]
pub struct KeybindPreset {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub examples: Vec<(String, String)>,
    pub default_mode: Option<InputMode>,
    pub defaults: BTreeMap<String, String>,
    pub placeholders: BTreeSet<String>,
    pub keybinds: KdlNode,
}

fn first_string_argument(node: &KdlNode) -> Option<String> {
    node.entries()
        .iter()
        .find(|entry| entry.name().is_none())
        .and_then(|entry| entry.value().as_string())
        .map(|value| value.to_owned())
}

pub fn placeholders_in(text: &str) -> Vec<String> {
    let mut found = vec![];
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after_start = &rest[start + 1..];
        match after_start.find('}') {
            Some(end) => {
                found.push(after_start[..end].to_owned());
                rest = &after_start[end + 1..];
            },
            None => break,
        }
    }
    found
}

fn collect_placeholders(node: &KdlNode, found: &mut BTreeSet<String>) -> Result<(), String> {
    let name = node.name().value();
    if name == "bind" || name == "unbind" {
        for entry in node.entries() {
            if entry.name().is_some() {
                continue;
            }
            if let Some(text) = entry.value().as_string() {
                for placeholder in placeholders_in(text) {
                    if !LEADER_PLACEHOLDERS.contains(&placeholder.as_str()) {
                        return Err(format!(
                            "Unknown placeholder '{{{}}}' in key '{}'; the known placeholders are {{primary}}, {{secondary}} and {{unlock}}",
                            placeholder, text
                        ));
                    }
                    found.insert(placeholder);
                }
            }
        }
        return Ok(());
    }
    if let Some(children) = node.children() {
        for child in children.nodes() {
            collect_placeholders(child, found)?;
        }
    }
    Ok(())
}

pub fn config_error_text(error: &ConfigError) -> String {
    match error {
        ConfigError::KdlError(kdl_error) => kdl_error.error_message.clone(),
        ConfigError::KdlDeserializationError(kdl_error) => kdl_error
            .help
            .map(|help| help.to_owned())
            .unwrap_or_else(|| format!("{}", kdl_error)),
        other => format!("{}", other),
    }
}

impl KeybindPreset {
    pub fn from_kdl(text: &str) -> Result<KeybindPreset, String> {
        let document: KdlDocument = text
            .parse()
            .map_err(|e| config_error_text(&ConfigError::KdlDeserializationError(e)))?;
        let mut display_name = None;
        let mut description = None;
        let mut examples = vec![];
        let mut default_mode = None;
        let mut defaults = BTreeMap::new();
        if let Some(header) = document.get("preset").and_then(|node| node.children()) {
            display_name = header.get("name").and_then(first_string_argument);
            description = header.get("description").and_then(first_string_argument);
            for node in header
                .nodes()
                .iter()
                .filter(|n| n.name().value() == "example")
            {
                let arguments: Vec<String> = node
                    .entries()
                    .iter()
                    .filter(|entry| entry.name().is_none())
                    .filter_map(|entry| entry.value().as_string().map(|s| s.to_owned()))
                    .collect();
                match arguments.as_slice() {
                    [keys, text] => examples.push((keys.clone(), text.clone())),
                    _ => {
                        return Err(
                            "An example in the preset header needs two strings: the keys and what they do"
                                .to_owned(),
                        )
                    },
                }
            }
            if let Some(mode) = header.get("default_mode").and_then(first_string_argument) {
                default_mode = Some(
                    InputMode::from_str(&mode)
                        .map_err(|_| format!("Invalid default_mode in the preset: '{}'", mode))?,
                );
            }
            if let Some(default_values) = header.get("defaults").and_then(|node| node.children()) {
                for node in default_values.nodes() {
                    let name = node.name().value();
                    if !LEADER_PLACEHOLDERS.contains(&name) {
                        return Err(format!(
                            "Unknown default '{}' in the preset header; the known values are primary, secondary and unlock",
                            name
                        ));
                    }
                    let value = first_string_argument(node)
                        .ok_or_else(|| format!("The default for '{}' must be a string", name))?;
                    defaults.insert(name.to_owned(), value);
                }
            }
        }
        let keybinds = document
            .get("keybinds")
            .cloned()
            .ok_or_else(|| "The preset has no keybinds block".to_owned())?;
        let mut placeholders = BTreeSet::new();
        collect_placeholders(&keybinds, &mut placeholders)?;
        Ok(KeybindPreset {
            display_name,
            description,
            examples,
            default_mode,
            defaults,
            placeholders,
            keybinds,
        })
    }
    pub fn leader_values(
        &self,
        requested: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut values = BTreeMap::new();
        for placeholder in &self.placeholders {
            let value = requested
                .get(placeholder)
                .or_else(|| self.defaults.get(placeholder))
                .ok_or_else(|| {
                    format!(
                        "The preset uses {{{}}}, but no value was given for it",
                        placeholder
                    )
                })?;
            let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
            if value.is_empty() {
                return Err(format!(
                    "The value for {} is empty, but the preset uses {{{}}}",
                    placeholder, placeholder
                ));
            }
            values.insert(placeholder.clone(), value);
        }
        if let (Some(primary), Some(secondary)) = (values.get("primary"), values.get("secondary")) {
            let modifier_set = |value: &str| -> BTreeSet<String> {
                value
                    .split_whitespace()
                    .map(|part| part.to_lowercase())
                    .collect()
            };
            if modifier_set(primary) == modifier_set(secondary) {
                return Err(format!(
                    "The primary and secondary values are both '{}'; they must differ",
                    primary
                ));
            }
        }
        Ok(values)
    }
    pub fn info(
        &self,
        name: &str,
        source: KeybindPresetSource,
        path: Option<&Path>,
    ) -> KeybindPresetInfo {
        KeybindPresetInfo {
            name: name.to_owned(),
            display_name: self.display_name.clone().unwrap_or_else(|| name.to_owned()),
            description: self.description.clone(),
            examples: self.examples.clone(),
            source,
            placeholders: self.placeholders.iter().cloned().collect(),
            path: path.map(|path| path.display().to_string()),
        }
    }
    pub fn keybinds(
        &self,
        values: &BTreeMap<String, String>,
        options: &Options,
    ) -> Result<Keybinds, String> {
        let substitute = |key: &str| -> Result<String, String> {
            let mut substituted = key.to_owned();
            for placeholder in placeholders_in(key) {
                let value = values
                    .get(&placeholder)
                    .ok_or_else(|| format!("No value for the placeholder {{{}}}", placeholder))?;
                substituted = substituted.replace(&format!("{{{}}}", placeholder), value);
            }
            Ok(substituted.split_whitespace().collect::<Vec<_>>().join(" "))
        };
        let changes = KeybindChanges::from_kdl_with_key_text(&self.keybinds, options, &substitute)
            .map_err(|e| config_error_text(&e))?;
        let mut keybinds = Keybinds::default();
        changes.apply(&mut keybinds);
        Ok(keybinds)
    }
}

pub fn command_line_preset_relative_to(options: &mut Options, cwd: &Path) {
    if let Some(preset) = options.keybinds_preset.as_mut() {
        if is_keybind_preset_path(preset) && Path::new(preset.as_str()).is_relative() {
            *preset = cwd.join(preset.as_str()).display().to_string();
        }
    }
}

pub fn is_keybind_preset_path(value: &str) -> bool {
    let path = Path::new(value);
    path.extension().is_some() || path.components().count() > 1
}

pub fn find_keybind_preset(
    value: &str,
    keybinds_dir: Option<&Path>,
    config_dir: Option<&Path>,
) -> Result<(String, KeybindPresetSource, Option<PathBuf>), String> {
    if is_keybind_preset_path(value) {
        let path = PathBuf::from(value);
        let path = if path.is_absolute() {
            path
        } else {
            match config_dir {
                Some(config_dir) => config_dir.join(path),
                None => path,
            }
        };
        return std::fs::read_to_string(&path)
            .map(|text| (text, KeybindPresetSource::File, Some(path.clone())))
            .map_err(|e| {
                format!(
                    "Could not read the keybinding preset file {}: {}",
                    path.display(),
                    e
                )
            });
    }
    if let Some(keybinds_dir) = keybinds_dir {
        let path = keybinds_dir.join(format!("{}.kdl", value));
        if path.is_file() {
            return std::fs::read_to_string(&path)
                .map(|text| (text, KeybindPresetSource::Folder, Some(path.clone())))
                .map_err(|e| {
                    format!(
                        "Could not read the keybinding preset file {}: {}",
                        path.display(),
                        e
                    )
                });
        }
    }
    builtin_keybind_preset_text(value)
        .map(|text| (text.to_owned(), KeybindPresetSource::BuiltIn, None))
        .ok_or_else(|| format!("The keybinding preset '{}' was not found", value))
}

#[derive(Debug, Clone)]
pub struct ResolvedKeybindPreset {
    pub keybinds: Keybinds,
    pub default_mode: Option<InputMode>,
    pub active: ActiveKeybindPreset,
}

pub fn resolve_keybind_preset(
    value: &str,
    requested_values: &BTreeMap<String, String>,
    keybinds_dir: Option<&Path>,
    config_dir: Option<&Path>,
    options: &Options,
) -> Result<ResolvedKeybindPreset, (ActiveKeybindPreset, String)> {
    let fallback_info = |source: KeybindPresetSource| KeybindPresetInfo {
        name: value.to_owned(),
        display_name: value.to_owned(),
        source,
        ..Default::default()
    };
    let source_for_value = if is_keybind_preset_path(value) {
        KeybindPresetSource::File
    } else {
        KeybindPresetSource::BuiltIn
    };
    let (text, source, path) =
        find_keybind_preset(value, keybinds_dir, config_dir).map_err(|e| {
            (
                ActiveKeybindPreset {
                    info: fallback_info(source_for_value),
                    error: Some(e.clone()),
                    ..Default::default()
                },
                e,
            )
        })?;
    let preset = KeybindPreset::from_kdl(&text).map_err(|e| {
        let mut info = fallback_info(source);
        info.path = path.as_ref().map(|path| path.display().to_string());
        (
            ActiveKeybindPreset {
                info,
                error: Some(e.clone()),
                ..Default::default()
            },
            e,
        )
    })?;
    let info = preset.info(value, source, path.as_deref());
    let failed = |e: String| {
        (
            ActiveKeybindPreset {
                info: info.clone(),
                values: BTreeMap::new(),
                error: Some(e.clone()),
            },
            e,
        )
    };
    let values = preset.leader_values(requested_values).map_err(failed)?;
    let keybinds = preset.keybinds(&values, options).map_err(failed)?;
    Ok(ResolvedKeybindPreset {
        keybinds,
        default_mode: preset.default_mode,
        active: ActiveKeybindPreset {
            info,
            values,
            error: None,
        },
    })
}

fn builtin_default_preset(options: &Options) -> ResolvedKeybindPreset {
    resolve_keybind_preset(
        DEFAULT_KEYBIND_PRESET,
        &BTreeMap::new(),
        None,
        None,
        options,
    )
    .unwrap_or_else(|(active, _)| ResolvedKeybindPreset {
        keybinds: Keybinds::default(),
        default_mode: None,
        active,
    })
}

fn take_folder_preset(
    name: &str,
    folder_presets: &mut BTreeMap<String, (PathBuf, Result<KeybindPreset, String>)>,
    presets: &mut Vec<KeybindPresetInfo>,
    errors: &mut Vec<KeybindPresetWithError>,
) -> bool {
    match folder_presets.remove(name) {
        Some((path, Ok(preset))) => {
            presets.push(preset.info(name, KeybindPresetSource::Folder, Some(&path)));
            true
        },
        Some((path, Err(error))) => {
            errors.push(KeybindPresetWithError {
                name: name.to_owned(),
                path: path.display().to_string(),
                error,
            });
            false
        },
        None => false,
    }
}

pub fn list_keybind_presets(
    keybinds_dir: Option<&Path>,
    extra_files: &[PathBuf],
) -> (Vec<KeybindPresetInfo>, Vec<KeybindPresetWithError>) {
    let mut presets = vec![];
    let mut errors = vec![];
    let mut folder_presets: BTreeMap<String, (PathBuf, Result<KeybindPreset, String>)> =
        BTreeMap::new();
    if let Some(entries) = keybinds_dir.and_then(|dir| std::fs::read_dir(dir).ok()) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("kdl") {
                continue;
            }
            let Some(name) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_owned())
            else {
                continue;
            };
            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| format!("{}", e))
                .and_then(|text| KeybindPreset::from_kdl(&text));
            folder_presets.insert(name, (path, parsed));
        }
    }
    for name in builtin_keybind_preset_names() {
        if take_folder_preset(name, &mut folder_presets, &mut presets, &mut errors) {
            continue;
        }
        if let Some(Ok(preset)) =
            builtin_keybind_preset_text(name).map(|text| KeybindPreset::from_kdl(text))
        {
            presets.push(preset.info(name, KeybindPresetSource::BuiltIn, None));
        }
    }
    let remaining: Vec<String> = folder_presets.keys().cloned().collect();
    for name in remaining {
        take_folder_preset(&name, &mut folder_presets, &mut presets, &mut errors);
    }
    for path in extra_files {
        let name = path.display().to_string();
        match std::fs::read_to_string(path)
            .map_err(|e| format!("{}", e))
            .and_then(|text| KeybindPreset::from_kdl(&text))
        {
            Ok(preset) => presets.push(preset.info(&name, KeybindPresetSource::File, Some(path))),
            Err(error) => errors.push(KeybindPresetWithError {
                name: name.clone(),
                path: name,
                error,
            }),
        }
    }
    (presets, errors)
}

pub fn is_usable_keybind_preset_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

pub fn copy_keybind_preset_to_folder(
    value: &str,
    new_name: &str,
    keybinds_dir: &Path,
    config_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    if !is_usable_keybind_preset_name(new_name) {
        return Err(format!(
            "'{}' cannot be used as a preset name; use letters, digits, '-' and '_'",
            new_name
        ));
    }
    let (text, _, _) = find_keybind_preset(value, Some(keybinds_dir), config_dir)?;
    let target = keybinds_dir.join(format!("{}.kdl", new_name));
    if target.exists() {
        return Err(format!("{} already exists", target.display()));
    }
    let mut document: KdlDocument = text
        .parse()
        .map_err(|e| config_error_text(&ConfigError::KdlDeserializationError(e)))?;
    if let Some(name_node) = document
        .get_mut("preset")
        .and_then(|header| header.children_mut().as_mut())
        .and_then(|header| header.get_mut("name"))
    {
        name_node.clear_entries();
        name_node.push(KdlEntry::new(new_name.to_owned()));
    }
    std::fs::create_dir_all(keybinds_dir)
        .map_err(|e| format!("Could not create {}: {}", keybinds_dir.display(), e))?;
    std::fs::write(&target, document.to_string())
        .map_err(|e| format!("Could not write {}: {}", target.display(), e))?;
    Ok(target)
}

impl Config {
    pub fn keybinds_config_dir(&self) -> Option<PathBuf> {
        self.keybinds_layers
            .config_dir
            .clone()
            .or_else(find_default_config_dir)
    }
    pub fn keybinds_dir(&self) -> Option<PathBuf> {
        self.keybinds_layers
            .keybinds_dir_override
            .clone()
            .or_else(|| self.options.keybinds_dir.clone())
            .or_else(|| get_keybinds_dir(self.keybinds_config_dir()))
    }
    pub fn active_keybind_preset_file(&self) -> Option<PathBuf> {
        self.keybinds_layers
            .active
            .info
            .path
            .as_ref()
            .map(PathBuf::from)
    }
    pub fn command_line_keybinds(&self) -> KeybindsSelection {
        KeybindsSelection {
            preset: self.options.keybinds_preset.clone(),
            primary: self.options.keybinds_primary.clone(),
            secondary: self.options.keybinds_secondary.clone(),
            unlock: self.options.keybinds_unlock.clone(),
            changes: KeybindChanges::default(),
        }
    }
    pub fn has_command_line_keybinds(&self) -> bool {
        !self.command_line_keybinds().is_empty()
    }
    pub fn set_command_line_keybinds(&mut self, options: &Options) {
        self.options.keybinds_preset = options.keybinds_preset.clone();
        self.options.keybinds_primary = options.keybinds_primary.clone();
        self.options.keybinds_secondary = options.keybinds_secondary.clone();
        self.options.keybinds_unlock = options.keybinds_unlock.clone();
    }
    pub fn clear_command_line_keybinds(&mut self) {
        self.set_command_line_keybinds(&Options::default());
    }
    pub fn apply_command_line_keybinds(&mut self, options: &Options) -> bool {
        let before = self.command_line_keybinds();
        self.set_command_line_keybinds(options);
        if self.command_line_keybinds() != before {
            self.resolve_keybinds();
            true
        } else {
            false
        }
    }
    pub fn resolve_keybinds(&mut self) {
        let command_line = self.command_line_keybinds();
        let preset_name = command_line
            .preset
            .clone()
            .unwrap_or_else(|| self.keybinds_layers.effective_preset());
        let mut requested_values = self.keybinds_layers.requested_leader_values(&preset_name);
        for placeholder in LEADER_PLACEHOLDERS {
            if let Some(value) = command_line.leader(placeholder) {
                requested_values.insert(placeholder.to_owned(), value.clone());
            }
        }
        let config_dir = self.keybinds_config_dir();
        let keybinds_dir = self.keybinds_dir();
        let resolved = match resolve_keybind_preset(
            &preset_name,
            &requested_values,
            keybinds_dir.as_deref(),
            config_dir.as_deref(),
            &self.options,
        ) {
            Ok(resolved) => resolved,
            Err((active, error)) => {
                log::warn!(
                    "Failed to load the keybinding preset '{}', using the default preset instead: {}",
                    preset_name,
                    error
                );
                let mut fallback = builtin_default_preset(&self.options);
                fallback.active = active;
                fallback
            },
        };
        let mut keybinds = resolved.keybinds;
        let preset_given_on_command_line = command_line.preset.is_some();
        for changes in [
            &self.keybinds_layers.user.changes,
            &self.keybinds_layers.layout.changes,
        ] {
            if !(preset_given_on_command_line && changes.clear_defaults) {
                changes.apply(&mut keybinds);
            }
        }
        if self.keybinds.as_ref() != &keybinds {
            self.keybinds = Arc::new(keybinds);
        }
        let config_default_mode = self.keybinds_layers.config_default_mode;
        let set_on_command_line = self.options.default_mode
            != self.keybinds_layers.injected_default_mode
            && self.options.default_mode != config_default_mode;
        if set_on_command_line {
            self.keybinds_layers.injected_default_mode = None;
        } else if command_line.preset.is_some() {
            let preset_mode = resolved.default_mode.unwrap_or_default();
            self.options.default_mode = Some(preset_mode);
            self.keybinds_layers.injected_default_mode = Some(preset_mode);
        } else if config_default_mode.is_some() {
            self.options.default_mode = config_default_mode;
            self.keybinds_layers.injected_default_mode = None;
        } else {
            self.options.default_mode = resolved.default_mode;
            self.keybinds_layers.injected_default_mode = resolved.default_mode;
        }
        self.keybinds_layers.active = resolved.active;
    }
    pub fn apply_keybinds_selection(&mut self, layer: KeybindsLayer, selection: KeybindsSelection) {
        self.keybinds_layers.layer_mut(layer).merge(selection);
    }
    pub fn keybinds_selection_snapshot(&self) -> KeybindsSelectionSnapshot {
        let layers = &self.keybinds_layers;
        KeybindsSelectionSnapshot {
            preset: layers.user.preset.clone(),
            primary: layers.user.primary.clone(),
            secondary: layers.user.secondary.clone(),
            unlock: layers.user.unlock.clone(),
            clears_defaults: layers.user.changes.clear_defaults,
            has_own_keybindings: layers.user.changes.has_keybindings(),
            active: layers.active.info.clone(),
            active_values: layers.active.values.clone(),
            error: layers.active.error.clone(),
            set_by_layout: layers.layout.preset.is_some() && self.options.keybinds_preset.is_none(),
            default_mode: self.options.default_mode,
            set_on_command_line: self.has_command_line_keybinds(),
        }
    }
}

#[cfg(test)]
#[path = "./unit/keybind_presets_test.rs"]
mod keybind_presets_test;

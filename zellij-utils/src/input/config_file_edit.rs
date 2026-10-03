use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use strum::IntoEnumIterator;

use super::actions::Action;
use super::config::Config;
use super::config_blocks::{
    config_blocks, env_node, load_plugin_node, menu_entries, menu_item_node, plugin_alias_node,
    styling_colours, theme_node, theme_slots,
};
use super::config_settings::{
    copy_setting, default_config, differing_settings, setting_is_default, setting_kdl_value,
    setting_node_path, setting_value,
};
use super::context_menu::{ContextMenuConfig, CONTEXT_MENU_SECTIONS};
use super::keybind_presets::{KeybindChanges, KeybindsSelection, LEADER_PLACEHOLDERS};
use super::keybinds::Keybinds;
use super::options::Options;
use crate::data::{
    ConfigBlocks, InputMode, KeyWithModifier, MenuItemEntry, PluginAliasEntry, PluginEntry,
    SettingKey, ThemeEntry,
};

const KEYBINDS: &str = "keybinds";
const CONTEXT_MENU: &str = "context_menu";
const PLUGINS: &str = "plugins";
const LOAD_PLUGINS: &str = "load_plugins";
const ENV: &str = "env";
const THEMES: &str = "themes";
const PALETTE_COLOURS: [&str; 11] = [
    "fg", "bg", "red", "green", "blue", "yellow", "magenta", "orange", "cyan", "black", "white",
];
const CLEAR_DEFAULTS: &str = "clear-defaults";
const SHARED_BLOCKS: [&str; 3] = ["shared", "shared_except", "shared_among"];

#[derive(Debug, Clone, PartialEq)]
pub enum KeyState {
    Bound(Vec<Action>),
    Unbound,
    Untouched,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigEdit {
    SetSetting {
        key: SettingKey,
        value: KdlValue,
    },
    RemoveSetting(SettingKey),
    KeybindsAttribute {
        name: &'static str,
        value: Option<String>,
    },
    KeybindsClearDefaults(bool),
    ModeClearDefaults(InputMode),
    Key {
        mode: InputMode,
        key: KeyWithModifier,
        state: KeyState,
    },
    PluginAlias {
        name: String,
        alias: Option<PluginAliasEntry>,
    },
    LoadPlugins {
        base: Vec<PluginEntry>,
        edits: Vec<ListEdit<PluginEntry>>,
    },
    EnvVar {
        name: String,
        value: Option<String>,
    },
    ContextMenuItems {
        section: &'static str,
        base: Vec<MenuItemEntry>,
        edits: Vec<ListEdit<MenuItemEntry>>,
    },
    ContextMenuDefaults {
        section: &'static str,
        entries: Vec<MenuItemEntry>,
    },
    Theme {
        name: String,
        theme: Option<ThemeEntry>,
    },
    ThemeColours {
        name: String,
        theme: ThemeEntry,
        slots: Vec<(&'static str, &'static str)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ListEdit<T> {
    Insert { index: usize, item: T },
    Replace { index: usize, item: T },
    Remove { index: usize },
    Move { from: usize, to: usize },
}

pub fn apply_list_edits<T: Clone>(list: &mut Vec<T>, edits: &[ListEdit<T>]) {
    for edit in edits {
        match edit {
            ListEdit::Insert { index, item } => list.insert((*index).min(list.len()), item.clone()),
            ListEdit::Replace { index, item } => {
                if let Some(target) = list.get_mut(*index) {
                    *target = item.clone();
                }
            },
            ListEdit::Remove { index } => {
                if *index < list.len() {
                    list.remove(*index);
                }
            },
            ListEdit::Move { from, to } => {
                if *from < list.len() {
                    let item = list.remove(*from);
                    list.insert((*to).min(list.len()), item);
                }
            },
        }
    }
}

pub fn list_edits<T: Clone + PartialEq>(old: &[T], new: &[T]) -> Vec<ListEdit<T>> {
    if old == new {
        return vec![];
    }
    if old.len() == new.len() {
        for from in 0..old.len() {
            for to in 0..old.len() {
                if from == to {
                    continue;
                }
                let mut moved = old.to_vec();
                let item = moved.remove(from);
                moved.insert(to, item);
                if moved == new {
                    return vec![ListEdit::Move { from, to }];
                }
            }
        }
    }
    let (n, m) = (old.len(), new.len());
    let mut common = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            common[i][j] = if old[i] == new[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let mut edits = vec![];
    let (mut i, mut j, mut index) = (0, 0, 0);
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            i += 1;
            j += 1;
            index += 1;
        } else if i < n && j < m && common[i + 1][j + 1] == common[i][j] {
            edits.push(ListEdit::Replace {
                index,
                item: new[j].clone(),
            });
            i += 1;
            j += 1;
            index += 1;
        } else if j < m && (i == n || common[i][j + 1] >= common[i + 1][j]) {
            edits.push(ListEdit::Insert {
                index,
                item: new[j].clone(),
            });
            j += 1;
            index += 1;
        } else {
            edits.push(ListEdit::Remove { index });
            i += 1;
        }
    }
    edits
}

#[derive(Debug, Clone, PartialEq)]
pub enum SaveOutcome {
    Written {
        file_contents: Option<String>,
        saved_config: Config,
    },
    ChangedOutside,
    Failed(Option<PathBuf>),
}

pub fn key_state(changes: &KeybindChanges, mode: InputMode, key: &KeyWithModifier) -> KeyState {
    match changes.modes.get(&mode) {
        Some(mode_changes) => match mode_changes.bind.get(key) {
            Some(actions) => KeyState::Bound(actions.clone()),
            None if mode_changes.unbind.contains(key) => KeyState::Unbound,
            None => KeyState::Untouched,
        },
        None => KeyState::Untouched,
    }
}

fn keybind_edits(saved: &KeybindsSelection, runtime: &KeybindsSelection) -> Vec<ConfigEdit> {
    let mut edits = vec![];
    if runtime.changes.clear_defaults != saved.changes.clear_defaults {
        edits.push(ConfigEdit::KeybindsClearDefaults(
            runtime.changes.clear_defaults,
        ));
    }
    if runtime.preset != saved.preset {
        edits.push(ConfigEdit::KeybindsAttribute {
            name: "preset",
            value: runtime.preset.clone(),
        });
    }
    for placeholder in LEADER_PLACEHOLDERS {
        if runtime.leader(placeholder) != saved.leader(placeholder) {
            edits.push(ConfigEdit::KeybindsAttribute {
                name: placeholder,
                value: runtime.leader(placeholder).cloned(),
            });
        }
    }
    let dropped_all_keys = saved.changes.clear_defaults && !runtime.changes.clear_defaults;
    let modes: BTreeSet<InputMode> = saved
        .changes
        .modes
        .keys()
        .chain(runtime.changes.modes.keys())
        .copied()
        .collect();
    for mode in modes {
        let saved_mode = saved.changes.modes.get(&mode).cloned().unwrap_or_default();
        let runtime_mode = runtime
            .changes
            .modes
            .get(&mode)
            .cloned()
            .unwrap_or_default();
        if runtime_mode.clear_defaults && (!saved_mode.clear_defaults || dropped_all_keys) {
            edits.push(ConfigEdit::ModeClearDefaults(mode));
        }
        let keys: BTreeSet<KeyWithModifier> = saved_mode
            .bind
            .keys()
            .chain(saved_mode.unbind.iter())
            .chain(runtime_mode.bind.keys())
            .chain(runtime_mode.unbind.iter())
            .cloned()
            .collect();
        for key in keys {
            let saved_state = if dropped_all_keys {
                KeyState::Untouched
            } else {
                key_state(&saved.changes, mode, &key)
            };
            let runtime_state = key_state(&runtime.changes, mode, &key);
            if saved_state != runtime_state {
                edits.push(ConfigEdit::Key {
                    mode,
                    key,
                    state: runtime_state,
                });
            }
        }
    }
    edits
}

fn menu_section(menu: &ContextMenuConfig, section: &str) -> Vec<MenuItemEntry> {
    menu.section(section)
        .map(|entries| menu_entries(entries, None))
        .unwrap_or_default()
}

fn context_menu_edits(saved: &ContextMenuConfig, runtime: &ContextMenuConfig) -> Vec<ConfigEdit> {
    let defaults = &default_config().context_menu;
    CONTEXT_MENU_SECTIONS
        .iter()
        .filter_map(|section| {
            let saved_entries = menu_section(saved, section);
            let runtime_entries = menu_section(runtime, section);
            if saved_entries == runtime_entries {
                return None;
            }
            let default_entries = menu_section(defaults, section);
            if runtime_entries == default_entries {
                Some(ConfigEdit::ContextMenuDefaults {
                    section,
                    entries: default_entries,
                })
            } else {
                Some(ConfigEdit::ContextMenuItems {
                    section,
                    edits: list_edits(&saved_entries, &runtime_entries),
                    base: saved_entries,
                })
            }
        })
        .collect()
}

fn named<T: Clone>(entries: &[T], name_of: impl Fn(&T) -> &str) -> BTreeMap<String, T> {
    entries
        .iter()
        .map(|entry| (name_of(entry).to_owned(), entry.clone()))
        .collect()
}

fn plugin_alias_edits(saved: &ConfigBlocks, runtime: &ConfigBlocks) -> Vec<ConfigEdit> {
    let defaults = named(
        &config_blocks(default_config(), false, None).plugin_aliases,
        |alias: &PluginAliasEntry| &alias.name,
    );
    let saved = named(&saved.plugin_aliases, |alias| &alias.name);
    let runtime = named(&runtime.plugin_aliases, |alias| &alias.name);
    let names: BTreeSet<&String> = saved.keys().chain(runtime.keys()).collect();
    names
        .into_iter()
        .filter(|name| saved.get(*name) != runtime.get(*name))
        .map(|name| {
            let alias = runtime.get(name).cloned();
            let alias = if alias.is_some() && alias.as_ref() == defaults.get(name) {
                None
            } else {
                alias
            };
            ConfigEdit::PluginAlias {
                name: name.clone(),
                alias,
            }
        })
        .collect()
}

fn env_edits(saved: &ConfigBlocks, runtime: &ConfigBlocks) -> Vec<ConfigEdit> {
    let saved = named(&saved.env, |entry| &entry.name);
    let runtime = named(&runtime.env, |entry| &entry.name);
    let names: BTreeSet<&String> = saved.keys().chain(runtime.keys()).collect();
    names
        .into_iter()
        .filter(|name| saved.get(*name) != runtime.get(*name))
        .map(|name| ConfigEdit::EnvVar {
            name: name.clone(),
            value: runtime.get(name).map(|entry| entry.value.clone()),
        })
        .collect()
}

fn theme_edits(saved: &ConfigBlocks, runtime: &ConfigBlocks) -> Vec<ConfigEdit> {
    let saved = named(&saved.themes, |theme| &theme.name);
    let runtime = named(&runtime.themes, |theme| &theme.name);
    let names: BTreeSet<&String> = saved.keys().chain(runtime.keys()).collect();
    let slots = theme_slots();
    let mut edits = vec![];
    for name in names {
        match (saved.get(name), runtime.get(name)) {
            (Some(saved_theme), Some(runtime_theme)) if saved_theme != runtime_theme => {
                let changed: Vec<(&'static str, &'static str)> = slots
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        saved_theme.colours.get(*index) != runtime_theme.colours.get(*index)
                    })
                    .map(|(_, slot)| *slot)
                    .collect();
                edits.push(ConfigEdit::ThemeColours {
                    name: name.clone(),
                    theme: runtime_theme.clone(),
                    slots: changed,
                });
            },
            (None, Some(runtime_theme)) => edits.push(ConfigEdit::Theme {
                name: name.clone(),
                theme: Some(runtime_theme.clone()),
            }),
            (Some(_), None) => edits.push(ConfigEdit::Theme {
                name: name.clone(),
                theme: None,
            }),
            _ => {},
        }
    }
    edits
}

fn block_edits(key: SettingKey, saved: &Config, runtime: &Config) -> Vec<ConfigEdit> {
    match key {
        SettingKey::Keybinds => {
            keybind_edits(&saved.keybinds_layers.user, &runtime.keybinds_layers.user)
        },
        SettingKey::ContextMenu => context_menu_edits(&saved.context_menu, &runtime.context_menu),
        _ => {
            let saved_blocks = config_blocks(saved, false, None);
            let runtime_blocks = config_blocks(runtime, false, None);
            match key {
                SettingKey::PluginAliases => plugin_alias_edits(&saved_blocks, &runtime_blocks),
                SettingKey::Env => env_edits(&saved_blocks, &runtime_blocks),
                SettingKey::Themes => theme_edits(&saved_blocks, &runtime_blocks),
                SettingKey::LoadPlugins => {
                    if saved_blocks.load_plugins == runtime_blocks.load_plugins {
                        vec![]
                    } else {
                        vec![ConfigEdit::LoadPlugins {
                            edits: list_edits(
                                &saved_blocks.load_plugins,
                                &runtime_blocks.load_plugins,
                            ),
                            base: saved_blocks.load_plugins,
                        }]
                    }
                },
                _ => vec![],
            }
        },
    }
}

pub fn edits_between(saved: &Config, runtime: &Config) -> Vec<ConfigEdit> {
    let mut edits = vec![];
    for key in differing_settings(saved, runtime) {
        if key.is_block() {
            edits.extend(block_edits(key, saved, runtime));
            continue;
        }
        if setting_is_default(runtime, key) {
            edits.push(ConfigEdit::RemoveSetting(key));
        } else {
            match setting_kdl_value(runtime, key) {
                Some(value) => edits.push(ConfigEdit::SetSetting { key, value }),
                None => edits.push(ConfigEdit::RemoveSetting(key)),
            }
        }
    }
    edits
}

pub fn edited_settings(edits: &[ConfigEdit]) -> BTreeSet<SettingKey> {
    edits
        .iter()
        .map(|edit| match edit {
            ConfigEdit::SetSetting { key, .. } | ConfigEdit::RemoveSetting(key) => *key,
            ConfigEdit::KeybindsAttribute { .. }
            | ConfigEdit::KeybindsClearDefaults(_)
            | ConfigEdit::ModeClearDefaults(_)
            | ConfigEdit::Key { .. } => SettingKey::Keybinds,
            ConfigEdit::PluginAlias { .. } => SettingKey::PluginAliases,
            ConfigEdit::LoadPlugins { .. } => SettingKey::LoadPlugins,
            ConfigEdit::EnvVar { .. } => SettingKey::Env,
            ConfigEdit::ContextMenuItems { .. } | ConfigEdit::ContextMenuDefaults { .. } => {
                SettingKey::ContextMenu
            },
            ConfigEdit::Theme { .. } | ConfigEdit::ThemeColours { .. } => SettingKey::Themes,
        })
        .collect()
}

fn node_name(node: &KdlNode) -> &str {
    node.name().value()
}

fn indent_of_leading(leading: Option<&str>) -> Option<String> {
    let leading = leading?;
    let after_last_line = leading.rsplit('\n').next().unwrap_or(leading);
    if after_last_line.chars().all(|c| c == ' ' || c == '\t') {
        Some(after_last_line.to_owned())
    } else {
        None
    }
}

fn child_indent(document: &KdlDocument, owner_indent: Option<&str>) -> String {
    document
        .nodes()
        .iter()
        .find_map(|node| indent_of_leading(node.leading()))
        .unwrap_or_else(|| match owner_indent {
            None => String::new(),
            Some(owner_indent) => format!("{}    ", owner_indent),
        })
}

fn whitespace_kept_before_first_node(text: &str, inside_block: bool) -> String {
    match text.rfind('\n') {
        Some(position) => text[..=position].to_owned(),
        None if inside_block => "\n".to_owned(),
        None if text.trim().is_empty() => String::new(),
        None => format!("{}\n", text),
    }
}

fn push_node(
    document: &mut KdlDocument,
    mut node: KdlNode,
    indent: &str,
    owner_indent: Option<&str>,
) {
    node.set_trailing("\n");
    if let Some(last) = document.nodes_mut().last_mut() {
        if let Some(trailing) = last.trailing() {
            if !trailing.contains('\n') {
                let trailing = format!("{}\n", trailing);
                last.set_trailing(trailing);
            }
        }
        let document_trailing = document.trailing().unwrap_or("").to_owned();
        let (kept, rest) = match document_trailing.rfind('\n') {
            Some(newline) => (
                document_trailing[..=newline].to_owned(),
                document_trailing[newline + 1..].to_owned(),
            ),
            None => (String::new(), document_trailing),
        };
        node.set_leading(format!("{}{}", kept, indent));
        document.set_trailing(rest);
    } else {
        let surrounding = format!(
            "{}{}",
            document.leading().unwrap_or(""),
            document.trailing().unwrap_or("")
        );
        let kept = whitespace_kept_before_first_node(&surrounding, owner_indent.is_some());
        node.set_leading(format!("{}{}", kept, indent));
        document.set_leading("");
        document.set_trailing(owner_indent.unwrap_or("").to_owned());
    }
    document.nodes_mut().push(node);
}

fn remove_node_at(document: &mut KdlDocument, position: usize) {
    let removed = document.nodes_mut().remove(position);
    let leading = removed.leading().unwrap_or("");
    let kept = match leading.rfind('\n') {
        Some(newline) => leading[..=newline].to_owned(),
        None => String::new(),
    };
    if kept.is_empty() {
        return;
    }
    match document.nodes_mut().get_mut(position) {
        Some(next) => {
            let next_leading = format!("{}{}", kept, next.leading().unwrap_or(""));
            next.set_leading(next_leading);
        },
        None => {
            let trailing = format!("{}{}", kept, document.trailing().unwrap_or(""));
            document.set_trailing(trailing);
        },
    }
}

fn remove_nodes_named(document: &mut KdlDocument, name: &str) {
    while let Some(position) = document
        .nodes()
        .iter()
        .position(|node| node_name(node) == name)
    {
        remove_node_at(document, position);
    }
}

fn node_indent(document: &KdlDocument, position: usize, owner_indent: Option<&str>) -> String {
    indent_of_leading(document.nodes()[position].leading())
        .unwrap_or_else(|| child_indent(document, owner_indent))
}

fn open_block<'a>(
    document: &'a mut KdlDocument,
    owner_indent: Option<&str>,
    name: &str,
    create: bool,
) -> Option<(&'a mut KdlDocument, String)> {
    let position = match document
        .nodes()
        .iter()
        .position(|node| node_name(node) == name)
    {
        Some(position) => position,
        None => {
            if !create {
                return None;
            }
            let indent = child_indent(document, owner_indent);
            push_node(document, KdlNode::new(name), &indent, owner_indent);
            document.nodes().len() - 1
        },
    };
    let indent = node_indent(document, position, owner_indent);
    let node = &mut document.nodes_mut()[position];
    if node.children().is_none() {
        node.set_children(KdlDocument::new());
    }
    node.children_mut()
        .as_mut()
        .map(|children| (children, indent))
}

fn block_at_path<'a>(
    document: &'a mut KdlDocument,
    path: &[&str],
    create: bool,
) -> Option<(&'a mut KdlDocument, Option<String>)> {
    let mut current = document;
    let mut owner_indent: Option<String> = None;
    for name in path {
        let (next, indent) = open_block(current, owner_indent.as_deref(), name, create)?;
        current = next;
        owner_indent = Some(indent);
    }
    Some((current, owner_indent))
}

fn replace_entry_keeping_format(node: &mut KdlNode, position: usize, mut entry: KdlEntry) {
    let old = &node.entries()[position];
    if let Some(leading) = old.leading() {
        entry.set_leading(leading.to_owned());
    }
    if let Some(trailing) = old.trailing() {
        entry.set_trailing(trailing.to_owned());
    }
    node.entries_mut()[position] = entry;
}

fn set_first_argument(node: &mut KdlNode, value: KdlValue) {
    match node
        .entries()
        .iter()
        .position(|entry| entry.name().is_none())
    {
        Some(position) => replace_entry_keeping_format(node, position, KdlEntry::new(value)),
        None => node.entries_mut().insert(0, KdlEntry::new(value)),
    }
}

fn property_name(entry: &KdlEntry) -> Option<&str> {
    entry.name().map(|name| name.value())
}

fn set_property(node: &mut KdlNode, name: &str, value: Option<KdlValue>) {
    match value {
        Some(value) => {
            let positions: Vec<usize> = node
                .entries()
                .iter()
                .enumerate()
                .filter(|(_, entry)| property_name(entry) == Some(name))
                .map(|(position, _)| position)
                .collect();
            if positions.is_empty() {
                node.entries_mut()
                    .push(KdlEntry::new_prop(name, value.clone()));
            }
            for position in positions {
                replace_entry_keeping_format(
                    node,
                    position,
                    KdlEntry::new_prop(name, value.clone()),
                );
            }
        },
        None => node
            .entries_mut()
            .retain(|entry| property_name(entry) != Some(name)),
    }
}

fn set_setting(document: &mut KdlDocument, key: SettingKey, value: KdlValue) {
    let (parents, name) = setting_node_path(key);
    let Some((block, owner_indent)) = block_at_path(document, parents, true) else {
        return;
    };
    match block
        .nodes_mut()
        .iter_mut()
        .find(|node| node_name(node) == name)
    {
        Some(node) => set_first_argument(node, value),
        None => {
            let indent = child_indent(block, owner_indent.as_deref());
            let mut node = KdlNode::new(name);
            node.push(KdlEntry::new(value));
            push_node(block, node, &indent, owner_indent.as_deref());
        },
    }
}

fn remove_setting(document: &mut KdlDocument, key: SettingKey) {
    let (parents, name) = setting_node_path(key);
    if let Some((block, _)) = block_at_path(document, parents, false) {
        remove_nodes_named(block, name);
    }
}

fn mode_name(mode: InputMode) -> String {
    format!("{:?}", mode).to_lowercase()
}

fn node_mode(node: &KdlNode) -> Option<InputMode> {
    InputMode::from_str(node_name(node)).ok()
}

fn string_arguments(node: &KdlNode) -> Vec<&str> {
    node.entries()
        .iter()
        .filter(|entry| entry.name().is_none())
        .filter_map(|entry| entry.value().as_string())
        .collect()
}

fn shared_block_covers(node: &KdlNode, mode: InputMode) -> bool {
    let listed: Vec<InputMode> = string_arguments(node)
        .into_iter()
        .filter_map(|name| InputMode::from_str(name).ok())
        .collect();
    match node_name(node) {
        "shared_among" => listed.contains(&mode),
        "shared" | "shared_except" => !listed.contains(&mode),
        _ => false,
    }
}

fn key_matches(text: &str, key: &KeyWithModifier) -> bool {
    KeyWithModifier::from_str(text)
        .map(|parsed| &parsed == key)
        .unwrap_or(false)
}

fn statement_has_key(node: &KdlNode, key: &KeyWithModifier) -> bool {
    string_arguments(node)
        .into_iter()
        .any(|text| key_matches(text, key))
}

fn remove_key_from_statements(block: &mut KdlDocument, kind: &str, key: &KeyWithModifier) -> bool {
    let mut removed_any = false;
    let mut position = 0;
    while position < block.nodes().len() {
        let node = &block.nodes()[position];
        if node_name(node) != kind || !statement_has_key(node, key) {
            position += 1;
            continue;
        }
        removed_any = true;
        let node = &mut block.nodes_mut()[position];
        node.entries_mut().retain(|entry| {
            entry.name().is_some()
                || !entry
                    .value()
                    .as_string()
                    .map(|text| key_matches(text, key))
                    .unwrap_or(false)
        });
        if string_arguments(&block.nodes()[position]).is_empty() {
            remove_node_at(block, position);
        } else {
            position += 1;
        }
    }
    removed_any
}

fn block_has_statement(block: &KdlDocument, kind: &str, key: &KeyWithModifier) -> bool {
    block
        .nodes()
        .iter()
        .any(|node| node_name(node) == kind && statement_has_key(node, key))
}

fn mode_blocks_have_statement(
    keybinds: &KdlDocument,
    mode: InputMode,
    kind: &str,
    key: &KeyWithModifier,
) -> bool {
    keybinds.nodes().iter().any(|node| {
        node_mode(node) == Some(mode)
            && node
                .children()
                .map(|children| block_has_statement(children, kind, key))
                .unwrap_or(false)
    })
}

fn bind_node(key: &KeyWithModifier, actions: &[Action]) -> KdlNode {
    let mut bindings = BTreeMap::new();
    bindings.insert(key.clone(), actions.to_vec());
    Keybinds::default()
        .serialize_mode_keybinds(&bindings)
        .nodes()
        .first()
        .cloned()
        .unwrap_or_else(|| KdlNode::new("bind"))
}

fn unbind_node(key: &KeyWithModifier) -> KdlNode {
    let mut node = KdlNode::new("unbind");
    node.push(key.to_kdl());
    node
}

fn append_to_mode_block(
    keybinds: &mut KdlDocument,
    keybinds_indent: Option<&str>,
    mode: InputMode,
    statement: KdlNode,
) {
    let mode_name = mode_name(mode);
    let position = keybinds
        .nodes()
        .iter()
        .rposition(|node| node_mode(node) == Some(mode));
    let position = match position {
        Some(position) => position,
        None => {
            let indent = child_indent(keybinds, keybinds_indent);
            push_node(keybinds, KdlNode::new(mode_name), &indent, keybinds_indent);
            keybinds.nodes().len() - 1
        },
    };
    let mode_indent = node_indent(keybinds, position, keybinds_indent);
    let node = &mut keybinds.nodes_mut()[position];
    if node.children().is_none() {
        node.set_children(KdlDocument::new());
    }
    if let Some(children) = node.children_mut().as_mut() {
        let indent = child_indent(children, Some(&mode_indent));
        push_node(children, statement, &indent, Some(&mode_indent));
    }
}

fn ensure_direct_state(
    keybinds: &mut KdlDocument,
    keybinds_indent: Option<&str>,
    mode: InputMode,
    key: &KeyWithModifier,
    state: &KeyState,
) {
    match state {
        KeyState::Bound(actions) => {
            if !mode_blocks_have_statement(keybinds, mode, "bind", key) {
                append_to_mode_block(keybinds, keybinds_indent, mode, bind_node(key, actions));
            }
        },
        KeyState::Unbound => {
            if !mode_blocks_have_statement(keybinds, mode, "unbind", key) {
                append_to_mode_block(keybinds, keybinds_indent, mode, unbind_node(key));
            }
        },
        KeyState::Untouched => {},
    }
}

fn split_shared_blocks(
    keybinds: &mut KdlDocument,
    keybinds_indent: Option<&str>,
    mode: InputMode,
    key: &KeyWithModifier,
    runtime: &KeybindChanges,
) {
    let mut other_modes: BTreeSet<InputMode> = BTreeSet::new();
    for node in keybinds.nodes_mut() {
        if !SHARED_BLOCKS.contains(&node_name(node)) || !shared_block_covers(node, mode) {
            continue;
        }
        let covered: Vec<InputMode> = InputMode::iter()
            .filter(|other| *other != mode && shared_block_covers(node, *other))
            .collect();
        if let Some(children) = node.children_mut().as_mut() {
            let removed_bind = remove_key_from_statements(children, "bind", key);
            let removed_unbind = remove_key_from_statements(children, "unbind", key);
            if removed_bind || removed_unbind {
                other_modes.extend(covered);
            }
        }
    }
    let global_unbind_had_key = remove_key_from_statements(keybinds, "unbind", key);
    if global_unbind_had_key {
        other_modes.extend(InputMode::iter().filter(|other| *other != mode));
    }
    for other in other_modes {
        let state = key_state(runtime, other, key);
        ensure_direct_state(keybinds, keybinds_indent, other, key, &state);
    }
}

fn edit_key(
    keybinds: &mut KdlDocument,
    keybinds_indent: Option<&str>,
    mode: InputMode,
    key: &KeyWithModifier,
    state: &KeyState,
    runtime: &KeybindChanges,
) {
    for node in keybinds.nodes_mut() {
        if node_mode(node) != Some(mode) {
            continue;
        }
        if let Some(children) = node.children_mut().as_mut() {
            remove_key_from_statements(children, "bind", key);
            remove_key_from_statements(children, "unbind", key);
        }
    }
    match state {
        KeyState::Bound(actions) => {
            append_to_mode_block(keybinds, keybinds_indent, mode, bind_node(key, actions));
            if keybinds
                .nodes()
                .iter()
                .any(|node| node_name(node) == "unbind" && statement_has_key(node, key))
            {
                split_shared_blocks(keybinds, keybinds_indent, mode, key, runtime);
            }
        },
        KeyState::Unbound => {
            append_to_mode_block(keybinds, keybinds_indent, mode, unbind_node(key));
        },
        KeyState::Untouched => {
            split_shared_blocks(keybinds, keybinds_indent, mode, key, runtime);
        },
    }
}

fn keybinds_block<'a>(document: &'a mut KdlDocument) -> (&'a mut KdlNode, Option<String>) {
    let position = match document
        .nodes()
        .iter()
        .position(|node| node_name(node) == KEYBINDS)
    {
        Some(position) => position,
        None => {
            let indent = child_indent(document, None);
            push_node(document, KdlNode::new(KEYBINDS), &indent, None);
            document.nodes().len() - 1
        },
    };
    let indent = node_indent(document, position, None);
    (&mut document.nodes_mut()[position], Some(indent))
}

fn keybinds_children(node: &mut KdlNode) -> &mut KdlDocument {
    if node.children().is_none() {
        node.set_children(KdlDocument::new());
    }
    node.children_mut()
        .as_mut()
        .expect("children were just set")
}

fn only_removes(edit: &ConfigEdit) -> bool {
    matches!(
        edit,
        ConfigEdit::KeybindsAttribute { value: None, .. }
            | ConfigEdit::KeybindsClearDefaults(false)
            | ConfigEdit::Key {
                state: KeyState::Untouched,
                ..
            }
    )
}

fn apply_keybinds_edit(document: &mut KdlDocument, edit: &ConfigEdit, runtime: &KeybindChanges) {
    let has_keybinds = document
        .nodes()
        .iter()
        .any(|node| node_name(node) == KEYBINDS);
    if !has_keybinds && only_removes(edit) {
        return;
    }
    let (node, indent) = keybinds_block(document);
    match edit {
        ConfigEdit::KeybindsAttribute { name, value } => {
            set_property(node, name, value.clone().map(KdlValue::String));
        },
        ConfigEdit::KeybindsClearDefaults(true) => {
            set_property(node, CLEAR_DEFAULTS, Some(KdlValue::Bool(true)));
        },
        ConfigEdit::KeybindsClearDefaults(false) => {
            set_property(node, CLEAR_DEFAULTS, None);
            if node.children().is_some() {
                node.clear_children();
                node.set_before_children("");
            }
        },
        ConfigEdit::ModeClearDefaults(mode) => {
            let children = keybinds_children(node);
            let position = children
                .nodes()
                .iter()
                .position(|block| node_mode(block) == Some(*mode));
            let position = match position {
                Some(position) => position,
                None => {
                    let child_indent_text = child_indent(children, indent.as_deref());
                    let mut block = KdlNode::new(mode_name(*mode));
                    block.set_children(KdlDocument::new());
                    push_node(children, block, &child_indent_text, indent.as_deref());
                    children.nodes().len() - 1
                },
            };
            set_property(
                &mut children.nodes_mut()[position],
                CLEAR_DEFAULTS,
                Some(KdlValue::Bool(true)),
            );
        },
        ConfigEdit::Key { mode, key, state } => {
            let children = keybinds_children(node);
            edit_key(children, indent.as_deref(), *mode, key, state, runtime);
        },
        _ => {},
    }
}

fn split_leading(leading: &str) -> (String, String) {
    match leading.find('\n') {
        Some(newline) => (
            leading[..=newline].to_owned(),
            leading[newline + 1..].to_owned(),
        ),
        None => (String::new(), leading.to_owned()),
    }
}

fn insert_node_at(
    document: &mut KdlDocument,
    index: usize,
    mut node: KdlNode,
    indent: &str,
    owner_indent: Option<&str>,
    body: Option<&str>,
) {
    let body = body.unwrap_or(indent);
    if index >= document.nodes().len() {
        push_node(document, node, indent, owner_indent);
        if let Some(last) = document.nodes_mut().last_mut() {
            let leading = last.leading().unwrap_or("").to_owned();
            let kept = leading.strip_suffix(indent).unwrap_or(&leading).to_owned();
            last.set_leading(format!("{}{}", kept, body));
        }
        return;
    }
    let existing_leading = document.nodes()[index].leading().unwrap_or("").to_owned();
    let (head, tail) = split_leading(&existing_leading);
    node.set_leading(format!("{}{}", head, body));
    node.set_trailing("\n");
    document.nodes_mut()[index].set_leading(tail);
    document.nodes_mut().insert(index, node);
}

fn take_node_at(document: &mut KdlDocument, position: usize) -> (KdlNode, String) {
    let node = document.nodes_mut().remove(position);
    let leading = node.leading().unwrap_or("").to_owned();
    let (head, body) = split_leading(&leading);
    if !head.is_empty() {
        match document.nodes_mut().get_mut(position) {
            Some(next) => {
                let next_leading = format!("{}{}", head, next.leading().unwrap_or(""));
                next.set_leading(next_leading);
            },
            None => {
                let trailing = format!("{}{}", head, document.trailing().unwrap_or(""));
                document.set_trailing(trailing);
            },
        }
    }
    (node, body)
}

fn replace_node_at(document: &mut KdlDocument, index: usize, mut node: KdlNode) {
    let old = &document.nodes()[index];
    if let Some(leading) = old.leading() {
        node.set_leading(leading.to_owned());
    }
    if let Some(trailing) = old.trailing() {
        node.set_trailing(trailing.to_owned());
    }
    document.nodes_mut()[index] = node;
}

fn apply_node_list_edits<T>(
    block: &mut KdlDocument,
    owner_indent: Option<&str>,
    edits: &[ListEdit<T>],
    make_node: impl Fn(&T) -> Option<KdlNode>,
) {
    let indent = child_indent(block, owner_indent);
    for edit in edits {
        match edit {
            ListEdit::Insert { index, item } => {
                if let Some(node) = make_node(item) {
                    insert_node_at(block, *index, node, &indent, owner_indent, None);
                }
            },
            ListEdit::Replace { index, item } => {
                if *index < block.nodes().len() {
                    if let Some(node) = make_node(item) {
                        replace_node_at(block, *index, node);
                    }
                }
            },
            ListEdit::Remove { index } => {
                if *index < block.nodes().len() {
                    remove_node_at(block, *index);
                }
            },
            ListEdit::Move { from, to } => {
                if *from < block.nodes().len() {
                    let (node, body) = take_node_at(block, *from);
                    let body = if body.trim().is_empty() {
                        None
                    } else {
                        Some(body)
                    };
                    insert_node_at(block, *to, node, &indent, owner_indent, body.as_deref());
                }
            },
        }
    }
}

fn rebuild_children<T: PartialEq>(
    block: &mut KdlDocument,
    owner_indent: Option<&str>,
    base: &[T],
    read_node: impl Fn(&KdlNode) -> Option<T>,
    make_node: impl Fn(&T) -> Option<KdlNode>,
) {
    let current: Vec<Option<T>> = block.nodes().iter().map(|node| read_node(node)).collect();
    let matches = current.len() == base.len()
        && current
            .iter()
            .zip(base.iter())
            .all(|(current, base)| current.as_ref() == Some(base));
    if matches {
        return;
    }
    let indent = child_indent(block, owner_indent);
    while !block.nodes().is_empty() {
        let last = block.nodes().len() - 1;
        remove_node_at(block, last);
    }
    block.set_trailing("");
    for item in base {
        if let Some(node) = make_node(item) {
            push_node(block, node, &indent, owner_indent);
        }
    }
}

fn last_block_position(document: &KdlDocument, name: &str) -> Option<usize> {
    document
        .nodes()
        .iter()
        .rposition(|node| node_name(node) == name)
}

fn top_level_block<'a>(
    document: &'a mut KdlDocument,
    name: &str,
    create: bool,
) -> Option<(&'a mut KdlDocument, String)> {
    let position = match last_block_position(document, name) {
        Some(position) => position,
        None => {
            if !create {
                return None;
            }
            let indent = child_indent(document, None);
            push_node(document, KdlNode::new(name), &indent, None);
            document.nodes().len() - 1
        },
    };
    let indent = node_indent(document, position, None);
    let node = &mut document.nodes_mut()[position];
    if node.children().is_none() {
        node.set_children(KdlDocument::new());
    }
    node.children_mut()
        .as_mut()
        .map(|children| (children, indent))
}

fn remove_empty_top_level_blocks(document: &mut KdlDocument, name: &str) {
    while let Some(position) = document.nodes().iter().position(|node| {
        node_name(node) == name
            && node
                .children()
                .map(|children| children.nodes().is_empty())
                .unwrap_or(true)
    }) {
        remove_node_at(document, position);
    }
}

fn set_named_node(document: &mut KdlDocument, block_name: &str, name: &str, node: Option<KdlNode>) {
    let mut placed = false;
    for block in document.nodes_mut() {
        if node_name(block) != block_name {
            continue;
        }
        let Some(children) = block.children_mut().as_mut() else {
            continue;
        };
        let positions: Vec<usize> = children
            .nodes()
            .iter()
            .enumerate()
            .filter(|(_, n)| node_name(n) == name)
            .map(|(position, _)| position)
            .collect();
        for position in positions.into_iter().rev() {
            match &node {
                Some(node) if !placed && position == first_position(children, name) => {
                    replace_node_at(children, position, node.clone());
                    placed = true;
                },
                _ => remove_node_at(children, position),
            }
        }
    }
    if let (Some(node), false) = (node, placed) {
        if let Some((block, indent)) = top_level_block(document, block_name, true) {
            let child_indent_text = child_indent(block, Some(&indent));
            push_node(block, node, &child_indent_text, Some(&indent));
        }
    }
}

fn first_position(document: &KdlDocument, name: &str) -> usize {
    document
        .nodes()
        .iter()
        .position(|n| node_name(n) == name)
        .unwrap_or(usize::MAX)
}

fn node_plugin_entry(node: &KdlNode, name_is_location: bool) -> PluginEntry {
    let location = if name_is_location {
        node_name(node).to_owned()
    } else {
        node.get("location")
            .and_then(|entry| entry.value().as_string())
            .unwrap_or("")
            .to_owned()
    };
    let mut entry = PluginEntry {
        location,
        cwd: None,
        configuration: vec![],
    };
    if let Some(children) = node.children() {
        for child in children.nodes() {
            let value = child
                .entries()
                .iter()
                .find(|entry| entry.name().is_none())
                .map(|entry| match entry.value() {
                    KdlValue::String(text) | KdlValue::RawString(text) => text.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
            if node_name(child) == "cwd" {
                entry.cwd = Some(value);
            } else {
                entry
                    .configuration
                    .push((node_name(child).to_owned(), value));
            }
        }
    }
    entry.configuration.sort();
    entry
}

fn apply_load_plugins_edit(
    document: &mut KdlDocument,
    base: &[PluginEntry],
    edits: &[ListEdit<PluginEntry>],
) {
    let Some((block, indent)) = top_level_block(document, LOAD_PLUGINS, true) else {
        return;
    };
    rebuild_children(
        block,
        Some(&indent),
        base,
        |node| Some(node_plugin_entry(node, true)),
        |entry| Some(load_plugin_node(entry)),
    );
    apply_node_list_edits(block, Some(&indent), edits, |entry| {
        Some(load_plugin_node(entry))
    });
}

fn apply_env_edit(document: &mut KdlDocument, name: &str, value: Option<&str>) {
    let existing = document.nodes_mut().iter_mut().rev().find_map(|block| {
        if node_name(block) != ENV {
            return None;
        }
        block.children_mut().as_mut().and_then(|children| {
            children
                .nodes_mut()
                .iter_mut()
                .find(|n| node_name(n) == name)
        })
    });
    match (existing, value) {
        (Some(node), Some(value)) => set_first_argument(node, KdlValue::String(value.to_owned())),
        (_, value) => set_named_node(
            document,
            ENV,
            name,
            value.map(|value| {
                env_node(&crate::data::EnvVarEntry {
                    name: name.to_owned(),
                    value: value.to_owned(),
                })
            }),
        ),
    }
    remove_empty_top_level_blocks(document, ENV);
}

fn menu_entry_of(node: &KdlNode) -> Option<MenuItemEntry> {
    ContextMenuConfig::entry_from_kdl(node, &Options::default())
        .ok()
        .map(|entry| menu_entries(&[entry], None).remove(0))
}

fn context_menu_clears_everything(document: &KdlDocument) -> bool {
    document
        .nodes()
        .iter()
        .filter(|node| node_name(node) == CONTEXT_MENU)
        .any(|node| {
            node.get(CLEAR_DEFAULTS)
                .map(|entry| entry.value().as_bool() == Some(true))
                .unwrap_or(false)
        })
}

fn context_menu_section<'a>(
    document: &'a mut KdlDocument,
    section: &str,
) -> Option<(&'a mut KdlDocument, String)> {
    let clears_everything = context_menu_clears_everything(document);
    let (block, owner_indent) = top_level_block(document, CONTEXT_MENU, true)?;
    let position = match block
        .nodes()
        .iter()
        .rposition(|node| node_name(node) == section)
    {
        Some(position) => position,
        None => {
            let indent = child_indent(block, Some(&owner_indent));
            let mut node = KdlNode::new(section);
            node.set_children(KdlDocument::new());
            push_node(block, node, &indent, Some(&owner_indent));
            block.nodes().len() - 1
        },
    };
    let indent = node_indent(block, position, Some(&owner_indent));
    let node = &mut block.nodes_mut()[position];
    if !clears_everything {
        set_property(node, CLEAR_DEFAULTS, Some(KdlValue::Bool(true)));
    }
    if node.children().is_none() {
        node.set_children(KdlDocument::new());
    }
    node.children_mut()
        .as_mut()
        .map(|children| (children, indent))
}

fn apply_context_menu_items(
    document: &mut KdlDocument,
    section: &str,
    base: &[MenuItemEntry],
    edits: &[ListEdit<MenuItemEntry>],
) {
    let Some((children, indent)) = context_menu_section(document, section) else {
        return;
    };
    rebuild_children(children, Some(&indent), base, menu_entry_of, |entry| {
        menu_item_node(entry).ok()
    });
    apply_node_list_edits(children, Some(&indent), edits, |entry| {
        menu_item_node(entry).ok()
    });
}

fn apply_context_menu_defaults(
    document: &mut KdlDocument,
    section: &str,
    entries: &[MenuItemEntry],
) {
    if context_menu_clears_everything(document) {
        if let Some((children, indent)) = context_menu_section(document, section) {
            rebuild_children(children, Some(&indent), entries, menu_entry_of, |entry| {
                menu_item_node(entry).ok()
            });
        }
        return;
    }
    for block in document.nodes_mut() {
        if node_name(block) != CONTEXT_MENU {
            continue;
        }
        if let Some(children) = block.children_mut().as_mut() {
            remove_nodes_named(children, section);
        }
    }
}

fn is_palette_theme(node: &KdlNode) -> bool {
    node.children()
        .map(|children| {
            children
                .nodes()
                .iter()
                .all(|child| PALETTE_COLOURS.contains(&node_name(child)))
        })
        .unwrap_or(false)
}

fn style_declaration_node(theme: &ThemeEntry, style: &str) -> Option<KdlNode> {
    theme_node(theme).children().and_then(|children| {
        children
            .nodes()
            .iter()
            .find(|node| node_name(node) == style)
            .cloned()
    })
}

fn colour_node(theme: &ThemeEntry, style: &str, component: &str) -> Option<KdlNode> {
    style_declaration_node(theme, style)?
        .children()?
        .nodes()
        .iter()
        .find(|node| node_name(node) == component)
        .cloned()
}

fn apply_theme_colours(
    document: &mut KdlDocument,
    name: &str,
    theme: &ThemeEntry,
    slots: &[(&'static str, &'static str)],
) {
    let found = document.nodes_mut().iter_mut().rev().find_map(|block| {
        if node_name(block) != THEMES {
            return None;
        }
        block.children_mut().as_mut().and_then(|children| {
            children
                .nodes_mut()
                .iter_mut()
                .rfind(|n| node_name(n) == name)
        })
    });
    let theme_node_in_file = match found {
        Some(node) if !is_palette_theme(node) && node.children().is_some() => node,
        _ => {
            set_named_node(document, THEMES, name, Some(theme_node(theme)));
            return;
        },
    };
    let theme_indent = indent_of_leading(theme_node_in_file.leading()).unwrap_or_default();
    let Some(styles) = theme_node_in_file.children_mut().as_mut() else {
        return;
    };
    let styles_indent = child_indent(styles, Some(&theme_indent));
    let mut handled_styles: BTreeSet<&str> = BTreeSet::new();
    for (style, component) in slots {
        if handled_styles.contains(style) {
            continue;
        }
        let style_position = styles.nodes().iter().position(|n| node_name(n) == *style);
        let replacement = style_declaration_node(theme, style);
        match (style_position, replacement) {
            (None, Some(node)) => {
                push_node(styles, node, &styles_indent, Some(&theme_indent));
                handled_styles.insert(*style);
            },
            (Some(position), None) => {
                remove_node_at(styles, position);
                handled_styles.insert(*style);
            },
            (Some(position), Some(_)) => {
                let style_indent = node_indent(styles, position, Some(&theme_indent));
                let style_node = &mut styles.nodes_mut()[position];
                if style_node.children().is_none() {
                    style_node.set_children(KdlDocument::new());
                }
                let Some(components) = style_node.children_mut().as_mut() else {
                    continue;
                };
                let Some(new_colour) = colour_node(theme, style, component) else {
                    continue;
                };
                match components
                    .nodes_mut()
                    .iter_mut()
                    .find(|n| node_name(n) == *component)
                {
                    Some(existing) => {
                        let first_entry = existing.entries().first().cloned();
                        existing.clear_entries();
                        for (index, entry) in new_colour.entries().iter().enumerate() {
                            let mut entry = entry.clone();
                            if index == 0 {
                                if let Some(leading) =
                                    first_entry.as_ref().and_then(|e| e.leading())
                                {
                                    entry.set_leading(leading.to_owned());
                                }
                            }
                            existing.push(entry);
                        }
                    },
                    None => {
                        let component_indent = child_indent(components, Some(&style_indent));
                        push_node(
                            components,
                            new_colour,
                            &component_indent,
                            Some(&style_indent),
                        );
                    },
                }
            },
            (None, None) => {},
        }
    }
}

fn apply_theme_edit(document: &mut KdlDocument, name: &str, theme: Option<&ThemeEntry>) {
    set_named_node(document, THEMES, name, theme.map(theme_node));
    remove_empty_top_level_blocks(document, THEMES);
}

fn apply_block_edit(document: &mut KdlDocument, edit: &ConfigEdit) {
    match edit {
        ConfigEdit::PluginAlias { name, alias } => set_named_node(
            document,
            PLUGINS,
            name,
            alias.as_ref().map(plugin_alias_node),
        ),
        ConfigEdit::LoadPlugins { base, edits } => apply_load_plugins_edit(document, base, edits),
        ConfigEdit::EnvVar { name, value } => apply_env_edit(document, name, value.as_deref()),
        ConfigEdit::ContextMenuItems {
            section,
            base,
            edits,
        } => apply_context_menu_items(document, section, base, edits),
        ConfigEdit::ContextMenuDefaults { section, entries } => {
            apply_context_menu_defaults(document, section, entries)
        },
        ConfigEdit::Theme { name, theme } => apply_theme_edit(document, name, theme.as_ref()),
        ConfigEdit::ThemeColours { name, theme, slots } => {
            apply_theme_colours(document, name, theme, slots)
        },
        _ => {},
    }
}

pub fn apply_edits(
    file_contents: Option<&str>,
    edits: &[ConfigEdit],
    runtime_keybinds: &KeybindChanges,
) -> Result<String, String> {
    let mut document: KdlDocument = match file_contents {
        Some(text) if !text.trim().is_empty() => text
            .parse()
            .map_err(|e: kdl::KdlError| format!("The config file could not be read: {}", e))?,
        Some(text) => {
            let mut document = KdlDocument::new();
            document.set_leading("");
            document.set_trailing(text.to_owned());
            document
        },
        None => KdlDocument::new(),
    };
    for edit in edits {
        match edit {
            ConfigEdit::SetSetting { key, value } => {
                set_setting(&mut document, *key, value.clone())
            },
            ConfigEdit::RemoveSetting(key) => remove_setting(&mut document, *key),
            ConfigEdit::KeybindsAttribute { .. }
            | ConfigEdit::KeybindsClearDefaults(_)
            | ConfigEdit::ModeClearDefaults(_)
            | ConfigEdit::Key { .. } => apply_keybinds_edit(&mut document, edit, runtime_keybinds),
            _ => apply_block_edit(&mut document, edit),
        }
    }
    Ok(document.to_string())
}

fn normalized_keybinds(selection: &KeybindsSelection, options: &Options) -> KeybindsSelection {
    match selection.to_kdl() {
        Some(node) => {
            let text = node.to_string();
            text.parse::<KdlDocument>()
                .ok()
                .and_then(|document| document.get(KEYBINDS).cloned())
                .and_then(|node| KeybindsSelection::from_kdl(&node, options).ok())
                .unwrap_or_else(|| selection.clone())
        },
        None => KeybindsSelection::default(),
    }
}

pub fn parse_saved_file(file_contents: &str, runtime: &Config) -> Result<Config, String> {
    let defaults = Config::from_default_assets().map_err(|e| e.to_string())?;
    let mut parsed = Config::from_kdl(file_contents, Some(defaults)).map_err(|e| e.to_string())?;
    parsed.keybinds_layers.config_dir = runtime.keybinds_layers.config_dir.clone();
    parsed.keybinds_layers.keybinds_dir_override =
        runtime.keybinds_layers.keybinds_dir_override.clone();
    parsed.resolve_keybinds();
    Ok(parsed)
}

pub fn written_file_matches(
    file_contents: &str,
    runtime: &Config,
    edits: &[ConfigEdit],
) -> Result<(), String> {
    let parsed = parse_saved_file(file_contents, runtime)?;
    for edit in edits {
        block_edit_reads_back(&parsed, runtime, edit)?;
    }
    for key in edited_settings(edits) {
        if key.is_block() && key != SettingKey::Keybinds {
            continue;
        }
        if key == SettingKey::Keybinds {
            let expected = normalized_keybinds(&runtime.keybinds_layers.user, &parsed.options);
            if parsed.keybinds_layers.user != expected {
                return Err("the keybindings read back differ from the ones saved".to_owned());
            }
        } else if setting_value(&parsed, key) != setting_value(runtime, key) {
            return Err(format!("{} reads back differently", key));
        }
    }
    Ok(())
}

fn config_file_theme<'a>(config: &'a Config, name: &str) -> Option<&'a crate::data::Styling> {
    config
        .themes
        .get_theme(name)
        .filter(|theme| !theme.sourced_from_external_file)
        .map(|theme| &theme.palette)
}

fn block_edit_reads_back(
    parsed: &Config,
    runtime: &Config,
    edit: &ConfigEdit,
) -> Result<(), String> {
    let matches = match edit {
        ConfigEdit::PluginAlias { name, .. } => {
            parsed.plugins.aliases.get(name) == runtime.plugins.aliases.get(name)
        },
        ConfigEdit::LoadPlugins { .. } => {
            config_blocks(parsed, false, None).load_plugins
                == config_blocks(runtime, false, None).load_plugins
        },
        ConfigEdit::EnvVar { name, .. } => {
            parsed.env.inner().get(name) == runtime.env.inner().get(name)
        },
        ConfigEdit::ContextMenuItems { section, .. }
        | ConfigEdit::ContextMenuDefaults { section, .. } => {
            menu_section(&parsed.context_menu, section)
                == menu_section(&runtime.context_menu, section)
        },
        ConfigEdit::Theme { name, .. } | ConfigEdit::ThemeColours { name, .. } => {
            config_file_theme(parsed, name).map(styling_colours)
                == config_file_theme(runtime, name).map(styling_colours)
        },
        _ => true,
    };
    if matches {
        Ok(())
    } else {
        Err(format!(
            "{:?} reads back differently",
            edited_settings(&[edit.clone()])
        ))
    }
}

fn copy_block_edit(new_saved: &mut Config, runtime: &Config, edit: &ConfigEdit) {
    match edit {
        ConfigEdit::PluginAlias { name, .. } => match runtime.plugins.aliases.get(name) {
            Some(alias) => {
                new_saved
                    .plugins
                    .aliases
                    .insert(name.clone(), alias.clone());
            },
            None => {
                new_saved.plugins.aliases.remove(name);
            },
        },
        ConfigEdit::LoadPlugins { .. } => {
            new_saved.background_plugins = runtime.background_plugins.clone()
        },
        ConfigEdit::EnvVar { name, .. } => {
            let mut env = new_saved.env.inner().clone();
            match runtime.env.inner().get(name) {
                Some(value) => env.insert(name.clone(), value.clone()),
                None => env.remove(name),
            };
            new_saved.env = crate::envs::EnvironmentVariables::from_data(env);
        },
        ConfigEdit::ContextMenuItems { section, .. }
        | ConfigEdit::ContextMenuDefaults { section, .. } => {
            if let (Some(target), Some(source)) = (
                new_saved.context_menu.section_mut(section),
                runtime.context_menu.section(section),
            ) {
                *target = source.clone();
            }
        },
        ConfigEdit::Theme { name, .. } | ConfigEdit::ThemeColours { name, .. } => {
            match runtime.themes.get_theme(name) {
                Some(theme) => new_saved.themes.insert(name.clone(), theme.clone()),
                None => {
                    new_saved.themes.remove(name);
                },
            }
        },
        _ => {},
    }
}

pub fn saved_config_after_edits(saved: &Config, runtime: &Config, edits: &[ConfigEdit]) -> Config {
    let mut new_saved = saved.clone();
    for key in edited_settings(edits) {
        if !key.is_block() || key == SettingKey::Keybinds {
            copy_setting(&mut new_saved, runtime, key);
        }
    }
    for edit in edits {
        copy_block_edit(&mut new_saved, runtime, edit);
    }
    new_saved
}

pub fn read_config_file(path: &Path) -> Result<Option<String>, std::io::Error> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn restore_previous_file(path: &Path, previous: Option<&str>) {
    let restored = match previous {
        Some(contents) => std::fs::write(path, contents),
        None => std::fs::remove_file(path),
    };
    if let Err(e) = restored {
        log::error!("Failed to restore {}: {}", path.display(), e);
    }
}

fn back_up(path: &Path, contents: &str) -> Result<(), PathBuf> {
    let backup_path = Config::backup_file_path(path);
    std::fs::write(&backup_path, contents).map_err(|e| {
        log::error!("Failed to back up {}: {}", path.display(), e);
        backup_path.clone()
    })?;
    match std::fs::read_to_string(&backup_path) {
        Ok(backed_up) if backed_up == contents => Ok(()),
        _ => Err(backup_path),
    }
}

pub fn save_config_in_place(
    path: &Path,
    last_read: Option<&str>,
    saved: &Config,
    base: &Config,
    target: &Config,
    overwrite: bool,
) -> SaveOutcome {
    let current = match read_config_file(path) {
        Ok(current) => current,
        Err(e) => {
            log::error!("Failed to read {}: {}", path.display(), e);
            return SaveOutcome::Failed(Some(path.to_path_buf()));
        },
    };
    if !overwrite && current.as_deref() != last_read {
        return SaveOutcome::ChangedOutside;
    }
    let edits = edits_between(base, target);
    if edits.is_empty() {
        return SaveOutcome::Written {
            file_contents: current,
            saved_config: saved.clone(),
        };
    }
    let new_contents = match apply_edits(
        current.as_deref(),
        &edits,
        &target.keybinds_layers.user.changes,
    ) {
        Ok(new_contents) => new_contents,
        Err(e) => {
            log::error!("Failed to edit {}: {}", path.display(), e);
            return SaveOutcome::Failed(None);
        },
    };
    if let Some(current) = current.as_deref() {
        if let Err(backup_path) = back_up(path, current) {
            return SaveOutcome::Failed(Some(backup_path));
        }
    }
    if let Err(e) = std::fs::write(path, new_contents.as_bytes()) {
        log::error!("Failed to write {}: {}", path.display(), e);
        restore_previous_file(path, current.as_deref());
        return SaveOutcome::Failed(Some(path.to_path_buf()));
    }
    let check = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|written| {
            if written != new_contents {
                return Err("the file read back differs from what was written".to_owned());
            }
            written_file_matches(&written, target, &edits)
        });
    if let Err(e) = check {
        log::error!("The saved config did not read back correctly: {}", e);
        restore_previous_file(path, current.as_deref());
        return SaveOutcome::Failed(Some(path.to_path_buf()));
    }
    SaveOutcome::Written {
        file_contents: Some(new_contents),
        saved_config: saved_config_after_edits(saved, target, &edits),
    }
}

#[cfg(test)]
#[path = "./unit/config_file_edit_test.rs"]
mod config_file_edit_test;

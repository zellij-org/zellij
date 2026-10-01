use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use strum::IntoEnumIterator;

use super::actions::Action;
use super::config::Config;
use super::config_settings::{
    copy_setting, differing_settings, setting_is_default, setting_kdl_value, setting_node_path,
    setting_value,
};
use super::context_menu::{ContextMenuConfig, CONTEXT_MENU_SECTIONS};
use super::keybind_presets::{KeybindChanges, KeybindsSelection, LEADER_PLACEHOLDERS};
use super::keybinds::Keybinds;
use super::options::Options;
use crate::data::{ContextMenuEntry, InputMode, KeyWithModifier, SettingKey};

const KEYBINDS: &str = "keybinds";
const CONTEXT_MENU: &str = "context_menu";
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
    ContextMenuSection {
        section: &'static str,
        entries: Vec<ContextMenuEntry>,
    },
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

fn context_menu_edits(saved: &ContextMenuConfig, runtime: &ContextMenuConfig) -> Vec<ConfigEdit> {
    CONTEXT_MENU_SECTIONS
        .iter()
        .filter_map(|section| {
            let saved_entries = saved.section(section)?;
            let runtime_entries = runtime.section(section)?;
            if saved_entries != runtime_entries {
                Some(ConfigEdit::ContextMenuSection {
                    section,
                    entries: runtime_entries.clone(),
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn edits_between(saved: &Config, runtime: &Config) -> Vec<ConfigEdit> {
    let mut edits = vec![];
    for key in differing_settings(saved, runtime) {
        if key == SettingKey::Keybinds {
            edits.extend(keybind_edits(
                &saved.keybinds_layers.user,
                &runtime.keybinds_layers.user,
            ));
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
    edits.extend(context_menu_edits(
        &saved.context_menu,
        &runtime.context_menu,
    ));
    edits
}

pub fn edited_settings(edits: &[ConfigEdit]) -> BTreeSet<SettingKey> {
    edits
        .iter()
        .filter_map(|edit| match edit {
            ConfigEdit::SetSetting { key, .. } | ConfigEdit::RemoveSetting(key) => Some(*key),
            ConfigEdit::KeybindsAttribute { .. }
            | ConfigEdit::KeybindsClearDefaults(_)
            | ConfigEdit::ModeClearDefaults(_)
            | ConfigEdit::Key { .. } => Some(SettingKey::Keybinds),
            ConfigEdit::ContextMenuSection { .. } => None,
        })
        .collect()
}

fn edited_context_menu_sections(edits: &[ConfigEdit]) -> Vec<&'static str> {
    edits
        .iter()
        .filter_map(|edit| match edit {
            ConfigEdit::ContextMenuSection { section, .. } => Some(*section),
            _ => None,
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
    node.children_mut().as_mut().map(|children| (children, indent))
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
    match node.entries().iter().position(|entry| entry.name().is_none()) {
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
    node.children_mut().as_mut().expect("children were just set")
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

fn context_menu_section_node(section: &str, entries: &[ContextMenuEntry]) -> KdlNode {
    let mut menu = ContextMenuConfig::default();
    if let Some(target) = menu.section_mut(section) {
        *target = entries.to_vec();
    }
    let mut node = menu
        .to_kdl()
        .and_then(|menu_node| {
            menu_node.children().and_then(|children| {
                children
                    .nodes()
                    .iter()
                    .find(|node| node_name(node) == section)
                    .cloned()
            })
        })
        .unwrap_or_else(|| KdlNode::new(section));
    node.insert(CLEAR_DEFAULTS, true);
    node
}

fn apply_context_menu_edit(document: &mut KdlDocument, section: &str, entries: &[ContextMenuEntry]) {
    let Some((block, owner_indent)) = block_at_path(document, &[CONTEXT_MENU], true) else {
        return;
    };
    let replacement = context_menu_section_node(section, entries);
    match block
        .nodes()
        .iter()
        .position(|node| node_name(node) == section)
    {
        Some(position) => {
            let node = &mut block.nodes_mut()[position];
            set_property(node, CLEAR_DEFAULTS, Some(KdlValue::Bool(true)));
            match replacement.children() {
                Some(children) => node.set_children(children.clone()),
                None => node.clear_children(),
            }
        },
        None => {
            let indent = child_indent(block, owner_indent.as_deref());
            push_node(block, replacement, &indent, owner_indent.as_deref());
        },
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
            ConfigEdit::SetSetting { key, value } => set_setting(&mut document, *key, value.clone()),
            ConfigEdit::RemoveSetting(key) => remove_setting(&mut document, *key),
            ConfigEdit::ContextMenuSection { section, entries } => {
                apply_context_menu_edit(&mut document, section, entries)
            },
            _ => apply_keybinds_edit(&mut document, edit, runtime_keybinds),
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
    for key in edited_settings(edits) {
        if key == SettingKey::Keybinds {
            let expected = normalized_keybinds(&runtime.keybinds_layers.user, &parsed.options);
            if parsed.keybinds_layers.user != expected {
                return Err("the keybindings read back differ from the ones saved".to_owned());
            }
        } else if setting_value(&parsed, key) != setting_value(runtime, key) {
            return Err(format!("{} reads back differently", key));
        }
    }
    for section in edited_context_menu_sections(edits) {
        if parsed.context_menu.section(section) != runtime.context_menu.section(section) {
            return Err(format!("context_menu {} reads back differently", section));
        }
    }
    Ok(())
}

pub fn saved_config_after_edits(saved: &Config, runtime: &Config, edits: &[ConfigEdit]) -> Config {
    let mut new_saved = saved.clone();
    for key in edited_settings(edits) {
        copy_setting(&mut new_saved, runtime, key);
    }
    for section in edited_context_menu_sections(edits) {
        if let (Some(target), Some(source)) = (
            new_saved.context_menu.section_mut(section),
            runtime.context_menu.section(section),
        ) {
            *target = source.clone();
        }
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

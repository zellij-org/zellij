use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use strum::IntoEnumIterator;

use super::actions::Action;
use super::config::Config;
use super::config_file_edit::{key_state, KeyState};
use super::context_menu::{context_menu_shortcut, ContextMenuConfig, CONTEXT_MENU_SECTIONS};
use super::layout::{RunPlugin, RunPluginOrAlias};
use super::plugins::PluginAliases;
use super::theme::{TerminalColors, Theme, Themes};
use crate::data::{
    ConfigBlocks, ContextMenuAction, ContextMenuEntry, EnvVarEntry, InputMode, KeyWithModifier,
    KeybindingEntry, KeybindingSource, KeybindsVec, MenuItemEntry, MenuSectionEntries,
    MultiplayerColors, PaletteColor, PluginAliasEntry, PluginEntry, StyleDeclaration, Styling,
    ThemeEntry, ThemeSource, DEFAULT_STYLES,
};
use crate::envs::EnvironmentVariables;
use crate::home::get_theme_dir;
use crate::kdl::{context_menu_item_to_kdl, load_plugins_from_kdl, theme_to_kdl};

pub const THEME_STYLES: [&str; 14] = [
    "text_unselected",
    "text_selected",
    "ribbon_unselected",
    "ribbon_selected",
    "table_title",
    "table_cell_unselected",
    "table_cell_selected",
    "list_unselected",
    "list_selected",
    "frame_unselected",
    "frame_selected",
    "frame_highlight",
    "exit_code_success",
    "exit_code_error",
];
pub const STYLE_COMPONENTS: [&str; 6] = [
    "base",
    "background",
    "emphasis_0",
    "emphasis_1",
    "emphasis_2",
    "emphasis_3",
];
pub const MULTIPLAYER_COLOURS: &str = "multiplayer_user_colors";
pub const PLAYER_COLOURS: [&str; 10] = [
    "player_1",
    "player_2",
    "player_3",
    "player_4",
    "player_5",
    "player_6",
    "player_7",
    "player_8",
    "player_9",
    "player_10",
];
pub const SHARED_BLOCKS: [&str; 3] = ["shared", "shared_except", "shared_among"];

pub fn theme_slots() -> Vec<(&'static str, &'static str)> {
    let mut slots = vec![];
    for style in THEME_STYLES {
        for component in STYLE_COMPONENTS {
            slots.push((style, component));
        }
    }
    for player in PLAYER_COLOURS {
        slots.push((MULTIPLAYER_COLOURS, player));
    }
    slots
}

pub fn colour_text(colour: &PaletteColor) -> String {
    match colour {
        PaletteColor::Rgb((r, g, b)) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        PaletteColor::EightBit(index) => index.to_string(),
    }
}

fn hex_channel(text: &str) -> Option<u8> {
    if text.chars().all(|c| c.is_ascii_hexdigit()) {
        u8::from_str_radix(text, 16).ok()
    } else {
        None
    }
}

pub fn parse_colour(text: &str) -> Option<PaletteColor> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    if let Some(inner) = lower
        .strip_prefix("rgb(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return match parse_colour(inner)? {
            PaletteColor::Rgb(rgb) => Some(PaletteColor::Rgb(rgb)),
            PaletteColor::EightBit(_) => None,
        };
    }
    let bare_hex = text.len() == 6 && text.chars().all(|c| c.is_ascii_hexdigit());
    let hex = text
        .strip_prefix('#')
        .or(if bare_hex { Some(text) } else { None });
    if let Some(hex) = hex {
        if !hex.is_ascii() {
            return None;
        }
        return match hex.len() {
            3 => Some(PaletteColor::Rgb((
                hex_channel(&hex[0..1])? * 0x11,
                hex_channel(&hex[1..2])? * 0x11,
                hex_channel(&hex[2..3])? * 0x11,
            ))),
            6 => Some(PaletteColor::Rgb((
                hex_channel(&hex[0..2])?,
                hex_channel(&hex[2..4])?,
                hex_channel(&hex[4..6])?,
            ))),
            _ => None,
        };
    }
    let parts: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .collect();
    match parts.as_slice() {
        [index] => index.parse::<u8>().ok().map(PaletteColor::EightBit),
        [r, g, b] => Some(PaletteColor::Rgb((
            r.parse().ok()?,
            g.parse().ok()?,
            b.parse().ok()?,
        ))),
        _ => None,
    }
}

fn style_ref<'a>(styling: &'a Styling, style: &str) -> Option<&'a StyleDeclaration> {
    match style {
        "text_unselected" => Some(&styling.text_unselected),
        "text_selected" => Some(&styling.text_selected),
        "ribbon_unselected" => Some(&styling.ribbon_unselected),
        "ribbon_selected" => Some(&styling.ribbon_selected),
        "table_title" => Some(&styling.table_title),
        "table_cell_unselected" => Some(&styling.table_cell_unselected),
        "table_cell_selected" => Some(&styling.table_cell_selected),
        "list_unselected" => Some(&styling.list_unselected),
        "list_selected" => Some(&styling.list_selected),
        "frame_unselected" => styling.frame_unselected.as_ref(),
        "frame_selected" => Some(&styling.frame_selected),
        "frame_highlight" => Some(&styling.frame_highlight),
        "exit_code_success" => Some(&styling.exit_code_success),
        "exit_code_error" => Some(&styling.exit_code_error),
        _ => None,
    }
}

fn style_mut<'a>(styling: &'a mut Styling, style: &str) -> Option<&'a mut StyleDeclaration> {
    let frame_selected = styling.frame_selected;
    match style {
        "text_unselected" => Some(&mut styling.text_unselected),
        "text_selected" => Some(&mut styling.text_selected),
        "ribbon_unselected" => Some(&mut styling.ribbon_unselected),
        "ribbon_selected" => Some(&mut styling.ribbon_selected),
        "table_title" => Some(&mut styling.table_title),
        "table_cell_unselected" => Some(&mut styling.table_cell_unselected),
        "table_cell_selected" => Some(&mut styling.table_cell_selected),
        "list_unselected" => Some(&mut styling.list_unselected),
        "list_selected" => Some(&mut styling.list_selected),
        "frame_unselected" => Some(styling.frame_unselected.get_or_insert(frame_selected)),
        "frame_selected" => Some(&mut styling.frame_selected),
        "frame_highlight" => Some(&mut styling.frame_highlight),
        "exit_code_success" => Some(&mut styling.exit_code_success),
        "exit_code_error" => Some(&mut styling.exit_code_error),
        _ => None,
    }
}

fn component_mut<'a>(
    declaration: &'a mut StyleDeclaration,
    component: &str,
) -> Option<&'a mut PaletteColor> {
    match component {
        "base" => Some(&mut declaration.base),
        "background" => Some(&mut declaration.background),
        "emphasis_0" => Some(&mut declaration.emphasis_0),
        "emphasis_1" => Some(&mut declaration.emphasis_1),
        "emphasis_2" => Some(&mut declaration.emphasis_2),
        "emphasis_3" => Some(&mut declaration.emphasis_3),
        _ => None,
    }
}

fn player_mut<'a>(
    colours: &'a mut MultiplayerColors,
    player: &str,
) -> Option<&'a mut PaletteColor> {
    match player {
        "player_1" => Some(&mut colours.player_1),
        "player_2" => Some(&mut colours.player_2),
        "player_3" => Some(&mut colours.player_3),
        "player_4" => Some(&mut colours.player_4),
        "player_5" => Some(&mut colours.player_5),
        "player_6" => Some(&mut colours.player_6),
        "player_7" => Some(&mut colours.player_7),
        "player_8" => Some(&mut colours.player_8),
        "player_9" => Some(&mut colours.player_9),
        "player_10" => Some(&mut colours.player_10),
        _ => None,
    }
}

pub fn slot_colour(styling: &Styling, style: &str, component: &str) -> Option<PaletteColor> {
    if style == MULTIPLAYER_COLOURS {
        let mut colours = styling.multiplayer_user_colors;
        return player_mut(&mut colours, component).copied();
    }
    let mut declaration = *style_ref(styling, style)?;
    component_mut(&mut declaration, component).copied()
}

pub fn set_slot_colour(styling: &mut Styling, style: &str, component: &str, colour: PaletteColor) {
    let target = if style == MULTIPLAYER_COLOURS {
        player_mut(&mut styling.multiplayer_user_colors, component)
    } else {
        style_mut(styling, style).and_then(|declaration| component_mut(declaration, component))
    };
    if let Some(target) = target {
        *target = colour;
    }
}

pub fn styling_colours(styling: &Styling) -> Vec<String> {
    theme_slots()
        .iter()
        .map(|(style, component)| {
            slot_colour(styling, style, component)
                .map(|colour| colour_text(&colour))
                .unwrap_or_default()
        })
        .collect()
}

pub fn styling_from_colours(colours: &[String]) -> Styling {
    let mut styling: Styling = DEFAULT_STYLES;
    styling.frame_unselected = None;
    for ((style, component), text) in theme_slots().iter().zip(colours.iter()) {
        if let Some(colour) = parse_colour(text) {
            set_slot_colour(&mut styling, style, component, colour);
        }
    }
    styling
}

fn config_value(value: &str) -> KdlValue {
    match value {
        "true" => KdlValue::Bool(true),
        "false" => KdlValue::Bool(false),
        other => KdlValue::String(other.to_owned()),
    }
}

fn plugin_children(entry: &PluginEntry) -> Option<KdlDocument> {
    let mut children = KdlDocument::new();
    if let Some(cwd) = &entry.cwd {
        let mut cwd_node = KdlNode::new("cwd");
        cwd_node.push(cwd.clone());
        children.nodes_mut().push(cwd_node);
    }
    for (key, value) in &entry.configuration {
        let mut node = KdlNode::new(key.as_str());
        node.push(KdlEntry::new(config_value(value)));
        children.nodes_mut().push(node);
    }
    if children.nodes().is_empty() {
        None
    } else {
        Some(children)
    }
}

pub fn plugin_alias_node(alias: &PluginAliasEntry) -> KdlNode {
    let mut node = KdlNode::new(alias.name.as_str());
    node.insert("location", alias.plugin.location.clone());
    if let Some(children) = plugin_children(&alias.plugin) {
        node.set_children(children);
    }
    node
}

pub fn load_plugin_node(entry: &PluginEntry) -> KdlNode {
    let mut node = KdlNode::new(entry.location.as_str());
    node.name_mut()
        .set_repr(KdlValue::String(entry.location.clone()).to_string());
    if let Some(children) = plugin_children(entry) {
        node.set_children(children);
    }
    node
}

pub fn env_node(entry: &EnvVarEntry) -> KdlNode {
    let mut node = KdlNode::new(entry.name.as_str());
    node.push(entry.value.clone());
    node
}

fn bare_node(mut node: KdlNode) -> KdlNode {
    node.set_leading("");
    node.set_trailing("\n");
    node
}

pub fn action_nodes(actions: &[String]) -> Result<Vec<KdlNode>, String> {
    let mut nodes = vec![];
    for text in actions {
        let document: KdlDocument = text
            .parse()
            .map_err(|e: kdl::KdlError| format!("{}: {}", text, e))?;
        for node in document.nodes() {
            nodes.push(bare_node(node.clone()));
        }
    }
    Ok(nodes)
}

pub fn menu_item_node(entry: &MenuItemEntry) -> Result<KdlNode, String> {
    match &entry.label {
        None => Ok(KdlNode::new("separator")),
        Some(label) => Ok(context_menu_item_to_kdl(
            label,
            action_nodes(&entry.actions)?,
        )),
    }
}

pub fn theme_node(theme: &ThemeEntry) -> KdlNode {
    theme_to_kdl(&theme.name, &styling_from_colours(&theme.colours))
}

fn block_text(name: &str, nodes: Vec<KdlNode>) -> String {
    let mut block = KdlNode::new(name);
    let mut children = KdlDocument::new();
    for node in nodes {
        children.nodes_mut().push(node);
    }
    block.set_children(children);
    block.to_string()
}

pub fn plugin_aliases_kdl(aliases: &[PluginAliasEntry]) -> String {
    block_text("plugins", aliases.iter().map(plugin_alias_node).collect())
}

pub fn load_plugins_kdl(entries: &[PluginEntry]) -> String {
    block_text(
        "load_plugins",
        entries.iter().map(load_plugin_node).collect(),
    )
}

pub fn env_kdl(entries: &[EnvVarEntry]) -> String {
    block_text("env", entries.iter().map(env_node).collect())
}

pub fn menu_section_kdl(section: &str, entries: &[MenuItemEntry]) -> Result<String, String> {
    let mut section_node = KdlNode::new(section);
    section_node.insert("clear-defaults", true);
    let mut children = KdlDocument::new();
    for entry in entries {
        children.nodes_mut().push(menu_item_node(entry)?);
    }
    section_node.set_children(children);
    Ok(block_text("context_menu", vec![section_node]))
}

pub fn themes_kdl(themes: &[ThemeEntry]) -> String {
    block_text(
        "themes",
        themes
            .iter()
            .filter(|theme| theme.source == ThemeSource::ConfigFile)
            .map(theme_node)
            .collect(),
    )
}

pub fn keybind_change_kdl(
    mode: InputMode,
    key: &KeyWithModifier,
    actions: Option<&[String]>,
) -> Result<String, String> {
    let statement = match actions {
        Some(actions) => {
            let mut bind = KdlNode::new("bind");
            bind.push(key.to_kdl());
            let nodes = action_nodes(actions)?;
            let mut children = KdlDocument::new();
            let has_children = nodes.iter().any(|node| node.children().is_some());
            for node in nodes {
                children.nodes_mut().push(node);
            }
            if !has_children {
                for node in children.nodes_mut() {
                    node.set_trailing("; ");
                }
                children.set_leading(" ");
                children.set_trailing("");
            }
            bind.set_children(children);
            bind
        },
        None => {
            let mut unbind = KdlNode::new("unbind");
            unbind.push(key.to_kdl());
            unbind
        },
    };
    let text = format!(
        "keybinds {{\n    {} {{\n        {}\n    }}\n}}\n",
        format!("{:?}", mode).to_lowercase(),
        statement.to_string().trim()
    );
    text.parse::<KdlDocument>()
        .map_err(|e: kdl::KdlError| e.to_string())?;
    Ok(text)
}

pub fn action_text(action: &Action) -> String {
    match action.to_kdl() {
        Some(node) => bare_node(node).to_string().trim().to_owned(),
        None => {
            let debug = format!("{:?}", action);
            debug
                .split(|c: char| c == ' ' || c == '{' || c == '(')
                .next()
                .unwrap_or("")
                .to_owned()
        },
    }
}

pub fn action_texts(actions: &[Action]) -> Vec<String> {
    actions.iter().map(action_text).collect()
}

pub fn menu_action_texts(actions: &[ContextMenuAction]) -> Vec<String> {
    actions
        .iter()
        .map(|action| match action {
            ContextMenuAction::Action(action) => action_text(action),
            other => other
                .to_kdl()
                .map(|node| bare_node(node).to_string().trim().to_owned())
                .unwrap_or_default(),
        })
        .collect()
}

fn configuration_pairs(configuration: &BTreeMap<String, String>) -> Vec<(String, String)> {
    configuration
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

pub fn run_plugin_entry(run_plugin: &RunPlugin) -> PluginEntry {
    PluginEntry {
        location: run_plugin.location.display(),
        cwd: run_plugin
            .initial_cwd
            .as_ref()
            .map(|cwd| cwd.display().to_string()),
        configuration: configuration_pairs(run_plugin.configuration.inner()),
    }
}

pub fn load_plugin_entry(plugin: &RunPluginOrAlias) -> PluginEntry {
    let (cwd, configuration) = match plugin {
        RunPluginOrAlias::RunPlugin(run_plugin) => (
            run_plugin.initial_cwd.clone(),
            configuration_pairs(run_plugin.configuration.inner()),
        ),
        RunPluginOrAlias::Alias(alias) => (
            alias.initial_cwd.clone(),
            alias
                .configuration
                .as_ref()
                .map(|configuration| configuration_pairs(configuration.inner()))
                .unwrap_or_default(),
        ),
    };
    PluginEntry {
        location: plugin.location_string(),
        cwd: cwd.map(|cwd: PathBuf| cwd.display().to_string()),
        configuration,
    }
}

pub fn menu_entries(
    entries: &[ContextMenuEntry],
    shortcuts: Option<(&KeybindsVec, InputMode)>,
) -> Vec<MenuItemEntry> {
    entries
        .iter()
        .map(|entry| match entry {
            ContextMenuEntry::Separator => MenuItemEntry::separator(),
            ContextMenuEntry::Item { label, actions } => MenuItemEntry {
                label: Some(label.clone()),
                actions: menu_action_texts(actions),
                shortcut: shortcuts.and_then(|(keybinds, base_mode)| {
                    context_menu_shortcut(keybinds, base_mode, actions)
                }),
            },
        })
        .collect()
}

pub fn built_in_themes() -> &'static Themes {
    static BUILT_IN_THEMES: std::sync::OnceLock<Themes> = std::sync::OnceLock::new();
    BUILT_IN_THEMES.get_or_init(crate::setup::get_default_themes)
}

pub fn copy_config_file_themes(target: &mut Themes, source: &Themes) {
    let removed: Vec<String> = target
        .inner()
        .iter()
        .filter(|(_, theme)| !theme.sourced_from_external_file)
        .map(|(name, _)| name.clone())
        .collect();
    target.retain(|_, theme| theme.sourced_from_external_file);
    for name in removed {
        if let Some(built_in) = built_in_themes().get_theme(&name) {
            target.insert(name, built_in.clone());
        }
    }
    for (name, theme) in source.inner() {
        if !theme.sourced_from_external_file {
            target.insert(name.clone(), theme.clone());
        }
    }
}

pub fn theme_source(name: &str, theme: &Theme) -> ThemeSource {
    if !theme.sourced_from_external_file {
        return ThemeSource::ConfigFile;
    }
    match built_in_themes().get_theme(name) {
        Some(built_in) if built_in.palette == theme.palette => ThemeSource::BuiltIn,
        _ => ThemeSource::ThemeFolder,
    }
}

pub fn theme_entries(
    themes: &Themes,
    include_external: bool,
    theme_dir: Option<&Path>,
) -> Vec<ThemeEntry> {
    let files = match theme_dir {
        Some(dir) if include_external => theme_files(dir),
        _ => BTreeMap::new(),
    };
    let sorted: BTreeMap<&String, &Theme> = themes.inner().iter().collect();
    sorted
        .into_iter()
        .filter(|(_, theme)| include_external || !theme.sourced_from_external_file)
        .map(|(name, theme)| {
            let source = theme_source(name, theme);
            ThemeEntry {
                name: name.clone(),
                source,
                colours: if source == ThemeSource::BuiltIn {
                    vec![]
                } else {
                    styling_colours(&theme.palette)
                },
                file_path: if source == ThemeSource::ThemeFolder {
                    files.get(name).map(|path| path.display().to_string())
                } else {
                    None
                },
            }
        })
        .collect()
}

pub fn theme_dir(config: &Config) -> Option<PathBuf> {
    config
        .options
        .theme_dir
        .clone()
        .or_else(|| get_theme_dir(config.keybinds_config_dir()))
}

pub fn theme_files(dir: &Path) -> BTreeMap<String, PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file() && path.extension().map(|e| e == "kdl").unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    let mut files = BTreeMap::new();
    for path in paths {
        if let Ok(themes) = Themes::from_path(path.clone()) {
            for name in themes.inner().keys() {
                files.insert(name.clone(), path.clone());
            }
        }
    }
    files
}

pub fn is_usable_theme_file_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

fn check_theme_text(text: &str, name: &str, path: &Path) -> Result<(), String> {
    let themes = Themes::from_string(&text.to_owned(), true)
        .map_err(|e| format!("Could not write a valid theme to {}: {}", path.display(), e))?;
    if themes.get_theme(name).is_none() {
        return Err(format!(
            "Could not write the theme {} to {}",
            name,
            path.display()
        ));
    }
    Ok(())
}

pub fn create_theme_file(
    dir: &Path,
    name: &str,
    palette: &Styling,
    terminal_colors: Option<&TerminalColors>,
) -> Result<PathBuf, String> {
    if !is_usable_theme_file_name(name) {
        return Err(format!(
            "{} cannot be a file name; use only letters, digits, - and _",
            name
        ));
    }
    if dir.exists() && !dir.is_dir() {
        return Err(format!("{} exists but is not a folder", dir.display()));
    }
    let target = dir.join(format!("{}.kdl", name));
    if target.exists() {
        return Err(format!("{} already exists", target.display()));
    }
    let mut themes_node = KdlNode::new("themes");
    let mut children = KdlDocument::new();
    let mut theme_node = theme_to_kdl(name, palette);
    if let Some(terminal_colors) = terminal_colors {
        theme_node
            .ensure_children()
            .nodes_mut()
            .push(terminal_colors.to_kdl());
    }
    children.nodes_mut().push(theme_node);
    themes_node.set_children(children);
    let mut document = KdlDocument::new();
    document.nodes_mut().push(themes_node);
    document.fmt();
    let text = document.to_string();
    check_theme_text(&text, name, &target)?;
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Could not create {}: {}", dir.display(), e))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => format!("{} already exists", target.display()),
            _ => format!("Could not create {}: {}", target.display(), e),
        })?;
    std::io::Write::write_all(&mut file, text.as_bytes())
        .map_err(|e| format!("Could not write {}: {}", target.display(), e))?;
    Ok(target)
}

pub fn delete_theme_from_file(path: &Path, name: &str) -> Result<bool, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read {}: {}", path.display(), e))?;
    let mut document: KdlDocument = text
        .parse()
        .map_err(|e: kdl::KdlError| format!("Could not read {}: {}", path.display(), e))?;
    let missing = || format!("{} does not define the theme {}", path.display(), name);
    let themes_node = document
        .nodes_mut()
        .iter_mut()
        .find(|node| node.name().value() == "themes")
        .ok_or_else(missing)?;
    let children = themes_node.children_mut().as_mut().ok_or_else(missing)?;
    let before = children.nodes().len();
    children
        .nodes_mut()
        .retain(|node| node.name().value() != name);
    let themes_left = children.nodes().len();
    if themes_left == before {
        return Err(missing());
    }
    if themes_left == 0 && document.nodes().len() == 1 {
        std::fs::remove_file(path)
            .map_err(|e| format!("Could not delete {}: {}", path.display(), e))?;
        return Ok(true);
    }
    document.fmt();
    std::fs::write(path, document.to_string())
        .map_err(|e| format!("Could not write {}: {}", path.display(), e))?;
    Ok(false)
}

pub fn update_theme_file(path: &Path, name: &str, palette: &Styling) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read {}: {}", path.display(), e))?;
    let mut document: KdlDocument = text
        .parse()
        .map_err(|e: kdl::KdlError| format!("Could not read {}: {}", path.display(), e))?;
    let missing = || format!("{} does not define the theme {}", path.display(), name);
    let themes_node = document
        .nodes_mut()
        .iter_mut()
        .find(|node| node.name().value() == "themes")
        .ok_or_else(missing)?;
    let theme_node = themes_node
        .children_mut()
        .as_mut()
        .and_then(|children| {
            children
                .nodes_mut()
                .iter_mut()
                .find(|node| node.name().value() == name)
        })
        .ok_or_else(missing)?;
    let terminal_colors = theme_node
        .children()
        .and_then(|children| children.get("terminal_colors"))
        .cloned();
    *theme_node = theme_to_kdl(name, palette);
    if let Some(terminal_colors) = terminal_colors {
        theme_node.ensure_children().nodes_mut().push(terminal_colors);
    }
    document.fmt();
    let new_text = document.to_string();
    check_theme_text(&new_text, name, path)?;
    std::fs::write(path, new_text).map_err(|e| format!("Could not write {}: {}", path.display(), e))
}

pub fn config_blocks(
    config: &Config,
    include_external_themes: bool,
    shortcuts: Option<(&KeybindsVec, InputMode)>,
) -> ConfigBlocks {
    let mut env: Vec<EnvVarEntry> = config
        .env
        .inner()
        .iter()
        .map(|(name, value)| EnvVarEntry {
            name: name.clone(),
            value: value.clone(),
        })
        .collect();
    env.sort_by(|a, b| a.name.cmp(&b.name));
    ConfigBlocks {
        plugin_aliases: config
            .plugins
            .aliases
            .iter()
            .map(|(name, run_plugin)| PluginAliasEntry {
                name: name.clone(),
                plugin: run_plugin_entry(run_plugin),
            })
            .collect(),
        load_plugins: config
            .background_plugins
            .iter()
            .map(load_plugin_entry)
            .collect(),
        env,
        context_menu: CONTEXT_MENU_SECTIONS
            .iter()
            .map(|section| MenuSectionEntries {
                section: section.to_string(),
                entries: config
                    .context_menu
                    .section(section)
                    .map(|entries| menu_entries(entries, shortcuts))
                    .unwrap_or_default(),
            })
            .collect(),
        themes: theme_entries(
            &config.themes,
            include_external_themes,
            theme_dir(config).as_deref(),
        ),
    }
}

fn has_child_nodes(node: &KdlNode) -> bool {
    node.children()
        .map(|children| !children.nodes().is_empty())
        .unwrap_or(false)
}

pub fn replace_config_blocks(config: &mut Config, kdl: &str) -> Result<(), String> {
    let document: KdlDocument = kdl.parse().map_err(|e: kdl::KdlError| e.to_string())?;
    for node in document.nodes() {
        match node.name().value() {
            "plugins" => {
                config.plugins = PluginAliases::from_kdl(node).map_err(|e| e.to_string())?;
            },
            "load_plugins" => {
                config.background_plugins =
                    load_plugins_from_kdl(node).map_err(|e| e.to_string())?;
            },
            "env" => {
                config.env = if has_child_nodes(node) {
                    EnvironmentVariables::from_kdl(node).map_err(|e| e.to_string())?
                } else {
                    EnvironmentVariables::default()
                };
            },
            "themes" => {
                let replacement = if has_child_nodes(node) {
                    Themes::from_kdl(node, false).map_err(|e| e.to_string())?
                } else {
                    Themes::default()
                };
                copy_config_file_themes(&mut config.themes, &replacement);
            },
            "context_menu" => {
                config.context_menu = ContextMenuConfig::from_kdl(
                    node,
                    std::mem::take(&mut config.context_menu),
                    &config.options,
                )
                .map_err(|e| e.to_string())?;
            },
            other => return Err(format!("The {} block cannot be replaced", other)),
        }
    }
    Ok(())
}

fn string_arguments(node: &KdlNode) -> Vec<String> {
    node.entries()
        .iter()
        .filter(|entry| entry.name().is_none())
        .filter_map(|entry| entry.value().as_string().map(|text| text.to_owned()))
        .collect()
}

fn shared_block_modes(node: &KdlNode) -> Vec<InputMode> {
    let listed: Vec<InputMode> = string_arguments(node)
        .iter()
        .filter_map(|name| InputMode::from_str(name).ok())
        .collect();
    InputMode::iter()
        .filter(|mode| match node.name().value() {
            "shared_among" => listed.contains(mode),
            _ => !listed.contains(mode),
        })
        .collect()
}

fn shared_block_label(node: &KdlNode) -> String {
    let mut label = node.name().value().to_owned();
    for argument in string_arguments(node) {
        label.push_str(&format!(" \"{}\"", argument));
    }
    label
}

fn statement_keys(block: &KdlDocument) -> Vec<KeyWithModifier> {
    block
        .nodes()
        .iter()
        .filter(|node| matches!(node.name().value(), "bind" | "unbind"))
        .flat_map(|node| string_arguments(node))
        .filter_map(|text| KeyWithModifier::from_str(&text).ok())
        .collect()
}

pub fn shared_block_names(
    file_contents: Option<&str>,
) -> BTreeMap<(InputMode, KeyWithModifier), Option<String>> {
    let mut names = BTreeMap::new();
    let Some(document) = file_contents.and_then(|text| text.parse::<KdlDocument>().ok()) else {
        return names;
    };
    let Some(keybinds) = document.get("keybinds").and_then(|node| node.children()) else {
        return names;
    };
    for node in keybinds.nodes() {
        if !SHARED_BLOCKS.contains(&node.name().value()) {
            continue;
        }
        let Some(children) = node.children() else {
            continue;
        };
        let label = shared_block_label(node);
        for mode in shared_block_modes(node) {
            for key in statement_keys(children) {
                names.insert((mode, key), Some(label.clone()));
            }
        }
    }
    for node in keybinds.nodes() {
        let Ok(mode) = InputMode::from_str(node.name().value()) else {
            continue;
        };
        if let Some(children) = node.children() {
            for key in statement_keys(children) {
                names.insert((mode, key), None);
            }
        }
    }
    names
}

pub fn keybinding_entries(
    saved: &Config,
    current: &Config,
    file_contents: Option<&str>,
) -> Vec<KeybindingEntry> {
    let preset = current.preset_keybinds();
    let user = &current.keybinds_layers.user.changes;
    let saved_user = &saved.keybinds_layers.user.changes;
    let layout = &current.keybinds_layers.layout.changes;
    let shared = shared_block_names(file_contents);
    let mut entries = vec![];
    for mode in InputMode::iter() {
        let resolved = current.keybinds.0.get(&mode);
        let preset_mode = preset.0.get(&mode);
        let mut keys: BTreeSet<KeyWithModifier> = resolved
            .map(|keys| keys.keys().cloned().collect())
            .unwrap_or_default();
        if let Some(mode_changes) = user.modes.get(&mode) {
            keys.extend(mode_changes.unbind.iter().cloned());
        }
        for key in keys {
            let user_state = key_state(user, mode, &key);
            let unsaved = key_state(saved_user, mode, &key) != user_state;
            let preset_actions = preset_mode
                .and_then(|keys| keys.get(&key))
                .map(|actions| action_texts(actions));
            let user_source = match shared.get(&(mode, key.clone())) {
                Some(Some(block)) => KeybindingSource::Shared(block.clone()),
                _ => KeybindingSource::User,
            };
            match resolved.and_then(|keys| keys.get(&key)) {
                Some(actions) => {
                    let source = if !matches!(key_state(layout, mode, &key), KeyState::Untouched) {
                        KeybindingSource::Layout
                    } else if matches!(user_state, KeyState::Bound(_)) || preset_actions.is_none() {
                        user_source
                    } else {
                        KeybindingSource::Preset
                    };
                    entries.push(KeybindingEntry {
                        mode,
                        key,
                        actions: action_texts(actions),
                        source,
                        unbound: false,
                        preset_actions,
                        unsaved,
                    });
                },
                None => {
                    if user_state == KeyState::Unbound {
                        entries.push(KeybindingEntry {
                            mode,
                            key,
                            actions: vec![],
                            source: user_source,
                            unbound: true,
                            preset_actions,
                            unsaved,
                        });
                    }
                },
            }
        }
    }
    entries
}

#[cfg(test)]
#[path = "./unit/config_blocks_test.rs"]
mod config_blocks_test;

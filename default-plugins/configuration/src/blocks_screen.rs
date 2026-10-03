use std::collections::BTreeMap;

use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::{
    env_kdl, load_plugins_kdl, menu_section_kdl, plugin_aliases_kdl,
};

use crate::list_editor::{EntryKind, FieldSpec, FieldValue, ListEditor, Section, SectionedList};
use crate::page::{actions_summary, is_plain, is_shift_tab, print_dim, Effect, Page, PageResponse};

fn text_field(label: &'static str, placeholder: &'static str) -> FieldSpec {
    FieldSpec::text(label, placeholder)
}

fn pairs_field(label: &'static str) -> FieldSpec {
    FieldSpec::pairs(label)
}

pub fn check_plugin_location(text: &str, allow_alias: bool) -> Result<String, String> {
    let text = text.trim();
    let error = if allow_alias {
        "Use a plugin URL (https://…), zellij:name, file:/path, or an alias name"
    } else {
        "Use a plugin URL (https://…), zellij:name or file:/path"
    };
    if text.is_empty() || text.chars().any(|c| c.is_control()) {
        return Err(error.to_owned());
    }
    if let Some(name) = text.strip_prefix("zellij:") {
        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err(error.to_owned());
        }
        return Ok(text.to_owned());
    }
    if let Some(path) = text.strip_prefix("file:") {
        if path.is_empty() {
            return Err(error.to_owned());
        }
        return Ok(text.to_owned());
    }
    if text.starts_with("https://") || text.starts_with("http://") {
        if text.len() <= "https://".len() || text.contains(char::is_whitespace) {
            return Err(error.to_owned());
        }
        return Ok(text.to_owned());
    }
    if text.starts_with('/') || text.starts_with('~') {
        return Ok(format!("file:{}", text));
    }
    if allow_alias
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Ok(text.to_owned());
    }
    Err(error.to_owned())
}

pub fn check_name(text: &str, what: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(format!("A {} is needed", what));
    }
    if !text
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("A {} can use letters, digits, '-' and '_'", what));
    }
    Ok(text.to_owned())
}

pub fn check_env_name(text: &str) -> Result<String, String> {
    let text = text.trim();
    let mut characters = text.chars();
    let valid = match characters.next() {
        Some(first) => {
            (first.is_ascii_alphabetic() || first == '_')
                && characters.all(|c| c.is_ascii_alphanumeric() || c == '_')
        },
        None => false,
    };
    if valid {
        Ok(text.to_owned())
    } else {
        Err(
            "A variable name starts with a letter or '_' and uses letters, digits and '_'"
                .to_owned(),
        )
    }
}

fn check_path(text: &str) -> Result<Option<String>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if text.chars().any(|c| c.is_control()) {
        return Err("A path cannot contain control characters".to_owned());
    }
    Ok(Some(text.to_owned()))
}

fn check_pairs(pairs: Vec<(String, String)>) -> Result<Vec<(String, String)>, String> {
    let mut checked: Vec<(String, String)> = vec![];
    for (key, value) in pairs {
        let key = check_name(&key, "config key")?;
        if checked.iter().any(|(existing, _)| *existing == key) {
            return Err(format!("{} is set twice", key));
        }
        checked.push((key, value));
    }
    checked.sort();
    Ok(checked)
}

fn plugin_details(entry: &PluginEntry) -> String {
    let mut details = vec![];
    if let Some(cwd) = &entry.cwd {
        details.push(format!("in {}", cwd));
    }
    if !entry.configuration.is_empty() {
        details.push(
            entry
                .configuration
                .iter()
                .map(|(key, value)| format!("{}={}", key, value))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    details.join(" · ")
}

fn plugin_summary(entry: &PluginEntry) -> String {
    let mut summary = entry.location.clone();
    if let Some(cwd) = &entry.cwd {
        summary.push_str(&format!(" (in {})", cwd));
    }
    if !entry.configuration.is_empty() {
        summary.push_str(&format!(
            " {{ {} }}",
            entry
                .configuration
                .iter()
                .map(|(key, value)| format!("{}={}", key, value))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    summary
}

pub struct AliasKind;

impl EntryKind for AliasKind {
    type Entry = PluginAliasEntry;
    fn title(&self) -> String {
        "Plugin aliases".to_owned()
    }
    fn heading_note(&self) -> Option<String> {
        Some("⟳ applies after a restart".to_owned())
    }
    fn noun(&self) -> &'static str {
        "plugin alias"
    }
    fn identity(&self, entry: &PluginAliasEntry) -> String {
        entry.name.clone()
    }
    fn columns(&self, entry: &PluginAliasEntry) -> Vec<String> {
        vec![
            entry.name.clone(),
            entry.plugin.location.clone(),
            plugin_details(&entry.plugin),
        ]
    }
    fn summary(&self, entry: &PluginAliasEntry) -> String {
        format!("{} → {}", entry.name, plugin_summary(&entry.plugin))
    }
    fn fields(&self) -> Vec<FieldSpec> {
        vec![
            text_field("Name", "my-plugin"),
            text_field("Location", "zellij:name, file:/path or https://…"),
            text_field("Folder", "optional"),
            pairs_field("Config"),
        ]
    }
    fn to_values(&self, entry: &PluginAliasEntry) -> Vec<FieldValue> {
        vec![
            FieldValue::Text(entry.name.clone()),
            FieldValue::Text(entry.plugin.location.clone()),
            FieldValue::Text(entry.plugin.cwd.clone().unwrap_or_default()),
            FieldValue::Pairs(entry.plugin.configuration.clone()),
        ]
    }
    fn from_values(&self, values: &[FieldValue]) -> Result<PluginAliasEntry, String> {
        Ok(PluginAliasEntry {
            name: check_name(&values[0].text(), "name")?,
            plugin: PluginEntry {
                location: check_plugin_location(&values[1].text(), false)?,
                cwd: check_path(&values[2].text())?,
                configuration: check_pairs(values[3].pairs())?,
            },
        })
    }
    fn block_kdl(&self, entries: &[PluginAliasEntry]) -> Result<String, String> {
        Ok(plugin_aliases_kdl(entries))
    }
    fn block_key(&self) -> SettingKey {
        SettingKey::PluginAliases
    }
    fn entries(&self, blocks: &ConfigBlocks) -> Vec<PluginAliasEntry> {
        blocks.plugin_aliases.clone()
    }
    fn can_delete(
        &self,
        entry: &PluginAliasEntry,
        defaults: &[PluginAliasEntry],
    ) -> Result<(), String> {
        if defaults.iter().any(|default| default.name == entry.name) {
            Err(format!(
                "{} is built in and cannot be removed; edit it, or press r to undo your change",
                entry.name
            ))
        } else {
            Ok(())
        }
    }
}

pub struct LoadPluginKind;

impl EntryKind for LoadPluginKind {
    type Entry = PluginEntry;
    fn title(&self) -> String {
        "Plugins loaded at start (load_plugins)".to_owned()
    }
    fn short_title(&self) -> String {
        "Plugins loaded at start".to_owned()
    }
    fn noun(&self) -> &'static str {
        "plugin"
    }
    fn identity(&self, entry: &PluginEntry) -> String {
        entry.location.clone()
    }
    fn columns(&self, entry: &PluginEntry) -> Vec<String> {
        vec![entry.location.clone(), plugin_details(entry)]
    }
    fn summary(&self, entry: &PluginEntry) -> String {
        plugin_summary(entry)
    }
    fn fields(&self) -> Vec<FieldSpec> {
        vec![
            text_field("Plugin", "alias, zellij:name, file:/path or https://…"),
            text_field("Folder", "optional"),
            pairs_field("Config"),
        ]
    }
    fn to_values(&self, entry: &PluginEntry) -> Vec<FieldValue> {
        vec![
            FieldValue::Text(entry.location.clone()),
            FieldValue::Text(entry.cwd.clone().unwrap_or_default()),
            FieldValue::Pairs(entry.configuration.clone()),
        ]
    }
    fn from_values(&self, values: &[FieldValue]) -> Result<PluginEntry, String> {
        Ok(PluginEntry {
            location: check_plugin_location(&values[0].text(), true)?,
            cwd: check_path(&values[1].text())?,
            configuration: check_pairs(values[2].pairs())?,
        })
    }
    fn block_kdl(&self, entries: &[PluginEntry]) -> Result<String, String> {
        Ok(load_plugins_kdl(entries))
    }
    fn block_key(&self) -> SettingKey {
        SettingKey::LoadPlugins
    }
    fn entries(&self, blocks: &ConfigBlocks) -> Vec<PluginEntry> {
        blocks.load_plugins.clone()
    }
    fn ordered(&self) -> bool {
        true
    }
    fn restart_only(&self) -> bool {
        true
    }
}

pub struct EnvKind;

impl EntryKind for EnvKind {
    type Entry = EnvVarEntry;
    fn title(&self) -> String {
        "Environment variables for new panes (env)".to_owned()
    }
    fn short_title(&self) -> String {
        "Environment variables".to_owned()
    }
    fn heading_note(&self) -> Option<String> {
        Some("new panes, for everyone in this session".to_owned())
    }
    fn noun(&self) -> &'static str {
        "variable"
    }
    fn identity(&self, entry: &EnvVarEntry) -> String {
        entry.name.clone()
    }
    fn columns(&self, entry: &EnvVarEntry) -> Vec<String> {
        vec![entry.name.clone(), entry.value.clone()]
    }
    fn summary(&self, entry: &EnvVarEntry) -> String {
        format!("{}={}", entry.name, entry.value)
    }
    fn fields(&self) -> Vec<FieldSpec> {
        vec![text_field("Name", "MY_VARIABLE"), text_field("Value", "")]
    }
    fn to_values(&self, entry: &EnvVarEntry) -> Vec<FieldValue> {
        vec![
            FieldValue::Text(entry.name.clone()),
            FieldValue::Text(entry.value.clone()),
        ]
    }
    fn from_values(&self, values: &[FieldValue]) -> Result<EnvVarEntry, String> {
        let value = match &values[1] {
            FieldValue::Text(text) => text.clone(),
            _ => String::new(),
        };
        Ok(EnvVarEntry {
            name: check_env_name(&values[0].text())?,
            value,
        })
    }
    fn block_kdl(&self, entries: &[EnvVarEntry]) -> Result<String, String> {
        Ok(env_kdl(entries))
    }
    fn block_key(&self) -> SettingKey {
        SettingKey::Env
    }
    fn entries(&self, blocks: &ConfigBlocks) -> Vec<EnvVarEntry> {
        blocks.env.clone()
    }
}

pub struct MenuKind {
    pub section: &'static str,
    pub shortcuts: BTreeMap<(Option<String>, Vec<String>), String>,
}

pub fn without_shortcut(entry: &MenuItemEntry) -> MenuItemEntry {
    MenuItemEntry {
        label: entry.label.clone(),
        actions: entry.actions.clone(),
        shortcut: None,
    }
}

const ITEM: &str = "item";
const SEPARATOR: &str = "separator";
const MENU_ENTRY_KINDS: &[&str] = &[ITEM, SEPARATOR];

fn section_title(section: &str) -> String {
    match section {
        "pane" => "Pane menu (right click on a pane or its frame)".to_owned(),
        "tab" => "Tab menu (right click on a tab)".to_owned(),
        "bar" => "Bar menu (right click on an empty part of the tab bar)".to_owned(),
        "common" => "Common items (added at the end of every menu)".to_owned(),
        other => format!("{} menu", other),
    }
}

impl EntryKind for MenuKind {
    type Entry = MenuItemEntry;
    fn title(&self) -> String {
        section_title(self.section)
    }
    fn menu_like(&self) -> bool {
        true
    }
    fn short_title(&self) -> String {
        match self.section {
            "pane" => "Pane menu".to_owned(),
            "tab" => "Tab menu".to_owned(),
            "bar" => "Bar menu".to_owned(),
            "common" => "Common items".to_owned(),
            other => format!("{} menu", other),
        }
    }
    fn noun(&self) -> &'static str {
        "item"
    }
    fn identity(&self, entry: &MenuItemEntry) -> String {
        entry.label.clone().unwrap_or_else(|| SEPARATOR.to_owned())
    }
    fn columns(&self, entry: &MenuItemEntry) -> Vec<String> {
        match &entry.label {
            None => vec![
                "──────── separator".to_owned(),
                String::new(),
                String::new(),
            ],
            Some(label) => vec![
                label.clone(),
                actions_summary(&entry.actions),
                self.shortcuts
                    .get(&(entry.label.clone(), entry.actions.clone()))
                    .cloned()
                    .unwrap_or_default(),
            ],
        }
    }
    fn summary(&self, entry: &MenuItemEntry) -> String {
        match &entry.label {
            None => "──────── separator".to_owned(),
            Some(label) => {
                let shortcut = self
                    .shortcuts
                    .get(&(entry.label.clone(), entry.actions.clone()))
                    .map(|shortcut| format!("  [{}]", shortcut))
                    .unwrap_or_default();
                format!(
                    "{}{}  ·  {}",
                    label,
                    shortcut,
                    actions_summary(&entry.actions)
                )
            },
        }
    }
    fn fields(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::choice("Kind", MENU_ENTRY_KINDS),
            text_field("Label", "shown in the menu").when(0, ITEM),
            FieldSpec::actions("Actions").when(0, ITEM),
        ]
    }
    fn to_values(&self, entry: &MenuItemEntry) -> Vec<FieldValue> {
        let kind = if entry.is_separator() {
            SEPARATOR
        } else {
            ITEM
        };
        vec![
            FieldValue::Text(kind.to_owned()),
            FieldValue::Text(entry.label.clone().unwrap_or_default()),
            FieldValue::Actions(entry.actions.clone()),
        ]
    }
    fn from_values(&self, values: &[FieldValue]) -> Result<MenuItemEntry, String> {
        if values[0].text() == SEPARATOR {
            return Ok(MenuItemEntry::separator());
        }
        let label = values[1].text();
        if label.is_empty() {
            return Err("A label is needed".to_owned());
        }
        let actions = values[2].actions();
        if actions.is_empty() {
            return Err("Choose at least one action".to_owned());
        }
        Ok(MenuItemEntry {
            label: Some(label),
            actions,
            shortcut: None,
        })
    }
    fn block_kdl(&self, entries: &[MenuItemEntry]) -> Result<String, String> {
        menu_section_kdl(self.section, entries)
    }
    fn block_key(&self) -> SettingKey {
        SettingKey::ContextMenu
    }
    fn entries(&self, blocks: &ConfigBlocks) -> Vec<MenuItemEntry> {
        blocks
            .menu_section(self.section)
            .iter()
            .map(without_shortcut)
            .collect()
    }
    fn refresh(&mut self, snapshot: &ConfigSnapshot) {
        self.shortcuts = snapshot
            .blocks
            .menu_section(self.section)
            .iter()
            .filter_map(|entry| {
                entry
                    .shortcut
                    .clone()
                    .map(|shortcut| ((entry.label.clone(), entry.actions.clone()), shortcut))
            })
            .collect();
    }
    fn ordered(&self) -> bool {
        true
    }
    fn unique_identity(&self) -> bool {
        false
    }
    fn separator(&self) -> Option<MenuItemEntry> {
        Some(MenuItemEntry::separator())
    }
    fn can_restore_defaults(&self) -> bool {
        true
    }
    fn replace_when_reverting(&self) -> bool {
        true
    }
}

const MENU_SECTIONS: [&str; 4] = ["pane", "tab", "bar", "common"];

pub struct BlockPage {
    note: &'static str,
    list: SectionedList,
}

impl BlockPage {
    pub fn plugins_and_environment() -> Self {
        BlockPage {
            note: "",
            list: SectionedList::new(vec![
                Box::new(ListEditor::new(AliasKind).dialog_form()),
                Box::new(ListEditor::new(LoadPluginKind).dialog_form()),
                Box::new(ListEditor::new(EnvKind).dialog_form()),
            ])
            .styled("Plugins and environment", "Show"),
        }
    }
    pub fn right_click_menu() -> Self {
        let sections: Vec<Box<dyn Section>> = MENU_SECTIONS
            .iter()
            .map(|section| {
                Box::new(
                    ListEditor::new(MenuKind {
                        section: *section,
                        shortcuts: BTreeMap::new(),
                    })
                    .for_menu()
                    .dialog_form(),
                ) as Box<dyn Section>
            })
            .collect();
        BlockPage {
            note: "",
            list: SectionedList::new(sections)
                .with_cross_moves()
                .with_shared_columns()
                .styled("Right-click menu", "Menu"),
        }
    }
}

impl Page for BlockPage {
    fn set_snapshot(&mut self, snapshot: &ConfigSnapshot) {
        self.list.set_snapshot(snapshot);
    }
    fn handle_key(&mut self, key: &KeyWithModifier) -> PageResponse {
        let busy = self.list.is_busy();
        if self.list.handle_key(key) || busy {
            return PageResponse::Handled;
        }
        if is_plain(key, BareKey::Esc) {
            return PageResponse::Close;
        }
        if is_plain(key, BareKey::Tab)
            || is_plain(key, BareKey::Left)
            || is_shift_tab(key)
            || is_plain(key, BareKey::Up)
        {
            return PageResponse::LeaveToMenu;
        }
        PageResponse::NotHandled
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> PageResponse {
        if self.list.handle_mouse(mouse) {
            PageResponse::Handled
        } else {
            PageResponse::NotHandled
        }
    }
    fn handle_timer(&mut self) -> bool {
        self.list.handle_timer()
    }
    fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        if self.list.is_busy() || self.note.is_empty() {
            self.list.render(x, y, width, height);
        } else {
            print_dim(self.note, x, y, width);
            self.list.render(x, y + 2, width, height.saturating_sub(2));
        }
    }
    fn render_overlays(&mut self, rows: usize, cols: usize) {
        self.list.render_overlays(rows, cols);
    }
    fn captures_keys(&self) -> bool {
        self.list.captures_keys()
    }
    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let mut hints = self.list.hints();
        if !self.list.is_busy() {
            hints.push(("<Tab>", "categories"));
        }
        hints
    }
    fn take_effects(&mut self) -> Vec<Effect> {
        self.list.take_effects()
    }
    fn take_notice(&mut self) -> Option<String> {
        self.list.take_notice()
    }
    fn leave(&mut self) {
        self.list.close();
    }
    fn set_focused(&mut self, focused: bool) {
        self.list.set_focused(focused);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_locations_are_checked() {
        assert_eq!(
            check_plugin_location("zellij:tab-bar", false),
            Ok("zellij:tab-bar".to_owned())
        );
        assert_eq!(
            check_plugin_location("/tmp/p.wasm", false),
            Ok("file:/tmp/p.wasm".to_owned())
        );
        assert!(check_plugin_location("https://example.com/p.wasm", false).is_ok());
        assert!(check_plugin_location("my-alias", false).is_err());
        assert_eq!(
            check_plugin_location("my-alias", true),
            Ok("my-alias".to_owned())
        );
        assert!(check_plugin_location("zellij:", false).is_err());
    }

    #[test]
    fn variable_names_are_checked() {
        assert!(check_env_name("EDITOR").is_ok());
        assert!(check_env_name("_x1").is_ok());
        assert!(check_env_name("1X").is_err());
        assert!(check_env_name("MY-VAR").is_err());
    }

    #[test]
    fn a_built_in_alias_cannot_be_deleted() {
        let alias = PluginAliasEntry {
            name: "tab-bar".to_owned(),
            plugin: PluginEntry::default(),
        };
        assert!(AliasKind.can_delete(&alias, &[alias.clone()]).is_err());
        assert!(AliasKind.can_delete(&alias, &[]).is_ok());
    }

    #[test]
    fn an_alias_form_writes_a_plugins_block_that_reads_back() {
        let entry = AliasKind
            .from_values(&[
                FieldValue::Text("mine".to_owned()),
                FieldValue::Text("/tmp/mine.wasm".to_owned()),
                FieldValue::Text(String::new()),
                FieldValue::Pairs(vec![("size".to_owned(), "3".to_owned())]),
            ])
            .unwrap();
        let kdl = AliasKind.block_kdl(&[entry]).unwrap();
        let mut config = zellij_utils::input::config::Config::default();
        zellij_utils::input::config_blocks::replace_config_blocks(&mut config, &kdl).unwrap();
        let alias = &config.plugins.aliases["mine"];
        assert_eq!(alias.location.display(), "file:/tmp/mine.wasm");
        assert_eq!(alias.configuration.inner()["size"], "3");
    }

    #[test]
    fn a_separator_can_be_edited_into_an_item_and_back() {
        let kind = MenuKind {
            section: "pane",
            shortcuts: BTreeMap::new(),
        };
        let separator = MenuItemEntry::separator();
        let mut values = kind.to_values(&separator);
        assert_eq!(values[0], FieldValue::Text("separator".to_owned()));
        assert_eq!(kind.from_values(&values), Ok(MenuItemEntry::separator()));
        values[0] = FieldValue::Text("item".to_owned());
        assert!(kind.from_values(&values).is_err());
        values[1] = FieldValue::Text("Hello".to_owned());
        values[2] = FieldValue::Actions(vec!["NewTab".to_owned()]);
        let item = kind.from_values(&values).unwrap();
        assert_eq!(item.label.as_deref(), Some("Hello"));
        let kdl = kind.block_kdl(&[item, separator]).unwrap();
        let mut config = zellij_utils::input::config::Config::default();
        zellij_utils::input::config_blocks::replace_config_blocks(&mut config, &kdl).unwrap();
        assert_eq!(config.context_menu.pane.len(), 2);
        assert_eq!(config.context_menu.pane[0].label(), Some("Hello"));
        assert_eq!(config.context_menu.pane[1], ContextMenuEntry::Separator);
    }
}

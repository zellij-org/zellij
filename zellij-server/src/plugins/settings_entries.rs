use std::collections::HashSet;

use crate::plugins::PluginId;
use zellij_utils::attention::sanitize_display_text;
use zellij_utils::data::PluginSettingsEntry;
use zellij_utils::input::layout::{PluginUserConfiguration, RunPluginLocation};

/// A plugin's location and configuration, so that instances of one plugin can be told apart from
/// different plugins that happen to use the same title.
pub type PluginIdentity = (RunPluginLocation, PluginUserConfiguration);

/// Titles longer than this are cut, since they are shown in a single settings screen row.
const MAX_TITLE_CHARS: usize = 80;

/// Settings screen entries registered by plugins, at most one per plugin id.
#[derive(Debug, Default)]
pub struct SettingsEntries {
    entries: Vec<RegisteredEntry>,
    // plugin ids are never reused, so a registration from an unloaded plugin that was still on
    // its way can be refused
    unloaded: HashSet<PluginId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegisteredEntry {
    plugin_id: PluginId,
    plugin_identity: PluginIdentity,
    title: String,
}

impl SettingsEntries {
    /// Adds or retitles the entry of `plugin_id`. Returns whether the listed entries changed.
    pub fn register(
        &mut self,
        plugin_id: PluginId,
        plugin_identity: PluginIdentity,
        title: &str,
    ) -> bool {
        if self.unloaded.contains(&plugin_id) {
            return false;
        }
        let title: String = sanitize_display_text(title)
            .trim()
            .chars()
            .take(MAX_TITLE_CHARS)
            .collect();
        let title = title.trim_end().to_owned();
        if title.is_empty() {
            log::warn!(
                "Ignoring a settings entry with an empty title from plugin {}",
                plugin_identity.0.display()
            );
            return false;
        }
        let before = self.entries();
        let entry = RegisteredEntry {
            plugin_id,
            plugin_identity,
            title,
        };
        match self
            .entries
            .iter_mut()
            .find(|existing| existing.plugin_id == plugin_id)
        {
            Some(existing) => *existing = entry,
            None => self.entries.push(entry),
        }
        self.entries() != before
    }
    /// Removes the entry of `plugin_id`, if any. Returns whether the listed entries changed.
    pub fn unregister(&mut self, plugin_id: PluginId) -> bool {
        let before = self.entries();
        self.entries.retain(|entry| entry.plugin_id != plugin_id);
        self.entries() != before
    }
    /// Removes the entry of an unloaded plugin and refuses any later registration from it.
    /// Returns whether the listed entries changed.
    pub fn plugin_unloaded(&mut self, plugin_id: PluginId) -> bool {
        self.unloaded.insert(plugin_id);
        self.unregister(plugin_id)
    }
    /// The entries to show, in registration order. Instances of the same plugin with the same
    /// title are listed once, pointing at the instance with the lowest id so that the choice
    /// doesn't depend on registration order.
    pub fn entries(&self) -> Vec<PluginSettingsEntry> {
        let mut listed: Vec<(&PluginIdentity, PluginSettingsEntry)> = vec![];
        for entry in &self.entries {
            match listed.iter_mut().find(|(plugin_identity, listed_entry)| {
                **plugin_identity == entry.plugin_identity && listed_entry.title == entry.title
            }) {
                Some((_, listed_entry)) => {
                    listed_entry.plugin_id = listed_entry.plugin_id.min(entry.plugin_id);
                },
                None => listed.push((
                    &entry.plugin_identity,
                    PluginSettingsEntry {
                        plugin_id: entry.plugin_id,
                        title: entry.title.clone(),
                    },
                )),
            }
        }
        listed.into_iter().map(|(_, entry)| entry).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(path: &str) -> PluginIdentity {
        (
            RunPluginLocation::File(path.into()),
            PluginUserConfiguration::default(),
        )
    }

    fn status() -> PluginIdentity {
        identity("/plugins/status.wasm")
    }

    fn clock() -> PluginIdentity {
        identity("/plugins/clock.wasm")
    }

    fn listed(entries: &SettingsEntries) -> Vec<(PluginId, String)> {
        entries
            .entries()
            .into_iter()
            .map(|entry| (entry.plugin_id, entry.title))
            .collect()
    }

    #[test]
    fn entries_keep_their_registration_order() {
        let mut entries = SettingsEntries::default();
        assert!(entries.register(2, status(), "Status"));
        assert!(entries.register(1, clock(), "Clock"));
        assert_eq!(
            listed(&entries),
            vec![(2, "Status".to_owned()), (1, "Clock".to_owned())]
        );
    }

    #[test]
    fn registering_again_retitles_the_entry_in_place() {
        let mut entries = SettingsEntries::default();
        entries.register(1, status(), "Status");
        entries.register(2, clock(), "Clock");
        assert!(entries.register(1, status(), "Status bar"));
        assert_eq!(
            listed(&entries),
            vec![(1, "Status bar".to_owned()), (2, "Clock".to_owned())]
        );
    }

    #[test]
    fn registering_the_same_title_again_reports_no_change() {
        let mut entries = SettingsEntries::default();
        assert!(entries.register(1, status(), "Status"));
        assert!(!entries.register(1, status(), "Status"));
    }

    #[test]
    fn unregistering_removes_only_that_plugins_entry() {
        let mut entries = SettingsEntries::default();
        entries.register(1, status(), "Status");
        entries.register(2, clock(), "Clock");
        assert!(entries.unregister(1));
        assert!(!entries.unregister(1));
        assert!(!entries.unregister(9));
        assert_eq!(listed(&entries), vec![(2, "Clock".to_owned())]);
    }

    #[test]
    fn instances_of_one_plugin_with_one_title_are_listed_once() {
        let mut entries = SettingsEntries::default();
        assert!(entries.register(5, status(), "Status"));
        // still one entry, now pointing at the lower id
        assert!(entries.register(3, status(), "Status"));
        assert!(!entries.register(6, status(), "Status"));
        assert!(entries.register(4, clock(), "Clock"));
        assert_eq!(
            listed(&entries),
            vec![(3, "Status".to_owned()), (4, "Clock".to_owned())]
        );
        // the entry stays while any instance is left, pointing at a live one
        assert!(entries.unregister(3));
        assert_eq!(
            listed(&entries),
            vec![(5, "Status".to_owned()), (4, "Clock".to_owned())]
        );
        assert!(entries.unregister(5));
        assert!(entries.unregister(6));
        assert_eq!(listed(&entries), vec![(4, "Clock".to_owned())]);
    }

    #[test]
    fn different_plugins_with_the_same_title_are_both_listed() {
        let mut entries = SettingsEntries::default();
        entries.register(1, status(), "Settings");
        entries.register(2, clock(), "Settings");
        assert_eq!(
            listed(&entries),
            vec![(1, "Settings".to_owned()), (2, "Settings".to_owned())]
        );
    }

    #[test]
    fn a_blank_title_is_refused_and_leaves_an_existing_entry_alone() {
        let mut entries = SettingsEntries::default();
        assert!(!entries.register(1, status(), "   "));
        assert!(entries.entries().is_empty());
        entries.register(1, status(), "Status");
        assert!(!entries.register(1, status(), "\u{1b}\n\t"));
        assert_eq!(listed(&entries), vec![(1, "Status".to_owned())]);
    }

    #[test]
    fn titles_are_stripped_of_control_characters_trimmed_and_capped() {
        let mut entries = SettingsEntries::default();
        entries.register(1, status(), "  \u{1b}[31mStatus\nbar\u{202e}  ");
        assert_eq!(listed(&entries), vec![(1, "[31mStatusbar".to_owned())]);
        entries.register(1, status(), &"x".repeat(200));
        assert_eq!(entries.entries()[0].title.chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn instances_with_another_configuration_are_listed_separately() {
        let mut entries = SettingsEntries::default();
        let mut configured = status();
        configured.1.insert("theme".to_owned(), "dark".to_owned());
        entries.register(1, status(), "Status");
        entries.register(2, configured, "Status");
        assert_eq!(
            listed(&entries),
            vec![(1, "Status".to_owned()), (2, "Status".to_owned())]
        );
    }

    #[test]
    fn an_unloaded_plugin_cannot_register_again() {
        let mut entries = SettingsEntries::default();
        entries.register(1, status(), "Status");
        assert!(entries.plugin_unloaded(1));
        assert!(!entries.register(1, status(), "Status"));
        assert!(entries.entries().is_empty());
        assert!(entries.register(2, status(), "Status"));
    }
}

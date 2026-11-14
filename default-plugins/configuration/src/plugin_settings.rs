use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use zellij_tile::prelude::*;

use crate::list_editor::Section;
use crate::page::Effect;

/// The entries plugins registered with `register_settings_entry`, shared between the page that
/// receives them and the section that lists them.
pub type SharedPluginSettingsEntries = Rc<RefCell<Vec<PluginSettingsEntry>>>;

/// Lists the settings entries of running plugins. Selecting one asks that plugin to open its own
/// settings, so the section has nothing to edit, add or delete.
pub struct PluginSettingsSection {
    entries: SharedPluginSettingsEntries,
    effects: Vec<Effect>,
}

impl PluginSettingsSection {
    pub fn new(entries: SharedPluginSettingsEntries) -> Self {
        PluginSettingsSection {
            entries,
            effects: vec![],
        }
    }
}

impl Section for PluginSettingsSection {
    fn heading(&self) -> String {
        "Plugin settings".to_owned()
    }
    fn heading_note(&self) -> Option<String> {
        if self.entries.borrow().is_empty() {
            Some("no running plugin has added its settings here".to_owned())
        } else {
            None
        }
    }
    fn set_snapshot(&mut self, _snapshot: &ConfigSnapshot) {}
    fn len(&self) -> usize {
        self.entries.borrow().len()
    }
    fn columns(&self, index: usize) -> Vec<String> {
        self.entries
            .borrow()
            .get(index)
            .map(|entry| vec![entry.title.clone()])
            .unwrap_or_default()
    }
    fn marker(&self, _index: usize) -> String {
        String::new()
    }
    fn actions(&self) -> Vec<String> {
        vec![]
    }
    fn run_action(&mut self, _action: usize, _after: Option<usize>) {}
    fn open_add(&mut self, _after: Option<usize>) {}
    fn add_separator(&mut self, _after: Option<usize>) -> bool {
        false
    }
    fn open_edit(&mut self, index: usize) {
        if let Some(entry) = self.entries.borrow().get(index) {
            self.effects
                .push(Effect::OpenPluginSettings(entry.plugin_id));
        }
    }
    fn request_delete(&mut self, _index: usize) {}
    fn revert(&mut self, _index: usize) {}
    fn restore_defaults(&mut self) -> bool {
        false
    }
    fn has_separators(&self) -> bool {
        false
    }
    fn can_restore_defaults(&self) -> bool {
        false
    }
    fn ordered(&self) -> bool {
        false
    }
    fn move_entry(&mut self, _index: usize, _down: bool) -> bool {
        false
    }
    fn take_entry(&mut self, _index: usize) -> Option<Box<dyn Any>> {
        None
    }
    fn insert_entry(&mut self, _index: usize, entry: Box<dyn Any>) -> Result<(), Box<dyn Any>> {
        Err(entry)
    }
    fn followed(&self) -> Option<usize> {
        None
    }
    fn clear_follow(&mut self) {}
    fn is_busy(&self) -> bool {
        false
    }
    fn handle_busy_key(&mut self, _key: &KeyWithModifier) {}
    fn handle_busy_mouse(&mut self, _mouse: Mouse) -> bool {
        false
    }
    fn handle_timer(&mut self) -> bool {
        false
    }
    fn render_busy(&mut self, _x: usize, _y: usize, _width: usize, _height: usize) {}
    fn read_only(&self) -> bool {
        true
    }
    fn render_overlays(&mut self, _rows: usize, _cols: usize) {}
    fn busy_hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![]
    }
    fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
    fn take_notice(&mut self) -> Option<String> {
        None
    }
    fn close(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::list_editor::{Row, SectionedList};

    fn entry(plugin_id: u32, title: &str) -> PluginSettingsEntry {
        PluginSettingsEntry {
            plugin_id,
            title: title.to_owned(),
        }
    }

    fn list_with(
        entries: Vec<PluginSettingsEntry>,
    ) -> (SectionedList, SharedPluginSettingsEntries) {
        let shared = SharedPluginSettingsEntries::new(RefCell::new(entries));
        let mut list =
            SectionedList::new(vec![Box::new(PluginSettingsSection::new(shared.clone()))])
                .styled("Plugins and environment", "Show");
        list.sections_changed();
        list.set_focused(true);
        (list, shared)
    }

    fn press(list: &mut SectionedList, bare_key: BareKey) -> bool {
        list.handle_key(&KeyWithModifier::new(bare_key))
    }

    #[test]
    fn entries_are_listed_without_an_actions_row() {
        let (list, _) = list_with(vec![entry(4, "Status bar"), entry(9, "Clock")]);
        assert_eq!(
            list.rows(),
            vec![Row::Heading(0), Row::Entry(0, 0), Row::Entry(0, 1)]
        );
        assert_eq!(list.selected(), Row::Entry(0, 0));
    }

    #[test]
    fn opening_an_entry_asks_its_plugin_to_open_its_settings() {
        let (mut list, _) = list_with(vec![entry(4, "Status bar"), entry(9, "Clock")]);
        assert!(press(&mut list, BareKey::Down));
        assert!(press(&mut list, BareKey::Enter));
        assert_eq!(list.take_effects(), vec![Effect::OpenPluginSettings(9)]);
        assert!(!list.is_busy());
    }

    #[test]
    fn entries_cannot_be_edited_added_or_deleted() {
        let (mut list, shared) = list_with(vec![entry(4, "Status bar")]);
        for character in ['a', 'd', 'r', 'R', 's'] {
            list.handle_key(&KeyWithModifier::new(BareKey::Char(character)));
        }
        press(&mut list, BareKey::Delete);
        list.handle_key(&KeyWithModifier::new(BareKey::Down).with_shift_modifier());
        assert!(list.take_notice().is_none());
        assert!(list.take_effects().is_empty());
        assert!(!list.is_busy());
        assert_eq!(*shared.borrow(), vec![entry(4, "Status bar")]);
        let hints = list.hints();
        assert!(hints.contains(&("<Enter>", "open")));
        assert!(!hints
            .iter()
            .any(|(_, action)| ["edit", "delete", "revert", "add"].contains(action)));
    }

    #[test]
    fn an_empty_section_says_so_and_offers_nothing_to_open() {
        let (mut list, _) = list_with(vec![]);
        assert_eq!(list.rows(), vec![Row::Heading(0)]);
        let section = PluginSettingsSection::new(SharedPluginSettingsEntries::default());
        assert!(section.heading_note().is_some());
        press(&mut list, BareKey::Down);
        press(&mut list, BareKey::Enter);
        assert!(list.take_effects().is_empty());
    }

    #[test]
    fn the_selection_follows_entries_that_go_away() {
        let (mut list, shared) = list_with(vec![entry(4, "Status bar"), entry(9, "Clock")]);
        press(&mut list, BareKey::Down);
        assert_eq!(list.selected(), Row::Entry(0, 1));
        *shared.borrow_mut() = vec![entry(4, "Status bar")];
        list.sections_changed();
        assert_eq!(list.selected(), Row::Entry(0, 0));
        press(&mut list, BareKey::Enter);
        assert_eq!(list.take_effects(), vec![Effect::OpenPluginSettings(4)]);
    }
}

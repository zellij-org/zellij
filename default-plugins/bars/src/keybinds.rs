use std::collections::BTreeMap;
use zellij_tile::prelude::*;

type TableId = usize;

#[derive(Default)]
pub struct KeybindStore {
    tables: BTreeMap<TableId, KeybindsVec>,
    clients: BTreeMap<ClientId, TableId>,
    next_table_id: TableId,
}

impl KeybindStore {
    pub fn set(&mut self, client_id: ClientId, keybinds: KeybindsVec) {
        let existing = self
            .tables
            .iter()
            .find(|(_, table)| **table == keybinds)
            .map(|(table_id, _)| *table_id);
        let table_id = match existing {
            Some(table_id) => table_id,
            None => {
                let table_id = self.next_table_id;
                self.next_table_id += 1;
                self.tables.insert(table_id, keybinds);
                table_id
            },
        };
        if let Some(previous) = self.clients.insert(client_id, table_id) {
            self.release(previous);
        }
    }

    pub fn remove(&mut self, client_id: ClientId) {
        if let Some(table_id) = self.clients.remove(&client_id) {
            self.release(table_id);
        }
    }

    pub fn client_ids(&self) -> impl Iterator<Item = ClientId> + '_ {
        self.clients.keys().copied()
    }

    pub fn swap(&mut self, client_id: ClientId, keybinds: &mut KeybindsVec) -> bool {
        let table = self
            .clients
            .get(&client_id)
            .and_then(|table_id| self.tables.get_mut(table_id));
        match table {
            Some(table) => {
                std::mem::swap(table, keybinds);
                true
            },
            None => false,
        }
    }

    fn release(&mut self, table_id: TableId) {
        if !self.clients.values().any(|id| *id == table_id) {
            self.tables.remove(&table_id);
        }
    }

    #[cfg(test)]
    fn table_count(&self) -> usize {
        self.tables.len()
    }
}

pub fn take_keybinds(event: &mut Event) -> Option<KeybindsVec> {
    match event {
        Event::InitialKeybinds(keybinds) => Some(std::mem::take(keybinds)),
        Event::ModeUpdate(mode_info) if !mode_info.keybinds.is_empty() => {
            Some(std::mem::take(&mut mode_info.keybinds))
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_tile::prelude::actions::Action;

    fn table(c: char) -> KeybindsVec {
        vec![(
            InputMode::Normal,
            vec![(KeyWithModifier::new(BareKey::Char(c)), vec![Action::Quit])],
        )]
    }

    #[test]
    fn clients_with_the_same_keybinds_share_one_table() {
        let mut store = KeybindStore::default();
        store.set(1, table('q'));
        store.set(2, table('q'));
        assert_eq!(store.table_count(), 1);
        store.set(2, table('x'));
        assert_eq!(store.table_count(), 2);
        store.remove(1);
        assert_eq!(store.table_count(), 1);
        store.remove(2);
        assert_eq!(store.table_count(), 0);
    }

    #[test]
    fn swapping_twice_restores_the_table() {
        let mut store = KeybindStore::default();
        store.set(1, table('q'));
        store.set(2, table('q'));
        let mut lent = KeybindsVec::new();
        assert!(store.swap(1, &mut lent));
        assert_eq!(lent, table('q'));
        assert!(store.swap(1, &mut lent));
        assert!(lent.is_empty());
        let mut other = KeybindsVec::new();
        assert!(store.swap(2, &mut other));
        assert_eq!(other, table('q'));
        assert!(!store.swap(3, &mut other));
    }

    #[test]
    fn take_keybinds_empties_the_event() {
        let mut event = Event::InitialKeybinds(table('q'));
        assert_eq!(take_keybinds(&mut event), Some(table('q')));
        assert_eq!(event, Event::InitialKeybinds(vec![]));
        let mut event = Event::ModeUpdate(ModeInfo::default());
        assert_eq!(take_keybinds(&mut event), None);
    }
}

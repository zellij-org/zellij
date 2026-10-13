use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub const DEFAULT_BYTE_LIMIT: usize = 32 * 1024 * 1024;
pub const ENTRY_OVERHEAD: usize = 64;
pub const MAX_ID: u32 = (1 << 23) - 1;

struct Entry {
    text: Option<Rc<str>>,
    refs: u32,
}

struct ClusterTable {
    entries: Vec<Entry>,
    free: Vec<u32>,
    by_text: HashMap<Rc<str>, u32>,
    bytes: usize,
    limit: usize,
    warned: bool,
}

impl ClusterTable {
    fn new() -> Self {
        ClusterTable {
            entries: Vec::new(),
            free: Vec::new(),
            by_text: HashMap::new(),
            bytes: 0,
            limit: DEFAULT_BYTE_LIMIT,
            warned: false,
        }
    }

    fn intern(&mut self, text: &str) -> Option<u32> {
        if let Some(id) = self.by_text.get(text) {
            let entry = &mut self.entries[*id as usize];
            entry.refs = entry.refs.saturating_add(1);
            return Some(*id);
        }
        let cost = text.len() + ENTRY_OVERHEAD;
        if self.bytes + cost > self.limit {
            if !self.warned {
                log::warn!(
                    "grapheme cluster table limit of {} bytes reached, dropping combining code points",
                    self.limit
                );
                self.warned = true;
            }
            return None;
        }
        let id = match self.free.pop() {
            Some(id) => id,
            None => {
                let next = self.entries.len() as u32;
                if next > MAX_ID {
                    return None;
                }
                self.entries.push(Entry {
                    text: None,
                    refs: 0,
                });
                next
            },
        };
        let text: Rc<str> = Rc::from(text);
        self.entries[id as usize] = Entry {
            text: Some(text.clone()),
            refs: 1,
        };
        self.by_text.insert(text, id);
        self.bytes += cost;
        Some(id)
    }

    fn retain(&mut self, id: u32) {
        if let Some(entry) = self.entries.get_mut(id as usize) {
            if entry.text.is_some() {
                entry.refs = entry.refs.saturating_add(1);
            }
        }
    }

    fn release(&mut self, id: u32) {
        let Some(entry) = self.entries.get_mut(id as usize) else {
            return;
        };
        if entry.text.is_none() || entry.refs == u32::MAX {
            return;
        }
        entry.refs -= 1;
        if entry.refs == 0 {
            if let Some(text) = entry.text.take() {
                self.by_text.remove(&text);
                self.bytes -= text.len() + ENTRY_OVERHEAD;
                self.free.push(id);
                if self.warned && self.bytes < self.limit / 4 * 3 {
                    self.warned = false;
                }
            }
        }
    }

    fn text(&self, id: u32) -> Option<Rc<str>> {
        self.entries.get(id as usize).and_then(|e| e.text.clone())
    }
}

thread_local! {
    static TABLE: RefCell<ClusterTable> = RefCell::new(ClusterTable::new());
}

pub fn intern(text: &str) -> Option<u32> {
    TABLE
        .try_with(|table| table.borrow_mut().intern(text))
        .ok()
        .flatten()
}

pub fn retain(id: u32) {
    let _ = TABLE.try_with(|table| {
        if let Ok(mut table) = table.try_borrow_mut() {
            table.retain(id);
        }
    });
}

pub fn release(id: u32) {
    let _ = TABLE.try_with(|table| {
        if let Ok(mut table) = table.try_borrow_mut() {
            table.release(id);
        }
    });
}

pub fn text(id: u32) -> Option<Rc<str>> {
    TABLE
        .try_with(|table| table.borrow().text(id))
        .ok()
        .flatten()
}

pub fn bytes() -> usize {
    TABLE.try_with(|table| table.borrow().bytes).unwrap_or(0)
}

pub fn live_entries() -> usize {
    TABLE
        .try_with(|table| table.borrow().by_text.len())
        .unwrap_or(0)
}

#[cfg(test)]
pub fn set_limit_for_tests(limit: usize) {
    TABLE.with(|table| table.borrow_mut().limit = limit);
}

#[cfg(test)]
pub fn refs_for_tests(id: u32) -> u32 {
    TABLE.with(|table| {
        table
            .borrow()
            .entries
            .get(id as usize)
            .map(|e| e.refs)
            .unwrap_or(0)
    })
}

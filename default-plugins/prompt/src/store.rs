use zellij_utils::prompt::ChoiceItem;

const CHUNK_BYTES: usize = 64 * 1024;
const NO_LABEL: u32 = u32::MAX;

#[derive(Debug, Clone, Copy)]
struct Span {
    chunk: u32,
    start: u32,
    value_len: u32,
    label_len: u32,
}

#[derive(Debug, Default)]
pub struct ItemStore {
    chunks: Vec<String>,
    spans: Vec<Span>,
    text_bytes: usize,
}

impl ItemStore {
    pub fn len(&self) -> usize {
        self.spans.len()
    }
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }
    #[cfg(test)]
    pub fn text_bytes(&self) -> usize {
        self.text_bytes
    }
    pub fn footprint(&self) -> usize {
        self.chunks
            .iter()
            .map(|chunk| chunk.capacity())
            .sum::<usize>()
            + self.spans.capacity() * std::mem::size_of::<Span>()
    }
    fn chunk_with_room(&mut self, needed: usize) -> usize {
        let fits = self
            .chunks
            .last()
            .map(|chunk| chunk.capacity() - chunk.len() >= needed)
            .unwrap_or(false);
        if !fits {
            self.chunks
                .push(String::with_capacity(needed.max(CHUNK_BYTES)));
        }
        self.chunks.len() - 1
    }
    pub fn push(&mut self, item: &ChoiceItem) {
        let label = item.label.as_deref();
        let needed = item.value.len() + label.map(|l| l.len()).unwrap_or(0);
        let chunk_index = self.chunk_with_room(needed);
        let chunk = &mut self.chunks[chunk_index];
        let start = chunk.len();
        chunk.push_str(&item.value);
        if let Some(label) = label {
            chunk.push_str(label);
        }
        self.text_bytes += needed;
        self.spans.push(Span {
            chunk: chunk_index as u32,
            start: start as u32,
            value_len: item.value.len() as u32,
            label_len: label.map(|l| l.len() as u32).unwrap_or(NO_LABEL),
        });
    }
    pub fn value(&self, index: usize) -> &str {
        let span = self.spans[index];
        let start = span.start as usize;
        &self.chunks[span.chunk as usize][start..start + span.value_len as usize]
    }
    pub fn display(&self, index: usize) -> &str {
        let span = self.spans[index];
        if span.label_len == NO_LABEL {
            return self.value(index);
        }
        let start = span.start as usize + span.value_len as usize;
        &self.chunks[span.chunk as usize][start..start + span.label_len as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_and_labels_come_back_as_stored() {
        let mut store = ItemStore::default();
        store.push(&ChoiceItem::plain("main"));
        store.push(&ChoiceItem::labeled("rm", "Delete file"));
        store.push(&ChoiceItem::labeled("x", ""));
        assert_eq!(store.len(), 3);
        assert_eq!((store.value(0), store.display(0)), ("main", "main"));
        assert_eq!((store.value(1), store.display(1)), ("rm", "Delete file"));
        assert_eq!((store.value(2), store.display(2)), ("x", ""));
        assert_eq!(store.text_bytes(), 4 + 2 + 11 + 1);
    }

    #[test]
    fn many_lines_share_few_allocations() {
        let mut store = ItemStore::default();
        for i in 0..57_000 {
            store.push(&ChoiceItem::plain(format!(
                "./some/path/to/file-{:06}.rs",
                i
            )));
        }
        assert_eq!(store.value(56_999), "./some/path/to/file-056999.rs");
        let text = store.text_bytes();
        assert!(store.footprint() < text + text / 4 + 57_000 * 32);
        assert!(store.chunks.len() < 40);
    }

    #[test]
    fn a_line_longer_than_a_chunk_gets_its_own_chunk() {
        let mut store = ItemStore::default();
        store.push(&ChoiceItem::plain("short"));
        let long = "y".repeat(CHUNK_BYTES * 2);
        store.push(&ChoiceItem::plain(long.clone()));
        store.push(&ChoiceItem::plain("after"));
        assert_eq!(store.value(1), long);
        assert_eq!(store.value(2), "after");
    }
}

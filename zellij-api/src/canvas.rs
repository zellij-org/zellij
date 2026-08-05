//! Per-pane canvas state and its change history.
//!
//! Each pane is treated as a file whose contents are the visible screen rows.
//! Every update the server pushes becomes a new *version* of that file, and the
//! change between consecutive versions is stored as a unified diff. A caller
//! can therefore ask "what happened on this pane since version N?" and get back
//! a list of diffs, exactly like walking a file's git history.

use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::diff::{diff_canvas, normalize_canvas, CanvasDiff};

/// How many diffs we keep per pane before dropping the oldest.
pub const DEFAULT_HISTORY_CAPACITY: usize = 1000;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// One recorded change to a pane's canvas.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub seq: u64,
    pub ts: u64,
    pub added: usize,
    pub removed: usize,
    pub unified: String,
    pub hunks: Vec<crate::diff::DiffHunk>,
}

/// What applying an update produced.
#[derive(Debug, Clone)]
pub enum CanvasUpdate {
    /// A new baseline was established; there is no diff to report.
    Reset { seq: u64, ts: u64, lines: Vec<String> },
    /// The canvas changed.
    Changed { ts: u64, diff: CanvasDiff },
    /// Nothing visible changed.
    Unchanged,
}

#[derive(Debug)]
pub struct PaneCanvas {
    /// Current materialised contents.
    pub lines: Vec<String>,
    /// Version of `lines`. Starts at 0 for the first snapshot and increments
    /// once per recorded change.
    pub version: u64,
    history: VecDeque<HistoryEntry>,
    capacity: usize,
}

impl PaneCanvas {
    fn new(lines: Vec<String>, capacity: usize) -> Self {
        PaneCanvas {
            lines,
            version: 0,
            history: VecDeque::new(),
            capacity,
        }
    }
}

#[derive(Debug)]
pub struct CanvasStore {
    panes: HashMap<String, PaneCanvas>,
    capacity: usize,
}

impl Default for CanvasStore {
    fn default() -> Self {
        Self::new(DEFAULT_HISTORY_CAPACITY)
    }
}

impl CanvasStore {
    pub fn new(capacity: usize) -> Self {
        CanvasStore {
            panes: HashMap::new(),
            capacity: capacity.max(1),
        }
    }

    /// Feed an update from the server.
    ///
    /// `is_initial` is set by the server on the snapshot it sends when a
    /// subscription is (re)established. We treat it as a baseline rather than a
    /// diff, because diffing against a stale canvas from before a resubscribe
    /// would manufacture a change that never happened on screen.
    pub fn apply(
        &mut self,
        pane_id: &str,
        raw_lines: Vec<String>,
        is_initial: bool,
    ) -> CanvasUpdate {
        let lines = normalize_canvas(&raw_lines);
        let ts = now_ms();

        let Some(canvas) = self.panes.get_mut(pane_id) else {
            let canvas = PaneCanvas::new(lines.clone(), self.capacity);
            let seq = canvas.version;
            self.panes.insert(pane_id.to_string(), canvas);
            return CanvasUpdate::Reset { seq, ts, lines };
        };

        if is_initial {
            // Re-baseline without inventing history, but keep what we already
            // recorded so earlier diffs remain queryable.
            if canvas.lines == lines {
                return CanvasUpdate::Unchanged;
            }
            canvas.lines = lines.clone();
            return CanvasUpdate::Reset {
                seq: canvas.version,
                ts,
                lines,
            };
        }

        let label = format!("pane/{}", pane_id);
        let next_version = canvas.version + 1;
        let Some(diff) = diff_canvas(&canvas.lines, &lines, canvas.version, next_version, &label)
        else {
            return CanvasUpdate::Unchanged;
        };

        canvas.lines = lines;
        canvas.version = next_version;
        canvas.history.push_back(HistoryEntry {
            seq: next_version,
            ts,
            added: diff.added,
            removed: diff.removed,
            unified: diff.unified.clone(),
            hunks: diff.hunks.clone(),
        });
        while canvas.history.len() > canvas.capacity {
            canvas.history.pop_front();
        }

        CanvasUpdate::Changed { ts, diff }
    }

    /// Current contents and version of a pane's canvas.
    pub fn snapshot(&self, pane_id: &str) -> Option<(u64, Vec<String>)> {
        self.panes
            .get(pane_id)
            .map(|c| (c.version, c.lines.clone()))
    }

    /// Recorded changes for a pane, oldest first.
    ///
    /// `since` returns only entries newer than that version; `limit` keeps the
    /// most recent N of whatever remains.
    pub fn history(&self, pane_id: &str, since: Option<u64>, limit: Option<usize>) -> Vec<HistoryEntry> {
        let Some(canvas) = self.panes.get(pane_id) else {
            return Vec::new();
        };
        let mut entries: Vec<HistoryEntry> = canvas
            .history
            .iter()
            .filter(|e| since.map(|s| e.seq > s).unwrap_or(true))
            .cloned()
            .collect();
        if let Some(limit) = limit {
            if entries.len() > limit {
                entries.drain(..entries.len() - limit);
            }
        }
        entries
    }

    /// Whether we have ever seen this pane.
    pub fn knows(&self, pane_id: &str) -> bool {
        self.panes.contains_key(pane_id)
    }

    pub fn forget(&mut self, pane_id: &str) {
        self.panes.remove(pane_id);
    }

    pub fn pane_ids(&self) -> Vec<String> {
        self.panes.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn first_update_is_a_baseline_not_a_diff() {
        let mut store = CanvasStore::default();
        let update = store.apply("terminal_1", lines(&["$ "]), true);
        match update {
            CanvasUpdate::Reset { seq, lines: l, .. } => {
                assert_eq!(seq, 0);
                // Stored normalised: trailing padding is stripped so that
                // later cursor movement does not read as a change.
                assert_eq!(l, lines(&["$"]));
            },
            other => panic!("expected a baseline, got {:?}", other),
        }
        assert!(store.history("terminal_1", None, None).is_empty());
    }

    #[test]
    fn subsequent_updates_are_recorded_as_diffs() {
        let mut store = CanvasStore::default();
        store.apply("terminal_1", lines(&["$ sudr"]), true);
        let update = store.apply("terminal_1", lines(&["$ sudo"]), false);

        match update {
            CanvasUpdate::Changed { diff, .. } => {
                assert_eq!(diff.from_version, 0);
                assert_eq!(diff.to_version, 1);
                assert!(diff.unified.contains("-$ sudr"));
                assert!(diff.unified.contains("+$ sudo"));
            },
            other => panic!("expected a change, got {:?}", other),
        }

        let history = store.history("terminal_1", None, None);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].seq, 1);
        assert_eq!(history[0].added, 1);
        assert_eq!(history[0].removed, 1);
    }

    #[test]
    fn version_advances_only_on_real_change() {
        let mut store = CanvasStore::default();
        store.apply("p", lines(&["a"]), true);
        store.apply("p", lines(&["a"]), false);
        store.apply("p", lines(&["a   "]), false); // padding only
        assert_eq!(store.snapshot("p").unwrap().0, 0);
        assert!(store.history("p", None, None).is_empty());

        store.apply("p", lines(&["b"]), false);
        assert_eq!(store.snapshot("p").unwrap().0, 1);
    }

    #[test]
    fn snapshot_tracks_the_latest_contents() {
        let mut store = CanvasStore::default();
        store.apply("p", lines(&["one"]), true);
        store.apply("p", lines(&["one", "two"]), false);
        let (version, contents) = store.snapshot("p").unwrap();
        assert_eq!(version, 1);
        assert_eq!(contents, lines(&["one", "two"]));
    }

    #[test]
    fn history_can_be_replayed_from_a_known_version() {
        let mut store = CanvasStore::default();
        store.apply("p", lines(&["v0"]), true);
        for i in 1..=5 {
            store.apply("p", lines(&[&format!("v{}", i)]), false);
        }
        let all = store.history("p", None, None);
        assert_eq!(all.len(), 5);
        assert_eq!(all.first().unwrap().seq, 1);
        assert_eq!(all.last().unwrap().seq, 5);

        let since_3 = store.history("p", Some(3), None);
        assert_eq!(
            since_3.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![4, 5],
            "a caller resuming at v3 gets only what it missed"
        );
    }

    #[test]
    fn limit_keeps_the_most_recent_entries() {
        let mut store = CanvasStore::default();
        store.apply("p", lines(&["v0"]), true);
        for i in 1..=5 {
            store.apply("p", lines(&[&format!("v{}", i)]), false);
        }
        let last_two = store.history("p", None, Some(2));
        assert_eq!(last_two.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![4, 5]);
    }

    #[test]
    fn history_is_bounded() {
        let mut store = CanvasStore::new(3);
        store.apply("p", lines(&["v0"]), true);
        for i in 1..=10 {
            store.apply("p", lines(&[&format!("v{}", i)]), false);
        }
        let history = store.history("p", None, None);
        assert_eq!(history.len(), 3, "oldest entries are dropped");
        assert_eq!(
            history.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![8, 9, 10]
        );
        // The live canvas is still correct despite the trimmed history.
        assert_eq!(store.snapshot("p").unwrap().1, lines(&["v10"]));
    }

    #[test]
    fn resubscribing_rebaselines_without_a_phantom_diff() {
        let mut store = CanvasStore::default();
        store.apply("p", lines(&["a"]), true);
        store.apply("p", lines(&["b"]), false);
        let before = store.history("p", None, None).len();

        // The server re-sends a snapshot after a resubscribe.
        let update = store.apply("p", lines(&["totally different"]), true);
        assert!(matches!(update, CanvasUpdate::Reset { .. }));
        assert_eq!(
            store.history("p", None, None).len(),
            before,
            "a re-baseline must not fabricate a history entry"
        );
        assert_eq!(store.snapshot("p").unwrap().1, lines(&["totally different"]));
    }

    #[test]
    fn identical_rebaseline_is_a_no_op() {
        let mut store = CanvasStore::default();
        store.apply("p", lines(&["a"]), true);
        assert!(matches!(
            store.apply("p", lines(&["a"]), true),
            CanvasUpdate::Unchanged
        ));
    }

    #[test]
    fn forgetting_a_pane_drops_its_state() {
        let mut store = CanvasStore::default();
        store.apply("p", lines(&["a"]), true);
        assert!(store.knows("p"));
        store.forget("p");
        assert!(!store.knows("p"));
        assert!(store.snapshot("p").is_none());
        assert!(store.history("p", None, None).is_empty());
    }

    #[test]
    fn panes_are_tracked_independently() {
        let mut store = CanvasStore::default();
        store.apply("terminal_1", lines(&["one"]), true);
        store.apply("terminal_2", lines(&["two"]), true);
        store.apply("terminal_1", lines(&["one!"]), false);

        assert_eq!(store.snapshot("terminal_1").unwrap().0, 1);
        assert_eq!(store.snapshot("terminal_2").unwrap().0, 0);
        assert_eq!(store.history("terminal_2", None, None).len(), 0);

        let mut ids = store.pane_ids();
        ids.sort();
        assert_eq!(ids, vec!["terminal_1", "terminal_2"]);
    }
}

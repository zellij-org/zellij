//! Events that arrive for a pane before the screen has created it.
//!
//! A pane's PTY can produce output, exit or be held before the layout that
//! creates it is applied. Those events are buffered here and replayed in
//! order once the pane exists (or discarded if it is never created).
//!
//! Everything buffered here is bounded. Output comes from programs, and a
//! pane may never appear, but bounding must not rewrite what is retained:
//!
//! * Output is budgeted per pane and across all pending panes. Adjacent
//!   output for a pane is coalesced into one buffer, so the number of
//!   retained instructions does not grow with the number of writes. A chunk
//!   that would exceed either budget is dropped whole. The prefix already
//!   buffered stays, and the first overflow appends one truncation notice so
//!   that prefix is not replayed as a complete stream. Notice bytes are not
//!   counted against the budget.
//! * Lifecycle facts (exit, hold) are settlement. Duplicate exits collapse to
//!   the latest close, and duplicate holds collapse to the latest hold, so a
//!   flood cannot grow the queue. A fact for a pane that already has an entry
//!   is never dropped.
//! * The number of panes with anything buffered is capped. A new pane past
//!   the cap is not inserted; nothing already buffered is evicted to make
//!   room. Existing panes still accept events. Creation is also bounded
//!   before the process is spawned, so every admitted pane can still settle.

use super::ScreenInstruction;
use crate::panes::PaneId;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

/// Buffered output retained per not-yet-created pane.
pub(crate) const MAX_PENDING_OUTPUT_BYTES_PER_PANE: usize = 1024 * 1024;
/// Buffered output retained across all not-yet-created panes.
pub(crate) const MAX_PENDING_OUTPUT_BYTES_TOTAL: usize = 16 * 1024 * 1024;
/// Not-yet-created panes with anything buffered.
pub(crate) const MAX_PENDING_PANES: usize = 1024;

/// Appended once, as its own `PtyBytes`, the first time a pane's output is
/// truncated. Not counted against the byte budget, so a full buffer can still
/// carry it.
const OUTPUT_TRUNCATION_NOTICE: &[u8] =
    b"\r\n[zellij: output before this pane opened was cut off]\r\n";

/// Terminals whose panes have been admitted but not yet installed.
///
/// Separate from [`PendingPaneEvents`] so the cap is enforced before spawn
/// and so queue tests do not have to enroll.
static ADMITTED_UNINSTALLED_TERMINALS: LazyLock<Mutex<HashSet<u32>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

fn admitted_terminals() -> std::sync::MutexGuard<'static, HashSet<u32>> {
    ADMITTED_UNINSTALLED_TERMINALS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Reserve a slot for a terminal that is about to be spawned.
///
/// Idempotent for an id already reserved. A new id is refused once
/// [`MAX_PENDING_PANES`] terminals are already admitted.
pub(crate) fn try_admit_uninstalled_terminal(terminal_id: u32) -> bool {
    let mut admitted = admitted_terminals();
    if admitted.contains(&terminal_id) {
        return true;
    }
    if admitted.len() >= MAX_PENDING_PANES {
        return false;
    }
    admitted.insert(terminal_id);
    true
}

/// Drop a reservation. Removing an id that was never admitted does nothing.
pub(crate) fn release_uninstalled_terminal(terminal_id: u32) {
    admitted_terminals().remove(&terminal_id);
}

#[derive(Default)]
struct PendingPane {
    events: Vec<ScreenInstruction>,
    output_bytes: usize,
    /// A truncation notice was already appended for this pane.
    truncation_noted: bool,
    /// The last event is the truncation notice, so later output must not be
    /// coalesced into it.
    truncation_notice_is_tail: bool,
}

#[derive(Default)]
pub(crate) struct PendingPaneEvents {
    panes: HashMap<PaneId, PendingPane>,
    total_output_bytes: usize,
}

impl PendingPaneEvents {
    /// Buffer PTY output for a pane that does not exist yet.
    ///
    /// When the chunk would exceed either budget, it is dropped and the
    /// prefix already buffered is kept. The first time that happens, one
    /// truncation notice is appended. Its bytes are not charged to the budget.
    /// A chunk larger than the per-pane budget is never retained.
    pub fn push_output(&mut self, pid: PaneId, bytes: Vec<u8>) {
        let PaneId::Terminal(terminal_id) = pid else {
            return;
        };
        let len = bytes.len();
        if len == 0 {
            return;
        }
        if self.output_exceeds_budget(pid, len) {
            log::warn!(
                "dropping {len} new bytes of output for not-yet-created pane {pid:?}: \
                 pending output budget exceeded"
            );
            self.note_output_truncated(pid, terminal_id);
            return;
        }
        let Some(pane) = self.pane_entry(pid) else {
            return;
        };
        Self::append_output(pane, terminal_id, bytes);
        pane.output_bytes += len;
        self.total_output_bytes += len;
    }

    /// Buffer a lifecycle event (child exit, hold).
    ///
    /// `ClosePane` and `HoldPane` are settlement facts. A later event of the
    /// same kind replaces the earlier one in place, so repeats cannot grow
    /// the queue, but the fact is kept. A pane that already has an entry is
    /// never refused. A new pane past the cap is not inserted.
    pub fn push_lifecycle(&mut self, pid: PaneId, event: ScreenInstruction) {
        let Some(pane) = self.pane_entry(pid) else {
            return;
        };
        if let Some(existing) = pane
            .events
            .iter_mut()
            .find(|existing| same_settlement(existing, &event))
        {
            *existing = event;
            return;
        }
        pane.events.push(event);
    }

    /// Remove and return a pane's buffered events, in arrival order.
    pub fn take(&mut self, pid: PaneId) -> Vec<ScreenInstruction> {
        match self.panes.remove(&pid) {
            Some(pane) => {
                self.total_output_bytes = self.total_output_bytes.saturating_sub(pane.output_bytes);
                pane.events
            },
            None => vec![],
        }
    }

    /// Discard everything buffered for a pane that will never be created.
    pub fn discard(&mut self, pid: PaneId) {
        let _ = self.take(pid);
    }

    fn output_exceeds_budget(&self, pid: PaneId, len: usize) -> bool {
        let pane_bytes = self
            .panes
            .get(&pid)
            .map(|pane| pane.output_bytes)
            .unwrap_or(0);
        len > MAX_PENDING_OUTPUT_BYTES_PER_PANE
            || pane_bytes.saturating_add(len) > MAX_PENDING_OUTPUT_BYTES_PER_PANE
            || self.total_output_bytes.saturating_add(len) > MAX_PENDING_OUTPUT_BYTES_TOTAL
    }

    fn note_output_truncated(&mut self, pid: PaneId, terminal_id: u32) {
        let Some(pane) = self.pane_entry(pid) else {
            return;
        };
        if pane.truncation_noted {
            return;
        }
        pane.truncation_noted = true;
        pane.truncation_notice_is_tail = true;
        pane.events.push(ScreenInstruction::PtyBytes(
            terminal_id,
            OUTPUT_TRUNCATION_NOTICE.to_vec(),
        ));
    }

    fn append_output(pane: &mut PendingPane, terminal_id: u32, bytes: Vec<u8>) {
        if pane.truncation_notice_is_tail {
            pane.truncation_notice_is_tail = false;
            pane.events
                .push(ScreenInstruction::PtyBytes(terminal_id, bytes));
            return;
        }
        match pane.events.last_mut() {
            Some(ScreenInstruction::PtyBytes(_, previous)) => previous.extend_from_slice(&bytes),
            _ => pane
                .events
                .push(ScreenInstruction::PtyBytes(terminal_id, bytes)),
        }
    }

    /// The pane's entry. A new id is not inserted once [`MAX_PENDING_PANES`]
    /// panes are already buffered; nothing is evicted to make room.
    ///
    /// An id that was admitted before spawn is inserted even at the cap. Its
    /// exit has to land somewhere, and refusing it would drop the settlement
    /// of a process that was allowed to start. The map can then hold about
    /// twice the cap (admissions plus older buffered panes) and no more:
    /// admissions themselves stop at the cap.
    fn pane_entry(&mut self, pid: PaneId) -> Option<&mut PendingPane> {
        if !self.panes.contains_key(&pid) && self.panes.len() >= MAX_PENDING_PANES {
            let admitted = match pid {
                PaneId::Terminal(id) => admitted_terminals().contains(&id),
                PaneId::Plugin(_) => false,
            };
            if !admitted {
                log::warn!(
                    "not buffering events for not-yet-created pane {pid:?}: \
                     too many panes are pending"
                );
                return None;
            }
        }
        Some(self.panes.entry(pid).or_default())
    }

    /// Buffer output that belongs to one run. Adjacent chunks of the same
    /// terminal and generation coalesce. A different generation stays a
    /// separate event so replay can drop it after the run changes.
    pub fn push_run_output(&mut self, pid: PaneId, generation: u64, bytes: Vec<u8>) {
        let PaneId::Terminal(terminal_id) = pid else {
            return;
        };
        let len = bytes.len();
        if len == 0 {
            return;
        }
        if self.output_exceeds_budget(pid, len) {
            log::warn!(
                "dropping {len} new bytes of output for not-yet-created pane {pid:?}: \
                 pending output budget exceeded"
            );
            self.note_output_truncated(pid, terminal_id);
            return;
        }
        let Some(pane) = self.pane_entry(pid) else {
            return;
        };
        Self::append_run_output(pane, terminal_id, generation, bytes);
        pane.output_bytes += len;
        self.total_output_bytes += len;
    }

    fn append_run_output(
        pane: &mut PendingPane,
        terminal_id: u32,
        generation: u64,
        bytes: Vec<u8>,
    ) {
        if pane.truncation_notice_is_tail {
            pane.truncation_notice_is_tail = false;
            pane.events.push(ScreenInstruction::PtyRunBytes(
                terminal_id,
                generation,
                bytes,
            ));
            return;
        }
        match pane.events.last_mut() {
            Some(ScreenInstruction::PtyRunBytes(id, existing, previous))
                if *id == terminal_id && *existing == generation =>
            {
                previous.extend_from_slice(&bytes);
            },
            _ => pane.events.push(ScreenInstruction::PtyRunBytes(
                terminal_id,
                generation,
                bytes,
            )),
        }
    }

    #[cfg(test)]
    pub fn total_output_bytes(&self) -> usize {
        self.total_output_bytes
    }

    #[cfg(test)]
    pub fn pending_pane_count(&self) -> usize {
        self.panes.len()
    }

    #[cfg(test)]
    pub fn buffered_event_count(&self, pid: PaneId) -> usize {
        self.panes
            .get(&pid)
            .map(|pane| pane.events.len())
            .unwrap_or(0)
    }
}

fn same_settlement(existing: &ScreenInstruction, event: &ScreenInstruction) -> bool {
    matches!(
        (existing, event),
        (
            ScreenInstruction::ClosePane(..),
            ScreenInstruction::ClosePane(..)
        ) | (
            ScreenInstruction::HoldPane(..),
            ScreenInstruction::HoldPane(..)
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::ClosePaneSource;

    fn output(len: usize) -> Vec<u8> {
        vec![b'x'; len]
    }

    fn child_exit(pid: PaneId, status: i32) -> ScreenInstruction {
        ScreenInstruction::ClosePane(pid, None, None, Some(status), ClosePaneSource::ChildExit)
    }

    fn describe(events: &[ScreenInstruction]) -> Vec<String> {
        events
            .iter()
            .map(|event| match event {
                ScreenInstruction::PtyBytes(_, bytes) => format!("out:{}", bytes.len()),
                ScreenInstruction::ClosePane(_, _, _, status, _) => format!("exit:{status:?}"),
                _ => "other".to_owned(),
            })
            .collect()
    }

    fn payload_byte_count(events: &[ScreenInstruction]) -> usize {
        events
            .iter()
            .map(|event| match event {
                ScreenInstruction::PtyBytes(_, bytes) => {
                    bytes.iter().filter(|byte| **byte == b'x').count()
                },
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn events_replay_in_arrival_order() {
        let pid = PaneId::Terminal(1);
        let mut pending = PendingPaneEvents::default();
        pending.push_output(pid, output(10));
        pending.push_lifecycle(pid, child_exit(pid, 0));
        pending.push_output(pid, output(5));
        assert_eq!(
            describe(&pending.take(pid)),
            ["out:10", "exit:Some(0)", "out:5"]
        );
        assert_eq!(pending.total_output_bytes(), 0);
        assert!(pending.take(pid).is_empty());
    }

    #[test]
    fn overflowing_output_keeps_the_child_exit() {
        // A command that prints more than the budget and exits before its
        // pane exists must still be replayed as exited. The prefix that fit
        // stays; the chunk that does not fit is absent; a notice marks the cut.
        let pid = PaneId::Terminal(1);
        let mut pending = PendingPaneEvents::default();
        let chunk = MAX_PENDING_OUTPUT_BYTES_PER_PANE / 2;
        pending.push_output(pid, output(chunk));
        pending.push_lifecycle(pid, child_exit(pid, 3));
        pending.push_output(pid, output(chunk));
        pending.push_output(pid, output(chunk));
        assert_eq!(pending.total_output_bytes(), chunk * 2);
        let events = pending.take(pid);
        assert_eq!(
            describe(&events),
            vec![
                format!("out:{chunk}"),
                "exit:Some(3)".to_owned(),
                format!("out:{chunk}"),
                format!("out:{}", OUTPUT_TRUNCATION_NOTICE.len()),
            ]
        );
        assert_eq!(payload_byte_count(&events), chunk * 2);
        assert!(matches!(
            events.last(),
            Some(ScreenInstruction::PtyBytes(_, bytes)) if bytes.as_slice() == OUTPUT_TRUNCATION_NOTICE
        ));
    }

    #[test]
    fn output_is_bounded_across_panes() {
        let mut pending = PendingPaneEvents::default();
        let chunk = MAX_PENDING_OUTPUT_BYTES_PER_PANE;
        let panes = MAX_PENDING_OUTPUT_BYTES_TOTAL / chunk + 4;
        for id in 0..panes as u32 {
            let pid = PaneId::Terminal(id);
            pending.push_lifecycle(pid, child_exit(pid, 0));
            pending.push_output(pid, output(chunk));
            assert!(pending.total_output_bytes() <= MAX_PENDING_OUTPUT_BYTES_TOTAL);
        }
        // Panes that did not fit still keep their lifecycle facts.
        let last = PaneId::Terminal(panes as u32 - 1);
        let last_events = pending.take(last);
        assert_eq!(
            describe(&last_events),
            vec![
                "exit:Some(0)".to_owned(),
                format!("out:{}", OUTPUT_TRUNCATION_NOTICE.len()),
            ]
        );
        assert_eq!(payload_byte_count(&last_events), 0);
        assert!(pending.total_output_bytes() <= MAX_PENDING_OUTPUT_BYTES_TOTAL);
    }

    #[test]
    fn a_single_oversized_chunk_is_not_retained() {
        let pid = PaneId::Terminal(1);
        let mut pending = PendingPaneEvents::default();
        let len = MAX_PENDING_OUTPUT_BYTES_PER_PANE + 1;
        pending.push_output(pid, output(len));
        assert_eq!(pending.total_output_bytes(), 0);
        let events = pending.take(pid);
        assert_eq!(payload_byte_count(&events), 0);
        assert!(events.iter().all(|event| match event {
            ScreenInstruction::PtyBytes(_, bytes) => bytes.as_slice() != output(len).as_slice(),
            _ => true,
        }));
        assert!(matches!(
            events.as_slice(),
            [ScreenInstruction::PtyBytes(_, bytes)] if bytes.as_slice() == OUTPUT_TRUNCATION_NOTICE
        ));
    }

    #[test]
    fn discarding_releases_the_budget() {
        let pid = PaneId::Terminal(1);
        let mut pending = PendingPaneEvents::default();
        pending.push_output(pid, output(100));
        pending.discard(pid);
        assert_eq!(pending.total_output_bytes(), 0);
        assert!(pending.take(pid).is_empty());
    }

    #[test]
    fn many_small_writes_are_coalesced_into_one_instruction() {
        // Paced one-byte writes must not retain one instruction each.
        let pid = PaneId::Terminal(1);
        let mut pending = PendingPaneEvents::default();
        for _ in 0..10_000 {
            pending.push_output(pid, output(1));
        }
        assert_eq!(pending.buffered_event_count(pid), 1);
        assert_eq!(pending.total_output_bytes(), 10_000);
        assert_eq!(describe(&pending.take(pid)), ["out:10000"]);
    }

    #[test]
    fn retained_instructions_are_bounded_when_output_alternates_with_exits() {
        let pid = PaneId::Terminal(1);
        let mut pending = PendingPaneEvents::default();
        for status in 0..1_000 {
            pending.push_output(pid, output(1));
            pending.push_lifecycle(pid, child_exit(pid, status));
        }
        assert_eq!(pending.total_output_bytes(), 1_000);
        // Exits collapse in place. Output coalesces only while it is adjacent,
        // so the first byte stays on the other side of the exit.
        assert_eq!(pending.buffered_event_count(pid), 3);
        assert_eq!(
            describe(&pending.take(pid)),
            ["out:1", "exit:Some(999)", "out:999"]
        );
    }

    #[test]
    fn the_number_of_pending_panes_is_bounded() {
        // Lifecycle events for panes that never appear must not accumulate
        // forever, and must not evict a pane that is already buffered.
        // Ids sit above the admission test's range so a concurrent admission
        // of the low ids cannot make this map accept an extra pane.
        let base = 50_000u32;
        let mut pending = PendingPaneEvents::default();
        for id in base..(base + MAX_PENDING_PANES as u32 * 3) {
            let pid = PaneId::Terminal(id);
            pending.push_lifecycle(pid, child_exit(pid, 0));
            assert!(pending.pending_pane_count() <= MAX_PENDING_PANES);
        }
        assert_eq!(pending.pending_pane_count(), MAX_PENDING_PANES);
        assert_eq!(
            describe(&pending.take(PaneId::Terminal(base))),
            ["exit:Some(0)"]
        );
        assert!(pending
            .take(PaneId::Terminal(base + MAX_PENDING_PANES as u32))
            .is_empty());
        assert!(pending.pending_pane_count() <= MAX_PENDING_PANES);
    }

    #[test]
    fn an_admitted_terminal_can_settle_when_the_pending_map_is_full() {
        const ADMITTED: u32 = 900_000;
        struct ClearAdmitted;
        impl Drop for ClearAdmitted {
            fn drop(&mut self) {
                release_uninstalled_terminal(ADMITTED);
            }
        }
        let _clear = ClearAdmitted;

        let base = 800_000u32;
        let mut pending = PendingPaneEvents::default();
        for id in base..(base + MAX_PENDING_PANES as u32) {
            let pid = PaneId::Terminal(id);
            pending.push_lifecycle(pid, child_exit(pid, 0));
        }
        assert_eq!(pending.pending_pane_count(), MAX_PENDING_PANES);
        let refused = PaneId::Terminal(base + MAX_PENDING_PANES as u32);
        pending.push_lifecycle(refused, child_exit(refused, 1));
        assert!(pending.take(refused).is_empty());

        assert!(try_admit_uninstalled_terminal(ADMITTED));
        let pid = PaneId::Terminal(ADMITTED);
        pending.push_lifecycle(pid, child_exit(pid, 7));
        assert_eq!(describe(&pending.take(pid)), ["exit:Some(7)"]);
        // The first of the already-buffered panes was not evicted to make room.
        assert_eq!(
            describe(&pending.take(PaneId::Terminal(base))),
            ["exit:Some(0)"]
        );
    }

    #[test]
    fn lifecycle_events_per_pane_are_bounded() {
        let pid = PaneId::Terminal(1);
        let mut pending = PendingPaneEvents::default();
        for status in 0..100 {
            pending.push_lifecycle(pid, child_exit(pid, status));
        }
        assert_eq!(describe(&pending.take(pid)), ["exit:Some(99)"]);
    }

    #[test]
    fn admitted_terminals_are_bounded_and_releasing_frees_a_slot() {
        struct ClearAdmittedTerminals;
        impl Drop for ClearAdmittedTerminals {
            fn drop(&mut self) {
                for id in 0..=MAX_PENDING_PANES as u32 + 1 {
                    release_uninstalled_terminal(id);
                }
                release_uninstalled_terminal(u32::MAX);
            }
        }
        let _clear = ClearAdmittedTerminals;

        // Releasing an id that was never admitted is a no-op.
        release_uninstalled_terminal(u32::MAX);

        for id in 0..MAX_PENDING_PANES as u32 {
            assert!(
                try_admit_uninstalled_terminal(id),
                "terminal {id} should be admitted"
            );
        }
        // An id already reserved stays reserved at the cap.
        assert!(try_admit_uninstalled_terminal(0));
        let rejected = MAX_PENDING_PANES as u32;
        assert!(!try_admit_uninstalled_terminal(rejected));
        // Releasing an id that was refused must not free someone else's slot.
        release_uninstalled_terminal(rejected);
        assert!(!try_admit_uninstalled_terminal(rejected + 1));

        release_uninstalled_terminal(0);
        assert!(try_admit_uninstalled_terminal(rejected));
        assert!(!try_admit_uninstalled_terminal(rejected + 1));
        release_uninstalled_terminal(rejected);
        assert!(try_admit_uninstalled_terminal(0));
        assert!(!try_admit_uninstalled_terminal(rejected));
    }
}

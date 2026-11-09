//! Events that arrive for a pane before the screen has created it.
//!
//! A pane's PTY can produce output, exit or be held before the layout that
//! creates it is applied. Those events are buffered here and replayed in
//! order once the pane exists (or discarded if it is never created).
//!
//! Everything buffered here is bounded, since the events come from programs
//! and from panes that may never appear:
//!
//! * Output is budgeted per pane and across all pending panes. Adjacent
//!   output for a pane is coalesced into one buffer, so the number of
//!   retained instructions does not grow with the number of writes.
//! * Lifecycle facts (exit, hold) are never dropped to make room for output:
//!   losing one would leave a pane that never shows as exited and blocking
//!   waiters that never complete. They are limited per pane.
//! * The number of panes with anything buffered is capped, evicting the pane
//!   that has been waiting longest. This covers events for panes that will
//!   never be created, such as the late exit of a long-closed terminal.

use super::ScreenInstruction;
use crate::panes::PaneId;
use std::collections::{HashMap, VecDeque};

/// Buffered output retained per not-yet-created pane.
pub(crate) const MAX_PENDING_OUTPUT_BYTES_PER_PANE: usize = 1024 * 1024;
/// Buffered output retained across all not-yet-created panes.
pub(crate) const MAX_PENDING_OUTPUT_BYTES_TOTAL: usize = 16 * 1024 * 1024;
/// Lifecycle events (exit, hold) retained per not-yet-created pane.
pub(crate) const MAX_PENDING_LIFECYCLE_EVENTS_PER_PANE: usize = 4;
/// Not-yet-created panes with anything buffered.
pub(crate) const MAX_PENDING_PANES: usize = 1024;

#[derive(Default)]
struct PendingPane {
    events: Vec<ScreenInstruction>,
    output_bytes: usize,
    lifecycle_events: usize,
}

#[derive(Default)]
pub(crate) struct PendingPaneEvents {
    panes: HashMap<PaneId, PendingPane>,
    /// Panes with buffered events, longest waiting first.
    arrival_order: VecDeque<PaneId>,
    total_output_bytes: usize,
}

impl PendingPaneEvents {
    /// Buffer PTY output for a pane that does not exist yet.
    ///
    /// When the output would exceed either budget, the pane's previously
    /// buffered output is dropped (its lifecycle events are kept, in order)
    /// and the new chunk is retained if it fits on its own. The pane then
    /// starts from the most recent output instead of growing without bound.
    pub fn push_output(&mut self, pid: PaneId, bytes: Vec<u8>) {
        let PaneId::Terminal(terminal_id) = pid else {
            return;
        };
        let len = bytes.len();
        if len == 0 {
            return;
        }
        let pane_bytes = self.panes.get(&pid).map(|p| p.output_bytes).unwrap_or(0);
        if pane_bytes.saturating_add(len) > MAX_PENDING_OUTPUT_BYTES_PER_PANE
            || self.total_output_bytes.saturating_add(len) > MAX_PENDING_OUTPUT_BYTES_TOTAL
        {
            self.drop_output(pid);
            let fits = len <= MAX_PENDING_OUTPUT_BYTES_PER_PANE
                && self.total_output_bytes.saturating_add(len) <= MAX_PENDING_OUTPUT_BYTES_TOTAL;
            let dropped = if fits { pane_bytes } else { pane_bytes + len };
            log::warn!(
                "dropping {dropped} bytes of output for not-yet-created pane {pid:?}: \
                 pending output budget exceeded"
            );
            if !fits {
                return;
            }
        }
        let pane = self.pane_entry(pid);
        match pane.events.last_mut() {
            Some(ScreenInstruction::PtyBytes(_, previous)) => previous.extend_from_slice(&bytes),
            _ => pane
                .events
                .push(ScreenInstruction::PtyBytes(terminal_id, bytes)),
        }
        pane.output_bytes += len;
        self.total_output_bytes += len;
    }

    /// Buffer a lifecycle event (child exit, hold). Never dropped to make
    /// room for output.
    pub fn push_lifecycle(&mut self, pid: PaneId, event: ScreenInstruction) {
        let pane = self.pane_entry(pid);
        if pane.lifecycle_events >= MAX_PENDING_LIFECYCLE_EVENTS_PER_PANE {
            log::warn!("dropping a lifecycle event for not-yet-created pane {pid:?}: too many");
            return;
        }
        pane.lifecycle_events += 1;
        pane.events.push(event);
    }

    /// Remove and return a pane's buffered events, in arrival order.
    pub fn take(&mut self, pid: PaneId) -> Vec<ScreenInstruction> {
        match self.panes.remove(&pid) {
            Some(pane) => {
                self.total_output_bytes = self.total_output_bytes.saturating_sub(pane.output_bytes);
                self.arrival_order.retain(|queued| *queued != pid);
                pane.events
            },
            None => vec![],
        }
    }

    /// Discard everything buffered for a pane that will never be created.
    pub fn discard(&mut self, pid: PaneId) {
        let _ = self.take(pid);
    }

    /// The pane's entry, created (evicting the longest waiting pane if too
    /// many are pending) if it has nothing buffered yet.
    fn pane_entry(&mut self, pid: PaneId) -> &mut PendingPane {
        if !self.panes.contains_key(&pid) {
            while self.panes.len() >= MAX_PENDING_PANES {
                let Some(oldest) = self.arrival_order.front().copied() else {
                    break;
                };
                log::warn!(
                    "dropping the events buffered for not-yet-created pane {oldest:?}: \
                     too many panes are pending"
                );
                self.discard(oldest);
            }
            self.arrival_order.push_back(pid);
        }
        self.panes.entry(pid).or_default()
    }

    fn drop_output(&mut self, pid: PaneId) {
        let Some(pane) = self.panes.get_mut(&pid) else {
            return;
        };
        self.total_output_bytes = self.total_output_bytes.saturating_sub(pane.output_bytes);
        pane.output_bytes = 0;
        // Rebuild rather than retain in place so the dropped output's
        // allocation is released along with its accounting.
        let events = std::mem::take(&mut pane.events);
        let mut kept = Vec::new();
        for event in events {
            if !matches!(event, ScreenInstruction::PtyBytes(..)) {
                kept.push(event);
            }
        }
        pane.events = kept;
        if pane.events.is_empty() {
            self.panes.remove(&pid);
            self.arrival_order.retain(|queued| *queued != pid);
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
        self.panes.get(&pid).map(|p| p.events.len()).unwrap_or(0)
    }
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
        // pane exists must still be replayed as exited.
        let pid = PaneId::Terminal(1);
        let mut pending = PendingPaneEvents::default();
        let chunk = MAX_PENDING_OUTPUT_BYTES_PER_PANE / 2;
        pending.push_output(pid, output(chunk));
        pending.push_lifecycle(pid, child_exit(pid, 3));
        pending.push_output(pid, output(chunk));
        pending.push_output(pid, output(chunk));
        assert_eq!(
            describe(&pending.take(pid)),
            ["exit:Some(3)", &format!("out:{chunk}")]
        );
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
        assert_eq!(describe(&pending.take(last)), ["exit:Some(0)"]);
    }

    #[test]
    fn a_single_oversized_chunk_is_not_retained() {
        let pid = PaneId::Terminal(1);
        let mut pending = PendingPaneEvents::default();
        let len = MAX_PENDING_OUTPUT_BYTES_PER_PANE + 1;
        pending.push_output(pid, output(len));
        assert_eq!(pending.total_output_bytes(), 0);
        assert!(pending.take(pid).is_empty());
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
        for _ in 0..1_000 {
            pending.push_output(pid, output(1));
            pending.push_lifecycle(pid, child_exit(pid, 0));
        }
        assert!(pending.buffered_event_count(pid) <= 2 * MAX_PENDING_LIFECYCLE_EVENTS_PER_PANE + 1);
    }

    #[test]
    fn the_number_of_pending_panes_is_bounded() {
        // Lifecycle events for panes that never appear (e.g. the late exit of
        // a long-closed terminal) must not accumulate forever.
        let mut pending = PendingPaneEvents::default();
        for id in 0..(MAX_PENDING_PANES as u32 * 3) {
            let pid = PaneId::Terminal(id);
            pending.push_lifecycle(pid, child_exit(pid, 0));
            assert!(pending.pending_pane_count() <= MAX_PENDING_PANES);
        }
        // The longest waiting panes were the ones evicted.
        assert!(pending.take(PaneId::Terminal(0)).is_empty());
        let newest = PaneId::Terminal(MAX_PENDING_PANES as u32 * 3 - 1);
        assert_eq!(describe(&pending.take(newest)), ["exit:Some(0)"]);
    }

    #[test]
    fn lifecycle_events_per_pane_are_bounded() {
        let pid = PaneId::Terminal(1);
        let mut pending = PendingPaneEvents::default();
        for _ in 0..100 {
            pending.push_lifecycle(pid, child_exit(pid, 0));
        }
        assert_eq!(
            pending.take(pid).len(),
            MAX_PENDING_LIFECYCLE_EVENTS_PER_PANE
        );
    }
}

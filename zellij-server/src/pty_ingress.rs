//! Byte budget for pty writes admitted onto the writer inbox.
//!
//! The inbox itself stays unbounded so control instructions are never blocked
//! behind a full terminal. `PtyWriteSender::send` reserves the backing length
//! of a `Write` before that send, and the writer releases the same length
//! when the write leaves the queue.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Per-terminal cap. Matches the writer queue's `MAX_PENDING_BYTES` defense.
pub(crate) const PER_TERMINAL_LIMIT: usize = 10 * 1024 * 1024;

/// Cap across every terminal sharing one session's pty writer.
pub(crate) const AGGREGATE_LIMIT: usize = 32 * 1024 * 1024;

const REJECTED_BEFORE_QUEUE: &str =
    "the write was rejected before it was queued because the pty pending-byte limit was reached";

#[derive(Default)]
struct IngressState {
    per_terminal: HashMap<u32, usize>,
    aggregate: usize,
}

/// Cheap to clone: every handle shares one mutex.
#[derive(Clone)]
pub struct PtyIngressBudget {
    state: Arc<Mutex<IngressState>>,
}

impl PtyIngressBudget {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(IngressState::default())),
        }
    }

    /// Reserve `len` against both limits, or leave both unchanged.
    /// `len == 0` reserves nothing.
    pub fn try_reserve(&self, terminal_id: u32, len: usize) -> Result<(), String> {
        if len == 0 {
            return Ok(());
        }
        let mut state = self.lock();
        let current = state.per_terminal.get(&terminal_id).copied().unwrap_or(0);
        let Some(next_terminal) = current.checked_add(len) else {
            return Err(REJECTED_BEFORE_QUEUE.to_string());
        };
        let Some(next_aggregate) = state.aggregate.checked_add(len) else {
            return Err(REJECTED_BEFORE_QUEUE.to_string());
        };
        if next_terminal > PER_TERMINAL_LIMIT || next_aggregate > AGGREGATE_LIMIT {
            return Err(REJECTED_BEFORE_QUEUE.to_string());
        }
        state.per_terminal.insert(terminal_id, next_terminal);
        state.aggregate = next_aggregate;
        Ok(())
    }

    /// Give back a reservation. A second release of the same bytes saturates
    /// at zero and does not subtract from any other terminal.
    pub fn release(&self, terminal_id: u32, len: usize) {
        if len == 0 {
            return;
        }
        let mut state = self.lock();
        let current = state.per_terminal.get(&terminal_id).copied().unwrap_or(0);
        let released = len.min(current);
        let next = current - released;
        if next == 0 {
            state.per_terminal.remove(&terminal_id);
        } else {
            state.per_terminal.insert(terminal_id, next);
        }
        state.aggregate = state.aggregate.saturating_sub(released);
    }

    #[cfg(test)]
    pub(crate) fn reserved_bytes(&self) -> usize {
        self.lock().aggregate
    }

    #[cfg(test)]
    pub(crate) fn terminal_reserved(&self, terminal_id: u32) -> usize {
        self.lock()
            .per_terminal
            .get(&terminal_id)
            .copied()
            .unwrap_or(0)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, IngressState> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_ingress_zero_len_reserves_nothing_and_release_saturates() {
        let budget = PtyIngressBudget::new();
        budget.try_reserve(1, 0).unwrap();
        assert_eq!(budget.reserved_bytes(), 0);
        budget.try_reserve(1, 5).unwrap();
        budget.release(1, 5);
        budget.release(1, 5);
        assert_eq!(budget.reserved_bytes(), 0);
        assert_eq!(budget.terminal_reserved(1), 0);
    }

    #[test]
    fn pty_ingress_failed_reserve_leaves_both_limits_unchanged() {
        let budget = PtyIngressBudget::new();
        budget.try_reserve(1, PER_TERMINAL_LIMIT).unwrap();
        let error = budget.try_reserve(1, 1).unwrap_err();
        assert!(error.contains("rejected before it was queued"), "{error}");
        assert_eq!(budget.terminal_reserved(1), PER_TERMINAL_LIMIT);
        assert_eq!(budget.reserved_bytes(), PER_TERMINAL_LIMIT);
        budget.try_reserve(2, 1).unwrap();
        assert_eq!(budget.reserved_bytes(), PER_TERMINAL_LIMIT + 1);
    }
}

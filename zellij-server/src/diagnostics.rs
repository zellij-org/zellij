use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use zellij_utils::errors::ScreenContext;

const SLOW_SCREEN_OPERATION_THRESHOLD: Duration = Duration::from_millis(100);

static SCREEN_MAX_OPERATION_MICROS: AtomicU64 = AtomicU64::new(0);
static SCREEN_SLOW_OPERATIONS: AtomicU64 = AtomicU64::new(0);
static PLUGIN_MAX_JOBS_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
static PTY_MAX_PENDING_BYTES: AtomicUsize = AtomicUsize::new(0);

struct LiveScreenOperation {
    action: ScreenContext,
    started: Instant,
}

/// The screen operation inside a live [`ScreenOperationGuard`]. Written when
/// the guard is created and cleared on drop only if this is still that same
/// guard, so a stuck operation that never returns stays visible to
/// [`snapshot`].
static CURRENT_SCREEN_OPERATION: Mutex<Option<LiveScreenOperation>> = Mutex::new(None);

fn current_screen_operation() -> std::sync::MutexGuard<'static, Option<LiveScreenOperation>> {
    CURRENT_SCREEN_OPERATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct ScreenOperationGuard {
    context: ScreenContext,
    started: Instant,
}

impl ScreenOperationGuard {
    pub fn new(context: ScreenContext) -> Self {
        let started = Instant::now();
        *current_screen_operation() = Some(LiveScreenOperation {
            action: context,
            started,
        });
        Self { context, started }
    }
}

impl Drop for ScreenOperationGuard {
    fn drop(&mut self) {
        {
            let mut current = current_screen_operation();
            let same_guard = current
                .as_ref()
                .is_some_and(|live| live.action == self.context && live.started == self.started);
            if same_guard {
                *current = None;
            }
        }
        let elapsed = self.started.elapsed();
        let micros = elapsed.as_micros().min(u64::MAX as u128) as u64;
        SCREEN_MAX_OPERATION_MICROS.fetch_max(micros, Ordering::Relaxed);
        if elapsed >= SLOW_SCREEN_OPERATION_THRESHOLD {
            SCREEN_SLOW_OPERATIONS.fetch_add(1, Ordering::Relaxed);
            let counters = snapshot();
            log::warn!(
                "Slow screen operation {:?} took {} ms (slowest so far: {} us, slow operations so far: {})",
                self.context,
                elapsed.as_millis(),
                counters.screen_max_operation_micros,
                counters.screen_slow_operations
            );
        }
    }
}

pub fn record_plugin_jobs_in_flight(jobs: usize) {
    PLUGIN_MAX_JOBS_IN_FLIGHT.fetch_max(jobs, Ordering::Relaxed);
}

pub fn record_pty_pending_bytes(bytes: usize) {
    PTY_MAX_PENDING_BYTES.fetch_max(bytes, Ordering::Relaxed);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiagnosticsSnapshot {
    pub screen_max_operation_micros: u64,
    pub screen_slow_operations: u64,
    pub plugin_max_jobs_in_flight: usize,
    pub pty_max_pending_bytes: usize,
    /// Screen operation currently inside a [`ScreenOperationGuard`], if any.
    /// Present for a stuck operation because the live record is written when
    /// the guard is created, not when it drops.
    pub current_screen_action: Option<ScreenContext>,
    /// How long [`Self::current_screen_action`] has been running at the
    /// moment of this snapshot. `None` when no operation is in progress.
    pub current_screen_action_age: Option<Duration>,
}

/// Read the counters. Each counter is individually atomic, but the four
/// loads are independent, so the snapshot is best-effort and not a
/// consistent cut: a concurrent update can be visible in one field and not
/// in another. That is fine for high-water marks and event counts, and keeps
/// those recording paths lock-free.
///
/// `current_screen_action` and `current_screen_action_age` come from the
/// mutex-protected live record. The age is how long the operation has been
/// running at the moment of the snapshot. A stuck operation stays visible
/// because that record is written when the guard is created, not when it drops.
pub fn snapshot() -> DiagnosticsSnapshot {
    let (current_screen_action, current_screen_action_age) = {
        let current = current_screen_operation();
        match current.as_ref() {
            Some(live) => (Some(live.action), Some(live.started.elapsed())),
            None => (None, None),
        }
    };
    DiagnosticsSnapshot {
        screen_max_operation_micros: SCREEN_MAX_OPERATION_MICROS.load(Ordering::Relaxed),
        screen_slow_operations: SCREEN_SLOW_OPERATIONS.load(Ordering::Relaxed),
        plugin_max_jobs_in_flight: PLUGIN_MAX_JOBS_IN_FLIGHT.load(Ordering::Relaxed),
        pty_max_pending_bytes: PTY_MAX_PENDING_BYTES.load(Ordering::Relaxed),
        current_screen_action,
        current_screen_action_age,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_water_counters_never_decrease() {
        record_plugin_jobs_in_flight(3);
        record_plugin_jobs_in_flight(1);
        record_pty_pending_bytes(4096);
        record_pty_pending_bytes(1024);

        let snapshot = snapshot();
        assert!(snapshot.plugin_max_jobs_in_flight >= 3);
        assert!(snapshot.pty_max_pending_bytes >= 4096);
    }

    #[test]
    fn current_screen_operation_is_visible_until_the_guard_drops() {
        let guard = ScreenOperationGuard::new(ScreenContext::Render);
        let during = snapshot();
        assert_eq!(during.current_screen_action, Some(ScreenContext::Render));
        assert!(during.current_screen_action_age.is_some());
        drop(guard);
        let after = snapshot();
        assert_eq!(after.current_screen_action, None);
        assert_eq!(after.current_screen_action_age, None);
    }
}

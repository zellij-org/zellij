use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use zellij_utils::errors::ScreenContext;

const SLOW_SCREEN_OPERATION_THRESHOLD: Duration = Duration::from_millis(100);
const STUCK_SCREEN_OPERATION_THRESHOLD: Duration = Duration::from_secs(5);
const SCREEN_OPERATION_REPORTER_INTERVAL: Duration = Duration::from_secs(1);

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

fn record_live_screen_operation(action: ScreenContext, started: Instant) {
    *current_screen_operation() = Some(LiveScreenOperation { action, started });
}

/// Start time of the stuck operation already logged. A later start is a
/// different operation. Cleared when no operation is live.
static REPORTED_STUCK_SCREEN_OPERATION: Mutex<Option<Instant>> = Mutex::new(None);

fn reported_stuck_screen_operation() -> std::sync::MutexGuard<'static, Option<Instant>> {
    REPORTED_STUCK_SCREEN_OPERATION
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
        record_live_screen_operation(context, started);
        Self { context, started }
    }

    /// Same record as [`Self::new`], with a caller-supplied start. Tests use
    /// this to install an already-old operation without waiting.
    #[cfg(test)]
    fn started_at(context: ScreenContext, started: Instant) -> Self {
        record_live_screen_operation(context, started);
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

/// Log a screen operation that has been inside its guard for more than 5s.
/// Returns whether this call logged. The same start time logs once; a new
/// start time is a new operation. Reads [`snapshot`] and does not run on the
/// screen thread, so a guard that never drops still produces the log.
pub(crate) fn report_stuck_screen_operation() -> bool {
    let snap = snapshot();
    let Some(age) = snap.current_screen_action_age else {
        *reported_stuck_screen_operation() = None;
        return false;
    };
    if age <= STUCK_SCREEN_OPERATION_THRESHOLD {
        return false;
    }
    let Some(action) = snap.current_screen_action else {
        return false;
    };
    let started = {
        let live = current_screen_operation();
        match live.as_ref() {
            Some(live)
                if live.action == action
                    && live.started.elapsed() > STUCK_SCREEN_OPERATION_THRESHOLD =>
            {
                Some(live.started)
            },
            _ => None,
        }
    };
    let Some(started) = started else {
        return false;
    };
    let mut reported = reported_stuck_screen_operation();
    if *reported == Some(started) {
        return false;
    }
    *reported = Some(started);
    log::warn!(
        "Stuck screen operation {:?} has been running for {} ms",
        action,
        age.as_millis(),
    );
    true
}

/// Spawn the stuck-operation reporter once. The thread is not the screen
/// thread and is not joined on shutdown.
pub(crate) fn start_screen_operation_reporter() {
    static START: Once = Once::new();
    START.call_once(|| {
        std::thread::Builder::new()
            .name("screen_operation_reporter".to_string())
            .spawn(|| loop {
                std::thread::sleep(SCREEN_OPERATION_REPORTER_INTERVAL);
                report_stuck_screen_operation();
            })
            .expect("failed to spawn screen operation reporter");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostics_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn backdated(age: Duration) -> Instant {
        Instant::now()
            .checked_sub(age)
            .expect("monotonic clock can represent this age")
    }

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
        let _lock = diagnostics_test_lock();
        let guard = ScreenOperationGuard::new(ScreenContext::Render);
        let during = snapshot();
        assert_eq!(during.current_screen_action, Some(ScreenContext::Render));
        assert!(during.current_screen_action_age.is_some());
        drop(guard);
        let after = snapshot();
        assert_eq!(after.current_screen_action, None);
        assert_eq!(after.current_screen_action_age, None);
    }

    #[test]
    fn screen_operation_wedge_is_reported_once_per_start_time() {
        let _lock = diagnostics_test_lock();
        *reported_stuck_screen_operation() = None;

        let fresh = ScreenOperationGuard::new(ScreenContext::Render);
        assert!(
            !report_stuck_screen_operation(),
            "an operation younger than 5s is not a wedge"
        );
        drop(fresh);

        let started = backdated(Duration::from_secs(6));
        let guard = ScreenOperationGuard::started_at(ScreenContext::Render, started);
        assert!(
            report_stuck_screen_operation(),
            "a live operation older than 5s is reported without the screen thread"
        );
        assert!(
            !report_stuck_screen_operation(),
            "the same start time is not reported again"
        );
        drop(guard);

        let again = backdated(Duration::from_secs(6));
        let guard = ScreenOperationGuard::started_at(ScreenContext::HandlePtyBytes, again);
        assert!(
            report_stuck_screen_operation(),
            "a new start time is a new stuck operation"
        );
        assert!(!report_stuck_screen_operation());
        drop(guard);
    }
}

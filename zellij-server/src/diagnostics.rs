use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use zellij_utils::errors::ScreenContext;

const SLOW_SCREEN_OPERATION_THRESHOLD: Duration = Duration::from_millis(100);

static SCREEN_MAX_OPERATION_MICROS: AtomicU64 = AtomicU64::new(0);
static SCREEN_SLOW_OPERATIONS: AtomicU64 = AtomicU64::new(0);
static PLUGIN_MAX_JOBS_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
static PTY_MAX_PENDING_BYTES: AtomicUsize = AtomicUsize::new(0);

pub struct ScreenOperationGuard {
    context: ScreenContext,
    started: Instant,
}

impl ScreenOperationGuard {
    pub fn new(context: ScreenContext) -> Self {
        Self {
            context,
            started: Instant::now(),
        }
    }
}

impl Drop for ScreenOperationGuard {
    fn drop(&mut self) {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticsSnapshot {
    pub screen_max_operation_micros: u64,
    pub screen_slow_operations: u64,
    pub plugin_max_jobs_in_flight: usize,
    pub pty_max_pending_bytes: usize,
}

/// Read the counters. Each counter is individually atomic, but the four
/// loads are independent, so the snapshot is best-effort and not a
/// consistent cut: a concurrent update can be visible in one field and not
/// in another. That is fine for high-water marks and event counts, and keeps
/// the recording paths lock-free.
pub fn snapshot() -> DiagnosticsSnapshot {
    DiagnosticsSnapshot {
        screen_max_operation_micros: SCREEN_MAX_OPERATION_MICROS.load(Ordering::Relaxed),
        screen_slow_operations: SCREEN_SLOW_OPERATIONS.load(Ordering::Relaxed),
        plugin_max_jobs_in_flight: PLUGIN_MAX_JOBS_IN_FLIGHT.load(Ordering::Relaxed),
        pty_max_pending_bytes: PTY_MAX_PENDING_BYTES.load(Ordering::Relaxed),
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
}

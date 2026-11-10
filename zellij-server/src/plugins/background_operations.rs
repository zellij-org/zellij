use crate::plugins::PluginId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use tokio::task::AbortHandle;

const MAX_RUN_COMMANDS_PER_PLUGIN: usize = 4;
const MAX_RUN_COMMANDS_IN_FLIGHT: usize = 16;
const MAX_RUN_COMMAND_BYTES_PER_PLUGIN: usize = 4 * 1024 * 1024;
const MAX_RUN_COMMAND_BYTES_IN_FLIGHT: usize = 16 * 1024 * 1024;
const MAX_WEB_REQUESTS_PER_PLUGIN: usize = 4;
const MAX_WEB_REQUESTS_IN_FLIGHT: usize = 16;
const MAX_WEB_REQUEST_BYTES_PER_PLUGIN: usize = 4 * 1024 * 1024;
const MAX_WEB_REQUEST_BYTES_IN_FLIGHT: usize = 16 * 1024 * 1024;
// Refusal messages do not consume an execution slot, but they must not become
// an unbounded stream when a plugin keeps asking after its cap is exhausted.
const MAX_REFUSAL_REPORTS_PER_PLUGIN: usize = 4;
const MAX_REFUSAL_REPORTS_IN_FLIGHT: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BackgroundOperationKind {
    RunCommand,
    WebRequest,
    RefusalReport,
}

#[derive(Default)]
struct Quota {
    total_jobs: usize,
    total_bytes: usize,
    per_plugin: HashMap<PluginId, (usize, usize)>,
}

impl Quota {
    fn try_admit(
        &mut self,
        plugin_id: PluginId,
        bytes: usize,
        max_jobs_per_plugin: usize,
        max_jobs: usize,
        max_bytes_per_plugin: usize,
        max_bytes: usize,
    ) -> bool {
        let (plugin_jobs, plugin_bytes) =
            self.per_plugin.get(&plugin_id).copied().unwrap_or((0, 0));
        if self.total_jobs >= max_jobs
            || plugin_jobs >= max_jobs_per_plugin
            || self.total_bytes.saturating_add(bytes) > max_bytes
            || plugin_bytes.saturating_add(bytes) > max_bytes_per_plugin
        {
            return false;
        }
        self.total_jobs += 1;
        self.total_bytes = self.total_bytes.saturating_add(bytes);
        self.per_plugin.insert(
            plugin_id,
            (plugin_jobs + 1, plugin_bytes.saturating_add(bytes)),
        );
        true
    }

    fn release(&mut self, plugin_id: PluginId, bytes: usize) {
        self.total_jobs = self.total_jobs.saturating_sub(1);
        self.total_bytes = self.total_bytes.saturating_sub(bytes);
        let Some((jobs, plugin_bytes)) = self.per_plugin.get_mut(&plugin_id) else {
            return;
        };
        *jobs = jobs.saturating_sub(1);
        *plugin_bytes = plugin_bytes.saturating_sub(bytes);
        if *jobs == 0 && *plugin_bytes == 0 {
            self.per_plugin.remove(&plugin_id);
        }
    }

    #[cfg(test)]
    fn jobs(&self, plugin_id: PluginId) -> usize {
        self.per_plugin
            .get(&plugin_id)
            .map(|(jobs, _)| *jobs)
            .unwrap_or(0)
    }
}

static RUN_COMMANDS: LazyLock<Mutex<Quota>> = LazyLock::new(|| Mutex::new(Quota::default()));
static WEB_REQUESTS: LazyLock<Mutex<Quota>> = LazyLock::new(|| Mutex::new(Quota::default()));
static REFUSAL_REPORTS: LazyLock<Mutex<Quota>> = LazyLock::new(|| Mutex::new(Quota::default()));

#[derive(Debug)]
struct ReservationInner {
    id: u64,
    plugin_id: PluginId,
    kind: BackgroundOperationKind,
    bytes: usize,
    load_alive: Arc<AtomicBool>,
    released: AtomicBool,
    cancelled: AtomicBool,
}

impl ReservationInner {
    fn quota(&self) -> &'static Mutex<Quota> {
        match self.kind {
            BackgroundOperationKind::RunCommand => &*RUN_COMMANDS,
            BackgroundOperationKind::WebRequest => &*WEB_REQUESTS,
            BackgroundOperationKind::RefusalReport => &*REFUSAL_REPORTS,
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn release(&self) {
        if self.released.swap(true, Ordering::AcqRel) {
            return;
        }
        self.quota()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .release(self.plugin_id, self.bytes);
    }
}

impl Drop for ReservationInner {
    fn drop(&mut self) {
        self.release();
        OPERATIONS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.id);
    }
}

#[derive(Default)]
struct RegistryEntry {
    inner: Weak<ReservationInner>,
    abort: Option<AbortHandle>,
}

static OPERATIONS: LazyLock<Mutex<HashMap<u64, RegistryEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_OPERATION_ID: AtomicU64 = AtomicU64::new(1);

/// A reservation owned by one background operation. It is created before the
/// job is queued and moved through execution into the result message, so the
/// admission cap covers queued, executing and result-pending work alike.
#[derive(Debug)]
pub struct BackgroundOperationReservation {
    inner: Arc<ReservationInner>,
}

impl Clone for BackgroundOperationReservation {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl PartialEq for BackgroundOperationReservation {
    fn eq(&self, other: &Self) -> bool {
        self.inner.id == other.inner.id
    }
}

impl Eq for BackgroundOperationReservation {}

impl std::hash::Hash for BackgroundOperationReservation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.id.hash(state);
    }
}

impl BackgroundOperationReservation {
    pub(crate) fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
            || !self.inner.load_alive.load(Ordering::Acquire)
    }

    /// Register the task that owns this operation so unload can abort it.
    pub(crate) fn register_abort_handle(&self, abort: AbortHandle) {
        if self.is_cancelled() {
            abort.abort();
            return;
        }
        let mut operations = OPERATIONS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.is_cancelled() {
            drop(operations);
            abort.abort();
            return;
        }
        if let Some(entry) = operations.get_mut(&self.inner.id) {
            entry.abort = Some(abort);
        }
    }
}

fn try_acquire(
    plugin_id: PluginId,
    kind: BackgroundOperationKind,
    bytes: usize,
    load_alive: Arc<AtomicBool>,
    max_jobs_per_plugin: usize,
    max_jobs: usize,
    max_bytes_per_plugin: usize,
    max_bytes: usize,
) -> Option<BackgroundOperationReservation> {
    if !load_alive.load(Ordering::Acquire) {
        return None;
    }
    let quota = match kind {
        BackgroundOperationKind::RunCommand => &*RUN_COMMANDS,
        BackgroundOperationKind::WebRequest => &*WEB_REQUESTS,
        BackgroundOperationKind::RefusalReport => &*REFUSAL_REPORTS,
    };
    {
        let mut quota = quota
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !quota.try_admit(
            plugin_id,
            bytes,
            max_jobs_per_plugin,
            max_jobs,
            max_bytes_per_plugin,
            max_bytes,
        ) {
            return None;
        }
    }

    let inner = Arc::new(ReservationInner {
        id: NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed),
        plugin_id,
        kind,
        bytes,
        load_alive,
        released: AtomicBool::new(false),
        cancelled: AtomicBool::new(false),
    });
    OPERATIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            inner.id,
            RegistryEntry {
                inner: Arc::downgrade(&inner),
                abort: None,
            },
        );
    Some(BackgroundOperationReservation { inner })
}

pub(crate) fn try_acquire_run_command(
    plugin_id: PluginId,
    load_alive: Arc<AtomicBool>,
    bytes: usize,
) -> Option<BackgroundOperationReservation> {
    try_acquire(
        plugin_id,
        BackgroundOperationKind::RunCommand,
        bytes,
        load_alive,
        MAX_RUN_COMMANDS_PER_PLUGIN,
        MAX_RUN_COMMANDS_IN_FLIGHT,
        MAX_RUN_COMMAND_BYTES_PER_PLUGIN,
        MAX_RUN_COMMAND_BYTES_IN_FLIGHT,
    )
}

pub(crate) fn try_acquire_web_request(
    plugin_id: PluginId,
    load_alive: Arc<AtomicBool>,
    bytes: usize,
) -> Option<BackgroundOperationReservation> {
    try_acquire(
        plugin_id,
        BackgroundOperationKind::WebRequest,
        bytes,
        load_alive,
        MAX_WEB_REQUESTS_PER_PLUGIN,
        MAX_WEB_REQUESTS_IN_FLIGHT,
        MAX_WEB_REQUEST_BYTES_PER_PLUGIN,
        MAX_WEB_REQUEST_BYTES_IN_FLIGHT,
    )
}

pub(crate) fn try_acquire_refusal_report(
    plugin_id: PluginId,
    load_alive: Arc<AtomicBool>,
) -> Option<BackgroundOperationReservation> {
    try_acquire(
        plugin_id,
        BackgroundOperationKind::RefusalReport,
        0,
        load_alive,
        MAX_REFUSAL_REPORTS_PER_PLUGIN,
        MAX_REFUSAL_REPORTS_IN_FLIGHT,
        0,
        0,
    )
}

/// Cancel and release every operation whose originating load was retired.
/// This makes unload recover capacity immediately even when a result message
/// is still queued behind a blocked plugin.
pub(crate) fn reap_retired_operations() {
    let mut retired = Vec::new();
    {
        let mut operations = OPERATIONS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        operations.retain(|_id, entry| {
            let Some(inner) = entry.inner.upgrade() else {
                return false;
            };
            if inner.load_alive.load(Ordering::Acquire) {
                return true;
            }
            inner.cancel();
            retired.push((inner, entry.abort.take()));
            false
        });
    }
    for (inner, abort) in retired {
        inner.release();
        if let Some(abort) = abort {
            abort.abort();
        }
        drop(inner);
    }
}

#[cfg(test)]
pub(crate) fn in_flight(kind: BackgroundOperationKind) -> usize {
    let quota = match kind {
        BackgroundOperationKind::RunCommand => &*RUN_COMMANDS,
        BackgroundOperationKind::WebRequest => &*WEB_REQUESTS,
        BackgroundOperationKind::RefusalReport => &*REFUSAL_REPORTS,
    };
    quota
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .total_jobs
}

#[cfg(test)]
pub(crate) fn in_flight_for(kind: BackgroundOperationKind, plugin_id: PluginId) -> usize {
    let quota = match kind {
        BackgroundOperationKind::RunCommand => &*RUN_COMMANDS,
        BackgroundOperationKind::WebRequest => &*WEB_REQUESTS,
        BackgroundOperationKind::RefusalReport => &*REFUSAL_REPORTS,
    };
    quota
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .jobs(plugin_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(true))
    }

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn a_reservation_counts_until_the_result_message_is_dropped() {
        let _lock = lock();
        let plugin_id = 91_001;
        let reservation = try_acquire_run_command(plugin_id, live(), 128).expect("admission");
        assert_eq!(
            in_flight_for(BackgroundOperationKind::RunCommand, plugin_id),
            1
        );
        let result_reservation = reservation.clone();
        drop(reservation);
        assert_eq!(
            in_flight_for(BackgroundOperationKind::RunCommand, plugin_id),
            1,
            "the result message still owns the reservation"
        );
        drop(result_reservation);
        assert_eq!(
            in_flight_for(BackgroundOperationKind::RunCommand, plugin_id),
            0
        );
    }

    #[test]
    fn a_retired_load_releases_queued_capacity_immediately() {
        let _lock = lock();
        let plugin_id = 91_002;
        let alive = live();
        let reservation = try_acquire_web_request(plugin_id, alive.clone(), 64).expect("admission");
        assert_eq!(
            in_flight_for(BackgroundOperationKind::WebRequest, plugin_id),
            1
        );
        alive.store(false, Ordering::Release);
        reap_retired_operations();
        assert_eq!(
            in_flight_for(BackgroundOperationKind::WebRequest, plugin_id),
            0,
            "a retired load must not retain admission capacity"
        );
        assert!(reservation.is_cancelled());
        drop(reservation);
        assert_eq!(
            in_flight_for(BackgroundOperationKind::WebRequest, plugin_id),
            0,
            "dropping the already-reaped reservation must not double release"
        );
    }

    #[test]
    fn refusal_reports_are_bounded_and_dropped_past_the_cap() {
        let _lock = lock();
        let plugin_id = 91_003;
        let holds: Vec<_> = (0..MAX_REFUSAL_REPORTS_PER_PLUGIN)
            .map(|_| {
                try_acquire_refusal_report(plugin_id, live())
                    .expect("per-plugin refusal report slot")
            })
            .collect();
        assert_eq!(
            in_flight_for(BackgroundOperationKind::RefusalReport, plugin_id),
            MAX_REFUSAL_REPORTS_PER_PLUGIN
        );
        assert!(
            try_acquire_refusal_report(plugin_id, live()).is_none(),
            "the refusal backlog must stay bounded"
        );
        drop(holds);
        assert_eq!(
            in_flight_for(BackgroundOperationKind::RefusalReport, plugin_id),
            0
        );
    }
}

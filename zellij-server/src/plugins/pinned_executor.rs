use crate::diagnostics;
use crate::plugins::plugin_map::PluginMap;
use crate::plugins::wasm_bridge::PluginCache;
use crate::ClientId;
use crate::ThreadSenders;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread;
use wasmi::Engine;

const DEFAULT_JOB_BASE_BYTES: usize = 256;

#[derive(Clone, Copy, Debug)]
struct QueueLimits {
    max_jobs_per_thread: usize,
    max_bytes_per_thread: usize,
    max_jobs_per_owner: usize,
    max_bytes_per_owner: usize,
    reserved_jobs_per_thread: usize,
    reserved_bytes_per_thread: usize,
    reserved_retirement_jobs_per_thread: usize,
    reserved_retirement_bytes_per_thread: usize,
    job_base_bytes: usize,
}

impl Default for QueueLimits {
    fn default() -> Self {
        Self {
            max_jobs_per_thread: 2048,
            max_bytes_per_thread: 32 * 1024 * 1024,
            max_jobs_per_owner: 256,
            max_bytes_per_owner: 4 * 1024 * 1024,
            reserved_jobs_per_thread: 64,
            reserved_bytes_per_thread: 4 * 1024 * 1024,
            reserved_retirement_jobs_per_thread: 64,
            reserved_retirement_bytes_per_thread: 4 * 1024 * 1024,
            job_base_bytes: DEFAULT_JOB_BASE_BYTES,
        }
    }
}

/// Admission class for work submitted to a pinned plugin thread.
///
/// `Ordered` work is delivered in submission order and is dropped when the
/// owner's budget is exhausted. `Replaceable` work carries a key: while an
/// earlier job with the same key is still pending and no other job has been
/// submitted for that owner afterwards, the newer payload replaces it.
/// `Lifecycle` work (loads, cleanups and completion-bearing control work) can
/// use the capacity reserved outside the ordinary budgets. `Retirement` work
/// has its own additional headroom so event saturation cannot prevent a plugin
/// from being retired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobClass {
    Ordered,
    Replaceable(u64),
    Lifecycle,
    Retirement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobRejection {
    BudgetExceeded,
    OwnerNotRegistered,
    QueueClosed,
}

impl std::fmt::Display for JobRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobRejection::BudgetExceeded => write!(f, "plugin job queue budget exceeded"),
            JobRejection::OwnerNotRegistered => write!(f, "plugin has no executor assignment"),
            JobRejection::QueueClosed => write!(f, "plugin executor thread is shutting down"),
        }
    }
}

impl std::error::Error for JobRejection {}

/// A dynamic thread pool that pins jobs to specific threads based on plugin_id
/// Starts with 1 thread and expands when threads are busy, shrinks when plugins unload
pub struct PinnedExecutor {
    // Live workers keyed by monotonically increasing, non-reused thread ids
    execution_threads: Arc<Mutex<HashMap<usize, Arc<ExecutionThread>>>>,

    // Maps plugin_id -> thread_index (permanent assignment)
    plugin_assignments: Arc<Mutex<HashMap<u32, usize>>>,

    // Maps thread_index -> set of plugin_ids assigned to it
    thread_plugins: Arc<Mutex<HashMap<usize, HashSet<u32>>>>,

    // Next thread index to use when spawning (monotonically increasing)
    next_thread_idx: AtomicUsize,

    // Maximum threads allowed
    max_threads: usize,

    // state to send to plugins (to be kept on execution threads)
    senders: ThreadSenders,
    plugin_map: Arc<Mutex<PluginMap>>,
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
    plugin_cache: PluginCache,
    engine: Engine,
    limits: QueueLimits,
}

struct ExecutionThread {
    queue: Arc<ThreadQueue>,
}

/// A unit of work executed on a plugin's pinned thread.
pub type PluginWork = Box<
    dyn FnOnce(ThreadSenders, Arc<Mutex<PluginMap>>, Arc<Mutex<Vec<ClientId>>>, PluginCache, Engine)
        + Send
        + 'static,
>;

enum Job {
    Work {
        owner: u32,
        accounted_bytes: usize,
        work: PluginWork,
    },
    Coalesced {
        slot: Arc<Mutex<CoalesceSlot>>,
    },
    Reserved {
        owner: u32,
        accounted_bytes: usize,
        slot: Arc<ReservedJob>,
    },
}

struct CoalesceSlot {
    owner: u32,
    sequence: u64,
    work: Option<PluginWork>,
    accounted_bytes: usize,
}

#[derive(Default)]
struct ReservedJobState {
    work: Option<PluginWork>,
    cancelled: bool,
}

#[derive(Default)]
struct ReservedJob {
    state: Mutex<ReservedJobState>,
    wake: Condvar,
}

/// A lifecycle job whose capacity is reserved before the caller publishes the
/// state the job will operate on. Dropping it without committing cancels the
/// placeholder; the worker skips it and the reserved capacity is released.
pub struct JobReservation {
    slot: Arc<ReservedJob>,
    committed: bool,
}

impl JobReservation {
    pub fn commit(mut self, work: PluginWork) {
        {
            let mut state = self.slot.state.lock().unwrap();
            if !state.cancelled {
                state.work = Some(work);
            }
        }
        self.committed = true;
        self.slot.wake.notify_one();
    }
}

impl Drop for JobReservation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        {
            let mut state = self.slot.state.lock().unwrap();
            state.cancelled = true;
        }
        self.slot.wake.notify_one();
    }
}

/// A load reservation that also owns rollback of the executor assignment it
/// created. It must be committed after the caller publishes its pending-load
/// state.
pub struct PluginLoadReservation {
    reservation: Option<JobReservation>,
    executor: Arc<PinnedExecutor>,
    plugin_id: u32,
    newly_registered: bool,
}

impl PluginLoadReservation {
    pub fn commit(mut self, work: PluginWork) {
        if let Some(reservation) = self.reservation.take() {
            reservation.commit(work);
        }
    }
}

impl Drop for PluginLoadReservation {
    fn drop(&mut self) {
        if self.reservation.is_some() && self.newly_registered {
            self.executor.unregister_plugin(self.plugin_id);
        }
    }
}

#[derive(Default)]
struct OwnerCounts {
    jobs: usize,
    bytes: usize,
}

#[derive(Default)]
struct QueueCounts {
    jobs: usize,
    bytes: usize,
    owners: HashMap<u32, OwnerCounts>,
}

impl QueueCounts {
    fn owner(&self, owner: u32) -> (usize, usize) {
        self.owners
            .get(&owner)
            .map(|counts| (counts.jobs, counts.bytes))
            .unwrap_or((0, 0))
    }

    fn admit(&mut self, owner: u32, jobs: usize, bytes: usize) {
        self.jobs = self.jobs.saturating_add(jobs);
        self.bytes = self.bytes.saturating_add(bytes);
        let owner_counts = self.owners.entry(owner).or_default();
        owner_counts.jobs = owner_counts.jobs.saturating_add(jobs);
        owner_counts.bytes = owner_counts.bytes.saturating_add(bytes);
    }

    fn adjust_bytes(&mut self, owner: u32, delta: isize) {
        if delta >= 0 {
            self.bytes = self.bytes.saturating_add(delta as usize);
            let owner_counts = self.owners.entry(owner).or_default();
            owner_counts.bytes = owner_counts.bytes.saturating_add(delta as usize);
        } else {
            let released = delta.unsigned_abs();
            self.bytes = self.bytes.saturating_sub(released);
            let owner_counts = self.owners.entry(owner).or_default();
            owner_counts.bytes = owner_counts.bytes.saturating_sub(released);
        }
    }

    fn release(&mut self, owner: u32, jobs: usize, bytes: usize) {
        self.jobs = self.jobs.saturating_sub(jobs);
        self.bytes = self.bytes.saturating_sub(bytes);
        if let Some(owner_counts) = self.owners.get_mut(&owner) {
            owner_counts.jobs = owner_counts.jobs.saturating_sub(jobs);
            owner_counts.bytes = owner_counts.bytes.saturating_sub(bytes);
            if owner_counts.jobs == 0 && owner_counts.bytes == 0 {
                self.owners.remove(&owner);
            }
        }
    }
}

struct ThreadQueueState {
    jobs: VecDeque<Job>,
    shutdown: bool,
    counts: QueueCounts,
    coalesced: HashMap<(u32, u64), Weak<Mutex<CoalesceSlot>>>,
    next_sequence: u64,
    last_sequence: HashMap<u32, u64>,
}

struct ThreadQueue {
    state: Mutex<ThreadQueueState>,
    wake: Condvar,
    limits: QueueLimits,
}

impl ThreadQueue {
    fn new(limits: QueueLimits) -> Self {
        Self {
            state: Mutex::new(ThreadQueueState {
                jobs: VecDeque::new(),
                shutdown: false,
                counts: QueueCounts::default(),
                coalesced: HashMap::new(),
                next_sequence: 0,
                last_sequence: HashMap::new(),
            }),
            wake: Condvar::new(),
            limits,
        }
    }

    fn can_admit(
        &self,
        state: &ThreadQueueState,
        owner: u32,
        class: JobClass,
        accounted_bytes: usize,
        replaced_bytes: Option<usize>,
    ) -> bool {
        let (owner_jobs, owner_bytes) = state.counts.owner(owner);
        let replacement = replaced_bytes.unwrap_or(0);
        let projected_jobs = state
            .counts
            .jobs
            .saturating_add(if replaced_bytes.is_some() { 0 } else { 1 });
        let projected_bytes = state
            .counts
            .bytes
            .saturating_sub(replacement)
            .saturating_add(accounted_bytes);
        let projected_owner_jobs =
            owner_jobs.saturating_add(if replaced_bytes.is_some() { 0 } else { 1 });
        let projected_owner_bytes = owner_bytes
            .saturating_sub(replacement)
            .saturating_add(accounted_bytes);

        if class == JobClass::Retirement {
            projected_jobs
                <= self
                    .limits
                    .max_jobs_per_thread
                    .saturating_add(self.limits.reserved_retirement_jobs_per_thread)
                && projected_bytes
                    <= self
                        .limits
                        .max_bytes_per_thread
                        .saturating_add(self.limits.reserved_retirement_bytes_per_thread)
        } else if class == JobClass::Lifecycle {
            projected_jobs <= self.limits.max_jobs_per_thread
                && projected_bytes <= self.limits.max_bytes_per_thread
        } else {
            projected_jobs.saturating_add(self.limits.reserved_jobs_per_thread)
                <= self.limits.max_jobs_per_thread
                && projected_bytes.saturating_add(self.limits.reserved_bytes_per_thread)
                    <= self.limits.max_bytes_per_thread
                && projected_owner_jobs <= self.limits.max_jobs_per_owner
                && projected_owner_bytes <= self.limits.max_bytes_per_owner
        }
    }

    fn enqueue(
        &self,
        owner: u32,
        class: JobClass,
        payload_bytes: usize,
        work: PluginWork,
    ) -> Result<(), JobRejection> {
        let accounted_bytes = payload_bytes
            .saturating_add(self.limits.job_base_bytes)
            .max(1);
        let mut state = self.state.lock().unwrap();
        if state.shutdown {
            return Err(JobRejection::QueueClosed);
        }

        let sequence = state.next_sequence.wrapping_add(1);
        if let JobClass::Replaceable(key) = class {
            let existing_slot = state
                .coalesced
                .get(&(owner, key))
                .and_then(|slot| slot.upgrade());
            if let Some(slot) = existing_slot {
                let mut slot = slot.lock().unwrap();
                let still_latest = state.last_sequence.get(&owner).copied() == Some(slot.sequence);
                if still_latest && slot.work.is_some() {
                    if !self.can_admit(
                        &state,
                        owner,
                        class,
                        accounted_bytes,
                        Some(slot.accounted_bytes),
                    ) {
                        return Err(JobRejection::BudgetExceeded);
                    }
                    state.counts.adjust_bytes(
                        owner,
                        accounted_bytes as isize - slot.accounted_bytes as isize,
                    );
                    let replaced_work = std::mem::replace(&mut slot.work, Some(work));
                    slot.accounted_bytes = accounted_bytes;
                    slot.sequence = sequence;
                    state.next_sequence = sequence;
                    state.last_sequence.insert(owner, sequence);
                    drop(slot);
                    drop(state);
                    drop(replaced_work);
                    return Ok(());
                }
            }

            if !self.can_admit(&state, owner, class, accounted_bytes, None) {
                return Err(JobRejection::BudgetExceeded);
            }
            let slot = Arc::new(Mutex::new(CoalesceSlot {
                owner,
                sequence,
                work: Some(work),
                accounted_bytes,
            }));
            state.coalesced.insert((owner, key), Arc::downgrade(&slot));
            state.counts.admit(owner, 1, accounted_bytes);
            state.next_sequence = sequence;
            state.last_sequence.insert(owner, sequence);
            state.jobs.push_back(Job::Coalesced { slot });
        } else {
            if !self.can_admit(&state, owner, class, accounted_bytes, None) {
                return Err(JobRejection::BudgetExceeded);
            }
            state.counts.admit(owner, 1, accounted_bytes);
            state.next_sequence = sequence;
            state.last_sequence.insert(owner, sequence);
            state.jobs.push_back(Job::Work {
                owner,
                accounted_bytes,
                work,
            });
        }
        diagnostics::record_plugin_jobs_in_flight(state.counts.jobs);
        self.wake.notify_one();
        Ok(())
    }

    fn reserve(
        &self,
        owner: u32,
        class: JobClass,
        payload_bytes: usize,
    ) -> Result<JobReservation, JobRejection> {
        let accounted_bytes = payload_bytes
            .saturating_add(self.limits.job_base_bytes)
            .max(1);
        let mut state = self.state.lock().unwrap();
        if state.shutdown {
            return Err(JobRejection::QueueClosed);
        }
        if !self.can_admit(&state, owner, class, accounted_bytes, None) {
            return Err(JobRejection::BudgetExceeded);
        }
        let sequence = state.next_sequence.wrapping_add(1);
        let slot = Arc::new(ReservedJob::default());
        state.counts.admit(owner, 1, accounted_bytes);
        state.next_sequence = sequence;
        state.last_sequence.insert(owner, sequence);
        state.jobs.push_back(Job::Reserved {
            owner,
            accounted_bytes,
            slot: slot.clone(),
        });
        diagnostics::record_plugin_jobs_in_flight(state.counts.jobs);
        self.wake.notify_one();
        Ok(JobReservation {
            slot,
            committed: false,
        })
    }

    fn complete(&self, owner: u32, accounted_bytes: usize) {
        let mut state = self.state.lock().unwrap();
        state.counts.release(owner, 1, accounted_bytes);
        if state.counts.owner(owner).0 == 0 {
            state.last_sequence.remove(&owner);
            state
                .coalesced
                .retain(|(entry_owner, _), _| *entry_owner != owner);
        }
        diagnostics::record_plugin_jobs_in_flight(state.counts.jobs);
    }

    fn request_shutdown(&self) {
        let mut state = self.state.lock().unwrap();
        state.shutdown = true;
        self.wake.notify_all();
    }

    fn queued_jobs(&self) -> usize {
        self.state.lock().unwrap().counts.jobs
    }

    #[cfg(test)]
    fn queued_bytes(&self) -> usize {
        self.state.lock().unwrap().counts.bytes
    }

    #[cfg(test)]
    fn owner_queued_jobs(&self, owner: u32) -> usize {
        self.state.lock().unwrap().counts.owner(owner).0
    }
}

impl PinnedExecutor {
    /// Creates a new pinned executor with the specified maximum number of threads
    /// Starts with exactly 1 thread
    pub fn new(
        max_threads: usize,
        senders: &ThreadSenders,
        plugin_map: &Arc<Mutex<PluginMap>>,
        connected_clients: &Arc<Mutex<Vec<ClientId>>>,
        plugin_cache: &PluginCache,
        engine: &Engine,
    ) -> Self {
        Self::new_with_limits(
            max_threads,
            senders,
            plugin_map,
            connected_clients,
            plugin_cache,
            engine,
            QueueLimits::default(),
        )
    }

    #[cfg(test)]
    pub(crate) fn new_with_tiny_limits_for_test(
        max_threads: usize,
        senders: &ThreadSenders,
        plugin_map: &Arc<Mutex<PluginMap>>,
        connected_clients: &Arc<Mutex<Vec<ClientId>>>,
        plugin_cache: &PluginCache,
        engine: &Engine,
    ) -> Self {
        Self::new_with_limits(
            max_threads,
            senders,
            plugin_map,
            connected_clients,
            plugin_cache,
            engine,
            QueueLimits {
                max_jobs_per_thread: 1,
                max_bytes_per_thread: 64 * 1024,
                max_jobs_per_owner: 1,
                max_bytes_per_owner: 64 * 1024,
                reserved_jobs_per_thread: 0,
                reserved_bytes_per_thread: 0,
                reserved_retirement_jobs_per_thread: 0,
                reserved_retirement_bytes_per_thread: 0,
                job_base_bytes: 32,
            },
        )
    }

    fn new_with_limits(
        max_threads: usize,
        senders: &ThreadSenders,
        plugin_map: &Arc<Mutex<PluginMap>>,
        connected_clients: &Arc<Mutex<Vec<ClientId>>>,
        plugin_cache: &PluginCache,
        engine: &Engine,
        limits: QueueLimits,
    ) -> Self {
        let max_threads = max_threads.max(1); // At least 1

        let mut execution_threads = HashMap::new();
        if let Some(thread_0) = Self::spawn_thread(
            0,
            senders.clone(),
            plugin_map.clone(),
            connected_clients.clone(),
            plugin_cache.clone(),
            engine.clone(),
            limits,
        ) {
            execution_threads.insert(0, Arc::new(thread_0));
        }

        PinnedExecutor {
            execution_threads: Arc::new(Mutex::new(execution_threads)),
            plugin_assignments: Arc::new(Mutex::new(HashMap::new())),
            thread_plugins: Arc::new(Mutex::new(HashMap::new())),
            next_thread_idx: AtomicUsize::new(1), // Next will be index 1
            max_threads,
            senders: senders.clone(),
            plugin_map: plugin_map.clone(),
            connected_clients: connected_clients.clone(),
            plugin_cache: plugin_cache.clone(),
            engine: engine.clone(),
            limits,
        }
    }

    fn spawn_thread(
        thread_idx: usize,
        senders: ThreadSenders,
        plugin_map: Arc<Mutex<PluginMap>>,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
        plugin_cache: PluginCache,
        engine: Engine,
        limits: QueueLimits,
    ) -> Option<ExecutionThread> {
        let queue = Arc::new(ThreadQueue::new(limits));
        let worker_queue = queue.clone();

        let thread_handle = thread::Builder::new()
            .name(format!("plugin-exec-{}", thread_idx))
            .spawn(move || {
                while let Some(job) = Self::next_job(&worker_queue) {
                    match job {
                        Job::Work {
                            owner,
                            accounted_bytes,
                            work,
                        } => {
                            work(
                                senders.clone(),
                                plugin_map.clone(),
                                connected_clients.clone(),
                                plugin_cache.clone(),
                                engine.clone(),
                            );
                            worker_queue.complete(owner, accounted_bytes);
                        },
                        Job::Coalesced { slot } => {
                            let taken = {
                                let mut slot = slot.lock().unwrap();
                                let owner = slot.owner;
                                let accounted_bytes = slot.accounted_bytes;
                                slot.accounted_bytes = 0;
                                slot.work.take().map(|work| (owner, accounted_bytes, work))
                            };
                            if let Some((owner, accounted_bytes, work)) = taken {
                                work(
                                    senders.clone(),
                                    plugin_map.clone(),
                                    connected_clients.clone(),
                                    plugin_cache.clone(),
                                    engine.clone(),
                                );
                                worker_queue.complete(owner, accounted_bytes);
                            }
                        },
                        Job::Reserved {
                            owner,
                            accounted_bytes,
                            slot,
                        } => {
                            let work = {
                                let mut state = slot.state.lock().unwrap();
                                while state.work.is_none() && !state.cancelled {
                                    state = slot.wake.wait(state).unwrap();
                                }
                                state.work.take()
                            };
                            if let Some(work) = work {
                                work(
                                    senders.clone(),
                                    plugin_map.clone(),
                                    connected_clients.clone(),
                                    plugin_cache.clone(),
                                    engine.clone(),
                                );
                            }
                            worker_queue.complete(owner, accounted_bytes);
                        },
                    }
                }
            });
        match thread_handle {
            Ok(_) => Some(ExecutionThread { queue }),
            Err(e) => {
                log::error!("Failed to spawn plugin execution thread: {}", e);
                None
            },
        }
    }

    fn next_job(queue: &Arc<ThreadQueue>) -> Option<Job> {
        let mut state = queue.state.lock().unwrap();
        loop {
            if let Some(job) = state.jobs.pop_front() {
                return Some(job);
            }
            if state.shutdown {
                return None;
            }
            state = queue.wake.wait(state).unwrap();
        }
    }

    /// Register a plugin and assign it to a thread
    /// Called from wasm_bridge when loading a plugin
    #[cfg(test)]
    pub fn register_plugin(&self, plugin_id: u32) -> usize {
        self.register_plugin_inner(plugin_id).0
    }

    fn register_plugin_inner(&self, plugin_id: u32) -> (usize, bool) {
        let mut assignments = self.plugin_assignments.lock().unwrap();

        if let Some(&thread_idx) = assignments.get(&plugin_id) {
            return (thread_idx, false);
        }

        let mut thread_plugins = self.thread_plugins.lock().unwrap();
        let threads = self.execution_threads.lock().unwrap();

        // Find a non-busy thread with assigned plugins (prefer reusing threads)
        let mut best_thread: Option<(usize, usize)> = None; // (index, load)

        for (idx, thread) in threads.iter() {
            let is_busy = thread.queue.queued_jobs() > 0;
            if !is_busy {
                let load = thread_plugins.get(idx).map(|s| s.len()).unwrap_or(0);
                let is_better = best_thread
                    .map(|(best_idx, best_load)| {
                        load < best_load || (load == best_load && *idx < best_idx)
                    })
                    .unwrap_or(true);
                if is_better {
                    best_thread = Some((*idx, load));
                }
            }
        }

        let thread_idx = if let Some((idx, _)) = best_thread {
            // Found a non-busy thread
            Some(idx)
        } else {
            // All threads are busy - need to expand
            if threads.len() < self.max_threads {
                // Spawn a new thread
                let new_idx = self.next_thread_idx.fetch_add(1, Ordering::SeqCst);
                drop(threads); // Release lock before spawning
                if self.add_thread(new_idx) {
                    Some(new_idx)
                } else {
                    let threads = self.execution_threads.lock().unwrap();
                    Self::least_loaded_thread(&threads, &thread_plugins)
                }
            } else {
                // At max capacity, assign to least-loaded thread
                Self::least_loaded_thread(&threads, &thread_plugins)
            }
        };
        let Some(thread_idx) = thread_idx else {
            log::error!("Failed to find or spawn a thread to run the plugin!");
            return (0, false);
        };

        // Update mappings
        assignments.insert(plugin_id, thread_idx);
        thread_plugins
            .entry(thread_idx)
            .or_insert_with(HashSet::new)
            .insert(plugin_id);

        (thread_idx, true)
    }

    fn least_loaded_thread(
        threads: &HashMap<usize, Arc<ExecutionThread>>,
        thread_plugins: &HashMap<usize, HashSet<u32>>,
    ) -> Option<usize> {
        threads
            .keys()
            .min_by_key(|idx| {
                (
                    thread_plugins.get(*idx).map(|s| s.len()).unwrap_or(0),
                    **idx,
                )
            })
            .copied()
    }

    fn add_thread(&self, thread_idx: usize) -> bool {
        let mut threads = self.execution_threads.lock().unwrap();
        let Some(new_thread) = Self::spawn_thread(
            thread_idx,
            self.senders.clone(),
            self.plugin_map.clone(),
            self.connected_clients.clone(),
            self.plugin_cache.clone(),
            self.engine.clone(),
            self.limits,
        ) else {
            return false;
        };

        threads.insert(thread_idx, Arc::new(new_thread));
        true
    }

    /// Execute job pinned to plugin's assigned thread
    #[cfg(test)]
    pub fn execute_for_plugin<F>(&self, plugin_id: u32, f: F)
    where
        F: FnOnce(
                ThreadSenders,
                Arc<Mutex<PluginMap>>,
                Arc<Mutex<Vec<ClientId>>>,
                PluginCache,
                Engine,
            ) + Send
            + 'static,
    {
        self.execute_for_plugin_with(plugin_id, JobClass::Ordered, 0, f);
    }

    /// Execute work with an explicit admission class, logging a bounded
    /// rejection instead of propagating it. Callers that must turn rejection
    /// into a client-visible failure use [`Self::try_execute_for_plugin`].
    pub fn execute_for_plugin_with<F>(
        &self,
        plugin_id: u32,
        class: JobClass,
        payload_bytes: usize,
        f: F,
    ) where
        F: FnOnce(
                ThreadSenders,
                Arc<Mutex<PluginMap>>,
                Arc<Mutex<Vec<ClientId>>>,
                PluginCache,
                Engine,
            ) + Send
            + 'static,
    {
        if let Err(e) = self.try_execute_for_plugin(plugin_id, class, payload_bytes, f) {
            log::warn!("Plugin executor rejected work for {}: {}", plugin_id, e);
        }
    }

    /// Execute work with an explicit admission class and an estimate of the
    /// captured payload's heap bytes. Admission never blocks and is resolved
    /// before any worker starts the job.
    pub fn try_execute_for_plugin<F>(
        &self,
        plugin_id: u32,
        class: JobClass,
        payload_bytes: usize,
        f: F,
    ) -> Result<(), JobRejection>
    where
        F: FnOnce(
                ThreadSenders,
                Arc<Mutex<PluginMap>>,
                Arc<Mutex<Vec<ClientId>>>,
                PluginCache,
                Engine,
            ) + Send
            + 'static,
    {
        let thread = {
            let assignments = self.plugin_assignments.lock().unwrap();
            let Some(thread_idx) = assignments.get(&plugin_id).copied() else {
                log::error!("Failed to find thread for plugin with id: {}", plugin_id);
                return Err(JobRejection::OwnerNotRegistered);
            };
            drop(assignments);

            let threads = self.execution_threads.lock().unwrap();
            let Some(thread) = threads.get(&thread_idx).cloned() else {
                log::error!("Failed to find thread for plugin with id: {}", plugin_id);
                return Err(JobRejection::QueueClosed);
            };
            thread
        };

        thread
            .queue
            .enqueue(plugin_id, class, payload_bytes, Box::new(f))
    }

    /// Reserve lifecycle capacity and a queue position without supplying the
    /// work yet. Callers use this before publishing the state the work will
    /// consume, then commit or drop the reservation.
    pub fn reserve_job(
        &self,
        plugin_id: u32,
        class: JobClass,
        payload_bytes: usize,
    ) -> Result<JobReservation, JobRejection> {
        if !matches!(class, JobClass::Lifecycle | JobClass::Retirement) {
            return Err(JobRejection::BudgetExceeded);
        }
        let thread = {
            let assignments = self.plugin_assignments.lock().unwrap();
            let Some(thread_idx) = assignments.get(&plugin_id).copied() else {
                return Err(JobRejection::OwnerNotRegistered);
            };
            drop(assignments);

            let threads = self.execution_threads.lock().unwrap();
            let Some(thread) = threads.get(&thread_idx).cloned() else {
                return Err(JobRejection::QueueClosed);
            };
            thread
        };
        thread.queue.reserve(plugin_id, class, payload_bytes)
    }

    /// Register an owner and reserve its load before the caller publishes any
    /// pending-load state. Dropping an uncommitted reservation also removes an
    /// assignment created by this reservation.
    pub fn reserve_plugin_load(
        self: &Arc<Self>,
        plugin_id: u32,
        payload_bytes: usize,
    ) -> Result<PluginLoadReservation, JobRejection> {
        let (_, newly_registered) = self.register_plugin_inner(plugin_id);
        match self.reserve_job(plugin_id, JobClass::Lifecycle, payload_bytes) {
            Ok(reservation) => Ok(PluginLoadReservation {
                reservation: Some(reservation),
                executor: self.clone(),
                plugin_id,
                newly_registered,
            }),
            Err(e) => {
                if newly_registered {
                    self.unregister_plugin(plugin_id);
                }
                Err(e)
            },
        }
    }

    /// Load a plugin: register it and execute the load work on its assigned thread
    /// This combines registration + execution for plugin loading
    #[cfg(test)]
    pub fn execute_plugin_load<F>(self: &Arc<Self>, plugin_id: u32, f: F)
    where
        F: FnOnce(
                ThreadSenders,
                Arc<Mutex<PluginMap>>,
                Arc<Mutex<Vec<ClientId>>>,
                PluginCache,
                Engine,
            ) + Send
            + 'static,
    {
        let _ = self.execute_plugin_load_with_payload(plugin_id, 0, f);
    }

    /// Execute plugin load work while accounting for the bytes it captures
    /// (typically the module image).
    pub fn execute_plugin_load_with_payload<F>(
        self: &Arc<Self>,
        plugin_id: u32,
        payload_bytes: usize,
        f: F,
    ) -> Result<(), JobRejection>
    where
        F: FnOnce(
                ThreadSenders,
                Arc<Mutex<PluginMap>>,
                Arc<Mutex<Vec<ClientId>>>,
                PluginCache,
                Engine,
            ) + Send
            + 'static,
    {
        let reservation = self.reserve_plugin_load(plugin_id, payload_bytes)?;
        reservation.commit(Box::new(f));
        Ok(())
    }

    /// Unload a plugin: execute cleanup work, then unregister and potentially shrink pool
    /// This combines cleanup execution + unregistration for plugin unloading
    /// Requires Arc<Self> so we can clone it into the closure for unregistration
    pub fn execute_plugin_unload(
        self: &Arc<Self>,
        plugin_id: u32,
        f: impl FnOnce(
                ThreadSenders,
                Arc<Mutex<PluginMap>>,
                Arc<Mutex<Vec<ClientId>>>,
                PluginCache,
                Engine,
            ) + Send
            + 'static,
    ) -> Result<(), JobRejection> {
        self.execute_plugin_unload_many(plugin_id, vec![Box::new(f)])
    }

    /// Unload a plugin family: run every cleanup job on the plugin's pinned
    /// thread, then retire its executor assignment exactly once.
    ///
    /// A plugin can own several client instances that all share one executor
    /// assignment. Submitting a separate unload per instance lets the first
    /// job's unregistration drop later submissions before their cleanup runs;
    /// batching them into a single job keeps every cleanup ahead of the one
    /// retirement.
    pub fn execute_plugin_unload_many(
        self: &Arc<Self>,
        plugin_id: u32,
        cleanups: Vec<PluginWork>,
    ) -> Result<(), JobRejection> {
        if cleanups.is_empty() && !self.is_registered(plugin_id) {
            // nothing to run and nothing to retire
            return Ok(());
        }
        let executor = self.clone();
        match self.try_execute_for_plugin(
            plugin_id,
            JobClass::Retirement,
            0,
            move |senders, plugin_map, connected_clients, plugin_cache, engine| {
                // Execute the cleanup work
                for cleanup in cleanups {
                    cleanup(
                        senders.clone(),
                        plugin_map.clone(),
                        connected_clients.clone(),
                        plugin_cache.clone(),
                        engine.clone(),
                    );
                }

                // Unregister plugin and potentially shrink the pool
                executor.unregister_plugin(plugin_id);
            },
        ) {
            Ok(()) => Ok(()),
            Err(e) => {
                // Retirement is containment-critical. If even its dedicated
                // headroom is exhausted, settle the registry synchronously
                // rather than leaving a retired plugin assigned forever.
                self.unregister_plugin(plugin_id);
                Err(e)
            },
        }
    }

    pub fn is_registered(&self, plugin_id: u32) -> bool {
        self.plugin_assignments
            .lock()
            .unwrap()
            .contains_key(&plugin_id)
    }

    /// Unregister a plugin and potentially shrink the pool
    /// Called from wasm_bridge after plugin cleanup is complete
    pub fn unregister_plugin(&self, plugin_id: u32) {
        let mut assignments = self.plugin_assignments.lock().unwrap();
        let mut thread_plugins = self.thread_plugins.lock().unwrap();

        if let Some(thread_idx) = assignments.remove(&plugin_id) {
            if let Some(plugins) = thread_plugins.get_mut(&thread_idx) {
                plugins.remove(&plugin_id);
            }
        }

        drop(assignments);
        drop(thread_plugins);

        // Try to shrink the pool
        self.try_shrink_pool();
    }

    fn try_shrink_pool(&self) {
        let mut thread_plugins = self.thread_plugins.lock().unwrap();
        let mut threads = self.execution_threads.lock().unwrap();

        // Find threads with no assigned plugins (except thread 0, always keep it)
        let threads_to_remove: Vec<usize> = threads
            .keys()
            .copied()
            .filter(|idx| {
                *idx != 0
                    && !thread_plugins
                        .get(idx)
                        .map(|s| !s.is_empty())
                        .unwrap_or(false)
            })
            .collect();

        // Shutdown and remove idle threads
        for idx in threads_to_remove {
            if let Some(thread) = threads.remove(&idx) {
                thread.queue.request_shutdown();
                thread_plugins.remove(&idx);
            }
        }
    }

    #[cfg(test)]
    pub fn thread_count(&self) -> usize {
        self.execution_threads.lock().unwrap().len()
    }

    #[cfg(test)]
    pub fn queued_jobs(&self) -> usize {
        self.execution_threads
            .lock()
            .unwrap()
            .values()
            .map(|thread| thread.queue.queued_jobs())
            .sum()
    }

    #[cfg(test)]
    pub fn queued_bytes(&self) -> usize {
        self.execution_threads
            .lock()
            .unwrap()
            .values()
            .map(|thread| thread.queue.queued_bytes())
            .sum()
    }

    #[cfg(test)]
    pub fn owner_queued_jobs(&self, owner: u32) -> usize {
        self.execution_threads
            .lock()
            .unwrap()
            .values()
            .map(|thread| thread.queue.owner_queued_jobs(owner))
            .sum()
    }
}

impl Drop for PinnedExecutor {
    fn drop(&mut self) {
        let threads = self.execution_threads.lock().unwrap();

        // Ask every worker to drain its admitted work and then exit.
        for thread in threads.values() {
            thread.queue.request_shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{channel, Sender};
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;
    use std::time::Duration;

    // Test fixtures
    fn create_test_dependencies() -> (
        ThreadSenders,
        Arc<Mutex<PluginMap>>,
        Arc<Mutex<Vec<ClientId>>>,
        PluginCache,
        Engine,
    ) {
        use std::path::PathBuf;
        use wasmi::Module;
        use zellij_utils::channels::{self, SenderWithContext};

        let (send_to_pty, _receive_pty) = channels::bounded(1);
        let (send_to_screen, _receive_screen) = channels::bounded(1);
        let (send_to_plugin, _receive_plugin) = channels::bounded(1);
        let (send_to_server, _receive_server) = channels::bounded(1);
        let (send_to_pty_writer, _receive_pty_writer) = channels::bounded(1);
        let (send_to_background_jobs, _receive_background_jobs) = channels::bounded(1);

        let to_pty = SenderWithContext::new(send_to_pty);
        let to_screen = SenderWithContext::new(send_to_screen);
        let to_plugin = SenderWithContext::new(send_to_plugin);
        let to_server = SenderWithContext::new(send_to_server);
        let to_pty_writer = SenderWithContext::new(send_to_pty_writer);
        let to_background_jobs = SenderWithContext::new(send_to_background_jobs);

        let senders = ThreadSenders {
            to_screen_priority: None,
            to_pty: Some(to_pty),
            to_screen: Some(to_screen),
            to_plugin: Some(to_plugin),
            to_server: Some(to_server),
            to_pty_writer: Some(to_pty_writer),
            to_background_jobs: Some(to_background_jobs),
            should_silently_fail: false,
        };

        let plugin_map = Arc::new(Mutex::new(PluginMap::default()));
        let connected_clients = Arc::new(Mutex::new(vec![]));

        let plugin_cache = Arc::new(Mutex::new(std::collections::HashMap::<
            PathBuf,
            (String, Module),
        >::new()));

        let engine = Engine::default();

        (senders, plugin_map, connected_clients, plugin_cache, engine)
    }

    fn create_test_executor(max_threads: usize) -> Arc<PinnedExecutor> {
        create_test_executor_with_limits(max_threads, QueueLimits::default())
    }

    fn create_test_executor_with_limits(
        max_threads: usize,
        limits: QueueLimits,
    ) -> Arc<PinnedExecutor> {
        let (senders, plugin_map, clients, cache, engine) = create_test_dependencies();
        Arc::new(PinnedExecutor::new_with_limits(
            max_threads,
            &senders,
            &plugin_map,
            &clients,
            &cache,
            &engine,
            limits,
        ))
    }

    fn small_test_limits() -> QueueLimits {
        QueueLimits {
            max_jobs_per_thread: 8,
            max_bytes_per_thread: 2048,
            max_jobs_per_owner: 4,
            max_bytes_per_owner: 1024,
            reserved_jobs_per_thread: 1,
            reserved_bytes_per_thread: 128,
            reserved_retirement_jobs_per_thread: 1,
            reserved_retirement_bytes_per_thread: 128,
            job_base_bytes: 32,
        }
    }

    fn wait_for<F: Fn() -> bool>(condition: F) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(
                std::time::Instant::now() < deadline,
                "condition was not met before the test deadline"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    // Helper to create a job that signals completion via channel
    fn make_signaling_job(
        tx: Sender<()>,
    ) -> impl FnOnce(
        ThreadSenders,
        Arc<Mutex<PluginMap>>,
        Arc<Mutex<Vec<ClientId>>>,
        PluginCache,
        Engine,
    ) + Send
           + 'static {
        move |_senders, _plugin_map, _clients, _cache, _engine| {
            tx.send(()).unwrap();
        }
    }

    // Helper to verify thread assignment by capturing thread name in job
    fn get_thread_name_for_plugin(executor: &Arc<PinnedExecutor>, plugin_id: u32) -> String {
        let (tx, rx) = channel();
        executor.execute_for_plugin(plugin_id, move |_s, _p, _c, _ca, _e| {
            let name = thread::current().name().unwrap().to_string();
            tx.send(name).unwrap();
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("Thread name should be received")
    }

    #[test]
    fn test_new_creates_one_thread() {
        let executor = create_test_executor(4);
        assert_eq!(executor.thread_count(), 1);
    }

    #[test]
    fn test_new_respects_min_threads() {
        let executor = create_test_executor(0);
        assert_eq!(
            executor.thread_count(),
            1,
            "Executor should enforce minimum of 1 thread"
        );
    }

    #[test]
    fn test_first_plugin_assigned_to_thread_zero() {
        let executor = create_test_executor(4);
        let thread_idx = executor.register_plugin(1);
        assert_eq!(thread_idx, 0);
    }

    #[test]
    fn test_multiple_plugins_share_thread_when_idle() {
        let executor = create_test_executor(4);
        let thread_idx1 = executor.register_plugin(1);
        let thread_idx2 = executor.register_plugin(2);
        assert_eq!(thread_idx1, 0);
        assert_eq!(
            thread_idx2, 0,
            "Second plugin should share thread 0 when idle"
        );
    }

    #[test]
    fn test_new_thread_spawns_when_all_busy() {
        let executor = create_test_executor(3);

        // Register plugin 1, gets thread 0
        let thread_idx1 = executor.register_plugin(1);
        assert_eq!(thread_idx1, 0);

        // Make thread 0 busy with a barrier
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier_clone.wait();
        });

        // Give the job a moment to start executing and block
        thread::sleep(Duration::from_millis(50));

        // Register plugin 2 while thread 0 is busy
        let thread_idx2 = executor.register_plugin(2);
        assert_eq!(
            thread_idx2, 1,
            "Plugin 2 should get new thread 1 when thread 0 is busy"
        );

        // Verify thread count
        assert_eq!(executor.thread_count(), 2);

        // Release barrier
        barrier.wait();
    }

    #[test]
    fn test_pool_expands_after_retired_thread_slots() {
        let executor = create_test_executor(2);
        executor.register_plugin(1);

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier_clone.wait();
        });
        thread::sleep(Duration::from_millis(50));

        assert_eq!(executor.register_plugin(2), 1);
        let (tx_unload, rx_unload) = channel();
        let _ = executor.execute_plugin_unload(2, make_signaling_job(tx_unload));
        rx_unload
            .recv_timeout(Duration::from_secs(5))
            .expect("Unload should complete");
        thread::sleep(Duration::from_millis(100));
        assert_eq!(executor.thread_count(), 1);
        assert!(
            !executor.thread_plugins.lock().unwrap().contains_key(&1),
            "retired worker bookkeeping must be removed"
        );

        barrier.wait();
        thread::sleep(Duration::from_millis(50));

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier_clone.wait();
        });
        thread::sleep(Duration::from_millis(50));

        assert_ne!(
            executor.register_plugin(3),
            0,
            "A retired slot must not prevent the pool from expanding"
        );
        assert_eq!(executor.thread_count(), 2);

        barrier.wait();
    }

    #[test]
    fn test_respects_max_threads_limit() {
        let executor = create_test_executor(2);

        // Register plugin 1, gets thread 0
        executor.register_plugin(1);

        // Make thread 0 busy
        let barrier1 = Arc::new(Barrier::new(2));
        let barrier1_clone = barrier1.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier1_clone.wait();
        });
        thread::sleep(Duration::from_millis(50));

        // Register plugin 2, gets thread 1
        executor.register_plugin(2);

        // Make thread 1 busy
        let barrier2 = Arc::new(Barrier::new(2));
        let barrier2_clone = barrier2.clone();
        executor.execute_for_plugin(2, move |_s, _p, _c, _ca, _e| {
            barrier2_clone.wait();
        });
        thread::sleep(Duration::from_millis(50));

        // Register plugin 3 when all threads busy
        let thread_idx3 = executor.register_plugin(3);
        assert!(
            thread_idx3 == 0 || thread_idx3 == 1,
            "Plugin 3 should be assigned to existing thread"
        );
        assert_eq!(executor.thread_count(), 2, "Should not exceed max_threads");

        // Release barriers
        barrier1.wait();
        barrier2.wait();
    }

    #[test]
    fn test_duplicate_registration_returns_same_thread() {
        let executor = create_test_executor(4);
        let thread_idx1 = executor.register_plugin(1);
        let thread_idx2 = executor.register_plugin(1);
        assert_eq!(
            thread_idx1, thread_idx2,
            "Duplicate registration should return same thread"
        );
    }

    #[test]
    fn test_load_balancing_prefers_least_loaded() {
        let executor = create_test_executor(3);

        // Register plugins 1, 2, 3 to thread 0 (when idle)
        executor.register_plugin(1);
        executor.register_plugin(2);
        executor.register_plugin(3);

        // Make thread 0 busy
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier_clone.wait();
        });
        thread::sleep(Duration::from_millis(50));

        // Register plugin 4 while thread 0 is busy (spawns thread 1)
        let thread_idx4 = executor.register_plugin(4);
        assert_eq!(thread_idx4, 1);

        // Release barrier
        barrier.wait();
        thread::sleep(Duration::from_millis(50));

        // Register plugin 5 when both threads idle
        // Thread 0 has 3 plugins, thread 1 has 1 plugin
        let thread_idx5 = executor.register_plugin(5);
        assert_eq!(
            thread_idx5, 1,
            "Plugin 5 should be assigned to less loaded thread 1"
        );
    }

    #[test]
    fn test_execute_for_plugin_runs_on_correct_thread() {
        let executor = create_test_executor(3);

        // Register plugin 1 to thread 0
        executor.register_plugin(1);

        // Make thread 0 busy to force plugin 2 to thread 1
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier_clone.wait();
        });
        thread::sleep(Duration::from_millis(50));

        // Register plugin 2 to thread 1
        executor.register_plugin(2);

        // Release barrier
        barrier.wait();
        thread::sleep(Duration::from_millis(50));

        // Get thread names for both plugins
        let thread_name1 = get_thread_name_for_plugin(&executor, 1);
        let thread_name2 = get_thread_name_for_plugin(&executor, 2);

        assert_eq!(thread_name1, "plugin-exec-0");
        assert_eq!(thread_name2, "plugin-exec-1");
    }

    #[test]
    fn test_execute_for_plugin_unregistered() {
        let executor = create_test_executor(4);
        let (tx, rx) = channel();

        // Execute job for unregistered plugin
        executor.execute_for_plugin(999, make_signaling_job(tx));

        // Try to receive with timeout - should timeout
        let result = rx.recv_timeout(Duration::from_millis(100));
        assert!(
            result.is_err(),
            "Job for unregistered plugin should not execute"
        );
    }

    #[test]
    fn test_job_execution_order_per_thread() {
        let executor = create_test_executor(4);
        executor.register_plugin(1);

        let order = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = channel();

        // Execute 3 jobs for plugin 1
        for i in 1..=3 {
            let order_clone = order.clone();
            let tx_clone = tx.clone();
            executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
                order_clone.lock().unwrap().push(i);
                tx_clone.send(()).unwrap();
            });
        }

        // Wait for all 3 jobs to complete
        for _ in 0..3 {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Job should complete");
        }

        assert_eq!(
            *order.lock().unwrap(),
            vec![1, 2, 3],
            "Jobs should execute in order"
        );
    }

    #[test]
    fn test_jobs_complete_successfully() {
        let executor = create_test_executor(4);
        executor.register_plugin(1);

        let (tx, rx) = channel();
        executor.execute_for_plugin(1, make_signaling_job(tx));

        let result = rx.recv_timeout(Duration::from_secs(5));
        assert!(result.is_ok(), "Job should complete successfully");
    }

    #[test]
    fn test_concurrent_jobs_on_different_threads() {
        let executor = create_test_executor(3);

        // Register plugin 1 to thread 0
        executor.register_plugin(1);

        // Make thread 0 busy to force plugin 2 to thread 1
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier_clone.wait();
        });
        thread::sleep(Duration::from_millis(50));

        // Register plugin 2 to thread 1
        executor.register_plugin(2);

        // Release barrier
        barrier.wait();
        thread::sleep(Duration::from_millis(50));

        // Execute jobs on both threads concurrently
        let sync_barrier = Arc::new(Barrier::new(3)); // 2 jobs + test thread
        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();

        let barrier1 = sync_barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier1.wait();
            tx1.send(()).unwrap();
        });

        let barrier2 = sync_barrier.clone();
        executor.execute_for_plugin(2, move |_s, _p, _c, _ca, _e| {
            barrier2.wait();
            tx2.send(()).unwrap();
        });

        // Release both jobs simultaneously
        sync_barrier.wait();

        // Both should complete
        assert!(rx1.recv_timeout(Duration::from_secs(5)).is_ok());
        assert!(rx2.recv_timeout(Duration::from_secs(5)).is_ok());
    }

    #[test]
    fn test_execute_plugin_load_registers_and_executes() {
        let executor = create_test_executor(4);

        let (tx, rx) = channel();
        executor.execute_plugin_load(1, make_signaling_job(tx));

        // Wait for load to complete
        rx.recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        // Verify plugin is registered
        let thread_idx = executor.register_plugin(1);
        assert_eq!(thread_idx, 0, "Plugin should already be registered");
    }

    #[test]
    fn test_execute_plugin_unload_runs_cleanup_before_unregister() {
        let executor = create_test_executor(4);

        // Load plugin
        let (tx_load, rx_load) = channel();
        executor.execute_plugin_load(1, make_signaling_job(tx_load));
        rx_load
            .recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        // Unload plugin with cleanup
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let (tx_unload, rx_unload) = channel();

        let _ = executor.execute_plugin_unload(1, move |_s, _p, _c, _ca, _e| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            tx_unload.send(()).unwrap();
        });

        // Wait for unload to complete
        rx_unload
            .recv_timeout(Duration::from_secs(5))
            .expect("Unload should complete");

        // Verify cleanup ran
        assert_eq!(counter.load(Ordering::SeqCst), 1, "Cleanup should have run");

        // Give unregister a moment to complete
        thread::sleep(Duration::from_millis(100));

        // Plugin should be unregistered - registering again should assign new thread
        let thread_idx = executor.register_plugin(1);
        assert_eq!(thread_idx, 0, "Plugin should be re-registered to thread 0");
    }

    #[test]
    fn test_unload_many_runs_every_cleanup_before_single_retirement() {
        // A plugin that was instantiated once per connected client shares one
        // executor assignment; its unload must run every instance's cleanup
        // and retire the assignment exactly once.
        let executor = create_test_executor(2);
        executor.register_plugin(1);
        executor.register_plugin(2);

        let cleanup_count = 3;
        let (tx, rx) = channel();
        let cleanups: Vec<PluginWork> = (0..cleanup_count)
            .map(|_| {
                let tx = tx.clone();
                let job = move |_s, _p, _c, _ca, _e| {
                    tx.send(()).unwrap();
                };
                Box::new(job) as PluginWork
            })
            .collect();

        let _ = executor.execute_plugin_unload_many(1, cleanups);

        for _ in 0..cleanup_count {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Every instance cleanup must run before retirement");
        }

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while executor.plugin_assignments.lock().unwrap().contains_key(&1) {
            assert!(
                std::time::Instant::now() < deadline,
                "Assignment was not retired"
            );
            thread::sleep(Duration::from_millis(5));
        }

        // The sibling plugin's assignment and thread survive the retirement
        assert!(executor.plugin_assignments.lock().unwrap().contains_key(&2));
        let (tx2, rx2) = channel();
        executor.execute_for_plugin(2, make_signaling_job(tx2));
        rx2.recv_timeout(Duration::from_secs(5))
            .expect("Sibling plugin must remain schedulable");

        // The retired plugin id can be registered again
        executor.register_plugin(1);
        let (tx3, rx3) = channel();
        executor.execute_for_plugin(1, make_signaling_job(tx3));
        rx3.recv_timeout(Duration::from_secs(5))
            .expect("Re-registered plugin must remain schedulable");
    }

    #[test]
    fn test_unload_without_cleanups_still_retires_the_assignment() {
        // A plugin whose initialization failed has no instance left to clean up
        // but still holds an assignment.
        let executor = create_test_executor(2);
        executor.register_plugin(1);
        let _ = executor.execute_plugin_unload_many(1, vec![]);

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while executor.is_registered(1) {
            assert!(
                std::time::Instant::now() < deadline,
                "Assignment was not retired"
            );
            thread::sleep(Duration::from_millis(5));
        }

        // Unloading an id that is not registered has nothing to run or retire
        let _ = executor.execute_plugin_unload_many(1, vec![]);
        assert!(!executor.is_registered(1));
    }

    #[test]
    fn test_unload_sequence_is_correct() {
        let executor = create_test_executor(4);

        // Load plugin
        let (tx_load, rx_load) = channel();
        executor.execute_plugin_load(1, make_signaling_job(tx_load));
        rx_load
            .recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        // Unload with sequence tracking
        let sequence = Arc::new(Mutex::new(Vec::new()));
        let sequence_clone = sequence.clone();
        let (tx_unload, rx_unload) = channel();

        let _ = executor.execute_plugin_unload(1, move |_s, _p, _c, _ca, _e| {
            sequence_clone.lock().unwrap().push("cleanup");
            tx_unload.send(()).unwrap();
        });

        // Wait for unload to complete
        rx_unload
            .recv_timeout(Duration::from_secs(5))
            .expect("Unload should complete");
        sequence.lock().unwrap().push("after");

        assert_eq!(*sequence.lock().unwrap(), vec!["cleanup", "after"]);
    }

    #[test]
    fn test_shrink_removes_idle_threads() {
        let executor = create_test_executor(4);

        // Load plugin 1 to thread 0
        let (tx1, rx1) = channel();
        executor.execute_plugin_load(1, make_signaling_job(tx1));
        rx1.recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        // Make thread 0 busy to force plugin 2 to thread 1
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier_clone.wait();
        });
        thread::sleep(Duration::from_millis(50));

        // Load plugin 2 to thread 1
        let (tx2, rx2) = channel();
        executor.execute_plugin_load(2, make_signaling_job(tx2));
        rx2.recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        // Release barrier
        barrier.wait();
        thread::sleep(Duration::from_millis(50));

        let thread_count_before = executor.thread_count();
        assert!(thread_count_before >= 2, "Should have at least 2 threads");

        // Unload plugin 2
        let (tx_unload2, rx_unload2) = channel();
        let _ = executor.execute_plugin_unload(2, make_signaling_job(tx_unload2));
        rx_unload2
            .recv_timeout(Duration::from_secs(5))
            .expect("Unload should complete");

        // Give shrinking a moment to complete
        thread::sleep(Duration::from_millis(100));

        // Thread count should decrease after unloading
        let thread_count_after = executor.thread_count();
        assert!(
            thread_count_after < thread_count_before,
            "Idle threads should be removed"
        );
        assert!(thread_count_after >= 1, "Thread 0 should remain");
    }

    #[test]
    fn test_thread_zero_never_removed() {
        let executor = create_test_executor(4);

        // Load a plugin and then unload it
        let (tx_load, rx_load) = channel();
        executor.execute_plugin_load(1, make_signaling_job(tx_load));
        rx_load
            .recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        let (tx_unload, rx_unload) = channel();
        let _ = executor.execute_plugin_unload(1, make_signaling_job(tx_unload));
        rx_unload
            .recv_timeout(Duration::from_secs(5))
            .expect("Unload should complete");

        // Give shrinking a moment
        thread::sleep(Duration::from_millis(100));

        // Thread 0 should remain
        assert!(
            executor.thread_count() >= 1,
            "Thread 0 should never be removed"
        );
    }

    #[test]
    fn test_active_threads_not_removed() {
        let executor = create_test_executor(4);

        // Load plugin 1 to thread 0
        let (tx1, rx1) = channel();
        executor.execute_plugin_load(1, make_signaling_job(tx1));
        rx1.recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        // Force plugin 2 to thread 1 by making thread 0 busy
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier_clone.wait();
        });
        thread::sleep(Duration::from_millis(50));

        let (tx2, rx2) = channel();
        executor.execute_plugin_load(2, make_signaling_job(tx2));
        rx2.recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        barrier.wait();
        thread::sleep(Duration::from_millis(100));

        let thread_count_with_both = executor.thread_count();
        assert!(
            thread_count_with_both >= 2,
            "Should have at least 2 threads with 2 plugins"
        );

        // Unload plugin 2
        let (tx_unload, rx_unload) = channel();
        let _ = executor.execute_plugin_unload(2, make_signaling_job(tx_unload));
        rx_unload
            .recv_timeout(Duration::from_secs(5))
            .expect("Unload should complete");

        thread::sleep(Duration::from_millis(100));

        // Plugin 1's thread should still work (verify active threads not affected)
        let (tx_test, rx_test) = channel();
        executor.execute_for_plugin(1, make_signaling_job(tx_test));
        assert!(
            rx_test.recv_timeout(Duration::from_secs(5)).is_ok(),
            "Plugin 1's thread should still work"
        );

        // Thread count should decrease after unloading
        let thread_count_after = executor.thread_count();
        assert!(
            thread_count_after < thread_count_with_both,
            "Idle thread should be removed"
        );
        assert!(thread_count_after >= 1, "Active threads should remain");
    }

    #[test]
    fn test_shrink_does_not_affect_remaining_threads() {
        let executor = create_test_executor(4);

        // Load plugin 1 to thread 0
        let (tx1, rx1) = channel();
        executor.execute_plugin_load(1, make_signaling_job(tx1));
        rx1.recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        // Force plugin 2 to thread 1
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier_clone.wait();
        });
        thread::sleep(Duration::from_millis(50));

        let (tx2, rx2) = channel();
        executor.execute_plugin_load(2, make_signaling_job(tx2));
        rx2.recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        barrier.wait();
        thread::sleep(Duration::from_millis(50));

        // Unload plugin 2 (shrinks pool)
        let (tx_unload, rx_unload) = channel();
        let _ = executor.execute_plugin_unload(2, make_signaling_job(tx_unload));
        rx_unload
            .recv_timeout(Duration::from_secs(5))
            .expect("Unload should complete");

        thread::sleep(Duration::from_millis(100));

        // Execute job for plugin 1
        let (tx_test, rx_test) = channel();
        executor.execute_for_plugin(1, make_signaling_job(tx_test));

        assert!(
            rx_test.recv_timeout(Duration::from_secs(5)).is_ok(),
            "Plugin 1's thread should still work"
        );
    }

    #[test]
    fn test_drop_cleans_up_gracefully() {
        let executor = create_test_executor(4);

        // Load multiple plugins on different threads
        let (tx1, rx1) = channel();
        executor.execute_plugin_load(1, make_signaling_job(tx1));
        rx1.recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier_clone.wait();
        });
        thread::sleep(Duration::from_millis(50));

        let (tx2, rx2) = channel();
        executor.execute_plugin_load(2, make_signaling_job(tx2));
        rx2.recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        barrier.wait();

        // Drop executor
        drop(executor);

        // Test completes without panic
    }

    #[test]
    fn test_drop_with_jobs_in_flight() {
        let executor = create_test_executor(4);
        executor.register_plugin(1);

        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        executor.execute_for_plugin(1, move |_s, _p, _c, _ca, _e| {
            barrier_clone.wait();
        });

        // Drop executor while job is blocked
        drop(executor);

        // Release barrier (job may or may not complete, but shouldn't panic)
        barrier.wait();

        // Test completes without panic
    }

    #[test]
    fn test_concurrent_plugin_registrations() {
        let executor = create_test_executor(4);

        let handles: Vec<_> = (1..=10)
            .map(|i| {
                let exec = executor.clone();
                thread::spawn(move || exec.register_plugin(i))
            })
            .collect();

        for handle in handles {
            let thread_idx = handle.join().expect("Thread should not panic");
            assert!(thread_idx < 4, "Thread index should be valid");
        }
    }

    #[test]
    fn test_unregister_nonexistent_plugin() {
        let executor = create_test_executor(4);

        // Unregister non-existent plugin
        executor.unregister_plugin(999);

        // Executor should still work
        let (tx, rx) = channel();
        executor.execute_plugin_load(1, make_signaling_job(tx));
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok());
    }

    #[test]
    fn test_max_threads_one() {
        let executor = create_test_executor(1);

        // Register multiple plugins
        let thread_idx1 = executor.register_plugin(1);
        let thread_idx2 = executor.register_plugin(2);
        let thread_idx3 = executor.register_plugin(3);

        assert_eq!(thread_idx1, 0);
        assert_eq!(thread_idx2, 0);
        assert_eq!(thread_idx3, 0);
        assert_eq!(executor.thread_count(), 1);
    }

    #[test]
    fn test_rapid_load_unload_cycles() {
        let executor = create_test_executor(4);

        // Load plugin 1
        let (tx1, rx1) = channel();
        executor.execute_plugin_load(1, make_signaling_job(tx1));
        rx1.recv_timeout(Duration::from_secs(5))
            .expect("Load should complete");

        // Unload plugin 1
        let (tx2, rx2) = channel();
        let _ = executor.execute_plugin_unload(1, make_signaling_job(tx2));
        rx2.recv_timeout(Duration::from_secs(5))
            .expect("Unload should complete");

        thread::sleep(Duration::from_millis(100));

        // Load plugin 1 again
        let (tx3, rx3) = channel();
        executor.execute_plugin_load(1, make_signaling_job(tx3));
        rx3.recv_timeout(Duration::from_secs(5))
            .expect("Second load should complete");

        // Execute job for plugin 1
        let (tx4, rx4) = channel();
        executor.execute_for_plugin(1, make_signaling_job(tx4));
        assert!(
            rx4.recv_timeout(Duration::from_secs(5)).is_ok(),
            "Executor should handle cycles correctly"
        );
    }

    #[test]
    fn test_many_plugins_limited_threads() {
        let executor = create_test_executor(4);
        let (tx, rx) = channel();

        // Load 20 plugins
        for i in 1..=20 {
            let tx_clone = tx.clone();
            executor.execute_plugin_load(i, make_signaling_job(tx_clone));
        }

        // Collect 20 completion signals
        for _ in 1..=20 {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Load should complete");
        }

        assert!(
            executor.thread_count() <= 4,
            "Should not exceed max_threads"
        );
    }

    #[test]
    fn test_full_lifecycle() {
        let executor = create_test_executor(4);
        let (tx, rx) = channel();

        // Load 5 plugins
        for i in 1..=5 {
            let tx_clone = tx.clone();
            executor.execute_plugin_load(i, make_signaling_job(tx_clone));
        }

        // Wait for all loads
        for _ in 0..5 {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Load should complete");
        }

        // Execute 2 jobs per plugin (10 total)
        for i in 1..=5 {
            for _ in 0..2 {
                let tx_clone = tx.clone();
                executor.execute_for_plugin(i, make_signaling_job(tx_clone));
            }
        }

        // Wait for all jobs
        for _ in 0..10 {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Job should complete");
        }

        let thread_count_before = executor.thread_count();

        // Unload 3 plugins
        for i in 1..=3 {
            let tx_clone = tx.clone();
            let _ = executor.execute_plugin_unload(i, make_signaling_job(tx_clone));
        }

        // Wait for unloads
        for _ in 0..3 {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Unload should complete");
        }

        thread::sleep(Duration::from_millis(100));

        // Thread count should decrease or stay the same
        let thread_count_after = executor.thread_count();
        assert!(
            thread_count_after <= thread_count_before,
            "Thread count should decrease after unloads"
        );

        // Execute jobs for remaining plugins
        for i in 4..=5 {
            let tx_clone = tx.clone();
            executor.execute_for_plugin(i, make_signaling_job(tx_clone));
        }

        for _ in 0..2 {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Job should complete");
        }

        // Drop executor
        drop(executor);
    }

    #[test]
    fn test_realistic_plugin_churn() {
        let executor = create_test_executor(4);
        let (tx, rx) = channel();

        // Load plugins 1, 2, 3
        for i in 1..=3 {
            let tx_clone = tx.clone();
            executor.execute_plugin_load(i, make_signaling_job(tx_clone));
        }
        for _ in 0..3 {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Load should complete");
        }

        // Execute jobs for each
        for i in 1..=3 {
            let tx_clone = tx.clone();
            executor.execute_for_plugin(i, make_signaling_job(tx_clone));
        }
        for _ in 0..3 {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Job should complete");
        }

        // Unload plugin 2
        let tx_clone = tx.clone();
        let _ = executor.execute_plugin_unload(2, make_signaling_job(tx_clone));
        rx.recv_timeout(Duration::from_secs(5))
            .expect("Unload should complete");

        thread::sleep(Duration::from_millis(100));

        // Load plugins 4, 5
        for i in 4..=5 {
            let tx_clone = tx.clone();
            executor.execute_plugin_load(i, make_signaling_job(tx_clone));
        }
        for _ in 0..2 {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Load should complete");
        }

        // Execute jobs for plugins 1, 3, 4, 5
        for i in &[1, 3, 4, 5] {
            let tx_clone = tx.clone();
            executor.execute_for_plugin(*i, make_signaling_job(tx_clone));
        }
        for _ in 0..4 {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Job should complete");
        }

        // Unload plugins 1, 3
        for i in &[1, 3] {
            let tx_clone = tx.clone();
            let _ = executor.execute_plugin_unload(*i, make_signaling_job(tx_clone));
        }
        for _ in 0..2 {
            rx.recv_timeout(Duration::from_secs(5))
                .expect("Unload should complete");
        }

        thread::sleep(Duration::from_millis(100));

        // Verify thread count reflects active plugins (4 and 5)
        assert!(
            executor.thread_count() >= 1,
            "Should have at least thread 0"
        );

        drop(executor);
    }

    #[test]
    fn a_slow_successful_callback_bounds_the_ordered_backlog() {
        let limits = small_test_limits();
        let executor = create_test_executor_with_limits(1, limits);
        executor.register_plugin(1);

        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let blocking_release = release_rx.clone();
        executor
            .try_execute_for_plugin(1, JobClass::Ordered, 0, move |_s, _p, _c, _ca, _e| {
                started_tx.send(()).unwrap();
                let _ = blocking_release.lock().unwrap().recv();
            })
            .expect("the blocking job must be admitted");
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the blocking job should start");

        let executed = Arc::new(AtomicUsize::new(0));
        let mut accepted = 0;
        for _ in 0..100 {
            let executed = executed.clone();
            if executor
                .try_execute_for_plugin(1, JobClass::Ordered, 128, move |_s, _p, _c, _ca, _e| {
                    executed.fetch_add(1, Ordering::SeqCst);
                })
                .is_ok()
            {
                accepted += 1;
            }
        }

        assert!(accepted > 0, "some ordered work should fit the budget");
        assert!(
            accepted < 100,
            "a blocked worker must not admit an unbounded ordered backlog"
        );
        assert!(executor.owner_queued_jobs(1) <= limits.max_jobs_per_owner);
        assert!(executor.queued_jobs() <= limits.max_jobs_per_thread);
        assert!(executor.queued_bytes() <= limits.max_bytes_per_thread);

        release_tx.send(()).unwrap();
        wait_for(|| executed.load(Ordering::SeqCst) == accepted && executor.queued_jobs() == 0);
        assert_eq!(executed.load(Ordering::SeqCst), accepted);
    }

    #[test]
    fn replaceable_state_coalesces_to_the_latest_payload() {
        let executor = create_test_executor_with_limits(1, small_test_limits());
        executor.register_plugin(1);

        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let blocking_release = release_rx.clone();
        executor
            .try_execute_for_plugin(1, JobClass::Ordered, 0, move |_s, _p, _c, _ca, _e| {
                started_tx.send(()).unwrap();
                let _ = blocking_release.lock().unwrap().recv();
            })
            .expect("the blocking job must be admitted");
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the blocking job should start");

        let executed = Arc::new(Mutex::new(Vec::new()));
        for value in 0..50u32 {
            let executed = executed.clone();
            executor
                .try_execute_for_plugin(
                    1,
                    JobClass::Replaceable(7),
                    32,
                    move |_s, _p, _c, _ca, _e| executed.lock().unwrap().push(value),
                )
                .expect("replaceable state should coalesce instead of growing");
        }

        assert_eq!(
            executor.owner_queued_jobs(1),
            2,
            "only the blocked job and one pending replaceable slot should remain"
        );

        release_tx.send(()).unwrap();
        wait_for(|| executor.queued_jobs() == 0);
        assert_eq!(*executed.lock().unwrap(), vec![49]);
    }

    #[test]
    fn coalescing_does_not_reorder_replaceable_state_past_ordered_work() {
        let executor = create_test_executor_with_limits(1, small_test_limits());
        executor.register_plugin(1);

        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let blocking_release = release_rx.clone();
        executor
            .try_execute_for_plugin(1, JobClass::Ordered, 0, move |_s, _p, _c, _ca, _e| {
                started_tx.send(()).unwrap();
                let _ = blocking_release.lock().unwrap().recv();
            })
            .expect("the blocking job must be admitted");
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the blocking job should start");

        let executed = Arc::new(Mutex::new(Vec::new()));
        let push = |executed: &Arc<Mutex<Vec<&'static str>>>, value| {
            let executed = executed.clone();
            move |_s: ThreadSenders,
                  _p: Arc<Mutex<PluginMap>>,
                  _c: Arc<Mutex<Vec<ClientId>>>,
                  _ca: PluginCache,
                  _e: Engine| executed.lock().unwrap().push(value)
        };
        executor
            .try_execute_for_plugin(1, JobClass::Replaceable(1), 32, push(&executed, "state-a"))
            .unwrap();
        executor
            .try_execute_for_plugin(1, JobClass::Ordered, 32, push(&executed, "event"))
            .unwrap();
        executor
            .try_execute_for_plugin(1, JobClass::Replaceable(1), 32, push(&executed, "state-b"))
            .unwrap();

        release_tx.send(()).unwrap();
        wait_for(|| executor.queued_jobs() == 0);
        assert_eq!(
            *executed.lock().unwrap(),
            vec!["state-a", "event", "state-b"]
        );
    }

    #[test]
    fn a_reserved_lifecycle_job_is_a_coalescing_barrier() {
        let executor = create_test_executor_with_limits(1, small_test_limits());
        executor.register_plugin(1);

        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let blocking_release = release_rx.clone();
        executor
            .try_execute_for_plugin(1, JobClass::Ordered, 0, move |_s, _p, _c, _ca, _e| {
                started_tx.send(()).unwrap();
                let _ = blocking_release.lock().unwrap().recv();
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        let executed = Arc::new(Mutex::new(Vec::new()));
        let push = |executed: &Arc<Mutex<Vec<&'static str>>>, value| {
            let executed = executed.clone();
            move |_s: ThreadSenders,
                  _p: Arc<Mutex<PluginMap>>,
                  _c: Arc<Mutex<Vec<ClientId>>>,
                  _ca: PluginCache,
                  _e: Engine| executed.lock().unwrap().push(value)
        };
        executor
            .try_execute_for_plugin(1, JobClass::Replaceable(1), 32, push(&executed, "before"))
            .unwrap();
        let reservation = executor
            .reserve_job(1, JobClass::Lifecycle, 0)
            .expect("the reserved lifecycle job must be admitted");
        executor
            .try_execute_for_plugin(1, JobClass::Replaceable(1), 32, push(&executed, "after"))
            .unwrap();
        reservation.commit(Box::new(push(&executed, "reserved")));

        release_tx.send(()).unwrap();
        wait_for(|| executor.queued_jobs() == 0);
        assert_eq!(
            *executed.lock().unwrap(),
            vec!["before", "reserved", "after"]
        );
    }

    #[test]
    fn replaceable_keys_with_different_destinations_both_execute() {
        let executor = create_test_executor_with_limits(1, small_test_limits());
        executor.register_plugin(1);

        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let blocking_release = release_rx.clone();
        executor
            .try_execute_for_plugin(1, JobClass::Ordered, 0, move |_s, _p, _c, _ca, _e| {
                started_tx.send(()).unwrap();
                let _ = blocking_release.lock().unwrap().recv();
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        let executed = Arc::new(Mutex::new(Vec::new()));
        let push = |executed: &Arc<Mutex<Vec<&'static str>>>, value| {
            let executed = executed.clone();
            move |_s: ThreadSenders,
                  _p: Arc<Mutex<PluginMap>>,
                  _c: Arc<Mutex<Vec<ClientId>>>,
                  _ca: PluginCache,
                  _e: Engine| executed.lock().unwrap().push(value)
        };
        executor
            .try_execute_for_plugin(
                1,
                JobClass::Replaceable(0x0200_0000_0000_000b),
                32,
                push(&executed, "client-11"),
            )
            .unwrap();
        executor
            .try_execute_for_plugin(
                1,
                JobClass::Replaceable(0x0200_0000_0000_000c),
                32,
                push(&executed, "client-12"),
            )
            .unwrap();

        release_tx.send(()).unwrap();
        wait_for(|| executor.queued_jobs() == 0);
        assert_eq!(
            *executed.lock().unwrap(),
            vec!["client-11", "client-12"],
            "different destinations must not replace one another"
        );
    }

    #[test]
    fn replaceable_keys_from_different_generations_both_execute() {
        let executor = create_test_executor_with_limits(1, small_test_limits());
        executor.register_plugin(1);

        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let blocking_release = release_rx.clone();
        executor
            .try_execute_for_plugin(1, JobClass::Ordered, 0, move |_s, _p, _c, _ca, _e| {
                started_tx.send(()).unwrap();
                let _ = blocking_release.lock().unwrap().recv();
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        let executed = Arc::new(Mutex::new(Vec::new()));
        let push = |executed: &Arc<Mutex<Vec<&'static str>>>, value| {
            let executed = executed.clone();
            move |_s: ThreadSenders,
                  _p: Arc<Mutex<PluginMap>>,
                  _c: Arc<Mutex<Vec<ClientId>>>,
                  _ca: PluginCache,
                  _e: Engine| executed.lock().unwrap().push(value)
        };
        executor
            .try_execute_for_plugin(
                1,
                JobClass::Replaceable(0x0200_0001_0000_000b),
                32,
                push(&executed, "generation-1"),
            )
            .unwrap();
        executor
            .try_execute_for_plugin(
                1,
                JobClass::Replaceable(0x0200_0002_0000_000b),
                32,
                push(&executed, "generation-2"),
            )
            .unwrap();

        release_tx.send(()).unwrap();
        wait_for(|| executor.queued_jobs() == 0);
        assert_eq!(
            *executed.lock().unwrap(),
            vec!["generation-1", "generation-2"]
        );
    }

    #[test]
    fn unavailable_or_missing_workers_return_a_load_admission_error() {
        let executor = create_test_executor(1);
        assert!(matches!(
            executor.reserve_job(42, JobClass::Lifecycle, 0),
            Err(JobRejection::OwnerNotRegistered)
        ));
        assert!(!executor.is_registered(42));

        executor.register_plugin(1);
        executor.execution_threads.lock().unwrap().clear();
        assert!(matches!(
            executor.reserve_plugin_load(1, 0),
            Err(JobRejection::QueueClosed)
        ));
    }

    #[test]
    fn retirement_rejection_still_settles_the_plugin_assignment() {
        let executor = create_test_executor_with_limits(1, small_test_limits());
        executor.register_plugin(1);
        executor.register_plugin(2);

        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let blocking_release = release_rx.clone();
        executor
            .try_execute_for_plugin(1, JobClass::Ordered, 0, move |_s, _p, _c, _ca, _e| {
                started_tx.send(()).unwrap();
                let _ = blocking_release.lock().unwrap().recv();
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        for _ in 0..7 {
            executor
                .try_execute_for_plugin(2, JobClass::Lifecycle, 0, |_s, _p, _c, _ca, _e| {})
                .expect("lifecycle headroom should be available");
        }
        executor
            .try_execute_for_plugin(2, JobClass::Retirement, 0, |_s, _p, _c, _ca, _e| {})
            .expect("retirement headroom should be available");

        let (cleanup_tx, cleanup_rx) = channel();
        let result = executor.execute_plugin_unload_many(
            1,
            vec![Box::new(move |_s, _p, _c, _ca, _e| {
                cleanup_tx.send(()).unwrap();
            })],
        );
        assert!(matches!(result, Err(JobRejection::BudgetExceeded)));
        assert!(
            !executor.is_registered(1),
            "a rejected retirement must still settle its assignment"
        );
        assert!(cleanup_rx.try_recv().is_err());

        release_tx.send(()).unwrap();
        wait_for(|| executor.queued_jobs() == 0);
    }

    #[test]
    fn lifecycle_work_is_admitted_and_retires_while_a_host_call_blocks() {
        let limits = small_test_limits();
        let executor = create_test_executor_with_limits(1, limits);
        executor.register_plugin(1);

        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let blocking_release = release_rx.clone();
        executor
            .try_execute_for_plugin(1, JobClass::Ordered, 0, move |_s, _p, _c, _ca, _e| {
                started_tx.send(()).unwrap();
                let _ = blocking_release.lock().unwrap().recv();
            })
            .expect("the blocking job must be admitted");
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the blocking job should start");

        let mut rejected_ordered_work = false;
        for _ in 0..100 {
            if executor
                .try_execute_for_plugin(1, JobClass::Ordered, 128, |_s, _p, _c, _ca, _e| {})
                .is_err()
            {
                rejected_ordered_work = true;
                break;
            }
        }
        assert!(
            rejected_ordered_work,
            "the ordered backlog should be saturated before testing reserved capacity"
        );

        let (cleanup_tx, cleanup_rx) = channel();
        let _ = executor.execute_plugin_unload_many(
            1,
            vec![Box::new(move |_s, _p, _c, _ca, _e| {
                cleanup_tx.send(()).unwrap();
            })],
        );

        release_tx.send(()).unwrap();
        cleanup_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("retirement work must use reserved capacity");
        wait_for(|| !executor.is_registered(1));
    }
}

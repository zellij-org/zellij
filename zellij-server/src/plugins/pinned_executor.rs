use crate::diagnostics;
use crate::plugins::plugin_map::PluginMap;
use crate::plugins::wasm_bridge::PluginCache;
use crate::ClientId;
use crate::ThreadSenders;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use wasmi::Engine;

/// A dynamic thread pool that pins jobs to specific threads based on plugin_id
/// Starts with 1 thread and expands when threads are busy, shrinks when plugins unload
pub struct PinnedExecutor {
    // Live workers keyed by monotonically increasing, non-reused thread ids
    execution_threads: Arc<Mutex<HashMap<usize, ExecutionThread>>>,

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
}

struct ExecutionThread {
    sender: Sender<Job>,
    jobs_in_flight: Arc<AtomicUsize>, // Busy state tracking
}

/// A unit of work executed on a plugin's pinned thread.
pub type PluginWork = Box<
    dyn FnOnce(ThreadSenders, Arc<Mutex<PluginMap>>, Arc<Mutex<Vec<ClientId>>>, PluginCache, Engine)
        + Send
        + 'static,
>;

enum Job {
    Work(PluginWork),
    Shutdown, // Signal to exit the worker loop
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
        let max_threads = max_threads.max(1); // At least 1

        let mut execution_threads = HashMap::new();
        if let Some(thread_0) = Self::spawn_thread(
            0,
            senders.clone(),
            plugin_map.clone(),
            connected_clients.clone(),
            plugin_cache.clone(),
            engine.clone(),
        ) {
            execution_threads.insert(0, thread_0);
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
        }
    }

    fn spawn_thread(
        thread_idx: usize,
        senders: ThreadSenders,
        plugin_map: Arc<Mutex<PluginMap>>,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
        plugin_cache: PluginCache,
        engine: Engine,
    ) -> Option<ExecutionThread> {
        let (sender, receiver) = channel::<Job>();
        let jobs_in_flight = Arc::new(AtomicUsize::new(0));
        let jobs_in_flight_clone = jobs_in_flight.clone();

        let thread_handle = thread::Builder::new()
            .name(format!("plugin-exec-{}", thread_idx))
            .spawn({
                move || {
                    let senders = senders;
                    let plugin_map = plugin_map;
                    let connected_clients = connected_clients;
                    let plugin_cache = plugin_cache;
                    let engine = engine;
                    while let Ok(job) = receiver.recv() {
                        match job {
                            Job::Work(work) => {
                                work(
                                    senders.clone(),
                                    plugin_map.clone(),
                                    connected_clients.clone(),
                                    plugin_cache.clone(),
                                    engine.clone(),
                                );
                                jobs_in_flight_clone.fetch_sub(1, Ordering::SeqCst);
                            },
                            Job::Shutdown => break,
                        }
                    }
                }
            });
        match thread_handle {
            Ok(_) => Some(ExecutionThread {
                sender,
                jobs_in_flight,
            }),
            Err(e) => {
                log::error!("Failed to spawn plugin execution thread: {}", e);
                None
            },
        }
    }

    /// Register a plugin and assign it to a thread
    /// Called from wasm_bridge when loading a plugin
    pub fn register_plugin(&self, plugin_id: u32) -> usize {
        let mut assignments = self.plugin_assignments.lock().unwrap();

        // If already assigned (shouldn't happen, but defensive)
        if let Some(&thread_idx) = assignments.get(&plugin_id) {
            return thread_idx;
        }

        let mut thread_plugins = self.thread_plugins.lock().unwrap();
        let threads = self.execution_threads.lock().unwrap();

        // Find a non-busy thread with assigned plugins (prefer reusing threads)
        let mut best_thread: Option<(usize, usize)> = None; // (index, load)

        for (idx, thread) in threads.iter() {
            let is_busy = thread.jobs_in_flight.load(Ordering::SeqCst) > 0;
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
            return 0;
        };

        // Update mappings
        assignments.insert(plugin_id, thread_idx);
        thread_plugins
            .entry(thread_idx)
            .or_insert_with(HashSet::new)
            .insert(plugin_id);

        thread_idx
    }

    fn least_loaded_thread(
        threads: &HashMap<usize, ExecutionThread>,
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
        ) else {
            return false;
        };

        threads.insert(thread_idx, new_thread);
        true
    }

    /// Execute job pinned to plugin's assigned thread
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
        // Look up assigned thread
        // Keep the assignment lock until the job is queued. Unregistering takes this lock before
        // it can empty the thread's plugin set and shrink the pool, so the thread cannot be
        // retired between the lookup and the send.
        let assignments = self.plugin_assignments.lock().unwrap();
        let Some(thread_idx) = assignments.get(&plugin_id).copied() else {
            log::error!("Failed to find thread for plugin with id: {}", plugin_id);
            return;
        };

        // Get thread and mark as busy
        let threads = self.execution_threads.lock().unwrap();
        let Some(thread) = threads.get(&thread_idx) else {
            log::error!("Failed to find thread for plugin with id: {}", plugin_id);
            return;
        };

        // Increment busy counter BEFORE sending work
        let jobs_in_flight = thread.jobs_in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        diagnostics::record_plugin_jobs_in_flight(jobs_in_flight);

        // Send work
        let job = Job::Work(Box::new(f));
        if let Err(_) = thread.sender.send(job) {
            // Thread died unexpectedly - this is a critical error
            thread.jobs_in_flight.fetch_sub(1, Ordering::SeqCst);
            log::error!("Plugin executor thread {} has died", thread_idx);
        }
    }

    /// Load a plugin: register it and execute the load work on its assigned thread
    /// This combines registration + execution for plugin loading
    pub fn execute_plugin_load<F>(&self, plugin_id: u32, f: F)
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
        // Register plugin and assign to a thread
        self.register_plugin(plugin_id);

        // Execute the load work on the assigned thread
        self.execute_for_plugin(plugin_id, f);
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
    ) {
        self.execute_plugin_unload_many(plugin_id, vec![Box::new(f)]);
    }

    /// Unload a plugin family: run every cleanup job on the plugin's pinned
    /// thread, then retire its executor assignment exactly once.
    ///
    /// A plugin can own several client instances that all share one executor
    /// assignment. Submitting a separate unload per instance lets the first
    /// job's unregistration drop later submissions before their cleanup runs;
    /// batching them into a single job keeps every cleanup ahead of the one
    /// retirement.
    pub fn execute_plugin_unload_many(self: &Arc<Self>, plugin_id: u32, cleanups: Vec<PluginWork>) {
        if cleanups.is_empty() && !self.is_registered(plugin_id) {
            // nothing to run and nothing to retire
            return;
        }
        let executor = self.clone();
        self.execute_for_plugin(
            plugin_id,
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
        );
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
                let _ = thread.sender.send(Job::Shutdown);
                thread_plugins.remove(&idx);
            }
        }
    }

    #[cfg(test)]
    pub fn thread_count(&self) -> usize {
        self.execution_threads.lock().unwrap().len()
    }
}

impl Drop for PinnedExecutor {
    fn drop(&mut self) {
        let mut threads = self.execution_threads.lock().unwrap();

        // Send shutdown to all threads
        for thread in threads.values_mut() {
            let _ = thread.sender.send(Job::Shutdown);
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
        let (senders, plugin_map, clients, cache, engine) = create_test_dependencies();
        Arc::new(PinnedExecutor::new(
            max_threads,
            &senders,
            &plugin_map,
            &clients,
            &cache,
            &engine,
        ))
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
        executor.execute_plugin_unload(2, make_signaling_job(tx_unload));
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

        executor.execute_plugin_unload(1, move |_s, _p, _c, _ca, _e| {
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

        executor.execute_plugin_unload_many(1, cleanups);

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
        executor.execute_plugin_unload_many(1, vec![]);

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while executor.is_registered(1) {
            assert!(
                std::time::Instant::now() < deadline,
                "Assignment was not retired"
            );
            thread::sleep(Duration::from_millis(5));
        }

        // Unloading an id that is not registered has nothing to run or retire
        executor.execute_plugin_unload_many(1, vec![]);
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

        executor.execute_plugin_unload(1, move |_s, _p, _c, _ca, _e| {
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
        executor.execute_plugin_unload(2, make_signaling_job(tx_unload2));
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
        executor.execute_plugin_unload(1, make_signaling_job(tx_unload));
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
        executor.execute_plugin_unload(2, make_signaling_job(tx_unload));
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
        executor.execute_plugin_unload(2, make_signaling_job(tx_unload));
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
        executor.execute_plugin_unload(1, make_signaling_job(tx2));
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
            executor.execute_plugin_unload(i, make_signaling_job(tx_clone));
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
        executor.execute_plugin_unload(2, make_signaling_job(tx_clone));
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
            executor.execute_plugin_unload(*i, make_signaling_job(tx_clone));
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
}

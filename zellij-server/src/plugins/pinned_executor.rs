use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender};
use std::thread::{self, JoinHandle};
use crate::ThreadSenders;
use crate::plugins::plugin_map::PluginMap;
use crate::ClientId;
use zellij_utils::input::layout::Layout;

/// A dynamic thread pool that pins jobs to specific threads based on plugin_id
/// Starts with 1 thread and expands when threads are busy, shrinks when plugins unload
pub struct PinnedExecutor {
    // Sparse vector - Some(thread) for active threads, None for removed threads
    execution_threads: Arc<Mutex<Vec<Option<ExecutionThread>>>>,

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
    default_layout: Box<Layout>,
}

struct ExecutionThread {
    sender: Sender<Job>,
    handle: Option<JoinHandle<()>>,  // Option for taking during shutdown
    jobs_in_flight: Arc<AtomicUsize>,  // Busy state tracking
}

enum Job {
    // Work(Box<dyn FnOnce() + Send + 'static>),
    Work(Box<dyn FnOnce(ThreadSenders, Arc<Mutex<PluginMap>>, Arc<Mutex<Vec<ClientId>>>, Box<Layout>) + Send + 'static>),
    Shutdown,  // Signal to exit the worker loop
}

impl PinnedExecutor {
    /// Creates a new pinned executor with the specified maximum number of threads
    /// Starts with exactly 1 thread
    pub fn new(max_threads: usize, senders: &ThreadSenders, plugin_map: &Arc<Mutex<PluginMap>>, connected_clients: &Arc<Mutex<Vec<ClientId>>>, default_layout: &Box<Layout>) -> Self {
        let max_threads = max_threads.max(1);  // At least 1

        // Start with exactly 1 thread (thread index 0)
        let thread_0 = Self::spawn_thread(0, senders.clone(), plugin_map.clone(), connected_clients.clone(), default_layout.clone());

        PinnedExecutor {
            execution_threads: Arc::new(Mutex::new(vec![Some(thread_0)])),
            plugin_assignments: Arc::new(Mutex::new(HashMap::new())),
            thread_plugins: Arc::new(Mutex::new(HashMap::new())),
            next_thread_idx: AtomicUsize::new(1),  // Next will be index 1
            max_threads,
            senders: senders.clone(),
            plugin_map: plugin_map.clone(),
            connected_clients: connected_clients.clone(),
            default_layout: default_layout.clone(),
        }
    }

    // fn spawn_thread(thread_idx: usize) -> ExecutionThread {
    fn spawn_thread(thread_idx: usize, senders: ThreadSenders, plugin_map: Arc<Mutex<PluginMap>>, connected_clients: Arc<Mutex<Vec<ClientId>>>, default_layout: Box<Layout>) -> ExecutionThread {
        let (sender, receiver) = channel::<Job>();
        let jobs_in_flight = Arc::new(AtomicUsize::new(0));
        let jobs_in_flight_clone = jobs_in_flight.clone();

        let handle = thread::Builder::new()
            .name(format!("plugin-exec-{}", thread_idx))
            .spawn({
                let senders = senders;
                let plugin_map = plugin_map;
                let connected_clients = connected_clients;
                let default_layout = default_layout;
                move || {
                    while let Ok(job) = receiver.recv() {
                        match job {
                            Job::Work(work) => {
                                work(
                                    senders.clone(),
                                    plugin_map.clone(),
                                    connected_clients.clone(),
                                    default_layout.clone()
                                );
                                jobs_in_flight_clone.fetch_sub(1, Ordering::SeqCst);
                            }
                            Job::Shutdown => break,
                        }
                    }
                }
            })
            .expect("Failed to spawn execution thread");

        ExecutionThread {
            sender,
            handle: Some(handle),
            jobs_in_flight,
        }
    }

    /// Register a plugin and assign it to a thread
    /// Called from wasm_bridge when loading a plugin
    pub fn register_plugin(&self, plugin_id: u32) -> usize {
        log::info!("register_plugin: {:?}, pool size: {:?}", plugin_id, self.thread_count());
        let mut assignments = self.plugin_assignments.lock().unwrap();

        // If already assigned (shouldn't happen, but defensive)
        if let Some(&thread_idx) = assignments.get(&plugin_id) {
            log::info!("already assigned");
            return thread_idx;
        }

        let mut thread_plugins = self.thread_plugins.lock().unwrap();
        let threads = self.execution_threads.lock().unwrap();

        // Find a non-busy thread with assigned plugins (prefer reusing threads)
        let mut best_thread: Option<(usize, usize)> = None;  // (index, load)

        for (idx, thread_opt) in threads.iter().enumerate() {
            if let Some(thread) = thread_opt {
                let is_busy = thread.jobs_in_flight.load(Ordering::SeqCst) > 0;
                if !is_busy {
                    let load = thread_plugins.get(&idx).map(|s| s.len()).unwrap_or(0);
                    if best_thread.is_none() || load < best_thread.unwrap().1 {
                        best_thread = Some((idx, load));
                    }
                }
            }
        }

        let thread_idx = if let Some((idx, _)) = best_thread {
            // Found a non-busy thread
            idx
        } else {
            // All threads are busy - need to expand
            if threads.len() < self.max_threads {
                // Spawn a new thread
                let new_idx = self.next_thread_idx.fetch_add(1, Ordering::SeqCst);
                drop(threads);  // Release lock before spawning
                self.add_thread(new_idx);
                new_idx
            } else {
                // At max capacity, assign to least-loaded thread
                threads.iter()
                    .enumerate()
                    .filter_map(|(idx, t)| t.as_ref().map(|_| idx))
                    .min_by_key(|&idx| {
                        thread_plugins.get(&idx).map(|s| s.len()).unwrap_or(0)
                    })
                    .expect("Must have at least one thread")
            }
        };

        // Update mappings
        assignments.insert(plugin_id, thread_idx);
        thread_plugins.entry(thread_idx).or_insert_with(HashSet::new).insert(plugin_id);

        thread_idx
    }

    fn add_thread(&self, thread_idx: usize) {
        log::info!("ADD_THREAD: {:?}", thread_idx);
        let mut threads = self.execution_threads.lock().unwrap();
        let new_thread = Self::spawn_thread(thread_idx, self.senders.clone(), self.plugin_map.clone(), self.connected_clients.clone(), self.default_layout.clone());

        // Extend vector if needed
        while threads.len() <= thread_idx {
            threads.push(None);
        }
        threads[thread_idx] = Some(new_thread);
    }

    /// Execute job pinned to plugin's assigned thread
    pub fn execute_for_plugin<F>(&self, plugin_id: u32, f: F)
    where
        // F: FnOnce() + Send + 'static,
        F: FnOnce(ThreadSenders, Arc<Mutex<PluginMap>>, Arc<Mutex<Vec<ClientId>>>, Box<Layout>) + Send + 'static,
    {
        // Look up assigned thread
        let thread_idx = {
            let assignments = self.plugin_assignments.lock().unwrap();
            *assignments.get(&plugin_id).expect(&format!(
                "Plugin {} not registered! Call register_plugin first.",
                plugin_id
            ))
        };

        // Get thread and mark as busy
        let threads = self.execution_threads.lock().unwrap();
        let thread = threads[thread_idx].as_ref().expect("Thread should exist");

        // Increment busy counter BEFORE sending work
        thread.jobs_in_flight.fetch_add(1, Ordering::SeqCst);

        // Send work
        let job = Job::Work(Box::new(f));
        if let Err(_) = thread.sender.send(job) {
            // Thread died unexpectedly - this is a critical error
            thread.jobs_in_flight.fetch_sub(1, Ordering::SeqCst);
            panic!("Plugin executor thread {} has died", thread_idx);
        }
    }

    /// Load a plugin: register it and execute the load work on its assigned thread
    /// This combines registration + execution for plugin loading
    pub fn execute_plugin_load<F>(&self, plugin_id: u32, f: F)
    where
        // F: FnOnce() + Send + 'static,
        F: FnOnce(ThreadSenders, Arc<Mutex<PluginMap>>, Arc<Mutex<Vec<ClientId>>>, Box<Layout>) + Send + 'static,
    {
        // Register plugin and assign to a thread
        self.register_plugin(plugin_id);

        // Execute the load work on the assigned thread
        self.execute_for_plugin(plugin_id, f);
    }

    /// Unload a plugin: execute cleanup work, then unregister and potentially shrink pool
    /// This combines cleanup execution + unregistration for plugin unloading
    /// Requires Arc<Self> so we can clone it into the closure for unregistration
    pub fn execute_plugin_unload(self: &Arc<Self>, plugin_id: u32, f: impl FnOnce(ThreadSenders, Arc<Mutex<PluginMap>>, Arc<Mutex<Vec<ClientId>>>, Box<Layout>) + Send + 'static)
        // FnOnce() + Send + 'static)
    {
        let executor = self.clone();
        self.execute_for_plugin(plugin_id, move |senders, plugin_map, connected_clients, default_layout| {
            // Execute the cleanup work
            f(senders, plugin_map, connected_clients, default_layout);

            // Unregister plugin and potentially shrink the pool
            executor.unregister_plugin(plugin_id);
        });
    }

    /// Unregister a plugin and potentially shrink the pool
    /// Called from wasm_bridge after plugin cleanup is complete
    pub fn unregister_plugin(&self, plugin_id: u32) {
        log::info!("unregister_plugin: {:?}, pool_size: {:?}", plugin_id, self.thread_count());
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
        log::info!("try_shrink_pool");
        let mut threads = self.execution_threads.lock().unwrap();
        let thread_plugins = self.thread_plugins.lock().unwrap();

        // Find threads with no assigned plugins (except thread 0, always keep it)
        let threads_to_remove: Vec<usize> = threads.iter()
            .enumerate()
            .skip(1)  // Never remove thread 0
            .filter_map(|(idx, thread_opt)| {
                if thread_opt.is_some() {
                    let has_plugins = thread_plugins.get(&idx)
                        .map(|s| !s.is_empty())
                        .unwrap_or(false);
                    if !has_plugins {
                        Some(idx)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        log::info!("threads_to_remove: {:?}", threads_to_remove);

        // Shutdown and remove idle threads
        for idx in threads_to_remove {
            if let Some(mut thread) = threads[idx].take() {
                // Send shutdown signal
                let _ = thread.sender.send(Job::Shutdown);

                // Join the thread (blocks until it exits)
                if let Some(handle) = thread.handle.take() {
                    let _ = handle.join();
                }
                log::info!("thread gone!");
            }
        }
    }

    /// Get the number of execution threads
    pub fn thread_count(&self) -> usize {
        self.execution_threads.lock().unwrap()
            .iter()
            .filter(|t| t.is_some())
            .count()
    }
}

impl Drop for PinnedExecutor {
    fn drop(&mut self) {
        let mut threads = self.execution_threads.lock().unwrap();

        // Send shutdown to all threads
        for thread_opt in threads.iter_mut() {
            if let Some(thread) = thread_opt {
                let _ = thread.sender.send(Job::Shutdown);
            }
        }

        // Join all threads
        for thread_opt in threads.iter_mut() {
            if let Some(mut thread) = thread_opt.take() {
                if let Some(handle) = thread.handle.take() {
                    let _ = handle.join();
                }
            }
        }
    }
}

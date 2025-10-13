# Pinned Thread Pool Migration Plan

## Problem Statement

The current async runtime causes memory fragmentation because plugins constantly move between threads during execution. When memory is allocated on one thread and freed on another, allocators cannot efficiently reclaim that memory, leading to fragmentation.

### Current Issues
- Async tasks for plugin operations can run on any thread
- Plugin allocations happen on random threads
- Plugin deallocations happen on different threads
- 99% of plugin operations are blocking (WASM compilation/execution)
- Memory fragmentation grows over time, especially with plugin load/unload cycles

## Solution: Pinned Thread Pool + Tokio

Use a **fixed-size pool of threads** where each plugin is **pinned to a specific thread** via consistent hashing (plugin_id % pool_size). This ensures all operations for a given plugin happen on the same thread.

### Architecture

```
┌─────────────────────────────────────────┐
│         Main Plugin Thread              │
│  (receives PluginInstruction messages)  │
└──────────────┬──────────────────────────┘
               │
               ├─> Tokio Runtime (2-4 threads)
               │   └─> Downloads (I/O bound, async)
               │
               └─> PinnedExecutor (8-16 threads)
                   ├─> Thread 0: Plugins 0, 8, 16, 24...
                   ├─> Thread 1: Plugins 1, 9, 17, 25...
                   ├─> Thread 2: Plugins 2, 10, 18, 26...
                   └─> ...

Each plugin lifecycle:
  Allocate → Compile → Execute → Deallocate
  ALL on the same pinned thread!
```

### Key Benefits

1. **Eliminates cross-thread fragmentation**: Allocation and deallocation on same thread
2. **Bounded resource usage**: Cap at 8-16 threads regardless of plugin count
3. **Better memory locality**: Plugin data stays on same CPU cache
4. **Simpler than async**: No work-stealing, predictable execution
5. **Hybrid approach**: Async for I/O (downloads), blocking threads for compute (WASM)

## Core Components

### 1. PinnedExecutor

A simple thread pool with deterministic job routing.

```rust
use std::sync::mpsc::{channel, Sender};
use std::thread::{self, JoinHandle};

pub struct PinnedExecutor {
    execution_threads: Vec<ExecutionThread>,
}

struct ExecutionThread {
    sender: Sender<Job>,
    handle: JoinHandle<()>,
}

type Job = Box<dyn FnOnce() + Send>;

impl PinnedExecutor {
    pub fn new(size: usize) -> Self {
        let execution_threads = (0..size)
            .map(|thread_idx| {
                let (sender, receiver) = channel::<Job>();

                let handle = thread::Builder::new()
                    .name(format!("plugin-exec-{}", thread_idx))
                    .spawn(move || {
                        while let Ok(job) = receiver.recv() {
                            job();
                        }
                    })
                    .expect("Failed to spawn execution thread");

                ExecutionThread { sender, handle }
            })
            .collect();

        PinnedExecutor { execution_threads }
    }

    /// Execute job pinned to plugin's thread
    pub fn execute_for_plugin<F>(&self, plugin_id: u32, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let thread_idx = plugin_id as usize % self.execution_threads.len();
        let _ = self.execution_threads[thread_idx]
            .sender
            .send(Box::new(f));
    }
}

impl Drop for PinnedExecutor {
    fn drop(&mut self) {
        // Channels close when senders are dropped
        // Threads exit their recv loop
    }
}
```

### 2. Tokio Runtime Setup

Since we don't use `#[tokio::main]`, we need a global runtime:

```rust
use tokio::runtime::Runtime;
use once_cell::sync::OnceCell;

static TOKIO_RUNTIME: OnceCell<Runtime> = OnceCell::new();

pub fn get_tokio_runtime() -> &'static Runtime {
    TOKIO_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2) // Small pool for I/O
            .thread_name("tokio-runtime")
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime")
    })
}

// Alternative: Use Handle pattern
static TOKIO_HANDLE: OnceCell<tokio::runtime::Handle> = OnceCell::new();

pub fn get_tokio_handle() -> tokio::runtime::Handle {
    TOKIO_HANDLE.get().expect("Tokio not initialized").clone()
}
```

### 3. WasmBridge Integration

```rust
pub struct WasmBridge {
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
    senders: ThreadSenders,
    engine: Engine,
    plugin_dir: PathBuf,
    plugin_map: Arc<Mutex<PluginMap>>,

    // NEW: Pinned executor for blocking plugin operations
    plugin_executor: Arc<PinnedExecutor>,

    // CHANGED: No JoinHandles, just track loading plugins
    loading_plugins: HashSet<(PluginId, RunPlugin)>,

    // ... rest of fields ...
}

impl WasmBridge {
    pub fn new(...) -> Self {
        let executor_size = num_cpus::get().max(4).min(16);
        let plugin_executor = Arc::new(PinnedExecutor::new(executor_size));

        WasmBridge {
            // ... existing fields ...
            plugin_executor,
            loading_plugins: HashSet::new(),
        }
    }
}
```

## Implementation Patterns

### Plugin Loading

**Pattern**: Tokio async for download → Pinned executor for compilation/execution

```rust
pub fn load_plugin(&mut self, ...) -> Result<(PluginId, ClientId)> {
    let plugin_id = self.next_plugin_id;

    self.cached_events_for_pending_plugins.insert(plugin_id, vec![]);
    self.cached_resizes_for_pending_plugins.insert(plugin_id, (size.rows, size.cols));
    self.loading_plugins.insert((plugin_id, run.clone()));

    let plugin_executor = self.plugin_executor.clone();
    let senders = self.senders.clone();
    // ... other clones ...

    // Spawn async task for I/O
    get_tokio_handle().spawn(async move {
        let _ = senders.send_to_background_jobs(
            BackgroundJob::AnimatePluginLoading(plugin_id),
        );
        let mut loading_indication = LoadingIndication::new(plugin_name.clone());

        // ASYNC I/O: Download on tokio runtime
        if let RunPluginLocation::Remote(url) = &plugin.location {
            let file_name: String = PortableHash::default()
                .hash128(url.as_bytes())
                .iter()
                .map(ToString::to_string)
                .collect();

            match downloader.download(url, Some(&file_name)).await {
                Ok(_) => plugin.path = ZELLIJ_CACHE_DIR.join(&file_name),
                Err(e) => {
                    handle_plugin_loading_failure(&senders, plugin_id, &mut loading_indication, e, cli_client_id);
                    return;
                }
            }
        }

        // BLOCKING WORK: Hand off to pinned executor
        plugin_executor.execute_for_plugin(plugin_id, move || {
            log::info!("Compiling plugin on pinned thread");
            match PluginLoader::start_plugin(
                plugin_id,
                client_id,
                &plugin,
                tab_index,
                plugin_dir,
                senders.clone(),
                engine,
                plugin_map.clone(),
                size,
                connected_clients.clone(),
                &mut loading_indication,
                path_to_default_shell,
                zellij_cwd.clone(),
                capabilities,
                client_attributes,
                default_shell,
                default_layout,
                skip_cache,
                layout_dir,
                default_mode,
                keybinds,
            ) {
                Ok(_) => {
                    let plugin_list = plugin_map.lock().unwrap().list_plugins();
                    handle_plugin_successful_loading(&senders, plugin_id, plugin_list);
                },
                Err(e) => handle_plugin_loading_failure(
                    &senders,
                    plugin_id,
                    &mut loading_indication,
                    e,
                    cli_client_id,
                ),
            }

            let _ = senders.send_to_plugin(PluginInstruction::ApplyCachedEvents {
                plugin_ids: vec![plugin_id],
                done_receiving_permissions: false,
            });
        });
    });

    self.next_plugin_id += 1;
    Ok((plugin_id, client_id))
}
```

### Plugin Operations (Update, Resize, etc.)

**Pattern**: Execute all operations on plugin's pinned thread

```rust
pub fn resize_plugin(&mut self, pid: PluginId, new_columns: usize, new_rows: usize, shutdown_sender: Sender<()>) -> Result<()> {
    let plugins_to_resize: Vec<_> = /* ... */;

    for (plugin_id, client_id, running_plugin) in plugins_to_resize {
        if plugin_id == pid {
            let event_id = running_plugin.lock().unwrap().next_event_id(AtomicEvent::Resize);

            // Execute on plugin's pinned thread
            self.plugin_executor.execute_for_plugin(plugin_id, {
                let senders = self.senders.clone();
                let running_plugin = running_plugin.clone();
                let _s = shutdown_sender.clone();
                move || {
                    let mut running_plugin = running_plugin.lock().unwrap();
                    let _s = _s;

                    if running_plugin.apply_event_id(AtomicEvent::Resize, event_id) {
                        // ... resize logic ...
                    }
                }
            });
        }
    }
    Ok(())
}

pub fn update_plugins(&mut self, updates: Vec<(Option<PluginId>, Option<ClientId>, Event)>, shutdown_sender: Sender<()>) -> Result<()> {
    let plugins_to_update: Vec<_> = /* ... */;

    // Group updates by plugin_id
    let mut updates_by_plugin: HashMap<PluginId, Vec<_>> = HashMap::new();
    for (pid, cid, event) in updates {
        for (plugin_id, client_id, running_plugin, subscriptions) in &plugins_to_update {
            if Self::message_is_directed_at_plugin(pid, cid, plugin_id, client_id) {
                updates_by_plugin
                    .entry(*plugin_id)
                    .or_default()
                    .push((event.clone(), running_plugin.clone(), subscriptions.clone(), *client_id));
            }
        }
    }

    // Execute each plugin's updates on its pinned thread
    for (plugin_id, plugin_updates) in updates_by_plugin {
        self.plugin_executor.execute_for_plugin(plugin_id, {
            let senders = self.senders.clone();
            let _s = shutdown_sender.clone();
            move || {
                let _s = _s;
                for (event, running_plugin, subscriptions, client_id) in plugin_updates {
                    // ... event handling ...
                }
            }
        });
    }

    Ok(())
}
```

### Plugin Cleanup (CRITICAL)

**Pattern**: Cleanup must happen on plugin's pinned thread to avoid cross-thread deallocation

```rust
pub fn unload_plugin(&mut self, pid: PluginId) -> Result<()> {
    info!("Bye from plugin {}", &pid);

    // Remove from plugin_map on main thread
    let plugins_to_cleanup: Vec<_> = {
        let mut plugin_map = self.plugin_map.lock().unwrap();
        plugin_map.remove_plugins(pid).into_iter().collect()
    };

    // Schedule cleanup on each plugin's pinned thread
    for ((plugin_id, client_id), (running_plugin, subscriptions, workers)) in plugins_to_cleanup {
        // Send worker exit messages
        for (_worker_name, worker_sender) in workers {
            drop(worker_sender.send(MessageToWorker::Exit));
        }

        let senders = self.senders.clone();

        // CRITICAL: Execute cleanup on plugin's pinned thread
        self.plugin_executor.execute_for_plugin(plugin_id, move || {
            let needs_before_close = subscriptions
                .lock()
                .unwrap()
                .contains(&EventType::BeforeClose);

            if needs_before_close {
                let mut rp = running_plugin.lock().unwrap();
                let _ = apply_before_close_event_to_plugin(
                    plugin_id,
                    client_id,
                    &mut rp,
                    senders.clone(),
                );
                let cache_dir = rp.store.data().plugin_own_data_dir.clone();
                drop(rp);
                let _ = std::fs::remove_dir_all(&cache_dir);
            } else {
                let cache_dir = running_plugin
                    .lock()
                    .unwrap()
                    .store
                    .data()
                    .plugin_own_data_dir
                    .clone();
                let _ = std::fs::remove_dir_all(&cache_dir);
            }

            // CRITICAL: Deallocate on same thread as allocation
            drop(running_plugin);
            drop(subscriptions);
        });
    }

    // Main thread cleanup
    self.cached_plugin_map.clear();
    let mut pipes_to_unblock = self.pending_pipes.unload_plugin(&pid);
    for pipe_name in pipes_to_unblock.drain(..) {
        let _ = self.senders
            .send_to_server(ServerInstruction::UnblockCliPipeInput(pipe_name));
    }
    let plugin_list = self.plugin_map.lock().unwrap().list_plugins();
    let _ = self.senders
        .send_to_background_jobs(BackgroundJob::ReportPluginList(plugin_list));

    Ok(())
}
```

## Migration Checklist

### Phase 1: Setup
- [ ] Create `pinned_executor.rs` module
- [ ] Set up tokio runtime in `OnceCell`
- [ ] Add `plugin_executor: Arc<PinnedExecutor>` to `WasmBridge`
- [ ] Change `loading_plugins` from `HashMap<..., JoinHandle>` to `HashSet<...>`

### Phase 2: Core Operations
- [ ] Convert `load_plugin` to tokio + pinned executor
- [ ] Convert `reload_plugin` to pinned executor
- [ ] Convert `resize_plugin` to pinned executor
- [ ] Convert `update_plugins` to pinned executor
- [ ] Convert `pipe_messages` to pinned executor
- [ ] Convert `apply_cached_events_and_resizes_for_plugin` to pinned executor
- [ ] Convert `change_plugin_host_dir` to pinned executor
- [ ] Convert `reconfigure` to pinned executor

### Phase 3: Cleanup
- [ ] Update `unload_plugin` with pinned cleanup pattern
- [ ] Update `cleanup` to clear loading set (no handles to cancel)
- [ ] Remove all `task::spawn` calls
- [ ] Remove `async_std` dependency

### Phase 4: Testing
- [ ] Test plugin load/unload cycles
- [ ] Test multiple plugins loading simultaneously
- [ ] Test plugin operations (resize, update, etc.)
- [ ] Monitor memory usage over time
- [ ] Verify no memory fragmentation

## Code Pattern Reference

### Before (async_std):
```rust
task::spawn({
    let senders = self.senders.clone();
    let running_plugin = running_plugin.clone();
    async move {
        let mut running_plugin = running_plugin.lock().unwrap();
        // ... plugin work ...
    }
});
```

### After (pinned executor):
```rust
self.plugin_executor.execute_for_plugin(plugin_id, {
    let senders = self.senders.clone();
    let running_plugin = running_plugin.clone();
    move || {
        let mut running_plugin = running_plugin.lock().unwrap();
        // ... plugin work ...
    }
});
```

## Performance Characteristics

### Resource Usage
- **Plugin threads**: 8-16 (bounded by CPU count)
- **Tokio threads**: 2-4 (for I/O)
- **Total**: ~10-20 threads regardless of plugin count

### Typical Plugin Count
- Normal: 10 plugins → ~1-2 plugins per thread
- High: 20 plugins → ~2-3 plugins per thread
- Extreme: 100 plugins → ~6-13 plugins per thread

### Memory Benefits
- **Before**: Cross-thread allocation/deallocation on every operation
- **After**: Cross-thread deallocation only on plugin load/unload (rare)
- **Reduction**: ~98% decrease in fragmentation-causing operations

## Considerations

### Shared Arc<Mutex<...>> Structures
The `RunningPlugin` is stored as `Arc<Mutex<RunningPlugin>>`. While the Arc itself can be cloned to any thread, the cleanup pattern ensures the final deallocation happens on the pinned thread, avoiding fragmentation.

### No Cancellation
Unlike async tasks with JoinHandles, we can't cancel jobs once submitted to the pinned executor. This is acceptable because:
1. Plugin operations are already blocking and need to complete
2. We track loading state in `HashSet` instead
3. Cleanup on shutdown just waits for jobs to complete naturally

### Thread Starvation
If one plugin blocks indefinitely, it only affects other plugins on the same thread (1/8th to 1/16th of plugins). This is better than blocking the entire async runtime.

## Future Optimizations

1. **Load balancing**: Track job queue depth per thread and route to least loaded
2. **Thread priority**: Give plugin threads higher priority than I/O threads
3. **Per-plugin metrics**: Track time spent per plugin for profiling
4. **Graceful shutdown**: Signal threads to finish current job and exit
5. **Dynamic sizing**: Adjust thread pool size based on plugin count

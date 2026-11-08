//! Regression tests for the plugin hardening review fixes. None of them needs a built plugin.

use super::*;
use crate::plugins::plugin_map::PluginMetadata;
use std::time::{Duration, Instant};
use tempfile::{tempdir, TempDir};
use zellij_utils::channels::{self, Receiver, SenderWithContext};
use zellij_utils::input::layout::RunPlugin;
use zellij_utils::input::plugins::PluginConfig;
use zellij_utils::pane_size::Size;

type TestBridge = (
    WasmBridge,
    TempDir,
    Receiver<(ScreenInstruction, zellij_utils::errors::ErrorContext)>,
    Receiver<(PluginInstruction, zellij_utils::errors::ErrorContext)>,
);

fn test_bridge() -> TestBridge {
    let (to_plugin, plugin_receiver) = channels::unbounded();
    let (to_screen, screen_receiver) = channels::unbounded();
    let senders = ThreadSenders {
        to_plugin: Some(SenderWithContext::new(to_plugin)),
        to_screen: Some(SenderWithContext::new(to_screen)),
        should_silently_fail: true,
        ..Default::default()
    };
    let plugin_dir = tempdir().unwrap();
    let mut config = wasmi::Config::default();
    config.set_max_stack_height(1024 * 1024);
    config.set_max_recursion_depth(1000);
    let bridge = WasmBridge::new(
        senders,
        wasmi::Engine::new(&config),
        plugin_dir.path().to_path_buf(),
        "/bin/sh".into(),
        plugin_dir.path().to_path_buf(),
        Default::default(),
        None,
        None,
        vec![],
        vec![],
        Default::default(),
        Default::default(),
    );
    (bridge, plugin_dir, screen_receiver, plugin_receiver)
}

fn metadata_of(run: &RunPlugin) -> PluginMetadata {
    PluginMetadata {
        plugin_config: PluginConfig::from_run_plugin(run).unwrap(),
        tab_index: None,
        rows: 5,
        columns: 80,
        cwd: PathBuf::from("/"),
        is_background: false,
    }
}

/// Register a shared instance with one pane slot without loading any module.
fn register_fake_shared_instance(
    bridge: &mut WasmBridge,
    run: &RunPlugin,
    instance_id: PluginId,
    slot_id: PluginId,
) {
    let loading_context = LoadingContext::new(
        bridge,
        None,
        PluginConfig::from_run_plugin(run).unwrap(),
        instance_id,
        1,
        None,
        Size { rows: 5, cols: 80 },
    );
    let key = WasmBridge::shared_key_of(run);
    bridge.shared_instances.insert(
        instance_id,
        SharedInstanceInfo {
            key: key.clone(),
            loading_context,
            slots: BTreeSet::from([slot_id]),
            background_slots: BTreeSet::new(),
        },
    );
    bridge.shared_keys.insert(key, instance_id);
    bridge.shared_slot_owner.insert(slot_id, instance_id);
    bridge
        .plugin_map
        .lock()
        .unwrap()
        .insert_metadata(slot_id, metadata_of(run));
}

fn released_intercepts(
    screen_receiver: &Receiver<(ScreenInstruction, zellij_utils::errors::ErrorContext)>,
) -> Vec<(PluginId, ClientId)> {
    let mut released = vec![];
    while let Ok((instruction, _)) = screen_receiver.try_recv() {
        if let ScreenInstruction::ClearKeyPressesIntercepts(plugin_id, client_id) = instruction {
            released.push((plugin_id, client_id));
        }
    }
    released.sort();
    released
}

fn cli_pipe(pipe_id: &str) -> PipeMessage {
    PipeMessage::new(
        PipeSource::Cli(pipe_id.to_owned()),
        "name",
        &Some("payload".to_owned()),
        &None,
        false,
    )
}

fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "condition was not met before the test deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn block_executor(executor: &Arc<PinnedExecutor>, owner: PluginId) -> std::sync::mpsc::Sender<()> {
    use std::sync::mpsc::channel;
    let (started_tx, started_rx) = channel();
    let (release_tx, release_rx) = channel();
    let release_rx = Arc::new(Mutex::new(release_rx));
    executor
        .try_execute_for_plugin(owner, JobClass::Ordered, 0, move |_s, _p, _c, _ca, _e| {
            started_tx.send(()).unwrap();
            let _ = release_rx.lock().unwrap().recv();
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    release_tx
}

#[test]
fn adjacent_client_specific_replaceable_jobs_both_run() {
    let (bridge, _dir, _screen_receiver, _plugin_receiver) = test_bridge();
    let executor = bridge.plugin_executor.clone();
    executor.register_plugin(1);
    let release = block_executor(&executor, 1);

    let executed = Arc::new(Mutex::new(Vec::new()));
    for client_id in [11, 12] {
        let executed = executed.clone();
        executor
            .try_execute_for_plugin(
                1,
                JobClass::Replaceable(replaceable_host_dir_key(client_id, 0)),
                PathBuf::from("/tmp").as_os_str().len(),
                move |_s, _p, _c, _ca, _e| executed.lock().unwrap().push(client_id),
            )
            .unwrap();
    }

    release.send(()).unwrap();
    wait_until(|| executor.queued_jobs() == 0);
    assert_eq!(*executed.lock().unwrap(), vec![11, 12]);
}

#[test]
fn replaceable_keys_include_destination_and_generation() {
    assert_ne!(
        replaceable_host_dir_key(11, 0),
        replaceable_host_dir_key(12, 0)
    );
    assert_ne!(
        replaceable_host_dir_key(11, 1),
        replaceable_host_dir_key(11, 2)
    );
    assert_ne!(
        replaceable_plugin_settings_key(11, 0),
        replaceable_host_dir_key(11, 0)
    );
}

#[test]
fn client_specific_host_dir_changes_reach_both_instances() {
    let (mut bridge, dir, _screen_receiver, plugin_receiver) = test_bridge();
    let run = write_noop_plugin(dir.path(), "host-dir.wasm");
    let plugin_id = load_noop_plugin(&mut bridge, &plugin_receiver, run);

    bridge.add_client(2).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let (instruction, _) = plugin_receiver.recv_deadline(deadline).unwrap();
        if let PluginInstruction::ApplyCachedEvents { plugin_ids, .. } = instruction {
            if plugin_ids.contains(&plugin_id)
                && bridge.plugin_map.lock().unwrap().contains(plugin_id, 2)
            {
                break;
            }
        }
    }

    let client_one_dir = dir.path().join("client-one");
    let client_two_dir = dir.path().join("client-two");
    std::fs::create_dir_all(&client_one_dir).unwrap();
    std::fs::create_dir_all(&client_two_dir).unwrap();

    let executor = bridge.plugin_executor.clone();
    let release = block_executor(&executor, plugin_id);
    bridge
        .change_plugin_host_dir(client_one_dir.clone(), plugin_id, 1)
        .unwrap();
    bridge
        .change_plugin_host_dir(client_two_dir.clone(), plugin_id, 2)
        .unwrap();
    release.send(()).unwrap();

    let cwd_for = |client_id| {
        bridge
            .plugin_map
            .lock()
            .unwrap()
            .get_running_plugin(plugin_id, Some(client_id))
            .map(|plugin| plugin.lock().unwrap().store.data().plugin_cwd.clone())
    };
    wait_until(|| {
        cwd_for(1).as_ref() == Some(&client_one_dir) && cwd_for(2).as_ref() == Some(&client_two_dir)
    });
}

#[test]
fn pending_permission_cache_is_bounded_by_bytes_and_entries() {
    let (mut bridge, _dir, _screen_receiver, _plugin_receiver) = test_bridge();
    let plugin_id = 7;
    bridge
        .cached_events_for_pending_plugins
        .insert_empty(plugin_id);

    let oversized = Event::PastedText("x".repeat(MAX_PENDING_CACHE_BYTES_PER_PLUGIN + 1));
    assert!(matches!(
        bridge.cached_events_for_pending_plugins.try_push(
            plugin_id,
            EventOrPipeMessage::Event(oversized, None),
            false,
        ),
        Err(PendingCacheRejection::ByteLimit)
    ));

    for _ in 0..MAX_PENDING_CACHE_ENTRIES_PER_PLUGIN {
        bridge
            .cached_events_for_pending_plugins
            .try_push(
                plugin_id,
                EventOrPipeMessage::Event(Event::PastedText("x".to_owned()), None),
                false,
            )
            .unwrap();
    }
    assert!(matches!(
        bridge.cached_events_for_pending_plugins.try_push(
            plugin_id,
            EventOrPipeMessage::Event(Event::PastedText("x".to_owned()), None),
            false,
        ),
        Err(PendingCacheRejection::EntryLimit)
    ));
}

#[test]
fn a_rejected_load_admission_rolls_back_pending_state() {
    let (mut bridge, dir, _screen_receiver, _plugin_receiver) = test_bridge();
    let plugin_cache: PluginCache = Arc::new(Mutex::new(HashMap::new()));
    bridge.plugin_executor = Arc::new(PinnedExecutor::new_with_tiny_limits_for_test(
        1,
        &bridge.senders,
        &bridge.plugin_map,
        &bridge.connected_clients,
        &plugin_cache,
        &wasmi::Engine::default(),
    ));
    let executor = bridge.plugin_executor.clone();
    executor.register_plugin(999);
    let release = block_executor(&executor, 999);

    let run = write_noop_plugin(dir.path(), "rejected.wasm");
    let plugin_id = bridge.next_plugin_id;
    bridge
        .load_plugin_with_kind(
            &Some(run),
            None,
            Size { rows: 5, cols: 80 },
            None,
            false,
            Some(1),
            true,
        )
        .expect_err("a rejected admission must not report a successful load");

    assert!(!executor.is_registered(plugin_id));
    assert!(!bridge
        .cached_events_for_pending_plugins
        .contains(&plugin_id));
    assert!(!bridge
        .cached_resizes_for_pending_plugins
        .contains_key(&plugin_id));
    assert!(!bridge.plugins_with_unfinished_load.contains(&plugin_id));
    assert!(!bridge
        .loading_plugins
        .iter()
        .any(|(loading_plugin_id, _)| *loading_plugin_id == plugin_id));

    release.send(()).unwrap();
    wait_until(|| executor.queued_jobs() == 0);
}

#[test]
fn quarantining_a_plugin_releases_its_key_intercept_for_every_client() {
    let (mut bridge, _dir, screen_receiver, _plugin_receiver) = test_bridge();
    let run = RunPlugin::from_url("file:/nonexistent/plugin.wasm").unwrap();
    bridge
        .plugin_map
        .lock()
        .unwrap()
        .insert_metadata(7, metadata_of(&run));
    bridge.connected_clients.lock().unwrap().extend([1, 2]);

    bridge.quarantine_plugin(7, 100);

    assert!(bridge.quarantine.contains(&7));
    assert_eq!(released_intercepts(&screen_receiver), vec![(7, 1), (7, 2)]);
    // a repeated report changes nothing
    bridge.quarantine_plugin(7, 100);
    assert!(released_intercepts(&screen_receiver).is_empty());
}

#[test]
fn a_fatal_error_in_a_shared_slot_quarantines_the_whole_instance() {
    let (mut bridge, _dir, screen_receiver, _plugin_receiver) = test_bridge();
    let run = RunPlugin::from_url("file:/nonexistent/shared.wasm").unwrap();
    register_fake_shared_instance(&mut bridge, &run, 10, 11);
    let key = WasmBridge::shared_key_of(&run);
    assert_eq!(bridge.shared_keys.get(&key), Some(&10));
    bridge.connected_clients.lock().unwrap().push(1);

    bridge.quarantine_plugin(11, 100);

    assert!(bridge.quarantine.contains(&11));
    assert!(bridge.quarantine.contains(&10));
    assert!(
        !bridge.shared_keys.contains_key(&key),
        "a quarantined instance must not receive new slots"
    );
    // the intercept of a shared plugin is held under the instance id
    assert_eq!(
        released_intercepts(&screen_receiver),
        vec![(10, 1), (11, 1)]
    );
}

#[test]
fn an_error_reported_for_the_instance_alone_quarantines_it() {
    // a shared instance without pane slots (background only) is reported under its own id
    let (mut bridge, _dir, _screen_receiver, _plugin_receiver) = test_bridge();
    let run = RunPlugin::from_url("file:/nonexistent/shared.wasm").unwrap();
    register_fake_shared_instance(&mut bridge, &run, 10, 11);

    bridge.quarantine_plugin(10, 100);

    assert!(bridge.quarantine.contains(&10));
}

#[test]
fn a_failed_shared_load_removes_the_dead_instance_and_its_slots() {
    let (mut bridge, _dir, _screen_receiver, _plugin_receiver) = test_bridge();
    let run = RunPlugin::from_url("file:/nonexistent/shared.wasm").unwrap();
    register_fake_shared_instance(&mut bridge, &run, 10, 11);
    let key = WasmBridge::shared_key_of(&run);

    bridge.shared_instance_load_failed(10);

    assert!(!bridge.shared_instances.contains_key(&10));
    assert!(!bridge.shared_keys.contains_key(&key));
    assert!(!bridge.shared_slot_owner.contains_key(&11));
    assert!(bridge.plugin_map.lock().unwrap().metadata(11).is_none());
}

#[test]
fn a_failed_shared_reload_restores_the_previous_instance_and_keeps_its_data_dir() {
    let (bridge, dir, _screen_receiver, _plugin_receiver) = test_bridge();
    let run = write_noop_plugin(dir.path(), "shared.wasm");
    let mut loading_context = LoadingContext::new(
        &bridge,
        None,
        PluginConfig::from_run_plugin(&run).unwrap(),
        10,
        1,
        None,
        Size { rows: 5, cols: 80 },
    );
    loading_context.plugin_own_data_dir = dir.path().join("shared-data");
    loading_context.plugin_own_cache_dir = dir.path().join("shared-cache");
    let plugin_cache: PluginCache = Arc::new(Mutex::new(HashMap::new()));
    let engine = wasmi::Engine::default();

    assert!(start_instance_job(
        bridge.senders.clone(),
        bridge.plugin_map.clone(),
        bridge.connected_clients.clone(),
        plugin_cache.clone(),
        engine.clone(),
        loading_context.clone(),
        false,
        10,
        BTreeMap::new(),
    ));
    let previous = bridge
        .plugin_map
        .lock()
        .unwrap()
        .shared_running_plugin(10)
        .unwrap()
        .0;

    let mut failing_context = loading_context.clone();
    failing_context.generation = next_plugin_generation();
    failing_context.wasm_bytes = Some(
        wat::parse_str(r#"(module (func (export "_start") unreachable) (func (export "load")))"#)
            .unwrap(),
    );
    assert!(!start_instance_job(
        bridge.senders.clone(),
        bridge.plugin_map.clone(),
        bridge.connected_clients.clone(),
        plugin_cache,
        engine,
        failing_context,
        true,
        10,
        BTreeMap::new(),
    ));

    let current = bridge
        .plugin_map
        .lock()
        .unwrap()
        .shared_running_plugin(10)
        .unwrap()
        .0;
    assert!(
        Arc::ptr_eq(&previous, &current),
        "a failed reload must leave the previous instance running"
    );
    assert!(loading_context.plugin_own_data_dir.exists());
}

#[test]
fn cached_cli_pipes_are_released_when_the_plugin_load_failed() {
    let (mut bridge, _dir, _screen_receiver, _plugin_receiver) = test_bridge();
    bridge
        .pending_pipes
        .mark_being_processed("failed-load", &7, &1);
    bridge
        .cached_events_for_pending_plugins
        .insert_entries_for_test(
            7,
            vec![EventOrPipeMessage::PipeMessage(
                cli_pipe("failed-load"),
                Some(1),
            )],
        );
    let (shutdown_sender, _shutdown_receiver) = tokio::sync::mpsc::channel(1);

    bridge
        .apply_cached_events_and_resizes_for_plugin(7, shutdown_sender)
        .unwrap();

    assert!(bridge.pending_pipes.is_finished("failed-load"));
    assert!(!bridge.pending_pipes.is_pending("failed-load"));
}

#[test]
fn a_failed_load_for_a_new_client_clears_its_pending_state() {
    let (mut bridge, dir, _screen_receiver, plugin_receiver) = test_bridge();
    let run = write_noop_plugin(dir.path(), "client-plugin.wasm");
    let plugin_id = load_noop_plugin(&mut bridge, &plugin_receiver, run);
    std::fs::write(dir.path().join("client-plugin.wasm"), b"not a wasm module").unwrap();

    bridge.add_client(2).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let apply = loop {
        let (instruction, _) = plugin_receiver
            .recv_deadline(deadline)
            .expect("the failed load must report its cleanup");
        if let PluginInstruction::ApplyCachedEvents {
            plugin_ids,
            done_receiving_permissions,
        } = instruction
        {
            if plugin_ids.contains(&plugin_id) {
                break (plugin_ids, done_receiving_permissions);
            }
        }
    };
    let (shutdown_sender, _shutdown_receiver) = tokio::sync::mpsc::channel(1);
    bridge
        .apply_cached_events(apply.0, apply.1, shutdown_sender)
        .unwrap();

    assert!(!bridge
        .cached_events_for_pending_plugins
        .contains(&plugin_id));
    assert!(!bridge
        .cached_resizes_for_pending_plugins
        .contains_key(&plugin_id));
    assert!(!bridge.plugins_with_unfinished_load.contains(&plugin_id));
    assert!(!bridge
        .loading_plugins
        .iter()
        .any(|(loading_plugin_id, _)| *loading_plugin_id == plugin_id));
}

#[test]
fn shared_pipes_are_not_dispatched_to_a_quarantined_instance() {
    let (mut bridge, _dir, _screen_receiver, _plugin_receiver) = test_bridge();
    let run = RunPlugin::from_url("file:/nonexistent/shared.wasm").unwrap();
    register_fake_shared_instance(&mut bridge, &run, 10, 11);
    let (shutdown_sender, _shutdown_receiver) = tokio::sync::mpsc::channel(1);

    let mut notification_end = None;
    let legacy = bridge.dispatch_shared_pipes(
        vec![(Some(11), Some(1), cli_pipe("before"))],
        shutdown_sender.clone(),
        &mut notification_end,
    );
    assert!(legacy.is_empty());
    // The fake instance has no executor assignment. Admission fails explicitly and
    // releases the caller instead of leaving the pipe pending forever.
    assert!(!bridge.pending_pipes.is_pending("before"));
    assert!(bridge.pending_pipes.is_finished("before"));

    bridge.quarantine_plugin(11, 100);
    let legacy = bridge.dispatch_shared_pipes(
        vec![
            (Some(11), Some(1), cli_pipe("addressed")),
            (None, Some(1), cli_pipe("broadcast")),
        ],
        shutdown_sender.clone(),
        &mut notification_end,
    );
    assert!(!bridge.pending_pipes.is_pending("addressed"));
    assert!(bridge.pending_pipes.is_finished("addressed"));
    assert!(!bridge.pending_pipes.is_pending("broadcast"));
    // the broadcast still goes to plugins that are not shared
    assert_eq!(legacy.len(), 1);
    assert_eq!(legacy[0].0, None);

    // reloading the instance releases it
    bridge.quarantine.begin_generation(10, 101);
    bridge.quarantine.begin_generation(11, 101);
    bridge.dispatch_shared_pipes(
        vec![(Some(11), Some(1), cli_pipe("after-reload"))],
        shutdown_sender,
        &mut notification_end,
    );
    assert!(!bridge.pending_pipes.is_pending("after-reload"));
    assert!(bridge.pending_pipes.is_finished("after-reload"));
}

#[test]
fn worker_messages_are_dropped_for_a_quarantined_plugin() {
    let (mut bridge, _dir, _screen_receiver, _plugin_receiver) = test_bridge();
    bridge.quarantine.report_fatal(7, 1);
    bridge
        .post_messages_to_plugin_worker(
            7,
            1,
            "worker".to_owned(),
            vec![("message".to_owned(), "payload".to_owned())],
        )
        .unwrap();
    assert!(bridge.cached_worker_messages.is_empty());
}

#[test]
fn quarantine_marks_the_running_instance_for_already_queued_callbacks() {
    let (mut bridge, dir, _screen_receiver, plugin_receiver) = test_bridge();
    let run = write_noop_plugin(dir.path(), "plugin.wasm");
    let plugin_id = load_noop_plugin(&mut bridge, &plugin_receiver, run);
    let (generation, _) = running_module_identity(&bridge, plugin_id);

    bridge.quarantine_plugin(plugin_id, generation);

    let running_plugin = bridge
        .plugin_map
        .lock()
        .unwrap()
        .get_running_plugin(plugin_id, Some(1))
        .unwrap();
    assert!(running_plugin.lock().unwrap().is_quarantined());
}

#[test]
fn an_existing_shared_instance_is_found_without_reading_its_module() {
    let (mut bridge, _dir, _screen_receiver, _plugin_receiver) = test_bridge();
    // the file does not exist: classifying the module from its bytes would find nothing
    let run = RunPlugin::from_url("file:/nonexistent/shared.wasm").unwrap();
    register_fake_shared_instance(&mut bridge, &run, 10, 11);

    assert_eq!(
        bridge.existing_shared_targets(&run, false),
        Some(vec![(11, None)])
    );
    // a plugin that is not shared has no shared key, and is not looked up on disk either
    let other = RunPlugin::from_url("file:/nonexistent/other.wasm").unwrap();
    assert_eq!(bridge.existing_shared_targets(&other, false), None);
}

fn write_noop_plugin(dir: &std::path::Path, name: &str) -> RunPlugin {
    let path = dir.join(name);
    std::fs::write(
        &path,
        wat::parse_str(r#"(module (func (export "_start")) (func (export "load")))"#).unwrap(),
    )
    .unwrap();
    RunPlugin::from_url(&format!("file:{}", path.display())).unwrap()
}

/// Load a plugin that does nothing and wait until its load finished.
fn load_noop_plugin(
    bridge: &mut WasmBridge,
    plugin_receiver: &Receiver<(PluginInstruction, zellij_utils::errors::ErrorContext)>,
    run: RunPlugin,
) -> PluginId {
    let (plugin_id, _client_id) = bridge
        .load_plugin_with_kind(
            &Some(run),
            None,
            Size { rows: 5, cols: 80 },
            None,
            false,
            Some(1),
            true,
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match plugin_receiver.recv_deadline(deadline).unwrap().0 {
            PluginInstruction::ApplyCachedEvents { plugin_ids, .. }
                if plugin_ids.contains(&plugin_id) =>
            {
                return plugin_id
            },
            _ => {},
        }
    }
}

fn running_module_identity(bridge: &WasmBridge, plugin_id: PluginId) -> (u64, String) {
    let running_plugin = bridge
        .plugin_map
        .lock()
        .unwrap()
        .get_running_plugin(plugin_id, Some(1))
        .expect("plugin loaded");
    let running_plugin = running_plugin.lock().unwrap();
    let data = running_plugin.store.data();
    (data.generation, data.plugin_digest.clone())
}

#[test]
fn a_permission_answer_only_counts_for_the_module_that_was_prompted() {
    let (mut bridge, dir, _screen_receiver, plugin_receiver) = test_bridge();
    let run = write_noop_plugin(dir.path(), "plugin-a.wasm");
    let plugin_id = load_noop_plugin(&mut bridge, &plugin_receiver, run);
    let (generation, digest) = running_module_identity(&bridge, plugin_id);

    // nobody asked: an answer is not an answer to anything
    assert!(!bridge.permission_prompt_is_current(plugin_id, Some(1)));

    bridge.cache_plugin_events(plugin_id, generation, digest.clone());
    assert!(bridge.permission_prompt_is_current(plugin_id, Some(1)));
    bridge.clear_permission_prompt(plugin_id);
    assert!(
        !bridge.permission_prompt_is_current(plugin_id, Some(1)),
        "a consumed prompt must not accept a duplicate answer"
    );

    // a prompt that belonged to different bytes
    bridge.cache_plugin_events(plugin_id, generation, "another-digest".to_owned());
    assert!(!bridge.permission_prompt_is_current(plugin_id, Some(1)));

    // a prompt of an instance that has been reloaded since
    bridge.cache_plugin_events(plugin_id, generation, digest.clone());
    bridge
        .quarantine
        .begin_generation(plugin_id, generation + 1);
    assert!(!bridge.permission_prompt_is_current(plugin_id, Some(1)));
    // the reloaded instance asking itself is fine
    bridge.cache_plugin_events(plugin_id, generation + 1, digest);
    assert!(bridge.permission_prompt_is_current(plugin_id, Some(1)));

    // nothing is left behind once the plugin is gone
    bridge.unload_plugin(plugin_id).unwrap();
    assert!(!bridge.permission_prompt_is_current(plugin_id, Some(1)));
}

#[test]
fn a_pending_permission_request_does_not_hold_back_the_unload_of_a_plugin_unloaded_while_loading() {
    let (mut bridge, _dir, _screen_receiver, _plugin_receiver) = test_bridge();
    let (shutdown_sender, _shutdown_receiver) = tokio::sync::mpsc::channel(1);
    let plugin_id = 42;
    bridge
        .plugin_ids_waiting_for_permission_request
        .insert(plugin_id);
    bridge.plugins_to_unload_once_loaded.insert(plugin_id);

    bridge
        .apply_cached_events(vec![plugin_id], false, shutdown_sender)
        .unwrap();

    assert!(!bridge.plugins_to_unload_once_loaded.contains(&plugin_id));
    assert!(!bridge
        .plugin_ids_waiting_for_permission_request
        .contains(&plugin_id));
}

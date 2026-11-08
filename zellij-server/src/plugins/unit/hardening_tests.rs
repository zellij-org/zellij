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
    assert!(bridge.pending_pipes.is_pending("before"));

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
    assert!(bridge.pending_pipes.is_pending("after-reload"));
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

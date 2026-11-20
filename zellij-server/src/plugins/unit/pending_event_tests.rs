use super::WasmBridge;
use crate::plugins::plugin_map::{PluginEnv, PluginMetadata, RunningPlugin};
use crate::thread_bus::ThreadSenders;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tempfile::{tempdir, TempDir};
use zellij_utils::data::{Event, EventType, PermissionStatus};
use zellij_utils::input::layout::{RunPlugin, RunPluginLocation};
use zellij_utils::input::plugins::PluginConfig;

fn bridge() -> (TempDir, WasmBridge) {
    let dir = tempdir().unwrap();
    let senders = ThreadSenders {
        should_silently_fail: true,
        ..Default::default()
    };
    let bridge = WasmBridge::new(
        senders,
        wasmi::Engine::default(),
        dir.path().to_path_buf(),
        "/bin/sh".into(),
        dir.path().to_path_buf(),
        Default::default(),
        None,
        None,
        vec![],
        vec![],
        Default::default(),
        Default::default(),
    );
    (dir, bridge)
}

fn pending_plugin(bridge: &mut WasmBridge, id: u32) {
    let run = RunPlugin::from_url("zellij:status-bar").unwrap();
    bridge.loading_plugins.insert((id, run));
    bridge
        .cached_resizes_for_pending_plugins
        .insert(id, (24, 80));
    bridge.cache_plugin_events(id);
}

fn running_plugin(bridge: &mut WasmBridge, id: u32) -> Arc<Mutex<RunningPlugin>> {
    let subscriptions = Arc::new(Mutex::new(
        [EventType::CustomMessage, EventType::PermissionRequestResult]
            .into_iter()
            .collect(),
    ));
    let env = PluginEnv {
        plugin_id: id,
        client_id: 1,
        plugin: PluginConfig {
            path: "pending-events.wasm".into(),
            _allow_exec_host_cmd: false,
            location: RunPluginLocation::File("pending-events.wasm".into()),
            initial_userspace_configuration: Default::default(),
            initial_cwd: None,
        },
        permissions: Default::default(),
        senders: bridge.senders.clone(),
        wasi_ctx: wasmi_wasi::sync::WasiCtxBuilder::new().build(),
        plugin_own_data_dir: Default::default(),
        plugin_own_cache_dir: Default::default(),
        path_to_default_shell: Default::default(),
        default_shell: None,
        layout_dir: None,
        plugin_cwd: Default::default(),
        session_env_vars: Default::default(),
        input_pipes_to_unblock: Default::default(),
        input_pipes_to_block: Default::default(),
        default_mode: Default::default(),
        subscriptions: subscriptions.clone(),
        stdin_pipe: Default::default(),
        stdout_pipe: Default::default(),
        keybinds: Default::default(),
        intercepting_key_presses: false,
        pending_popup_size: None,
        store_limits: wasmi::StoreLimitsBuilder::new().build(),
        shared: None,
    };
    let engine = wasmi::Engine::default();
    let mut store = wasmi::Store::new(&engine, env);
    let module = wasmi::Module::new(
        &engine,
        r#"(module
            (global $updates (export "updates") (mut i32) (i32.const 0))
            (func (export "update") (result i32)
                global.get $updates
                i32.const 1
                i32.add
                global.set $updates
                i32.const 0)
            (func (export "render") (param i32 i32)))"#,
    )
    .unwrap();
    let instance = wasmi::Linker::new(&engine)
        .instantiate_and_start(&mut store, &module)
        .unwrap();
    let plugin = Arc::new(Mutex::new(RunningPlugin::new(store, instance, 24, 80)));
    bridge.plugin_executor.register_plugin(id);
    bridge.plugin_map.lock().unwrap().insert(
        id,
        1,
        plugin.clone(),
        subscriptions,
        Default::default(),
    );
    plugin
}

fn applied_events(bridge: &WasmBridge, id: u32, plugin: &Arc<Mutex<RunningPlugin>>) -> i32 {
    let (sender, receiver) = mpsc::channel();
    bridge
        .plugin_executor
        .execute_for_plugin(id, move |_, _, _, _, _| sender.send(()).unwrap());
    receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    let plugin = plugin.lock().unwrap();
    plugin
        .instance
        .get_global(&plugin.store, "updates")
        .unwrap()
        .get(&plugin.store)
        .i32()
        .unwrap()
}

#[test]
fn unloading_pending_plugin_stops_caching_broadcasts() {
    let (_dir, mut bridge) = bridge();
    pending_plugin(&mut bridge, 1);
    pending_plugin(&mut bridge, 2);
    bridge.cached_worker_messages.insert(
        1,
        vec![(1, "worker".into(), "message".into(), "payload".into())],
    );
    let (shutdown, _receiver) = tokio::sync::mpsc::channel(1);
    let update = (
        None,
        None,
        Event::CustomMessage("update".into(), "x".repeat(4096)),
    );
    bridge
        .update_plugins(vec![update.clone()], shutdown.clone())
        .unwrap();
    assert_eq!(bridge.cached_events_for_pending_plugins[&1].len(), 1);

    bridge.unload_plugin(1).unwrap();
    assert!(!bridge.cached_events_for_pending_plugins.contains_key(&1));
    assert!(!bridge.cached_resizes_for_pending_plugins.contains_key(&1));
    assert!(!bridge.cached_worker_messages.contains_key(&1));
    assert!(!bridge
        .plugin_ids_waiting_for_permission_request
        .contains(&1));
    assert!(!bridge.loading_plugins.iter().any(|(id, _)| *id == 1));

    for _ in 0..128 {
        bridge
            .update_plugins(vec![update.clone()], shutdown.clone())
            .unwrap();
    }
    assert!(!bridge.cached_events_for_pending_plugins.contains_key(&1));
    assert_eq!(bridge.cached_events_for_pending_plugins[&2].len(), 129);
    assert!(bridge
        .plugin_ids_waiting_for_permission_request
        .contains(&2));
}

#[test]
fn late_permission_request_does_not_recreate_unloaded_plugin_cache() {
    let (_dir, mut bridge) = bridge();
    pending_plugin(&mut bridge, 1);
    bridge.unload_plugin(1).unwrap();
    bridge.cache_plugin_events(1);
    bridge.cache_plugin_events(99);
    assert!(bridge.cached_events_for_pending_plugins.is_empty());
    assert!(bridge.plugin_ids_waiting_for_permission_request.is_empty());
}

#[test]
fn canceled_plugin_finishing_load_is_unloaded_for_either_permission_order() {
    for permission_reply_first in [false, true] {
        let (_dir, mut bridge) = bridge();
        pending_plugin(&mut bridge, 1);
        bridge.unload_plugin(1).unwrap();
        let (shutdown, _receiver) = tokio::sync::mpsc::channel(1);
        if permission_reply_first {
            bridge
                .apply_cached_events(vec![1], true, shutdown.clone())
                .unwrap();
            assert!(bridge.plugins_to_unload_once_loaded.contains(&1));
        }
        running_plugin(&mut bridge, 1);
        bridge.cache_plugin_events(1);
        assert!(!bridge.cached_events_for_pending_plugins.contains_key(&1));
        bridge
            .apply_cached_events(vec![1], false, shutdown.clone())
            .unwrap();
        assert!(bridge
            .plugin_map
            .lock()
            .unwrap()
            .get_running_plugin(1, None)
            .is_none());
        if !permission_reply_first {
            bridge.apply_cached_events(vec![1], true, shutdown).unwrap();
        }
        assert!(!bridge.plugins_to_unload_once_loaded.contains(&1));
        bridge.cache_plugin_events(1);
        assert!(!bridge.cached_events_for_pending_plugins.contains_key(&1));
    }
}

#[test]
fn loading_plugin_can_cache_events_before_instance_is_inserted() {
    let (_dir, mut bridge) = bridge();
    pending_plugin(&mut bridge, 1);
    assert!(bridge.cached_events_for_pending_plugins.contains_key(&1));
    assert!(bridge
        .plugin_ids_waiting_for_permission_request
        .contains(&1));
}

#[test]
fn running_plugin_receives_cached_events_after_permission_reply() {
    let (_dir, mut bridge) = bridge();
    let plugin = running_plugin(&mut bridge, 1);
    bridge.cache_plugin_events(1);
    let (shutdown, _receiver) = tokio::sync::mpsc::channel(1);
    let update = (
        Some(1),
        None,
        Event::CustomMessage("update".into(), "payload".into()),
    );
    bridge
        .update_plugins(vec![update.clone()], shutdown.clone())
        .unwrap();
    bridge
        .apply_cached_events(vec![1], false, shutdown.clone())
        .unwrap();
    assert_eq!(bridge.cached_events_for_pending_plugins[&1].len(), 1);
    assert_eq!(applied_events(&bridge, 1, &plugin), 0);

    bridge
        .update_plugins(
            vec![(
                Some(1),
                None,
                Event::PermissionRequestResult(PermissionStatus::Granted),
            )],
            shutdown.clone(),
        )
        .unwrap();
    bridge
        .apply_cached_events(vec![1], true, shutdown.clone())
        .unwrap();
    assert!(!bridge.cached_events_for_pending_plugins.contains_key(&1));
    assert!(!bridge
        .plugin_ids_waiting_for_permission_request
        .contains(&1));
    assert_eq!(applied_events(&bridge, 1, &plugin), 2);
    bridge.update_plugins(vec![update], shutdown).unwrap();
    assert_eq!(applied_events(&bridge, 1, &plugin), 3);

    bridge.unload_plugin(1).unwrap();
    assert!(!bridge.plugins_to_unload_once_loaded.contains(&1));
    bridge.cache_plugin_events(1);
    assert!(!bridge.cached_events_for_pending_plugins.contains_key(&1));
}

#[test]
fn plugin_metadata_without_a_running_instance_cannot_start_caching_events() {
    let (_dir, mut bridge) = bridge();
    let plugin = running_plugin(&mut bridge, 1);
    let plugin_config = plugin.lock().unwrap().store.data().plugin.clone();
    {
        let mut plugin_map = bridge.plugin_map.lock().unwrap();
        plugin_map.remove_plugins(1);
        plugin_map.insert_metadata(
            1,
            PluginMetadata {
                plugin_config,
                tab_index: None,
                rows: 24,
                columns: 80,
                cwd: Default::default(),
                is_background: false,
            },
        );
        assert_eq!(plugin_map.plugin_ids(), vec![1]);
    }
    bridge.cache_plugin_events(1);
    assert!(bridge.cached_events_for_pending_plugins.is_empty());
    assert!(bridge.plugin_ids_waiting_for_permission_request.is_empty());
}

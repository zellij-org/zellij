use super::WasmBridge;
use crate::thread_bus::ThreadSenders;
use tempfile::{tempdir, TempDir};
use zellij_utils::data::Event;
use zellij_utils::input::layout::RunPlugin;

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
fn permission_reply_does_not_consume_deferred_unload_before_load_finishes() {
    let (_dir, mut bridge) = bridge();
    pending_plugin(&mut bridge, 1);
    bridge.unload_plugin(1).unwrap();
    let (shutdown, _receiver) = tokio::sync::mpsc::channel(1);
    bridge
        .apply_cached_events(vec![1], true, shutdown.clone())
        .unwrap();
    assert!(bridge.plugins_to_unload_once_loaded.contains(&1));
    bridge
        .apply_cached_events(vec![1], false, shutdown)
        .unwrap();
    assert!(!bridge.plugins_to_unload_once_loaded.contains(&1));
    assert!(!bridge.cached_events_for_pending_plugins.contains_key(&1));
    bridge.cache_plugin_events(1);
    assert!(!bridge.cached_events_for_pending_plugins.contains_key(&1));
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

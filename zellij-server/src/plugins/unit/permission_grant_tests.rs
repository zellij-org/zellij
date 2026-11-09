//! Executed coverage for additive permission grants (Task A / remaining item 2).
//! Equivalent to upstream's ignored `granted_permissions_add_up_and_a_denial_keeps_them`,
//! plus shared / worker / pending-completion cases reachable without the e2e fixture wasm.

use super::*;
use crate::plugins::plugin_map::RunningPlugin;
use std::collections::HashSet;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::{tempdir, TempDir};
use zellij_utils::channels::{self, Receiver, SenderWithContext};
use zellij_utils::data::{Event, PermissionStatus, PermissionType};
use zellij_utils::errors::ErrorContext;
use zellij_utils::input::layout::RunPlugin;
use zellij_utils::input::permission::PermissionCache;
use zellij_utils::pane_size::Size;

struct LoadedBridge {
    bridge: WasmBridge,
    plugin_id: PluginId,
    client_id: ClientId,
    _plugin_receiver: Receiver<(PluginInstruction, ErrorContext)>,
    _plugin_dir: TempDir,
}

fn wait_until_running(bridge: &WasmBridge, plugin_id: PluginId, client_id: ClientId) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let map = bridge.plugin_map.lock().unwrap();
        let ready = if let Some(instance_id) = bridge.shared_instance_of(plugin_id) {
            map.shared_running_plugin(instance_id).is_some()
        } else {
            map.get_running_plugin(plugin_id, Some(client_id)).is_some()
        };
        drop(map);
        if ready {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for plugin {plugin_id} to finish loading");
}

fn drain_load_notifications(
    plugin_receiver: &Receiver<(PluginInstruction, ErrorContext)>,
    bridge: &mut WasmBridge,
) {
    let deadline = Instant::now() + Duration::from_millis(200);
    while let Ok((instruction, _)) = plugin_receiver.recv_deadline(deadline) {
        if let PluginInstruction::ApplyCachedEvents {
            plugin_ids,
            done_receiving_permissions,
        } = instruction
        {
            let (shutdown_sender, _shutdown_receiver) = tokio::sync::mpsc::channel(1);
            bridge
                .apply_cached_events(plugin_ids, done_receiving_permissions, shutdown_sender)
                .unwrap();
        }
    }
}

fn load_wat(wat: &str, shared: bool) -> LoadedBridge {
    let (to_plugin, plugin_receiver) = channels::unbounded();
    let (to_screen, _screen_receiver) = channels::unbounded();
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
    let mut bridge = WasmBridge::new(
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
    let name = if shared {
        "shared-grant.wasm"
    } else {
        "grant.wasm"
    };
    let plugin_path = plugin_dir.path().join(name);
    std::fs::write(&plugin_path, wat::parse_str(wat).unwrap()).unwrap();
    let run = RunPlugin::from_url(&format!("file:{}", plugin_path.display())).unwrap();
    let client_id = 1;
    let (plugin_id, loaded_client_id) = bridge
        .load_plugin_with_kind(
            &Some(run),
            None,
            Size { rows: 5, cols: 80 },
            None,
            false,
            Some(client_id),
            false,
        )
        .unwrap();
    assert_eq!(loaded_client_id, client_id);
    wait_until_running(&bridge, plugin_id, client_id);
    drain_load_notifications(&plugin_receiver, &mut bridge);
    LoadedBridge {
        bridge,
        plugin_id,
        client_id,
        _plugin_receiver: plugin_receiver,
        _plugin_dir: plugin_dir,
    }
}

fn nonshared_wat() -> &'static str {
    r#"(module
        (func (export "_start"))
        (func (export "load"))
        (func (export "test_worker")))"#
}

fn shared_wat() -> &'static str {
    r#"(module
        (func (export "_start"))
        (func (export "load"))
        (func (export "zellij_shared_plugin"))
        (func (export "test_worker")))"#
}

fn running_plugin(
    bridge: &WasmBridge,
    plugin_id: PluginId,
    client_id: ClientId,
) -> Arc<Mutex<RunningPlugin>> {
    let map = bridge.plugin_map.lock().unwrap();
    if let Some(instance_id) = bridge.shared_instance_of(plugin_id) {
        return map
            .shared_running_plugin(instance_id)
            .map(|(running, _)| running)
            .expect("shared running plugin");
    }
    map.get_running_plugin(plugin_id, Some(client_id))
        .expect("running plugin")
}

fn permission_cache_key(bridge: &WasmBridge, plugin_id: PluginId, client_id: ClientId) -> String {
    let running = running_plugin(bridge, plugin_id, client_id);
    let running = running.lock().unwrap();
    format!(
        "{}#{}",
        running.store.data().plugin.location,
        running.store.data().plugin_digest
    )
}

fn runtime_permissions(
    bridge: &WasmBridge,
    plugin_id: PluginId,
    client_id: ClientId,
) -> HashSet<PermissionType> {
    running_plugin(bridge, plugin_id, client_id)
        .lock()
        .unwrap()
        .store
        .data()
        .permissions
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
}

fn disk_permissions(cache_path: &std::path::Path, key: &str) -> HashSet<PermissionType> {
    PermissionCache::from_path_or_default(Some(cache_path.to_path_buf()))
        .get_permissions(key.to_owned())
        .into_iter()
        .flatten()
        .copied()
        .collect()
}

fn respond(
    bridge: &mut WasmBridge,
    plugin_id: PluginId,
    client_id: ClientId,
    cache_path: &std::path::Path,
    permissions: Vec<PermissionType>,
    status: PermissionStatus,
) -> (HashSet<PermissionType>, HashSet<PermissionType>) {
    bridge
        .cache_plugin_permissions(
            plugin_id,
            Some(client_id),
            permissions,
            status,
            Some(cache_path.to_path_buf()),
        )
        .unwrap();
    let key = permission_cache_key(bridge, plugin_id, client_id);
    (
        runtime_permissions(bridge, plugin_id, client_id),
        disk_permissions(cache_path, &key),
    )
}

/// Upstream scenario: A → grant B → deny C → grant A again; runtime vs disk;
/// earlier disk-only grant preserved; unchanged re-grant does not rewrite.
fn additive_grant_sequence(bridge: &mut WasmBridge, plugin_id: PluginId, client_id: ClientId) {
    let cache_dir = tempdir().unwrap();
    let cache_path = cache_dir.path().join("permissions.kdl");
    let key = permission_cache_key(bridge, plugin_id, client_id);

    // Granted in an earlier session but not requested in this one.
    assert!(PermissionCache::grant_and_persist(
        Some(cache_path.clone()),
        key.clone(),
        vec![PermissionType::WriteToStdin],
    )
    .unwrap());

    assert_eq!(
        respond(
            bridge,
            plugin_id,
            client_id,
            &cache_path,
            vec![PermissionType::ReadApplicationState],
            PermissionStatus::Granted,
        ),
        (
            HashSet::from([PermissionType::ReadApplicationState]),
            HashSet::from([
                PermissionType::ReadApplicationState,
                PermissionType::WriteToStdin,
            ]),
        )
    );

    let granted = (
        HashSet::from([
            PermissionType::ReadApplicationState,
            PermissionType::RunCommands,
        ]),
        HashSet::from([
            PermissionType::ReadApplicationState,
            PermissionType::RunCommands,
            PermissionType::WriteToStdin,
        ]),
    );
    assert_eq!(
        respond(
            bridge,
            plugin_id,
            client_id,
            &cache_path,
            vec![PermissionType::RunCommands],
            PermissionStatus::Granted,
        ),
        granted
    );

    let untouched = "// untouched\n";
    std::fs::OpenOptions::new()
        .append(true)
        .open(&cache_path)
        .unwrap()
        .write_all(untouched.as_bytes())
        .unwrap();

    assert_eq!(
        respond(
            bridge,
            plugin_id,
            client_id,
            &cache_path,
            vec![PermissionType::ChangeApplicationState],
            PermissionStatus::Denied,
        ),
        granted
    );
    assert_eq!(
        respond(
            bridge,
            plugin_id,
            client_id,
            &cache_path,
            vec![PermissionType::ReadApplicationState],
            PermissionStatus::Granted,
        ),
        granted
    );
    assert!(
        std::fs::read_to_string(&cache_path)
            .unwrap()
            .ends_with(untouched),
        "unchanged re-grant must not rewrite the durable cache"
    );
}

#[test]
fn first_denial_on_a_fresh_cache_creates_no_entry() {
    // Non-ignored equivalent of denied_permission_request_result under the
    // intentional no-denial-cache contract (golden must be None, not Some([])).
    let LoadedBridge {
        mut bridge,
        plugin_id,
        client_id,
        ..
    } = load_wat(nonshared_wat(), false);
    let cache_dir = tempdir().unwrap();
    let cache_path = cache_dir.path().join("permissions.kdl");
    let key = permission_cache_key(&bridge, plugin_id, client_id);

    let (runtime, disk) = respond(
        &mut bridge,
        plugin_id,
        client_id,
        &cache_path,
        vec![
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::RunCommands,
        ],
        PermissionStatus::Denied,
    );
    assert!(
        runtime.is_empty(),
        "first denial must not invent runtime grants"
    );
    assert!(
        disk.is_empty(),
        "first denial must not invent durable grants"
    );
    assert_eq!(
        PermissionCache::from_path_or_default(Some(cache_path.clone())).get_permissions(key),
        None,
        "fresh-cache first denial must leave no plugin entry (not Some([]))"
    );
    assert!(
        !cache_path.exists()
            || std::fs::read_to_string(&cache_path)
                .unwrap()
                .trim()
                .is_empty(),
        "denial must not publish a permissions file for an empty cache"
    );
}

#[test]
fn granted_permissions_add_up_and_a_denial_keeps_them_nonshared() {
    let LoadedBridge {
        mut bridge,
        plugin_id,
        client_id,
        ..
    } = load_wat(nonshared_wat(), false);
    additive_grant_sequence(&mut bridge, plugin_id, client_id);
}

#[test]
fn granted_permissions_add_up_and_a_denial_keeps_them_shared() {
    let LoadedBridge {
        mut bridge,
        plugin_id,
        client_id,
        ..
    } = load_wat(shared_wat(), true);
    // Shared loads use the instance id as the map key; client_id still resolves
    // through shared_running_plugin when present.
    additive_grant_sequence(&mut bridge, plugin_id, client_id);
}

#[test]
fn denial_completes_a_pending_permission_request_exactly_once() {
    let LoadedBridge {
        mut bridge,
        plugin_id,
        client_id,
        ..
    } = load_wat(nonshared_wat(), false);
    bridge.plugins_with_unfinished_load.remove(&plugin_id);
    let cache_dir = tempdir().unwrap();
    let cache_path = cache_dir.path().join("permissions.kdl");

    // Seed a runtime grant so denial must not wipe it.
    bridge
        .cache_plugin_permissions(
            plugin_id,
            Some(client_id),
            vec![PermissionType::ReadApplicationState],
            PermissionStatus::Granted,
            Some(cache_path.clone()),
        )
        .unwrap();

    let running = running_plugin(&bridge, plugin_id, client_id);
    let (generation, digest) = {
        let running = running.lock().unwrap();
        (
            running.store.data().generation,
            running.store.data().plugin_digest.clone(),
        )
    };
    bridge.cache_plugin_events(plugin_id, generation, digest);
    assert!(bridge
        .plugin_ids_waiting_for_permission_request
        .contains(&plugin_id));

    // Mirror apply_permission_result for a denial: update runtime/cache, deliver
    // PermissionRequestResult, then drain pending work once.
    bridge
        .cache_plugin_permissions(
            plugin_id,
            Some(client_id),
            vec![PermissionType::ChangeApplicationState],
            PermissionStatus::Denied,
            Some(cache_path.clone()),
        )
        .unwrap();
    let (shutdown_sender, _shutdown_receiver) = tokio::sync::mpsc::channel(1);
    bridge
        .update_plugins(
            vec![(
                Some(plugin_id),
                Some(client_id),
                Event::PermissionRequestResult(PermissionStatus::Denied),
            )],
            shutdown_sender.clone(),
        )
        .unwrap();
    bridge
        .apply_cached_events(vec![plugin_id], true, shutdown_sender.clone())
        .unwrap();

    assert!(
        !bridge
            .plugin_ids_waiting_for_permission_request
            .contains(&plugin_id),
        "denial must clear the pending-permission wait exactly once"
    );
    assert_eq!(
        runtime_permissions(&bridge, plugin_id, client_id),
        HashSet::from([PermissionType::ReadApplicationState]),
        "denial must leave earlier runtime grants intact"
    );
    let key = permission_cache_key(&bridge, plugin_id, client_id);
    assert_eq!(
        disk_permissions(&cache_path, &key),
        HashSet::from([PermissionType::ReadApplicationState]),
        "denial must not rewrite the durable cache"
    );

    // A second completion pass must be a no-op (already cleared).
    bridge
        .apply_cached_events(vec![plugin_id], true, shutdown_sender)
        .unwrap();
    assert!(!bridge
        .plugin_ids_waiting_for_permission_request
        .contains(&plugin_id));
}

#[test]
fn denial_flushes_cached_worker_messages_when_the_pending_request_completes() {
    let LoadedBridge {
        mut bridge,
        plugin_id,
        client_id,
        ..
    } = load_wat(nonshared_wat(), false);
    let cache_dir = tempdir().unwrap();
    let cache_path = cache_dir.path().join("permissions.kdl");

    // Cache a worker message while the plugin is waiting for a permission answer.
    let running = running_plugin(&bridge, plugin_id, client_id);
    let (generation, digest) = {
        let running = running.lock().unwrap();
        (
            running.store.data().generation,
            running.store.data().plugin_digest.clone(),
        )
    };
    bridge.cache_plugin_events(plugin_id, generation, digest);
    // Force the worker path to cache by temporarily removing workers, then
    // restoring after we have proven the pending cache was populated.
    let workers = {
        let mut map = bridge.plugin_map.lock().unwrap();
        let assets = map
            .remove_plugin_instance(plugin_id, client_id)
            .expect("nonshared instance");
        let (running, subscriptions, workers) = assets;
        // Re-insert without workers so post_messages caches.
        map.insert(
            plugin_id,
            client_id,
            running,
            subscriptions,
            Default::default(),
        );
        workers
    };
    // worker_sender looks up `{name}_worker`, so the API name is "test".
    bridge
        .post_messages_to_plugin_worker(
            plugin_id,
            client_id,
            "test".to_owned(),
            vec![("hello".to_owned(), "world".to_owned())],
        )
        .unwrap();
    assert!(
        bridge.cached_worker_messages.contains_key(&plugin_id),
        "worker messages must be cached while the destination worker is absent"
    );
    // Restore workers so the flush has somewhere to go.
    {
        let mut map = bridge.plugin_map.lock().unwrap();
        let assets = map.remove_plugin_instance(plugin_id, client_id).unwrap();
        map.insert(plugin_id, client_id, assets.0, assets.1, workers);
    }

    bridge
        .cache_plugin_permissions(
            plugin_id,
            Some(client_id),
            vec![PermissionType::RunCommands],
            PermissionStatus::Denied,
            Some(cache_path),
        )
        .unwrap();
    // Permission answers must not skip the flush when the loader already finished.
    bridge.plugins_with_unfinished_load.remove(&plugin_id);
    let (shutdown_sender, _shutdown_receiver) = tokio::sync::mpsc::channel(1);
    bridge
        .apply_cached_events(vec![plugin_id], true, shutdown_sender)
        .unwrap();

    assert!(
        !bridge.cached_worker_messages.contains_key(&plugin_id),
        "completing the pending permission request must flush cached worker messages"
    );
    assert!(!bridge
        .plugin_ids_waiting_for_permission_request
        .contains(&plugin_id));
}

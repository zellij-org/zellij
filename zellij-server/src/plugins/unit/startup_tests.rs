use super::WasmBridge;
use crate::plugins::{plugin_tests::e2e_plugin_path, PluginId, PluginInstruction};
use crate::screen::ScreenInstruction;
use crate::thread_bus::ThreadSenders;
use crate::ClientId;
use std::collections::HashSet;
use std::io::Write;
use std::time::{Duration, Instant};
use tempfile::{tempdir, TempDir};
use zellij_utils::channels::{self, SenderWithContext};
use zellij_utils::data::{Event, EventType, PermissionStatus, PermissionType};
use zellij_utils::errors::ErrorContext;
use zellij_utils::input::layout::RunPlugin;
use zellij_utils::input::permission::PermissionCache;
use zellij_utils::pane_size::Size;

struct FixtureBridge {
    bridge: WasmBridge,
    plugin_ids: Vec<PluginId>,
    screen_receiver: channels::Receiver<(ScreenInstruction, ErrorContext)>,
    _plugin_dir: TempDir,
}

/// Loads `count` fixture plugins for one client and waits until each has loaded.
fn load_fixture_plugins(count: usize, client_id: ClientId) -> FixtureBridge {
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
    let run = RunPlugin::from_url(&format!(
        "file:{}",
        e2e_plugin_path("fixture-plugin-for-tests").display()
    ))
    .unwrap();
    let mut plugin_ids = vec![];
    for _ in 0..count {
        let (plugin_id, loaded_client_id) = bridge
            .load_plugin_with_kind(
                &Some(run.clone()),
                None,
                Size { rows: 5, cols: 80 },
                None,
                false,
                Some(client_id),
                true,
            )
            .unwrap();
        assert_eq!(loaded_client_id, client_id);
        plugin_ids.push(plugin_id);
    }
    let mut pending_plugins = plugin_ids.clone();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !pending_plugins.is_empty() {
        match plugin_receiver.recv_deadline(deadline).unwrap().0 {
            PluginInstruction::ApplyCachedEvents { plugin_ids, .. } => {
                pending_plugins.retain(|plugin_id| !plugin_ids.contains(plugin_id));
            },
            _ => {},
        }
    }

    FixtureBridge {
        bridge,
        plugin_ids,
        screen_receiver,
        _plugin_dir: plugin_dir,
    }
}

#[test]
#[ignore]
fn cached_background_permission_results_reach_existing_instances_before_client_connects() {
    let client_id = 1;
    let FixtureBridge {
        mut bridge,
        plugin_ids,
        screen_receiver,
        _plugin_dir,
    } = load_fixture_plugins(2, client_id);
    // Let the existing fixture render permission results as well as its usual events.
    for plugin_id in &plugin_ids {
        bridge
            .plugin_map
            .lock()
            .unwrap()
            .get_running_plugin_and_subscriptions(*plugin_id, client_id)
            .expect("background plugin loaded")
            .1
            .lock()
            .unwrap()
            .insert(EventType::PermissionRequestResult);
    }
    let (shutdown_sender, _shutdown_receiver) = tokio::sync::mpsc::channel(1);
    bridge
        .update_plugins(
            vec![
                (
                    Some(plugin_ids[0]),
                    Some(client_id + 1),
                    Event::PermissionRequestResult(PermissionStatus::Denied),
                ),
                (
                    Some(plugin_ids[0]),
                    Some(client_id),
                    Event::PermissionRequestResult(PermissionStatus::Granted),
                ),
                (
                    Some(plugin_ids[1]),
                    Some(client_id),
                    Event::PermissionRequestResult(PermissionStatus::Denied),
                ),
            ],
            shutdown_sender.clone(),
        )
        .unwrap();
    bridge
        .apply_cached_events(plugin_ids.clone(), false, shutdown_sender.clone())
        .unwrap();
    for attached in [false, true] {
        if attached {
            bridge.add_client(client_id).unwrap();
            bridge
                .apply_cached_events(plugin_ids.clone(), false, shutdown_sender.clone())
                .unwrap();
            bridge
                .update_plugins(
                    vec![(
                        None,
                        Some(client_id),
                        Event::CustomMessage("after-attach".into(), String::new()),
                    )],
                    shutdown_sender.clone(),
                )
                .unwrap();
        } else {
            assert!(bridge.connected_clients.lock().unwrap().is_empty());
        }
        let mut pending_plugins = plugin_ids.clone();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !pending_plugins.is_empty() {
            let instruction = screen_receiver
                .recv_deadline(deadline)
                .expect("background plugins must receive their cached permission results")
                .0;
            if let ScreenInstruction::PluginBytes(assets) = instruction {
                for asset in assets {
                    let bytes = String::from_utf8_lossy(&asset.bytes);
                    if asset.client_id != client_id
                        || !pending_plugins.contains(&asset.plugin_id)
                        || (attached && !bytes.contains("after-attach"))
                        || !bytes.contains("PermissionRequestResult")
                    {
                        continue;
                    }
                    let granted = usize::from(asset.plugin_id == plugin_ids[0]);
                    assert_eq!(
                        bytes.matches("PermissionRequestResult(Granted)").count(),
                        granted
                    );
                    assert_eq!(
                        bytes.matches("PermissionRequestResult(Denied)").count(),
                        1 - granted
                    );
                    pending_plugins.retain(|plugin_id| *plugin_id != asset.plugin_id);
                }
            }
        }
    }
}

#[test]
#[ignore]
fn granted_permissions_add_up_and_a_denial_keeps_them() {
    let client_id = 1;
    let FixtureBridge {
        mut bridge,
        plugin_ids,
        _plugin_dir,
        ..
    } = load_fixture_plugins(1, client_id);
    let plugin_id = plugin_ids[0];
    let running_plugin = bridge
        .plugin_map
        .lock()
        .unwrap()
        .get_running_plugin(plugin_id, Some(client_id))
        .unwrap();
    let location = running_plugin
        .lock()
        .unwrap()
        .store
        .data()
        .plugin
        .location
        .to_string();

    // Granted in an earlier session but not requested in this one.
    let cache_dir = tempdir().unwrap();
    let cache_path = cache_dir.path().join("permissions.kdl");
    let mut earlier_session = PermissionCache::from_path_or_default(Some(cache_path.clone()));
    earlier_session.cache(location.clone(), vec![PermissionType::WriteToStdin]);
    earlier_session.write_to_file().unwrap();

    let mut respond = |permissions: Vec<PermissionType>, status| {
        bridge
            .cache_plugin_permissions(
                plugin_id,
                Some(client_id),
                permissions,
                status,
                Some(cache_path.clone()),
            )
            .unwrap();
        let running = running_plugin
            .lock()
            .unwrap()
            .store
            .data()
            .permissions
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_default();
        let cached: HashSet<_> = PermissionCache::from_path_or_default(Some(cache_path.clone()))
            .get_permissions(location.clone())
            .into_iter()
            .flatten()
            .copied()
            .collect();
        (running, cached)
    };

    assert_eq!(
        respond(
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
        respond(vec![PermissionType::RunCommands], PermissionStatus::Granted),
        granted
    );

    // Rewriting the cache drops comments, so this one shows whether the file was written again.
    let untouched = "// untouched\n";
    std::fs::OpenOptions::new()
        .append(true)
        .open(&cache_path)
        .unwrap()
        .write_all(untouched.as_bytes())
        .unwrap();
    assert_eq!(
        respond(
            vec![PermissionType::ChangeApplicationState],
            PermissionStatus::Denied,
        ),
        granted
    );
    assert_eq!(
        respond(
            vec![PermissionType::ReadApplicationState],
            PermissionStatus::Granted,
        ),
        granted
    );
    assert!(std::fs::read_to_string(&cache_path)
        .unwrap()
        .ends_with(untouched));
}

use super::WasmBridge;
use crate::plugins::{plugin_tests::e2e_plugin_path, PluginInstruction};
use crate::screen::ScreenInstruction;
use crate::thread_bus::ThreadSenders;
use std::time::{Duration, Instant};
use tempfile::tempdir;
use zellij_utils::channels::{self, SenderWithContext};
use zellij_utils::data::{Event, EventType, PermissionStatus};
use zellij_utils::input::layout::RunPlugin;
use zellij_utils::pane_size::Size;

#[test]
#[ignore]
fn cached_background_permission_results_reach_existing_instances_before_client_connects() {
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
    let client_id = 1;
    for _ in 0..2 {
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

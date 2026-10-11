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
fn cached_background_permission_result_reaches_first_client() {
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
    let (plugin_id, client_id) = bridge
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
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match plugin_receiver.recv_deadline(deadline).unwrap().0 {
            PluginInstruction::ApplyCachedEvents { plugin_ids, .. }
                if plugin_ids.contains(&plugin_id) =>
            {
                break;
            },
            _ => {},
        }
    }

    // Let the existing fixture render permission results as well as its usual events.
    bridge
        .plugin_map
        .lock()
        .unwrap()
        .get_running_plugin_and_subscriptions(plugin_id, client_id)
        .expect("background plugin loaded")
        .1
        .lock()
        .unwrap()
        .insert(EventType::PermissionRequestResult);
    let (shutdown_sender, _shutdown_receiver) = tokio::sync::mpsc::channel(1);
    bridge
        .update_plugins(
            vec![(
                Some(plugin_id),
                Some(client_id),
                Event::PermissionRequestResult(PermissionStatus::Granted),
            )],
            shutdown_sender.clone(),
        )
        .unwrap();
    bridge
        .apply_cached_events(vec![plugin_id], false, shutdown_sender.clone())
        .unwrap();

    bridge.add_client(client_id).unwrap();
    let instruction = plugin_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("first client must trigger cached-event replay")
        .0;
    if let PluginInstruction::ApplyCachedEvents {
        plugin_ids,
        done_receiving_permissions,
    } = instruction
    {
        bridge
            .apply_cached_events(plugin_ids, done_receiving_permissions, shutdown_sender)
            .unwrap();
    } else {
        panic!("unexpected plugin instruction: {:?}", instruction);
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let instruction = screen_receiver
            .recv_deadline(deadline)
            .expect("background plugin must receive its cached permission result")
            .0;
        if let ScreenInstruction::PluginBytes(assets) = instruction {
            if assets.iter().any(|asset| {
                asset.plugin_id == plugin_id
                    && asset.client_id == client_id
                    && String::from_utf8_lossy(&asset.bytes)
                        .contains("PermissionRequestResult(Granted)")
            }) {
                break;
            }
        }
    }
}

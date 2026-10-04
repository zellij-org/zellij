use super::plugin_tests::create_plugin_thread;
use crate::panes::PaneId;
use crate::plugins::{PluginId, PluginInstruction};
use crate::screen::ScreenInstruction;
use crate::ClientId;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::tempdir;
use zellij_utils::channels::{Receiver, SenderWithContext};
use zellij_utils::data::{BareKey, Event, KeyWithModifier, PermissionStatus};
use zellij_utils::errors::ErrorContext;
use zellij_utils::input::layout::{
    PluginUserConfiguration, RunPlugin, RunPluginLocation, RunPluginOrAlias,
};
use zellij_utils::pane_size::Size;

type Renders = HashMap<(PluginId, ClientId), String>;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/../target/e2e-data/plugins/{}.wasm",
        std::env::var_os("CARGO_MANIFEST_DIR")
            .unwrap()
            .to_string_lossy(),
        name
    ))
}

fn shared_fixture(configuration: &[(&str, &str)]) -> RunPluginOrAlias {
    let configuration: BTreeMap<String, String> = configuration
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(fixture_path("fixture-shared-plugin-for-tests")),
        configuration: PluginUserConfiguration::new(configuration),
        ..Default::default()
    })
}

fn legacy_fixture() -> RunPluginOrAlias {
    RunPluginOrAlias::RunPlugin(RunPlugin {
        _allow_exec_host_cmd: false,
        location: RunPluginLocation::File(fixture_path("fixture-plugin-for-tests")),
        configuration: Default::default(),
        ..Default::default()
    })
}

fn load(
    plugin_thread_sender: &SenderWithContext<PluginInstruction>,
    run_plugin: RunPluginOrAlias,
    tab_index: usize,
    client_id: ClientId,
) {
    let _ = plugin_thread_sender.send(PluginInstruction::Load(
        Some(false),
        false,
        false,
        None,
        run_plugin,
        Some(tab_index),
        None,
        client_id,
        Size { cols: 80, rows: 5 },
        None,
        None,
        false,
        None,
        None,
        None,
    ));
}

fn key_event() -> Event {
    Event::Key(KeyWithModifier::new(BareKey::Char('!')))
}

struct ScreenLog {
    receiver: Receiver<(ScreenInstruction, ErrorContext)>,
    plugin_thread_sender: SenderWithContext<PluginInstruction>,
    cache_path: PathBuf,
    renders: Renders,
    render_log: Vec<(PluginId, ClientId, String)>,
    collapsed: HashSet<PaneId>,
}

impl ScreenLog {
    fn new(
        receiver: Receiver<(ScreenInstruction, ErrorContext)>,
        plugin_thread_sender: SenderWithContext<PluginInstruction>,
        cache_path: PathBuf,
    ) -> Self {
        ScreenLog {
            receiver,
            plugin_thread_sender,
            cache_path,
            renders: HashMap::new(),
            render_log: vec![],
            collapsed: HashSet::new(),
        }
    }
    fn receive_one(&mut self, timeout: Duration) {
        let Ok((instruction, _)) = self.receiver.recv_timeout(timeout) else {
            return;
        };
        match instruction {
            ScreenInstruction::PluginBytes(assets) => {
                for asset in assets {
                    if asset.bytes.is_empty() {
                        continue;
                    }
                    let rendered = String::from_utf8_lossy(&asset.bytes).to_string();
                    self.render_log
                        .push((asset.plugin_id, asset.client_id, rendered.clone()));
                    self.renders
                        .insert((asset.plugin_id, asset.client_id), rendered);
                }
            },
            ScreenInstruction::RequestPluginPermissions(plugin_id, plugin_permission) => {
                let _ = self
                    .plugin_thread_sender
                    .send(PluginInstruction::PermissionRequestResult(
                        plugin_id,
                        None,
                        plugin_permission.permissions,
                        PermissionStatus::Granted,
                        Some(self.cache_path.clone()),
                    ));
            },
            ScreenInstruction::SetPaneCollapsed(pane_id, true) => {
                self.collapsed.insert(pane_id);
            },
            _ => {},
        }
    }
    fn wait_until(&mut self, description: &str, condition: impl Fn(&Renders) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !condition(&self.renders) {
            if Instant::now() > deadline {
                panic!(
                    "timed out waiting for: {}, renders: {:#?}",
                    description, self.renders
                );
            }
            self.receive_one(Duration::from_millis(100));
        }
    }
    fn settle(&mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.receive_one(Duration::from_millis(50));
        }
    }
}

fn field<'a>(rendered: &'a str, name: &str) -> &'a str {
    let start = rendered
        .find(&format!("{}:", name))
        .map(|i| i + name.len() + 1)
        .unwrap_or(rendered.len());
    let rest = &rendered[start..];
    let end = if rest.starts_with('[') {
        rest.find(']').map(|i| i + 1).unwrap_or(rest.len())
    } else {
        rest.find(' ').unwrap_or(rest.len())
    };
    &rest[..end]
}

fn slots_of_client(renders: &Renders, client_id: ClientId) -> Vec<PluginId> {
    let mut slots: Vec<PluginId> = renders
        .keys()
        .filter(|(_, c)| *c == client_id)
        .map(|(slot_id, _)| *slot_id)
        .collect();
    slots.sort();
    slots
}

fn start() -> (
    SenderWithContext<PluginInstruction>,
    ScreenLog,
    Box<dyn FnOnce()>,
    tempfile::TempDir,
) {
    let temp_folder = tempdir().unwrap();
    let cache_path = PathBuf::from(temp_folder.path()).join("permissions_test.kdl");
    let (plugin_thread_sender, screen_receiver, teardown) = create_plugin_thread(None, None);
    let screen_log = ScreenLog::new(screen_receiver, plugin_thread_sender.clone(), cache_path);
    (plugin_thread_sender, screen_log, teardown, temp_folder)
}

#[test]
#[ignore]
pub fn shared_plugin_one_instance_many_slots() {
    let (sender, mut screen, teardown, _temp_folder) = start();
    let client_id = 1;
    let _ = sender.send(PluginInstruction::AddClient(client_id));
    load(&sender, shared_fixture(&[("label", "a")]), 0, client_id);
    load(&sender, shared_fixture(&[("label", "b")]), 1, client_id);
    load(&sender, shared_fixture(&[("label", "c")]), 2, client_id);
    screen.wait_until("three slots rendered", |renders| {
        slots_of_client(renders, client_id).len() == 3
    });
    let first_slot = slots_of_client(&screen.renders, client_id)[0];
    let _ = sender.send(PluginInstruction::Update(vec![(
        Some(first_slot),
        Some(client_id),
        key_event(),
    )]));
    screen.wait_until("three slots rendered with all slots known", |renders| {
        let slots = slots_of_client(renders, client_id);
        slots.len() == 3
            && slots.iter().all(|slot_id| {
                field(&renders[&(*slot_id, client_id)], "slots") == format!("{:?}", slots)
            })
    });
    teardown();
    let slots = slots_of_client(&screen.renders, client_id);
    let instances: HashSet<&str> = slots
        .iter()
        .map(|slot_id| field(&screen.renders[&(*slot_id, client_id)], "instance"))
        .collect();
    assert_eq!(instances.len(), 1, "all slots are served by one instance");
    for slot_id in &slots {
        let rendered = &screen.renders[&(*slot_id, client_id)];
        assert_eq!(
            field(rendered, "loads"),
            "1",
            "load runs once: {}",
            rendered
        );
        assert_eq!(field(rendered, "slot"), slot_id.to_string());
    }
    let names: HashSet<&str> = slots
        .iter()
        .map(|slot_id| field(&screen.renders[&(*slot_id, client_id)], "label"))
        .collect();
    assert_eq!(
        names,
        HashSet::from(["a", "b", "c"]),
        "each slot keeps its own configuration"
    );
}

#[test]
#[ignore]
pub fn shared_plugin_instance_key_creates_separate_instances() {
    let (sender, mut screen, teardown, _temp_folder) = start();
    let client_id = 1;
    let _ = sender.send(PluginInstruction::AddClient(client_id));
    load(
        &sender,
        shared_fixture(&[("instance", "one"), ("label", "a")]),
        0,
        client_id,
    );
    load(
        &sender,
        shared_fixture(&[("instance", "two"), ("label", "b")]),
        0,
        client_id,
    );
    load(
        &sender,
        shared_fixture(&[("instance", "one"), ("label", "c")]),
        1,
        client_id,
    );
    screen.wait_until("three slots rendered", |renders| {
        slots_of_client(renders, client_id).len() == 3
    });
    for slot_id in slots_of_client(&screen.renders, client_id) {
        let _ = sender.send(PluginInstruction::Update(vec![(
            Some(slot_id),
            Some(client_id),
            key_event(),
        )]));
    }
    screen.wait_until(
        "instance one has two slots and instance two has one",
        |renders| {
            let slots = slots_of_client(renders, client_id);
            slots.len() == 3 && {
                let mut slot_list_lengths: Vec<usize> = slots
                    .iter()
                    .map(|slot_id| {
                        field(&renders[&(*slot_id, client_id)], "slots")
                            .split(',')
                            .count()
                    })
                    .collect();
                slot_list_lengths.sort();
                slot_list_lengths == vec![1, 2, 2]
            }
        },
    );
    teardown();
    let by_name: HashMap<&str, &str> = screen
        .renders
        .values()
        .map(|rendered| (field(rendered, "label"), field(rendered, "instance")))
        .collect();
    assert_eq!(
        by_name["a"], by_name["c"],
        "same instance name shares an instance"
    );
    assert_ne!(
        by_name["a"], by_name["b"],
        "different instance names do not share"
    );
}

#[test]
#[ignore]
pub fn shared_plugin_renders_per_client() {
    let (sender, mut screen, teardown, _temp_folder) = start();
    let _ = sender.send(PluginInstruction::AddClient(1));
    let _ = sender.send(PluginInstruction::AddClient(2));
    load(&sender, shared_fixture(&[("label", "a")]), 0, 1);
    screen.wait_until("the slot rendered for both clients", |renders| {
        slots_of_client(renders, 1).len() == 1 && slots_of_client(renders, 2).len() == 1
    });
    let slot_id = slots_of_client(&screen.renders, 1)[0];
    assert_eq!(field(&screen.renders[&(slot_id, 1)], "client"), "1");
    assert_eq!(field(&screen.renders[&(slot_id, 2)], "client"), "2");
    let renders_before_key = screen.render_log.len();
    let _ = sender.send(PluginInstruction::Update(vec![(
        Some(slot_id),
        Some(2),
        key_event(),
    )]));
    screen.wait_until("client 2 re-rendered after its key", |renders| {
        field(&renders[&(slot_id, 2)], "keys") == format!("[(Some({}), Some(2))]", slot_id)
    });
    screen.settle(Duration::from_millis(300));
    teardown();
    let clients_rendered_after_key: HashSet<ClientId> = screen.render_log[renders_before_key..]
        .iter()
        .map(|(_, client_id, _)| *client_id)
        .collect();
    assert_eq!(
        clients_rendered_after_key,
        HashSet::from([2]),
        "RenderResponse::Client only re-renders the named client"
    );
}

#[test]
#[ignore]
pub fn shared_plugin_slot_removed_when_pane_closes() {
    let (sender, mut screen, teardown, _temp_folder) = start();
    let client_id = 1;
    let _ = sender.send(PluginInstruction::AddClient(client_id));
    load(&sender, shared_fixture(&[("label", "a")]), 0, client_id);
    load(&sender, shared_fixture(&[("label", "b")]), 1, client_id);
    screen.wait_until("both slots rendered", |renders| {
        slots_of_client(renders, client_id).len() == 2
    });
    let slots = slots_of_client(&screen.renders, client_id);
    let (closed_slot, remaining_slot) = (slots[0], slots[1]);
    let _ = sender.send(PluginInstruction::Unload(closed_slot));
    let renders_before_key = screen.render_log.len();
    let _ = sender.send(PluginInstruction::Update(vec![(
        Some(remaining_slot),
        Some(client_id),
        key_event(),
    )]));
    screen.wait_until(
        "remaining slot knows the other slot was removed",
        |renders| {
            let rendered = &renders[&(remaining_slot, client_id)];
            field(rendered, "removed") == format!("[{}]", closed_slot)
                && field(rendered, "slots") == format!("[{}]", remaining_slot)
        },
    );
    screen.settle(Duration::from_millis(300));
    teardown();
    assert!(
        screen.render_log[renders_before_key..]
            .iter()
            .all(|(slot_id, _, _)| *slot_id != closed_slot),
        "a removed slot is not rendered"
    );
    assert_eq!(
        field(&screen.renders[&(remaining_slot, client_id)], "loads"),
        "1"
    );
}

#[test]
#[ignore]
pub fn shared_plugin_client_connect_and_disconnect() {
    let (sender, mut screen, teardown, _temp_folder) = start();
    let _ = sender.send(PluginInstruction::AddClient(1));
    load(&sender, shared_fixture(&[("label", "a")]), 0, 1);
    screen.wait_until("slot rendered for client 1", |renders| {
        slots_of_client(renders, 1).len() == 1
    });
    let slot_id = slots_of_client(&screen.renders, 1)[0];
    assert_eq!(field(&screen.renders[&(slot_id, 1)], "clients"), "[1]");
    let _ = sender.send(PluginInstruction::AddClient(2));
    screen.wait_until("new client is rendered without a new instance", |renders| {
        renders
            .get(&(slot_id, 2))
            .map(|rendered| field(rendered, "clients") == "[1, 2]")
            .unwrap_or(false)
    });
    let _ = sender.send(PluginInstruction::RemoveClient(2));
    let _ = sender.send(PluginInstruction::Update(vec![(
        Some(slot_id),
        Some(1),
        key_event(),
    )]));
    screen.wait_until("client 2 is gone", |renders| {
        let rendered = &renders[&(slot_id, 1)];
        field(rendered, "clients") == "[1]" && field(rendered, "disconnected") == "[2]"
    });
    teardown();
    let instances: HashSet<&str> = screen
        .renders
        .values()
        .map(|rendered| field(rendered, "instance"))
        .collect();
    assert_eq!(instances.len(), 1);
    assert!(screen
        .renders
        .values()
        .all(|rendered| field(rendered, "loads") == "1"));
}

#[test]
#[ignore]
pub fn shared_plugin_slot_commands_are_checked() {
    let (sender, mut screen, teardown, _temp_folder) = start();
    let client_id = 1;
    let _ = sender.send(PluginInstruction::AddClient(client_id));
    load(&sender, shared_fixture(&[("label", "a")]), 0, client_id);
    screen.wait_until("slot rendered", |renders| {
        slots_of_client(renders, client_id).len() == 1
    });
    let slot_id = slots_of_client(&screen.renders, client_id)[0];
    let _ = sender.send(PluginInstruction::KeybindPipe {
        name: "selectable_slot".to_owned(),
        payload: None,
        plugin: None,
        args: None,
        configuration: None,
        floating: None,
        pane_id_to_replace: None,
        pane_title: None,
        cwd: None,
        skip_cache: false,
        cli_client_id: client_id,
        plugin_and_client_id: Some((slot_id, client_id)),
        notification_end: None,
    });
    screen.wait_until("slot command results rendered", |renders| {
        field(&renders[&(slot_id, client_id)], "self_commands").contains("Err")
    });
    teardown();
    let rendered = &screen.renders[&(slot_id, client_id)];
    let results = field(rendered, "self_commands");
    assert!(
        results.starts_with("[\"Ok(())\""),
        "own slot is accepted: {}",
        results
    );
    assert!(
        results.contains("does not belong"),
        "foreign slot is rejected: {}",
        results
    );
}

#[test]
#[ignore]
pub fn shared_plugin_collapses_the_slot_it_renders() {
    let (sender, mut screen, teardown, _temp_folder) = start();
    let client_id = 1;
    let _ = sender.send(PluginInstruction::AddClient(client_id));
    load(
        &sender,
        shared_fixture(&[("label", "a"), ("collapse_on_render", "true")]),
        0,
        client_id,
    );
    load(
        &sender,
        shared_fixture(&[("label", "b"), ("collapse_on_render", "true")]),
        1,
        client_id,
    );
    screen.wait_until("two slots rendered", |renders| {
        slots_of_client(renders, client_id).len() == 2
    });
    let slots = slots_of_client(&screen.renders, client_id);
    let expected: HashSet<PaneId> = slots
        .iter()
        .map(|slot_id| PaneId::Plugin(*slot_id))
        .collect();
    let deadline = Instant::now() + Duration::from_secs(30);
    while screen.collapsed != expected && Instant::now() < deadline {
        screen.receive_one(Duration::from_millis(100));
    }
    teardown();
    assert_eq!(
        screen.collapsed, expected,
        "each slot collapses its own pane, not the instance's"
    );
}

#[test]
#[ignore]
pub fn shared_plugin_collapses_a_slot_by_id() {
    let (sender, mut screen, teardown, _temp_folder) = start();
    let client_id = 1;
    let _ = sender.send(PluginInstruction::AddClient(client_id));
    load(&sender, shared_fixture(&[("label", "a")]), 0, client_id);
    load(&sender, shared_fixture(&[("label", "b")]), 1, client_id);
    screen.wait_until("two slots rendered", |renders| {
        slots_of_client(renders, client_id).len() == 2
    });
    let slots = slots_of_client(&screen.renders, client_id);
    let _ = sender.send(PluginInstruction::KeybindPipe {
        name: "collapse_slots".to_owned(),
        payload: None,
        plugin: None,
        args: None,
        configuration: None,
        floating: None,
        pane_id_to_replace: None,
        pane_title: None,
        cwd: None,
        skip_cache: false,
        cli_client_id: client_id,
        plugin_and_client_id: Some((slots[0], client_id)),
        notification_end: None,
    });
    screen.wait_until("slot command results rendered", |renders| {
        field(&renders[&(slots[0], client_id)], "self_commands").contains("Err")
    });
    let expected: HashSet<PaneId> = slots
        .iter()
        .map(|slot_id| PaneId::Plugin(*slot_id))
        .collect();
    let deadline = Instant::now() + Duration::from_secs(30);
    while screen.collapsed != expected && Instant::now() < deadline {
        screen.receive_one(Duration::from_millis(100));
    }
    teardown();
    assert_eq!(
        screen.collapsed, expected,
        "every slot of the plugin is collapsed, from outside of render"
    );
    let results = field(&screen.renders[&(slots[0], client_id)], "self_commands");
    assert!(
        results.starts_with("[\"Ok(())\", \"Ok(())\""),
        "own slots are accepted: {}",
        results
    );
    assert!(
        results.contains("does not belong"),
        "foreign slot is rejected: {}",
        results
    );
}

#[test]
#[ignore]
pub fn legacy_plugin_keeps_one_instance_per_client() {
    let (sender, mut screen, teardown, _temp_folder) = start();
    let _ = sender.send(PluginInstruction::AddClient(1));
    let _ = sender.send(PluginInstruction::AddClient(2));
    load(&sender, legacy_fixture(), 0, 1);
    let _ = sender.send(PluginInstruction::Update(vec![(
        None,
        None,
        Event::InputReceived,
    )]));
    screen.wait_until("legacy plugin rendered for both clients", |renders| {
        slots_of_client(renders, 1).len() == 1 && slots_of_client(renders, 2).len() == 1
    });
    let plugin_id = slots_of_client(&screen.renders, 1)[0];
    let _ = sender.send(PluginInstruction::Update(vec![(
        Some(plugin_id),
        Some(1),
        key_event(),
    )]));
    screen.wait_until("client 1 instance received the key", |renders| {
        renders[&(plugin_id, 1)].contains("Key(")
    });
    screen.settle(Duration::from_millis(300));
    teardown();
    assert!(
        !screen.renders[&(plugin_id, 2)].contains("Key("),
        "the other client's instance did not receive the key"
    );
    assert!(screen.renders[&(plugin_id, 1)].starts_with("Rows: "));
}

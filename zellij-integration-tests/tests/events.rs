#![cfg(unix)]

use serde_json::Value;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, split_right_and_wait_for_prompt, start_zellij,
    CliOutputHandle, FakePtyHandle, TestSession, PROMPT,
};
use zellij_utils::data::InputMode;
use zellij_utils::input::actions::Action;

fn parse_events(stdout: &str) -> Vec<Value> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("each line is a json event"))
        .collect()
}

fn wait_for_event(
    events: &CliOutputHandle,
    what: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Value {
    let stdout = events.wait_until_stdout(what, |stdout| {
        stdout
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .any(|event| predicate(&event))
    });
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|event| predicate(event))
        .unwrap()
}

fn is_event(event: &Value, name: &str) -> bool {
    event["event"] == name
}

fn start_listening(zellij: &TestSession, args: &[&str]) -> CliOutputHandle {
    let events = zellij.run_events(args);
    if !args.contains(&"--no-initial-state") {
        wait_for_event(&events, "initial state", |e| is_event(e, "initial_state"));
    }
    events
}

fn first_terminal_pane_id(events: &CliOutputHandle) -> String {
    let initial_state = wait_for_event(&events, "initial state", |e| is_event(e, "initial_state"));
    initial_state["panes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pane| pane["is_plugin"] == false)
        .map(|pane| pane["pane_id"].as_str().unwrap().to_owned())
        .expect("a terminal pane in the initial state")
}

fn opened_pane_id(event: &Value) -> String {
    event["pane"]["pane_id"].as_str().unwrap().to_owned()
}

fn osc7(path: &str) -> Vec<u8> {
    format!("\u{1b}]7;file://localhost{}\u{1b}\\", path).into_bytes()
}

fn open_new_tab_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    terminal
}

fn quit_and_collect_events(mut zellij: TestSession, events: CliOutputHandle) -> Vec<Value> {
    zellij.quit();
    parse_events(&events.wait_for_exit().stdout)
}

#[test]
fn initial_state_describes_the_session() {
    let zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let events = zellij.run_events(&[]);

    let initial_state = wait_for_event(&events, "initial state", |e| is_event(e, "initial_state"));

    assert_eq!(initial_state["session_name"], zellij.session_name());
    assert_eq!(initial_state["tabs"].as_array().unwrap().len(), 1);
    let clients = initial_state["clients"].as_array().unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0]["mode"], "normal");
    assert!(clients[0]["pane_id"]
        .as_str()
        .unwrap()
        .starts_with("terminal_"));
    quit_and_collect_events(zellij, events);
}

#[test]
fn splitting_a_pane_streams_mode_open_and_focus_events() {
    let zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let events = start_listening(&zellij, &[]);
    let first_pane_id = first_terminal_pane_id(&events);

    split_right_and_wait_for_prompt(&zellij);

    let opened = wait_for_event(&events, "pane opened", |e| is_event(e, "pane_opened"));
    let new_pane_id = opened_pane_id(&opened);
    let focused = wait_for_event(&events, "new pane focused", |e| {
        is_event(e, "pane_focused") && e["pane_id"] == new_pane_id.as_str()
    });
    assert_eq!(focused["previous_pane_id"], first_pane_id.as_str());
    wait_for_event(&events, "pane mode entered", |e| {
        is_event(e, "mode_changed") && e["mode"] == "pane"
    });
    wait_for_event(&events, "back to normal mode", |e| {
        is_event(e, "mode_changed") && e["mode"] == "normal" && e["previous_mode"] == "pane"
    });

    let all_events = quit_and_collect_events(zellij, events);
    let opened_count = all_events
        .iter()
        .filter(|e| is_event(e, "pane_opened") && e["pane"]["pane_id"] == new_pane_id.as_str())
        .count();
    assert_eq!(opened_count, 1, "{:?}", all_events);
    assert!(is_event(all_events.last().unwrap(), "session_ending"));
}

#[test]
fn new_tab_streams_tab_created_and_tab_focused() {
    let zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let events = start_listening(&zellij, &[]);

    open_new_tab_and_wait_for_prompt(&zellij);

    let created = wait_for_event(&events, "tab created", |e| is_event(e, "tab_created"));
    let new_tab_id = created["tab"]["tab_id"].clone();
    wait_for_event(&events, "new tab focused", |e| {
        is_event(e, "tab_focused") && e["tab_id"] == new_tab_id
    });
    let all_events = quit_and_collect_events(zellij, events);
    let created_index = all_events
        .iter()
        .position(|e| is_event(e, "tab_created"))
        .unwrap();
    let opened_index = all_events
        .iter()
        .position(|e| is_event(e, "pane_opened") && e["pane"]["tab_id"] == new_tab_id)
        .expect("the new tab's pane was reported");
    assert!(created_index < opened_index);
}

#[test]
fn type_filter_only_emits_the_requested_events() {
    let zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let events = start_listening(&zellij, &["--type", "tab_created"]);

    open_new_tab_and_wait_for_prompt(&zellij);
    wait_for_event(&events, "tab created", |e| is_event(e, "tab_created"));

    let all_events = quit_and_collect_events(zellij, events);
    let names: Vec<&str> = all_events
        .iter()
        .map(|e| e["event"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["initial_state", "tab_created"]);
}

#[test]
fn pane_filter_only_emits_events_about_that_pane() {
    let zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let probe = start_listening(&zellij, &[]);
    let first_pane_id = first_terminal_pane_id(&probe);
    let events = start_listening(&zellij, &["--pane-id", &first_pane_id]);

    let second_terminal = split_right_and_wait_for_prompt(&zellij);
    second_terminal.output(&osc7("/second"));
    wait_for_event(&probe, "second pane cwd", |e| {
        is_event(e, "pane_cwd_changed") && e["cwd"] == "/second"
    });
    first_terminal.output(&osc7("/first"));
    wait_for_event(&events, "first pane cwd", |e| {
        is_event(e, "pane_cwd_changed") && e["cwd"] == "/first"
    });

    zellij_quit_with_two_listeners(zellij, probe, events, |all_events| {
        assert!(
            all_events
                .iter()
                .filter(|e| e.get("pane_id").is_some())
                .all(|e| e["pane_id"] == first_pane_id.as_str()),
            "{:?}",
            all_events
        );
        assert!(
            !all_events
                .iter()
                .any(|e| is_event(e, "pane_opened") || e["cwd"] == "/second"),
            "{:?}",
            all_events
        );
    });
}

fn zellij_quit_with_two_listeners(
    mut zellij: TestSession,
    probe: CliOutputHandle,
    events: CliOutputHandle,
    check_filtered: impl Fn(&[Value]),
) {
    zellij.quit();
    probe.wait_for_exit();
    check_filtered(&parse_events(&events.wait_for_exit().stdout));
}

#[test]
fn cwd_reported_by_the_shell_is_streamed_with_its_tab() {
    let zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let events = start_listening(&zellij, &[]);
    let pane_id = first_terminal_pane_id(&events);

    terminal.output(&osc7("/tmp/somewhere"));

    let cwd_changed = wait_for_event(&events, "cwd changed", |e| {
        is_event(e, "pane_cwd_changed") && e["cwd"] == "/tmp/somewhere"
    });
    assert_eq!(cwd_changed["pane_id"], pane_id.as_str());
    assert!(cwd_changed["tab_id"].is_u64());
    quit_and_collect_events(zellij, events);
}

#[test]
fn focus_next_pane_is_streamed() {
    let zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let events = start_listening(&zellij, &[]);
    let first_pane_id = first_terminal_pane_id(&events);
    split_right_and_wait_for_prompt(&zellij);
    let opened = wait_for_event(&events, "pane opened", |e| is_event(e, "pane_opened"));
    let second_pane_id = opened_pane_id(&opened);

    zellij.main_client().send_action(Action::FocusNextPane);

    let focused = wait_for_event(&events, "focus moved back to the first pane", |e| {
        is_event(e, "pane_focused") && e["pane_id"] == first_pane_id.as_str()
    });
    assert_eq!(focused["previous_pane_id"], second_pane_id.as_str());
    quit_and_collect_events(zellij, events);
}

#[test]
fn cancelling_a_pane_rename_streams_the_restored_title() {
    let zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let events = start_listening(&zellij, &[]);
    let pane_id = first_terminal_pane_id(&events);
    let client = zellij.main_client();

    client.send_action(Action::SwitchToMode {
        input_mode: InputMode::RenamePane,
    });
    wait_for_event(&events, "rename pane mode entered", |e| {
        is_event(e, "mode_changed") && e["mode"] == "renamepane"
    });
    client.send_action(Action::PaneNameInput {
        input: b"draft".to_vec(),
    });
    let renamed = wait_for_event(&events, "pane renamed to the draft", |e| {
        is_event(e, "pane_renamed") && e["title"] == "draft"
    });
    let original_title = renamed["previous_title"].as_str().unwrap().to_owned();
    client.send_action(Action::UndoRenamePane);

    let restored = wait_for_event(&events, "rename undone", |e| {
        is_event(e, "pane_renamed") && e["previous_title"] == "draft"
    });
    assert_eq!(restored["pane_id"], pane_id.as_str());
    assert_eq!(restored["title"], original_title.as_str());
    quit_and_collect_events(zellij, events);
}

#[test]
fn closing_the_last_pane_ends_the_stream_with_session_ending() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let events = start_listening(&zellij, &[]);
    let pane_id = first_terminal_pane_id(&events);

    terminal.exit(Some(0));
    zellij.wait_for_main_client_to_exit();

    let all_events = parse_events(&events.wait_for_exit().stdout);
    assert!(
        is_event(all_events.last().unwrap(), "session_ending"),
        "{:?}",
        all_events
    );
    assert!(
        all_events
            .iter()
            .any(|e| is_event(e, "pane_closed") && e["pane_id"] == pane_id.as_str()),
        "{:?}",
        all_events
    );
    zellij.quit();
}

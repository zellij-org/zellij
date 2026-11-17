use super::{diff_snapshots, SessionSnapshot};
use zellij_utils::data::PaneId;
use zellij_utils::session_events::{CliClientState, CliEvent, CliPaneState, CliTabState};

fn tab(tab_id: usize, position: usize, name: &str) -> CliTabState {
    CliTabState {
        tab_id,
        position,
        name: name.to_owned(),
        is_fullscreen_active: false,
        is_sync_panes_active: false,
        are_floating_panes_visible: false,
    }
}

fn pane(id: u32, tab_id: usize, title: &str) -> CliPaneState {
    CliPaneState {
        pane_id: PaneId::Terminal(id),
        tab_id,
        title: title.to_owned(),
        is_plugin: false,
        is_floating: false,
        is_fullscreen: false,
        is_suppressed: false,
        is_selectable: true,
        exited: false,
        exit_status: None,
        terminal_command: None,
        plugin_url: None,
    }
}

fn client(client_id: u32, tab_id: usize, pane_id: u32, mode: &str) -> CliClientState {
    CliClientState {
        client_id,
        is_web_client: false,
        tab_id: Some(tab_id),
        pane_id: Some(PaneId::Terminal(pane_id)),
        mode: Some(mode.to_owned()),
    }
}

fn snapshot(
    tabs: Vec<CliTabState>,
    panes: Vec<CliPaneState>,
    clients: Vec<CliClientState>,
) -> SessionSnapshot {
    SessionSnapshot {
        session_name: "session".to_owned(),
        tabs,
        panes,
        clients,
    }
}

#[test]
fn identical_snapshots_produce_no_events() {
    let s = snapshot(
        vec![tab(0, 0, "Tab #1")],
        vec![pane(1, 0, "a")],
        vec![client(1, 0, 1, "normal")],
    );
    assert!(diff_snapshots(&s, &s).is_empty());
}

#[test]
fn new_tab_with_pane_and_focus_move() {
    let prev = snapshot(
        vec![tab(0, 0, "Tab #1")],
        vec![pane(1, 0, "a")],
        vec![client(1, 0, 1, "normal")],
    );
    let next = snapshot(
        vec![tab(0, 0, "Tab #1"), tab(1, 1, "Tab #2")],
        vec![pane(1, 0, "a"), pane(2, 1, "b")],
        vec![client(1, 1, 2, "normal")],
    );
    let events = diff_snapshots(&prev, &next);
    assert_eq!(
        events,
        vec![
            CliEvent::TabCreated {
                tab: tab(1, 1, "Tab #2")
            },
            CliEvent::PaneOpened {
                pane: pane(2, 1, "b")
            },
            CliEvent::TabFocused {
                client_id: 1,
                tab_id: 1,
                previous_tab_id: Some(0),
            },
            CliEvent::PaneFocused {
                client_id: 1,
                pane_id: PaneId::Terminal(2),
                tab_id: 1,
                previous_pane_id: Some(PaneId::Terminal(1)),
            },
        ]
    );
}

#[test]
fn closing_a_tab_closes_its_panes_first_without_spurious_moves() {
    let prev = snapshot(
        vec![tab(0, 0, "one"), tab(1, 1, "two"), tab(2, 2, "three")],
        vec![pane(1, 0, "a"), pane(2, 1, "b"), pane(3, 2, "c")],
        vec![],
    );
    let next = snapshot(
        vec![tab(0, 0, "one"), tab(2, 1, "three")],
        vec![pane(1, 0, "a"), pane(3, 2, "c")],
        vec![],
    );
    let events = diff_snapshots(&prev, &next);
    assert_eq!(
        events,
        vec![
            CliEvent::PaneClosed {
                pane_id: PaneId::Terminal(2),
                tab_id: Some(1),
            },
            CliEvent::TabClosed {
                tab_id: 1,
                name: "two".to_owned(),
            },
        ]
    );
}

#[test]
fn reordering_tabs_emits_tab_moved() {
    let prev = snapshot(vec![tab(0, 0, "one"), tab(1, 1, "two")], vec![], vec![]);
    let next = snapshot(vec![tab(1, 0, "two"), tab(0, 1, "one")], vec![], vec![]);
    let events = diff_snapshots(&prev, &next);
    assert_eq!(
        events,
        vec![
            CliEvent::TabMoved {
                tab_id: 1,
                position: 0,
                previous_position: 1,
            },
            CliEvent::TabMoved {
                tab_id: 0,
                position: 1,
                previous_position: 0,
            },
        ]
    );
}

#[test]
fn pane_property_changes() {
    let prev = snapshot(
        vec![tab(0, 0, "one"), tab(1, 1, "two")],
        vec![pane(1, 0, "a")],
        vec![],
    );
    let mut changed = pane(1, 1, "renamed");
    changed.is_floating = true;
    changed.is_fullscreen = true;
    changed.exited = true;
    changed.exit_status = Some(3);
    let next = snapshot(
        vec![tab(0, 0, "one"), tab(1, 1, "two renamed")],
        vec![changed],
        vec![],
    );
    let events = diff_snapshots(&prev, &next);
    assert_eq!(
        events,
        vec![
            CliEvent::PaneMoved {
                pane_id: PaneId::Terminal(1),
                tab_id: 1,
                previous_tab_id: 0,
            },
            CliEvent::PaneRenamed {
                pane_id: PaneId::Terminal(1),
                tab_id: 1,
                title: "renamed".to_owned(),
                previous_title: "a".to_owned(),
            },
            CliEvent::PaneFloatingChanged {
                pane_id: PaneId::Terminal(1),
                tab_id: 1,
                is_floating: true,
            },
            CliEvent::PaneFullscreenChanged {
                pane_id: PaneId::Terminal(1),
                tab_id: 1,
                is_fullscreen: true,
            },
            CliEvent::PaneExited {
                pane_id: PaneId::Terminal(1),
                tab_id: 1,
                exit_status: Some(3),
            },
            CliEvent::TabRenamed {
                tab_id: 1,
                name: "two renamed".to_owned(),
                previous_name: "two".to_owned(),
            },
        ]
    );
}

#[test]
fn clients_attach_detach_and_change_mode() {
    let prev = snapshot(
        vec![tab(0, 0, "one")],
        vec![pane(1, 0, "a")],
        vec![client(1, 0, 1, "normal"), client(2, 0, 1, "normal")],
    );
    let next = snapshot(
        vec![tab(0, 0, "one")],
        vec![pane(1, 0, "a")],
        vec![client(1, 0, 1, "locked"), client(3, 0, 1, "normal")],
    );
    let events = diff_snapshots(&prev, &next);
    assert_eq!(
        events,
        vec![
            CliEvent::ModeChanged {
                client_id: 1,
                mode: "locked".to_owned(),
                previous_mode: Some("normal".to_owned()),
            },
            CliEvent::ClientAttached {
                client_id: 3,
                is_web_client: false,
            },
            CliEvent::TabFocused {
                client_id: 3,
                tab_id: 0,
                previous_tab_id: None,
            },
            CliEvent::PaneFocused {
                client_id: 3,
                pane_id: PaneId::Terminal(1),
                tab_id: 0,
                previous_pane_id: None,
            },
            CliEvent::ModeChanged {
                client_id: 3,
                mode: "normal".to_owned(),
                previous_mode: None,
            },
            CliEvent::ClientDetached { client_id: 2 },
        ]
    );
}

#[test]
fn session_rename() {
    let prev = snapshot(vec![], vec![], vec![]);
    let mut next = prev.clone();
    next.session_name = "renamed".to_owned();
    assert_eq!(
        diff_snapshots(&prev, &next),
        vec![CliEvent::SessionRenamed {
            name: "renamed".to_owned(),
            previous_name: "session".to_owned(),
        }]
    );
}

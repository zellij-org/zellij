#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    keys, normalized, LayoutInfo, TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};

const RESURRECT_LAYOUT: &str = r#"
layout {
    default_tab_template {
        pane size=1 borderless=true {
            plugin location="tab-bar"
        }
        children
        pane size=1 borderless=true {
            plugin location="status-bar"
        }
    }
    tab name="alpha" {
        pane
    }
    tab name="beta" focus=true {
        pane
    }
}
"#;

fn start_serializing_session(extra_config: &str) -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config(&format!("session_serialization true\n{}", extra_config))
        .with_layout(LayoutInfo::Stringified(RESURRECT_LAYOUT.to_string()))
        .start()
}

fn wait_for_layout_loaded(zellij: &TestSession) {
    zellij.wait_until("layout loaded with both tabs", |grid_snapshot| {
        grid_snapshot.contains("alpha")
            && grid_snapshot.contains("beta")
            && grid_snapshot.status_bar_appears()
    });
}

#[test]
fn quit_and_resurrect_session() {
    let mut zellij = start_serializing_session("");
    wait_for_layout_loaded(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let new_pane = zellij.expect_pty_spawn();
    new_pane.output(PROMPT);
    zellij.wait_until("new pane opened before serialization", |grid_snapshot| {
        grid_snapshot.contains("│")
    });

    zellij.save_session();
    zellij.wait_for_serialized_session();
    zellij.quit();

    zellij.resurrect(TERMINAL_SIZE);
    let grid_snapshot = zellij.wait_until(
        "resurrected session restored the runtime pane",
        |grid_snapshot| {
            grid_snapshot.contains("Zellij (test")
                && grid_snapshot.contains("alpha")
                && grid_snapshot.contains("beta")
                && grid_snapshot.contains("│")
                && grid_snapshot.status_bar_appears()
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn quit_and_resurrect_session_with_viewport_serialization() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("session_serialization true\nserialize_pane_viewport true")
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(b"VIEWPORT_CONTENT_TO_RESTORE\n");
    zellij.wait_until(
        "pane content rendered before serialization",
        |grid_snapshot| {
            grid_snapshot.contains("VIEWPORT_CONTENT_TO_RESTORE")
                && grid_snapshot.status_bar_appears()
        },
    );

    zellij.save_session();
    zellij.wait_for_serialized_session();
    zellij.quit();

    zellij.resurrect(TERMINAL_SIZE);
    let grid_snapshot =
        zellij.wait_until("resurrected viewport and chrome settled", |grid_snapshot| {
            grid_snapshot.contains("VIEWPORT_CONTENT_TO_RESTORE")
                && grid_snapshot.contains("Zellij (test")
                && grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

const PRE_BARS_PLUGIN_ALIASES: &str = r#"
plugins {
    tab-bar location="zellij:tab-bar"
    status-bar location="zellij:status-bar"
    compact-bar location="zellij:compact-bar"
}
"#;

fn assert_bars_stay_in_place_after_closing_a_resurrected_pane(config: &str, save_explicitly: bool) {
    let mut zellij = TestRunner::new(TERMINAL_SIZE).with_config(config).start();
    let first = zellij.expect_pty_spawn();
    first.output(PROMPT);
    for _ in 0..2 {
        zellij.send_stdin(&keys::ctrl('p'));
        zellij.send_stdin(&keys::key('n'));
        let pane = zellij.expect_pty_spawn();
        pane.output(PROMPT);
    }
    zellij.wait_until("three panes are open", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains("Pane #3")
    });

    if save_explicitly {
        zellij.save_session();
    }
    zellij.wait_for_serialized_session();
    zellij.quit();

    zellij.resurrect(TERMINAL_SIZE);
    zellij.wait_until("the resurrected session is drawn", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Pane #3")
    });
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('h'));
    zellij.send_stdin(&keys::key('x'));

    let grid_snapshot = zellij.wait_until("the first pane is closed", |grid_snapshot| {
        grid_snapshot.text.matches("Pane #").count() < 3
    });
    assert_eq!(
        grid_snapshot.row_of_line("Tab #1"),
        Some(0),
        "tab bar is not on the first row:\n{}",
        grid_snapshot
    );
    assert_eq!(
        grid_snapshot.row_of_line("LOCK"),
        Some(TERMINAL_SIZE.rows - 1),
        "status bar is not on the last row:\n{}",
        grid_snapshot
    );
    zellij.quit();
}

#[test]
fn closing_a_pane_after_resurrection_keeps_the_bars_in_place() {
    assert_bars_stay_in_place_after_closing_a_resurrected_pane("session_serialization true", true);
}

#[test]
fn closing_a_pane_after_periodic_resurrection_keeps_the_bars_in_place() {
    assert_bars_stay_in_place_after_closing_a_resurrected_pane(
        "session_serialization true\nserialization_interval 1\ndisable_session_metadata false",
        false,
    );
}

#[test]
fn closing_a_pane_after_resurrection_with_pre_bars_aliases_keeps_the_bars_in_place() {
    assert_bars_stay_in_place_after_closing_a_resurrected_pane(
        &format!("session_serialization true\n{}", PRE_BARS_PLUGIN_ALIASES),
        true,
    );
}

#[test]
fn closing_a_pane_after_periodic_resurrection_with_pre_bars_aliases_keeps_the_bars_in_place() {
    assert_bars_stay_in_place_after_closing_a_resurrected_pane(
        &format!(
            "session_serialization true\nserialization_interval 1\ndisable_session_metadata false\n{}",
            PRE_BARS_PLUGIN_ALIASES
        ),
        false,
    );
}

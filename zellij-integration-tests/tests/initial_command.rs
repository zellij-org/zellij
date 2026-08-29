#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    col, normalized, FakePtyHandle, LayoutInfo, TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};
use zellij_utils::input::command::TerminalAction;

const TWO_TABS_SECOND_FOCUSED: &str = r#"
layout {
    tab name="first"
    tab name="second" focus=true
}
"#;

fn command_line_of(terminal: &FakePtyHandle) -> Vec<String> {
    match terminal.terminal_action() {
        Some(TerminalAction::RunCommand(run_command)) => {
            let mut command_line = vec![run_command.command.display().to_string()];
            command_line.extend(run_command.args);
            command_line
        },
        other => panic!("expected a command pane, got {:?}", other),
    }
}

fn runs_initial_command(terminal: &FakePtyHandle) -> bool {
    command_line_of(terminal)
        .first()
        .is_some_and(|command| command.ends_with("initial-command"))
}

fn wait_for_loaded_app(zellij: &TestSession) {
    zellij.wait_until("app loaded", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });
}

#[test]
fn a_session_started_with_an_initial_command_runs_it_in_its_first_pane() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_initial_command(&["initial-command", "--flag", "value"])
        .start();

    let initial_terminal = zellij.expect_pty_spawn();
    assert_eq!(
        command_line_of(&initial_terminal),
        vec!["initial-command", "--flag", "value"],
        "the initial command is spawned instead of the default shell"
    );

    initial_terminal.output(b"initial-command output\r\n");
    let grid_snapshot = zellij.wait_until("initial command output rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains("initial-command output")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn an_initial_command_pane_is_held_open_when_the_command_exits() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_initial_command(&["initial-command"])
        .start();

    let initial_terminal = zellij.expect_pty_spawn();
    wait_for_loaded_app(&zellij);

    initial_terminal.exit(Some(42));
    let grid_snapshot = zellij.wait_until("exited command pane held open", |grid_snapshot| {
        grid_snapshot.contains("EXIT CODE: 42")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn an_initial_command_lands_in_the_focused_tab_of_a_layout() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_layout(LayoutInfo::Stringified(TWO_TABS_SECOND_FOCUSED.to_string()))
        .with_initial_command(&["initial-command"])
        .start();

    let first_tab_terminal = zellij.expect_pty_spawn();
    let second_tab_terminal = zellij.expect_pty_spawn();
    first_tab_terminal.output(b"unfocused tab shell\r\n");
    second_tab_terminal.output(b"initial-command output\r\n");

    assert!(
        !runs_initial_command(&first_tab_terminal),
        "the unfocused tab keeps its default shell"
    );
    assert_eq!(
        command_line_of(&second_tab_terminal),
        vec!["initial-command"],
        "the focused tab runs the initial command"
    );

    let grid_snapshot = zellij
        .wait_until("focused tab shows the command output", |grid_snapshot| {
            grid_snapshot.contains("initial-command output")
        });
    assert!(
        !grid_snapshot.contains("unfocused tab shell"),
        "the unfocused tab is not the one being displayed"
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn attaching_to_a_background_session_shows_its_initial_command() {
    let background_session = TestRunner::new(TERMINAL_SIZE)
        .with_initial_command(&["initial-command"])
        .start_in_background();

    let initial_terminal = background_session.expect_pty_spawn();
    assert_eq!(
        command_line_of(&initial_terminal),
        vec!["initial-command"],
        "the background session runs the initial command without a client attached"
    );
    initial_terminal.output(b"output while detached\r\n");

    let mut zellij = background_session.attach(TERMINAL_SIZE);
    let grid_snapshot = zellij.wait_until(
        "attached client renders the initial command output",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears() && grid_snapshot.contains("output while detached")
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));

    initial_terminal.output(b"output while attached\r\n");
    zellij.wait_until(
        "attached client renders live command output",
        |grid_snapshot| grid_snapshot.contains("output while attached"),
    );
    zellij.quit();
}

#[test]
fn a_session_started_without_an_initial_command_runs_the_default_shell() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE).start();

    let first_terminal = zellij.expect_pty_spawn();
    assert!(
        !runs_initial_command(&first_terminal),
        "the first pane is the default shell"
    );

    first_terminal.output(PROMPT);
    zellij.wait_until("default shell prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
    });
    zellij.quit();
}

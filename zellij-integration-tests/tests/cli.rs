#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    col, keys, normalized, FakePtyHandle, Size, TestRunner, TestSession,
};

const TERMINAL_SIZE: Size = Size {
    cols: 120,
    rows: 24,
};
const PROMPT: &[u8] = b"$ ";

fn start_zellij() -> TestSession {
    TestRunner::new(TERMINAL_SIZE).start()
}

fn claim_first_terminal_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(col(3).row(2))
    });
    terminal
}

#[test]
fn override_layout_from_default_to_compact() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.override_layout("compact");

    let grid_snapshot =
        zellij.wait_until("compact bar rendered with session name", |grid_snapshot| {
            grid_snapshot.contains("Zellij (test")
                && grid_snapshot.contains("NORMAL")
                && grid_snapshot.contains("Tab #1")
                && !grid_snapshot.status_bar_appears()
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn send_command_through_the_cli() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.run_suspended_command(&["suspended-command"]);
    let command_terminal = zellij.expect_pty_spawn();
    zellij.wait_until("suspended command pane waiting to run", |grid_snapshot| {
        grid_snapshot.contains("<Ctrl-c>")
    });

    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("command running", |grid_snapshot| {
        !grid_snapshot.contains("<Ctrl-c>")
    });
    command_terminal.output(b"foo\r\n");
    zellij.wait_until("first run printed foo", |grid_snapshot| {
        grid_snapshot.contains("foo")
    });
    command_terminal.exit(Some(0));
    zellij.wait_until("command pane held again after exit", |grid_snapshot| {
        grid_snapshot.contains("EXIT CODE")
    });

    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("command re-running", |grid_snapshot| {
        !grid_snapshot.contains("EXIT CODE")
    });
    command_terminal.output(b"foo\r\nfoo\r\n");
    let grid_snapshot = zellij.wait_until("command re-ran", |grid_snapshot| {
        grid_snapshot.text.matches("foo").count() >= 2
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn send_blocking_command_through_the_cli() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let blocking_command = zellij.run_blocking_floating_command(&["blocking-command"]);
    let floating_terminal = zellij.expect_pty_spawn();
    zellij.wait_until("floating command pane appeared", |grid_snapshot| {
        grid_snapshot.contains("PIN")
    });

    floating_terminal.exit(Some(42));
    assert_eq!(blocking_command.wait_for_exit(), 42);

    let grid_snapshot = zellij.wait_until("floating pane closed on exit", |grid_snapshot| {
        !grid_snapshot.contains("PIN") && grid_snapshot.cursor_is_at(col(3).row(2))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn watcher_client_functionality() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    first_terminal.output(b"\r\nWATCHER_OUTPUT_1\r\n");
    zellij.wait_until("first output line", |grid_snapshot| {
        grid_snapshot.contains("WATCHER_OUTPUT_1")
    });

    let watcher = zellij.attach_watcher(TERMINAL_SIZE);
    watcher.wait_until("watcher connected and sees the output", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains("WATCHER_OUTPUT_1")
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);
    watcher.wait_until("watcher sees the split", |grid_snapshot| {
        grid_snapshot.contains("┐┌")
    });

    let ignored_new_tab_from_watcher = [keys::ctrl('t'), keys::key('n')].concat();
    watcher.send_stdin(&ignored_new_tab_from_watcher);

    zellij.detach_main_client();
    first_terminal.output(b"WATCHER_OUTPUT_2\r\nWATCHER_OUTPUT_3\r\nWATCHER_DONE\r\n");
    watcher.wait_until(
        "watcher keeps receiving output while no main client is attached",
        |grid_snapshot| {
            grid_snapshot.contains("WATCHER_DONE")
                && grid_snapshot.contains("┐┌")
                && !grid_snapshot.contains("Tab #2")
        },
    );

    let main = zellij.attach_client(TERMINAL_SIZE);
    let main_snapshot = main.wait_until(
        "re-attached main converges on the same state",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("WATCHER_DONE")
                && grid_snapshot.contains("┐┌")
        },
    );
    assert_snapshot!(normalized(&main_snapshot));

    let watcher_snapshot =
        watcher.wait_until("watcher mirrors the re-attached session", |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("WATCHER_DONE")
                && grid_snapshot.contains("┐┌")
                && !grid_snapshot.contains("Tab #2")
        });
    assert_snapshot!(normalized(&watcher_snapshot));

    main.quit();
    watcher.quit();
    zellij.quit();
}

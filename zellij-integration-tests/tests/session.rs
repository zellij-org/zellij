#![cfg(unix)]

use zellij_integration_tests::{col, keys, FakePtyHandle, Size, TestRunner, TestSession};

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
    zellij.wait_until(
        "first terminal prompt rendered in loaded app",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(3).row(2))
        },
    );
    terminal
}

#[test]
fn launch_session_manager_plugin() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('w'));

    zellij.wait_until("session manager plugin opened", |grid_snapshot| {
        grid_snapshot.contains("Session Manager") && grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
fn launch_about_plugin() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('a'));

    zellij.wait_until("about plugin opened", |grid_snapshot| {
        grid_snapshot.contains("About Zellij") && grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

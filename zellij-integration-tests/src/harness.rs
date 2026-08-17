use crate::{col, keys, FakePtyHandle, Size, TestRunner, TestSession};

pub const TERMINAL_SIZE: Size = Size {
    cols: 120,
    rows: 24,
};
pub const PROMPT: &[u8] = b"$ ";

pub fn start_zellij() -> TestSession {
    TestRunner::new(TERMINAL_SIZE).start()
}

pub fn claim_first_terminal_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until(
        "first terminal prompt rendered in loaded app",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(2).row(1))
        },
    );
    terminal
}

pub fn split_right_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("right terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(2))
    });
    terminal
}

pub fn split_down_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('d'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("lower terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(13))
    });
    terminal
}

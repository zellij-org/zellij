#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, split_right_and_wait_for_prompt, start_zellij,
    FakePtyHandle, GridSnapshot, TestSession,
};

const LAST_LINE: &str = "line40";

fn tab_line_shows_scroll(grid_snapshot: &GridSnapshot) -> bool {
    grid_snapshot
        .lines()
        .first()
        .is_some_and(|tab_bar| tab_bar.contains("SCROLL"))
}

fn fill_pane_past_viewport(zellij: &TestSession) -> FakePtyHandle {
    let terminal = claim_first_terminal_and_wait_for_prompt(zellij);
    for line in 1..=40 {
        terminal.output(format!("line{:02}\r\n", line).as_bytes());
    }
    zellij.wait_until("pane filled past its viewport", |grid_snapshot| {
        grid_snapshot.contains(LAST_LINE)
    });
    terminal
}

#[test]
fn single_pane_scroll_shows_on_the_tab_line() {
    let mut zellij = start_zellij();
    fill_pane_past_viewport(&zellij);

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::ctrl('b'));

    let grid_snapshot = zellij.wait_until(
        "scroll indicator appears on the tab line",
        |grid_snapshot| tab_line_shows_scroll(grid_snapshot) && !grid_snapshot.contains(LAST_LINE),
    );
    assert!(
        !grid_snapshot.contains("SCROLL:"),
        "the single-pane tab-line indicator uses the colon-free format:\n{}",
        grid_snapshot.text
    );

    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("back in the base mode after scrolling", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    split_right_and_wait_for_prompt(&zellij);

    zellij.wait_until(
        "tab-line scroll indicator clears once a second pane exists",
        |grid_snapshot| !tab_line_shows_scroll(grid_snapshot),
    );

    zellij.quit();
}

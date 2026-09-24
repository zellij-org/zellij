#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, FakePtyHandle, Size, TestRunner,
    TestSession, PROMPT, TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;

fn new_pane_action(stacked: bool, no_focus: bool, tab_id: Option<usize>) -> CliAction {
    CliAction::NewPane {
        direction: None,
        command: vec![],
        plugin: None,
        cwd: None,
        floating: false,
        in_place: false,
        close_replaced_pane: false,
        pane_id: None,
        name: None,
        close_on_exit: false,
        start_suspended: false,
        configuration: None,
        skip_plugin_cache: false,
        x: None,
        y: None,
        width: None,
        height: None,
        pinned: None,
        stacked,
        blocking: false,
        block_until_exit_success: false,
        block_until_exit_failure: false,
        block_until_exit: false,
        unblock_condition: None,
        near_current_pane: false,
        no_focus,
        borderless: None,
        tab_id,
        border_style: None,
        env: vec![],
    }
}

fn open_new_tab_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("second tab opened with prompt", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Tab #2")
            && grid_snapshot.cursor_is_at(col(2).row(1))
    });
    terminal
}

#[test]
fn moving_focus_into_a_tab_whose_focused_pane_was_stacked_in_the_background() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("stacked_pane_list true")
        .start();
    let first_pane = claim_first_terminal_and_wait_for_prompt(&zellij);
    first_pane.output(b"first-tab-pane\r\n$ ");
    zellij.wait_until("first tab pane labelled", |grid_snapshot| {
        grid_snapshot.contains("first-tab-pane")
    });
    let _second_tab_pane = open_new_tab_and_wait_for_prompt(&zellij);

    let exit_code = zellij.run_cli_action(new_pane_action(true, true, Some(0)));
    assert_eq!(exit_code, 0);
    let stacked_pane = zellij.expect_pty_spawn();
    stacked_pane.output(b"stacked-in-background\r\n$ ");

    zellij.send_stdin(&keys::alt('h'));
    zellij.wait_until(
        "focus moved into the first tab and its stacked pane is shown",
        |grid_snapshot| grid_snapshot.contains("stacked-in-background"),
    );

    let probe = b"still-alive-probe";
    zellij.send_stdin(probe);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let reached_a_pane = [&first_pane, &stacked_pane].iter().any(|pane| {
            pane.stdin_bytes()
                .windows(probe.len())
                .any(|window| window == probe)
        });
        if reached_a_pane {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "input after the tab switch reached no pane in the first tab"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    zellij.quit();
}

#[test]
fn new_pane_without_room_reports_failure_to_the_cli() {
    let mut zellij = TestRunner::new(Size { cols: 8, rows: 6 }).start();
    let first_pane = zellij.expect_pty_spawn();
    first_pane.output(PROMPT);
    zellij.wait_until("first pane rendered", |grid_snapshot| {
        grid_snapshot.contains("$")
    });
    let exit_code = zellij.run_cli_action(new_pane_action(false, false, None));
    assert_ne!(
        exit_code, 0,
        "the cli reports that the pane was not created"
    );
    zellij.quit();
}

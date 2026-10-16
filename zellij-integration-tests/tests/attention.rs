#![cfg(unix)]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::time::{Duration, Instant};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, split_right_and_wait_for_prompt,
    FakePtyHandle, HostTerminal, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;

const FAST_IDLE_RULE: &str = "attention {\n    idle_activity_secs 1\n    idle_silence_secs 1\n}";

fn program_status(body: &str) -> Vec<u8> {
    format!("\u{1b}]7501;{}\u{1b}\\", body).into_bytes()
}

fn encoded(text: &str) -> String {
    STANDARD.encode(text)
}

fn position_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    position_of(haystack, needle).is_some()
}

fn start_with_two_panes(runner: TestRunner) -> (TestSession, FakePtyHandle, FakePtyHandle) {
    let zellij = runner.start();
    let left = claim_first_terminal_and_wait_for_prompt(&zellij);
    left.disable_echo();
    let right = split_right_and_wait_for_prompt(&zellij);
    right.disable_echo();
    (zellij, left, right)
}

fn focus_left_pane(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('h'));
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("focus moved to the left pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot
                .cursor
                .map(|cursor| cursor.x < 60)
                .unwrap_or(false)
    });
}

fn focus_right_pane(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('l'));
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("focus moved to the right pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot
                .cursor
                .map(|cursor| cursor.x >= 60)
                .unwrap_or(false)
    });
}

const MARKERS: [char; 7] = ['●', '✗', '✓', '◌', '•', '◐', '⊘'];

fn markers_on_screen(text: &str) -> String {
    text.lines()
        .skip(1)
        .flat_map(|line| line.chars())
        .filter(|c| MARKERS.contains(c))
        .collect()
}

fn wait_for_markers(zellij: &TestSession, expected: &str) {
    zellij.wait_until(
        &format!("the pane frames show exactly {:?}", expected),
        |grid_snapshot| markers_on_screen(&grid_snapshot.text) == expected,
    );
}

fn shell_command(terminal: &FakePtyHandle, prompt_start: &str, command: &str, end: &str) {
    terminal.output(
        format!(
            "\u{1b}]133;{}\u{1b}\\$ \u{1b}]133;B\u{1b}\\{}\u{1b}]133;C\u{1b}\\\r\n\u{1b}]133;{}\u{1b}\\",
            prompt_start, command, end
        )
        .as_bytes(),
    );
}

fn produce_steady_output(terminals: &[&FakePtyHandle], duration: Duration) {
    let start = Instant::now();
    let mut line = 0;
    while start.elapsed() < duration {
        for terminal in terminals {
            terminal.output(format!("line {}\r\n", line).as_bytes());
        }
        line += 1;
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn split_right_pane_down(zellij: &TestSession) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('d'));
    let terminal = zellij.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    zellij.wait_until(
        "the lower right terminal prompt rendered",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(13))
        },
    );
    terminal
}

fn watch(zellij: &TestSession, terminal: &FakePtyHandle) {
    zellij.run_cli_action(CliAction::WatchPane {
        pane_id: Some(format!("terminal_{}", terminal.terminal_id())),
    });
}

#[test]
fn the_program_status_query_is_answered_before_the_device_attributes() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE).start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    terminal.output(b"\x1b]7501;?\x1b\\\x1b[c");
    let stdin = terminal.wait_for_stdin("both queries were answered", |stdin| {
        contains_subslice(stdin, b"\x1b]7501;?") && contains_subslice(stdin, b"\x1b[?")
    });
    let program_status_reply = position_of(&stdin, b"\x1b]7501;?").unwrap();
    let device_attributes_reply = position_of(&stdin, b"\x1b[?").unwrap();
    assert!(program_status_reply < device_attributes_reply);
    zellij.quit();
}

#[test]
fn an_agent_reporting_its_status_is_shown_on_its_frame_until_seen() {
    let (mut zellij, agent, _other) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    agent.output(&program_status("state=working:app=claude"));
    wait_for_markers(&zellij, "◐");
    agent.output(&program_status(&format!(
        "state=blocked:kind=permission:app=claude:msg={}",
        encoded("Allow edit?")
    )));
    wait_for_markers(&zellij, "●");
    assert!(!zellij.snapshot().contains("Allow edit?"));
    agent.output(&program_status(&format!(
        "state=working:app=claude:msg={}",
        encoded("Editing")
    )));
    wait_for_markers(&zellij, "◐");
    agent.output(&program_status(&format!(
        "state=done:app=claude:msg={}",
        encoded("All set")
    )));
    wait_for_markers(&zellij, "✓");
    focus_left_pane(&zellij);
    wait_for_markers(&zellij, "");
    zellij.quit();
}

#[test]
fn a_new_shell_prompt_drops_stale_working_and_blocked_reports() {
    let (mut zellij, agent, _other) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    agent.output(&program_status("state=blocked:kind=question"));
    wait_for_markers(&zellij, "●");
    agent.output(b"\x1b]133;A;k=s\x1b\\> ");
    agent.output(b"\x1b]133;P;k=r\x1b\\");
    agent.output(b"still waiting\r\n");
    zellij.wait_until("the agent kept running", |grid_snapshot| {
        grid_snapshot.contains("still waiting")
    });
    assert_eq!(markers_on_screen(&zellij.snapshot().text), "●");
    agent.output(b"\x1b]133;A\x1b\\$ ");
    wait_for_markers(&zellij, "");
    zellij.quit();
}

#[test]
fn fish_style_shell_reports_a_failed_command() {
    let (mut zellij, shell, _other) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    shell.output(
        b"\x1b]133;A;click_events=1\x1b\\$ \x1b]133;B\x1b\\\x1b]133;C;cmdline_url=false\x1b\\\r\n\x1b]133;D;1\x1b\\\x1b]133;A;click_events=1\x1b\\$ ",
    );
    wait_for_markers(&zellij, "✗");
    focus_left_pane(&zellij);
    wait_for_markers(&zellij, "");
    zellij.quit();
}

#[test]
fn nushell_style_shell_reports_a_failed_command_with_screen_text() {
    let (mut zellij, shell, _other) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    shell.output(
        b"\x1b]133;A;k=i;click_events=1\x1b\\> \x1b]133;P;k=r\x1b\\\x1b]133;B\x1b\\ls nope\x1b]133;C\x1b\\\r\n\x1b]133;D;2\x1b\\",
    );
    wait_for_markers(&zellij, "✗");
    zellij.quit();
}

#[test]
fn a_bell_and_a_notification_from_an_unfocused_pane_need_you() {
    let (mut zellij, left, _right) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    left.output(b"\x07");
    wait_for_markers(&zellij, "●");
    focus_left_pane(&zellij);
    wait_for_markers(&zellij, "");
    focus_right_pane(&zellij);
    left.output(b"\x1b]9;Build finished\x07");
    wait_for_markers(&zellij, "●");
    assert!(!zellij.snapshot().contains("Build finished"));
    zellij.quit();
}

#[test]
fn watching_a_pane_notifies_every_time_until_the_watch_is_turned_off() {
    let (mut zellij, shell, _right) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    watch(&zellij, &shell);
    shell_command(&shell, "A", "make", "D;0");
    wait_for_markers(&zellij, "●");
    focus_left_pane(&zellij);
    wait_for_markers(&zellij, "");
    focus_right_pane(&zellij);
    shell_command(&shell, "A", "make again", "D;0");
    wait_for_markers(&zellij, "●");
    watch(&zellij, &shell);
    focus_left_pane(&zellij);
    wait_for_markers(&zellij, "");
    focus_right_pane(&zellij);
    shell_command(&shell, "A", "make a third time", "D;0");
    shell.output(b"\x1b]133;A\x1b\\$ marker\r\n");
    zellij.wait_until("the second command finished", |grid_snapshot| {
        grid_snapshot.contains("marker")
    });
    assert_eq!(markers_on_screen(&zellij.snapshot().text), "");
    zellij.quit();
}

#[test]
fn muting_a_pane_hides_its_status() {
    let (mut zellij, left, _right) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    zellij.run_cli_action(CliAction::MutePane {
        pane_id: Some(format!("terminal_{}", left.terminal_id())),
    });
    wait_for_markers(&zellij, "⊘");
    left.output(&program_status("state=blocked:kind=auth"));
    left.output(b"report sent\r\n");
    zellij.wait_until("the blocked report arrived", |grid_snapshot| {
        grid_snapshot.contains("report sent")
    });
    assert_eq!(markers_on_screen(&zellij.snapshot().text), "⊘");
    zellij.run_cli_action(CliAction::MutePane {
        pane_id: Some(format!("terminal_{}", left.terminal_id())),
    });
    wait_for_markers(&zellij, "●");
    zellij.quit();
}

#[test]
fn a_watched_busy_pane_reports_going_quiet_but_a_pane_at_a_prompt_does_not() {
    let (mut zellij, busy, at_prompt) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(FAST_IDLE_RULE));
    at_prompt.output(b"\x1b]133;A\x1b\\$ ");
    let _focused = split_right_pane_down(&zellij);
    watch(&zellij, &busy);
    watch(&zellij, &at_prompt);
    produce_steady_output(&[&busy, &at_prompt], Duration::from_millis(3000));
    wait_for_markers(&zellij, "●");
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(markers_on_screen(&zellij.snapshot().text), "●");
    zellij.quit();
}

#[test]
fn the_host_terminal_is_told_what_the_session_needs() {
    let (mut zellij, agent, _right) = start_with_two_panes(
        TestRunner::new(TERMINAL_SIZE).with_host_terminal(HostTerminal::ProgramStatus),
    );
    agent.output(&program_status(&format!(
        "state=blocked:kind=permission:msg={}",
        encoded("Allow edit?")
    )));
    zellij.wait_until_raw_output("the host terminal got a blocked status", |raw_bytes| {
        contains_subslice(
            raw_bytes,
            b"\x1b]7501;state=blocked:app=zellij:kind=permission:msg=",
        )
    });
    zellij.quit();
}

#[test]
fn only_programs_that_report_it_are_shown_as_working() {
    let (mut zellij, left, right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config("session_indicator true"));
    left.output(
        b"\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\cargo build\x1b]133;C\x1b\\\r\nCompiling...\r\n",
    );
    right.set_foreground(&["htop"]);
    right.output(b"htop is drawing\r\n");
    right.wait_for_foreground_to_be_seen();
    left.output(b"still compiling\r\n");
    right.output(b"htop is still drawing\r\n");
    zellij.wait_until("both panes drew their output", |grid_snapshot| {
        grid_snapshot.contains("still compiling") && grid_snapshot.contains("htop is still drawing")
    });
    std::thread::sleep(Duration::from_millis(1500));
    let grid_snapshot = zellij.snapshot();
    assert_eq!(
        markers_on_screen(&grid_snapshot.text),
        "",
        "a running command or a foreground program is not working:\n{}",
        grid_snapshot.text
    );
    let indicator = grid_snapshot.lines()[0].clone();
    assert!(
        !indicator.contains('◐') && !indicator.contains("working"),
        "the indicator counts something as working: {}",
        indicator
    );
    left.output(&program_status("state=working:app=claude"));
    wait_for_markers(&zellij, "◐");
    zellij.quit();
}

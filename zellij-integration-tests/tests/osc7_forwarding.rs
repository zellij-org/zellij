#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, split_down_and_wait_for_prompt,
    start_zellij, FakePtyHandle, TestSession,
};

const OSC7_OPENER: &[u8] = b"\x1b]7;";

fn osc7(uri: &str) -> Vec<u8> {
    format!("\u{1b}]7;{}\u{1b}\\", uri).into_bytes()
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    occurrences_of(haystack, needle) > 0
}

fn occurrences_of(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

fn claim_quiet_terminal(zellij: &TestSession) -> FakePtyHandle {
    let terminal = claim_first_terminal_and_wait_for_prompt(zellij);
    terminal.disable_echo();
    terminal
}

fn split_down_to_quiet_terminal(zellij: &TestSession) -> FakePtyHandle {
    let terminal = split_down_and_wait_for_prompt(zellij);
    terminal.disable_echo();
    terminal
}

fn report_cwd_and_wait(zellij: &TestSession, terminal: &FakePtyHandle, uri: &str) {
    terminal.output(&osc7(uri));
    zellij.wait_until_raw_output(
        &format!("the host terminal was told about {}", uri),
        |raw_bytes| contains_subslice(raw_bytes, &osc7(uri)),
    );
}

fn echo_and_wait(zellij: &TestSession, terminal: &FakePtyHandle, sentinel: &str) {
    terminal.output(format!("{}\r\n", sentinel).as_bytes());
    zellij.wait_until("the pane kept processing its output", |grid_snapshot| {
        grid_snapshot.contains(sentinel)
    });
}

fn focus_upper_pane(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('k'));
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("focus moved to the upper pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(2))
    });
}

#[test]
fn a_working_directory_report_reaches_the_host_terminal() {
    let mut zellij = start_zellij();
    let terminal = claim_quiet_terminal(&zellij);

    report_cwd_and_wait(&zellij, &terminal, "file://host/tmp");

    zellij.quit();
}

#[test]
fn an_unchanged_working_directory_is_reported_only_once() {
    let mut zellij = start_zellij();
    let terminal = claim_quiet_terminal(&zellij);

    report_cwd_and_wait(&zellij, &terminal, "file://host/tmp");
    terminal.output(&osc7("file://host/tmp"));
    echo_and_wait(&zellij, &terminal, "second-report-processed");

    assert_eq!(
        occurrences_of(&zellij.raw_bytes(), &osc7("file://host/tmp")),
        1,
        "a pane repeating its working directory does not report it again"
    );

    zellij.quit();
}

#[test]
fn a_changed_working_directory_is_reported_again() {
    let mut zellij = start_zellij();
    let terminal = claim_quiet_terminal(&zellij);

    report_cwd_and_wait(&zellij, &terminal, "file://host/first");
    report_cwd_and_wait(&zellij, &terminal, "file://host/second");

    zellij.quit();
}

#[test]
fn moving_focus_reports_the_working_directory_of_the_newly_focused_pane() {
    let mut zellij = start_zellij();
    let upper_terminal = claim_quiet_terminal(&zellij);
    report_cwd_and_wait(&zellij, &upper_terminal, "file://host/upper");

    let lower_terminal = split_down_to_quiet_terminal(&zellij);
    report_cwd_and_wait(&zellij, &lower_terminal, "file://host/lower");

    focus_upper_pane(&zellij);

    zellij.wait_until_raw_output(
        "the upper pane's working directory was restated on focus",
        |raw_bytes| occurrences_of(raw_bytes, &osc7("file://host/upper")) == 2,
    );

    zellij.quit();
}

#[test]
fn a_pane_that_never_reported_a_working_directory_reports_nothing_on_focus() {
    let mut zellij = start_zellij();
    let upper_terminal = claim_quiet_terminal(&zellij);
    report_cwd_and_wait(&zellij, &upper_terminal, "file://host/upper");

    let lower_terminal = split_down_to_quiet_terminal(&zellij);
    echo_and_wait(&zellij, &lower_terminal, "lower-pane-is-focused");

    assert_eq!(
        occurrences_of(&zellij.raw_bytes(), OSC7_OPENER),
        1,
        "focusing a pane with nothing to report leaves the host terminal as it was"
    );

    focus_upper_pane(&zellij);
    zellij.wait_until_raw_output(
        "the upper pane's working directory was restated on focus",
        |raw_bytes| occurrences_of(raw_bytes, &osc7("file://host/upper")) == 2,
    );

    zellij.quit();
}

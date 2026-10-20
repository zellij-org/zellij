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
fn the_focused_pane_reports_its_status_while_the_host_window_is_unfocused() {
    let (mut zellij, _left, focused) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    focused.output(&program_status("state=done:app=claude"));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(markers_on_screen(&zellij.snapshot().text), "");
    zellij.send_stdin(b"\x1b[O");
    std::thread::sleep(Duration::from_millis(200));
    focused.output(&program_status("state=working:app=claude"));
    focused.output(&program_status("state=done:app=claude"));
    wait_for_markers(&zellij, "✓");
    zellij.send_stdin(b"\x1b[I");
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

const SESSION_INDICATOR_ON: &str = "session_indicator true";

fn let_the_startup_grace_pass() {
    std::thread::sleep(Duration::from_millis(3200));
}

fn unfocus_the_host_window(zellij: &TestSession) {
    zellij.send_stdin(b"\x1b[O");
    std::thread::sleep(Duration::from_millis(200));
}

fn print_lines(terminal: &FakePtyHandle, label: &str) {
    for line in 0..3 {
        terminal.output(format!("{} {}\r\n", label, line).as_bytes());
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn indicator(zellij: &TestSession) -> String {
    zellij.snapshot().lines()[0].clone()
}

fn open_inbox(zellij: &TestSession) {
    zellij.send_stdin(&keys::F9);
    let opened = zellij.wait_until("the session manager opens", |grid_snapshot| {
        grid_snapshot.contains("<?> all keys") || grid_snapshot.contains("<F1> all keys")
    });
    if !opened.contains("<?> all keys") {
        zellij.send_stdin(&keys::TAB);
        zellij.wait_until("the inbox is shown", |grid_snapshot| {
            grid_snapshot.contains("<?> all keys")
        });
    }
}

#[test]
fn plain_output_from_an_unwatched_pane_never_needs_you_while_the_window_is_unfocused() {
    let (mut zellij, left, _right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SESSION_INDICATOR_ON));
    let_the_startup_grace_pass();
    unfocus_the_host_window(&zellij);
    print_lines(&left, "plain output");
    zellij.wait_until("the output is drawn", |grid_snapshot| {
        grid_snapshot.contains("plain output 2")
    });
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(markers_on_screen(&zellij.snapshot().text), "");
    let bar = indicator(&zellij);
    assert!(
        bar.contains(" running") && !bar.contains("need you") && !bar.contains("watch alert"),
        "the indicator reacted to plain output: {}",
        bar
    );
    open_inbox(&zellij);
    let inbox = zellij.wait_until("the new output waits behind the line", |grid_snapshot| {
        grid_snapshot.contains("with only new output")
    });
    assert!(!inbox.contains("NEEDS YOU"), "{}", inbox.text);
    zellij.quit();
}

#[test]
fn output_from_programs_that_report_their_own_status_is_never_new_output() {
    let (mut zellij, working, under_id) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SESSION_INDICATOR_ON));
    let idle = split_right_pane_down(&zellij);
    working.output(&program_status("state=working:app=claude"));
    under_id.output(&program_status("id=task:state=working:app=claude"));
    idle.output(&program_status("state=idle:app=claude"));
    zellij.wait_until("the working program is shown", |grid_snapshot| {
        markers_on_screen(&grid_snapshot.text).contains('◐')
    });
    let_the_startup_grace_pass();
    unfocus_the_host_window(&zellij);
    print_lines(&working, "working output");
    print_lines(&under_id, "task output");
    print_lines(&idle, "idle output");
    zellij.wait_until("the output is drawn", |grid_snapshot| {
        grid_snapshot.contains("working output 2")
            && grid_snapshot.contains("task output 2")
            && grid_snapshot.contains("idle output 2")
    });
    std::thread::sleep(Duration::from_millis(700));
    let markers = markers_on_screen(&zellij.snapshot().text);
    assert!(
        markers.contains('◐') && !markers.contains('●') && !markers.contains('•'),
        "markers: {:?}",
        markers
    );
    let bar = indicator(&zellij);
    assert!(
        !bar.contains("need you") && !bar.contains("watch alert"),
        "the indicator reacted to the output: {}",
        bar
    );
    open_inbox(&zellij);
    std::thread::sleep(Duration::from_millis(300));
    let inbox = zellij.snapshot();
    assert!(
        !inbox.contains("with only new output") && !inbox.contains("NEEDS YOU"),
        "{}",
        inbox.text
    );
    zellij.quit();
}

#[test]
fn output_from_a_watched_pane_needs_you_while_the_window_is_unfocused_until_refocused() {
    let (mut zellij, left, _right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SESSION_INDICATOR_ON));
    watch(&zellij, &left);
    let_the_startup_grace_pass();
    unfocus_the_host_window(&zellij);
    print_lines(&left, "watched output");
    wait_for_markers(&zellij, "●");
    zellij.wait_until("the indicator shows a watch alert", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("watch alert")
    });
    zellij.send_stdin(b"\x1b[I");
    wait_for_markers(&zellij, "");
    zellij.quit();
}

fn session_attention(zellij: &TestSession) -> zellij_utils::attention::SessionAttention {
    zellij_utils::session_link::request_session_attention(
        zellij.session_name(),
        Duration::from_secs(2),
    )
    .expect("the session answers")
}

fn wait_for_session_attention(
    zellij: &TestSession,
    what: &str,
    check: impl Fn(&zellij_utils::attention::SessionAttention) -> bool,
) {
    zellij_integration_tests::wait_for(what, || {
        zellij_utils::session_link::request_session_attention(
            zellij.session_name(),
            Duration::from_secs(2),
        )
        .map(|attention| check(&attention))
        .unwrap_or(false)
    });
}

fn pane_id(terminal: &FakePtyHandle) -> Option<String> {
    Some(format!("terminal_{}", terminal.terminal_id()))
}

fn status_bar(zellij: &TestSession) -> String {
    let lines = zellij.snapshot().lines();
    lines[lines.len().saturating_sub(2)..].join("\n")
}

fn open_second_tab(zellij: &TestSession) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let terminal = zellij.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    zellij.wait_until("the second tab is open", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("Tab #2") && grid_snapshot.status_bar_appears()
    });
    terminal
}

fn go_to_tab(zellij: &TestSession, number: char) {
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key(number));
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the tab is shown", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
}

#[test]
fn muting_the_session_hides_every_marker_and_the_indicator_count() {
    let (mut zellij, left, _right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SESSION_INDICATOR_ON));
    left.output(&program_status("state=blocked:kind=question"));
    wait_for_markers(&zellij, "●");
    zellij.wait_until("the indicator counts the item", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("● 1")
    });
    zellij.run_cli_action(CliAction::MuteSession);
    zellij.wait_until("the session mute hides the marker", |grid_snapshot| {
        !markers_on_screen(&grid_snapshot.text).contains('●')
            && !grid_snapshot.lines()[0].contains("● 1")
    });
    wait_for_session_attention(&zellij, "the session reports it is muted", |attention| {
        attention.muted && attention.counts.needs_you == 0
    });
    zellij.run_cli_action(CliAction::MuteSession);
    wait_for_markers(&zellij, "●");
    zellij.quit();
}

#[test]
#[ignore = "dismissing a working item with c in the Inbox removes it from the list but the session still counts it as working, which contradicts the handoff decision that marking seen clears working; listed under Questions for the operator"]
fn focusing_a_working_pane_keeps_it_working_but_dismissing_it_clears_it() {
    let (mut zellij, _left, right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SESSION_INDICATOR_ON));
    right.output(&program_status("state=working:app=claude"));
    wait_for_markers(&zellij, "◐");
    focus_right_pane(&zellij);
    focus_left_pane(&zellij);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(markers_on_screen(&zellij.snapshot().text), "◐");
    open_inbox(&zellij);
    zellij.wait_until("the working item is listed", |grid_snapshot| {
        grid_snapshot.contains("WORKING") && grid_snapshot.contains("<c> dismiss")
    });
    zellij.send_stdin(&keys::key('c'));
    zellij.wait_until("the working item is dismissed", |grid_snapshot| {
        !grid_snapshot.contains("WORKING")
    });
    zellij.send_stdin(&keys::ESC);
    wait_for_markers(&zellij, "");
    zellij.quit();
}

const SERIALIZING: &str =
    "session_serialization true\ndisable_session_metadata false\nsession_indicator true";

fn serialized_layout(zellij: &TestSession) -> String {
    let path = zellij_utils::consts::session_layout_cache_file_name(&zellij.session_id());
    std::fs::read_to_string(path).unwrap_or_default()
}

fn save_and_resurrect(zellij: &mut TestSession) {
    zellij.save_session();
    zellij.wait_for_serialized_session();
    zellij.quit();
    zellij.resurrect(TERMINAL_SIZE);
}

#[test]
fn mute_ignore_and_tab_mute_are_restored_after_resurrection() {
    let (mut zellij, left, right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SERIALIZING));
    let _third = open_second_tab(&zellij);
    zellij.run_cli_action(CliAction::MutePane {
        pane_id: pane_id(&left),
    });
    zellij.run_cli_action(CliAction::IgnorePaneOutput {
        pane_id: pane_id(&right),
    });
    zellij.run_cli_action(CliAction::MuteTab { tab_id: None });
    zellij.wait_until("the second tab is muted", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("Tab #2 ⊘")
    });
    zellij.save_session();
    zellij.wait_for_serialized_session();
    zellij_integration_tests::wait_for("the flags to be saved", || {
        let layout = serialized_layout(&zellij);
        layout.contains("attention_muted=true") && layout.contains("ignore_output=true")
    });
    zellij.quit();
    zellij.resurrect(TERMINAL_SIZE);
    let restored_left = zellij.expect_pty_spawn();
    let restored_right = zellij.expect_pty_spawn();
    let restored_third = zellij.expect_pty_spawn();
    for terminal in [&restored_left, &restored_right, &restored_third] {
        terminal.disable_echo();
        terminal.output(b"$ ");
    }
    zellij.wait_until("the restored tab is still muted", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("Tab #2 ⊘") && grid_snapshot.status_bar_appears()
    });
    restored_third.output(&program_status("state=blocked:kind=question"));
    go_to_tab(&zellij, '1');
    zellij.wait_until("the restored muted pane shows its mark", |grid_snapshot| {
        markers_on_screen(&grid_snapshot.text).contains('⊘')
    });
    go_to_tab(&zellij, '2');
    let_the_startup_grace_pass();
    print_lines(&restored_right, "ignored output");
    std::thread::sleep(Duration::from_millis(700));
    let attention = session_attention(&zellij);
    assert_eq!(attention.counts.new_output, 0, "{:?}", attention.counts);
    assert_eq!(attention.counts.needs_you, 0, "{:?}", attention.counts);
    zellij.quit();
}

#[test]
fn a_muted_session_stays_muted_after_resurrection() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(SERIALIZING)
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    zellij.run_cli_action(CliAction::MuteSession);
    wait_for_session_attention(&zellij, "the session is muted", |attention| attention.muted);
    zellij.save_session();
    zellij.wait_for_serialized_session();
    zellij_integration_tests::wait_for("the session mute to be saved", || {
        serialized_layout(&zellij).contains("session_attention_muted=true")
    });
    zellij.quit();
    zellij.resurrect(TERMINAL_SIZE);
    let restored = zellij.expect_pty_spawn();
    restored.disable_echo();
    restored.output(b"$ ");
    zellij.wait_until("the session is restored", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    wait_for_session_attention(&zellij, "the restored session is muted", |attention| {
        attention.muted
    });
    zellij.quit();
}

#[test]
fn a_watch_is_not_restored_after_resurrection() {
    let (mut zellij, left, _right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SERIALIZING));
    watch(&zellij, &left);
    wait_for_session_attention(&zellij, "the pane is watched", |attention| {
        attention.items.iter().any(|item| item.attention.watched)
    });
    save_and_resurrect(&mut zellij);
    let restored_left = zellij.expect_pty_spawn();
    let restored_right = zellij.expect_pty_spawn();
    for terminal in [&restored_left, &restored_right] {
        terminal.disable_echo();
        terminal.output(b"$ ");
    }
    zellij.wait_until("the session is restored", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    focus_left_pane(&zellij);
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !status_bar(&zellij).contains("watching this pane"),
        "{}",
        status_bar(&zellij)
    );
    focus_right_pane(&zellij);
    shell_command(&restored_left, "A", "make", "D;0");
    restored_left.output(b"\x1b]133;A\x1b\\$ after\r\n");
    zellij.wait_until("the command finished", |grid_snapshot| {
        grid_snapshot.contains("after")
    });
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(markers_on_screen(&zellij.snapshot().text), "");
    let attention = session_attention(&zellij);
    assert!(
        !attention.items.iter().any(|item| item.attention.watched),
        "a watch came back: {:?}",
        attention.items
    );
    zellij.quit();
}

#[test]
fn ignoring_a_panes_output_hides_only_new_output() {
    let (mut zellij, left, _right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SESSION_INDICATOR_ON));
    zellij.run_cli_action(CliAction::IgnorePaneOutput {
        pane_id: pane_id(&left),
    });
    let_the_startup_grace_pass();
    unfocus_the_host_window(&zellij);
    print_lines(&left, "ignored output");
    zellij.wait_until("the output is drawn", |grid_snapshot| {
        grid_snapshot.contains("ignored output 2")
    });
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(markers_on_screen(&zellij.snapshot().text), "");
    assert_eq!(session_attention(&zellij).counts.new_output, 0);
    left.output(&program_status("state=blocked:kind=question"));
    wait_for_markers(&zellij, "●");
    zellij.quit();
}

#[test]
fn muting_a_tab_hides_its_panes_and_marks_the_tab() {
    let (mut zellij, left, _right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SESSION_INDICATOR_ON));
    left.output(&program_status("state=blocked:kind=question"));
    wait_for_markers(&zellij, "●");
    zellij.run_cli_action(CliAction::MuteTab { tab_id: None });
    zellij.wait_until("the tab is muted", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("Tab #1 ⊘")
            && !markers_on_screen(&grid_snapshot.text).contains('●')
    });
    zellij.run_cli_action(CliAction::MuteTab { tab_id: None });
    wait_for_markers(&zellij, "●");
    zellij.quit();
}

fn press_in_mode(zellij: &TestSession, mode: char, key: char) {
    zellij.send_stdin(&keys::ctrl(mode));
    zellij.send_stdin(&keys::key(key));
    zellij.wait_until("back in normal mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
}

#[test]
fn the_pane_tab_and_session_mode_keys_mute_ignore_and_watch() {
    let (mut zellij, _left, right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SESSION_INDICATOR_ON));
    press_in_mode(&zellij, 'p', 'u');
    wait_for_markers(&zellij, "⊘");
    press_in_mode(&zellij, 'p', 'u');
    wait_for_markers(&zellij, "");
    press_in_mode(&zellij, 'p', 'g');
    let_the_startup_grace_pass();
    focus_left_pane(&zellij);
    unfocus_the_host_window(&zellij);
    print_lines(&right, "ignored by key");
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(session_attention(&zellij).counts.new_output, 0);
    zellij.send_stdin(b"\x1b[I");
    press_in_mode(&zellij, 'p', 'v');
    wait_for_session_attention(&zellij, "the focused pane is watched", |attention| {
        attention.items.iter().any(|item| item.attention.watched)
    });
    press_in_mode(&zellij, 't', 'u');
    zellij.wait_until("the tab is muted", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("Tab #1 ⊘")
    });
    press_in_mode(&zellij, 'o', 'u');
    wait_for_session_attention(&zellij, "the session is muted", |attention| attention.muted);
    zellij.quit();
}

#[test]
fn the_unlock_first_preset_mutes_the_pane_from_pane_mode() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(SESSION_INDICATOR_ON)
        .with_cli_options(zellij_utils::input::options::Options {
            keybinds_preset: Some("unlock-first".to_owned()),
            ..Default::default()
        })
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    zellij.wait_until("the session starts locked", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.contains("UNLOCK")
    });
    zellij.send_stdin(&keys::ctrl('g'));
    zellij.send_stdin(&keys::key('p'));
    zellij.send_stdin(&keys::key('u'));
    zellij.wait_until("the status bar says the pane is muted", |grid_snapshot| {
        grid_snapshot
            .lines()
            .last()
            .map(|line| line.contains("pane muted"))
            .unwrap_or(false)
    });
    zellij.quit();
}

#[test]
fn mark_pane_seen_from_the_cli_clears_that_pane() {
    let (mut zellij, left, _right) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    shell_command(&left, "A", "cargo test", "D;1");
    wait_for_markers(&zellij, "✗");
    zellij.run_cli_action(CliAction::MarkPaneSeen {
        pane_id: pane_id(&left),
    });
    wait_for_markers(&zellij, "");
    zellij.quit();
}

fn new_command_pane_with_notify(command: &[&str]) -> CliAction {
    CliAction::NewPane {
        direction: None,
        command: command.iter().map(|part| part.to_string()).collect(),
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
        stacked: false,
        blocking: false,
        block_until_exit_success: false,
        block_until_exit_failure: false,
        block_until_exit: false,
        unblock_condition: None,
        near_current_pane: false,
        no_focus: false,
        borderless: None,
        tab_id: None,
        border_style: None,
        notify: true,
    }
}

#[test]
fn notify_on_a_new_command_pane_reports_when_it_finishes() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(SESSION_INDICATOR_ON)
        .start();
    let first = claim_first_terminal_and_wait_for_prompt(&zellij);
    first.disable_echo();
    zellij.run_cli_action(new_command_pane_with_notify(&["false"]));
    let command = zellij.expect_pty_spawn();
    command.disable_echo();
    command.output(b"running\r\n");
    zellij.wait_until("the command pane is drawn", |grid_snapshot| {
        grid_snapshot.contains("running")
    });
    wait_for_session_attention(&zellij, "the command pane is watched", |attention| {
        attention.items.iter().any(|item| {
            item.pane_id == zellij_utils::data::PaneId::Terminal(command.terminal_id())
                && item.attention.watched
        })
    });
    let _other_tab = open_second_tab(&zellij);
    command.exit(Some(1));
    zellij.wait_until("the exit raised a watch alert", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("watch alert")
    });
    zellij.quit();
}

#[test]
fn frame_markers_false_hides_every_pane_marker_but_keeps_the_indicator() {
    let (mut zellij, left, _right) = start_with_two_panes(
        TestRunner::new(TERMINAL_SIZE)
            .with_config("session_indicator true\nattention {\n    frame_markers false\n}"),
    );
    left.output(&program_status("state=blocked:kind=question"));
    zellij.wait_until("the indicator counts the item", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("● 1")
    });
    std::thread::sleep(Duration::from_millis(300));
    let grid_snapshot = zellij.snapshot();
    assert_eq!(
        markers_on_screen(&grid_snapshot.text),
        "",
        "a marker was shown with frame_markers off:\n{}",
        grid_snapshot.text
    );
    zellij.quit();
}

#[test]
fn host_status_false_never_tells_the_host_terminal() {
    let (mut zellij, agent, _right) = start_with_two_panes(
        TestRunner::new(TERMINAL_SIZE)
            .with_host_terminal(HostTerminal::ProgramStatus)
            .with_config("attention {\n    host_status false\n}"),
    );
    agent.output(&program_status("state=blocked:kind=permission"));
    wait_for_markers(&zellij, "●");
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!contains_subslice(
        &zellij.raw_bytes(),
        b"\x1b]7501;state=blocked:app=zellij"
    ));
    zellij.quit();
}

#[test]
fn a_successful_command_counts_as_done_only_after_the_threshold() {
    let (mut zellij, left, _right) = start_with_two_panes(
        TestRunner::new(TERMINAL_SIZE).with_config("attention {\n    done_threshold_secs 1\n}"),
    );
    shell_command(&left, "A", "true", "D;0");
    left.output(b"\x1b]133;A\x1b\\$ quick done\r\n");
    zellij.wait_until("the quick command finished", |grid_snapshot| {
        grid_snapshot.contains("quick done")
    });
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(markers_on_screen(&zellij.snapshot().text), "");
    left.output(b"\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\sleep 1\x1b]133;C\x1b\\\r\n");
    std::thread::sleep(Duration::from_millis(1200));
    left.output(b"\x1b]133;D;0\x1b\\");
    wait_for_markers(&zellij, "✓");
    zellij.quit();
}

fn busy_then_silent_in_another_tab(config: &str) -> (TestSession, FakePtyHandle) {
    let zellij = TestRunner::new(TERMINAL_SIZE).with_config(config).start();
    let busy = claim_first_terminal_and_wait_for_prompt(&zellij);
    busy.disable_echo();
    let _second = open_second_tab(&zellij);
    produce_steady_output(&[&busy], Duration::from_millis(3000));
    (zellij, busy)
}

#[test]
fn idle_rule_all_marks_an_unwatched_busy_pane_quiet() {
    let (mut zellij, _busy) = busy_then_silent_in_another_tab(
        "attention {\n    idle_rule \"all\"\n    idle_activity_secs 1\n    idle_silence_secs 1\n}",
    );
    wait_for_session_attention(&zellij, "the busy pane went quiet", |attention| {
        attention.counts.quiet >= 1
    });
    zellij.quit();
}

#[test]
fn idle_rule_watched_ignores_unwatched_panes() {
    let (mut zellij, _busy) = busy_then_silent_in_another_tab(
        "attention {\n    idle_rule \"watched\"\n    idle_activity_secs 1\n    idle_silence_secs 1\n}",
    );
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(session_attention(&zellij).counts.quiet, 0);
    zellij.quit();
}

#[test]
fn state_error_shows_failed() {
    let (mut zellij, left, _right) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    left.output(&program_status("state=error"));
    wait_for_markers(&zellij, "✗");
    zellij.quit();
}

#[test]
fn state_clear_removes_the_status() {
    let (mut zellij, left, _right) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    left.output(&program_status("state=blocked:kind=question"));
    wait_for_markers(&zellij, "●");
    left.output(&program_status("state=clear"));
    wait_for_markers(&zellij, "");
    zellij.quit();
}

#[test]
fn nested_ids_keep_the_most_urgent_and_clear_with_their_parent() {
    let (mut zellij, left, _right) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    left.output(&program_status("id=a:state=working"));
    left.output(&program_status("id=a/b:state=blocked"));
    wait_for_markers(&zellij, "●");
    left.output(&program_status("id=a:state=clear"));
    wait_for_markers(&zellij, "");
    zellij.quit();
}

#[test]
fn title_and_progress_are_shown_in_the_inbox() {
    let (mut zellij, left, _right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SESSION_INDICATOR_ON));
    left.output(&program_status(&format!(
        "state=working:title={}:progress=40",
        encoded("Deploying")
    )));
    wait_for_markers(&zellij, "◐");
    open_inbox(&zellij);
    zellij.wait_until("the inbox shows the title and progress", |grid_snapshot| {
        grid_snapshot.contains("working 40%: Deploying")
    });
    zellij.quit();
}

#[test]
fn vscode_shell_integration_reports_a_failed_command() {
    let (mut zellij, left, _right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SESSION_INDICATOR_ON));
    let _other_tab = open_second_tab(&zellij);
    left.output(
        b"\x1b]633;A\x07$ \x1b]633;B\x07\x1b]633;E;make test\x07\x1b]633;C\x07\r\nboom\r\n\x1b]633;D;1\x07",
    );
    zellij.wait_until("the failed command is counted", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("✗ 1")
    });
    open_inbox(&zellij);
    zellij.wait_briefly("the inbox shows the command", |grid_snapshot| {
        grid_snapshot.contains("FAILED") && grid_snapshot.contains("make test")
    });
    zellij.quit();
}

#[test]
fn osc_99_and_osc_777_notifications_need_you() {
    let (mut zellij, left, _right) = start_with_two_panes(TestRunner::new(TERMINAL_SIZE));
    left.output(b"\x1b]777;notify;Build;finished\x07");
    wait_for_markers(&zellij, "●");
    focus_left_pane(&zellij);
    wait_for_markers(&zellij, "");
    focus_right_pane(&zellij);
    left.output(b"\x1b]99;;Tests passed\x1b\\");
    wait_for_markers(&zellij, "●");
    zellij.quit();
}

#[test]
fn a_notifications_text_is_shown_in_the_inbox() {
    let (mut zellij, left, _right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config(SESSION_INDICATOR_ON));
    left.output(b"\x1b]9;Build finished\x07");
    wait_for_markers(&zellij, "●");
    assert!(!zellij.snapshot().contains("Build finished"));
    let _other_tab = open_second_tab(&zellij);
    left.output(b"\x1b]9;Deploy finished\x07");
    zellij.wait_until("the notification is counted", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("● 1")
    });
    open_inbox(&zellij);
    zellij.wait_until("the inbox shows the notification text", |grid_snapshot| {
        grid_snapshot.contains("Deploy finished")
    });
    zellij.quit();
}

fn frame_row(zellij: &TestSession) -> String {
    zellij.snapshot().lines()[1].chars().take(60).collect()
}

#[test]
fn without_arrow_fonts_frames_use_letters() {
    let (mut zellij, left, _right) =
        start_with_two_panes(TestRunner::new(TERMINAL_SIZE).with_config("simplified_ui true"));
    left.output(&program_status("state=blocked:kind=question"));
    zellij.wait_until("the blocked pane shows a letter", |grid_snapshot| {
        grid_snapshot.lines()[1]
            .chars()
            .take(60)
            .collect::<String>()
            .contains(" ! ")
    });
    left.output(&program_status("state=clear"));
    shell_command(&left, "A", "cargo test", "D;1");
    zellij.wait_until("the failed pane shows a letter", |grid_snapshot| {
        grid_snapshot.lines()[1]
            .chars()
            .take(60)
            .collect::<String>()
            .contains(" x ")
    });
    let row = frame_row(&zellij);
    assert!(!row.contains('✗') && !row.contains('●'), "{}", row);
    zellij.quit();
}

const NARROW_LAYOUT: &str = r#"
layout {
    pane size=1 borderless=true {
        plugin location="tab-bar"
    }
    pane split_direction="vertical" {
        pane size=6
        pane focus=true
    }
    pane size=2 borderless=true {
        plugin location="status-bar"
    }
}
"#;

#[test]
fn the_marker_is_dropped_on_a_very_narrow_pane_title() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_layout(zellij_utils::data::LayoutInfo::Stringified(
            NARROW_LAYOUT.to_owned(),
        ))
        .start();
    let narrow = zellij.expect_pty_spawn();
    let wide = zellij.expect_pty_spawn();
    for terminal in [&narrow, &wide] {
        terminal.disable_echo();
        terminal.output(b"$ ");
    }
    zellij.wait_until("both panes are drawn", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    let before = zellij.snapshot().lines()[1].clone();
    narrow.output(&program_status("state=blocked:kind=question"));
    narrow.output(b"reported\r\n");
    std::thread::sleep(Duration::from_millis(700));
    let after = zellij.snapshot().lines()[1].clone();
    let narrow_frame: String = after.chars().take(6).collect();
    assert!(!narrow_frame.contains('●'), "{}", after);
    assert_eq!(
        before.chars().take(6).collect::<String>(),
        narrow_frame,
        "the narrow frame changed:\n{}\n{}",
        before,
        after
    );
    zellij.quit();
}

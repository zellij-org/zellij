#![cfg(unix)]

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::engine::Engine as _;
use zellij_integration_tests::{keys, FakePtyHandle, TestRunner, TestSession, TERMINAL_SIZE};

const SELECTION_CONFIG: &str = concat!(
    "mouse_mode true\n",
    "copy_clipboard \"system\"\n",
    "keybinds {\n",
    " normal {\n",
    "  bind \"Ctrl y\" { Copy; }\n",
    " }\n",
    "}\n",
);

const MARKED_COMMAND: &[u8] = b"\x1b]133;A\x07$ \x1b]133;B\x07echo hi\x1b]133;C\x07\r\nhello world\r\nsecond line\x1b]133;D;0\x07\r\n\x1b]133;A\x07$ ";

const UNMARKED_LINE_BEFORE_COMMAND: &[u8] = b"plain unmarked line\r\n\x1b]133;A\x07$ \x1b]133;B\x07echo hi\x1b]133;C\x07\r\nhello world\x1b]133;D;0\x07\r\n\x1b]133;A\x07$ ";

const COMMAND_AND_OUTPUT: &str = "echo hi\nhello world\nsecond line";
const UNMARKED_LOGICAL_LINE: &str = "plain unmarked line";

fn start_zellij() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config(SELECTION_CONFIG)
        .start()
}

fn claim_terminal_with(zellij: &TestSession, pane_output: &[u8], last_line: &str) -> FakePtyHandle {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(pane_output);
    zellij.wait_until("marked command output rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains(last_line)
    });
    terminal
}

fn sgr_mouse_report(column: usize, line: usize, button: u8, final_byte: char) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}{}", button, column, line, final_byte).into_bytes()
}

fn triple_click(zellij: &TestSession, column: usize, line: usize) {
    for _ in 0..3 {
        zellij.send_stdin(&sgr_mouse_report(column, line, 0, 'M'));
        zellij.send_stdin(&sgr_mouse_report(column, line, 0, 'm'));
    }
}

fn osc52_copy_of(text: &str) -> Vec<u8> {
    format!("\u{1b}]52;c;{}\u{1b}\\", BASE64_STANDARD.encode(text)).into_bytes()
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

#[test]
fn triple_click_inside_marked_output_selects_the_whole_command() {
    let mut zellij = start_zellij();
    claim_terminal_with(&zellij, MARKED_COMMAND, "second line");

    triple_click(&zellij, 4, 3);

    let expected_copy = osc52_copy_of(COMMAND_AND_OUTPUT);
    zellij.wait_until_raw_output(
        "the whole marked command and its output were copied",
        |bytes| occurrences_of(bytes, &expected_copy) >= 1,
    );

    zellij.quit();
}

#[test]
fn triple_click_outside_a_marked_region_selects_the_logical_line() {
    let mut zellij = start_zellij();
    claim_terminal_with(&zellij, UNMARKED_LINE_BEFORE_COMMAND, "hello world");

    triple_click(&zellij, 4, 2);

    let expected_copy = osc52_copy_of(UNMARKED_LOGICAL_LINE);
    let raw_output = zellij
        .wait_until_raw_output("the unmarked logical line was copied on its own", |bytes| {
            occurrences_of(bytes, &expected_copy) >= 1
        });

    let command_copy = osc52_copy_of("echo hi\nhello world");
    assert_eq!(
        occurrences_of(&raw_output, &command_copy),
        0,
        "a click outside the marked region must not select the marked command"
    );

    zellij.quit();
}

#[test]
fn a_triple_click_selection_survives_scrollback_movement() {
    let mut zellij = start_zellij();
    let terminal = claim_terminal_with(&zellij, MARKED_COMMAND, "second line");

    triple_click(&zellij, 4, 3);

    let expected_copy = osc52_copy_of(COMMAND_AND_OUTPUT);
    zellij.wait_until_raw_output("the marked command was copied once", |bytes| {
        occurrences_of(bytes, &expected_copy) >= 1
    });

    for line in 1..=25 {
        terminal.output(format!("filler{:02}\r\n", line).as_bytes());
    }
    zellij.wait_until(
        "the selected region was pushed out of the viewport",
        |grid_snapshot| {
            grid_snapshot.contains("filler25") && !grid_snapshot.contains("second line")
        },
    );

    zellij.send_stdin(&keys::ctrl('y'));

    zellij.wait_until_raw_output(
        "the same selection was copied again after it moved into the scrollback",
        |bytes| occurrences_of(bytes, &expected_copy) >= 2,
    );

    zellij.quit();
}

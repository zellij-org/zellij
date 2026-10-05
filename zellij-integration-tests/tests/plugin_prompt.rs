#![cfg(unix)]

use zellij_integration_tests::{
    keys, CliCaller, GridSnapshot, LayoutInfo, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;

const FIXTURE_PLUGIN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../zellij-utils/assets/plugins/fixture-plugin-for-tests.wasm"
);
const PERMISSION_PROMPT: &str = "Allow? (y/n)";
const PROMPT_MARKER: &str = "<Esc> - cancel";
const DOWN: &[u8] = b"\x1b[B";
const RIGHT: &[u8] = b"\x1b[C";
const FIRST_CLIENT: &str = "1";

fn fixture_plugin_url() -> String {
    format!("file:{}", FIXTURE_PLUGIN_PATH)
}

fn start_with_the_fixture_plugin() -> TestSession {
    let layout = format!(
        r#"
        layout {{
            pane size=1 borderless=true {{
                plugin location="tab-bar"
            }}
            pane focus=true {{
                plugin location="{}"
            }}
            pane
            pane size=2 borderless=true {{
                plugin location="status-bar"
            }}
        }}
        "#,
        fixture_plugin_url()
    );
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_layout(LayoutInfo::Stringified(layout))
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(b"$ ");
    zellij.wait_until("the fixture plugin asks for permissions", |grid_snapshot| {
        grid_snapshot.contains(PERMISSION_PROMPT)
    });
    zellij.send_stdin(b"y");
    zellij.wait_until("the fixture plugin is running", |grid_snapshot| {
        !grid_snapshot.contains(PERMISSION_PROMPT)
            && grid_snapshot.contains("Rows:")
            && grid_snapshot.status_bar_appears()
    });
    zellij
}

fn send_to_the_fixture(zellij: &TestSession, name: &str, args: &[(&str, &str)]) {
    let args = if args.is_empty() {
        None
    } else {
        let joined = args
            .iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<_>>()
            .join(",");
        Some(joined.parse().unwrap())
    };
    let pipe = CliAction::Pipe {
        name: Some(name.to_owned()),
        payload: None,
        args,
        plugin: Some(fixture_plugin_url()),
        plugin_configuration: None,
        force_launch_plugin: false,
        skip_plugin_cache: false,
        floating_plugin: None,
        in_place_plugin: None,
        plugin_cwd: None,
        plugin_title: None,
        popup: None,
        popup_no_focus: false,
    };
    let caller = CliCaller {
        pane_id: None,
        stdin: None,
    };
    zellij.run_cli_pipe(pipe, caller).wait_for_exit();
}

fn ask(zellij: &TestSession, name: &str, text: &str) -> GridSnapshot {
    send_to_the_fixture(zellij, name, &[]);
    wait_for_prompt_showing(zellij, text)
}

fn wait_for_prompt_showing(zellij: &TestSession, text: &str) -> GridSnapshot {
    zellij.wait_until(&format!("prompt showing {:?}", text), |grid_snapshot| {
        grid_snapshot.contains(PROMPT_MARKER) && grid_snapshot.contains(text)
    })
}

fn wait_for_result(zellij: &TestSession, result: &str) -> GridSnapshot {
    let expected = format!("Prompt result 1: {}", result);
    zellij.wait_until(
        &format!("the plugin shows {:?}", expected),
        |grid_snapshot| grid_snapshot.contains(&expected) && !grid_snapshot.contains(PROMPT_MARKER),
    )
}

#[test]
fn confirm_yes_reaches_the_plugin() {
    let mut zellij = start_with_the_fixture_plugin();
    let grid_snapshot = ask(&zellij, "prompt_confirm", "Delete branch?");
    assert!(grid_snapshot.contains("Delete") && grid_snapshot.contains("Keep"));
    let row = grid_snapshot.row_of_line("Delete branch?").unwrap();
    let line = grid_snapshot.lines()[row].clone();
    let start = line[..line.find("Delete branch?").unwrap()].chars().count();
    let plain_color = grid_snapshot.cell_foreground(start, row);
    let word_color = grid_snapshot.cell_foreground(start + 7, row);
    assert!(word_color.is_some() && word_color != plain_color);
    for column in start + 7..start + 13 {
        assert_eq!(grid_snapshot.cell_foreground(column, row), word_color);
    }
    assert_eq!(grid_snapshot.cell_foreground(start + 13, row), plain_color);
    assert_eq!(line.matches("Delete branch?").count(), 1);
    zellij.send_stdin(&keys::ENTER);
    wait_for_result(&zellij, "Confirmed(true)");
    zellij.quit();
}

#[test]
fn confirm_no_reaches_the_plugin() {
    let mut zellij = start_with_the_fixture_plugin();
    ask(&zellij, "prompt_confirm", "Delete branch?");
    zellij.send_stdin(RIGHT);
    zellij.send_stdin(&keys::ENTER);
    wait_for_result(&zellij, "Confirmed(false)");
    zellij.quit();
}

#[test]
fn esc_on_a_confirm_reaches_the_plugin_as_cancelled() {
    let mut zellij = start_with_the_fixture_plugin();
    ask(&zellij, "prompt_confirm", "Delete branch?");
    zellij.send_stdin(&keys::ESC);
    wait_for_result(&zellij, "Cancelled");
    zellij.quit();
}

#[test]
fn choose_with_multi_returns_every_ticked_item() {
    let mut zellij = start_with_the_fixture_plugin();
    ask(&zellij, "prompt_choose_multi", "0 ticked");
    zellij.send_stdin(&keys::SPACE);
    zellij.send_stdin(DOWN);
    zellij.send_stdin(DOWN);
    zellij.send_stdin(&keys::SPACE);
    zellij.wait_until("two items ticked", |grid_snapshot| {
        grid_snapshot.contains("2 ticked")
    });
    zellij.send_stdin(&keys::ENTER);
    wait_for_result(&zellij, r#"Answered(Choices(["alpha", "gamma"]))"#);
    zellij.quit();
}

#[test]
fn a_form_returns_a_map_of_its_fields() {
    let mut zellij = start_with_the_fixture_plugin();
    ask(&zellij, "prompt_form", "Use CI");
    zellij.send_stdin(&keys::ctrl('a'));
    wait_for_result(
        &zellij,
        r#"Answered(Form({"ci": Bool(true), "name": Text("demo"), "port": Number(8080)}))"#,
    );
    zellij.quit();
}

#[test]
fn a_timeout_without_a_default_reaches_the_plugin() {
    let mut zellij = start_with_the_fixture_plugin();
    ask(&zellij, "prompt_input_timeout", "Name");
    wait_for_result(&zellij, "TimedOut(None)");
    zellij.quit();
}

#[test]
fn a_timeout_with_a_default_answers_the_default() {
    let mut zellij = start_with_the_fixture_plugin();
    send_to_the_fixture(&zellij, "prompt_input_timeout", &[("default", "fallback")]);
    wait_for_prompt_showing(&zellij, "Name");
    wait_for_result(&zellij, r#"TimedOut(Some(Text("fallback")))"#);
    zellij.quit();
}

#[test]
fn an_invalid_request_is_answered_with_an_error_and_no_popup() {
    let mut zellij = start_with_the_fixture_plugin();
    send_to_the_fixture(&zellij, "prompt_invalid", &[]);
    let grid_snapshot = wait_for_result(&zellij, "Error(");
    assert!(grid_snapshot.contains("min is larger than max"));
    zellij.quit();
}

#[test]
fn closing_the_calling_plugin_closes_its_prompt() {
    let mut zellij = start_with_the_fixture_plugin();
    ask(&zellij, "prompt_confirm", "Delete branch?");
    send_to_the_fixture(&zellij, "close_self", &[]);
    zellij.wait_until("the plugin and its prompt are gone", |grid_snapshot| {
        !grid_snapshot.contains(PROMPT_MARKER)
            && !grid_snapshot.contains("Delete branch?")
            && grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
fn a_second_attached_user_does_not_see_the_prompt() {
    let mut zellij = start_with_the_fixture_plugin();
    let second_client = zellij.attach_client(TERMINAL_SIZE);
    second_client.wait_until("second client loaded", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });
    send_to_the_fixture(&zellij, "prompt_confirm", &[("only_client", FIRST_CLIENT)]);
    wait_for_prompt_showing(&zellij, "Delete branch?");
    let second_grid = second_client.wait_until("second client rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    assert!(!second_grid.contains("Delete branch?"));
    assert!(!second_grid.contains(PROMPT_MARKER));
    zellij.send_stdin(&keys::ENTER);
    wait_for_result(&zellij, "Confirmed(true)");
    let second_grid = second_client.snapshot();
    assert!(!second_grid.contains("Prompt result 1"));
    second_client.quit();
    zellij.quit();
}

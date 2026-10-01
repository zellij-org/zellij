#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, split_right_and_wait_for_prompt, CliCaller,
    CliOutputHandle, CliResult, FakePtyHandle, GridSnapshot, TestRunner, TestSession, PROMPT,
    TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;
use zellij_utils::data::PipePopupPlacement;

const PROMPT_MARKER: &str = "<Esc> - cancel";
const DOWN: &[u8] = b"\x1b[B";
const RIGHT: &[u8] = b"\x1b[C";

fn start_with_terminal() -> (TestSession, FakePtyHandle) {
    let zellij = zellij_integration_tests::start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    (zellij, terminal)
}

fn from(terminal: &FakePtyHandle) -> CliCaller {
    CliCaller::from_pane(terminal.terminal_id())
}

fn wait_for_prompt_showing(zellij: &TestSession, text: &str) -> GridSnapshot {
    zellij.wait_until(&format!("prompt showing {:?}", text), |grid_snapshot| {
        grid_snapshot.contains(PROMPT_MARKER) && grid_snapshot.contains(text)
    })
}

fn wait_for_prompts_to_close(zellij: &TestSession) {
    zellij.wait_until("prompt popups closed", |grid_snapshot| {
        !grid_snapshot.contains(PROMPT_MARKER) && grid_snapshot.status_bar_appears()
    });
}

fn type_text(zellij: &TestSession, text: &str) {
    for character in text.chars() {
        zellij.send_stdin(&keys::key(character));
    }
}

fn answered(result: &CliResult, exit_code: i32, stdout: &str) {
    assert_eq!(
        (result.exit_code, result.stdout.as_str()),
        (exit_code, stdout),
        "unexpected prompt result"
    );
}

fn confirm(zellij: &TestSession, terminal: &FakePtyHandle, question: &str) -> CliOutputHandle {
    zellij.run_prompt(
        &["confirm", question, "--yes", "Push", "--no", "Cancel"],
        from(terminal),
    )
}

#[test]
fn confirm_yes_exits_zero_and_prints_nothing() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = confirm(&zellij, &terminal, "Force push?");
    let grid_snapshot = wait_for_prompt_showing(&zellij, "Force push?");
    assert!(grid_snapshot.contains("Push") && grid_snapshot.contains("Cancel"));
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "");
    wait_for_prompts_to_close(&zellij);
    zellij.quit();
}

#[test]
fn confirm_no_exits_one() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = confirm(&zellij, &terminal, "Force push?");
    wait_for_prompt_showing(&zellij, "Force push?");
    zellij.send_stdin(RIGHT);
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 1, "");
    zellij.quit();
}

#[test]
fn confirm_esc_exits_one() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = confirm(&zellij, &terminal, "Force push?");
    wait_for_prompt_showing(&zellij, "Force push?");
    zellij.send_stdin(&keys::ESC);
    answered(&prompt.wait_for_exit(), 1, "");
    wait_for_prompts_to_close(&zellij);
    zellij.quit();
}

#[test]
fn choose_from_piped_stdin_prints_the_chosen_line() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(
        &["choose"],
        from(&terminal).with_stdin_text("main\ndevelop\nfeature/login\n"),
    );
    zellij.wait_until("all branches listed", |grid_snapshot| {
        grid_snapshot.contains(PROMPT_MARKER)
            && grid_snapshot.contains("feature/login")
            && !grid_snapshot.contains("reading")
    });
    type_text(&zellij, "dev");
    zellij.wait_until("search narrowed the list", |grid_snapshot| {
        grid_snapshot.contains("develop") && !grid_snapshot.contains("feature/login")
    });
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "develop\n");
    zellij.quit();
}

#[test]
fn choose_multi_from_piped_stdin_prints_every_ticked_line() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(
        &["choose", "--multi"],
        from(&terminal).with_stdin_text("alpha\nbeta\ngamma\n"),
    );
    zellij.wait_until("all items listed", |grid_snapshot| {
        grid_snapshot.contains("gamma") && grid_snapshot.contains("0 ticked")
    });
    zellij.send_stdin(&keys::SPACE);
    zellij.send_stdin(DOWN);
    zellij.send_stdin(DOWN);
    zellij.send_stdin(&keys::SPACE);
    zellij.wait_until("two items ticked", |grid_snapshot| {
        grid_snapshot.contains("2 ticked")
    });
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "alpha\ngamma\n");
    zellij.quit();
}

#[test]
fn choose_multi_with_nothing_ticked_prints_nothing_and_exits_zero() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(&["choose", "--multi", "a", "b"], from(&terminal));
    wait_for_prompt_showing(&zellij, "0 ticked");
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "");
    zellij.quit();
}

#[test]
fn choose_shows_slow_input_as_it_arrives() {
    let (mut zellij, terminal) = start_with_terminal();
    let (stdin, stdin_receiver) = crossbeam::channel::unbounded();
    stdin.send(b"first-item\n".to_vec()).unwrap();
    let prompt = zellij.run_prompt(&["choose"], from(&terminal).with_stdin(stdin_receiver));
    zellij.wait_until("first item shown while still reading", |grid_snapshot| {
        grid_snapshot.contains("first-item") && grid_snapshot.contains("reading")
    });
    stdin.send(b"second-item\n".to_vec()).unwrap();
    zellij.wait_until("second item shown", |grid_snapshot| {
        grid_snapshot.contains("second-item")
    });
    assert!(!prompt.has_exited());
    drop(stdin);
    zellij.wait_until("reading finished", |grid_snapshot| {
        grid_snapshot.contains("second-item") && !grid_snapshot.contains("reading")
    });
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "first-item\n");
    zellij.quit();
}

#[test]
fn choose_can_be_answered_before_stdin_ends() {
    let (mut zellij, terminal) = start_with_terminal();
    let (stdin, stdin_receiver) = crossbeam::channel::unbounded();
    stdin.send(b"early\n".to_vec()).unwrap();
    let prompt = zellij.run_prompt(&["choose"], from(&terminal).with_stdin(stdin_receiver));
    wait_for_prompt_showing(&zellij, "early");
    zellij.send_stdin(&keys::ENTER);
    let result = prompt.wait_for_exit();
    drop(stdin);
    answered(&result, 0, "early\n");
    zellij.quit();
}

#[test]
fn input_with_validation_refuses_bad_text() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(
        &["input", "Branch name", "--validate", "^[a-z]+$"],
        from(&terminal),
    );
    wait_for_prompt_showing(&zellij, "Branch name");
    type_text(&zellij, "Bad");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("validation error shown", |grid_snapshot| {
        grid_snapshot.contains("Must match")
    });
    assert!(!prompt.has_exited());
    for _ in 0..3 {
        zellij.send_stdin(&[0x7f]);
    }
    type_text(&zellij, "good");
    zellij.wait_until("error cleared", |grid_snapshot| {
        grid_snapshot.contains("good") && !grid_snapshot.contains("Must match")
    });
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "good\n");
    zellij.quit();
}

const FORM: &str = r#"{ "title": "New project",
  "fields": [
    { "id": "name",    "type": "input",  "label": "Name", "validate": "^[a-z-]+$", "required": true },
    { "id": "license", "type": "select", "label": "License", "options": ["MIT", "Apache-2.0"], "default": "MIT" },
    { "id": "ci",      "type": "toggle", "label": "Use CI", "default": true },
    { "id": "port",    "type": "number", "label": "Port", "min": 1, "max": 65535, "default": 8080 },
    { "id": "files",   "type": "choose", "label": "Files", "options": ["a", "b"], "multi": true } ],
  "buttons": { "submit": "Create", "cancel": "Cancel" } }"#;

#[test]
fn form_answers_one_json_object() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(&["form"], from(&terminal).with_stdin_text(FORM));
    wait_for_prompt_showing(&zellij, "New project");
    zellij.send_stdin(&keys::ctrl('s'));
    zellij.wait_until("required field refused", |grid_snapshot| {
        grid_snapshot.contains("This field is required")
    });
    assert!(!prompt.has_exited());
    type_text(&zellij, "demo");
    for _ in 0..4 {
        zellij.send_stdin(&keys::TAB);
    }
    zellij.send_stdin(&keys::SPACE);
    zellij.send_stdin(&keys::ctrl('s'));
    answered(
        &prompt.wait_for_exit(),
        0,
        "{\"name\":\"demo\",\"license\":\"MIT\",\"ci\":true,\"port\":8080,\"files\":[\"a\"]}\n",
    );
    zellij.quit();
}

#[test]
fn an_invalid_form_description_exits_two_without_a_popup() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(
        &["form"],
        from(&terminal)
            .with_stdin_text(r#"{"fields":[{"id":"port","type":"number","default":"x"}]}"#),
    );
    let result = prompt.wait_for_exit();
    assert_eq!(result.exit_code, 2);
    assert!(!zellij.snapshot().contains(PROMPT_MARKER));
    zellij.quit();
}

#[test]
fn timeout_without_a_default_exits_124() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(&["input", "Name", "--timeout", "1s"], from(&terminal));
    wait_for_prompt_showing(&zellij, "Name");
    answered(&prompt.wait_for_exit(), 124, "");
    wait_for_prompts_to_close(&zellij);
    zellij.quit();
}

#[test]
fn timeout_with_a_default_answers_the_default() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(
        &["input", "Name", "--timeout", "1s", "--default", "fallback"],
        from(&terminal),
    );
    wait_for_prompt_showing(&zellij, "Name");
    answered(&prompt.wait_for_exit(), 0, "fallback\n");
    zellij.quit();
}

#[test]
fn two_prompts_from_different_panes_are_answered_newest_first() {
    let (mut zellij, left_terminal) = start_with_terminal();
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    let first = confirm(&zellij, &left_terminal, "First question?");
    wait_for_prompt_showing(&zellij, "First question?");
    let second = confirm(&zellij, &right_terminal, "Second question?");
    wait_for_prompt_showing(&zellij, "Second question?");
    zellij.send_stdin(&keys::ENTER);
    answered(&second.wait_for_exit(), 0, "");
    assert!(!first.has_exited());
    zellij.wait_until("first prompt back on top", |grid_snapshot| {
        grid_snapshot.contains("First question?") && !grid_snapshot.contains("Second question?")
    });
    zellij.send_stdin(&keys::ESC);
    answered(&first.wait_for_exit(), 1, "");
    wait_for_prompts_to_close(&zellij);
    zellij.quit();
}

#[test]
fn two_prompts_from_the_same_pane_hand_keys_back_in_order() {
    let (mut zellij, terminal) = start_with_terminal();
    let first = zellij.run_prompt(&["toggle", "First toggle"], from(&terminal));
    wait_for_prompt_showing(&zellij, "First toggle");
    let second = zellij.run_prompt(
        &["toggle", "Second toggle", "--default", "on"],
        from(&terminal),
    );
    wait_for_prompt_showing(&zellij, "Second toggle");
    zellij.send_stdin(&keys::ENTER);
    answered(&second.wait_for_exit(), 0, "true\n");
    wait_for_prompt_showing(&zellij, "First toggle");
    zellij.send_stdin(&keys::ENTER);
    answered(&first.wait_for_exit(), 0, "false\n");
    wait_for_prompts_to_close(&zellij);
    zellij.send_stdin(&keys::key('x'));
    terminal.wait_for_stdin("keys reach the pane again", |bytes| bytes.contains(&b'x'));
    zellij.quit();
}

#[test]
fn closing_the_popup_without_answering_exits_one() {
    let (mut zellij, _first_terminal) = start_with_terminal();
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let second_terminal = zellij.expect_pty_spawn();
    second_terminal.output(PROMPT);
    zellij.wait_until("second tab opened", |grid_snapshot| {
        grid_snapshot.contains("Tab #2") && grid_snapshot.status_bar_appears()
    });
    let prompt = confirm(&zellij, &second_terminal, "Leave?");
    wait_for_prompt_showing(&zellij, "Leave?");
    zellij.run_cli_action(CliAction::GoToTab { index: 1 });
    answered(&prompt.wait_for_exit(), 1, "");
    wait_for_prompts_to_close(&zellij);
    zellij.quit();
}

#[test]
fn a_second_attached_user_does_not_see_the_popup() {
    let (mut zellij, terminal) = start_with_terminal();
    let second_client = zellij.attach_client(TERMINAL_SIZE);
    second_client.wait_until("second client loaded", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });
    let prompt = confirm(&zellij, &terminal, "Only for you?");
    wait_for_prompt_showing(&zellij, "Only for you?");
    terminal.output(b"AFTER-PROMPT-MARKER");
    let second_grid = second_client
        .wait_until("second client rendered the output", |grid_snapshot| {
            grid_snapshot.contains("AFTER-PROMPT-MARKER")
        });
    assert!(!second_grid.contains("Only for you?"));
    assert!(!second_grid.contains(PROMPT_MARKER));
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "");
    second_client.quit();
    zellij.quit();
}

#[test]
fn json_output_reports_a_cancelled_prompt() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(
        &["select", "License", "MIT", "Apache-2.0", "--json"],
        from(&terminal),
    );
    wait_for_prompt_showing(&zellij, "License");
    zellij.send_stdin(&keys::ESC);
    answered(&prompt.wait_for_exit(), 1, "{\"result\":\"cancelled\"}\n");
    zellij.quit();
}

#[test]
fn prompt_outside_a_pane_exits_two() {
    let (mut zellij, _terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(
        &["confirm", "Nobody?"],
        CliCaller {
            pane_id: None,
            stdin: None,
        },
    );
    assert_eq!(prompt.wait_for_exit().exit_code, 2);
    zellij.quit();
}

fn pipe_action(
    name: &str,
    payload: Option<&str>,
    plugin: Option<&str>,
    args: Option<&str>,
    popup: Option<PipePopupPlacement>,
) -> CliAction {
    CliAction::Pipe {
        name: Some(name.to_owned()),
        payload: payload.map(|p| p.to_owned()),
        args: args.map(|a| a.parse().unwrap()),
        plugin: plugin.map(|p| p.to_owned()),
        plugin_configuration: None,
        force_launch_plugin: plugin.is_some(),
        skip_plugin_cache: false,
        floating_plugin: None,
        in_place_plugin: None,
        plugin_cwd: None,
        plugin_title: None,
        popup,
    }
}

#[test]
fn pipe_to_the_prompt_plugin_keeps_its_exit_codes() {
    let (mut zellij, terminal) = start_with_terminal();
    let pipe = zellij.run_cli_pipe(
        pipe_action(
            "confirm",
            None,
            Some("zellij:prompt"),
            Some("message=Piped question?"),
            Some(PipePopupPlacement::Pane),
        ),
        from(&terminal),
    );
    wait_for_prompt_showing(&zellij, "Piped question?");
    zellij.send_stdin(&keys::ESC);
    assert_eq!(pipe.wait_for_exit().exit_code, 1);
    zellij.quit();
}

#[test]
fn existing_pipes_without_a_listener_still_exit_zero() {
    let (mut zellij, terminal) = start_with_terminal();
    let pipe = zellij.run_cli_pipe(
        pipe_action("nobody-listens", Some("hello"), None, None, None),
        from(&terminal),
    );
    assert_eq!(pipe.wait_for_exit().exit_code, 0);
    let piped = zellij.run_cli_pipe(
        pipe_action("nobody-listens", None, None, None, None),
        from(&terminal).with_stdin_text("line one\nline two\n"),
    );
    assert_eq!(piped.wait_for_exit().exit_code, 0);
    zellij.quit();
}

fn column_of(grid_snapshot: &GridSnapshot, needle: &str) -> Option<(usize, usize)> {
    grid_snapshot
        .lines()
        .iter()
        .enumerate()
        .find_map(|(row, line)| {
            line.find(needle)
                .map(|byte_index| (row, line[..byte_index].chars().count()))
        })
}

#[test]
fn at_mouse_opens_the_popup_where_the_pointer_last_was() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(b"\x1b[<35;81;5M");
    let prompt = zellij.run_prompt(&["choose", "--at-mouse", "pointer-item"], from(&terminal));
    let grid_snapshot = wait_for_prompt_showing(&zellij, "pointer-item");
    let (row, column) = column_of(&grid_snapshot, "pointer-item").unwrap();
    assert!(
        column >= 70 && row >= 4,
        "popup not at the pointer: {:?}",
        (row, column)
    );
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "pointer-item\n");
    zellij.quit();
}

#[test]
fn popups_without_their_own_frame_get_one_with_the_title() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(
        &["choose", "a", "b", "--title", "Pick one"],
        from(&terminal),
    );
    let grid_snapshot = wait_for_prompt_showing(&zellij, "Pick one");
    assert!(grid_snapshot
        .lines()
        .iter()
        .any(|line| line.contains("╭─ Pick one ─")));
    assert!(!grid_snapshot.contains("asked by"));
    zellij.send_stdin(&keys::ESC);
    answered(&prompt.wait_for_exit(), 1, "");
    zellij.quit();
}

#[test]
fn at_cursor_opens_the_popup_below_the_cursor_of_the_calling_pane() {
    let (mut zellij, terminal) = start_with_terminal();
    terminal.output(b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n$ ");
    zellij.wait_until("cursor moved down", |grid_snapshot| {
        grid_snapshot
            .cursor
            .map(|cursor| cursor.y >= 13)
            .unwrap_or(false)
    });
    let prompt = zellij.run_prompt(&["choose", "--at-cursor", "cursor-item"], from(&terminal));
    let grid_snapshot = wait_for_prompt_showing(&zellij, "cursor-item");
    let (row, _) = column_of(&grid_snapshot, "cursor-item").unwrap();
    assert!(row >= 13, "popup not below the cursor: row {}", row);
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "cursor-item\n");
    zellij.quit();
}

fn frame_edges(grid_snapshot: &GridSnapshot, needle: &str) -> (usize, usize, usize) {
    let (row, column) = column_of(grid_snapshot, needle).unwrap();
    let line = &grid_snapshot.lines()[row];
    let chars: Vec<char> = line.chars().collect();
    let left = chars[..column].iter().rposition(|c| *c == '│').unwrap();
    let right = column + chars[column..].iter().position(|c| *c == '│').unwrap();
    (row, left, right)
}

fn open_menu_from(
    zellij: &TestSession,
    terminal: &FakePtyHandle,
    extra: &[&str],
    item: &str,
) -> (CliOutputHandle, GridSnapshot) {
    let mut args = vec!["menu", item];
    args.extend_from_slice(extra);
    let prompt = zellij.run_prompt(&args, from(terminal));
    let grid_snapshot = wait_for_prompt_showing(zellij, item);
    (prompt, grid_snapshot)
}

fn close_with_esc(zellij: &TestSession, prompt: CliOutputHandle) {
    zellij.send_stdin(&keys::ESC);
    answered(&prompt.wait_for_exit(), 1, "");
    wait_for_prompts_to_close(zellij);
}

#[test]
fn by_default_the_popup_opens_over_the_calling_pane() {
    let (mut zellij, left_terminal) = start_with_terminal();
    let _right_terminal = split_right_and_wait_for_prompt(&zellij);
    let (prompt, grid_snapshot) = open_menu_from(&zellij, &left_terminal, &[], "LEFT-ITEM");
    let (_, left, right) = frame_edges(&grid_snapshot, "LEFT-ITEM");
    assert!(
        right < 60,
        "popup not over the left pane: {}..{}",
        left,
        right
    );
    close_with_esc(&zellij, prompt);
    zellij.quit();
}

#[test]
fn at_center_opens_the_popup_in_the_middle_of_the_screen() {
    let (mut zellij, left_terminal) = start_with_terminal();
    let _right_terminal = split_right_and_wait_for_prompt(&zellij);
    let (prompt, grid_snapshot) =
        open_menu_from(&zellij, &left_terminal, &["--at-center"], "CENTER-ITEM");
    let (_, left, right) = frame_edges(&grid_snapshot, "CENTER-ITEM");
    assert!(
        left < 60 && right > 60,
        "popup not centered: {}..{}",
        left,
        right
    );
    assert!((left + right) / 2 >= 55 && (left + right) / 2 <= 65);
    close_with_esc(&zellij, prompt);
    zellij.quit();
}

#[test]
fn at_cursor_follows_the_focused_pane_not_the_calling_one() {
    let (mut zellij, left_terminal) = start_with_terminal();
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    right_terminal.output(b"\r\n\r\n\r\n\r\n\r\n\r\n$ ");
    zellij.wait_until("right cursor moved down", |grid_snapshot| {
        grid_snapshot
            .cursor
            .map(|cursor| cursor.y >= 8)
            .unwrap_or(false)
    });
    let (prompt, grid_snapshot) =
        open_menu_from(&zellij, &left_terminal, &["--at-cursor"], "FOLLOW-ITEM");
    let (row, left, _) = frame_edges(&grid_snapshot, "FOLLOW-ITEM");
    assert!(
        left >= 60,
        "popup not in the focused right pane: column {}",
        left
    );
    assert!(row >= 9, "popup not below the right cursor: row {}", row);
    close_with_esc(&zellij, prompt);
    zellij.quit();
}

#[test]
fn at_mouse_without_mouse_activity_falls_back_to_the_cursor() {
    let (mut zellij, left_terminal) = start_with_terminal();
    let _right_terminal = split_right_and_wait_for_prompt(&zellij);
    let (prompt, grid_snapshot) =
        open_menu_from(&zellij, &left_terminal, &["--at-mouse"], "NOMOUSE-ITEM");
    let (_, left, _) = frame_edges(&grid_snapshot, "NOMOUSE-ITEM");
    assert!(
        left >= 60,
        "popup not at the focused cursor: column {}",
        left
    );
    close_with_esc(&zellij, prompt);
    zellij.quit();
}

#[test]
fn menu_prints_the_value_of_the_chosen_item() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(
        &["menu", "--item", "o=Open file", "--item", "rm=Delete file"],
        from(&terminal),
    );
    wait_for_prompt_showing(&zellij, "Delete file");
    zellij.send_stdin(DOWN);
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "rm\n");
    zellij.quit();
}

#[test]
fn number_steps_with_the_arrows_and_prints_the_value() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(
        &[
            "number",
            "Port",
            "--min",
            "1",
            "--max",
            "10",
            "--default",
            "5",
        ],
        from(&terminal),
    );
    wait_for_prompt_showing(&zellij, "Port");
    zellij.send_stdin(RIGHT);
    zellij.send_stdin(RIGHT);
    zellij.wait_until("value stepped", |grid_snapshot| {
        grid_snapshot.contains(" 7 ")
    });
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "7\n");
    zellij.quit();
}

#[test]
fn number_refuses_a_typed_value_outside_the_range_and_centers_the_label() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(
        &[
            "number",
            "Port",
            "--min",
            "1",
            "--max",
            "10",
            "--default",
            "5",
        ],
        from(&terminal),
    );
    let grid_snapshot = wait_for_prompt_showing(&zellij, "Port");
    let (row, left, right) = frame_edges(&grid_snapshot, "Port");
    let line = &grid_snapshot.lines()[row];
    let label_column = line[..line.find("Port").unwrap()].chars().count();
    assert!(
        (label_column + 2).abs_diff((left + right) / 2) <= 1,
        "label not centered"
    );
    type_text(&zellij, "42");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("range error shown", |grid_snapshot| {
        grid_snapshot.contains("Must be between 1 and 10")
    });
    assert!(!prompt.has_exited());
    zellij.send_stdin(&[0x7f]);
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "4\n");
    zellij.quit();
}

#[test]
fn toggle_shows_the_label_above_the_switch_inside_an_unbroken_frame() {
    let (mut zellij, terminal) = start_with_terminal();
    let prompt = zellij.run_prompt(&["toggle", "Enable CI"], from(&terminal));
    let grid_snapshot = wait_for_prompt_showing(&zellij, "Enable CI");
    let (label_row, left, right) = frame_edges(&grid_snapshot, "Enable CI");
    let lines = grid_snapshot.lines();
    let label_line: Vec<char> = lines[label_row].chars().collect();
    let label_start = lines[label_row].find("Enable CI").unwrap();
    let label_column = lines[label_row][..label_start].chars().count();
    let middle = (left + right) / 2;
    assert!(
        (label_column + 4).abs_diff(middle) <= 1,
        "label not centered: {} in {}..{}",
        label_column,
        left,
        right
    );
    assert_eq!(label_line[right], '│');
    assert!(lines[label_row + 1].chars().nth(left + 1) == Some(' '));
    assert!(lines[label_row + 2].contains("[off]") || lines[label_row + 2].contains("off"));
    for row in label_row..label_row + 4 {
        let chars: Vec<char> = lines[row].chars().collect();
        assert_eq!(chars[left], '│', "left border broken on row {}", row);
        assert_eq!(chars[right], '│', "right border broken on row {}", row);
    }
    zellij.send_stdin(&keys::TAB);
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "true\n");
    zellij.quit();
}

#[test]
fn choose_holds_several_megabytes_of_input_without_closing() {
    let (mut zellij, terminal) = start_with_terminal();
    let padding = "x".repeat(90);
    let lines: String = (0..40_000)
        .map(|i| format!("./dir/{:05}-{}\n", i, padding))
        .collect();
    let prompt = zellij.run_prompt(&["choose"], from(&terminal).with_stdin_text(&lines));
    zellij.wait_until("all lines read", |grid_snapshot| {
        grid_snapshot.contains("./dir/00000") && !grid_snapshot.contains("reading")
    });
    assert!(!prompt.has_exited());
    type_text(&zellij, "39999");
    zellij.wait_until("last line found", |grid_snapshot| {
        grid_snapshot.contains("./dir/39999")
    });
    zellij.send_stdin(&keys::ENTER);
    let result = prompt.wait_for_exit();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, format!("./dir/39999-{}\n", padding));
    zellij.quit();
}

#[test]
fn choose_reads_many_short_lines_quickly() {
    let (mut zellij, terminal) = start_with_terminal();
    let lines: String = (0..20_000).map(|i| format!("path/{:05}\n", i)).collect();
    let prompt = zellij.run_prompt(&["choose"], from(&terminal).with_stdin_text(&lines));
    zellij.wait_until("all lines read", |grid_snapshot| {
        grid_snapshot.contains("path/00000") && !grid_snapshot.contains("reading")
    });
    type_text(&zellij, "19999");
    zellij.wait_until("last line found", |grid_snapshot| {
        grid_snapshot.contains("path/19999")
    });
    zellij.send_stdin(&keys::ENTER);
    answered(&prompt.wait_for_exit(), 0, "path/19999\n");
    zellij.quit();
}

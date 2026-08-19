#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    col, composite_contains_settled_guest_grid, guest_ascended_bar_settled,
    host_descended_bar_settled, host_descended_bar_with_ascend_keys_settled, keys, normalized,
    wait_for_settled_composite, GridSnapshot, NestedDepthThreeHarness, NestedHarness, PROMPT,
    TERMINAL_SIZE,
};
use zellij_utils::data::Direction;
use zellij_utils::nested_session::NestedSessionMessage;

const ARROW_UP: &[u8] = b"\x1b[A";
const ARROW_LEFT: &[u8] = b"\x1b[D";
const ARROW_RIGHT: &[u8] = b"\x1b[C";
const ARROW_DOWN: &[u8] = b"\x1b[B";
const ASCEND_KEY: &[u8] = b"]";
const DESCEND_KEY: &[u8] = b"[";
const ALT_ARROW_RIGHT: &[u8] = b"\x1b[1;3C";

fn sgr_mouse_report(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}M", button, column, line).into_bytes()
}

fn prompt_count(grid_snapshot: &GridSnapshot) -> usize {
    grid_snapshot.text.matches('$').count()
}

fn session_mode_bar_settled(grid_snapshot: &GridSnapshot) -> bool {
    last_line_contains(grid_snapshot, "SESSION") && last_line_contains(grid_snapshot, "Detach")
}

fn last_line_contains(grid_snapshot: &GridSnapshot, needle: &str) -> bool {
    grid_snapshot
        .lines()
        .last()
        .map_or(false, |last_line| last_line.contains(needle))
}

fn normal_mode_bar_settled(grid_snapshot: &GridSnapshot) -> bool {
    last_line_contains(grid_snapshot, "LOCK")
}

fn pane_mode_bar_settled(grid_snapshot: &GridSnapshot) -> bool {
    last_line_contains(grid_snapshot, "Fullscreen")
}

fn guest_pane_mode_bar_settled(grid_snapshot: &GridSnapshot) -> bool {
    last_line_contains(grid_snapshot, "PANE") && last_line_contains(grid_snapshot, "Move")
}

fn guest_tab_mode_bar_settled(grid_snapshot: &GridSnapshot) -> bool {
    last_line_contains(grid_snapshot, "TAB") && last_line_contains(grid_snapshot, "New")
}

fn guest_normal_mode_bar_settled(guest_grid: &GridSnapshot) -> bool {
    guest_grid.lines().last().map_or(false, |last_line| {
        if last_line.contains("LOCK") {
            return true;
        }
        let tokens: Vec<String> = last_line
            .replace('\u{e0b0}', " ")
            .split_whitespace()
            .map(|token| token.to_owned())
            .collect();
        ["g", "p", "t", "n", "h", "s", "o", "q"]
            .iter()
            .all(|mode_letter| tokens.iter().any(|token| token == mode_letter))
    })
}

fn guest_ui_settled(guest_grid: &GridSnapshot) -> bool {
    let lines = guest_grid.lines();
    lines
        .first()
        .map_or(false, |first_line| first_line.contains("Tab #1"))
        && last_line_contains(guest_grid, "Ctrl +")
        && guest_normal_mode_bar_settled(guest_grid)
}

fn single_blank_pane_guest_settled(guest_grid: &GridSnapshot) -> bool {
    guest_ui_settled(guest_grid) && guest_grid.cursor_is_at(col(0).row(1))
}

fn guest_ascended_ui_settled(guest_grid: &GridSnapshot) -> bool {
    guest_grid
        .lines()
        .first()
        .map_or(false, |first_line| first_line.contains("Tab #1"))
        && guest_ascended_bar_settled(guest_grid)
}

fn ascended_single_blank_pane_guest_settled(guest_grid: &GridSnapshot) -> bool {
    guest_ascended_ui_settled(guest_grid) && prompt_count(guest_grid) == 0
}

fn ascended_two_pane_guest_settled(guest_grid: &GridSnapshot) -> bool {
    guest_ascended_ui_settled(guest_grid)
        && guest_grid.contains("Pane #1")
        && guest_grid.contains("Pane #2")
        && prompt_count(guest_grid) == 1
}

fn two_pane_guest_settled(guest_grid: &GridSnapshot) -> bool {
    guest_ui_settled(guest_grid)
        && guest_grid.contains("Pane #1")
        && guest_grid.contains("Pane #2")
        && prompt_count(guest_grid) == 1
        && guest_grid.cursor.is_some()
}

fn host_own_tab_bar_settled(grid_snapshot: &GridSnapshot, guest_session_name: &str) -> bool {
    let lines = grid_snapshot.lines();
    let first_row_is_host_tab_bar = lines.first().map_or(false, |first_line| {
        first_line.contains("Tab #1") && !first_line.contains(guest_session_name)
    });
    let guest_pane_title_row_present = lines.get(1).map_or(false, |second_line| {
        second_line.contains(guest_session_name)
    });
    first_row_is_host_tab_bar && guest_pane_title_row_present
}

fn host_tab_bar_renamed_to_guest(grid_snapshot: &GridSnapshot, guest_session_name: &str) -> bool {
    grid_snapshot
        .lines()
        .first()
        .map_or(false, |first_line| first_line.contains(guest_session_name))
}

fn host_chrome_dimmed(host_grid: &GridSnapshot) -> bool {
    host_grid.line_has_dim("Pane #2") && host_grid.char_dim_of("test-").map_or(false, |dim| dim)
}

fn host_chrome_undimmed(host_grid: &GridSnapshot) -> bool {
    !host_grid.line_has_dim("Pane #2") && host_grid.char_dim_of("test-").map_or(true, |dim| !dim)
}

#[test]
fn a_guest_zellij_running_inside_a_host_zellij_pane_introduces_itself_and_is_kept_alive() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.descend_into_guest_via_modal();
    nested.wait_for_host_to_ping_guest();
    nested.wait_for_guest_to_reply_to_ping();

    nested.guest.wait_for_app_load();
    nested.host.wait_for_app_load();

    nested.guest.quit();
    nested.wait_for_host_to_release_guest_focus();
    nested.host.quit();
}

#[test]
fn a_host_zellij_stops_pinging_a_guest_that_freezes_inside_one_of_its_panes() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.wait_for_host_to_ping_guest();

    nested.freeze_guest();
    nested.assert_host_stops_pinging_frozen_guest();

    nested.guest.quit();
    nested.host.quit();
}

fn boot_and_descend_on_first_load(nested: &NestedHarness) {
    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.descend_into_guest_via_modal();
    nested.guest.wait_for_app_load();
    nested.wait_for_host_to_ping_guest();
    nested.wait_for_guest_to_reply_to_ping();
}

fn ascend_via_focus_host_binding(nested: &NestedHarness) {
    let requested = nested.mark_guest_to_host();
    let ascended = nested.mark_host_to_guest();
    nested.guest.wait_until(
        "guest settled in normal mode before the mode key is passed through",
        guest_ui_settled,
    );
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.guest.wait_until(
        "guest entered session mode after the passed-through mode key",
        session_mode_bar_settled,
    );
    nested.host.send_stdin(ASCEND_KEY);
    nested.wait_for_guest_to_request_host_focus_after(requested);
    nested.wait_for_host_to_ascend_from_guest_after(ascended);
}

fn split_host_sibling(nested: &NestedHarness) {
    nested.host.send_stdin(&keys::ctrl('p'));
    nested.host.wait_until(
        "host entered pane mode before the split key",
        pane_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::key('r'));
    let sibling = nested.host.expect_pty_spawn();
    sibling.output(PROMPT);
    nested.host.wait_until(
        "host returned to normal mode after the split key",
        |host_grid| normal_mode_bar_settled(host_grid) && prompt_count(host_grid) >= 1,
    );
}

fn descend_into_guest_on_the_left(nested: &NestedHarness) {
    let descended = nested.mark_host_to_guest();
    nested.host.send_stdin(&keys::ctrl('p'));
    nested.host.wait_until(
        "host entered pane mode before the focus-left key",
        pane_mode_bar_settled,
    );
    nested.host.send_stdin(ARROW_LEFT);
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.host.wait_until(
        "host entered session mode before the descend key",
        session_mode_bar_settled,
    );
    nested.host.send_stdin(DESCEND_KEY);
    nested.wait_for_host_to_descend_into_guest_after(descended);
}

fn build_descended_state_with_host_sibling(nested: &NestedHarness) {
    boot_and_descend_on_first_load(nested);
    ascend_via_focus_host_binding(nested);
    split_host_sibling(nested);
    descend_into_guest_on_the_left(nested);
}

fn split_host_sibling_below(nested: &NestedHarness) {
    nested.host.send_stdin(&keys::ctrl('p'));
    nested.host.wait_until(
        "host entered pane mode before the split-down key",
        pane_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::key('d'));
    let sibling = nested.host.expect_pty_spawn();
    sibling.output(PROMPT);
    nested.host.wait_until(
        "host returned to normal mode after the split-down key",
        |host_grid| normal_mode_bar_settled(host_grid) && prompt_count(host_grid) >= 1,
    );
}

fn descend_into_guest_above(nested: &NestedHarness) {
    let descended = nested.mark_host_to_guest();
    nested.host.send_stdin(&keys::ctrl('p'));
    nested.host.wait_until(
        "host entered pane mode before the focus-up key",
        pane_mode_bar_settled,
    );
    nested.host.send_stdin(ARROW_UP);
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.host.wait_until(
        "host entered session mode before the descend key",
        session_mode_bar_settled,
    );
    nested.host.send_stdin(DESCEND_KEY);
    nested.wait_for_host_to_descend_into_guest_after(descended);
}

fn build_descended_state_with_host_sibling_below(nested: &NestedHarness) {
    boot_and_descend_on_first_load(nested);
    ascend_via_focus_host_binding(nested);
    split_host_sibling_below(nested);
    descend_into_guest_above(nested);
}

fn quit_guest_then_host(mut nested: NestedHarness) {
    nested.guest.quit();
    nested.wait_for_host_to_reclaim_focus_after_guest_exit();
    nested.host.quit();
}

#[test]
fn host_keys_act_on_host_until_focus_enters_the_guest_then_route_to_guest() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    let guest_session_name = nested.guest.session_name().to_string();

    boot_and_descend_on_first_load(&nested);
    ascend_via_focus_host_binding(&nested);

    split_host_sibling(&nested);
    let host_focused_on_sibling = nested.wait_until_host_composites_settled_guest(
        "host acted on its own split keybind and created a sibling pane",
        ascended_single_blank_pane_guest_settled,
        |host_grid| {
            normal_mode_bar_settled(host_grid)
                && host_grid.contains("Pane #2")
                && prompt_count(host_grid) == 1
                && host_own_tab_bar_settled(host_grid, &guest_session_name)
        },
    );
    assert_snapshot!(
        "host_keys_act_on_host_before_descend",
        normalized(&host_focused_on_sibling)
    );

    descend_into_guest_on_the_left(&nested);
    let host_descended = nested.wait_until_host_composites_settled_guest(
        "host descended into the guest pane, showing the guest nested inside",
        single_blank_pane_guest_settled,
        |host_grid| {
            host_descended_bar_settled(host_grid)
                && host_grid.contains("Pane #2")
                && prompt_count(host_grid) == 1
                && host_own_tab_bar_settled(host_grid, &guest_session_name)
        },
    );
    assert_snapshot!("host_grid_while_descended", normalized(&host_descended));

    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    let guest_after_new_pane = nested.guest.wait_until(
        "guest spawned a second pane from the passed-through key",
        two_pane_guest_settled,
    );
    assert_snapshot!(
        "guest_reacts_to_passed_through_new_pane_key",
        normalized(&guest_after_new_pane)
    );

    let host_pane_count_unchanged = nested.wait_until_host_composites_settled_guest(
        "host still shows exactly its own two panes while the guest gained an inner pane",
        two_pane_guest_settled,
        |host_grid| {
            host_descended_bar_settled(host_grid)
                && host_grid.contains("Pane #2")
                && !host_grid.contains("Pane #3")
                && host_own_tab_bar_settled(host_grid, &guest_session_name)
        },
    );
    assert_snapshot!(
        "host_pane_count_unchanged_while_descended",
        normalized(&host_pane_count_unchanged)
    );

    quit_guest_then_host(nested);
}

#[test]
fn focus_host_session_binding_returns_control_to_host() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);
    let guest_session_name = nested.guest.session_name().to_string();

    build_descended_state_with_host_sibling(&nested);

    let requested = nested.mark_guest_to_host();
    let ascended = nested.mark_host_to_guest();
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.host.send_stdin(ASCEND_KEY);
    nested.wait_for_guest_to_request_host_focus_after(requested);
    nested.wait_for_host_to_ascend_from_guest_after(ascended);

    let host_ascended = nested.wait_until_host_composites_settled_guest(
        "host took control back from the guest",
        ascended_single_blank_pane_guest_settled,
        |host_grid| {
            normal_mode_bar_settled(host_grid)
                && host_grid.contains("Pane #2")
                && prompt_count(host_grid) == 1
                && host_own_tab_bar_settled(host_grid, &guest_session_name)
        },
    );
    assert_snapshot!(
        "host_ascended_after_focus_host_binding",
        normalized(&host_ascended)
    );

    nested.host.send_stdin(&keys::alt('n'));
    let host_new_pane = nested.host.expect_pty_spawn();
    host_new_pane.output(PROMPT);
    let host_after_new_pane = nested.wait_until_host_composites_settled_guest(
        "host acted on its own key after ascending",
        ascended_single_blank_pane_guest_settled,
        |host_grid| {
            normal_mode_bar_settled(host_grid)
                && host_grid.contains("Pane #3")
                && prompt_count(host_grid) == 2
                && host_own_tab_bar_settled(host_grid, &guest_session_name)
        },
    );
    assert_snapshot!(
        "host_acts_on_key_after_ascend",
        normalized(&host_after_new_pane)
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn clicking_a_host_pane_while_descended_refocuses_and_ascends() {
    let mut nested = NestedHarness::start_with_host_config(TERMINAL_SIZE, "mouse_mode true");
    let guest_session_name = nested.guest.session_name().to_string();

    build_descended_state_with_host_sibling(&nested);

    let ascended = nested.mark_host_to_guest();
    nested.host.send_stdin(&sgr_mouse_report(90, 8, 0));
    nested.wait_for_host_to_ascend_from_guest_after(ascended);

    let host_after_click = nested.wait_until_host_composites_settled_guest(
        "host focus moved onto the clicked host pane",
        ascended_single_blank_pane_guest_settled,
        |host_grid| {
            normal_mode_bar_settled(host_grid)
                && host_grid.contains("Pane #2")
                && prompt_count(host_grid) == 1
                && host_own_tab_bar_settled(host_grid, &guest_session_name)
        },
    );
    assert_snapshot!(
        "host_ascended_after_mouse_click",
        normalized(&host_after_click)
    );

    nested.host.send_stdin(&keys::alt('n'));
    let host_new_pane = nested.host.expect_pty_spawn();
    host_new_pane.output(PROMPT);
    let host_after_new_pane = nested.wait_until_host_composites_settled_guest(
        "host acted on its own key after the click ascended it",
        ascended_single_blank_pane_guest_settled,
        |host_grid| {
            normal_mode_bar_settled(host_grid)
                && host_grid.contains("Pane #3")
                && prompt_count(host_grid) == 2
                && host_own_tab_bar_settled(host_grid, &guest_session_name)
        },
    );
    assert_snapshot!(
        "host_acts_on_key_after_mouse_ascend",
        normalized(&host_after_new_pane)
    );

    nested.guest.quit();
    nested.host.quit();
}

fn sgr_left_click(column: usize, line: usize) -> Vec<u8> {
    format!(
        "\u{1b}[<0;{};{}M\u{1b}[<0;{};{}m",
        column, line, column, line
    )
    .into_bytes()
}

fn display_column_of(line: &str, needle: &str) -> Option<usize> {
    line.find(needle)
        .map(|byte_offset| line[..byte_offset].chars().count())
}

#[test]
fn clicking_the_host_new_tab_button_while_descended_ascends_and_keeps_working() {
    let mut nested = NestedHarness::start_with_host_config(TERMINAL_SIZE, "mouse_mode true");
    let guest_session_name = nested.guest.session_name().to_string();

    boot_and_descend_on_first_load(&nested);
    nested.guest.wait_for_app_load();

    let host_before = nested.host.wait_until(
        "the host tab bar shows the new tab button while descended",
        |host_grid| {
            host_grid
                .lines()
                .first()
                .is_some_and(|tab_bar| tab_bar.contains('+'))
        },
    );
    let tab_bar = host_before.lines().first().cloned().unwrap();
    let new_tab_button_column =
        display_column_of(&tab_bar, "+").expect("new tab button is on the host tab bar") + 1;

    let ascended = nested.mark_host_to_guest();
    nested
        .host
        .send_stdin(&sgr_left_click(new_tab_button_column, 1));
    let host_new_tab_pane = nested.host.expect_pty_spawn();
    host_new_tab_pane.output(PROMPT);
    nested.wait_for_host_to_ascend_from_guest_after(ascended);

    let host_after_new_tab = nested.host.wait_until(
        "the host opened a second tab and undimmed after clicking the new tab button",
        |host_grid| {
            host_grid.lines().first().map_or(false, |tab_bar| {
                tab_bar.contains("Tab #1") && tab_bar.contains("Tab #2")
            }) && normal_mode_bar_settled(host_grid)
        },
    );
    assert_snapshot!(
        "host_ascended_after_clicking_new_tab_button",
        normalized(&host_after_new_tab)
    );

    nested.host.send_stdin(&keys::alt('n'));
    let host_new_pane = nested.host.expect_pty_spawn();
    host_new_pane.output(PROMPT);
    nested.host.wait_until(
        "the host acts on its own key after clicking the new tab button ascended it",
        |host_grid| host_grid.contains("Pane #2") && normal_mode_bar_settled(host_grid),
    );

    let _ = guest_session_name;
    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn clicking_a_host_tab_while_descended_switches_tabs_and_ascends() {
    let mut nested = NestedHarness::start_with_host_config(TERMINAL_SIZE, "mouse_mode true");
    let guest_session_name = nested.guest.session_name().to_string();

    boot_and_descend_on_first_load(&nested);
    nested.guest.wait_for_app_load();

    let host_before = nested.host.wait_until(
        "the host tab bar shows the new tab button while descended",
        |host_grid| {
            host_grid
                .lines()
                .first()
                .is_some_and(|tab_bar| tab_bar.contains('+'))
        },
    );
    let tab_bar = host_before.lines().first().cloned().unwrap();
    let new_tab_button_column =
        display_column_of(&tab_bar, "+").expect("new tab button is on the host tab bar") + 1;

    let ascended_by_new_tab = nested.mark_host_to_guest();
    nested
        .host
        .send_stdin(&sgr_left_click(new_tab_button_column, 1));
    let host_new_tab_pane = nested.host.expect_pty_spawn();
    host_new_tab_pane.output(PROMPT);
    nested.wait_for_host_to_ascend_from_guest_after(ascended_by_new_tab);
    let host_two_tabs = nested.host.wait_until(
        "the host now has two tabs with the second one focused on a host pane",
        |host_grid| {
            host_grid.lines().first().map_or(false, |tab_bar| {
                tab_bar.contains("Tab #1") && tab_bar.contains("Tab #2")
            }) && normal_mode_bar_settled(host_grid)
        },
    );
    let first_tab_column = host_two_tabs
        .lines()
        .first()
        .and_then(|tab_bar| display_column_of(tab_bar, "Tab #1"))
        .expect("the first host tab ribbon is on the host tab bar")
        + 1;
    let second_tab_column = host_two_tabs
        .lines()
        .first()
        .and_then(|tab_bar| display_column_of(tab_bar, "Tab #2"))
        .expect("the second host tab ribbon is on the host tab bar")
        + 1;

    let descended_by_tab_one = nested.mark_host_to_guest();
    nested.host.send_stdin(&sgr_left_click(first_tab_column, 1));
    nested.wait_for_host_to_descend_into_guest_after(descended_by_tab_one);

    let ascended_by_tab_two = nested.mark_host_to_guest();
    nested
        .host
        .send_stdin(&sgr_left_click(second_tab_column, 1));
    nested.wait_for_host_to_ascend_from_guest_after(ascended_by_tab_two);
    nested.host.wait_until(
        "clicking the second host tab switches away from the guest tab in normal mode",
        |host_grid| {
            host_grid.lines().first().map_or(false, |tab_bar| {
                tab_bar.contains("Tab #1") && tab_bar.contains("Tab #2")
            }) && normal_mode_bar_settled(host_grid)
        },
    );

    nested.host.send_stdin(&keys::alt('n'));
    let host_new_pane = nested.host.expect_pty_spawn();
    host_new_pane.output(PROMPT);
    nested.host.wait_until(
        "the host acts on its own key after the tab click ascended it",
        |host_grid| host_grid.contains("Pane #2") && normal_mode_bar_settled(host_grid),
    );

    let _ = guest_session_name;
    nested.guest.quit();
    nested.host.quit();
}

fn stdin_contains(stdin: &[u8], needle: &[u8]) -> bool {
    stdin.windows(needle.len()).any(|window| window == needle)
}

#[test]
fn alt_clicking_the_guest_while_descended_reaches_the_guest_and_keeps_keys_flowing() {
    let mut nested = NestedHarness::start_with_host_and_guest_config(
        TERMINAL_SIZE,
        "mouse_mode true",
        "mouse_mode true\nadvanced_mouse_actions false",
    );

    boot_and_descend_on_first_load(&nested);
    nested.guest.wait_for_app_load();
    nested.wait_until_host_composites_settled_guest(
        "the host composited the descended guest before the alt click",
        single_blank_pane_guest_settled,
        host_descended_bar_settled,
    );

    nested.host.send_stdin(&sgr_mouse_report(30, 8, 8));
    nested.host_pane.wait_for_stdin(
        "the alt-modified mouse report to be written down into the guest pane",
        |stdin| stdin_contains(stdin, b"\x1b[<8;30;"),
    );

    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    let guest_after_new_pane = nested.guest.wait_until(
        "the key after the alt click still reached the guest",
        two_pane_guest_settled,
    );
    assert_snapshot!(
        "guest_reacts_to_key_after_alt_click",
        normalized(&guest_after_new_pane)
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn alt_wheel_over_the_descended_guest_reaches_the_guest_instead_of_jumping_host_prompts() {
    let mut nested = NestedHarness::start_with_host_and_guest_config(
        TERMINAL_SIZE,
        "mouse_mode true\nadvanced_mouse_actions true",
        "mouse_mode true\nadvanced_mouse_actions false",
    );

    boot_and_descend_on_first_load(&nested);
    nested.guest.wait_for_app_load();
    nested.wait_until_host_composites_settled_guest(
        "the host composited the descended guest before the alt wheel",
        single_blank_pane_guest_settled,
        host_descended_bar_settled,
    );

    nested.host.send_stdin(&sgr_mouse_report(30, 8, 72));
    nested.host_pane.wait_for_stdin(
        "the alt-modified wheel report to be written down into the guest pane",
        |stdin| stdin_contains(stdin, b"\x1b[<72;30;"),
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn alt_wheel_over_an_undescended_guest_still_reaches_the_guest() {
    let mut nested = NestedHarness::start_with_host_and_guest_config(
        TERMINAL_SIZE,
        "mouse_mode true\nadvanced_mouse_actions true",
        "mouse_mode true\nadvanced_mouse_actions false",
    );

    boot_and_descend_on_first_load(&nested);
    ascend_via_focus_host_binding(&nested);

    nested.host.send_stdin(&sgr_mouse_report(30, 8, 72));
    nested.host_pane.wait_for_stdin(
        "the alt-modified wheel report to reach a guest the host has ascended out of",
        |stdin| stdin_contains(stdin, b"\x1b[<72;30;"),
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn a_key_immediately_after_descend_lands_in_the_guest_and_after_ascend_lands_in_the_host() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);
    let guest_session_name = nested.guest.session_name().to_string();

    boot_and_descend_on_first_load(&nested);
    ascend_via_focus_host_binding(&nested);
    split_host_sibling(&nested);

    descend_into_guest_on_the_left(&nested);
    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    let guest_end_state = nested.guest.wait_until(
        "key immediately after descend landed in the guest",
        two_pane_guest_settled,
    );
    assert_snapshot!(
        "key_after_descend_lands_in_guest",
        normalized(&guest_end_state)
    );

    let requested = nested.mark_guest_to_host();
    let ascended = nested.mark_host_to_guest();
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.host.send_stdin(ASCEND_KEY);
    nested.wait_for_guest_to_request_host_focus_after(requested);
    nested.wait_for_host_to_ascend_from_guest_after(ascended);
    nested.host.send_stdin(&keys::alt('n'));
    let host_new_pane = nested.host.expect_pty_spawn();
    host_new_pane.output(PROMPT);
    let host_end_state = nested.wait_until_host_composites_settled_guest(
        "key immediately after ascend landed in the host",
        ascended_two_pane_guest_settled,
        |host_grid| {
            normal_mode_bar_settled(host_grid)
                && host_grid.contains("Pane #3")
                && prompt_count(host_grid) == 3
                && host_own_tab_bar_settled(host_grid, &guest_session_name)
        },
    );
    assert_snapshot!(
        "key_after_ascend_lands_in_host",
        normalized(&host_end_state)
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn descend_on_first_load_when_guest_pane_is_already_focused() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);
    let guest_session_name = nested.guest.session_name().to_string();

    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.descend_into_guest_via_modal();
    assert_eq!(
        nested.focus_gained_count(),
        1,
        "the host should descend exactly once after the modal is answered without any focus change"
    );
    assert_eq!(
        nested.focus_lost_count(),
        0,
        "the host should not ascend on first load"
    );

    nested.guest.wait_for_app_load();
    let host_descended_on_load = nested.wait_until_host_composites_settled_guest(
        "host descended into the already-focused guest pane at load",
        single_blank_pane_guest_settled,
        |host_grid| {
            host_descended_bar_with_ascend_keys_settled(host_grid)
                && host_tab_bar_renamed_to_guest(host_grid, &guest_session_name)
        },
    );
    assert_snapshot!(
        "host_descended_on_first_load",
        normalized(&host_descended_on_load)
    );

    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    let guest_after_new_pane = nested.guest.wait_until(
        "guest reacted to the passed-through key after a first-load descend",
        two_pane_guest_settled,
    );
    assert_snapshot!(
        "guest_reacts_after_first_load_descend",
        normalized(&guest_after_new_pane)
    );

    nested.guest.quit();
    nested.wait_for_host_to_reclaim_focus_after_guest_exit();
    nested.host.quit();
}

#[test]
fn guest_going_away_clears_passthrough() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);
    let guest_session_name = nested.guest.session_name().to_string();

    build_descended_state_with_host_sibling(&nested);
    nested.wait_until_host_composites_settled_guest(
        "host settled the descended guest view before the guest froze",
        single_blank_pane_guest_settled,
        |host_grid| {
            host_descended_bar_settled(host_grid)
                && host_grid.contains("Pane #2")
                && prompt_count(host_grid) == 1
                && host_own_tab_bar_settled(host_grid, &guest_session_name)
        },
    );

    nested.freeze_guest();
    nested.assert_host_stops_pinging_frozen_guest();
    nested.wait_for_host_to_reclaim_focus_after_guest_exit();

    nested.host.send_stdin(&keys::alt('n'));
    let host_new_pane = nested.host.expect_pty_spawn();
    host_new_pane.output(PROMPT);
    let host_after_guest_gone = nested.host.wait_until(
        "host acts on its own key after the guest went away",
        |host_grid| {
            normal_mode_bar_settled(host_grid)
                && host_grid.contains("Pane #3")
                && prompt_count(host_grid) == 2
                && host_grid.cursor.is_some()
                && host_own_tab_bar_settled(host_grid, &guest_session_name)
        },
    );
    assert_snapshot!(
        "host_acts_after_guest_cleared_passthrough",
        normalized(&host_after_guest_gone)
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn descend_and_ascend_work_at_depth_three() {
    let mut nested = NestedDepthThreeHarness::start_depth_three(TERMINAL_SIZE);
    let inner_session_name = nested.inner.session_name().to_string();

    nested.boot_and_descend_depth_three();
    nested.inner.wait_for_app_load();

    let middle_descended_into_settled_inner = |middle_grid: &GridSnapshot| {
        let inner_grid = nested.inner.snapshot();
        single_blank_pane_guest_settled(&inner_grid)
            && host_descended_bar_settled(middle_grid)
            && middle_grid
                .lines()
                .first()
                .map_or(false, |first_line| first_line.contains(&inner_session_name))
            && composite_contains_settled_guest_grid(middle_grid, &inner_grid)
    };
    let middle_ascended_from_settled_inner = |middle_grid: &GridSnapshot| {
        let inner_grid = nested.inner.snapshot();
        ascended_single_blank_pane_guest_settled(&inner_grid)
            && normal_mode_bar_settled(middle_grid)
            && middle_grid
                .lines()
                .first()
                .map_or(false, |first_line| first_line.contains(&inner_session_name))
            && composite_contains_settled_guest_grid(middle_grid, &inner_grid)
    };
    let settled_outer_tab_title = format!("{} | {}", inner_session_name, inner_session_name);
    let outer_settled = |outer_grid: &GridSnapshot| {
        host_descended_bar_settled(outer_grid)
            && outer_grid.lines().first().map_or(false, |first_line| {
                first_line.contains(&settled_outer_tab_title)
            })
    };

    let outer_doubly_nested = wait_for_settled_composite(
        &nested.outer,
        &nested.middle,
        "outer host shows the doubly-nested descended UI",
        &middle_descended_into_settled_inner,
        &outer_settled,
    );
    assert_snapshot!(
        "outer_grid_doubly_nested_descended",
        normalized(&outer_doubly_nested)
    );

    let requested = nested.mark_inner_to_middle();
    let ascended = nested.mark_middle_to_inner();
    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.inner.wait_until(
        "inner entered session mode after the passed-through mode key",
        session_mode_bar_settled,
    );
    nested.outer.send_stdin(ASCEND_KEY);
    nested.wait_for_inner_to_request_host_focus_after(requested);
    nested.wait_for_middle_to_ascend_from_inner_after(ascended);

    let outer_after_inner_ascend = wait_for_settled_composite(
        &nested.outer,
        &nested.middle,
        "middle ascended out of the inner while outer stays descended into middle",
        &middle_ascended_from_settled_inner,
        &outer_settled,
    );
    assert_snapshot!(
        "outer_grid_after_inner_ascend",
        normalized(&outer_after_inner_ascend)
    );

    nested.inner.quit();

    nested.middle.quit();
    nested.wait_for_outer_to_reclaim_focus_after_middle_exit();

    nested.outer.quit();
}

fn pass_ctrl_p_to_guest(nested: &NestedHarness) {
    nested.host.send_stdin(&keys::ctrl('p'));
    nested.guest.wait_until(
        "guest entered pane mode after the passed-through mode key",
        guest_pane_mode_bar_settled,
    );
}

fn split_second_guest_pane(nested: &NestedHarness) {
    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    nested.guest.wait_until(
        "guest spawned a second pane from the passed-through key",
        two_pane_guest_settled,
    );
}

#[test]
fn descended_client_sees_dimmed_host_chrome_while_second_client_does_not() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    let guest_session_name = nested.guest.session_name().to_string();

    boot_and_descend_on_first_load(&nested);
    ascend_via_focus_host_binding(&nested);
    split_host_sibling(&nested);

    let second = nested.host.attach_client(TERMINAL_SIZE);
    let second_client = second.wait_until(
        "the second host client focuses the sibling pane undimmed on attach",
        |host_grid| {
            host_own_tab_bar_settled(host_grid, &guest_session_name)
                && host_grid.contains("Pane #2")
                && host_chrome_undimmed(host_grid)
        },
    );

    let descended = nested.mark_host_to_guest();
    nested.host.send_stdin(&keys::ctrl('p'));
    nested.host.wait_until(
        "first client entered pane mode before moving focus onto the guest",
        pane_mode_bar_settled,
    );
    nested.host.send_stdin(ARROW_LEFT);
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.host.wait_until(
        "first client entered session mode before the descend key",
        session_mode_bar_settled,
    );
    nested.host.send_stdin(DESCEND_KEY);
    nested.wait_for_host_to_descend_into_guest_after(descended);

    let descended_client = nested.host.wait_until(
        "the first host client renders dimmed host chrome while descended",
        |host_grid| {
            host_grid.contains("Pane #2")
                && host_grid.contains("Tab #1")
                && host_chrome_dimmed(host_grid)
        },
    );

    let second_still_undimmed = second.wait_until(
        "the second host client stays undimmed while the first is descended",
        |host_grid| host_grid.contains("Pane #2") && host_chrome_undimmed(host_grid),
    );

    assert!(host_chrome_dimmed(&descended_client));
    assert!(host_chrome_undimmed(&second_client));
    assert!(host_chrome_undimmed(&second_still_undimmed));

    second.quit();
    quit_guest_then_host(nested);
}

fn host_chrome_rows(host_grid: &GridSnapshot) -> (String, String) {
    let lines = host_grid.lines();
    let first = lines.first().cloned().unwrap_or_default();
    let last = lines.last().cloned().unwrap_or_default();
    (first, last)
}

#[test]
fn host_chrome_restores_exactly_after_ascend() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);
    let guest_session_name = nested.guest.session_name().to_string();

    boot_and_descend_on_first_load(&nested);
    ascend_via_focus_host_binding(&nested);
    split_host_sibling(&nested);

    let before_descend =
        nested
            .host
            .wait_until("host chrome is undimmed before descending", |host_grid| {
                host_own_tab_bar_settled(host_grid, &guest_session_name)
                    && host_grid.contains("Pane #2")
                    && normal_mode_bar_settled(host_grid)
                    && host_chrome_undimmed(host_grid)
            });
    let before_chrome = host_chrome_rows(&before_descend);

    descend_into_guest_on_the_left(&nested);
    nested
        .host
        .wait_until("host chrome dims while descended", |host_grid| {
            host_own_tab_bar_settled(host_grid, &guest_session_name)
                && host_grid.contains("Pane #2")
                && host_chrome_dimmed(host_grid)
        });

    let released = nested.mark_host_to_guest();
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.host.send_stdin(ASCEND_KEY);
    nested.wait_for_host_to_ascend_from_guest_after(released);
    nested.host.send_stdin(&keys::ESC);
    let after_ascend = nested.host.wait_until(
        "host chrome restores to undimmed after ascending",
        |host_grid| {
            host_grid.contains("Pane #2")
                && normal_mode_bar_settled(host_grid)
                && host_chrome_undimmed(host_grid)
                && host_chrome_rows(host_grid) == before_chrome
        },
    );

    assert_eq!(
        host_chrome_rows(&after_ascend),
        before_chrome,
        "host chrome rows restore exactly after ascend"
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn guest_edge_focus_move_bubbles_out_to_the_adjacent_host_pane() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    build_descended_state_with_host_sibling(&nested);

    let requested = nested.mark_guest_to_host();
    let ascended = nested.mark_host_to_guest();
    pass_ctrl_p_to_guest(&nested);
    nested.host.send_stdin(ARROW_RIGHT);
    nested.guest_to_host().wait_for_after(
        requested,
        "the guest at its right edge bubbles a focus-host request to the right",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FocusHost {
                    direction: Some(Direction::Right)
                }
            )
        },
    );
    nested.wait_for_host_to_ascend_from_guest_after(ascended);

    let host_on_sibling = nested.host.wait_until(
        "host focus landed on the sibling pane to the right of the guest",
        |host_grid| {
            host_grid.contains("Pane #2")
                && host_grid.contains("Tab #1")
                && host_chrome_undimmed(host_grid)
        },
    );
    assert!(host_chrome_undimmed(&host_on_sibling));

    nested.host.send_stdin(&keys::alt('n'));
    let host_new_pane = nested.host.expect_pty_spawn();
    host_new_pane.output(PROMPT);
    nested.host.wait_until(
        "host acts on its own key after the guest bubbled focus out",
        |host_grid| host_grid.contains("Pane #3") && host_grid.contains("Tab #1"),
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn host_focus_move_onto_guest_pane_enters_at_the_correct_edge() {
    let nested = NestedHarness::start(TERMINAL_SIZE);

    build_descended_state_with_host_sibling(&nested);

    let requested = nested.mark_guest_to_host();
    let ascended = nested.mark_host_to_guest();
    pass_ctrl_p_to_guest(&nested);
    nested.host.send_stdin(ARROW_RIGHT);
    nested.guest_to_host().wait_for_after(
        requested,
        "the guest at its right edge bubbles a focus-host request to the right",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FocusHost {
                    direction: Some(Direction::Right)
                }
            )
        },
    );
    nested.wait_for_host_to_ascend_from_guest_after(ascended);

    let entered = nested.mark_host_to_guest();
    nested.host.send_stdin(&keys::ctrl('p'));
    nested.host.wait_until(
        "host entered pane mode before moving focus onto the guest",
        pane_mode_bar_settled,
    );
    nested.host.send_stdin(ARROW_LEFT);
    nested.host_to_guest().wait_for_after(
        entered,
        "the host tells the guest it entered from the left edge when moving left onto it",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FocusGained {
                    from_direction: Some(Direction::Left)
                }
            )
        },
    );

    quit_guest_then_host(nested);
}

#[test]
fn no_bubble_when_the_focus_move_stays_inside_the_guest() {
    let nested = NestedHarness::start(TERMINAL_SIZE);

    build_descended_state_with_host_sibling(&nested);
    split_second_guest_pane(&nested);

    nested.guest.wait_until("two panes", two_pane_guest_settled);
    pass_ctrl_p_to_guest(&nested);
    let focus_host_before = nested
        .guest_to_host()
        .count(|message| matches!(message, NestedSessionMessage::FocusHost { .. }));
    nested.host.send_stdin(ARROW_LEFT);
    nested.guest.wait_until(
        "guest focus moved to the left pane while staying in pane mode",
        |guest_grid| {
            guest_grid.cursor.map_or(false, |cursor| cursor.x < 15)
                && guest_pane_mode_bar_settled(guest_grid)
        },
    );
    assert_eq!(
        nested
            .guest_to_host()
            .count(|message| matches!(message, NestedSessionMessage::FocusHost { .. })),
        focus_host_before,
        "moving focus between two guest panes must not bubble a focus-host request"
    );

    quit_guest_then_host(nested);
}

#[test]
fn fullscreened_guest_pane_still_bubbles_at_the_edges() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    build_descended_state_with_host_sibling_below(&nested);
    split_second_guest_pane(&nested);

    pass_ctrl_p_to_guest(&nested);
    nested.host.send_stdin(&keys::key('f'));
    nested
        .guest
        .wait_until("guest fullscreened its focused pane", |guest_grid| {
            guest_grid.contains("FULLSCREEN") || guest_ui_settled(guest_grid)
        });

    let requested = nested.mark_guest_to_host();
    let ascended = nested.mark_host_to_guest();
    pass_ctrl_p_to_guest(&nested);
    nested.host.send_stdin(ARROW_DOWN);
    nested.guest_to_host().wait_for_after(
        requested,
        "the fullscreened guest bubbles a downward focus-host request off its edge",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FocusHost {
                    direction: Some(Direction::Down)
                }
            )
        },
    );
    nested.wait_for_host_to_ascend_from_guest_after(ascended);
    nested.host.wait_until(
        "host focus landed on the pane below the guest",
        |host_grid| {
            host_grid.contains("Pane #2")
                && host_grid.contains("Tab #1")
                && host_chrome_undimmed(host_grid)
        },
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn move_focus_or_tab_bubbles_only_when_the_guest_has_a_single_tab() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    build_descended_state_with_host_sibling(&nested);

    let requested = nested.mark_guest_to_host();
    let ascended = nested.mark_host_to_guest();
    nested.guest.wait_until(
        "guest settled in normal mode before the alt-arrow is passed through",
        guest_ui_settled,
    );
    nested.host.send_stdin(ALT_ARROW_RIGHT);
    nested.guest_to_host().wait_for_after(
        requested,
        "a single-tab guest bubbles the move-or-tab off its right edge",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FocusHost {
                    direction: Some(Direction::Right)
                }
            )
        },
    );
    nested.wait_for_host_to_ascend_from_guest_after(ascended);
    nested.host.wait_until(
        "host focus landed on the sibling after the single-tab move-or-tab bubbled",
        |host_grid| {
            host_grid.contains("Pane #2")
                && host_grid.contains("Tab #1")
                && host_chrome_undimmed(host_grid)
        },
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn move_focus_or_tab_wraps_tabs_without_bubbling_when_the_guest_has_multiple_tabs() {
    let nested = NestedHarness::start(TERMINAL_SIZE);

    build_descended_state_with_host_sibling(&nested);

    nested.host.send_stdin(&keys::ctrl('t'));
    nested.guest.wait_until(
        "guest entered tab mode after the passed-through key",
        guest_tab_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::key('n'));
    nested
        .guest
        .wait_until("guest opened a second tab and focused it", |guest_grid| {
            guest_grid.contains("Tab #2") && guest_normal_mode_bar_settled(guest_grid)
        });

    let focus_host_before = nested
        .guest_to_host()
        .count(|message| matches!(message, NestedSessionMessage::FocusHost { .. }));
    nested.host.send_stdin(ALT_ARROW_RIGHT);
    nested.guest.wait_until(
        "guest wrapped back to the first tab instead of bubbling",
        |guest_grid| {
            guest_grid.lines().first().map_or(false, |first_line| {
                first_line.contains("Tab #1") && first_line.contains("Tab #2")
            }) && guest_normal_mode_bar_settled(guest_grid)
        },
    );
    assert_eq!(
        nested
            .guest_to_host()
            .count(|message| matches!(message, NestedSessionMessage::FocusHost { .. })),
        focus_host_before,
        "a multi-tab guest wraps tabs and must not bubble a focus-host request"
    );

    quit_guest_then_host(nested);
}

#[test]
fn nested_fullscreen_exit_keeps_hidden_host_floating_panes_hidden() {
    let nested = NestedDepthThreeHarness::start_depth_three(TERMINAL_SIZE);

    nested.boot_and_descend_depth_three();
    nested.inner.wait_for_app_load();

    nested.outer.send_stdin(&keys::alt('f'));
    let inner_float = nested.inner.expect_pty_spawn();
    inner_float.output(b"INNER-FLOAT ");
    nested
        .inner
        .wait_until("inner floating pane shown", |inner_grid| {
            inner_grid.contains("INNER-FLOAT")
        });

    let requested = nested.mark_inner_to_middle();
    let ascended = nested.mark_middle_to_inner();
    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.inner.wait_until(
        "inner entered session mode after the passed-through mode key",
        session_mode_bar_settled,
    );
    nested.outer.send_stdin(ASCEND_KEY);
    nested.wait_for_inner_to_request_host_focus_after(requested);
    nested.wait_for_middle_to_ascend_from_inner_after(ascended);

    nested.outer.send_stdin(&keys::alt('f'));
    let middle_float = nested.middle.expect_pty_spawn();
    middle_float.output(b"MIDDLE-FLOAT ");
    nested
        .middle
        .wait_until("middle floating pane shown", |middle_grid| {
            middle_grid.contains("MIDDLE-FLOAT")
        });
    nested.outer.send_stdin(&keys::alt('f'));
    nested
        .middle
        .wait_until("middle floating pane hidden", |middle_grid| {
            !middle_grid.contains("MIDDLE-FLOAT")
        });

    nested.outer.send_stdin(&keys::ctrl('p'));
    nested.middle.wait_until(
        "middle entered pane mode before the split key",
        pane_mode_bar_settled,
    );
    nested.outer.send_stdin(&keys::key('r'));
    let middle_sibling = nested.middle.expect_pty_spawn();
    middle_sibling.output(b"MIDDLE-SIBLING ");
    nested.middle.wait_until(
        "middle spawned a sibling pane next to the inner guest",
        |middle_grid| middle_grid.contains("MIDDLE-SIBLING"),
    );

    let ascended_outer = nested.mark_outer_to_middle();
    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.middle.wait_until(
        "middle entered session mode before the ascend key",
        session_mode_bar_settled,
    );
    nested.outer.send_stdin(ASCEND_KEY);
    nested.wait_for_outer_to_ascend_from_middle_after(ascended_outer);

    nested.outer.send_stdin(&keys::alt('f'));
    let outer_float = nested.outer.expect_pty_spawn();
    outer_float.output(b"OUTER-FLOAT ");
    nested
        .outer
        .wait_until("outer floating pane shown", |outer_grid| {
            outer_grid.contains("OUTER-FLOAT")
        });
    nested.outer.send_stdin(&keys::alt('f'));
    nested
        .outer
        .wait_until("outer floating pane hidden", |outer_grid| {
            !outer_grid.contains("OUTER-FLOAT")
        });

    nested.outer.send_stdin(&keys::ctrl('p'));
    nested.outer.wait_until(
        "outer entered pane mode before the split key",
        pane_mode_bar_settled,
    );
    nested.outer.send_stdin(&keys::key('r'));
    let outer_sibling = nested.outer.expect_pty_spawn();
    outer_sibling.output(b"OUTER-SIBLING ");
    nested.outer.wait_until(
        "outer spawned a sibling pane next to the middle guest",
        |outer_grid| outer_grid.contains("OUTER-SIBLING"),
    );

    let outer_descended = nested.mark_outer_to_middle();
    nested.outer.send_stdin(&keys::ctrl('p'));
    nested.outer.wait_until(
        "outer entered pane mode before the focus-left key",
        pane_mode_bar_settled,
    );
    nested.outer.send_stdin(ARROW_LEFT);
    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.outer.wait_until(
        "outer entered session mode before the descend key",
        session_mode_bar_settled,
    );
    nested.outer.send_stdin(DESCEND_KEY);
    nested.wait_for_outer_to_descend_into_middle_after(outer_descended);

    let middle_descended = nested.mark_middle_to_inner();
    nested.outer.send_stdin(&keys::ctrl('p'));
    nested.middle.wait_until(
        "middle entered pane mode before the focus-left key",
        guest_pane_mode_bar_settled,
    );
    nested.outer.send_stdin(ARROW_LEFT);
    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.middle.wait_until(
        "middle entered session mode before the descend key",
        session_mode_bar_settled,
    );
    nested.outer.send_stdin(DESCEND_KEY);
    nested.wait_for_middle_to_descend_into_inner_after(middle_descended);

    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.outer.send_stdin(&keys::key('f'));
    nested.middle.wait_until(
        "middle hid its own chrome and sibling while the inner guest is fullscreen",
        |middle_grid| {
            middle_grid.contains("INNER-FLOAT")
                && !middle_grid.contains("MIDDLE-SIBLING")
                && !middle_grid.contains("MIDDLE-FLOAT")
        },
    );
    nested.outer.wait_until(
        "outer hid its own chrome and sibling while the nested guest chain is fullscreen",
        |outer_grid| {
            outer_grid.contains("INNER-FLOAT")
                && !outer_grid.contains("OUTER-SIBLING")
                && !outer_grid.contains("MIDDLE-SIBLING")
        },
    );

    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.outer.send_stdin(&keys::key('f'));
    nested.middle.wait_until(
        "middle restored its sibling pane after fullscreen exit",
        |middle_grid| middle_grid.contains("MIDDLE-SIBLING"),
    );
    nested.outer.wait_until(
        "outer restored its sibling pane after fullscreen exit",
        |outer_grid| outer_grid.contains("OUTER-SIBLING"),
    );

    let middle_after_exit = nested.middle.snapshot();
    let outer_after_exit = nested.outer.snapshot();
    assert!(
        !middle_after_exit.contains("MIDDLE-FLOAT") && !outer_after_exit.contains("OUTER-FLOAT"),
        "hidden host floating panes must stay hidden after nested fullscreen exit\n=== middle grid ===\n{}\n=== outer grid ===\n{}\n=== log tail ===\n{}",
        middle_after_exit.text,
        outer_after_exit.text,
        zellij_integration_tests::test_env::log_tail(400),
    );

    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.outer.send_stdin(&keys::key('f'));
    nested.middle.wait_until(
        "middle hid its sibling again during the second fullscreen",
        |middle_grid| !middle_grid.contains("MIDDLE-SIBLING"),
    );
    nested.outer.wait_until(
        "outer hid its sibling again during the second fullscreen",
        |outer_grid| !outer_grid.contains("OUTER-SIBLING"),
    );

    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.outer.send_stdin(&keys::key('f'));
    nested.middle.wait_until(
        "middle restored its sibling pane after the second fullscreen exit",
        |middle_grid| middle_grid.contains("MIDDLE-SIBLING"),
    );
    nested.outer.wait_until(
        "outer restored its sibling pane after the second fullscreen exit",
        |outer_grid| outer_grid.contains("OUTER-SIBLING"),
    );

    let middle_after_second_exit = nested.middle.snapshot();
    let outer_after_second_exit = nested.outer.snapshot();
    assert!(
        !middle_after_second_exit.contains("MIDDLE-FLOAT")
            && !outer_after_second_exit.contains("OUTER-FLOAT"),
        "hidden host floating panes must stay hidden after a repeated nested fullscreen exit\n=== middle grid ===\n{}\n=== outer grid ===\n{}\n=== log tail ===\n{}",
        middle_after_second_exit.text,
        outer_after_second_exit.text,
        zellij_integration_tests::test_env::log_tail(400),
    );
}

fn column_in_last_line(grid: &GridSnapshot, needle: &str) -> Option<(usize, usize)> {
    let lines = grid.lines();
    let y = lines.len().checked_sub(1)?;
    let line = lines.last()?;
    let byte_index = line.find(needle)?;
    Some((line[..byte_index].chars().count(), y))
}

fn descended_hint_present(grid: &GridSnapshot) -> bool {
    last_line_contains(grid, "Ascend:")
}

#[test]
fn descended_host_tab_bar_session_name_is_dimmed() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);
    let host_session_name = nested.host.session_name().to_string();

    build_descended_state_with_host_sibling(&nested);

    let descended = nested.host.wait_until(
        "descended host renders its dimmed tab-bar with the status hint",
        |host_grid| {
            host_grid.contains("Pane #2")
                && host_grid.contains(&format!("({})", host_session_name))
                && descended_hint_present(host_grid)
                && host_grid
                    .char_dim_of(&format!("({})", host_session_name))
                    .unwrap_or(false)
        },
    );

    assert!(
        descended
            .char_dim_of("Zellij")
            .expect("host tab-bar prefix present"),
        "the host tab-bar 'Zellij' prefix is dimmed while descended"
    );
    assert!(
        descended
            .char_dim_of(&format!("({})", host_session_name))
            .expect("host session name present"),
        "the host tab-bar session name is dimmed while descended"
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn descended_status_hint_is_dim_italic_with_colored_keys() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    build_descended_state_with_host_sibling(&nested);

    let descended = nested.host.wait_until(
        "descended host shows the nested session status hint",
        |host_grid| host_grid.contains("Pane #2") && descended_hint_present(host_grid),
    );

    let (label_column, label_row) = column_in_last_line(&descended, "Ascend:")
        .expect("the ascend hint label is present on the last line");
    assert!(
        descended.char_is_dim(label_column, label_row),
        "the ascend hint label is dimmed"
    );
    assert!(
        descended.char_is_italic(label_column, label_row),
        "the ascend hint label is italic"
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn nested_pane_frames_never_render_the_guest_choice_indicator() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    build_descended_state_with_host_sibling(&nested);

    let descended = nested.host.wait_until(
        "descended host settled with its guest pane and sibling present",
        |host_grid| host_grid.contains("Pane #2") && descended_hint_present(host_grid),
    );

    assert!(
        !descended.contains("NESTED ZELLIJ")
            && !descended.contains("NESTED AUTO FOCUS")
            && !descended.contains("NESTED MANUAL FOCUS"),
        "no guest-choice indicator is rendered in any pane frame while descended\n=== host grid ===\n{}",
        descended.text
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn a_guest_sessions_kitty_probe_is_answered_by_its_host_pane() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    nested.wait_for_host_to_acknowledge_guest();

    // A guest zellij writes its whole startup query batch in one chunk:
    // forwarded host queries (pixel size, fg/bg) precede the kitty probe, so
    // the host pane pauses part-way through that chunk. The probe still has
    // to be answered once the pane resumes, otherwise the guest concludes the
    // host cannot render images and every image tool inside it gives up.
    nested.host_pane.wait_for_stdin(
        "the guest's kitty probe is answered by the host pane",
        |stdin_bytes| {
            stdin_bytes
                .windows(b"\x1b_Gi=31;".len())
                .any(|window| window == b"\x1b_Gi=31;")
        },
    );
}

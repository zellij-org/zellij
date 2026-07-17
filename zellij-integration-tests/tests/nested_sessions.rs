#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    col, composite_contains_settled_guest_grid, keys, normalized, wait_for_settled_composite,
    GridSnapshot, NestedDepthThreeHarness, NestedHarness, PROMPT, TERMINAL_SIZE,
};

const ARROW_UP: &[u8] = b"\x1b[A";
const ARROW_LEFT: &[u8] = b"\x1b[D";

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

#[test]
fn a_guest_zellij_running_inside_a_host_zellij_pane_introduces_itself_and_is_kept_alive() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
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
    nested.wait_for_host_to_descend_into_guest();
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
    nested.host.send_stdin(ARROW_UP);
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
    nested.wait_for_host_to_descend_into_guest_after(descended);
}

fn build_descended_state_with_host_sibling(nested: &NestedHarness) {
    boot_and_descend_on_first_load(nested);
    ascend_via_focus_host_binding(nested);
    split_host_sibling(nested);
    descend_into_guest_on_the_left(nested);
}

fn quit_guest_then_host(mut nested: NestedHarness) {
    let released = nested.mark_host_to_guest();
    nested.guest.quit();
    nested.wait_for_host_to_ascend_from_guest_after(released);
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
        single_blank_pane_guest_settled,
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
            pane_mode_bar_settled(host_grid)
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
            pane_mode_bar_settled(host_grid)
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
    nested.host.send_stdin(ARROW_UP);
    nested.wait_for_guest_to_request_host_focus_after(requested);
    nested.wait_for_host_to_ascend_from_guest_after(ascended);

    let host_ascended = nested.wait_until_host_composites_settled_guest(
        "host took control back from the guest",
        single_blank_pane_guest_settled,
        |host_grid| {
            pane_mode_bar_settled(host_grid)
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
        single_blank_pane_guest_settled,
        |host_grid| {
            pane_mode_bar_settled(host_grid)
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
        single_blank_pane_guest_settled,
        |host_grid| {
            pane_mode_bar_settled(host_grid)
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
        single_blank_pane_guest_settled,
        |host_grid| {
            pane_mode_bar_settled(host_grid)
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
    nested.host.send_stdin(ARROW_UP);
    nested.wait_for_guest_to_request_host_focus_after(requested);
    nested.wait_for_host_to_ascend_from_guest_after(ascended);
    nested.host.send_stdin(&keys::alt('n'));
    let host_new_pane = nested.host.expect_pty_spawn();
    host_new_pane.output(PROMPT);
    let host_end_state = nested.wait_until_host_composites_settled_guest(
        "key immediately after ascend landed in the host",
        two_pane_guest_settled,
        |host_grid| {
            pane_mode_bar_settled(host_grid)
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
    nested.wait_for_host_to_descend_into_guest();
    assert_eq!(
        nested.focus_gained_count(),
        1,
        "the host should descend exactly once on first load without any focus change"
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
            normal_mode_bar_settled(host_grid)
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

    let released = nested.mark_host_to_guest();
    nested.guest.quit();
    nested.wait_for_host_to_ascend_from_guest_after(released);
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
            pane_mode_bar_settled(host_grid)
                && host_grid.contains("Pane #2")
                && prompt_count(host_grid) == 1
                && host_own_tab_bar_settled(host_grid, &guest_session_name)
        },
    );

    let ascended = nested.mark_host_to_guest();
    nested.freeze_guest();
    nested.assert_host_stops_pinging_frozen_guest();
    nested.wait_for_host_to_ascend_from_guest_after(ascended);

    nested.host.send_stdin(&keys::alt('n'));
    let host_new_pane = nested.host.expect_pty_spawn();
    host_new_pane.output(PROMPT);
    let host_after_guest_gone = nested.host.wait_until(
        "host acts on its own key after the guest went away",
        |host_grid| {
            pane_mode_bar_settled(host_grid)
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

    nested.wait_for_outer_to_descend_into_middle();
    nested.wait_for_middle_to_descend_into_inner();
    nested.inner.wait_for_app_load();

    let middle_shows_settled_inner = |middle_grid: &GridSnapshot| {
        let inner_grid = nested.inner.snapshot();
        single_blank_pane_guest_settled(&inner_grid)
            && normal_mode_bar_settled(middle_grid)
            && middle_grid
                .lines()
                .first()
                .map_or(false, |first_line| first_line.contains(&inner_session_name))
            && composite_contains_settled_guest_grid(middle_grid, &inner_grid)
    };
    let settled_outer_tab_title = format!("{} | {}", inner_session_name, inner_session_name);
    let outer_settled = |outer_grid: &GridSnapshot| {
        normal_mode_bar_settled(outer_grid)
            && outer_grid.lines().first().map_or(false, |first_line| {
                first_line.contains(&settled_outer_tab_title)
            })
    };

    let outer_doubly_nested = wait_for_settled_composite(
        &nested.outer,
        &nested.middle,
        "outer host shows the doubly-nested descended UI",
        &middle_shows_settled_inner,
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
    nested.outer.send_stdin(ARROW_UP);
    nested.wait_for_inner_to_request_host_focus_after(requested);
    nested.wait_for_middle_to_ascend_from_inner_after(ascended);

    let outer_after_inner_ascend = wait_for_settled_composite(
        &nested.outer,
        &nested.middle,
        "middle ascended out of the inner while outer stays descended into middle",
        &middle_shows_settled_inner,
        &outer_settled,
    );
    assert_snapshot!(
        "outer_grid_after_inner_ascend",
        normalized(&outer_after_inner_ascend)
    );

    nested.inner.quit();

    let outer_released_middle = nested.mark_outer_to_middle();
    nested.middle.quit();
    nested.wait_for_outer_to_ascend_from_middle_after(outer_released_middle);

    nested.outer.quit();
}

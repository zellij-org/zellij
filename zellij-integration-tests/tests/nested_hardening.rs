#![cfg(unix)]

use zellij_integration_tests::{
    keys, FakePtyHandle, GridSnapshot, NestedHarness, Size, PROMPT, TERMINAL_SIZE,
};
use zellij_utils::nested_session::NestedSessionMessage;

const ARROW_UP: &[u8] = b"\x1b[A";
const ARROW_LEFT: &[u8] = b"\x1b[D";

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

fn session_mode_bar_settled(grid_snapshot: &GridSnapshot) -> bool {
    last_line_contains(grid_snapshot, "SESSION") && last_line_contains(grid_snapshot, "Detach")
}

fn scroll_mode_bar_settled(grid_snapshot: &GridSnapshot) -> bool {
    last_line_contains(grid_snapshot, "Scroll")
        && last_line_contains(grid_snapshot, "Edit")
        && last_line_contains(grid_snapshot, "Select")
}

fn entering_search_bar_settled(grid_snapshot: &GridSnapshot) -> bool {
    last_line_contains(grid_snapshot, "ENTERING SEARCH TERM")
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

fn prompt_count(grid_snapshot: &GridSnapshot) -> usize {
    grid_snapshot.text.matches('$').count()
}

fn boot_and_descend_on_first_load(nested: &NestedHarness) {
    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.descend_into_guest_via_modal();
    nested.guest.wait_for_app_load();
    nested.wait_for_host_to_ping_guest();
    nested.wait_for_guest_to_reply_to_ping();
    nested
        .guest
        .wait_until("guest settled in normal mode", guest_ui_settled);
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

fn split_host_sibling(nested: &NestedHarness) -> FakePtyHandle {
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
    sibling
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

fn build_descended_state_with_host_sibling(nested: &NestedHarness) -> FakePtyHandle {
    boot_and_descend_on_first_load(nested);
    ascend_via_focus_host_binding(nested);
    let sibling = split_host_sibling(nested);
    descend_into_guest_on_the_left(nested);
    sibling
}

fn quit_guest_then_host(mut nested: NestedHarness) {
    let released = nested.mark_host_to_guest();
    nested.guest.quit();
    nested.wait_for_host_to_ascend_from_guest_after(released);
    nested.host.quit();
}

const SMALLER_SIZE: Size = Size {
    cols: 90,
    rows: 18,
};

const LARGER_SIZE: Size = Size {
    cols: 140,
    rows: 30,
};

#[test]
fn resizing_the_terminal_while_descended_keeps_the_guest_sized_to_its_pane() {
    let nested = NestedHarness::start(TERMINAL_SIZE);

    boot_and_descend_on_first_load(&nested);

    let guest_pane_before = nested.host_pane.size().expect("guest pane sized on load");
    let guest_rows_before = nested.guest.snapshot().row_count();

    nested.host.resize(SMALLER_SIZE);
    let guest_pane_smaller = nested.host_pane.wait_for_size(
        "the guest pane shrinks with the host terminal while descended",
        move |cols, rows| (cols, rows) != guest_pane_before,
    );
    assert!(
        guest_pane_smaller.0 < guest_pane_before.0
            || guest_pane_smaller.1 < guest_pane_before.1,
        "the guest pane must shrink when the host terminal shrinks \
         (before={guest_pane_before:?}, after={guest_pane_smaller:?})",
    );
    let guest_after_shrink = nested.guest.wait_until(
        "the guest re-renders at the smaller pane size while staying descended",
        |guest_grid| {
            guest_grid.row_count() == guest_pane_smaller.1 as usize
                && guest_grid.tab_bar_appears()
        },
    );
    assert!(
        guest_after_shrink.row_count() < guest_rows_before,
        "the descended guest must render fewer rows after the host shrinks",
    );

    nested.host.resize(LARGER_SIZE);
    let guest_pane_larger = nested.host_pane.wait_for_size(
        "the guest pane grows with the host terminal while descended",
        move |cols, rows| (cols, rows) != guest_pane_smaller,
    );
    nested.guest.wait_until(
        "the guest re-renders at the larger pane size while staying descended",
        |guest_grid| {
            guest_grid.row_count() == guest_pane_larger.1 as usize
                && guest_ui_settled(guest_grid)
        },
    );

    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    nested.guest.wait_until(
        "a key after the resize still reaches the descended guest",
        |guest_grid| guest_grid.contains("Pane #2") && guest_grid.contains("Pane #1"),
    );
    assert_eq!(
        nested.focus_lost_count(),
        0,
        "resizing while descended must not ascend the host out of the guest",
    );

    quit_guest_then_host(nested);
}

#[test]
fn resizing_the_terminal_while_nested_fullscreen_keeps_the_guest_filling_the_display() {
    let nested = NestedHarness::start(TERMINAL_SIZE);

    boot_and_descend_on_first_load(&nested);

    let entered = nested.mark_guest_to_host();
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.guest.wait_until(
        "guest entered session mode before the fullscreen bind",
        session_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::key('f'));
    nested.guest_to_host().wait_for_after(
        entered,
        "the descended guest asks its host to fullscreen its pane",
        |message| {
            matches!(
                message,
                NestedSessionMessage::ToggleHostFullscreen { fullscreen: true }
            )
        },
    );

    let fullscreen_pane_before = nested.host.wait_until(
        "the guest fills the whole host display before the resize",
        |host_grid| {
            host_grid.contains("[NESTED]")
                && host_grid.contains("Tab #1")
                && host_grid.row_count() == TERMINAL_SIZE.rows
        },
    );
    let host_rows_before = fullscreen_pane_before.row_count();

    nested.host.resize(LARGER_SIZE);
    let host_after_resize = nested.host.wait_until(
        "the fullscreened guest still fills the whole enlarged host display",
        |host_grid| {
            host_grid.row_count() == LARGER_SIZE.rows
                && host_grid.contains("[NESTED]")
                && host_grid.contains("Tab #1")
        },
    );
    assert!(
        host_after_resize.row_count() > host_rows_before,
        "the host display must render more rows after the terminal grows",
    );
    assert!(
        host_after_resize.row_of_line("Pane #1").is_none(),
        "no stray host pane-title line while the guest is fullscreened after resize",
    );
    nested.guest.wait_until(
        "the guest re-renders filling the enlarged display while fullscreened",
        |guest_grid| {
            guest_grid.row_count() == LARGER_SIZE.rows && guest_grid.contains("[NESTED]")
        },
    );

    let exited = nested.mark_guest_to_host();
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.guest.wait_until(
        "guest entered session mode before the toggle-off bind",
        session_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::key('f'));
    nested.guest_to_host().wait_for_after(
        exited,
        "the guest asks its host to exit fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::ToggleHostFullscreen { fullscreen: false }
            )
        },
    );
    nested.host.wait_until(
        "the host chrome returns at the enlarged size after exiting fullscreen",
        |host_grid| {
            host_grid.row_count() == LARGER_SIZE.rows
                && !host_grid.contains("[NESTED]")
                && host_grid.tab_bar_appears()
                && host_grid.status_bar_appears()
        },
    );

    quit_guest_then_host(nested);
}

#[test]
fn host_scrollback_on_a_guest_pane_scrolls_the_host_not_the_guest() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    boot_and_descend_on_first_load(&nested);
    ascend_via_focus_host_binding(&nested);

    nested.host.send_stdin(&keys::ctrl('s'));
    nested.host.wait_until(
        "the host enters scroll mode over the guest pane",
        scroll_mode_bar_settled,
    );

    let focus_gained_before_scroll = nested.focus_gained_count();
    nested.host.send_stdin(&keys::key('k'));
    nested.host.send_stdin(&keys::key('k'));
    nested.host.wait_until(
        "the host stays in scroll mode after scrolling the guest pane",
        scroll_mode_bar_settled,
    );
    assert_eq!(
        nested.focus_gained_count(),
        focus_gained_before_scroll,
        "scrolling the host over a guest pane must not descend into the guest",
    );

    nested.host.send_stdin(&keys::ESC);
    let host_back_to_normal = nested.host.wait_until(
        "the host returns to normal mode after scrolling the guest pane",
        |host_grid| normal_mode_bar_settled(host_grid),
    );
    assert!(
        guest_ui_settled(&nested.guest.snapshot()),
        "the guest UI must be undisturbed by the host scrolling its pane",
    );
    assert!(normal_mode_bar_settled(&host_back_to_normal));

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn host_search_over_a_guest_pane_stays_host_side() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    boot_and_descend_on_first_load(&nested);
    ascend_via_focus_host_binding(&nested);

    nested.host.send_stdin(&keys::ctrl('s'));
    nested.host.wait_until(
        "the host enters scroll mode over the guest pane",
        scroll_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::key('s'));
    nested.host.wait_until(
        "the host enters search-input over the guest pane",
        entering_search_bar_settled,
    );

    let focus_gained_before_search = nested.focus_gained_count();
    nested.host.send_stdin(b"Tab");
    nested.host.send_stdin(&keys::ESC);
    nested.host.wait_until(
        "the host settles back in scroll mode after the search input",
        scroll_mode_bar_settled,
    );
    assert_eq!(
        nested.focus_gained_count(),
        focus_gained_before_search,
        "searching the host over a guest pane must not descend into the guest",
    );

    nested.host.send_stdin(&keys::ESC);
    nested.host.wait_until(
        "the host returns to normal mode after searching the guest pane",
        |host_grid| normal_mode_bar_settled(host_grid),
    );
    assert!(
        guest_ui_settled(&nested.guest.snapshot()),
        "the guest UI must be undisturbed by the host searching its pane",
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn closing_a_host_sibling_pane_while_descended_keeps_the_guest_live() {
    let nested = NestedHarness::start(TERMINAL_SIZE);

    let sibling = build_descended_state_with_host_sibling(&nested);
    nested.host.wait_until(
        "the host shows its guest pane and its sibling before the sibling closes",
        |host_grid| host_grid.contains("Pane #2"),
    );
    nested.guest.wait_until(
        "the guest is settled while the host is descended into it",
        guest_ui_settled,
    );

    sibling.exit(Some(0));
    nested.host.wait_until(
        "the host closed its exited sibling, leaving only the guest pane",
        |host_grid| !host_grid.contains("Pane #2"),
    );

    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    nested.guest.wait_until(
        "the guest is still live and reacts to keys after the host sibling closed",
        |guest_grid| guest_grid.contains("Pane #2") && guest_grid.contains("│"),
    );

    quit_guest_then_host(nested);
}

#[test]
fn closing_an_inner_guest_pane_while_descended_keeps_the_guest_live() {
    let nested = NestedHarness::start(TERMINAL_SIZE);

    boot_and_descend_on_first_load(&nested);

    nested.host.send_stdin(&keys::alt('n'));
    let guest_second_pane = nested.guest.expect_pty_spawn();
    guest_second_pane.output(PROMPT);
    nested.guest.wait_until(
        "the guest opened a second inner pane from the passed-through key",
        |guest_grid| {
            guest_grid.contains("Pane #1")
                && guest_grid.contains("Pane #2")
                && guest_ui_settled(guest_grid)
        },
    );

    guest_second_pane.exit(Some(0));
    nested.guest.wait_until(
        "the guest closed its inner pane and returned to a single pane",
        |guest_grid| {
            !guest_grid.contains("Pane #2")
                && guest_grid.tab_bar_appears()
                && guest_grid.cursor.is_some()
        },
    );

    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    nested.guest.wait_until(
        "the guest is still descended-into and reacts to keys after closing an inner pane",
        |guest_grid| guest_grid.contains("Pane #2") && guest_grid.contains("│"),
    );

    quit_guest_then_host(nested);
}

#[test]
fn detaching_the_guest_client_while_descended_returns_control_to_the_host() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    boot_and_descend_on_first_load(&nested);

    let released = nested.mark_host_to_guest();
    nested.guest.detach_main_client();
    nested.wait_for_host_to_ascend_from_guest_after(released);

    let sibling = split_host_sibling(&nested);
    sibling.output(PROMPT);
    nested.host.wait_until(
        "the host acts on its own keybindings again after the guest detached",
        |host_grid| host_grid.contains("Pane #2") && normal_mode_bar_settled(host_grid),
    );

    nested.host.quit();
}

#[test]
fn opening_the_host_session_manager_with_a_guest_pane_does_not_misbehave() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    boot_and_descend_on_first_load(&nested);
    ascend_via_focus_host_binding(&nested);

    nested.host.send_stdin(&keys::ctrl('o'));
    nested.host.wait_until(
        "the host entered session mode before launching the session manager",
        session_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::key('w'));
    nested.host.wait_until(
        "the host session manager opened over its guest pane",
        |host_grid| host_grid.contains("Session Manager") && host_grid.status_bar_appears(),
    );

    nested.host.send_stdin(&keys::ctrl('c'));
    nested.host.wait_until(
        "the host session manager closed and normal chrome returned",
        |host_grid| !host_grid.contains("Session Manager") && normal_mode_bar_settled(host_grid),
    );

    nested.guest.wait_until(
        "the guest is undisturbed by the host session manager",
        guest_ui_settled,
    );
    nested.host.send_stdin(&keys::alt('n'));
    let host_new_pane = nested.host.expect_pty_spawn();
    host_new_pane.output(PROMPT);
    nested.host.wait_until(
        "the host still acts on its own keybindings after the session manager closed",
        |host_grid| host_grid.contains("Pane #2") && normal_mode_bar_settled(host_grid),
    );

    nested.guest.quit();
    nested.host.quit();
}

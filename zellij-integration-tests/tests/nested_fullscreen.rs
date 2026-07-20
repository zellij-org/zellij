#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized, split_right_and_wait_for_prompt,
    start_zellij, GridSnapshot, NestedDepthThreeHarness, NestedHarness, TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;
use zellij_utils::nested_session::NestedSessionMessage;

const ARROW_UP: &[u8] = b"\x1b[A";
const ARROW_LEFT: &[u8] = b"\x1b[D";

fn sgr_mouse_report(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}M", button, column, line).into_bytes()
}

fn last_line_contains(grid_snapshot: &GridSnapshot, needle: &str) -> bool {
    grid_snapshot
        .lines()
        .last()
        .map_or(false, |last_line| last_line.contains(needle))
}

fn session_mode_bar_settled(grid_snapshot: &GridSnapshot) -> bool {
    last_line_contains(grid_snapshot, "SESSION") && last_line_contains(grid_snapshot, "Detach")
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

fn guest_fullscreen_breadcrumb_line(guest_grid: &GridSnapshot) -> bool {
    guest_grid.lines().first().map_or(false, |first_line| {
        first_line.contains("[NESTED]") && first_line.contains('▸')
    })
}

fn breadcrumb_for(ancestry: &[&str], name: &str) -> String {
    format!("({} ▸ {} [NESTED]) ", ancestry.join(" ▸ "), name)
}

fn ancestor_segment_range(first_line: &str) -> (usize, usize) {
    let start = first_line.find('(').expect("breadcrumb open paren present");
    let after_arrow = first_line[start..]
        .rfind('▸')
        .expect("breadcrumb arrow present");
    let end = start + after_arrow + '▸'.len_utf8();
    (
        first_line[..start].chars().count(),
        first_line[..end].chars().count() + 1,
    )
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

fn enter_fullscreen_from_descended_guest(nested: &NestedHarness) {
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.guest.wait_until(
        "guest entered session mode before the fullscreen bind",
        session_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::key('f'));
}

fn host_own_name_present(host_grid: &GridSnapshot, host_session_name: &str) -> bool {
    let host_own_slot = format!("({})", host_session_name);
    host_grid.contains(&host_own_slot)
}

fn depth_three_descend(nested: &NestedDepthThreeHarness) {
    nested.boot_and_descend_depth_three();
    nested.inner.wait_for_app_load();
    nested
        .inner
        .wait_until("inner settled in normal mode", guest_ui_settled);
}

fn enter_fullscreen_from_descended_inner(nested: &NestedDepthThreeHarness) {
    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.inner.wait_until(
        "inner entered session mode before the fullscreen bind",
        session_mode_bar_settled,
    );
    nested.outer.send_stdin(&keys::key('f'));
}

#[test]
fn guest_fullscreen_fills_the_host_display_and_hides_chrome() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    let host_session_name = nested.host.session_name().to_string();
    let guest_session_name = nested.guest.session_name().to_string();

    boot_and_descend_on_first_load(&nested);

    let entered = nested.mark_guest_to_host();
    enter_fullscreen_from_descended_guest(&nested);
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

    let expected_breadcrumb = breadcrumb_for(&[&host_session_name], &guest_session_name);
    let fullscreen_grid = nested.host.wait_until(
        "guest content fills the whole host display without host chrome",
        |host_grid| {
            host_grid.contains(&expected_breadcrumb)
                && !host_own_name_present(host_grid, &host_session_name)
                && host_grid.lines().first().map_or(false, |first_line| {
                    first_line.contains(&expected_breadcrumb) && first_line.contains("Tab #1")
                })
                && host_grid.cursor_is_at(col(0).row(1))
        },
    );
    assert!(
        fullscreen_grid.row_of_line("Pane #1").is_none(),
        "no stray host pane-title line while the guest is fullscreened"
    );
    assert_snapshot!(
        "depth2_guest_fullscreen_fills_display",
        normalized(&fullscreen_grid)
    );
}

#[test]
fn toggle_off_restores_the_host_layout_exactly() {
    let nested = NestedHarness::start(TERMINAL_SIZE);

    boot_and_descend_on_first_load(&nested);

    let host_session_name = nested.host.session_name().to_string();
    let guest_session_name = nested.guest.session_name().to_string();
    let expected_breadcrumb = breadcrumb_for(&[&host_session_name], &guest_session_name);

    let before_fullscreen = nested.host.wait_until(
        "host chrome present before fullscreen",
        |host_grid| {
            host_grid.status_bar_appears()
                && host_grid.tab_bar_appears()
                && host_own_name_present(host_grid, &host_session_name)
                && !guest_fullscreen_breadcrumb_line(host_grid)
        },
    );

    let entered = nested.mark_guest_to_host();
    enter_fullscreen_from_descended_guest(&nested);
    nested.guest_to_host().wait_for_after(
        entered,
        "guest asks host to fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::ToggleHostFullscreen { fullscreen: true }
            )
        },
    );
    nested.host.wait_until("host chrome hidden while fullscreen", |host_grid| {
        host_grid.contains(&expected_breadcrumb)
            && !host_own_name_present(host_grid, &host_session_name)
    });

    let exited = nested.mark_guest_to_host();
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.guest.wait_until(
        "guest entered session mode before the toggle-off bind",
        session_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::key('f'));
    nested.guest_to_host().wait_for_after(
        exited,
        "guest asks host to exit fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::ToggleHostFullscreen { fullscreen: false }
            )
        },
    );

    let after_fullscreen = nested.host.wait_until(
        "host chrome and layout restored after toggle-off",
        |host_grid| {
            host_grid.status_bar_appears()
                && host_grid.tab_bar_appears()
                && host_own_name_present(host_grid, &host_session_name)
                && !guest_fullscreen_breadcrumb_line(host_grid)
        },
    );
    assert_eq!(
        after_fullscreen.lines().last(),
        before_fullscreen.lines().last(),
        "host status bar restored exactly after fullscreen exit"
    );
    assert!(
        after_fullscreen
            .lines()
            .first()
            .map_or(false, |first_line| !first_line.contains("[NESTED]")
                && !first_line.contains('▸')),
        "breadcrumb gone from the host tab-bar after fullscreen exit"
    );
}

#[test]
fn fullscreen_state_reaches_the_guest_on_enter_and_exit() {
    let nested = NestedHarness::start(TERMINAL_SIZE);

    boot_and_descend_on_first_load(&nested);

    let entered = nested.mark_host_to_guest();
    enter_fullscreen_from_descended_guest(&nested);
    nested.host_to_guest().wait_for_after(
        entered,
        "the guest is told it entered host fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FullscreenState { fullscreen: true }
            )
        },
    );
    nested
        .guest
        .wait_until("guest breadcrumb shown while fullscreen", |guest_grid| {
            guest_fullscreen_breadcrumb_line(guest_grid)
        });

    let exited = nested.mark_host_to_guest();
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.guest.wait_until(
        "guest entered session mode before the toggle-off bind",
        session_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::key('f'));
    nested.host_to_guest().wait_for_after(
        exited,
        "the guest is told it left host fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FullscreenState { fullscreen: false }
            )
        },
    );
    nested
        .guest
        .wait_until("guest breadcrumb cleared after exit", |guest_grid| {
            !guest_fullscreen_breadcrumb_line(guest_grid)
        });
}

fn column_of(first_line: &str, needle: &str) -> usize {
    let byte_index = first_line.find(needle).expect("needle present on line");
    first_line[..byte_index].chars().count()
}

#[test]
fn breadcrumb_is_present_only_while_fullscreened() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    let host_session_name = nested.host.session_name().to_string();
    let guest_session_name = nested.guest.session_name().to_string();

    boot_and_descend_on_first_load(&nested);

    let before = nested
        .guest
        .wait_until("guest tab-bar settled before fullscreen", guest_ui_settled);
    assert!(
        !before.contains("[NESTED]") && !guest_fullscreen_breadcrumb_line(&before),
        "no breadcrumb on the guest tab-bar before fullscreen"
    );

    enter_fullscreen_from_descended_guest(&nested);

    let expected_breadcrumb = breadcrumb_for(&[&host_session_name], &guest_session_name);
    let fullscreened = nested.guest.wait_until(
        "guest tab-bar shows the breadcrumb while fullscreened",
        |guest_grid| {
            guest_grid.lines().first().map_or(false, |first_line| {
                first_line.contains(&expected_breadcrumb)
            })
        },
    );
    let first_line = fullscreened.lines().into_iter().next().unwrap();
    assert!(
        first_line.contains(&format!("{} [NESTED])", guest_session_name)),
        "the guest's own name and the [NESTED] marker are present in the breadcrumb"
    );

    let ancestor_column = column_of(&first_line, &host_session_name);
    assert!(
        fullscreened.char_is_italic(ancestor_column, 0),
        "the ancestor segment of the breadcrumb is styled italic"
    );
    let name_column = column_of(
        &first_line,
        &format!("{} [NESTED]", guest_session_name),
    );
    assert!(
        fullscreened.char_is_bold(name_column, 0),
        "the guest's own name in the breadcrumb is styled bold"
    );
    assert!(
        !fullscreened.char_is_dim(ancestor_column, 0),
        "the ancestor segment is not dimmed"
    );
    assert_snapshot!(
        "depth2_breadcrumb_tab_bar_line",
        normalized(&fullscreened).lines().next().unwrap()
    );

    let exited = nested.mark_host_to_guest();
    nested.host.send_stdin(&keys::ctrl('o'));
    nested.guest.wait_until(
        "guest entered session mode before the toggle-off bind",
        session_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::key('f'));
    nested.host_to_guest().wait_for_after(
        exited,
        "guest told it left host fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FullscreenState { fullscreen: false }
            )
        },
    );
    nested.guest.wait_until(
        "guest breadcrumb gone after fullscreen exit",
        |guest_grid| {
            guest_grid.lines().first().map_or(false, |first_line| {
                !first_line.contains("[NESTED]") && !first_line.contains('▸')
            })
        },
    );
}

#[test]
fn nested_but_tiled_guest_shows_no_breadcrumb() {
    let nested = NestedHarness::start(TERMINAL_SIZE);

    boot_and_descend_on_first_load(&nested);

    let tiled = nested
        .guest
        .wait_until("nested guest tab-bar settled while tiled", guest_ui_settled);
    assert!(
        !tiled.contains("[NESTED]")
            && tiled.lines().first().map_or(true, |first_line| {
                !first_line.contains('▸')
            }),
        "a nested but not fullscreened guest shows no breadcrumb"
    );
}

#[test]
fn standalone_no_ui_fullscreen_shows_no_breadcrumb() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('F'));

    let fullscreened = zellij.wait_until(
        "standalone pane covers the display without chrome",
        |grid| {
            !grid.tab_bar_appears()
                && !grid.status_bar_appears()
                && grid.cursor_is_at(col(2).row(0))
        },
    );
    assert!(
        !fullscreened.contains("[NESTED]") && !fullscreened.contains("▸"),
        "a standalone (non-nested) NoUi fullscreen shows no breadcrumb"
    );
    zellij.quit();
}

#[test]
fn innermost_fullscreen_fills_the_whole_outer_terminal_and_zooms_the_chain() {
    let nested = NestedDepthThreeHarness::start_depth_three(TERMINAL_SIZE);
    let outer_session_name = nested.outer.session_name().to_string();
    let middle_session_name = nested.middle.session_name().to_string();
    let inner_session_name = nested.inner.session_name().to_string();

    depth_three_descend(&nested);

    let bubbled = nested.mark_inner_to_middle();
    let outer_told = nested.mark_outer_to_middle();
    let middle_told = nested.mark_middle_to_inner();
    enter_fullscreen_from_descended_inner(&nested);

    nested.inner_to_middle_frames().wait_for_after(
        bubbled,
        "inner asks the middle host to fullscreen it",
        |message| {
            matches!(
                message,
                NestedSessionMessage::ToggleHostFullscreen { fullscreen: true }
            )
        },
    );
    nested.outer_to_middle_frames().wait_for_after(
        outer_told,
        "outer tells the middle it is in host fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FullscreenState { fullscreen: true }
            )
        },
    );
    nested.middle_to_inner_frames().wait_for_after(
        middle_told,
        "middle tells the inner it is in host fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FullscreenState { fullscreen: true }
            )
        },
    );

    let expected_breadcrumb = breadcrumb_for(
        &[&outer_session_name, &middle_session_name],
        &inner_session_name,
    );
    let fullscreen_grid = nested.outer.wait_until(
        "inner content fills the whole outer display with all three ancestry segments",
        |outer_grid| {
            outer_grid.contains(&expected_breadcrumb)
                && !host_own_name_present(outer_grid, &outer_session_name)
                && !host_own_name_present(outer_grid, &middle_session_name)
                && outer_grid.lines().first().map_or(false, |first_line| {
                    first_line.contains(&expected_breadcrumb) && first_line.contains("Tab #1")
                })
                && outer_grid.cursor_is_at(col(0).row(1))
        },
    );
    assert_snapshot!(
        "depth3_inner_fullscreen_breadcrumb",
        normalized(&fullscreen_grid)
    );
}

fn ascend_inner_to_middle(nested: &NestedDepthThreeHarness) {
    let requested = nested.mark_inner_to_middle();
    let ascended = nested.mark_middle_to_inner();
    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.inner.wait_until(
        "inner entered session mode before ascending to the middle",
        session_mode_bar_settled,
    );
    nested.outer.send_stdin(ARROW_UP);
    nested.wait_for_inner_to_request_host_focus_after(requested);
    nested.wait_for_middle_to_ascend_from_inner_after(ascended);
}

fn redescend_middle_into_inner(nested: &NestedDepthThreeHarness) {
    let descended = nested.mark_middle_to_inner();
    nested.outer.send_stdin(&keys::ctrl('p'));
    nested.middle.wait_until(
        "middle entered pane mode before re-descending into the inner",
        |middle_grid| {
            last_line_contains(middle_grid, "PANE") && last_line_contains(middle_grid, "Move")
        },
    );
    nested.outer.send_stdin(ARROW_LEFT);
    nested.wait_for_middle_to_descend_into_inner_after(descended);
}

#[test]
fn fullscreen_from_inner_while_middle_already_fullscreen_does_not_cancel() {
    let nested = NestedDepthThreeHarness::start_depth_three(TERMINAL_SIZE);
    let outer_session_name = nested.outer.session_name().to_string();
    let middle_session_name = nested.middle.session_name().to_string();
    let inner_session_name = nested.inner.session_name().to_string();

    depth_three_descend(&nested);

    ascend_inner_to_middle(&nested);

    nested.outer.send_stdin(&keys::ctrl('p'));
    nested.middle.wait_until(
        "middle entered pane mode before splitting a sibling",
        |middle_grid| {
            last_line_contains(middle_grid, "PANE") && last_line_contains(middle_grid, "Move")
        },
    );
    nested.outer.send_stdin(&keys::key('r'));
    let middle_sibling = nested.middle.expect_pty_spawn();
    middle_sibling.output(b"MIDDLE-SIBLING ");
    nested.middle.wait_until(
        "middle spawned a sibling pane next to the inner guest",
        |middle_grid| middle_grid.contains("MIDDLE-SIBLING"),
    );

    let outer_told_true = nested.mark_outer_to_middle();
    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.middle.wait_until(
        "middle entered session mode before fullscreening itself",
        session_mode_bar_settled,
    );
    nested.outer.send_stdin(&keys::key('f'));
    nested.outer_to_middle_frames().wait_for_after(
        outer_told_true,
        "outer fullscreens the middle pane and tells the middle it is fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FullscreenState { fullscreen: true }
            )
        },
    );

    let middle_breadcrumb = breadcrumb_for(&[&outer_session_name], &middle_session_name);
    nested.outer.wait_until(
        "outer display fullscreened by the middle before the inner enters fullscreen",
        |outer_grid| {
            outer_grid.contains(&middle_breadcrumb)
                && !host_own_name_present(outer_grid, &outer_session_name)
        },
    );

    redescend_middle_into_inner(&nested);

    let cancels_before = nested.outer_to_middle_frames().count(|message| {
        matches!(
            message,
            NestedSessionMessage::FullscreenState { fullscreen: false }
        )
    });
    let bubbled = nested.mark_inner_to_middle();
    enter_fullscreen_from_descended_inner(&nested);
    nested.inner_to_middle_frames().wait_for_after(
        bubbled,
        "inner asks the middle to fullscreen it",
        |message| {
            matches!(
                message,
                NestedSessionMessage::ToggleHostFullscreen { fullscreen: true }
            )
        },
    );

    let inner_breadcrumb = breadcrumb_for(
        &[&outer_session_name, &middle_session_name],
        &inner_session_name,
    );
    let final_grid = nested.outer.wait_until(
        "the whole chain ends fullscreen with the inner content covering the outer display",
        |outer_grid| {
            outer_grid.contains(&inner_breadcrumb)
                && !host_own_name_present(outer_grid, &outer_session_name)
                && !host_own_name_present(outer_grid, &middle_session_name)
                && outer_grid.cursor_is_at(col(0).row(1))
        },
    );

    let cancels_after = nested.outer_to_middle_frames().count(|message| {
        matches!(
            message,
            NestedSessionMessage::FullscreenState { fullscreen: false }
        )
    });
    assert_eq!(
        cancels_before, cancels_after,
        "entering fullscreen at the inner while the middle is already fullscreen must not emit a spurious exit to the outer"
    );
    assert_eq!(
        nested.inner_to_middle_frames().count(|message| matches!(
            message,
            NestedSessionMessage::ToggleHostFullscreen { fullscreen: false }
        )),
        0,
        "no ToggleHostFullscreen{{false}} is emitted while entering fullscreen"
    );
    assert!(
        final_grid.contains(&inner_breadcrumb),
        "the outer display is fullscreen by the inner content at the end"
    );
}

const FLOATING_MARKER: &str = "HOST-FLOAT";

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
    sibling.output(b"HOST-SIBLING ");
    nested.host.wait_until(
        "host returned to normal mode after the split key",
        |host_grid| host_grid.contains("HOST-SIBLING"),
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

#[test]
fn host_floating_layer_is_hidden_during_nested_fullscreen_and_reappears_on_exit() {
    let nested = NestedHarness::start(TERMINAL_SIZE);

    boot_and_descend_on_first_load(&nested);
    ascend_via_focus_host_binding(&nested);
    split_host_sibling(&nested);

    let observer = nested.host.attach_client(TERMINAL_SIZE);
    observer.wait_until(
        "observer host client attached focused on the sibling host pane",
        |host_grid| host_grid.contains("HOST-SIBLING") && last_line_contains(host_grid, "LOCK"),
    );

    observer.send_stdin(&keys::ctrl('p'));
    observer.wait_until(
        "observer entered pane mode before the floating pane",
        |host_grid| host_grid.contains("PANE") && host_grid.contains("Move"),
    );
    observer.send_stdin(&keys::key('w'));
    let floating = nested.host.expect_pty_spawn();
    floating.output(FLOATING_MARKER.as_bytes());
    observer.wait_until(
        "the host floating pane spawned and is visible on the observer",
        |host_grid| host_grid.contains(FLOATING_MARKER),
    );

    observer.send_stdin(&keys::alt('f'));
    observer.wait_until("observer hid the host floating pane", |host_grid| {
        !host_grid.contains(FLOATING_MARKER)
    });

    descend_into_guest_on_the_left(&nested);
    nested.guest.wait_until("guest settled after descend", guest_ui_settled);

    let entered = nested.mark_guest_to_host();
    enter_fullscreen_from_descended_guest(&nested);
    nested.guest_to_host().wait_for_after(
        entered,
        "guest asks host to fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::ToggleHostFullscreen { fullscreen: true }
            )
        },
    );
    let host_session_name = nested.host.session_name().to_string();
    let guest_session_name = nested.guest.session_name().to_string();
    let expected_breadcrumb = breadcrumb_for(&[&host_session_name], &guest_session_name);
    nested.host.wait_until(
        "guest fullscreened over the host display",
        |host_grid| {
            host_grid.contains(&expected_breadcrumb)
                && !host_own_name_present(host_grid, &host_session_name)
        },
    );

    observer.send_stdin(&keys::alt('f'));
    observer.wait_until(
        "the host floating layer stays hidden while the guest covers the display, even though show_panes was set",
        |host_grid| !host_grid.contains(FLOATING_MARKER) && !host_grid.contains("HOST-SIBLING"),
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
        "guest asks host to exit fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::ToggleHostFullscreen { fullscreen: false }
            )
        },
    );

    observer.wait_until(
        "the host floating pane renders again on the observer after fullscreen exit",
        |host_grid| host_grid.contains(FLOATING_MARKER) && host_grid.status_bar_appears(),
    );
}

#[test]
fn restoring_the_innermost_fullscreen_unzooms_the_whole_chain() {
    let nested = NestedDepthThreeHarness::start_depth_three(TERMINAL_SIZE);
    let outer_session_name = nested.outer.session_name().to_string();
    let middle_session_name = nested.middle.session_name().to_string();
    let inner_session_name = nested.inner.session_name().to_string();

    depth_three_descend(&nested);

    enter_fullscreen_from_descended_inner(&nested);
    let expected_breadcrumb = breadcrumb_for(
        &[&outer_session_name, &middle_session_name],
        &inner_session_name,
    );
    nested.outer.wait_until(
        "outer display fullscreened by the inner before restore",
        |outer_grid| {
            outer_grid.contains(&expected_breadcrumb)
                && !host_own_name_present(outer_grid, &outer_session_name)
        },
    );

    let bubbled = nested.mark_inner_to_middle();
    let outer_told = nested.mark_outer_to_middle();
    let middle_told = nested.mark_middle_to_inner();
    nested.outer.send_stdin(&keys::ctrl('o'));
    nested.inner.wait_until(
        "inner entered session mode before the toggle-off bind",
        session_mode_bar_settled,
    );
    nested.outer.send_stdin(&keys::key('f'));

    nested.inner_to_middle_frames().wait_for_after(
        bubbled,
        "inner asks the middle host to exit fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::ToggleHostFullscreen { fullscreen: false }
            )
        },
    );
    nested.outer_to_middle_frames().wait_for_after(
        outer_told,
        "outer tells the middle it left host fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FullscreenState { fullscreen: false }
            )
        },
    );
    nested.middle_to_inner_frames().wait_for_after(
        middle_told,
        "middle tells the inner it left host fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FullscreenState { fullscreen: false }
            )
        },
    );

    nested.outer.wait_until(
        "outer chrome restored and breadcrumb gone across the whole chain",
        |outer_grid| {
            outer_grid.status_bar_appears()
                && host_own_name_present(outer_grid, &outer_session_name)
                && outer_grid.lines().first().map_or(false, |first_line| {
                    !first_line.contains("[NESTED]") && !first_line.contains('▸')
                })
        },
    );
    nested.middle.wait_until(
        "middle chrome restored after chain un-zoom",
        |middle_grid| {
            middle_grid.status_bar_appears()
                && middle_grid.lines().first().map_or(false, |first_line| {
                    !first_line.contains("[NESTED]") && !first_line.contains('▸')
                })
        },
    );
}

#[test]
fn clicking_the_ancestor_breadcrumb_segment_ascends_out_of_the_guest() {
    let nested = NestedHarness::start_with_host_and_guest_config(
        TERMINAL_SIZE,
        "mouse_mode true",
        "mouse_mode true",
    );
    let host_session_name = nested.host.session_name().to_string();
    let guest_session_name = nested.guest.session_name().to_string();

    boot_and_descend_on_first_load(&nested);

    enter_fullscreen_from_descended_guest(&nested);
    let expected_breadcrumb = breadcrumb_for(&[&host_session_name], &guest_session_name);
    let fullscreen_grid = nested.host.wait_until(
        "guest fullscreened with the breadcrumb visible before clicking",
        |host_grid| {
            host_grid.lines().first().map_or(false, |first_line| {
                first_line.contains(&expected_breadcrumb)
            })
        },
    );
    let first_line = fullscreen_grid.lines().into_iter().next().unwrap();
    let (ancestor_start, ancestor_end) = ancestor_segment_range(&first_line);

    let non_ascend_before = nested
        .guest_to_host()
        .count(|message| matches!(message, NestedSessionMessage::FocusHost { direction: None }));
    let name_column = column_of(&first_line, &format!("{} [NESTED]", guest_session_name)) + 1;
    nested.host.send_stdin(&sgr_mouse_report(name_column, 1, 0));
    nested
        .host
        .send_stdin(&format!("\u{1b}[<0;{};1m", name_column).into_bytes());
    nested
        .guest
        .wait_until("guest still fullscreened after a non-ancestor click", |guest_grid| {
            guest_ui_settled(guest_grid)
        });
    assert_eq!(
        nested
            .guest_to_host()
            .count(|message| matches!(message, NestedSessionMessage::FocusHost { direction: None })),
        non_ascend_before,
        "clicking the own-name/[NESTED] part of the breadcrumb must not ascend"
    );

    let requested = nested.mark_guest_to_host();
    let ascended = nested.mark_host_to_guest();
    let ancestor_click_column = (ancestor_start + ancestor_end) / 2 + 1;
    nested.host.send_stdin(&sgr_mouse_report(ancestor_click_column, 1, 0));
    nested
        .host
        .send_stdin(&format!("\u{1b}[<0;{};1m", ancestor_click_column).into_bytes());
    nested.guest_to_host().wait_for_after(
        requested,
        "clicking the ancestor breadcrumb segment asks the host to take focus back",
        |message| matches!(message, NestedSessionMessage::FocusHost { direction: None }),
    );
    nested.wait_for_host_to_ascend_from_guest_after(ascended);

    nested.host.wait_until(
        "host regained control and restored its chrome after the breadcrumb click",
        |host_grid| {
            host_grid.status_bar_appears()
                && host_own_name_present(host_grid, &host_session_name)
        },
    );

    nested.host.send_stdin(&keys::alt('n'));
    let host_new_pane = nested.host.expect_pty_spawn();
    host_new_pane.output(b"$ ");
    nested.host.wait_until(
        "host acts on its own key after the breadcrumb click ascended it",
        |host_grid| host_grid.contains("Pane #2"),
    );
}

#[test]
fn host_manually_breaking_fullscreen_drifts_the_guest_to_not_fullscreen() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    let host_session_name = nested.host.session_name().to_string();
    let guest_session_name = nested.guest.session_name().to_string();

    boot_and_descend_on_first_load(&nested);

    let entered = nested.mark_host_to_guest();
    enter_fullscreen_from_descended_guest(&nested);
    nested.host_to_guest().wait_for_after(
        entered,
        "the guest is told it entered host fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FullscreenState { fullscreen: true }
            )
        },
    );
    let expected_breadcrumb = breadcrumb_for(&[&host_session_name], &guest_session_name);
    nested.host.wait_until("guest fullscreened over the host display", |host_grid| {
        host_grid.contains(&expected_breadcrumb)
            && !host_own_name_present(host_grid, &host_session_name)
    });

    let drifted = nested.mark_host_to_guest();
    let exit_code = nested
        .host
        .run_cli_action(CliAction::ToggleNoUiFullscreen { pane_id: None });
    assert_eq!(exit_code, 0, "host toggled its own no-ui fullscreen off cleanly");

    nested.host_to_guest().wait_for_after(
        drifted,
        "the host's drift detection tells the guest it is no longer fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FullscreenState { fullscreen: false }
            )
        },
    );
    nested.guest.wait_until(
        "guest breadcrumb clears after the host manually broke the fullscreen",
        |guest_grid| {
            guest_grid.lines().first().map_or(false, |first_line| {
                !first_line.contains("[NESTED]") && !first_line.contains('▸')
            })
        },
    );
    nested.host.wait_until(
        "host chrome restored after manually breaking the fullscreen",
        |host_grid| {
            host_grid.status_bar_appears()
                && host_own_name_present(host_grid, &host_session_name)
        },
    );
}

#[test]
fn guest_killed_while_fullscreened_unzooms_the_host_after_liveness_timeout() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    let host_session_name = nested.host.session_name().to_string();
    let guest_session_name = nested.guest.session_name().to_string();

    boot_and_descend_on_first_load(&nested);

    let entered = nested.mark_host_to_guest();
    enter_fullscreen_from_descended_guest(&nested);
    nested.host_to_guest().wait_for_after(
        entered,
        "the guest is told it entered host fullscreen",
        |message| {
            matches!(
                message,
                NestedSessionMessage::FullscreenState { fullscreen: true }
            )
        },
    );
    let expected_breadcrumb = breadcrumb_for(&[&host_session_name], &guest_session_name);
    nested.host.wait_until(
        "guest fullscreen composite fully settled before freezing the guest",
        |host_grid| {
            host_grid.contains(&expected_breadcrumb)
                && !host_own_name_present(host_grid, &host_session_name)
                && host_grid.cursor_is_at(col(0).row(1))
        },
    );

    nested.freeze_guest();

    nested.host.wait_until(
        "host un-zooms and restores its chrome after the frozen guest times out",
        |host_grid| {
            host_grid.status_bar_appears()
                && host_own_name_present(host_grid, &host_session_name)
                && host_grid.lines().first().map_or(false, |first_line| {
                    !first_line.contains("[NESTED]") && !first_line.contains('▸')
                })
        },
    );
}

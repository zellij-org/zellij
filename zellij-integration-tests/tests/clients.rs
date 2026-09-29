#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    assert_same_rendered_grid, claim_first_terminal_and_wait_for_prompt, col, keys, normalized,
    start_zellij, FakePtyHandle, GridSnapshot, Size, TestClient, TestRunner, TestSession, PROMPT,
    TERMINAL_SIZE,
};

const LARGER_CLIENT_SIZE: Size = Size {
    cols: 160,
    rows: 40,
};
const SMALLER_CLIENT_SIZE: Size = Size {
    cols: 100,
    rows: 20,
};

fn session_name_rendered(grid_snapshot: &GridSnapshot) -> bool {
    grid_snapshot.contains("(test-")
}

fn pane_size_in_tab_sized(tab_size: Size) -> (u16, u16) {
    (tab_size.cols as u16, tab_size.rows as u16 - 2)
}

fn split_pane_rows_in_tab_sized(tab_size: Size) -> u16 {
    tab_size.rows as u16 - 3
}

fn rendered_tab_height(grid_snapshot: &GridSnapshot) -> Option<usize> {
    grid_snapshot
        .row_of_line("Ctrl +")
        .map(|status_bar_row| status_bar_row + 1)
}

fn settled_in_tab_sized(grid_snapshot: &GridSnapshot, tab_size: Size, marker: &str) -> bool {
    grid_snapshot.contains(marker) && rendered_tab_height(grid_snapshot) == Some(tab_size.rows)
}

fn settled_in_normal_mode(grid_snapshot: &GridSnapshot, tab_size: Size, marker: &str) -> bool {
    grid_snapshot.status_bar_appears() && settled_in_tab_sized(grid_snapshot, tab_size, marker)
}

fn tab_region(grid_snapshot: &GridSnapshot) -> GridSnapshot {
    let tab_height =
        rendered_tab_height(grid_snapshot).unwrap_or_else(|| grid_snapshot.row_count());
    GridSnapshot {
        text: grid_snapshot
            .lines()
            .into_iter()
            .take(tab_height)
            .collect::<Vec<_>>()
            .join("\n"),
        cursor: grid_snapshot.cursor,
        styles: grid_snapshot
            .styles
            .iter()
            .take(tab_height)
            .cloned()
            .collect(),
    }
}

fn rendered_content(grid_snapshot: &GridSnapshot) -> String {
    normalized(grid_snapshot).trim_end().to_string()
}

fn rendered_tab_content(grid_snapshot: &GridSnapshot) -> String {
    rendered_content(&tab_region(grid_snapshot))
}

fn mark_pane(client: &TestClient, terminal: &FakePtyHandle, marker: &str) {
    terminal.output(marker.as_bytes());
    let expected_marker = marker.to_owned();
    client.wait_until("pane marked", move |grid_snapshot| {
        grid_snapshot.contains(&expected_marker)
    });
}

fn open_marked_tab(zellij: &TestSession, client: &TestClient, marker: &str) -> FakePtyHandle {
    client.send_stdin(&keys::ctrl('t'));
    client.send_stdin(&keys::key('n'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    terminal.output(marker.as_bytes());
    let expected_marker = marker.to_owned();
    client.wait_until("new marked tab opened", move |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains(&expected_marker)
    });
    terminal
}

fn split_right_marked(zellij: &TestSession, client: &TestClient, marker: &str) -> FakePtyHandle {
    client.send_stdin(&keys::ctrl('p'));
    client.send_stdin(&keys::key('r'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    terminal.output(marker.as_bytes());
    let expected_marker = marker.to_owned();
    client.wait_until("split pane rendered its marker", move |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains(&expected_marker)
    });
    terminal
}

fn close_focused_tab(client: &TestClient) {
    client.send_stdin(&keys::ctrl('t'));
    client.send_stdin(&keys::key('x'));
}

fn go_to_first_tab(client: &TestClient) {
    client.send_stdin(&keys::ctrl('t'));
    client.send_stdin(&keys::key('1'));
}

#[test]
fn mirrored_sessions() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mirror_session true")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let second_client = zellij.attach_client(TERMINAL_SIZE);
    second_client.wait_until("second client loaded on the first tab", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(col(2).row(1))
    });

    second_client.send_stdin(&keys::ctrl('p'));
    second_client.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);

    let mirror_focused_right_pane = |grid_snapshot: &GridSnapshot| {
        grid_snapshot.tab_bar_appears()
            && session_name_rendered(grid_snapshot)
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Pane #2")
            && grid_snapshot.cursor_is_at(col(62).row(2))
    };
    let main_grid = zellij.wait_until(
        "main client follows the second client's focus into the split it never asked for",
        mirror_focused_right_pane,
    );
    let second_grid = second_client.wait_until(
        "second client shows the split it created",
        mirror_focused_right_pane,
    );
    assert_eq!(
        normalized(&main_grid),
        normalized(&second_grid),
        "mirrored clients must render an identical view"
    );
    assert_snapshot!(normalized(&main_grid));
    second_client.quit();
    zellij.quit();
}

#[test]
fn multiple_users_in_same_pane_and_tab() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("pane_frame_style \"full\"")
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("$ ")
    });

    let second_client = zellij.attach_client(TERMINAL_SIZE);
    let second_grid =
        second_client.wait_until("second client shares the focused pane", |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("MY FOCUS")
                && grid_snapshot.cursor_is_at(col(3).row(2))
        });
    let main_grid = zellij.wait_until(
        "main client shows the shared-focus indicator",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("MY FOCUS")
                && grid_snapshot.cursor_is_at(col(3).row(2))
        },
    );
    assert_snapshot!(normalized(&main_grid));
    assert_snapshot!(normalized(&second_grid));
    second_client.quit();
    zellij.quit();
}

#[test]
fn multiple_users_in_different_panes_and_same_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let second_client = zellij.attach_client(TERMINAL_SIZE);
    second_client.wait_until("second client loaded on the first tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
    });

    second_client.send_stdin(&keys::ctrl('p'));
    second_client.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);

    let second_grid = second_client.wait_until(
        "second client focused the new right pane",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(62).row(2))
        },
    );
    let main_grid = zellij.wait_until(
        "main client sees the second client's split while staying on the left pane",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.contains("Pane #2")
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(2).row(2))
        },
    );
    assert_snapshot!(normalized(&main_grid));
    assert_snapshot!(normalized(&second_grid));
    second_client.quit();
    zellij.quit();
}

#[test]
fn multiple_users_in_different_tabs() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let second_client = zellij.attach_client(TERMINAL_SIZE);
    second_client.wait_until("second client loaded on the first tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
    });

    second_client.send_stdin(&keys::ctrl('t'));
    second_client.send_stdin(&keys::key('n'));
    let second_tab_terminal = zellij.expect_pty_spawn();
    second_tab_terminal.output(PROMPT);

    let second_grid =
        second_client.wait_until("second client moved to the new tab", |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("Tab #2")
                && grid_snapshot.cursor_is_at(col(2).row(1))
        });
    let main_grid = zellij.wait_until(
        "main client sees the new tab while staying on the first tab",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("Tab #2")
                && grid_snapshot.cursor_is_at(col(2).row(1))
        },
    );
    assert_snapshot!(normalized(&main_grid));
    assert_snapshot!(normalized(&second_grid));
    second_client.quit();
    zellij.quit();
}

#[test]
fn detach_and_attach_session() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);
    zellij.wait_until("right terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(2))
    });

    right_terminal.output(b"I am some text");
    zellij.wait_until("text rendered in the right terminal", |grid_snapshot| {
        grid_snapshot.contains("I am some text")
    });

    zellij.detach_main_client();

    let reattached_client = zellij.attach_client(TERMINAL_SIZE);
    let grid_snapshot = reattached_client.wait_until(
        "reattached client sees the restored split and text",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("Pane #2")
                && grid_snapshot.contains("I am some text")
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    reattached_client.quit();
    zellij.quit();
}

#[test]
fn two_clients_of_different_sizes_on_different_tabs_render_independently() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    mark_pane(zellij.main_client(), &first_terminal, "oneone");

    let second_client = zellij.attach_client(LARGER_CLIENT_SIZE);
    second_client.wait_until(
        "larger client is held to the size of the smaller client sharing its tab",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "oneone"),
    );

    let second_tab_terminal = open_marked_tab(&zellij, &second_client, "twotwo");

    second_tab_terminal.wait_for_size(
        "the new tab is laid out at the size of the client that created it",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE),
    );
    first_terminal.wait_for_size(
        "the vacated tab keeps the size of its only remaining viewer",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(TERMINAL_SIZE),
    );

    let second_grid = second_client.wait_until(
        "second client renders its own tab at its own dimensions",
        |grid_snapshot| {
            settled_in_normal_mode(grid_snapshot, LARGER_CLIENT_SIZE, "twotwo")
                && grid_snapshot.contains("Tab #1 [ ]")
                && !grid_snapshot.contains("oneone")
        },
    );
    let main_grid = zellij.wait_until(
        "main client keeps rendering its tab at its own dimensions",
        |grid_snapshot| {
            settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "oneone")
                && grid_snapshot.contains("Tab #2 [ ]")
                && !grid_snapshot.contains("twotwo")
        },
    );

    assert_snapshot!(normalized(&tab_region(&main_grid)));
    assert_snapshot!(normalized(&tab_region(&second_grid)));
    second_client.quit();
    zellij.quit();
}

#[test]
fn resizing_one_client_does_not_disturb_the_other_clients_tab() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    mark_pane(zellij.main_client(), &first_terminal, "oneone");

    let second_client = zellij.attach_client(LARGER_CLIENT_SIZE);
    second_client.wait_until(
        "second client attached to the shared tab",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "oneone"),
    );
    let second_tab_terminal = open_marked_tab(&zellij, &second_client, "twotwo");
    second_tab_terminal.wait_for_size("second tab laid out for its creator", |cols, rows| {
        (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE)
    });
    second_client.wait_until(
        "second client settled on its own tab in normal mode before resizing",
        |grid_snapshot| settled_in_normal_mode(grid_snapshot, LARGER_CLIENT_SIZE, "twotwo"),
    );

    let main_grid_before_resize = zellij.wait_until(
        "main client settled on its own tab before the peer resizes",
        |grid_snapshot| {
            settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "oneone")
                && grid_snapshot.contains("Tab #2 [ ]")
        },
    );
    let main_pane_resizes_before = first_terminal.size_history().len();

    let resized_client_size = Size { cols: 90, rows: 18 };
    second_client.resize(resized_client_size);

    second_tab_terminal.wait_for_size(
        "the resized client's own tab follows its new window size",
        move |cols, rows| (cols, rows) == pane_size_in_tab_sized(resized_client_size),
    );
    let second_grid = second_client.wait_until(
        "resized client re-rendered its tab at its new dimensions",
        move |grid_snapshot| {
            settled_in_tab_sized(grid_snapshot, resized_client_size, "twotwo")
                && grid_snapshot.contains("Tab #1 [ ]")
        },
    );

    std::thread::sleep(std::time::Duration::from_millis(300));

    assert_eq!(
        first_terminal.size_history().len(),
        main_pane_resizes_before,
        "a resize of a client on another tab must not resize this tab's panes"
    );
    assert_eq!(
        first_terminal.size(),
        Some(pane_size_in_tab_sized(TERMINAL_SIZE)),
        "the untouched tab must still be laid out for its own viewer"
    );
    assert_same_rendered_grid(
        &zellij.snapshot(),
        &main_grid_before_resize,
        "the other client's view must be untouched by a resize happening on another tab",
    );

    assert_snapshot!(normalized(&tab_region(&main_grid_before_resize)));
    assert_snapshot!(normalized(&tab_region(&second_grid)));
    second_client.quit();
    zellij.quit();
}

#[test]
fn clients_converge_when_focusing_the_same_tab() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    mark_pane(zellij.main_client(), &first_terminal, "oneone");

    let second_client = zellij.attach_client(LARGER_CLIENT_SIZE);
    second_client.wait_until(
        "second client attached to the shared tab",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "oneone"),
    );
    let second_tab_terminal = open_marked_tab(&zellij, &second_client, "twotwo");
    second_tab_terminal.wait_for_size("second tab laid out for its lone viewer", |cols, rows| {
        (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE)
    });

    go_to_first_tab(&second_client);

    second_client.wait_until(
        "larger client shrinks back into the tab it now shares",
        |grid_snapshot| {
            settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "oneone")
                && !grid_snapshot.contains("twotwo")
        },
    );
    first_terminal.wait_for_size(
        "shared tab is sized to its smallest viewer",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(TERMINAL_SIZE),
    );
    let main_grid = zellij.wait_until(
        "main client is unaffected by the peer joining its tab",
        |grid_snapshot| {
            settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "oneone")
                && grid_snapshot.contains("Tab #1 [ ]")
        },
    );

    let expected_content = rendered_tab_content(&main_grid);
    let second_grid = second_client.wait_until(
        "clients sharing a tab converge on the same rendering",
        move |grid_snapshot| rendered_tab_content(grid_snapshot) == expected_content,
    );
    assert_eq!(
        rendered_tab_height(&main_grid),
        rendered_tab_height(&second_grid),
        "clients sharing a tab must render it at the same height"
    );
    assert_eq!(
        rendered_tab_content(&main_grid),
        rendered_tab_content(&second_grid),
        "clients sharing a tab must agree on what that tab looks like"
    );

    assert_snapshot!(normalized(&tab_region(&main_grid)));
    assert_snapshot!(normalized(&tab_region(&second_grid)));
    second_client.quit();
    zellij.quit();
}

#[test]
fn focusing_a_smaller_tab_leaves_nothing_behind_outside_it() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    mark_pane(zellij.main_client(), &first_terminal, "oneone");

    let second_client = zellij.attach_client(LARGER_CLIENT_SIZE);
    second_client.wait_until(
        "second client attached to the shared tab",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "oneone"),
    );
    let second_tab_terminal = open_marked_tab(&zellij, &second_client, "twotwo");
    second_tab_terminal.wait_for_size("second tab laid out for its lone viewer", |cols, rows| {
        (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE)
    });

    go_to_first_tab(&second_client);

    let main_grid = zellij.wait_until("main client settled on the shared tab", |grid_snapshot| {
        settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "oneone")
            && grid_snapshot.contains("Tab #1 [ ]")
    });
    let second_grid =
        second_client.wait_until("larger client settled on the shared tab", |grid_snapshot| {
            settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "oneone")
                && grid_snapshot.contains("Tab #1 [ ]")
                && !grid_snapshot.contains("twotwo")
        });

    assert_eq!(
        rendered_content(&second_grid),
        rendered_content(&main_grid),
        "the display of a client whose window is larger than the tab it focuses must hold nothing but that tab"
    );
    second_client.quit();
    zellij.quit();
}

#[test]
fn closing_a_larger_tab_leaves_nothing_behind_outside_the_one_returned_to() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    mark_pane(zellij.main_client(), &first_terminal, "oneone");

    let second_client = zellij.attach_client(LARGER_CLIENT_SIZE);
    second_client.wait_until(
        "second client attached to the shared tab",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "oneone"),
    );
    let second_tab_terminal = open_marked_tab(&zellij, &second_client, "twotwo");
    second_tab_terminal.wait_for_size("second tab laid out for its lone viewer", |cols, rows| {
        (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE)
    });

    close_focused_tab(&second_client);

    let main_grid = zellij.wait_until("main client settled on the shared tab", |grid_snapshot| {
        settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "oneone")
            && grid_snapshot.contains("Tab #1 [ ]")
    });
    let second_grid =
        second_client.wait_until("larger client settled on the shared tab", |grid_snapshot| {
            settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "oneone")
                && grid_snapshot.contains("Tab #1 [ ]")
                && !grid_snapshot.contains("twotwo")
        });

    assert_eq!(
        rendered_content(&second_grid),
        rendered_content(&main_grid),
        "a client returning from a tab larger than the one it lands in must not leave that tab's leftovers around it"
    );
    second_client.quit();
    zellij.quit();
}

#[test]
fn opening_a_new_tab_regrows_the_tab_its_creator_left() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    mark_pane(zellij.main_client(), &first_terminal, "oneone");

    let second_client = zellij.attach_client(LARGER_CLIENT_SIZE);
    second_client.wait_until(
        "larger client attached to the shared tab",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "oneone"),
    );
    first_terminal.wait_for_size("shared tab sized to its smallest viewer", |cols, rows| {
        (cols, rows) == pane_size_in_tab_sized(TERMINAL_SIZE)
    });

    open_marked_tab(&zellij, zellij.main_client(), "twotwo");

    first_terminal.wait_for_size(
        "the tab left behind grows back to fit the larger client still viewing it",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE),
    );
    second_client.wait_until(
        "the larger client regains its full dimensions on the tab it was left alone on",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, LARGER_CLIENT_SIZE, "oneone"),
    );
    second_client.quit();
    zellij.quit();
}

#[test]
fn closing_a_tab_resizes_survivors_per_client() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    mark_pane(zellij.main_client(), &first_terminal, "oneone");

    let second_client = zellij.attach_client(LARGER_CLIENT_SIZE);
    second_client.wait_until(
        "second client attached to the shared tab",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "oneone"),
    );
    let second_tab_terminal = open_marked_tab(&zellij, &second_client, "twotwo");
    second_tab_terminal.wait_for_size("second tab laid out for its creator", |cols, rows| {
        (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE)
    });

    go_to_first_tab(&second_client);
    second_client.wait_until("both clients share the first tab again", |grid_snapshot| {
        settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "oneone")
            && !grid_snapshot.contains("twotwo")
    });
    second_tab_terminal.wait_for_size(
        "the tab left with no viewer keeps the size of its last one",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE),
    );

    close_focused_tab(zellij.main_client());

    second_tab_terminal.wait_for_size(
        "the tab both clients land in after closing the one they shared is sized to the smaller of them",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(TERMINAL_SIZE),
    );
    second_client.wait_until(
        "larger client renders the surviving tab at the shared size",
        |grid_snapshot| {
            settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "twotwo")
                && !grid_snapshot.contains("oneone")
        },
    );
    zellij.wait_until(
        "main client renders the surviving tab at its own size",
        |grid_snapshot| {
            settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "twotwo")
                && !grid_snapshot.contains("oneone")
        },
    );

    let third_tab_terminal = open_marked_tab(&zellij, &second_client, "threethree");
    third_tab_terminal.wait_for_size("third tab laid out for its creator", |cols, rows| {
        (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE)
    });

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('2'));
    third_tab_terminal.wait_for_size(
        "the third tab shrinks once the smaller client joins it",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(TERMINAL_SIZE),
    );
    zellij.wait_until("main client followed to the third tab", |grid_snapshot| {
        settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "threethree")
    });

    go_to_first_tab(&second_client);
    second_tab_terminal.wait_for_size(
        "the second tab grows back for the larger client returning to it alone",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE),
    );
    second_client.wait_until(
        "larger client regains its full dimensions on the tab it now has to itself",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, LARGER_CLIENT_SIZE, "twotwo"),
    );

    close_focused_tab(zellij.main_client());

    second_tab_terminal.wait_for_size(
        "the tab the returning client lands in shrinks to the smaller of its two viewers",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(TERMINAL_SIZE),
    );
    let second_grid = second_client.wait_until(
        "larger client is constrained again by the client returning from the closed tab",
        |grid_snapshot| {
            settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "twotwo")
                && grid_snapshot.contains("Tab #2 [ ]")
                && !grid_snapshot.contains("threethree")
        },
    );
    let main_grid = zellij.wait_until(
        "main client returns to the surviving tab at its own size",
        |grid_snapshot| {
            settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "twotwo")
                && grid_snapshot.contains("Tab #2 [ ]")
                && !grid_snapshot.contains("threethree")
        },
    );

    assert_snapshot!(normalized(&tab_region(&main_grid)));
    assert_snapshot!(normalized(&tab_region(&second_grid)));
    second_client.quit();
    zellij.quit();
}

#[test]
fn detaching_one_client_reflows_the_remaining_client() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    mark_pane(zellij.main_client(), &first_terminal, "oneone");

    let second_client = zellij.attach_client(SMALLER_CLIENT_SIZE);
    second_client.wait_until(
        "smaller client attached to the shared tab",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, SMALLER_CLIENT_SIZE, "oneone"),
    );
    first_terminal.wait_for_size(
        "the shared tab shrinks to the smaller of its two viewers",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(SMALLER_CLIENT_SIZE),
    );
    zellij.wait_until(
        "main client renders inside the shrunken shared tab",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, SMALLER_CLIENT_SIZE, "oneone"),
    );

    second_client.detach();

    first_terminal.wait_for_size(
        "the tab grows back to fit its only remaining viewer",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(TERMINAL_SIZE),
    );
    let main_grid = zellij.wait_until(
        "remaining client reflows to its own dimensions with the detached peer gone from the tab bar",
        |grid_snapshot| {
            settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "oneone")
                && !grid_snapshot.contains("[ ]")
        },
    );

    assert_snapshot!(normalized(&tab_region(&main_grid)));
    zellij.quit();
}

#[test]
fn moving_a_pane_to_another_tab_recomputes_both_tabs_sizes() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    mark_pane(zellij.main_client(), &first_terminal, "oneone");

    let second_client = zellij.attach_client(LARGER_CLIENT_SIZE);
    second_client.wait_until(
        "second client attached to the shared tab",
        |grid_snapshot| settled_in_tab_sized(grid_snapshot, TERMINAL_SIZE, "oneone"),
    );
    let second_tab_terminal = open_marked_tab(&zellij, &second_client, "twotwo");
    second_tab_terminal.wait_for_size("second tab laid out for its creator", |cols, rows| {
        (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE)
    });

    let moved_terminal = split_right_marked(&zellij, &second_client, "threethree");
    moved_terminal.wait_for_size(
        "the pane to move shares the second tab's height",
        |_, rows| rows == split_pane_rows_in_tab_sized(LARGER_CLIENT_SIZE),
    );

    second_client.send_stdin(&keys::ctrl('t'));
    second_client.send_stdin(&keys::key('['));

    let second_grid = second_client.wait_until(
        "second client follows the moved pane into the first tab",
        |grid_snapshot| {
            settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "threethree")
                && grid_snapshot.contains("Tab #1 [ ]")
                && grid_snapshot.contains("oneone")
                && !grid_snapshot.contains("twotwo")
        },
    );
    let main_grid = zellij.wait_until(
        "main client sees the arriving pane inside its own tab",
        |grid_snapshot| {
            settled_in_normal_mode(grid_snapshot, TERMINAL_SIZE, "oneone")
                && grid_snapshot.contains("Tab #1 [ ]")
                && grid_snapshot.contains("threethree")
        },
    );

    let (moved_cols, _) = moved_terminal.wait_for_size(
        "the moved pane adopts the destination tab's height",
        |_, rows| rows == split_pane_rows_in_tab_sized(TERMINAL_SIZE),
    );
    assert!(
        moved_cols < TERMINAL_SIZE.cols as u16,
        "the moved pane must share the destination tab's width, got {} columns",
        moved_cols
    );
    let (_, resident_rows) = first_terminal.wait_for_size(
        "the destination tab's resident pane makes room for the arrival",
        |cols, _| cols < TERMINAL_SIZE.cols as u16,
    );
    assert_eq!(
        resident_rows,
        split_pane_rows_in_tab_sized(TERMINAL_SIZE),
        "the destination tab must keep the height its viewers give it"
    );
    second_tab_terminal.wait_for_size(
        "the source tab keeps the size of its last viewer while relayouting around the departure",
        |cols, rows| (cols, rows) == pane_size_in_tab_sized(LARGER_CLIENT_SIZE),
    );

    assert_snapshot!(normalized(&tab_region(&main_grid)));
    assert_snapshot!(normalized(&tab_region(&second_grid)));
    second_client.quit();
    zellij.quit();
}

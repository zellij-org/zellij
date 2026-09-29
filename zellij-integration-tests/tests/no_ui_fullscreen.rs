#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    assert_same_rendered_grid, claim_first_terminal_and_wait_for_prompt, col, keys, normalized,
    split_right_and_wait_for_prompt, start_zellij, Size, PROMPT, TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;
use zellij_utils::input::options::PaneFrameStyle;

const FULL_FRAME_CORNER: &str = "┌";

fn toggle_no_ui_fullscreen_via_keybinding(zellij: &zellij_integration_tests::TestSession) {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('F'));
}

#[test]
fn no_ui_fullscreen_hides_ui_and_covers_display() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    toggle_no_ui_fullscreen_via_keybinding(&zellij);

    let grid_snapshot = zellij.wait_until(
        "focused pane covers the whole display without chrome",
        |grid_snapshot| {
            !grid_snapshot.tab_bar_appears()
                && !grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(2).row(0))
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn no_ui_fullscreen_toggle_off_restores_ui_and_layout() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    let before = zellij.wait_until("two panes settled with chrome", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(col(62).row(2))
    });

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    zellij.wait_until("chrome hidden while no-ui fullscreen", |grid_snapshot| {
        !grid_snapshot.tab_bar_appears() && !grid_snapshot.status_bar_appears()
    });

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    let after = zellij.wait_until("chrome and layout restored", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("(FULLSCREEN)")
            && grid_snapshot.cursor_is_at(col(62).row(2))
    });

    assert_same_rendered_grid(
        &after,
        &before,
        "layout, UI panes and frames are restored exactly",
    );
    zellij.quit();
}

#[test]
fn no_ui_fullscreen_works_with_single_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    toggle_no_ui_fullscreen_via_keybinding(&zellij);

    let grid_snapshot = zellij.wait_until(
        "single pane covers the whole display without chrome",
        |grid_snapshot| {
            !grid_snapshot.tab_bar_appears()
                && !grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(2).row(0))
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    zellij.wait_until("chrome restored for single pane", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(col(2).row(1))
    });
    zellij.quit();
}

#[test]
fn resize_terminal_while_no_ui_fullscreen_tracks_display() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    zellij.wait_until("chrome hidden while no-ui fullscreen", |grid_snapshot| {
        !grid_snapshot.tab_bar_appears()
            && !grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(col(2).row(0))
    });

    zellij.resize(Size {
        cols: 100,
        rows: 24,
    });
    right_terminal.wait_for_size(
        "no-ui fullscreen pane tracks the new display size",
        |cols, rows| cols == 100 && rows == 24,
    );
    let grid_snapshot = zellij.wait_until(
        "no-ui fullscreen still covers the resized display",
        |grid_snapshot| {
            !grid_snapshot.tab_bar_appears()
                && !grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(2).row(0))
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    zellij.wait_until("chrome restored at the new size", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.contains("Ctrl +")
            && grid_snapshot.cursor_is_at(col(52).row(2))
    });
    zellij.quit();
}

#[test]
fn no_ui_fullscreen_with_full_frames() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);

    let exit_code = zellij.run_cli_action(CliAction::SetPaneFrameStyle {
        style: PaneFrameStyle::Full,
    });
    assert_eq!(exit_code, 0, "set-pane-frame-style exited cleanly");
    zellij.wait_until("full frames drawn", |grid_snapshot| {
        grid_snapshot.contains(FULL_FRAME_CORNER)
    });

    toggle_no_ui_fullscreen_via_keybinding(&zellij);

    right_terminal.wait_for_size(
        "no-ui fullscreen pane forced frameless and sized to the exact display",
        |cols, rows| cols == TERMINAL_SIZE.cols as u16 && rows == TERMINAL_SIZE.rows as u16,
    );
    let grid_snapshot = zellij.wait_until(
        "frameless fullscreen pane covers the whole display without chrome",
        |grid_snapshot| {
            !grid_snapshot.tab_bar_appears()
                && !grid_snapshot.status_bar_appears()
                && !grid_snapshot.contains(FULL_FRAME_CORNER)
                && grid_snapshot.cursor_is_at(col(2).row(0))
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    zellij.wait_until("chrome and frames restored", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains(FULL_FRAME_CORNER)
    });
    zellij.quit();
}

#[test]
fn no_ui_fullscreen_with_frameless_panes() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);

    let exit_code = zellij.run_cli_action(CliAction::SetPaneFrameStyle {
        style: PaneFrameStyle::None,
    });
    assert_eq!(exit_code, 0, "set-pane-frame-style exited cleanly");
    zellij.wait_until("titles removed in none mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && !grid_snapshot.contains("Pane #1")
    });

    toggle_no_ui_fullscreen_via_keybinding(&zellij);

    right_terminal.wait_for_size(
        "frameless no-ui fullscreen pane sized to the exact display",
        |cols, rows| cols == TERMINAL_SIZE.cols as u16 && rows == TERMINAL_SIZE.rows as u16,
    );
    zellij.wait_until(
        "frameless pane covers the whole display without chrome",
        |grid_snapshot| {
            !grid_snapshot.tab_bar_appears()
                && !grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(2).row(0))
        },
    );

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    zellij.wait_until("chrome restored in none mode", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
fn no_ui_fullscreen_on_focused_floating_pane_covers_display() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(PROMPT);
    floating_terminal.output(b"floating_marker");
    zellij.wait_until(
        "floating pane rendered and focused in normal mode",
        |grid_snapshot| {
            grid_snapshot.contains(FULL_FRAME_CORNER)
                && grid_snapshot.contains("floating_marker")
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.tab_bar_appears()
        },
    );

    let exit_code = zellij.run_cli_action(CliAction::ToggleNoUiFullscreen { pane_id: None });
    assert_eq!(exit_code, 0, "toggle-no-ui-fullscreen exited cleanly");

    zellij.wait_until(
        "focused floating pane goes no-ui fullscreen and hides the chrome",
        |grid_snapshot| {
            grid_snapshot.contains("floating_marker")
                && !grid_snapshot.tab_bar_appears()
                && !grid_snapshot.status_bar_appears()
                && !grid_snapshot.contains(FULL_FRAME_CORNER)
        },
    );

    let exit_code = zellij.run_cli_action(CliAction::ToggleNoUiFullscreen { pane_id: None });
    assert_eq!(exit_code, 0, "toggle-no-ui-fullscreen exited cleanly");
    zellij.wait_until(
        "toggling off restores the chrome and the floating frame",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains(FULL_FRAME_CORNER)
        },
    );
    zellij.quit();
}

#[test]
fn regular_fullscreen_switches_to_no_ui_fullscreen() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('f'));
    zellij.wait_until("regular fullscreen keeps the chrome", |grid_snapshot| {
        grid_snapshot.contains("(FULLSCREEN)")
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(col(2).row(2))
    });

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    zellij.wait_until(
        "switching kinds hides the chrome without leaving fullscreen",
        |grid_snapshot| {
            !grid_snapshot.tab_bar_appears()
                && !grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(2).row(0))
        },
    );

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    zellij.wait_until("fullscreen fully off, layout restored", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("(FULLSCREEN)")
            && grid_snapshot.cursor_is_at(col(62).row(2))
    });
    zellij.quit();
}

#[test]
fn move_focus_while_no_ui_fullscreen_keeps_no_ui() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    zellij.wait_until("right pane is no-ui fullscreen", |grid_snapshot| {
        !grid_snapshot.tab_bar_appears()
            && !grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(col(2).row(0))
    });

    zellij.send_stdin(&keys::alt('h'));
    first_terminal.output(b"left_marker");

    let grid_snapshot = zellij.wait_until(
        "focus moved to the left pane which is now no-ui fullscreen",
        |grid_snapshot| grid_snapshot.contains("left_marker"),
    );
    assert!(
        !grid_snapshot.tab_bar_appears() && !grid_snapshot.status_bar_appears(),
        "no-ui fullscreen kind survives a focus move"
    );
    zellij.quit();
}

#[test]
fn new_pane_while_no_ui_fullscreen_breaks_out_and_restores_chrome() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    zellij.wait_until("chrome hidden while no-ui fullscreen", |grid_snapshot| {
        !grid_snapshot.tab_bar_appears() && !grid_snapshot.status_bar_appears()
    });

    zellij.send_stdin(&keys::alt('n'));
    let new_terminal = zellij.expect_pty_spawn();
    new_terminal.output(PROMPT);

    let grid_snapshot = zellij.wait_until(
        "all three panes tiled with chrome after breaking out of no-ui fullscreen",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && !grid_snapshot.contains("(FULLSCREEN)")
                && grid_snapshot.contains("Pane #1")
                && grid_snapshot.contains("Pane #2")
                && grid_snapshot.contains("Pane #3")
                && grid_snapshot.cursor.is_some()
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn closing_the_no_ui_fullscreen_pane_restores_the_remaining_layout() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);
    let two_pane_state = zellij.wait_until("two panes settled", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(2))
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('d'));
    let third_terminal = zellij.expect_pty_spawn();
    third_terminal.output(PROMPT);
    zellij.wait_until("third pane split below the right pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(13))
    });

    toggle_no_ui_fullscreen_via_keybinding(&zellij);
    zellij.wait_until("third pane is no-ui fullscreen", |grid_snapshot| {
        !grid_snapshot.tab_bar_appears() && !grid_snapshot.status_bar_appears()
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('x'));
    let after_close = zellij.wait_until(
        "no-ui fullscreen pane closed, two panes and chrome restored",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && !grid_snapshot.contains("(FULLSCREEN)")
                && grid_snapshot.cursor_is_at(col(62).row(2))
        },
    );

    assert_same_rendered_grid(
        &after_close,
        &two_pane_state,
        "closing the no-ui fullscreen pane restores the pre-split layout",
    );
    zellij.quit();
}

#[test]
fn no_ui_fullscreen_through_the_cli() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    let exit_code = zellij.run_cli_action(CliAction::ToggleNoUiFullscreen { pane_id: None });
    assert_eq!(exit_code, 0, "toggle-no-ui-fullscreen exited cleanly");
    zellij.wait_until("cli action hid the chrome", |grid_snapshot| {
        !grid_snapshot.tab_bar_appears() && !grid_snapshot.status_bar_appears()
    });

    let exit_code = zellij.run_cli_action(CliAction::ToggleNoUiFullscreen { pane_id: None });
    assert_eq!(exit_code, 0, "toggle-no-ui-fullscreen exited cleanly");
    zellij.wait_until("cli action restored the chrome", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(col(62).row(2))
    });
    zellij.quit();
}

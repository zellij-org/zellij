#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, start_zellij, GridSnapshot, TestRunner,
    TestSession, TERMINAL_SIZE,
};

const ARROW_LEFT: &[u8] = b"\x1b[D";
const ARROW_RIGHT: &[u8] = b"\x1b[C";
const ARROW_DOWN: &[u8] = b"\x1b[B";

fn open_settings(zellij: &TestSession) -> GridSnapshot {
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    zellij.wait_until("settings screen opened", |grid_snapshot| {
        grid_snapshot.contains("Configuration") && grid_snapshot.contains("unsaved change")
    })
}

fn close_search(zellij: &TestSession) {
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("search closed and categories shown", |grid_snapshot| {
        grid_snapshot.contains("Mouse and clipboard") && !grid_snapshot.contains(" match")
    });
}

fn close_settings(zellij: &TestSession) {
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("settings screen closed", |grid_snapshot| {
        !grid_snapshot.contains("Configuration")
    });
}

fn focus_setting_by_search(zellij: &TestSession, query: &str, setting_name: &str) {
    zellij.send_stdin(&keys::key('/'));
    zellij.send_stdin(query.as_bytes());
    let setting_name = setting_name.to_owned();
    zellij.wait_until("search shows the setting", |grid_snapshot| {
        grid_snapshot.contains(&setting_name)
    });
    zellij.send_stdin(&keys::ENTER);
}

fn tab_bar_shows_session_name(grid_snapshot: &GridSnapshot, session_name: &str) -> bool {
    grid_snapshot
        .lines()
        .first()
        .map(|line| line.contains(session_name))
        .unwrap_or(false)
}

fn tiled_pane_has_full_frame(grid_snapshot: &GridSnapshot) -> bool {
    grid_snapshot
        .lines()
        .get(10)
        .map(|line| line.starts_with('│'))
        .unwrap_or(false)
}

fn toggle_hide_session_name(zellij: &TestSession) {
    focus_setting_by_search(zellij, "hide session", "Hide session name");
    zellij.send_stdin(&keys::SPACE);
}

#[test]
fn changing_a_toggle_and_a_dropdown_reaches_the_session() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let session_name = zellij.session_name().to_owned();
    open_settings(&zellij);
    zellij.wait_until("tab bar shows the session name", |grid_snapshot| {
        tab_bar_shows_session_name(grid_snapshot, &session_name)
            && !tiled_pane_has_full_frame(grid_snapshot)
    });

    toggle_hide_session_name(&zellij);
    zellij.wait_until("session name hidden in the tab bar", |grid_snapshot| {
        !tab_bar_shows_session_name(grid_snapshot, &session_name)
            && grid_snapshot.contains("● unsaved")
    });

    focus_setting_by_search(&zellij, "frame style", "Pane frame style");
    zellij.send_stdin(ARROW_LEFT);
    zellij.wait_until("tiled pane drawn with a full frame", |grid_snapshot| {
        tiled_pane_has_full_frame(grid_snapshot) && grid_snapshot.contains("2 unsaved changes")
    });
    zellij.quit();
}

#[test]
fn the_unsaved_marker_is_still_shown_after_closing_and_reopening_the_screen() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_settings(&zellij);

    toggle_hide_session_name(&zellij);
    zellij.wait_until("unsaved marker shown", |grid_snapshot| {
        grid_snapshot.contains("● unsaved") && grid_snapshot.contains("1 unsaved change ")
    });
    close_search(&zellij);
    close_settings(&zellij);

    open_settings(&zellij);
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(&keys::TAB);
    zellij.wait_until(
        "unsaved marker shown again after reopening",
        |grid_snapshot| {
            grid_snapshot.contains("1 unsaved change ")
                && grid_snapshot.contains("Hide session name")
                && grid_snapshot.contains("● unsaved")
        },
    );
    zellij.quit();
}

#[test]
fn a_restart_only_setting_is_written_to_the_file_after_confirming_the_save() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij
        .config_file_path()
        .expect("the test session has a config file");
    let backup_file_path = config_file_path.with_file_name(format!(
        "{}.bak",
        config_file_path.file_name().unwrap().to_string_lossy()
    ));
    assert!(!backup_file_path.exists());
    open_settings(&zellij);

    focus_setting_by_search(&zellij, "scrollback lines", "Scrollback lines");
    zellij.send_stdin(ARROW_RIGHT);
    zellij.wait_until("restart-only change is pending", |grid_snapshot| {
        grid_snapshot.contains("11000")
            && grid_snapshot.contains("● unsaved ⟳ restart")
            && grid_snapshot.contains("1 unsaved change ")
    });

    zellij.send_stdin(&keys::ctrl('a'));
    zellij.wait_until("save confirm dialog shown", |grid_snapshot| {
        grid_snapshot.contains("Save settings?") && grid_snapshot.contains("Cancel")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("save reported with a restart note", |grid_snapshot| {
        grid_snapshot.contains("Restart Zellij to apply: Scrollback lines")
    });

    let written = std::fs::read_to_string(&config_file_path).unwrap();
    assert!(
        written.contains("scroll_buffer_size 11000"),
        "config file was not updated: {}",
        written
    );
    assert!(backup_file_path.exists(), "no backup file was created");
    let backup = std::fs::read_to_string(&backup_file_path).unwrap();
    assert!(!backup.contains("scroll_buffer_size 11000"));
    zellij.quit();
}

#[test]
fn ctrl_r_reverts_all_unsaved_changes() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let session_name = zellij.session_name().to_owned();
    open_settings(&zellij);

    toggle_hide_session_name(&zellij);
    zellij.wait_until("change applied", |grid_snapshot| {
        !tab_bar_shows_session_name(grid_snapshot, &session_name)
            && grid_snapshot.contains("1 unsaved change ")
    });

    zellij.send_stdin(&keys::ctrl('r'));
    zellij.wait_until("revert confirm dialog shown", |grid_snapshot| {
        grid_snapshot.contains("Revert all?")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("change reverted", |grid_snapshot| {
        tab_bar_shows_session_name(grid_snapshot, &session_name)
            && grid_snapshot.contains("0 unsaved changes")
            && !grid_snapshot.contains("● unsaved")
    });
    zellij.quit();
}

fn sgr_motion(column: usize, line: usize) -> Vec<u8> {
    format!("\u{1b}[<35;{};{}M", column, line).into_bytes()
}

#[test]
fn the_unsaved_marker_in_the_side_menu_takes_the_hover_background() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true\nadvanced_mouse_actions true\nmouse_hover_effects true")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_settings(&zellij);
    toggle_hide_session_name(&zellij);
    zellij.wait_until("unsaved marker shown", |grid_snapshot| {
        grid_snapshot.contains("1 unsaved change ")
    });
    close_search(&zellij);
    let grid_snapshot = zellij.wait_until("menu marker shown", |grid_snapshot| {
        grid_snapshot.contains("Pane frames and borders ●")
    });
    let row = grid_snapshot
        .row_of_line("Pane frames and borders ●")
        .unwrap();
    let line = grid_snapshot.lines()[row].clone();
    let title_column = line[..line.find("Pane frames").unwrap()].chars().count();
    let marker_column = line[..line.find('●').unwrap()].chars().count();

    let background_before_hover = grid_snapshot.cell_style(title_column, row).background;
    zellij.send_stdin(&sgr_motion(title_column + 2, row + 1));
    zellij.wait_until(
        "the hovered row and its marker share a background",
        |grid_snapshot| {
            let title_background = grid_snapshot.cell_style(title_column, row).background;
            let marker_background = grid_snapshot.cell_style(marker_column, row).background;
            title_background != background_before_hover && marker_background == title_background
        },
    );

    zellij.quit();
}

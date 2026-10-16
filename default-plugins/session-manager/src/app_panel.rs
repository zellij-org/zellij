use std::cell::Cell;
use std::path::PathBuf;

use zellij_tile::prelude::{ConnectToSession, LayoutInfo, PaneId};

pub const ROLE: &str = "app_panel";
pub const VIEW_PIPE: &str = "zellij-app-panel-view";
pub const QUIT_REQUEST: &str = ".zellij-app-quit";
pub const QUIT_HINT: &str = "<Enter> stop everything and quit    <Esc> cancel";

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static SWITCHED: Cell<bool> = const { Cell::new(false) };
}

pub fn enable() {
    ENABLED.with(|enabled| enabled.set(true));
}

pub fn is_enabled() -> bool {
    ENABLED.with(|enabled| enabled.get())
}

pub fn begin_event() {
    SWITCHED.with(|switched| switched.set(false));
}

fn note_switch() {
    if is_enabled() {
        SWITCHED.with(|switched| switched.set(true));
    }
}

fn leave() {
    let switched = SWITCHED.with(|switched| switched.replace(false));
    if !switched {
        zellij_tile::prelude::detach();
    }
}

pub fn close_self() {
    if is_enabled() {
        leave();
    } else {
        zellij_tile::prelude::close_self();
    }
}

pub fn hide_self() {
    if is_enabled() {
        leave();
    } else {
        zellij_tile::prelude::hide_self();
    }
}

pub fn switch_session(name: Option<&str>) {
    note_switch();
    zellij_tile::prelude::switch_session(name);
}

pub fn switch_session_with_focus(name: &str, tab: Option<usize>, pane: Option<(u32, bool)>) {
    note_switch();
    zellij_tile::prelude::switch_session_with_focus(name, tab, pane);
}

pub fn switch_session_with_layout(name: Option<&str>, layout: LayoutInfo, cwd: Option<PathBuf>) {
    note_switch();
    zellij_tile::prelude::switch_session_with_layout(name, layout, cwd);
}

pub fn switch_session_with_options(options: ConnectToSession) {
    note_switch();
    zellij_tile::prelude::switch_session_with_options(options);
}

pub fn switch_session_and_close_current(name: &str, tab: Option<usize>, pane: Option<(u32, bool)>) {
    note_switch();
    if is_enabled() {
        zellij_tile::prelude::switch_session_with_focus(name, tab, pane);
    } else {
        zellij_tile::prelude::switch_session_and_close_current(name, tab, pane);
    }
}

pub const CLOSE_BUTTON: &str = "×";

pub fn close_button_column(cols: usize) -> usize {
    cols.saturating_sub(2)
}

pub fn is_close_button(line: isize, column: usize, cols: usize) -> bool {
    let at = close_button_column(cols);
    line == 0 && cols > 0 && column + 1 >= at && column <= at + 1
}

pub fn render_close_button(cols: usize) {
    use zellij_tile::prelude::{print_text_with_coordinates, Text};
    if cols < 4 {
        return;
    }
    print_text_with_coordinates(
        Text::from(CLOSE_BUTTON).color_range(3, ..),
        close_button_column(cols),
        0,
        None,
        None,
    );
}

pub fn request_quit() {
    note_switch();
    zellij_tile::prelude::switch_session(Some(QUIT_REQUEST));
}

pub fn title(view: &str) -> String {
    format!("Zellij · {}", view)
}

pub fn quit_lines(sessions: &[String]) -> Vec<String> {
    let mut lines = vec![];
    match sessions.len() {
        0 => {
            lines.push("Quit Zellij?".to_owned());
            lines.push(String::new());
            lines.push("No sessions are running.".to_owned());
        },
        count => {
            lines.push("Quit Zellij and stop all sessions?".to_owned());
            lines.push(String::new());
            lines.push(if count == 1 {
                "This stops 1 session:".to_owned()
            } else {
                format!("This stops these {} sessions:", count)
            });
            lines.extend(sessions.iter().map(|name| format!("  {}", name)));
        },
    }
    lines.push(String::new());
    lines.push(if sessions.is_empty() {
        "<Enter> quit    <Esc> cancel".to_owned()
    } else {
        QUIT_HINT.to_owned()
    });
    lines
}

pub fn centered_block(lines: &[String], rows: usize, cols: usize) -> (usize, usize, Vec<String>) {
    let room = rows.saturating_sub(2).max(1);
    let mut shown: Vec<String> = lines.to_vec();
    if shown.len() > room && room >= 4 {
        let hidden = shown.len() - room + 1;
        let tail = shown.split_off(shown.len() - 2);
        shown.truncate(room.saturating_sub(3));
        shown.push(format!("  … and {} more", hidden));
        shown.extend(tail);
    }
    let width = shown
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        .min(cols);
    let x = cols.saturating_sub(width) / 2;
    let y = rows.saturating_sub(shown.len()) / 2;
    (x, y, shown)
}

pub fn focus_session_pane(session: &str, pane_id: PaneId) {
    note_switch();
    zellij_tile::prelude::focus_session_pane(session, pane_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quit_question_names_every_session_and_sits_in_the_middle() {
        let sessions = vec!["api".to_owned(), "docs".to_owned()];
        let lines = quit_lines(&sessions);
        assert_eq!(lines[0], "Quit Zellij and stop all sessions?");
        assert!(lines.contains(&"  api".to_owned()));
        assert!(lines.contains(&"  docs".to_owned()));
        let (x, y, shown) = centered_block(&lines, 30, 100);
        let width = shown.iter().map(|l| l.chars().count()).max().unwrap();
        assert_eq!(x, (100 - width) / 2);
        assert_eq!(y, (30 - shown.len()) / 2);
        let none = quit_lines(&[]);
        assert_eq!(none[0], "Quit Zellij?");
        assert!(none.iter().any(|l| l == "No sessions are running."));
        assert!(!none.iter().any(|l| l.contains("stop everything")));
    }

    #[test]
    fn the_close_button_sits_at_the_end_of_the_first_line() {
        assert_eq!(close_button_column(80), 78);
        assert!(is_close_button(0, 78, 80));
        assert!(is_close_button(0, 77, 80));
        assert!(is_close_button(0, 79, 80));
        assert!(!is_close_button(1, 78, 80));
        assert!(!is_close_button(0, 70, 80));
    }

    #[test]
    fn a_long_list_is_shortened_but_keeps_the_question_and_the_keys() {
        let sessions: Vec<String> = (0..40).map(|i| format!("s{}", i)).collect();
        let lines = quit_lines(&sessions);
        let (_, _, shown) = centered_block(&lines, 20, 100);
        assert!(shown.len() <= 18);
        assert_eq!(shown[0], lines[0]);
        assert_eq!(shown.last(), lines.last());
        assert!(shown.iter().any(|l| l.contains("… and")));
    }
}

use anyhow::{Context, Result};
use miette::{GraphicalReportHandler, GraphicalTheme};
use zellij_utils::input::config::ConfigError;
use zellij_utils::ipc::ExitReason;
use zellij_utils::structured_render::{FrameBuilder, WireCell};

use crate::client_loop::Ending;

use crate::connection::Geometry;
use crate::font::FontStack;
use crate::options::Options;

const MARGIN: usize = 2;
const TAB: &str = "    ";
const WHEEL_LINES: f64 = 3.0;

const NOTICE_COLS: usize = 100;
const NOTICE_MIN_ROWS: usize = 10;
const NOTICE_MAX_ROWS: usize = 40;

const LOST: &str = "The connection to the session closed without a reason. The session's server \
                    may have crashed.";

pub const LEAVE: &str = "Press Enter or Escape, or close this window, to exit.";

pub fn trouble(ending: &Ending, session_name: &str) -> Option<String> {
    match ending {
        Ending::Switched(_) => None,
        Ending::Exited(
            ExitReason::Normal | ExitReason::NormalDetached | ExitReason::CustomExitStatus(0),
        ) => None,
        Ending::Exited(ExitReason::CustomExitStatus(status)) => Some(exit_status(*status)),
        Ending::Exited(ExitReason::Disconnect) => Some(disconnected(session_name)),
        Ending::Exited(reason) => Some(reason.to_string().trim().to_owned()),
        Ending::Lost => Some(LOST.to_owned()),
        Ending::Failed(message) => Some(message.clone()),
    }
}

pub fn exit_status(status: i32) -> String {
    format!("The session ended with exit status {}.", status)
}

fn disconnected(session_name: &str) -> String {
    let reattach = if session_name.is_empty() {
        "see `zellij ls` and `zellij attach`".to_owned()
    } else {
        format!("`zellij attach {}`", session_name)
    };
    format!(
        "This window lost its connection to the session and was disconnected as a safety \
         measure.\n\nThe session should still exist, and none of its data should be lost.\n\n\
         To reattach: {}",
        reattach
    )
}

pub fn ended(trouble: &str) -> String {
    format!(
        "{}\n\nDetails are in the log at {}\n\n{}",
        trouble,
        crate::diagnostics::log_file().display(),
        LEAVE
    )
}

pub fn refused(text: &str) -> String {
    format!("{}\n\n{}", text, LEAVE)
}

pub fn described(error: &ConfigError) -> String {
    let ConfigError::KdlError(kdl) = error else {
        return error.to_string();
    };
    if kdl.src.is_none() {
        let mut text = format!("{}: {}", kdl, kdl.error_message);
        if let Some(help) = &kdl.help_message {
            text.push_str("\n\n");
            text.push_str(help);
        }
        return text;
    }
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor())
        .with_width(text_width(NOTICE_COLS))
        .with_links(false);
    let mut text = String::new();
    match handler.render_report(&mut text, kdl) {
        Ok(()) => text.trim_end().to_owned(),
        Err(_) => error.to_string(),
    }
}

pub struct Notice {
    text: String,
    top: usize,
    pending: f64,
}

impl Notice {
    pub fn new(text: impl Into<String>) -> Self {
        Notice {
            text: text.into(),
            top: 0,
            pending: 0.0,
        }
    }

    #[cfg(test)]
    pub fn top(&self) -> usize {
        self.top
    }

    pub fn frame(&mut self, rows: usize, cols: usize) -> Vec<u8> {
        self.top = self.top.min(self.last_top(rows, cols));
        frame_from(&self.text, rows, cols, self.top)
    }

    pub fn scroll(&mut self, by: isize, rows: usize, cols: usize) -> bool {
        let last = self.last_top(rows, cols) as isize;
        let top = (self.top as isize).saturating_add(by).clamp(0, last) as usize;
        let moved = top != self.top;
        self.top = top;
        moved
    }

    pub fn wheel(&mut self, lines: f64, rows: usize, cols: usize) -> bool {
        if !lines.is_finite() {
            return false;
        }
        self.pending += lines;
        let whole = self.pending.trunc();
        self.pending -= whole;
        self.scroll(whole as isize, rows, cols)
    }

    pub fn page(rows: usize) -> isize {
        visible(rows).saturating_sub(1).max(1) as isize
    }

    fn last_top(&self, rows: usize, cols: usize) -> usize {
        wrapped(&self.text, text_width(cols))
            .len()
            .saturating_sub(visible(rows))
    }
}

pub fn wheel_lines(line_delta: f32) -> f64 {
    -(line_delta as f64) * WHEEL_LINES
}

fn text_width(cols: usize) -> usize {
    cols.saturating_sub(MARGIN * 2).max(1)
}

fn top_margin(rows: usize) -> usize {
    MARGIN.min(rows.saturating_sub(1))
}

fn visible(rows: usize) -> usize {
    rows.saturating_sub(top_margin(rows) + MARGIN).max(1)
}

fn frame_from(message: &str, rows: usize, cols: usize, top: usize) -> Vec<u8> {
    let mut builder = FrameBuilder::new(cols as u16, rows as u16, 0);
    builder.set_full_repaint(true);
    builder.set_clear(true);
    let first = top_margin(rows);
    let lines = wrapped(message, text_width(cols));
    for (index, line) in lines.into_iter().skip(top).take(visible(rows)).enumerate() {
        let y = index + first;
        if y >= rows {
            break;
        }
        let cells: Vec<WireCell> = line
            .chars()
            .map(|ch| WireCell {
                ch: ch as u32,
                ..WireCell::BLANK
            })
            .collect();
        builder.push_row(y as u16, MARGIN as u16, &cells);
    }
    builder.finish()
}

fn wrapped(message: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in message.split('\n') {
        let paragraph = paragraph.replace('\t', TAB);
        let paragraph = paragraph.trim_end();
        if paragraph.chars().count() <= width {
            lines.push(paragraph.to_owned());
            continue;
        }
        let indent_len = paragraph
            .chars()
            .take_while(|ch| *ch == ' ')
            .count()
            .min(width / 2);
        let indent = " ".repeat(indent_len);
        let room = width - indent_len;
        let mut line = String::new();
        for word in paragraph.split(' ').filter(|word| !word.is_empty()) {
            if line.is_empty() {
                line.push_str(word);
            } else if line.chars().count() + 1 + word.chars().count() <= room {
                line.push(' ');
                line.push_str(word);
            } else {
                lines.push(format!("{}{}", indent, std::mem::take(&mut line)));
                line.push_str(word);
            }
            while line.chars().count() > room {
                let head: String = line.chars().take(room).collect();
                let tail: String = line.chars().skip(room).collect();
                lines.push(format!("{}{}", indent, head));
                line = tail;
            }
        }
        lines.push(format!("{}{}", indent, line));
    }
    lines
}

pub fn show(message: &str, fonts: FontStack, options: &Options) -> Result<()> {
    let metrics = fonts.metrics();
    let text = refused(message);
    let needed = wrapped(&text, text_width(NOTICE_COLS)).len() + MARGIN * 2;
    let geometry = Geometry {
        rows: needed.clamp(NOTICE_MIN_ROWS, NOTICE_MAX_ROWS),
        cols: NOTICE_COLS,
        cell_width: metrics.width as usize,
        cell_height: metrics.height as usize,
    };
    crate::window::run_notice(&text, geometry, fonts, options)
        .context("the message could not be shown in a window")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TerminalState;
    use zellij_utils::input::config::KdlError;

    fn dumped(message: &str, rows: usize, cols: usize) -> String {
        let mut state = TerminalState::new(rows, cols);
        state
            .apply_frame(&frame_from(message, rows, cols, 0))
            .unwrap();
        state.dump()
    }

    #[test]
    fn the_message_is_readable_in_the_frame_the_window_draws() {
        let dump = dumped("No session with the name 'nope' found!", 10, 80);
        assert!(
            dump.contains("No session with the name 'nope' found!"),
            "{}",
            dump
        );
    }

    #[test]
    fn a_message_wider_than_the_window_wraps_rather_than_being_cut_off() {
        let lines = wrapped("one two three four five six seven", 12);
        assert!(
            lines.iter().all(|line| line.chars().count() <= 12),
            "{:?}",
            lines
        );
        assert_eq!(
            lines.join(" "),
            "one two three four five six seven",
            "{:?}",
            lines
        );
    }

    #[test]
    fn a_word_longer_than_the_window_is_broken_rather_than_dropped() {
        let lines = wrapped(&"x".repeat(100), 40);
        assert_eq!(lines.concat().len(), 100, "{:?}", lines);
        assert!(lines.iter().all(|line| line.chars().count() <= 40));
    }

    #[test]
    fn the_frame_repaints_everything_so_no_earlier_screen_shows_through() {
        let bytes = frame_from("anything", 10, 40, 0);
        let view = zellij_utils::structured_render::decode(&bytes).unwrap();
        assert!(view.header().full_repaint());
        assert!(view.header().clear());
    }

    #[test]
    fn a_normal_ending_needs_no_explanation() {
        for reason in [
            ExitReason::Normal,
            ExitReason::NormalDetached,
            ExitReason::CustomExitStatus(0),
        ] {
            assert_eq!(
                trouble(&Ending::Exited(reason.clone()), "work"),
                None,
                "{:?}",
                reason
            );
        }
        assert_eq!(
            trouble(
                &Ending::Switched(zellij_utils::data::ConnectToSession::default()),
                "work"
            ),
            None
        );
    }

    #[test]
    fn every_other_ending_is_explained_in_the_servers_own_words() {
        for reason in [
            ExitReason::ForceDetached,
            ExitReason::CannotAttach,
            ExitReason::WebClientsForbidden,
            ExitReason::KickedByHost,
            ExitReason::Error("the screen thread failed".to_owned()),
        ] {
            let explained = trouble(&Ending::Exited(reason.clone()), "work")
                .unwrap_or_else(|| panic!("{:?} went unexplained", reason));
            assert_eq!(explained, reason.to_string().trim(), "{:?}", reason);
        }
    }

    #[test]
    fn a_dropped_connection_and_a_client_failure_are_explained() {
        assert_eq!(trouble(&Ending::Lost, "work"), Some(LOST.to_owned()));
        assert_eq!(
            trouble(&Ending::Failed("the server is too old".to_owned()), "work"),
            Some("the server is too old".to_owned())
        );
    }

    #[test]
    fn an_ending_message_names_the_log_and_how_to_leave() {
        let message = ended("Disconnected by host");
        assert!(message.starts_with("Disconnected by host"));
        assert!(message.contains(&crate::diagnostics::log_file().display().to_string()));
        assert!(message.ends_with(LEAVE));
        let dump = dumped(&message, 20, 200);
        assert!(dump.contains("Disconnected by host"), "{}", dump);
        assert!(dump.contains(LEAVE), "{}", dump);
    }

    #[test]
    fn several_lines_stay_several_lines() {
        let dump = dumped("first line\nsecond line", 10, 40);
        assert!(dump.contains("first line"), "{}", dump);
        assert!(dump.contains("second line"), "{}", dump);
        assert!(
            !dump.contains("first line second line"),
            "the two lines must not be joined:\n{}",
            dump
        );
    }

    #[test]
    fn a_non_zero_exit_status_is_explained() {
        assert_eq!(
            trouble(&Ending::Exited(ExitReason::CustomExitStatus(3)), "work"),
            Some("The session ended with exit status 3.".to_owned())
        );
    }

    #[test]
    fn a_disconnect_names_the_session_to_reattach_to() {
        let explained = trouble(&Ending::Exited(ExitReason::Disconnect), "work").unwrap();
        assert!(explained.contains("`zellij attach work`"), "{}", explained);
        assert!(!explained.contains("terminal emulator"), "{}", explained);
        let unnamed = trouble(&Ending::Exited(ExitReason::Disconnect), "").unwrap();
        assert!(unnamed.contains("`zellij ls`"), "{}", unnamed);
    }

    #[test]
    fn indentation_survives_wrapping() {
        let lines = wrapped("list:\n    - alpha beta gamma delta", 16);
        assert_eq!(lines[0], "list:");
        assert!(
            lines[1..].iter().all(|line| line.starts_with("    ")),
            "{:?}",
            lines
        );
        assert!(
            lines.iter().all(|line| line.chars().count() <= 16),
            "{:?}",
            lines
        );
        assert_eq!(wrapped("  short", 40), vec!["  short".to_owned()]);
    }

    #[test]
    fn a_config_error_shows_where_the_mistake_is() {
        let source = "keybinds {\n    normal {\n        bind \"a\" { Nope; }\n    }\n}\n";
        let offset = source.find("Nope").unwrap();
        let error = ConfigError::KdlError(
            KdlError {
                error_message: "Unknown action: Nope".to_owned(),
                src: None,
                offset: Some(offset),
                len: Some(4),
                help_message: None,
            }
            .add_src("config.kdl".to_owned(), source.to_owned()),
        );
        let text = described(&error);
        assert!(
            text.contains("Failed to parse Zellij configuration"),
            "{}",
            text
        );
        assert!(text.contains("config.kdl"), "{}", text);
        assert!(text.contains("bind \"a\" { Nope; }"), "{}", text);
        assert!(text.contains("Unknown action: Nope"), "{}", text);
        assert!(!text.contains('\u{1b}'), "{:?}", text);
        let dump = dumped(&text, 30, NOTICE_COLS);
        assert!(dump.contains("Unknown action: Nope"), "{}", dump);
    }

    #[test]
    fn a_config_error_without_its_source_still_says_what_is_wrong() {
        let error = ConfigError::KdlError(KdlError {
            error_message: "Invalid mode: 'nope'".to_owned(),
            src: None,
            offset: Some(3),
            len: Some(4),
            help_message: None,
        });
        assert!(described(&error).contains("Invalid mode: 'nope'"));
    }

    #[test]
    fn a_long_message_scrolls_and_stops_at_its_ends() {
        let text = (0..30)
            .map(|n| format!("line {}", n))
            .collect::<Vec<_>>()
            .join("\n");
        let mut notice = Notice::new(text);
        let (rows, cols) = (10, 40);
        let mut state = TerminalState::new(rows, cols);
        state.apply_frame(&notice.frame(rows, cols)).unwrap();
        assert!(state.dump().contains("line 0"));
        assert!(!state.dump().contains("line 29"));
        assert!(
            !notice.scroll(-1, rows, cols),
            "the top cannot scroll further up"
        );
        assert!(notice.scroll(isize::MAX, rows, cols));
        assert_eq!(notice.top(), 30 - visible(rows));
        state.apply_frame(&notice.frame(rows, cols)).unwrap();
        assert!(state.dump().contains("line 29"), "{}", state.dump());
        assert!(
            !notice.scroll(1, rows, cols),
            "the end cannot scroll further down"
        );
        assert!(notice.wheel(wheel_lines(1.0), rows, cols));
        assert_eq!(notice.top(), 30 - visible(rows) - 3);
    }

    #[test]
    fn a_taller_window_pulls_a_scrolled_message_back_into_view() {
        let text = (0..30)
            .map(|n| format!("line {}", n))
            .collect::<Vec<_>>()
            .join("\n");
        let mut notice = Notice::new(text);
        notice.scroll(isize::MAX, 10, 40);
        notice.frame(40, 40);
        assert_eq!(notice.top(), 0);
    }
}

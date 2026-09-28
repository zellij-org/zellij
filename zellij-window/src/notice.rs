use anyhow::{anyhow, Context, Result};
use zellij_utils::structured_render::{FrameBuilder, WireCell};

use crate::connection::Geometry;
use crate::font::FontStack;
use crate::options::Options;
use crate::terminal::TerminalState;

const MARGIN: usize = 2;

pub fn frame(message: &str, rows: usize, cols: usize) -> Vec<u8> {
    let mut builder = FrameBuilder::new(cols as u16, rows as u16, 0);
    builder.set_full_repaint(true);
    builder.set_clear(true);
    let width = cols.saturating_sub(MARGIN * 2).max(1);
    for (index, line) in wrapped(message, width).into_iter().enumerate() {
        let y = index + MARGIN.min(rows.saturating_sub(1));
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
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if line.is_empty() {
                line.push_str(word);
            } else if line.chars().count() + 1 + word.chars().count() <= width {
                line.push(' ');
                line.push_str(word);
            } else {
                lines.push(std::mem::take(&mut line));
                line.push_str(word);
            }
            while line.chars().count() > width {
                let head: String = line.chars().take(width).collect();
                let tail: String = line.chars().skip(width).collect();
                lines.push(head);
                line = tail;
            }
        }
        lines.push(line);
    }
    lines
}

pub fn show(message: &str, fonts: FontStack, options: &Options) -> Result<()> {
    let metrics = fonts.metrics();
    let geometry = Geometry {
        rows: NOTICE_ROWS,
        cols: NOTICE_COLS,
        cell_width: metrics.width as usize,
        cell_height: metrics.height as usize,
    };
    let mut state = TerminalState::new(geometry.rows, geometry.cols);
    state.set_cell_size(metrics.width, metrics.height);
    state
        .apply_frame(&frame(message, geometry.rows, geometry.cols))
        .map_err(|e| anyhow!("the message could not be drawn: {}", e))?;
    crate::window::run_notice(state, geometry, fonts, options)
        .context("the message could not be shown in a window")
}

const NOTICE_ROWS: usize = 10;
const NOTICE_COLS: usize = 80;

#[cfg(test)]
mod tests {
    use super::*;

    fn dumped(message: &str, rows: usize, cols: usize) -> String {
        let mut state = TerminalState::new(rows, cols);
        state.apply_frame(&frame(message, rows, cols)).unwrap();
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
        let bytes = frame("anything", 10, 40);
        let view = zellij_utils::structured_render::decode(&bytes).unwrap();
        assert!(view.header().full_repaint());
        assert!(view.header().clear());
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
}

use std::fmt::Write as _;

use zellij_utils::structured_render::{
    WireCell, WireColor, ATTR_BOLD, ATTR_DIM, ATTR_FAST_BLINK, ATTR_HIDDEN, ATTR_ITALIC,
    ATTR_REVERSE, ATTR_SLOW_BLINK, ATTR_STRIKE, UNDERLINE_CURLY, UNDERLINE_DASHED,
    UNDERLINE_DOTTED, UNDERLINE_DOUBLE, UNDERLINE_STRAIGHT,
};

use crate::screen_buffer::{CursorShape, Occupancy};
use crate::terminal::TerminalState;

const LEADING_ATTRS: [(u16, &str); 4] = [
    (ATTR_REVERSE, "INVERSE"),
    (ATTR_BOLD, "BOLD"),
    (ATTR_DIM, "DIM"),
    (ATTR_ITALIC, "ITALIC"),
];

const TRAILING_ATTRS: [(u16, &str); 2] = [(ATTR_STRIKE, "STRIKEOUT"), (ATTR_HIDDEN, "HIDDEN")];

const NAMED_COLORS: [&str; 16] = [
    "Black",
    "Red",
    "Green",
    "Yellow",
    "Blue",
    "Magenta",
    "Cyan",
    "White",
    "BrightBlack",
    "BrightRed",
    "BrightGreen",
    "BrightYellow",
    "BrightBlue",
    "BrightMagenta",
    "BrightCyan",
    "BrightWhite",
];

#[derive(PartialEq, Eq)]
struct Style {
    fg: String,
    bg: String,
    underline_color: Option<String>,
    flags: String,
}

impl Style {
    fn of(cell: WireCell, occupancy: Occupancy) -> Self {
        Self {
            fg: color(cell.fg, "Foreground"),
            bg: color(cell.bg, "Background"),
            underline_color: match WireColor::unpack(cell.underline_color) {
                WireColor::Default => None,
                _ => Some(color(cell.underline_color, "Foreground")),
            },
            flags: flags(cell, occupancy),
        }
    }

    fn is_default(&self) -> bool {
        self.fg == "Foreground"
            && self.bg == "Background"
            && self.underline_color.is_none()
            && self.flags.is_empty()
    }

    fn describe(&self) -> String {
        let mut described = format!("fg={} bg={}", self.fg, self.bg);
        if let Some(underline_color) = &self.underline_color {
            let _ = write!(described, " underline={}", underline_color);
        }
        if !self.flags.is_empty() {
            let _ = write!(described, " flags={}", self.flags);
        }
        described
    }
}

pub fn dump(state: &TerminalState) -> String {
    let size = state.size();
    let (row, col) = state.cursor_position();

    let mut dumped = String::new();
    let _ = writeln!(dumped, "size: {}x{}", size.rows, size.cols);
    let _ = writeln!(
        dumped,
        "cursor: row={} col={} {} shape={}{}",
        row,
        col,
        if state.cursor_is_visible() {
            "visible"
        } else {
            "hidden"
        },
        match state.cursor_shape() {
            CursorShape::Block => "Block",
            CursorShape::Underline => "Underline",
            CursorShape::Beam => "Beam",
            CursorShape::Default => "Default",
        },
        if state.cursor_is_blinking() {
            " blinking"
        } else {
            ""
        }
    );

    let _ = writeln!(dumped, "grid:");
    for row in grid_rows(state) {
        let _ = writeln!(dumped, "|{}|", row);
    }

    let _ = writeln!(dumped, "styles:");
    for line in style_runs(state) {
        let _ = writeln!(dumped, "{}", line);
    }

    let _ = writeln!(dumped, "images:");
    for line in image_lines(state) {
        let _ = writeln!(dumped, "{}", line);
    }

    dumped
}

fn image_lines(state: &TerminalState) -> Vec<String> {
    let graphics = state.graphics();
    let mut lines = Vec::new();

    for id in graphics.resident_ids() {
        let Some(image) = graphics.image(id) else {
            continue;
        };
        lines.push(format!(
            "image {}: {}x{} rgba {} bytes digest={:016x}",
            id,
            image.width,
            image.height,
            image.pixels.len(),
            image.digest()
        ));
    }

    for placement in graphics.placements() {
        lines.push(format!(
            "place {}/{}: cell {},{} offset {},{} src {},{} {}x{} z={}",
            placement.image_id,
            placement.placement_id,
            placement.cell_x,
            placement.cell_y,
            placement.offset_x,
            placement.offset_y,
            placement.source_x,
            placement.source_y,
            placement.source_width,
            placement.source_height,
            placement.z
        ));
    }

    for chunk in state.sixels().chunks() {
        lines.push(format!(
            "sixel {}: cell {},{} {}x{} rgba {} bytes digest={:016x}",
            chunk.id,
            chunk.cell_x,
            chunk.cell_y,
            chunk.width,
            chunk.height,
            chunk.image.pixels.len(),
            chunk.image.digest()
        ));
    }

    lines
}

pub fn grid_rows(state: &TerminalState) -> Vec<String> {
    let size = state.size();
    (0..size.rows)
        .map(|row| {
            (0..size.cols)
                .map(|col| state.screen().cell(row, col).character())
                .collect()
        })
        .collect()
}

fn style_runs(state: &TerminalState) -> Vec<String> {
    let size = state.size();
    let mut lines = Vec::new();

    for row in 0..size.rows {
        let mut run_start = 0usize;
        let mut run_style = Style::of(state.screen().cell(row, 0), state.occupancy(row, 0));

        for col in 1..=size.cols {
            let style = (col < size.cols)
                .then(|| Style::of(state.screen().cell(row, col), state.occupancy(row, col)));
            if style.as_ref() == Some(&run_style) {
                continue;
            }
            if !run_style.is_default() {
                lines.push(format!(
                    "row {}: cols {}-{} {}",
                    row,
                    run_start,
                    col - 1,
                    run_style.describe()
                ));
            }
            run_start = col;
            if let Some(style) = style {
                run_style = style;
            }
        }
    }

    lines
}

fn color(packed: u32, default: &str) -> String {
    match WireColor::unpack(packed) {
        WireColor::Default => default.to_owned(),
        WireColor::Named(index) => NAMED_COLORS
            .get(index as usize)
            .map(|name| (*name).to_owned())
            .unwrap_or_else(|| format!("Named{}", index)),
        WireColor::Indexed(index) => format!("idx{}", index),
        WireColor::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
    }
}

fn flags(cell: WireCell, occupancy: Occupancy) -> String {
    let mut named: Vec<&str> = LEADING_ATTRS
        .iter()
        .filter(|(bit, _)| cell.has(*bit))
        .map(|(_, name)| *name)
        .collect();
    if let Some(underline) = underline_name(cell.underline_style()) {
        named.push(underline);
    }
    named.extend(
        TRAILING_ATTRS
            .iter()
            .filter(|(bit, _)| cell.has(*bit))
            .map(|(_, name)| *name),
    );
    match occupancy {
        Occupancy::WideHead => named.push("WIDE_CHAR"),
        Occupancy::WideTail => named.push("WIDE_CHAR_SPACER"),
        Occupancy::Single => {},
    }
    if cell.has(ATTR_SLOW_BLINK) {
        named.push("BLINK_SLOW");
    }
    if cell.has(ATTR_FAST_BLINK) {
        named.push("BLINK_FAST");
    }
    named.join("|")
}

fn underline_name(style: u16) -> Option<&'static str> {
    match style {
        UNDERLINE_STRAIGHT => Some("UNDERLINE"),
        UNDERLINE_DOUBLE => Some("DOUBLE_UNDERLINE"),
        UNDERLINE_CURLY => Some("UNDERCURL"),
        UNDERLINE_DOTTED => Some("DOTTED_UNDERLINE"),
        UNDERLINE_DASHED => Some("DASHED_UNDERLINE"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::structured_render::{FrameBuilder, CURSOR_SHAPE_UNDERLINE};

    fn state_of(rows: usize, cols: usize, paint: impl FnOnce(&mut FrameBuilder)) -> TerminalState {
        let mut state = TerminalState::new(rows, cols);
        let mut builder = FrameBuilder::new(cols as u16, rows as u16, 0);
        paint(&mut builder);
        state.apply_frame(&builder.finish()).unwrap();
        state
    }

    fn run(text: &str) -> Vec<WireCell> {
        text.chars()
            .map(|character| WireCell {
                ch: character as u32,
                ..WireCell::BLANK
            })
            .collect()
    }

    #[test]
    fn the_dump_carries_size_cursor_grid_and_style_sections() {
        let state = state_of(2, 4, |builder| {
            builder.push_row(0, 0, &run("ab"));
            builder.set_cursor(zellij_utils::structured_render::CursorState {
                x: 2,
                y: 0,
                shape: 0,
                visible: true,
            });
        });
        let dumped = state.dump();
        let lines: Vec<&str> = dumped.lines().collect();
        assert_eq!(lines[0], "size: 2x4");
        assert_eq!(lines[1], "cursor: row=0 col=2 visible shape=Block");
        assert_eq!(lines[2], "grid:");
        assert_eq!(lines[3], "|ab  |");
        assert_eq!(lines[4], "|    |");
        assert_eq!(lines[5], "styles:");
        assert_eq!(lines[6], "images:");
        assert_eq!(lines.len(), 7);
    }

    #[test]
    fn a_hidden_cursor_and_its_shape_are_reported() {
        let state = state_of(1, 4, |builder| {
            builder.push_row(0, 0, &run("a"));
            builder.set_cursor(zellij_utils::structured_render::CursorState {
                x: 1,
                y: 0,
                shape: CURSOR_SHAPE_UNDERLINE | zellij_utils::structured_render::CURSOR_BLINKING,
                visible: false,
            });
        });
        assert!(
            state
                .dump()
                .contains("cursor: row=0 col=1 hidden shape=Underline blinking"),
            "{}",
            state.dump()
        );
    }

    #[test]
    fn adjacent_cells_sharing_a_style_collapse_into_one_run() {
        let state = state_of(1, 8, |builder| {
            let mut cells = run("abcd");
            for cell in &mut cells {
                cell.fg = WireColor::Named(1).pack();
            }
            cells.extend(run("ef"));
            builder.push_row(0, 0, &cells);
        });
        assert_eq!(
            style_runs(&state),
            vec!["row 0: cols 0-3 fg=Red bg=Background".to_owned()]
        );
    }

    #[test]
    fn a_run_that_reaches_the_last_column_is_still_emitted() {
        let state = state_of(1, 4, |builder| {
            let mut cells = run("abcd");
            for cell in &mut cells {
                cell.attrs = ATTR_REVERSE;
            }
            builder.push_row(0, 0, &cells);
        });
        assert_eq!(
            style_runs(&state),
            vec!["row 0: cols 0-3 fg=Foreground bg=Background flags=INVERSE".to_owned()]
        );
    }

    #[test]
    fn flags_are_rendered_in_a_stable_order() {
        let cell = WireCell {
            attrs: ATTR_ITALIC | ATTR_BOLD | (UNDERLINE_STRAIGHT << 8),
            ..WireCell::BLANK
        };
        assert_eq!(flags(cell, Occupancy::Single), "BOLD|ITALIC|UNDERLINE");
        assert_eq!(flags(WireCell::BLANK, Occupancy::Single), "");
    }

    #[test]
    fn blink_is_appended_after_the_cell_flags() {
        let slow = WireCell {
            attrs: ATTR_BOLD | ATTR_SLOW_BLINK,
            ..WireCell::BLANK
        };
        let both = WireCell {
            attrs: ATTR_SLOW_BLINK | ATTR_FAST_BLINK,
            ..WireCell::BLANK
        };
        assert_eq!(flags(slow, Occupancy::Single), "BOLD|BLINK_SLOW");
        assert_eq!(flags(both, Occupancy::Single), "BLINK_SLOW|BLINK_FAST");
    }

    #[test]
    fn a_wide_character_is_reported_with_its_spacer() {
        let state = state_of(1, 4, |builder| {
            builder.push_row(
                0,
                0,
                &[
                    WireCell {
                        ch: '\u{4f60}' as u32,
                        width: 2,
                        ..WireCell::BLANK
                    },
                    WireCell::BLANK,
                ],
            );
        });
        let dumped = state.dump();
        assert!(dumped.contains("WIDE_CHAR"), "{}", dumped);
        assert!(dumped.contains("WIDE_CHAR_SPACER"), "{}", dumped);
    }

    #[test]
    fn colors_are_rendered_by_kind() {
        assert_eq!(
            color(WireColor::Indexed(208).pack(), "Foreground"),
            "idx208"
        );
        assert_eq!(
            color(WireColor::Rgb(1, 2, 3).pack(), "Foreground"),
            "#010203"
        );
        assert_eq!(
            color(WireColor::Named(14).pack(), "Foreground"),
            "BrightCyan"
        );
        assert_eq!(color(WireColor::Default.pack(), "Background"), "Background");
    }

    #[test]
    fn a_grid_with_no_graphics_reports_no_image_lines() {
        let state = state_of(2, 4, |builder| builder.push_row(0, 0, &run("ab")));
        assert!(image_lines(&state).is_empty());
    }

    #[test]
    fn a_dump_of_identical_frames_is_identical() {
        let paint = |builder: &mut FrameBuilder| {
            builder.push_row(0, 0, &run("green"));
            builder.push_row(1, 0, &run("plain"));
        };
        assert_eq!(state_of(4, 12, paint).dump(), state_of(4, 12, paint).dump());
    }
}

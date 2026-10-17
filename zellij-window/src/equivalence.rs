use std::collections::BTreeSet;

use zellij_server::output::RenderPayload;
use zellij_utils::structured_render::{
    decode, WireColor, ATTR_BOLD, ATTR_DIM, ATTR_FAST_BLINK, ATTR_HIDDEN, ATTR_ITALIC,
    ATTR_REVERSE, ATTR_SLOW_BLINK, ATTR_STRIKE, UNDERLINE_CURLY, UNDERLINE_DASHED,
    UNDERLINE_DOTTED, UNDERLINE_DOUBLE, UNDERLINE_STRAIGHT,
};

use crate::projection::{Cell, Cursor, Occupancy, Projection, DEFAULT_COLOR};
use crate::reference::Emitter;
use crate::screen_buffer::{Occupancy as BufferOccupancy, ScreenBuffer};

const NAMED_COLORS: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "brightblack",
    "brightred",
    "brightgreen",
    "brightyellow",
    "brightblue",
    "brightmagenta",
    "brightcyan",
    "brightwhite",
];

pub fn wire_color(packed: u32) -> String {
    match WireColor::unpack(packed) {
        WireColor::Default => DEFAULT_COLOR.to_owned(),
        WireColor::Named(index) => NAMED_COLORS
            .get(index as usize)
            .map(|name| (*name).to_owned())
            .unwrap_or_else(|| format!("named{}", index)),
        WireColor::Indexed(index) => format!("idx{}", index),
        WireColor::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
    }
}

pub fn project_screen_buffer(buffer: &ScreenBuffer) -> Projection {
    let size = buffer.size();
    let mut cells = Vec::with_capacity(size.rows);
    for row in 0..size.rows {
        let mut projected = Vec::with_capacity(size.cols);
        for col in 0..size.cols {
            let cell = buffer.cell(row, col);
            let occupancy = match buffer.occupancy(row, col) {
                BufferOccupancy::WideHead => Occupancy::WideHead,
                BufferOccupancy::WideTail => Occupancy::WideTail,
                BufferOccupancy::Single => Occupancy::Single,
            };
            let mut attrs = BTreeSet::new();
            for (bit, name) in [
                (ATTR_BOLD, "bold"),
                (ATTR_DIM, "dim"),
                (ATTR_ITALIC, "italic"),
                (ATTR_REVERSE, "inverse"),
                (ATTR_HIDDEN, "hidden"),
                (ATTR_STRIKE, "strike"),
                (ATTR_SLOW_BLINK, "blink-slow"),
                (ATTR_FAST_BLINK, "blink-fast"),
            ] {
                if cell.has(bit) {
                    attrs.insert(name);
                }
            }
            let underline = match cell.underline_style() {
                UNDERLINE_STRAIGHT => Some("underline"),
                UNDERLINE_DOUBLE => Some("double-underline"),
                UNDERLINE_CURLY => Some("undercurl"),
                UNDERLINE_DOTTED => Some("dotted-underline"),
                UNDERLINE_DASHED => Some("dashed-underline"),
                _ => None,
            };
            if let Some(underline) = underline {
                attrs.insert(underline);
            }
            let underline_color = match WireColor::unpack(cell.underline_color) {
                WireColor::Default => None,
                _ => Some(wire_color(cell.underline_color)),
            };
            projected.push(Cell {
                text: buffer.text_at(row, col),
                occupancy,
                fg: wire_color(cell.fg),
                bg: wire_color(cell.bg),
                underline: underline_color,
                attrs,
                link: buffer.link_at(row, col).map(str::to_owned),
            });
        }
        cells.push(projected);
    }

    let (row, col) = buffer.cursor_position();
    Projection {
        rows: size.rows,
        cols: size.cols,
        cursor: Cursor {
            position: Some((col, row)),
            visible: Some(buffer.cursor_is_visible()),
        },
        cells,
        wrapped_rows: Vec::new(),
    }
}

pub struct Dialects {
    pub ansi: Vec<String>,
    pub frames: Vec<Vec<u8>>,
}

pub fn dialects_of(bytes: &[u8], rows: usize, cols: usize, chunk: usize) -> Dialects {
    let mut emitter = Emitter::new(rows, cols);
    let mut dialects = Dialects {
        ansi: Vec::new(),
        frames: Vec::new(),
    };
    let take = |emitter: &mut Emitter, dialects: &mut Dialects| {
        if let Some((ansi, frame)) = emitter.dialects() {
            match (ansi, frame) {
                (RenderPayload::Ansi(ansi), RenderPayload::Frame(frame)) => {
                    dialects.ansi.push(ansi);
                    dialects.frames.push(frame);
                },
                (ansi, frame) => panic!("the dialects came out swapped: {:?} {:?}", ansi, frame),
            }
        }
    };
    for piece in bytes.chunks(chunk.max(1)) {
        emitter.advance(piece);
        take(&mut emitter, &mut dialects);
    }
    take(&mut emitter, &mut dialects);
    dialects
}

pub fn grid_and_wire(
    bytes: &[u8],
    rows: usize,
    cols: usize,
    chunk: usize,
) -> (Projection, Projection) {
    let (grid, buffer) = grid_and_buffer(bytes, rows, cols, chunk);
    (grid, project_screen_buffer(&buffer))
}

pub fn grid_and_buffer(
    bytes: &[u8],
    rows: usize,
    cols: usize,
    chunk: usize,
) -> (Projection, ScreenBuffer) {
    let mut emitter = Emitter::new(rows, cols);
    let mut buffer = ScreenBuffer::new(rows, cols);
    let drain = |emitter: &mut Emitter, buffer: &mut ScreenBuffer| {
        while let Some((_, frame)) = emitter.dialects() {
            if let RenderPayload::Frame(frame) = frame {
                let view =
                    decode(&frame).expect("the server emitted a frame its own decoder rejects");
                buffer
                    .apply(&view)
                    .expect("the server emitted a frame the screen buffer rejects");
            }
        }
    };
    for piece in bytes.chunks(chunk.max(1)) {
        emitter.advance(piece);
        drain(&mut emitter, &mut buffer);
    }
    drain(&mut emitter, &mut buffer);
    (emitter.project(), buffer)
}

pub fn buffer_of(frames: &[Vec<u8>], rows: usize, cols: usize) -> ScreenBuffer {
    let mut buffer = ScreenBuffer::new(rows, cols);
    for frame in frames {
        let view = decode(frame).expect("the server emitted a frame its own decoder rejects");
        buffer
            .apply(&view)
            .expect("the server emitted a frame the screen buffer rejects");
    }
    buffer
}

pub fn state_of(frames: &[Vec<u8>], rows: usize, cols: usize) -> crate::terminal::TerminalState {
    let mut state = crate::terminal::TerminalState::new(rows, cols);
    state.set_cell_size(
        crate::reference::CELL_WIDTH as u32,
        crate::reference::CELL_HEIGHT as u32,
    );
    for frame in frames {
        state
            .apply_frame(frame)
            .expect("the server emitted a frame the window rejects");
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::differential::{reference_of, stream_corpus, STREAM_COLS, STREAM_ROWS};
    use crate::projection::compare;
    use std::fs;

    const EMIT_CHUNK: usize = 4096;

    fn divergences_for(bytes: &[u8]) -> Vec<String> {
        if crate::differential::uses_grapheme_mode(bytes) {
            let (grid, wire) = grid_and_wire(bytes, STREAM_ROWS, STREAM_COLS, EMIT_CHUNK);
            return compare(&grid, &wire)
                .into_iter()
                .map(|divergence| divergence.describe())
                .collect();
        }
        let dialects = dialects_of(bytes, STREAM_ROWS, STREAM_COLS, EMIT_CHUNK);
        let through_ansi = reference_of(&dialects.ansi, STREAM_ROWS, STREAM_COLS).project();
        let through_the_wire =
            project_screen_buffer(&buffer_of(&dialects.frames, STREAM_ROWS, STREAM_COLS));
        compare(&through_ansi, &through_the_wire)
            .into_iter()
            .map(|divergence| divergence.describe())
            .collect()
    }

    fn assert_agrees(bytes: &[u8]) {
        let divergences = divergences_for(bytes);
        assert!(divergences.is_empty(), "{}", divergences.join("\n"));
    }

    #[test]
    fn plain_text_agrees_across_the_dialects() {
        assert_agrees(b"hello, dialects");
    }

    #[test]
    fn every_color_form_agrees_across_the_dialects() {
        assert_agrees(
            b"\x1b[31ma\x1b[38;5;208mb\x1b[38;2;1;2;3mc\x1b[39m\
              \x1b[41md\x1b[48;5;99me\x1b[48;2;9;8;7mf\x1b[49mg\x1b[91mh\x1b[107mi",
        );
    }

    #[test]
    fn every_attribute_agrees_across_the_dialects() {
        assert_agrees(b"\x1b[1ma\x1b[2mb\x1b[3mc\x1b[4md\x1b[7me\x1b[8mf\x1b[9mg\x1b[5mh\x1b[6mi");
    }

    #[test]
    fn every_styled_underline_agrees_across_the_dialects() {
        assert_agrees(
            b"\x1b[4:1ma\x1b[4:2mb\x1b[4:3mc\x1b[4:4md\x1b[4:5me\
              \x1b[58:2::1:2:3mf\x1b[58:5:208mg\x1b[59mh",
        );
    }

    #[test]
    fn wide_characters_agree_on_both_of_their_columns() {
        assert_agrees("\u{4f60}\u{597d}x\u{1b}[3;5H\u{4e16}".as_bytes());
    }

    #[test]
    fn a_scroll_region_agrees_across_the_dialects() {
        assert_agrees(b"\x1b[Haaaa\r\nbbbb\r\ncccc\r\ndddd\x1b[2;4r\x1b[4;1H\n\x1b[reeee");
    }

    #[test]
    fn a_hyperlink_agrees_across_the_dialects() {
        assert_agrees(b"\x1b]8;id=1;https://example.com/one\x1b\\linked\x1b]8;;\x1b\\ plain");
    }

    #[test]
    fn a_hyperlink_split_by_a_line_wrap_agrees_across_the_dialects() {
        let mut stream = vec![b' '; STREAM_COLS - 3];
        stream.extend_from_slice(
            b"\x1b]8;id=1;https://example.com/wrapped\x1b\\linked\x1b]8;;\x1b\\",
        );
        assert_agrees(&stream);
    }

    #[test]
    fn a_wide_character_inside_a_hyperlink_carries_it_on_both_columns() {
        assert_agrees(
            "\u{1b}]8;id=1;https://example.com/wide\u{1b}\\\u{4f60}\u{1b}]8;;\u{1b}\\x".as_bytes(),
        );
    }

    #[test]
    fn combining_marks_agree_across_the_dialects() {
        assert_agrees("e\u{301} a\u{308}\u{30a} x\u{ad}y \u{ff76}\u{ff9e}".as_bytes());
    }

    #[test]
    fn emoji_sequences_agree_across_the_dialects_in_default_mode() {
        assert_agrees(
            "\u{26a0}\u{fe0f}|\u{1f44d}\u{1f3fd}|\u{1f1e9}\u{1f1ea}|\
             \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}|\u{1f3f3}\u{fe0f}\u{200d}\u{1f308}|\
             1\u{fe0f}\u{20e3}|"
                .as_bytes(),
        );
    }

    #[test]
    fn indic_thai_and_hangul_clusters_agree_across_the_dialects() {
        assert_agrees(
            "\u{915}\u{94d}\u{937}|\u{915}\u{93e}|\u{e01}\u{e49}\u{e33}|\u{1100}\u{1161}\u{11a8}|"
                .as_bytes(),
        );
    }

    #[test]
    fn a_styled_group_agrees_across_the_dialects() {
        assert_agrees(
            "\u{1b}[31m\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\u{1b}[m \u{1b}[32m\u{1f44d}\u{1b}[33m\u{1f3fd}\u{1b}[m"
                .as_bytes(),
        );
    }

    #[test]
    fn mode_2027_clusters_reach_the_wire_as_the_grid_holds_them() {
        let stream = "\u{1b}[?2027h\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}|\u{26a0}\u{fe0f}|\
                      \u{915}\u{94d}\u{937}|\u{231a}\u{fe0e}|\u{1f1e9}\u{1f1ea}\u{1f1eb}|";
        let (grid, wire) = grid_and_wire(stream.as_bytes(), STREAM_ROWS, STREAM_COLS, EMIT_CHUNK);
        let divergences: Vec<String> = compare(&grid, &wire)
            .into_iter()
            .map(|divergence| divergence.describe())
            .collect();
        assert!(divergences.is_empty(), "{}", divergences.join("\n"));
        assert_eq!(
            wire.cells[0][0].text,
            "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}"
        );
        assert_eq!(wire.cells[0][0].occupancy, Occupancy::WideHead);
        assert_eq!(wire.cells[0][1].occupancy, Occupancy::WideTail);
        assert_eq!(wire.cells[0][2].text, "|");
    }

    #[test]
    fn a_palette_change_is_substituted_into_the_wire_cells() {
        assert_agrees(b"\x1b]4;3;rgb:1234/5678/9abc\x1b\\\x1b[33mpalette\x1b[m plain");
    }

    #[test]
    fn every_application_stream_agrees_across_the_dialects() {
        let mut failures = Vec::new();
        for path in stream_corpus().expect("a readable stream corpus") {
            let bytes = fs::read(&path).expect("a readable stream");
            let divergences = divergences_for(&bytes);
            if !divergences.is_empty() {
                let described: Vec<String> = divergences.into_iter().take(12).collect();
                failures.push(format!(
                    "{}:\n{}",
                    path.file_name().unwrap().to_str().unwrap(),
                    described.join("\n")
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }
}

use std::collections::HashMap;

use zellij_utils::structured_render::{
    CursorState, FrameView, WireCell, ATTR_FAST_BLINK, ATTR_SLOW_BLINK, CURSOR_SHAPE_BEAM,
    CURSOR_SHAPE_DEFAULT, CURSOR_SHAPE_UNDERLINE, LINK_NONE,
};

use crate::sixel::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermSize {
    pub rows: usize,
    pub cols: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Blink {
    pub slow: bool,
    pub fast: bool,
}

impl Blink {
    pub fn is_set(&self) -> bool {
        self.slow || self.fast
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Underline,
    Beam,
    Default,
}

impl CursorShape {
    pub fn or(self, configured: Option<CursorShape>) -> CursorShape {
        match self {
            CursorShape::Default => configured.unwrap_or(CursorShape::Block),
            asked_for => asked_for,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occupancy {
    Single,
    WideHead,
    WideTail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyError {
    ReshapeWithoutFullRepaint {
        expected: (usize, usize),
        found: (usize, usize),
    },
    DegenerateViewport {
        found: (usize, usize),
    },
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::ReshapeWithoutFullRepaint { expected, found } => write!(
                f,
                "a partial frame of {}x{} arrived for a {}x{} screen",
                found.0, found.1, expected.0, expected.1
            ),
            ApplyError::DegenerateViewport { found } => write!(
                f,
                "a frame claimed a {}x{} viewport, which has no cells",
                found.0, found.1
            ),
        }
    }
}

impl std::error::Error for ApplyError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub seq: u64,
    pub cleared: bool,
    pub reshaped: bool,
    pub written: Vec<Span>,
    pub cursor_rows: Option<(usize, usize)>,
}

pub struct ScreenBuffer {
    cols: usize,
    rows: usize,
    cells: Vec<WireCell>,
    cursor: CursorState,
    blinking: usize,
    blinking_rows: Vec<usize>,
    links: HashMap<u8, String>,
}

impl ScreenBuffer {
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        ScreenBuffer {
            cols,
            rows,
            cells: vec![WireCell::BLANK; cols * rows],
            cursor: CursorState::default(),
            blinking: 0,
            blinking_rows: vec![0; rows],
            links: HashMap::new(),
        }
    }

    pub fn size(&self) -> TermSize {
        TermSize {
            rows: self.rows,
            cols: self.cols,
        }
    }

    pub fn cell(&self, row: usize, col: usize) -> WireCell {
        if row >= self.rows || col >= self.cols {
            return WireCell::BLANK;
        }
        self.cells[row * self.cols + col]
    }

    pub fn cursor_is_visible(&self) -> bool {
        self.cursor.visible
    }

    pub fn cursor_position(&self) -> (usize, usize) {
        (self.cursor.y as usize, self.cursor.x as usize)
    }

    pub fn cursor_shape(&self) -> CursorShape {
        match self.cursor.shape_kind() {
            CURSOR_SHAPE_UNDERLINE => CursorShape::Underline,
            CURSOR_SHAPE_BEAM => CursorShape::Beam,
            CURSOR_SHAPE_DEFAULT => CursorShape::Default,
            _ => CursorShape::Block,
        }
    }

    pub fn cursor_is_blinking(&self) -> bool {
        self.cursor.blinking()
    }

    pub fn occupancy(&self, row: usize, col: usize) -> Occupancy {
        if self.cell(row, col).is_wide_head() {
            return Occupancy::WideHead;
        }
        if col > 0 && self.cell(row, col - 1).is_wide_head() {
            return Occupancy::WideTail;
        }
        Occupancy::Single
    }

    pub fn blink_at(&self, row: usize, col: usize) -> Blink {
        let cell = self.cell(row, col);
        Blink {
            slow: cell.has(ATTR_SLOW_BLINK),
            fast: cell.has(ATTR_FAST_BLINK),
        }
    }

    pub fn has_blinking_cells(&self) -> bool {
        self.blinking > 0
    }

    pub fn blinking_rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.blinking_rows
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(row, _)| row)
    }

    pub fn link_at(&self, row: usize, col: usize) -> Option<&str> {
        let id = self.cell(row, col).link;
        if id == LINK_NONE {
            return None;
        }
        self.links.get(&id).map(String::as_str)
    }

    pub fn link_id_at(&self, row: usize, col: usize) -> u8 {
        self.cell(row, col).link
    }

    pub fn apply(&mut self, frame: &FrameView<'_>) -> Result<Applied, ApplyError> {
        let header = frame.header();
        let (cols, rows) = (header.cols as usize, header.rows as usize);
        if cols == 0 || rows == 0 {
            return Err(ApplyError::DegenerateViewport {
                found: (rows, cols),
            });
        }
        let reshaped = cols != self.cols || rows != self.rows;
        if reshaped && !header.full_repaint() {
            return Err(ApplyError::ReshapeWithoutFullRepaint {
                expected: (self.rows, self.cols),
                found: (rows, cols),
            });
        }
        if reshaped {
            self.cols = cols;
            self.rows = rows;
            self.cells = vec![WireCell::BLANK; cols * rows];
            self.blinking_rows = vec![0; rows];
            self.blinking = 0;
        } else if header.full_repaint() || header.clear() {
            self.cells
                .iter_mut()
                .for_each(|cell| *cell = WireCell::BLANK);
            self.blinking_rows.iter_mut().for_each(|count| *count = 0);
            self.blinking = 0;
        }
        if header.full_repaint() {
            self.links.clear();
        }
        for entry in frame.links().entries {
            self.links.insert(entry.id, entry.uri);
        }

        let mut written = Vec::with_capacity(header.row_record_count as usize);
        for index in 0..header.row_record_count as usize {
            if let Some(record) = frame.record(index) {
                let last = record.x0 as usize + record.len as usize;
                written.push(Span::new(
                    record.y as usize,
                    record.x0 as usize,
                    last.saturating_sub(1),
                ));
            }
        }
        frame.apply(&mut self.cells, self.cols, self.rows);
        let before = self.cursor;
        self.cursor = frame.cursor();
        let cursor_rows = (before != self.cursor).then(|| {
            (
                (before.y as usize).min(self.rows.saturating_sub(1)),
                (self.cursor.y as usize).min(self.rows.saturating_sub(1)),
            )
        });
        for index in 0..written.len() {
            self.recount_blinking(written[index].line);
        }

        Ok(Applied {
            seq: header.seq,
            cleared: header.clear() || header.full_repaint(),
            reshaped,
            written,
            cursor_rows,
        })
    }

    fn recount_blinking(&mut self, row: usize) {
        if row >= self.rows {
            return;
        }
        let counted = self.cells[row * self.cols..(row + 1) * self.cols]
            .iter()
            .filter(|cell| cell.has(ATTR_SLOW_BLINK) || cell.has(ATTR_FAST_BLINK))
            .count();
        self.blinking = self.blinking + counted - self.blinking_rows[row];
        self.blinking_rows[row] = counted;
    }
}

#[cfg(test)]
pub mod painter {

    use super::CursorShape;
    use crate::terminal::TerminalState;
    use zellij_utils::structured_render::{
        CursorState, FrameBuilder, GraphicsMedium, GraphicsRecord, LinkEntry, LinkRecord, WireCell,
        CURSOR_SHAPE_BEAM, CURSOR_SHAPE_DEFAULT, CURSOR_SHAPE_UNDERLINE, GRAPHICS_FORMAT_RGBA8,
    };

    pub fn link_record(links: &[(u8, &str)]) -> LinkRecord {
        LinkRecord {
            entries: links
                .iter()
                .map(|(id, uri)| LinkEntry {
                    id: *id,
                    uri: (*uri).to_owned(),
                })
                .collect(),
        }
    }

    pub fn linked_frame(
        rows: usize,
        cols: usize,
        seq: u64,
        full_repaint: bool,
        painted: &[(usize, usize, &str, &[u8])],
        links: &[(u8, &str)],
    ) -> Vec<u8> {
        let mut builder = FrameBuilder::new(cols as u16, rows as u16, seq);
        builder.set_full_repaint(full_repaint);
        for (row, col, text, ids) in painted {
            let cells: Vec<WireCell> = text
                .chars()
                .enumerate()
                .map(|(index, character)| WireCell {
                    ch: character as u32,
                    link: ids.get(index).copied().unwrap_or(0),
                    ..WireCell::BLANK
                })
                .collect();
            builder.push_row(*row as u16, *col as u16, &cells);
        }
        builder.push_links(&link_record(links));
        builder.finish()
    }

    pub fn residency(id: u64, width: u32, height: u32, fill: [u8; 4]) -> GraphicsRecord {
        let pixels = fill.repeat((width * height) as usize);
        GraphicsRecord::Residency {
            id,
            width,
            height,
            format: GRAPHICS_FORMAT_RGBA8,
            medium: GraphicsMedium::Inline,
            byte_len: pixels.len() as u32,
            data: pixels,
        }
    }

    pub fn sixel_chunk(
        cell_y: u32,
        cell_x: u32,
        width: u32,
        height: u32,
        payload: &str,
    ) -> GraphicsRecord {
        GraphicsRecord::SixelChunk {
            cell_x,
            cell_y,
            pixel_x: 0,
            pixel_y: 0,
            pixel_width: width,
            pixel_height: height,
            payload: payload.as_bytes().to_vec(),
        }
    }

    pub struct Place {
        image_id: u64,
        placement_id: u64,
        cell_x: u32,
        cell_y: u32,
        offset_x: u32,
        offset_y: u32,
        source_x: u32,
        source_y: u32,
        source_width: u32,
        source_height: u32,
        z: i32,
    }

    impl Place {
        pub fn of(image_id: u64, width: u32, height: u32) -> Self {
            Self {
                image_id,
                placement_id: 1,
                cell_x: 0,
                cell_y: 0,
                offset_x: 0,
                offset_y: 0,
                source_x: 0,
                source_y: 0,
                source_width: width,
                source_height: height,
                z: 0,
            }
        }

        pub fn at(mut self, cell_y: u32, cell_x: u32) -> Self {
            self.cell_y = cell_y;
            self.cell_x = cell_x;
            self
        }

        pub fn offset(mut self, offset_x: u32, offset_y: u32) -> Self {
            self.offset_x = offset_x;
            self.offset_y = offset_y;
            self
        }

        pub fn cropped(mut self, x: u32, y: u32, width: u32, height: u32) -> Self {
            self.source_x = x;
            self.source_y = y;
            self.source_width = width;
            self.source_height = height;
            self
        }

        pub fn depth(mut self, z: i32) -> Self {
            self.z = z;
            self
        }

        pub fn record(self) -> GraphicsRecord {
            GraphicsRecord::Placement {
                image_id: self.image_id,
                placement_id: self.placement_id,
                cell_x: self.cell_x,
                cell_y: self.cell_y,
                offset_x: self.offset_x,
                offset_y: self.offset_y,
                source_x: self.source_x,
                source_y: self.source_y,
                source_width: self.source_width,
                source_height: self.source_height,
                z: self.z,
            }
        }
    }

    pub struct Painter {
        builder: FrameBuilder,
    }

    impl Painter {
        pub fn state(rows: usize, cols: usize, paint: impl FnOnce(&mut Painter)) -> TerminalState {
            let mut state = TerminalState::new(rows, cols);
            Self::apply(&mut state, paint);
            state
        }

        pub fn apply(state: &mut TerminalState, paint: impl FnOnce(&mut Painter)) {
            Self::applied(state, paint);
        }

        pub fn applied(
            state: &mut TerminalState,
            paint: impl FnOnce(&mut Painter),
        ) -> crate::terminal::FrameApplied {
            let size = state.size();
            let frame = Self::frame(size.rows, size.cols, paint);
            state
                .apply_frame(&frame)
                .expect("the painted frame should apply")
        }

        pub fn frame(rows: usize, cols: usize, paint: impl FnOnce(&mut Painter)) -> Vec<u8> {
            let mut painter = Painter {
                builder: FrameBuilder::new(cols as u16, rows as u16, 0),
            };
            paint(&mut painter);
            painter.builder.finish()
        }

        pub fn apply_marking(
            state: &mut TerminalState,
            retained: &mut crate::retained::RetainedScene,
            paint: impl FnOnce(&mut Painter),
        ) {
            let applied = Self::applied(state, paint);
            retained.mark(&applied.damage);
        }

        pub fn graphics(&mut self, records: &[GraphicsRecord]) {
            self.builder.extend_graphics(records.iter());
        }

        pub fn geometry(&mut self, record: &zellij_utils::structured_render::GeometryRecord) {
            self.builder.push_geometry(record);
        }

        pub fn links(&mut self, links: &[(u8, &str)]) {
            self.builder.push_links(&link_record(links));
        }

        pub fn linked(&mut self, row: usize, col: usize, text: &str, id: u8) {
            self.styled(row, col, text, |cell| cell.link = id);
        }

        pub fn full_repaint(&mut self) {
            self.builder.set_full_repaint(true);
        }

        pub fn text(&mut self, row: usize, col: usize, text: &str) {
            self.styled(row, col, text, |_| {});
        }

        pub fn styled(
            &mut self,
            row: usize,
            col: usize,
            text: &str,
            style: impl Fn(&mut WireCell),
        ) {
            let cells: Vec<WireCell> = text
                .chars()
                .map(|character| {
                    let mut cell = WireCell {
                        ch: character as u32,
                        ..WireCell::BLANK
                    };
                    style(&mut cell);
                    cell
                })
                .collect();
            self.builder.push_row(row as u16, col as u16, &cells);
        }

        pub fn wide(&mut self, row: usize, col: usize, character: char) {
            self.wide_styled(row, col, character, |_| {});
        }

        pub fn wide_styled(
            &mut self,
            row: usize,
            col: usize,
            character: char,
            style: impl Fn(&mut WireCell),
        ) {
            let mut head = WireCell {
                ch: character as u32,
                width: 2,
                ..WireCell::BLANK
            };
            style(&mut head);
            let spacer = WireCell {
                ch: ' ' as u32,
                width: 1,
                ..head
            };
            self.builder
                .push_row(row as u16, col as u16, &[head, spacer]);
        }

        pub fn cursor(&mut self, row: usize, col: usize, shape: CursorShape) {
            self.set_cursor(row, col, shape, false);
        }

        pub fn blinking_cursor(&mut self, row: usize, col: usize, shape: CursorShape) {
            self.set_cursor(row, col, shape, true);
        }

        fn set_cursor(&mut self, row: usize, col: usize, shape: CursorShape, blinking: bool) {
            let shape = match shape {
                CursorShape::Block => zellij_utils::structured_render::CURSOR_SHAPE_BLOCK,
                CursorShape::Underline => CURSOR_SHAPE_UNDERLINE,
                CursorShape::Beam => CURSOR_SHAPE_BEAM,
                CursorShape::Default => CURSOR_SHAPE_DEFAULT,
            };
            self.builder.set_cursor(CursorState {
                x: col as u16,
                y: row as u16,
                shape: if blinking {
                    shape | zellij_utils::structured_render::CURSOR_BLINKING
                } else {
                    shape
                },
                visible: true,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::structured_render::{decode, FrameBuilder, ATTR_BOLD, CURSOR_SHAPE_BLOCK};

    fn run(text: &str) -> Vec<WireCell> {
        text.chars()
            .map(|character| WireCell {
                ch: character as u32,
                ..WireCell::BLANK
            })
            .collect()
    }

    fn row_text(buffer: &ScreenBuffer, row: usize) -> String {
        (0..buffer.size().cols)
            .map(|col| buffer.cell(row, col).character())
            .collect()
    }

    fn apply(buffer: &mut ScreenBuffer, frame: &[u8]) -> Result<Applied, ApplyError> {
        let view = decode(frame).expect("the frame should decode");
        buffer.apply(&view)
    }

    #[test]
    fn a_new_buffer_is_blank_with_a_hidden_cursor() {
        let buffer = ScreenBuffer::new(2, 4);
        assert_eq!(buffer.size(), TermSize { rows: 2, cols: 4 });
        assert_eq!(row_text(&buffer, 0), "    ");
        assert!(!buffer.cursor_is_visible());
        assert!(!buffer.has_blinking_cells());
    }

    #[test]
    fn a_degenerate_size_is_clamped_rather_than_producing_an_empty_grid() {
        let buffer = ScreenBuffer::new(0, 0);
        assert_eq!(buffer.size(), TermSize { rows: 1, cols: 1 });
    }

    #[test]
    fn row_records_land_at_the_columns_they_name_and_report_their_spans() {
        let mut buffer = ScreenBuffer::new(2, 6);
        let mut builder = FrameBuilder::new(6, 2, 3);
        builder.push_row(0, 1, &run("abc"));
        builder.push_row(1, 4, &run("zz"));
        let applied = apply(&mut buffer, &builder.finish()).unwrap();

        assert_eq!(applied.seq, 3);
        assert!(!applied.cleared);
        assert_eq!(row_text(&buffer, 0), " abc  ");
        assert_eq!(row_text(&buffer, 1), "    zz");
        assert_eq!(
            applied.written,
            vec![Span::new(0, 1, 3), Span::new(1, 4, 5)]
        );
    }

    #[test]
    fn a_partial_frame_leaves_the_cells_it_does_not_name_alone() {
        let mut buffer = ScreenBuffer::new(1, 6);
        let mut first = FrameBuilder::new(6, 1, 0);
        first.push_row(0, 0, &run("abcdef"));
        apply(&mut buffer, &first.finish()).unwrap();

        let mut second = FrameBuilder::new(6, 1, 1);
        second.push_row(0, 2, &run("XY"));
        apply(&mut buffer, &second.finish()).unwrap();
        assert_eq!(row_text(&buffer, 0), "abXYef");
    }

    #[test]
    fn a_full_repaint_clears_before_it_paints() {
        let mut buffer = ScreenBuffer::new(1, 6);
        let mut first = FrameBuilder::new(6, 1, 0);
        first.push_row(0, 0, &run("abcdef"));
        apply(&mut buffer, &first.finish()).unwrap();

        let mut second = FrameBuilder::new(6, 1, 1);
        second.set_full_repaint(true);
        second.push_row(0, 0, &run("gh"));
        let applied = apply(&mut buffer, &second.finish()).unwrap();
        assert!(applied.cleared);
        assert_eq!(row_text(&buffer, 0), "gh    ");
    }

    #[test]
    fn a_clear_without_a_full_repaint_blanks_every_cell_it_does_not_repaint() {
        let mut buffer = ScreenBuffer::new(2, 6);
        let mut first = FrameBuilder::new(6, 2, 0);
        first.push_row(0, 0, &run("abcdef"));
        first.push_row(1, 0, &run("ghijkl"));
        apply(&mut buffer, &first.finish()).unwrap();

        let mut second = FrameBuilder::new(6, 2, 1);
        second.set_clear(true);
        second.push_row(0, 0, &run("ab"));
        let applied = apply(&mut buffer, &second.finish()).unwrap();
        assert!(applied.cleared);
        assert!(!applied.reshaped);
        assert_eq!(row_text(&buffer, 0), "ab    ");
        assert_eq!(row_text(&buffer, 1), "      ");
    }

    #[test]
    fn a_full_repaint_may_reshape_the_screen() {
        let mut buffer = ScreenBuffer::new(1, 6);
        let mut builder = FrameBuilder::new(3, 2, 0);
        builder.set_full_repaint(true);
        builder.push_row(1, 0, &run("xyz"));
        let applied = apply(&mut buffer, &builder.finish()).unwrap();

        assert!(applied.reshaped);
        assert_eq!(buffer.size(), TermSize { rows: 2, cols: 3 });
        assert_eq!(row_text(&buffer, 0), "   ");
        assert_eq!(row_text(&buffer, 1), "xyz");
    }

    #[test]
    fn a_partial_frame_of_the_wrong_shape_is_a_protocol_error() {
        let mut buffer = ScreenBuffer::new(1, 6);
        let mut builder = FrameBuilder::new(3, 2, 0);
        builder.push_row(0, 0, &run("xyz"));
        assert_eq!(
            apply(&mut buffer, &builder.finish()),
            Err(ApplyError::ReshapeWithoutFullRepaint {
                expected: (1, 6),
                found: (2, 3)
            })
        );
        assert_eq!(buffer.size(), TermSize { rows: 1, cols: 6 });
    }

    #[test]
    fn a_frame_with_no_cells_at_all_is_refused_rather_than_emptying_the_buffer() {
        let mut buffer = ScreenBuffer::new(2, 4);
        let mut builder = FrameBuilder::new(0, 0, 0);
        builder.set_full_repaint(true);
        assert_eq!(
            apply(&mut buffer, &builder.finish()),
            Err(ApplyError::DegenerateViewport { found: (0, 0) })
        );
        assert_eq!(buffer.size(), TermSize { rows: 2, cols: 4 });
    }

    #[test]
    fn the_cursor_comes_from_the_frame_header() {
        let mut buffer = ScreenBuffer::new(4, 8);
        let mut builder = FrameBuilder::new(8, 4, 0);
        builder.set_cursor(CursorState {
            x: 5,
            y: 2,
            shape: CURSOR_SHAPE_BEAM | zellij_utils::structured_render::CURSOR_BLINKING,
            visible: true,
        });
        builder.push_row(0, 0, &run("a"));
        apply(&mut buffer, &builder.finish()).unwrap();

        assert!(buffer.cursor_is_visible());
        assert_eq!(buffer.cursor_position(), (2, 5));
        assert_eq!(buffer.cursor_shape(), CursorShape::Beam);
        assert!(buffer.cursor_is_blinking());
    }

    #[test]
    fn cursor_shapes_map_to_their_wire_encoding() {
        for (encoded, expected) in [
            (CURSOR_SHAPE_BLOCK, CursorShape::Block),
            (CURSOR_SHAPE_UNDERLINE, CursorShape::Underline),
            (CURSOR_SHAPE_BEAM, CursorShape::Beam),
        ] {
            let mut buffer = ScreenBuffer::new(1, 2);
            let mut builder = FrameBuilder::new(2, 1, 0);
            builder.set_cursor(CursorState {
                x: 0,
                y: 0,
                shape: encoded,
                visible: true,
            });
            builder.push_row(0, 0, &run("a"));
            apply(&mut buffer, &builder.finish()).unwrap();
            assert_eq!(buffer.cursor_shape(), expected);
            assert!(!buffer.cursor_is_blinking());
        }
    }

    #[test]
    fn a_wide_head_is_followed_by_a_tail_the_buffer_recognizes() {
        let mut buffer = ScreenBuffer::new(1, 4);
        let mut builder = FrameBuilder::new(4, 1, 0);
        let wide = WireCell {
            ch: '\u{4f60}' as u32,
            width: 2,
            ..WireCell::BLANK
        };
        builder.push_row(0, 0, &[wide, WireCell::BLANK, WireCell::BLANK]);
        apply(&mut buffer, &builder.finish()).unwrap();

        assert_eq!(buffer.occupancy(0, 0), Occupancy::WideHead);
        assert_eq!(buffer.occupancy(0, 1), Occupancy::WideTail);
        assert_eq!(buffer.occupancy(0, 2), Occupancy::Single);
    }

    #[test]
    fn blink_is_read_off_the_cells_rather_than_tracked_alongside_them() {
        let mut buffer = ScreenBuffer::new(1, 4);
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_row(
            0,
            0,
            &[
                WireCell {
                    ch: 'a' as u32,
                    attrs: ATTR_SLOW_BLINK | ATTR_BOLD,
                    ..WireCell::BLANK
                },
                WireCell {
                    ch: 'b' as u32,
                    attrs: ATTR_FAST_BLINK,
                    ..WireCell::BLANK
                },
                WireCell {
                    ch: 'c' as u32,
                    ..WireCell::BLANK
                },
            ],
        );
        apply(&mut buffer, &builder.finish()).unwrap();

        assert_eq!(
            buffer.blink_at(0, 0),
            Blink {
                slow: true,
                fast: false
            }
        );
        assert_eq!(
            buffer.blink_at(0, 1),
            Blink {
                slow: false,
                fast: true
            }
        );
        assert_eq!(buffer.blink_at(0, 2), Blink::default());
        assert!(buffer.has_blinking_cells());
    }

    #[test]
    fn a_repaint_that_removes_the_last_blinking_cell_stops_the_blink_timer() {
        let mut buffer = ScreenBuffer::new(1, 4);
        let mut first = FrameBuilder::new(4, 1, 0);
        first.push_row(
            0,
            0,
            &[WireCell {
                ch: 'a' as u32,
                attrs: ATTR_SLOW_BLINK,
                ..WireCell::BLANK
            }],
        );
        apply(&mut buffer, &first.finish()).unwrap();
        assert!(buffer.has_blinking_cells());

        let mut second = FrameBuilder::new(4, 1, 1);
        second.push_row(0, 0, &run("a"));
        apply(&mut buffer, &second.finish()).unwrap();
        assert!(!buffer.has_blinking_cells());
    }

    #[test]
    fn reads_outside_the_grid_answer_blank_rather_than_panicking() {
        let buffer = ScreenBuffer::new(2, 2);
        assert_eq!(buffer.cell(9, 9), WireCell::BLANK);
        assert_eq!(buffer.occupancy(9, 9), Occupancy::Single);
        assert_eq!(buffer.blink_at(9, 9), Blink::default());
    }

    #[test]
    fn a_cell_resolves_its_url_through_the_table_the_frame_carried() {
        let mut buffer = ScreenBuffer::new(1, 6);
        let frame = painter::linked_frame(
            1,
            6,
            0,
            true,
            &[(0, 0, "ab cd ", &[3, 3, 0, 0, 0, 0])],
            &[(3, "https://example.com/three")],
        );
        apply(&mut buffer, &frame).unwrap();
        assert_eq!(buffer.link_at(0, 0), Some("https://example.com/three"));
        assert_eq!(buffer.link_at(0, 2), None);
        assert_eq!(buffer.link_id_at(0, 1), 3);
    }

    #[test]
    fn a_delta_row_resolves_against_the_table_the_buffer_already_holds() {
        let mut buffer = ScreenBuffer::new(2, 6);
        let first = painter::linked_frame(
            2,
            6,
            0,
            true,
            &[(0, 0, "ab    ", &[4, 4, 0, 0, 0, 0])],
            &[(4, "https://example.com/four")],
        );
        apply(&mut buffer, &first).unwrap();

        let delta = painter::linked_frame(2, 6, 1, false, &[(1, 0, "cd", &[4, 4])], &[]);
        apply(&mut buffer, &delta).unwrap();
        assert_eq!(buffer.link_at(1, 0), Some("https://example.com/four"));
        assert_eq!(buffer.link_at(0, 0), Some("https://example.com/four"));
    }

    #[test]
    fn a_full_repaint_drops_the_table_the_buffer_was_holding() {
        let mut buffer = ScreenBuffer::new(1, 6);
        let first = painter::linked_frame(
            1,
            6,
            0,
            true,
            &[(0, 0, "ab    ", &[5, 5, 0, 0, 0, 0])],
            &[
                (5, "https://example.com/before"),
                (6, "https://example.com/x"),
            ],
        );
        apply(&mut buffer, &first).unwrap();

        let second = painter::linked_frame(
            1,
            6,
            1,
            true,
            &[(0, 0, "cdef  ", &[5, 5, 6, 6, 0, 0])],
            &[(5, "https://example.com/after")],
        );
        apply(&mut buffer, &second).unwrap();
        assert_eq!(buffer.link_at(0, 0), Some("https://example.com/after"));
        assert_eq!(buffer.link_id_at(0, 2), 6);
        assert_eq!(buffer.link_at(0, 2), None);
    }

    #[test]
    fn a_reshape_leaves_no_link_behind() {
        let mut buffer = ScreenBuffer::new(1, 6);
        let first = painter::linked_frame(
            1,
            6,
            0,
            true,
            &[(0, 0, "ab    ", &[5, 5, 0, 0, 0, 0])],
            &[(5, "https://example.com/before")],
        );
        apply(&mut buffer, &first).unwrap();

        let reshaped = painter::linked_frame(2, 4, 1, true, &[(0, 0, "abcd", &[5, 5, 0, 0])], &[]);
        apply(&mut buffer, &reshaped).unwrap();
        assert_eq!(buffer.link_id_at(0, 0), 5);
        assert_eq!(buffer.link_at(0, 0), None);
    }
}

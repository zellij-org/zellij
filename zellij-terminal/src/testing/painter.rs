use crate::emulation::screen_buffer::CursorShape;
use crate::emulation::terminal::TerminalState;
use zellij_utils::structured_render::{
    CursorState, FrameBuilder, GraphicsMedium, GraphicsRecord, LinkEntry, LinkRecord, WireCell,
    ATTR_GROUP_CONTINUE, CURSOR_SHAPE_BEAM, CURSOR_SHAPE_DEFAULT, CURSOR_SHAPE_UNDERLINE,
    GRAPHICS_FORMAT_RGBA8,
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
    ) -> crate::emulation::terminal::FrameApplied {
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
        retained: &mut crate::render::retained::RetainedScene,
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

    pub fn styled(&mut self, row: usize, col: usize, text: &str, style: impl Fn(&mut WireCell)) {
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

    pub fn cluster(&mut self, row: usize, col: usize, text: &str, width: usize) {
        self.cluster_styled(row, col, text, width, |_| {});
    }

    pub fn cluster_styled(
        &mut self,
        row: usize,
        col: usize,
        text: &str,
        width: usize,
        style: impl Fn(&mut WireCell),
    ) {
        let cells = self.unit_cells(&[(text, width)], &style);
        self.builder.push_row(row as u16, col as u16, &cells);
    }

    pub fn group(&mut self, row: usize, col: usize, members: &[(&str, usize)]) {
        self.group_styled(row, col, members, |_| {});
    }

    pub fn group_styled(
        &mut self,
        row: usize,
        col: usize,
        members: &[(&str, usize)],
        style: impl Fn(&mut WireCell),
    ) {
        let cells = self.unit_cells(members, &style);
        self.builder.push_row(row as u16, col as u16, &cells);
    }

    fn unit_cells(
        &mut self,
        members: &[(&str, usize)],
        style: &impl Fn(&mut WireCell),
    ) -> Vec<WireCell> {
        let mut cells = Vec::new();
        for (index, (text, width)) in members.iter().enumerate() {
            let mut characters = text.chars();
            let ch = match (characters.next(), characters.next()) {
                (Some(only), None) => only as u32,
                (Some(first), Some(_)) => self.builder.cluster(text).unwrap_or(first as u32),
                _ => ' ' as u32,
            };
            let mut head = WireCell {
                ch,
                width: (*width).max(1) as u8,
                ..WireCell::BLANK
            };
            style(&mut head);
            if index > 0 {
                head.attrs |= ATTR_GROUP_CONTINUE;
            }
            cells.push(head);
            for _ in 1..(*width).max(1) {
                cells.push(WireCell {
                    ch: ' ' as u32,
                    width: 1,
                    attrs: head.attrs & !ATTR_GROUP_CONTINUE,
                    ..head
                });
            }
        }
        cells
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

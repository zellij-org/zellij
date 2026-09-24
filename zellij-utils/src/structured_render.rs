use std::fmt::{self, Display, Formatter};

const WIRE_VERSION: u8 = 2;

const HEADER_LEN: usize = 48;
const ROW_RECORD_LEN: usize = 8;
const CELL_LEN: usize = 20;
const GRAPHICS_RECORD_HEADER_LEN: usize = 8;

const GRAPHICS_KIND_RESIDENCY: u8 = 0;
const GRAPHICS_KIND_PLACEMENT: u8 = 1;
const GRAPHICS_KIND_DELETE_IMAGE: u8 = 2;
const GRAPHICS_KIND_DELETE_PLACEMENT: u8 = 3;
const GRAPHICS_KIND_CLEAR: u8 = 4;
const GRAPHICS_KIND_SIXEL: u8 = 5;
const RECORD_KIND_GEOMETRY: u8 = 6;
const RECORD_KIND_LINKS: u8 = 7;

const GEOMETRY_HEADER_LEN: usize = 8;
const GEOMETRY_ENTRY_LEN: usize = 16;

const LINKS_HEADER_LEN: usize = 8;
const LINK_ENTRY_HEADER_LEN: usize = 4;

pub const LINK_NONE: u8 = 0;
pub const MAX_LINK_ID: u8 = u8::MAX;
pub const MAX_LINK_URI_LEN: usize = 2048;

const GRAPHICS_MEDIUM_INLINE: u8 = 0;
const GRAPHICS_MEDIUM_SHARED_FILE: u8 = 1;

pub const PANE_FRAMED: u8 = 1 << 0;
pub const PANE_FOCUSED: u8 = 1 << 1;
pub const PANE_WANTS_MOUSE: u8 = 1 << 2;
pub const PANE_SELECTABLE: u8 = 1 << 3;

pub const GRAPHICS_FORMAT_RGBA8: u32 = 0;

const FLAG_FULL_REPAINT: u8 = 1 << 0;
const FLAG_CLEAR: u8 = 1 << 1;

const COLOR_DEFAULT: u32 = 0;
const COLOR_TAG_DEFAULT: u8 = 0;
const COLOR_TAG_NAMED: u8 = 1;
const COLOR_TAG_INDEXED: u8 = 2;
const COLOR_TAG_RGB: u8 = 3;

pub const ATTR_BOLD: u16 = 1 << 0;
pub const ATTR_DIM: u16 = 1 << 1;
pub const ATTR_ITALIC: u16 = 1 << 2;
pub const ATTR_REVERSE: u16 = 1 << 3;
pub const ATTR_HIDDEN: u16 = 1 << 4;
pub const ATTR_STRIKE: u16 = 1 << 5;
pub const ATTR_SLOW_BLINK: u16 = 1 << 6;
pub const ATTR_FAST_BLINK: u16 = 1 << 7;
pub const UNDERLINE_SHIFT: u32 = 8;
const UNDERLINE_MASK: u16 = 0b111 << UNDERLINE_SHIFT;

pub const UNDERLINE_NONE: u16 = 0;
pub const UNDERLINE_STRAIGHT: u16 = 1;
pub const UNDERLINE_DOUBLE: u16 = 2;
pub const UNDERLINE_CURLY: u16 = 3;
pub const UNDERLINE_DOTTED: u16 = 4;
pub const UNDERLINE_DASHED: u16 = 5;

pub const CURSOR_SHAPE_BLOCK: u8 = 0;
pub const CURSOR_SHAPE_UNDERLINE: u8 = 1;
pub const CURSOR_SHAPE_BEAM: u8 = 2;
pub const CURSOR_SHAPE_DEFAULT: u8 = 3;
const CURSOR_SHAPE_MASK: u8 = 0b0111_1111;
pub const CURSOR_BLINKING: u8 = 1 << 7;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireCell {
    pub ch: u32,
    pub fg: u32,
    pub bg: u32,
    pub underline_color: u32,
    pub attrs: u16,
    pub width: u8,
    pub link: u8,
}

const _: [(); CELL_LEN] = [(); std::mem::size_of::<WireCell>()];
const _: [(); 4] = [(); std::mem::align_of::<WireCell>()];

impl Default for WireCell {
    fn default() -> Self {
        Self::BLANK
    }
}

impl WireCell {
    pub const BLANK: WireCell = WireCell {
        ch: ' ' as u32,
        fg: COLOR_DEFAULT,
        bg: COLOR_DEFAULT,
        underline_color: COLOR_DEFAULT,
        attrs: 0,
        width: 1,
        link: LINK_NONE,
    };

    pub fn character(&self) -> char {
        char::from_u32(self.ch).unwrap_or(' ')
    }

    pub fn is_wide_head(&self) -> bool {
        self.width >= 2
    }

    pub fn has(&self, attr: u16) -> bool {
        self.attrs & attr != 0
    }

    pub fn underline_style(&self) -> u16 {
        (self.attrs & UNDERLINE_MASK) >> UNDERLINE_SHIFT
    }

    fn to_bytes(self) -> [u8; CELL_LEN] {
        let mut bytes = [0u8; CELL_LEN];
        bytes[0..4].copy_from_slice(&self.ch.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.fg.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.bg.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.underline_color.to_le_bytes());
        bytes[16..18].copy_from_slice(&self.attrs.to_le_bytes());
        bytes[18] = self.width;
        bytes[19] = self.link;
        bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireColor {
    Default,
    Named(u8),
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl WireColor {
    pub fn pack(self) -> u32 {
        match self {
            WireColor::Default => (COLOR_TAG_DEFAULT as u32) << 24,
            WireColor::Named(index) => ((COLOR_TAG_NAMED as u32) << 24) | index as u32,
            WireColor::Indexed(index) => ((COLOR_TAG_INDEXED as u32) << 24) | index as u32,
            WireColor::Rgb(r, g, b) => {
                ((COLOR_TAG_RGB as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
            },
        }
    }

    pub fn unpack(packed: u32) -> WireColor {
        match (packed >> 24) as u8 {
            COLOR_TAG_NAMED => WireColor::Named(packed as u8),
            COLOR_TAG_INDEXED => WireColor::Indexed(packed as u8),
            COLOR_TAG_RGB => WireColor::Rgb(
                ((packed >> 16) & 0xff) as u8,
                ((packed >> 8) & 0xff) as u8,
                (packed & 0xff) as u8,
            ),
            _ => WireColor::Default,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct RowRecord {
    pub y: u16,
    pub x0: u16,
    pub len: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsMedium {
    Inline,
    SharedFile,
}

impl GraphicsMedium {
    fn encode(self) -> u8 {
        match self {
            GraphicsMedium::Inline => GRAPHICS_MEDIUM_INLINE,
            GraphicsMedium::SharedFile => GRAPHICS_MEDIUM_SHARED_FILE,
        }
    }

    fn decode(medium: u8) -> Option<Self> {
        match medium {
            GRAPHICS_MEDIUM_INLINE => Some(GraphicsMedium::Inline),
            GRAPHICS_MEDIUM_SHARED_FILE => Some(GraphicsMedium::SharedFile),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphicsRecord {
    Residency {
        id: u64,
        width: u32,
        height: u32,
        format: u32,
        medium: GraphicsMedium,
        byte_len: u32,
        data: Vec<u8>,
    },
    Placement {
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
    },
    DeleteImage {
        id: u64,
    },
    DeletePlacement {
        image_id: u64,
        placement_id: u64,
    },
    Clear,
    SixelChunk {
        cell_x: u32,
        cell_y: u32,
        pixel_x: u32,
        pixel_y: u32,
        pixel_width: u32,
        pixel_height: u32,
        payload: Vec<u8>,
    },
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn take_u32(payload: &[u8], at: usize) -> Option<u32> {
    let bytes = payload.get(at..at + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn take_u64(payload: &[u8], at: usize) -> Option<u64> {
    let bytes = payload.get(at..at + 8)?;
    let mut buffer = [0u8; 8];
    buffer.copy_from_slice(bytes);
    Some(u64::from_le_bytes(buffer))
}

impl GraphicsRecord {
    fn kind(&self) -> u8 {
        match self {
            GraphicsRecord::Residency { .. } => GRAPHICS_KIND_RESIDENCY,
            GraphicsRecord::Placement { .. } => GRAPHICS_KIND_PLACEMENT,
            GraphicsRecord::DeleteImage { .. } => GRAPHICS_KIND_DELETE_IMAGE,
            GraphicsRecord::DeletePlacement { .. } => GRAPHICS_KIND_DELETE_PLACEMENT,
            GraphicsRecord::Clear => GRAPHICS_KIND_CLEAR,
            GraphicsRecord::SixelChunk { .. } => GRAPHICS_KIND_SIXEL,
        }
    }

    fn medium(&self) -> u8 {
        match self {
            GraphicsRecord::Residency { medium, .. } => medium.encode(),
            _ => GRAPHICS_MEDIUM_INLINE,
        }
    }

    fn encode_payload(&self, out: &mut Vec<u8>) {
        match self {
            GraphicsRecord::Residency {
                id,
                width,
                height,
                format,
                byte_len,
                data,
                ..
            } => {
                put_u64(out, *id);
                put_u32(out, *width);
                put_u32(out, *height);
                put_u32(out, *format);
                put_u32(out, *byte_len);
                put_u32(out, data.len() as u32);
                put_u32(out, 0);
                out.extend_from_slice(data);
            },
            GraphicsRecord::Placement {
                image_id,
                placement_id,
                cell_x,
                cell_y,
                offset_x,
                offset_y,
                source_x,
                source_y,
                source_width,
                source_height,
                z,
            } => {
                put_u64(out, *image_id);
                put_u64(out, *placement_id);
                for value in [
                    *cell_x,
                    *cell_y,
                    *offset_x,
                    *offset_y,
                    *source_x,
                    *source_y,
                    *source_width,
                    *source_height,
                ] {
                    put_u32(out, value);
                }
                out.extend_from_slice(&z.to_le_bytes());
                put_u32(out, 0);
            },
            GraphicsRecord::DeleteImage { id } => put_u64(out, *id),
            GraphicsRecord::DeletePlacement {
                image_id,
                placement_id,
            } => {
                put_u64(out, *image_id);
                put_u64(out, *placement_id);
            },
            GraphicsRecord::Clear => {},
            GraphicsRecord::SixelChunk {
                cell_x,
                cell_y,
                pixel_x,
                pixel_y,
                pixel_width,
                pixel_height,
                payload,
            } => {
                for value in [
                    *cell_x,
                    *cell_y,
                    *pixel_x,
                    *pixel_y,
                    *pixel_width,
                    *pixel_height,
                    payload.len() as u32,
                    0,
                ] {
                    put_u32(out, value);
                }
                out.extend_from_slice(payload);
            },
        }
    }

    fn decode(kind: u8, medium: u8, payload: &[u8]) -> Option<Self> {
        match kind {
            GRAPHICS_KIND_RESIDENCY => {
                let data_len = take_u32(payload, 24)? as usize;
                let data = payload.get(32..32 + data_len)?.to_vec();
                Some(GraphicsRecord::Residency {
                    id: take_u64(payload, 0)?,
                    width: take_u32(payload, 8)?,
                    height: take_u32(payload, 12)?,
                    format: take_u32(payload, 16)?,
                    medium: GraphicsMedium::decode(medium)?,
                    byte_len: take_u32(payload, 20)?,
                    data,
                })
            },
            GRAPHICS_KIND_PLACEMENT => Some(GraphicsRecord::Placement {
                image_id: take_u64(payload, 0)?,
                placement_id: take_u64(payload, 8)?,
                cell_x: take_u32(payload, 16)?,
                cell_y: take_u32(payload, 20)?,
                offset_x: take_u32(payload, 24)?,
                offset_y: take_u32(payload, 28)?,
                source_x: take_u32(payload, 32)?,
                source_y: take_u32(payload, 36)?,
                source_width: take_u32(payload, 40)?,
                source_height: take_u32(payload, 44)?,
                z: take_u32(payload, 48)? as i32,
            }),
            GRAPHICS_KIND_DELETE_IMAGE => Some(GraphicsRecord::DeleteImage {
                id: take_u64(payload, 0)?,
            }),
            GRAPHICS_KIND_DELETE_PLACEMENT => Some(GraphicsRecord::DeletePlacement {
                image_id: take_u64(payload, 0)?,
                placement_id: take_u64(payload, 8)?,
            }),
            GRAPHICS_KIND_CLEAR => Some(GraphicsRecord::Clear),
            GRAPHICS_KIND_SIXEL => {
                let payload_len = take_u32(payload, 24)? as usize;
                Some(GraphicsRecord::SixelChunk {
                    cell_x: take_u32(payload, 0)?,
                    cell_y: take_u32(payload, 4)?,
                    pixel_x: take_u32(payload, 8)?,
                    pixel_y: take_u32(payload, 12)?,
                    pixel_width: take_u32(payload, 16)?,
                    pixel_height: take_u32(payload, 20)?,
                    payload: payload.get(32..32 + payload_len)?.to_vec(),
                })
            },
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PaneRect {
    pub x: u16,
    pub y: u16,
    pub cols: u16,
    pub rows: u16,
    pub top: u8,
    pub bottom: u8,
    pub left: u8,
    pub right: u8,
    pub flags: u8,
}

impl PaneRect {
    pub fn framed(&self) -> bool {
        self.flags & PANE_FRAMED != 0
    }

    pub fn focused(&self) -> bool {
        self.flags & PANE_FOCUSED != 0
    }

    pub fn wants_mouse(&self) -> bool {
        self.flags & PANE_WANTS_MOUSE != 0
    }

    pub fn selectable(&self) -> bool {
        self.flags & PANE_SELECTABLE != 0
    }

    pub fn content_x(&self) -> u16 {
        self.x.saturating_add(self.left as u16)
    }

    pub fn content_y(&self) -> u16 {
        self.y.saturating_add(self.top as u16)
    }

    pub fn content_cols(&self) -> u16 {
        self.cols
            .saturating_sub(self.left as u16)
            .saturating_sub(self.right as u16)
    }

    pub fn content_rows(&self) -> u16 {
        self.rows
            .saturating_sub(self.top as u16)
            .saturating_sub(self.bottom as u16)
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        within(x, self.x, self.cols) && within(y, self.y, self.rows)
    }

    pub fn content_contains(&self, x: u16, y: u16) -> bool {
        within(x, self.content_x(), self.content_cols())
            && within(y, self.content_y(), self.content_rows())
    }

    pub fn clip_to(&self, cols: u16, rows: u16) -> Option<PaneRect> {
        if self.x >= cols || self.y >= rows || self.cols == 0 || self.rows == 0 {
            return None;
        }
        let width = self.cols.min(cols - self.x);
        let height = self.rows.min(rows - self.y);
        let visible_right = self.x.saturating_add(width);
        let visible_bottom = self.y.saturating_add(height);
        let content_right = self
            .x
            .saturating_add(self.cols)
            .saturating_sub(self.right as u16);
        let content_bottom = self
            .y
            .saturating_add(self.rows)
            .saturating_sub(self.bottom as u16);
        let right = visible_right.saturating_sub(content_right.min(visible_right));
        let bottom = visible_bottom.saturating_sub(content_bottom.min(visible_bottom));
        Some(PaneRect {
            x: self.x,
            y: self.y,
            cols: width,
            rows: height,
            top: (self.top as u16).min(height) as u8,
            bottom: bottom.min(u8::MAX as u16) as u8,
            left: (self.left as u16).min(width) as u8,
            right: right.min(u8::MAX as u16) as u8,
            flags: self.flags,
        })
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.x.to_le_bytes());
        out.extend_from_slice(&self.y.to_le_bytes());
        out.extend_from_slice(&self.cols.to_le_bytes());
        out.extend_from_slice(&self.rows.to_le_bytes());
        out.push(self.top);
        out.push(self.bottom);
        out.push(self.left);
        out.push(self.right);
        out.push(self.flags);
        out.extend_from_slice(&[0u8; 3]);
    }

    fn decode(entry: &[u8]) -> Option<Self> {
        let entry = entry.get(..GEOMETRY_ENTRY_LEN)?;
        Some(PaneRect {
            x: u16::from_le_bytes([entry[0], entry[1]]),
            y: u16::from_le_bytes([entry[2], entry[3]]),
            cols: u16::from_le_bytes([entry[4], entry[5]]),
            rows: u16::from_le_bytes([entry[6], entry[7]]),
            top: entry[8],
            bottom: entry[9],
            left: entry[10],
            right: entry[11],
            flags: entry[12],
        })
    }
}

fn within(value: u16, start: u16, len: u16) -> bool {
    value >= start && (value as u32) < start as u32 + len as u32
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct GeometryRecord {
    pub panes: Vec<PaneRect>,
}

impl GeometryRecord {
    pub fn pane_at(&self, x: u16, y: u16) -> Option<&PaneRect> {
        self.panes.iter().rev().find(|pane| pane.contains(x, y))
    }

    pub fn content_pane_at(&self, x: u16, y: u16) -> Option<&PaneRect> {
        self.panes
            .iter()
            .rev()
            .find(|pane| pane.content_contains(x, y))
    }

    pub fn clip_to(&self, cols: u16, rows: u16) -> GeometryRecord {
        GeometryRecord {
            panes: self
                .panes
                .iter()
                .filter_map(|pane| pane.clip_to(cols, rows))
                .collect(),
        }
    }

    fn encode_payload(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.panes.len() as u16).to_le_bytes());
        out.extend_from_slice(&(GEOMETRY_ENTRY_LEN as u16).to_le_bytes());
        put_u32(out, 0);
        for pane in &self.panes {
            pane.encode(out);
        }
    }

    fn decode(payload: &[u8]) -> Option<Self> {
        let header = payload.get(..GEOMETRY_HEADER_LEN)?;
        let pane_count = u16::from_le_bytes([header[0], header[1]]) as usize;
        let entry_len = u16::from_le_bytes([header[2], header[3]]) as usize;
        if entry_len < GEOMETRY_ENTRY_LEN {
            return None;
        }
        let entries = payload.get(GEOMETRY_HEADER_LEN..)?;
        let mut panes = Vec::with_capacity(pane_count);
        for index in 0..pane_count {
            let at = index.checked_mul(entry_len)?;
            let end = at.checked_add(entry_len)?;
            panes.push(PaneRect::decode(entries.get(at..end)?)?);
        }
        Some(GeometryRecord { panes })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkEntry {
    pub id: u8,
    pub uri: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LinkRecord {
    pub entries: Vec<LinkEntry>,
}

impl LinkRecord {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn encode_payload(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(LINK_ENTRY_HEADER_LEN as u16).to_le_bytes());
        put_u32(out, 0);
        for entry in &self.entries {
            let uri = entry.uri.as_bytes();
            out.push(entry.id);
            out.push(0);
            out.extend_from_slice(&(uri.len() as u16).to_le_bytes());
            out.extend_from_slice(uri);
        }
    }

    fn decode(payload: &[u8]) -> Option<Self> {
        let header = payload.get(..LINKS_HEADER_LEN)?;
        let entry_count = u16::from_le_bytes([header[0], header[1]]) as usize;
        let entry_header_len = u16::from_le_bytes([header[2], header[3]]) as usize;
        if entry_header_len < LINK_ENTRY_HEADER_LEN {
            return None;
        }
        let mut at = LINKS_HEADER_LEN;
        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            let entry_header = payload.get(at..at.checked_add(entry_header_len)?)?;
            let id = entry_header[0];
            let uri_len = u16::from_le_bytes([entry_header[2], entry_header[3]]) as usize;
            at += entry_header_len;
            let uri = payload.get(at..at.checked_add(uri_len)?)?;
            at += uri_len;
            if id == LINK_NONE {
                return None;
            }
            entries.push(LinkEntry {
                id,
                uri: std::str::from_utf8(uri).ok()?.to_owned(),
            });
        }
        Some(LinkRecord { entries })
    }
}

fn record_is_decodable(kind: u8, medium: u8, payload: &[u8]) -> bool {
    match kind {
        RECORD_KIND_GEOMETRY => GeometryRecord::decode(payload).is_some(),
        RECORD_KIND_LINKS => LinkRecord::decode(payload).is_some(),
        _ => GraphicsRecord::decode(kind, medium, payload).is_some(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    pub version: u8,
    pub flags: u8,
    pub cols: u16,
    pub rows: u16,
    pub seq: u64,
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub cursor_shape: u8,
    pub cursor_visible: bool,
    pub row_record_count: u32,
    pub cells_offset: u32,
    pub cells_len: u32,
    pub graphics_offset: u32,
    pub graphics_len: u32,
    pub sideband_offset: u32,
    pub sideband_len: u32,
}

impl FrameHeader {
    pub fn full_repaint(&self) -> bool {
        self.flags & FLAG_FULL_REPAINT != 0
    }

    pub fn clear(&self) -> bool {
        self.flags & FLAG_CLEAR != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorState {
    pub x: u16,
    pub y: u16,
    pub shape: u8,
    pub visible: bool,
}

impl CursorState {
    pub fn blinking(&self) -> bool {
        self.shape & CURSOR_BLINKING != 0
    }

    pub fn shape_kind(&self) -> u8 {
        self.shape & CURSOR_SHAPE_MASK
    }
}

impl Default for CursorState {
    fn default() -> Self {
        CursorState {
            x: 0,
            y: 0,
            shape: CURSOR_SHAPE_DEFAULT,
            visible: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    TooShort,
    UnsupportedVersion(u8),
    BadSection,
    CellCountMismatch,
    RowOutOfRange,
    CellsExhausted,
    BadGraphicsRecord,
}

impl Display for DecodeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::TooShort => write!(f, "the frame is shorter than its header"),
            DecodeError::UnsupportedVersion(version) => {
                write!(f, "the frame declares wire version {}", version)
            },
            DecodeError::BadSection => write!(f, "a frame section runs outside the frame"),
            DecodeError::CellCountMismatch => {
                write!(f, "the cell section is not a whole number of cells")
            },
            DecodeError::RowOutOfRange => write!(f, "a row record runs outside the viewport"),
            DecodeError::CellsExhausted => {
                write!(f, "the row records name more cells than the frame carries")
            },
            DecodeError::BadGraphicsRecord => {
                write!(f, "a graphics record is malformed or runs past its section")
            },
        }
    }
}

impl std::error::Error for DecodeError {}

fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) / alignment * alignment
}

pub struct FrameBuilder {
    cols: u16,
    rows: u16,
    seq: u64,
    flags: u8,
    cursor: CursorState,
    records: Vec<RowRecord>,
    cells: Vec<u8>,
    graphics: Vec<u8>,
    sideband: Vec<u8>,
}

impl FrameBuilder {
    pub fn new(cols: u16, rows: u16, seq: u64) -> Self {
        FrameBuilder {
            cols,
            rows,
            seq,
            flags: 0,
            cursor: CursorState::default(),
            records: Vec::new(),
            cells: Vec::new(),
            graphics: Vec::new(),
            sideband: Vec::new(),
        }
    }

    pub fn set_full_repaint(&mut self, full_repaint: bool) {
        self.set_flag(FLAG_FULL_REPAINT, full_repaint);
    }

    pub fn set_clear(&mut self, clear: bool) {
        self.set_flag(FLAG_CLEAR, clear);
    }

    fn set_flag(&mut self, flag: u8, on: bool) {
        if on {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    pub fn set_cursor(&mut self, cursor: CursorState) {
        self.cursor = cursor;
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    pub fn push_row(&mut self, y: u16, x0: u16, cells: &[WireCell]) {
        if y >= self.rows || x0 >= self.cols || cells.is_empty() {
            return;
        }
        let room = (self.cols - x0) as usize;
        let cells = &cells[..cells.len().min(room)];
        self.records.push(RowRecord {
            y,
            x0,
            len: cells.len() as u16,
        });
        self.cells.reserve(cells.len() * CELL_LEN);
        for cell in cells {
            self.cells.extend_from_slice(&cell.to_bytes());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty() && self.graphics.is_empty() && self.sideband.is_empty()
    }

    pub fn sideband_mut(&mut self) -> &mut Vec<u8> {
        &mut self.sideband
    }

    fn push_record(&mut self, kind: u8, medium: u8, payload: impl FnOnce(&mut Vec<u8>)) {
        let start = self.graphics.len();
        self.graphics.push(kind);
        self.graphics.push(medium);
        self.graphics.extend_from_slice(&0u16.to_le_bytes());
        self.graphics.extend_from_slice(&0u32.to_le_bytes());
        payload(&mut self.graphics);
        let payload_len = self.graphics.len() - start - GRAPHICS_RECORD_HEADER_LEN;
        self.graphics[start + 4..start + 8].copy_from_slice(&(payload_len as u32).to_le_bytes());
        self.graphics.resize(align_up(self.graphics.len(), 4), 0);
    }

    fn push_graphics(&mut self, record: &GraphicsRecord) {
        self.push_record(record.kind(), record.medium(), |out| {
            record.encode_payload(out)
        });
    }

    pub fn push_geometry(&mut self, record: &GeometryRecord) {
        self.push_record(RECORD_KIND_GEOMETRY, GRAPHICS_MEDIUM_INLINE, |out| {
            record.encode_payload(out)
        });
    }

    pub fn push_links(&mut self, record: &LinkRecord) {
        if record.is_empty() {
            return;
        }
        self.push_record(RECORD_KIND_LINKS, GRAPHICS_MEDIUM_INLINE, |out| {
            record.encode_payload(out)
        });
    }

    pub fn extend_graphics<'a>(&mut self, records: impl IntoIterator<Item = &'a GraphicsRecord>) {
        for record in records {
            self.push_graphics(record);
        }
    }

    pub fn finish(self) -> Vec<u8> {
        let records_offset = HEADER_LEN;
        let records_len = self.records.len() * ROW_RECORD_LEN;
        let cells_offset = align_up(records_offset + records_len, 4);
        let cells_len = self.cells.len();
        let graphics_offset = align_up(cells_offset + cells_len, 4);
        let graphics_len = self.graphics.len();
        let sideband_offset = graphics_offset + graphics_len;
        let total = sideband_offset + self.sideband.len();

        let mut frame = Vec::with_capacity(total);
        frame.push(WIRE_VERSION);
        frame.push(self.flags);
        frame.extend_from_slice(&self.cols.to_le_bytes());
        frame.extend_from_slice(&self.rows.to_le_bytes());
        frame.extend_from_slice(&self.seq.to_le_bytes());
        frame.extend_from_slice(&self.cursor.x.to_le_bytes());
        frame.extend_from_slice(&self.cursor.y.to_le_bytes());
        frame.push(self.cursor.shape);
        frame.push(u8::from(self.cursor.visible));
        frame.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        frame.extend_from_slice(&(cells_offset as u32).to_le_bytes());
        frame.extend_from_slice(&(cells_len as u32).to_le_bytes());
        frame.extend_from_slice(&(graphics_offset as u32).to_le_bytes());
        frame.extend_from_slice(&(graphics_len as u32).to_le_bytes());
        frame.extend_from_slice(&(sideband_offset as u32).to_le_bytes());
        frame.extend_from_slice(&(self.sideband.len() as u32).to_le_bytes());
        debug_assert_eq!(frame.len(), HEADER_LEN);

        for record in &self.records {
            frame.extend_from_slice(&record.y.to_le_bytes());
            frame.extend_from_slice(&record.x0.to_le_bytes());
            frame.extend_from_slice(&record.len.to_le_bytes());
            frame.extend_from_slice(&0u16.to_le_bytes());
        }
        frame.resize(cells_offset, 0);
        frame.extend_from_slice(&self.cells);
        frame.resize(graphics_offset, 0);
        frame.extend_from_slice(&self.graphics);
        frame.extend_from_slice(&self.sideband);
        frame
    }
}

pub struct FrameView<'a> {
    header: FrameHeader,
    buffer: &'a [u8],
}

pub fn decode(buffer: &[u8]) -> Result<FrameView<'_>, DecodeError> {
    if buffer.len() < HEADER_LEN {
        return Err(DecodeError::TooShort);
    }
    let u16_at = |offset: usize| u16::from_le_bytes([buffer[offset], buffer[offset + 1]]);
    let u32_at = |offset: usize| {
        u32::from_le_bytes([
            buffer[offset],
            buffer[offset + 1],
            buffer[offset + 2],
            buffer[offset + 3],
        ])
    };
    let version = buffer[0];
    if version != WIRE_VERSION {
        return Err(DecodeError::UnsupportedVersion(version));
    }
    let mut seq_bytes = [0u8; 8];
    seq_bytes.copy_from_slice(&buffer[6..14]);
    let header = FrameHeader {
        version,
        flags: buffer[1],
        cols: u16_at(2),
        rows: u16_at(4),
        seq: u64::from_le_bytes(seq_bytes),
        cursor_x: u16_at(14),
        cursor_y: u16_at(16),
        cursor_shape: buffer[18],
        cursor_visible: buffer[19] != 0,
        row_record_count: u32_at(20),
        cells_offset: u32_at(24),
        cells_len: u32_at(28),
        graphics_offset: u32_at(32),
        graphics_len: u32_at(36),
        sideband_offset: u32_at(40),
        sideband_len: u32_at(44),
    };

    let records_len = (header.row_record_count as usize)
        .checked_mul(ROW_RECORD_LEN)
        .ok_or(DecodeError::BadSection)?;
    let records_end = HEADER_LEN
        .checked_add(records_len)
        .ok_or(DecodeError::BadSection)?;
    if records_end > buffer.len() {
        return Err(DecodeError::BadSection);
    }

    let cells_offset = header.cells_offset as usize;
    let cells_len = header.cells_len as usize;
    let cells_end = cells_offset
        .checked_add(cells_len)
        .ok_or(DecodeError::BadSection)?;
    if cells_offset < records_end || cells_end > buffer.len() {
        return Err(DecodeError::BadSection);
    }
    if cells_len % CELL_LEN != 0 {
        return Err(DecodeError::CellCountMismatch);
    }

    let graphics_offset = header.graphics_offset as usize;
    let graphics_end = graphics_offset
        .checked_add(header.graphics_len as usize)
        .ok_or(DecodeError::BadSection)?;
    if graphics_offset < cells_end || graphics_end > buffer.len() {
        return Err(DecodeError::BadSection);
    }

    let sideband_offset = header.sideband_offset as usize;
    let sideband_end = sideband_offset
        .checked_add(header.sideband_len as usize)
        .ok_or(DecodeError::BadSection)?;
    if sideband_offset < graphics_end || sideband_end > buffer.len() {
        return Err(DecodeError::BadSection);
    }

    let mut at = graphics_offset;
    while at < graphics_end {
        let header_end = at
            .checked_add(GRAPHICS_RECORD_HEADER_LEN)
            .ok_or(DecodeError::BadGraphicsRecord)?;
        if header_end > graphics_end {
            return Err(DecodeError::BadGraphicsRecord);
        }
        let payload_len = u32::from_le_bytes([
            buffer[at + 4],
            buffer[at + 5],
            buffer[at + 6],
            buffer[at + 7],
        ]) as usize;
        let payload_end = header_end
            .checked_add(payload_len)
            .ok_or(DecodeError::BadGraphicsRecord)?;
        if payload_end > graphics_end {
            return Err(DecodeError::BadGraphicsRecord);
        }
        if !record_is_decodable(buffer[at], buffer[at + 1], &buffer[header_end..payload_end]) {
            return Err(DecodeError::BadGraphicsRecord);
        }
        at = align_up(payload_end, 4);
    }

    let available_cells = cells_len / CELL_LEN;
    let mut named_cells = 0usize;
    for index in 0..header.row_record_count as usize {
        let base = HEADER_LEN + index * ROW_RECORD_LEN;
        let y = u16::from_le_bytes([buffer[base], buffer[base + 1]]);
        let x0 = u16::from_le_bytes([buffer[base + 2], buffer[base + 3]]);
        let len = u16::from_le_bytes([buffer[base + 4], buffer[base + 5]]);
        if y >= header.rows {
            return Err(DecodeError::RowOutOfRange);
        }
        if x0 as usize + len as usize > header.cols as usize {
            return Err(DecodeError::RowOutOfRange);
        }
        named_cells += len as usize;
        if named_cells > available_cells {
            return Err(DecodeError::CellsExhausted);
        }
    }
    if named_cells != available_cells {
        return Err(DecodeError::CellCountMismatch);
    }

    Ok(FrameView { header, buffer })
}

impl<'a> FrameView<'a> {
    pub fn header(&self) -> &FrameHeader {
        &self.header
    }

    pub fn cursor(&self) -> CursorState {
        CursorState {
            x: self.header.cursor_x,
            y: self.header.cursor_y,
            shape: self.header.cursor_shape,
            visible: self.header.cursor_visible,
        }
    }

    pub fn sideband(&self) -> &'a [u8] {
        let start = self.header.sideband_offset as usize;
        &self.buffer[start..start + self.header.sideband_len as usize]
    }

    pub fn graphics(&self) -> Vec<GraphicsRecord> {
        let start = self.header.graphics_offset as usize;
        let end = start + self.header.graphics_len as usize;
        let mut records = Vec::new();
        let mut at = start;
        while at < end {
            let header_end = at + GRAPHICS_RECORD_HEADER_LEN;
            let payload_len = u32::from_le_bytes([
                self.buffer[at + 4],
                self.buffer[at + 5],
                self.buffer[at + 6],
                self.buffer[at + 7],
            ]) as usize;
            let payload_end = header_end + payload_len;
            if let Some(record) = GraphicsRecord::decode(
                self.buffer[at],
                self.buffer[at + 1],
                &self.buffer[header_end..payload_end],
            ) {
                records.push(record);
            }
            at = align_up(payload_end, 4);
        }
        records
    }

    pub fn geometry(&self) -> Option<GeometryRecord> {
        let start = self.header.graphics_offset as usize;
        let end = start + self.header.graphics_len as usize;
        let mut geometry = None;
        let mut at = start;
        while at < end {
            let header_end = at + GRAPHICS_RECORD_HEADER_LEN;
            let payload_len = u32::from_le_bytes([
                self.buffer[at + 4],
                self.buffer[at + 5],
                self.buffer[at + 6],
                self.buffer[at + 7],
            ]) as usize;
            let payload_end = header_end + payload_len;
            if self.buffer[at] == RECORD_KIND_GEOMETRY {
                geometry = GeometryRecord::decode(&self.buffer[header_end..payload_end]);
            }
            at = align_up(payload_end, 4);
        }
        geometry
    }

    pub fn links(&self) -> LinkRecord {
        let start = self.header.graphics_offset as usize;
        let end = start + self.header.graphics_len as usize;
        let mut links = LinkRecord::default();
        let mut at = start;
        while at < end {
            let header_end = at + GRAPHICS_RECORD_HEADER_LEN;
            let payload_len = u32::from_le_bytes([
                self.buffer[at + 4],
                self.buffer[at + 5],
                self.buffer[at + 6],
                self.buffer[at + 7],
            ]) as usize;
            let payload_end = header_end + payload_len;
            if self.buffer[at] == RECORD_KIND_LINKS {
                if let Some(record) = LinkRecord::decode(&self.buffer[header_end..payload_end]) {
                    links.entries.extend(record.entries);
                }
            }
            at = align_up(payload_end, 4);
        }
        links
    }

    pub fn record(&self, index: usize) -> Option<RowRecord> {
        (index < self.header.row_record_count as usize).then(|| self.record_at(index))
    }

    fn record_at(&self, index: usize) -> RowRecord {
        let base = HEADER_LEN + index * ROW_RECORD_LEN;
        RowRecord {
            y: u16::from_le_bytes([self.buffer[base], self.buffer[base + 1]]),
            x0: u16::from_le_bytes([self.buffer[base + 2], self.buffer[base + 3]]),
            len: u16::from_le_bytes([self.buffer[base + 4], self.buffer[base + 5]]),
        }
    }

    pub fn rows(&self) -> RowIter<'_, 'a> {
        RowIter {
            view: self,
            index: 0,
            cursor: self.header.cells_offset as usize,
        }
    }

    fn blit(&self, record: RowRecord, at: usize, destination: &mut [WireCell], cols: usize) {
        let start = record.y as usize * cols + record.x0 as usize;
        let len = record.len as usize;
        let bytes = len * CELL_LEN;
        if start + len > destination.len() {
            return;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.buffer.as_ptr().add(at),
                destination.as_mut_ptr().add(start) as *mut u8,
                bytes,
            );
        }
    }

    pub fn apply(&self, destination: &mut [WireCell], cols: usize, rows: usize) {
        if cols != self.header.cols as usize || rows != self.header.rows as usize {
            return;
        }
        let mut at = self.header.cells_offset as usize;
        for index in 0..self.header.row_record_count as usize {
            let record = self.record_at(index);
            self.blit(record, at, destination, cols);
            at += record.len as usize * CELL_LEN;
        }
    }

    pub fn cell(&self, at: usize) -> WireCell {
        let mut chunk = [0u8; CELL_LEN];
        chunk.copy_from_slice(&self.buffer[at..at + CELL_LEN]);
        WireCell {
            ch: u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            fg: u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
            bg: u32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]),
            underline_color: u32::from_le_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]),
            attrs: u16::from_le_bytes([chunk[16], chunk[17]]),
            width: chunk[18],
            link: chunk[19],
        }
    }
}

pub struct RowIter<'v, 'a> {
    view: &'v FrameView<'a>,
    index: usize,
    cursor: usize,
}

impl<'v, 'a> Iterator for RowIter<'v, 'a> {
    type Item = (RowRecord, Vec<WireCell>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.view.header.row_record_count as usize {
            return None;
        }
        let record = self.view.record_at(self.index);
        let mut cells = Vec::with_capacity(record.len as usize);
        for offset in 0..record.len as usize {
            cells.push(self.view.cell(self.cursor + offset * CELL_LEN));
        }
        self.index += 1;
        self.cursor += record.len as usize * CELL_LEN;
        Some((record, cells))
    }
}

#[derive(Debug)]
pub struct PendingOverlay {
    cols: usize,
    rows: usize,
    cells: Vec<WireCell>,
    spans: Vec<Vec<(u16, u16)>>,
    cursor: CursorState,
    cursor_seen: bool,
    full_repaint: bool,
    clear: bool,
    geometry: Option<GeometryRecord>,
    links: Vec<LinkEntry>,
    graphics: Vec<GraphicsRecord>,
    sideband: Vec<u8>,
}

impl PendingOverlay {
    pub fn new(cols: usize, rows: usize) -> Self {
        PendingOverlay {
            cols,
            rows,
            cells: vec![WireCell::BLANK; cols * rows],
            spans: vec![Vec::new(); rows],
            cursor: CursorState::default(),
            cursor_seen: false,
            full_repaint: false,
            clear: false,
            geometry: None,
            links: Vec::new(),
            graphics: Vec::new(),
            sideband: Vec::new(),
        }
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        *self = PendingOverlay::new(cols, rows);
    }

    pub fn is_dirty(&self) -> bool {
        self.cursor_seen
            || self.full_repaint
            || self.clear
            || self.geometry.is_some()
            || !self.links.is_empty()
            || !self.graphics.is_empty()
            || !self.sideband.is_empty()
            || self.spans.iter().any(|spans| !spans.is_empty())
    }

    fn merge_row(&mut self, y: u16, x0: u16, cells: &[WireCell]) {
        let row = y as usize;
        if row >= self.rows || cells.is_empty() {
            return;
        }
        let start = x0 as usize;
        let end = (start + cells.len()).min(self.cols);
        if start >= end {
            return;
        }
        self.cells[row * self.cols + start..row * self.cols + end]
            .copy_from_slice(&cells[..end - start]);
        insert_span(&mut self.spans[row], start as u16, end as u16);
    }

    pub fn merge_frame(&mut self, frame: &FrameView<'_>) {
        let header = frame.header();
        if header.cols as usize != self.cols || header.rows as usize != self.rows {
            self.resize(header.cols as usize, header.rows as usize);
        }
        if header.full_repaint() {
            self.full_repaint = true;
            self.links.clear();
        }
        if header.clear() {
            self.clear = true;
        }
        if header.full_repaint() || header.clear() {
            self.cells
                .iter_mut()
                .for_each(|cell| *cell = WireCell::BLANK);
            self.spans.iter_mut().for_each(Vec::clear);
        }
        for (record, cells) in frame.rows() {
            self.merge_row(record.y, record.x0, &cells);
        }
        self.cursor = frame.cursor();
        self.cursor_seen = true;
        if let Some(geometry) = frame.geometry() {
            self.geometry = Some(geometry);
        }
        for entry in frame.links().entries {
            match self.links.iter_mut().find(|held| held.id == entry.id) {
                Some(held) => held.uri = entry.uri,
                None => self.links.push(entry),
            }
        }
        self.graphics.extend(frame.graphics());
        self.sideband.extend_from_slice(frame.sideband());
    }

    pub fn drain(&mut self, seq: u64) -> Vec<u8> {
        let mut builder = FrameBuilder::new(self.cols as u16, self.rows as u16, seq);
        builder.set_full_repaint(self.full_repaint);
        builder.set_clear(self.clear);
        if self.cursor_seen {
            builder.set_cursor(self.cursor);
        }
        for row in 0..self.rows {
            for (start, end) in std::mem::take(&mut self.spans[row]) {
                let base = row * self.cols;
                builder.push_row(
                    row as u16,
                    start,
                    &self.cells[base + start as usize..base + end as usize],
                );
            }
        }
        if let Some(geometry) = self.geometry.take() {
            builder.push_geometry(&geometry);
        }
        builder.push_links(&LinkRecord {
            entries: std::mem::take(&mut self.links),
        });
        builder.extend_graphics(std::mem::take(&mut self.graphics).iter());
        builder
            .sideband_mut()
            .extend_from_slice(&std::mem::take(&mut self.sideband));
        self.cursor_seen = false;
        self.full_repaint = false;
        self.clear = false;
        builder.finish()
    }
}

fn insert_span(spans: &mut Vec<(u16, u16)>, start: u16, end: u16) {
    let mut start = start;
    let mut end = end;
    let mut merged: Vec<(u16, u16)> = Vec::with_capacity(spans.len() + 1);
    let mut inserted = false;
    for (span_start, span_end) in spans.drain(..) {
        if span_end < start {
            merged.push((span_start, span_end));
        } else if span_start > end {
            if !inserted {
                merged.push((start, end));
                inserted = true;
            }
            merged.push((span_start, span_end));
        } else {
            start = start.min(span_start);
            end = end.max(span_end);
        }
    }
    if !inserted {
        merged.push((start, end));
    }
    merged.sort_unstable();
    *spans = merged;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(character: char) -> WireCell {
        WireCell {
            ch: character as u32,
            ..WireCell::BLANK
        }
    }

    fn run(text: &str) -> Vec<WireCell> {
        text.chars().map(cell).collect()
    }

    fn grid(cols: usize, rows: usize, frame: &[u8]) -> Vec<WireCell> {
        let view = decode(frame).expect("the frame should decode");
        let mut destination = vec![WireCell::BLANK; cols * rows];
        view.apply(&mut destination, cols, rows);
        destination
    }

    fn error_of(frame: &[u8]) -> Option<DecodeError> {
        decode(frame).err()
    }

    fn text_of(cells: &[WireCell], cols: usize, row: usize) -> String {
        cells[row * cols..(row + 1) * cols]
            .iter()
            .map(WireCell::character)
            .collect()
    }

    #[test]
    fn a_frame_round_trips_its_header_rows_and_sideband() {
        let mut builder = FrameBuilder::new(8, 3, 42);
        builder.set_full_repaint(true);
        builder.set_cursor(CursorState {
            x: 3,
            y: 1,
            shape: CURSOR_SHAPE_BEAM | CURSOR_BLINKING,
            visible: true,
        });
        builder.push_row(1, 2, &run("abc"));
        builder.sideband_mut().extend_from_slice(b"\x1b_Gq=2\x1b\\");
        let frame = builder.finish();

        let view = decode(&frame).unwrap();
        let header = *view.header();
        assert_eq!(header.version, WIRE_VERSION);
        assert_eq!((header.cols, header.rows), (8, 3));
        assert_eq!(header.seq, 42);
        assert!(header.full_repaint());
        assert!(!header.clear());
        assert_eq!(
            view.cursor(),
            CursorState {
                x: 3,
                y: 1,
                shape: CURSOR_SHAPE_BEAM | CURSOR_BLINKING,
                visible: true
            }
        );
        assert!(view.cursor().blinking());
        assert_eq!(view.cursor().shape_kind(), CURSOR_SHAPE_BEAM);
        assert_eq!(view.sideband(), b"\x1b_Gq=2\x1b\\");

        let rows: Vec<_> = view.rows().collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].0,
            RowRecord {
                y: 1,
                x0: 2,
                len: 3
            }
        );
        assert_eq!(rows[0].1, run("abc"));
        assert_eq!(text_of(&grid(8, 3, &frame), 8, 1), "  abc   ");
    }

    #[test]
    fn the_cell_array_starts_on_a_four_byte_boundary() {
        for record_count in 0..5u16 {
            let mut builder = FrameBuilder::new(4, 8, 0);
            for y in 0..record_count {
                builder.push_row(y, 0, &run("z"));
            }
            let frame = builder.finish();
            let view = decode(&frame).unwrap();
            assert_eq!(view.header().cells_offset % 4, 0);
        }
    }

    #[test]
    fn every_style_field_survives_the_wire() {
        let styled = WireCell {
            ch: '\u{4f60}' as u32,
            fg: WireColor::Rgb(1, 2, 3).pack(),
            bg: WireColor::Indexed(208).pack(),
            underline_color: WireColor::Named(4).pack(),
            attrs: ATTR_BOLD | ATTR_ITALIC | ATTR_FAST_BLINK | (UNDERLINE_CURLY << UNDERLINE_SHIFT),
            width: 2,
            link: 17,
        };
        let mut builder = FrameBuilder::new(4, 1, 1);
        builder.push_row(0, 0, &[styled, WireCell::BLANK]);
        let frame = builder.finish();
        let cells = grid(4, 1, &frame);

        assert_eq!(cells[0], styled);
        assert_eq!(cells[0].character(), '\u{4f60}');
        assert!(cells[0].is_wide_head());
        assert!(cells[0].has(ATTR_BOLD) && cells[0].has(ATTR_ITALIC));
        assert!(!cells[0].has(ATTR_SLOW_BLINK) && cells[0].has(ATTR_FAST_BLINK));
        assert_eq!(cells[0].underline_style(), UNDERLINE_CURLY);
        assert_eq!(WireColor::unpack(cells[0].fg), WireColor::Rgb(1, 2, 3));
        assert_eq!(WireColor::unpack(cells[0].bg), WireColor::Indexed(208));
        assert_eq!(
            WireColor::unpack(cells[0].underline_color),
            WireColor::Named(4)
        );
        assert_eq!(WireColor::unpack(COLOR_DEFAULT), WireColor::Default);
    }

    #[test]
    fn a_row_may_carry_several_partial_records_and_later_ones_win() {
        let mut builder = FrameBuilder::new(6, 1, 0);
        builder.push_row(0, 0, &run("aaaa"));
        builder.push_row(0, 2, &run("bb"));
        let frame = builder.finish();
        assert_eq!(text_of(&grid(6, 1, &frame), 6, 0), "aabb  ");
    }

    #[test]
    fn a_run_that_would_overrun_the_right_edge_is_truncated_rather_than_emitted() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_row(0, 2, &run("abcd"));
        let frame = builder.finish();
        let view = decode(&frame).unwrap();
        assert_eq!(
            view.rows().next().unwrap().0,
            RowRecord {
                y: 0,
                x0: 2,
                len: 2
            }
        );
        assert_eq!(text_of(&grid(4, 1, &frame), 4, 0), "  ab");
    }

    #[test]
    fn a_run_outside_the_viewport_is_dropped_entirely() {
        let mut builder = FrameBuilder::new(4, 2, 0);
        builder.push_row(2, 0, &run("gone"));
        builder.push_row(0, 4, &run("gone"));
        builder.push_row(0, 0, &[]);
        let frame = builder.finish();
        assert_eq!(decode(&frame).unwrap().header().row_record_count, 0);
    }

    #[test]
    fn a_truncated_buffer_is_rejected() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_row(0, 0, &run("ab"));
        let frame = builder.finish();
        assert_eq!(
            error_of(&frame[..HEADER_LEN - 1]),
            Some(DecodeError::TooShort)
        );
        for cut in [HEADER_LEN, HEADER_LEN + 4, frame.len() - 1] {
            assert!(
                decode(&frame[..cut]).is_err(),
                "a frame cut at {} decoded",
                cut
            );
        }
    }

    #[test]
    fn a_foreign_wire_version_is_rejected() {
        let mut frame = FrameBuilder::new(4, 1, 0).finish();
        frame[0] = WIRE_VERSION + 1;
        assert_eq!(
            error_of(&frame),
            Some(DecodeError::UnsupportedVersion(WIRE_VERSION + 1))
        );
    }

    #[test]
    fn a_row_record_outside_the_viewport_is_rejected() {
        let mut builder = FrameBuilder::new(4, 2, 0);
        builder.push_row(0, 0, &run("ab"));
        let frame = builder.finish();

        let mut past_the_bottom = frame.clone();
        past_the_bottom[HEADER_LEN] = 9;
        assert_eq!(error_of(&past_the_bottom), Some(DecodeError::RowOutOfRange));

        let mut past_the_right = frame.clone();
        past_the_right[HEADER_LEN + 2] = 3;
        assert_eq!(error_of(&past_the_right), Some(DecodeError::RowOutOfRange));

        let mut overlong = frame;
        overlong[HEADER_LEN + 4] = 5;
        assert_eq!(error_of(&overlong), Some(DecodeError::RowOutOfRange));
    }

    #[test]
    fn section_offsets_that_leave_the_frame_are_rejected() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_row(0, 0, &run("ab"));
        builder.sideband_mut().extend_from_slice(b"xy");
        let frame = builder.finish();

        let mut cells_past_the_end = frame.clone();
        cells_past_the_end[24..28].copy_from_slice(&(frame.len() as u32 + 8).to_le_bytes());
        assert_eq!(error_of(&cells_past_the_end), Some(DecodeError::BadSection));

        let mut cells_inside_the_records = frame.clone();
        cells_inside_the_records[24..28].copy_from_slice(&(HEADER_LEN as u32).to_le_bytes());
        assert_eq!(
            error_of(&cells_inside_the_records),
            Some(DecodeError::BadSection)
        );

        let mut graphics_past_the_end = frame.clone();
        graphics_past_the_end[36..40].copy_from_slice(&1024u32.to_le_bytes());
        assert_eq!(
            error_of(&graphics_past_the_end),
            Some(DecodeError::BadSection)
        );

        let mut sideband_past_the_end = frame.clone();
        sideband_past_the_end[44..48].copy_from_slice(&1024u32.to_le_bytes());
        assert_eq!(
            error_of(&sideband_past_the_end),
            Some(DecodeError::BadSection)
        );

        let mut ragged_cells = frame;
        ragged_cells[28..32].copy_from_slice(&(CELL_LEN as u32 * 2 - 1).to_le_bytes());
        assert!(decode(&ragged_cells).is_err());
    }

    #[test]
    fn records_that_name_more_cells_than_the_frame_carries_are_rejected() {
        let mut builder = FrameBuilder::new(8, 1, 0);
        builder.push_row(0, 0, &run("ab"));
        let mut frame = builder.finish();
        frame[HEADER_LEN + 4] = 4;
        assert_eq!(error_of(&frame), Some(DecodeError::CellsExhausted));
    }

    #[test]
    fn records_that_name_fewer_cells_than_the_frame_carries_are_rejected() {
        let mut builder = FrameBuilder::new(8, 1, 0);
        builder.push_row(0, 0, &run("ab"));
        let mut frame = builder.finish();
        frame[HEADER_LEN + 4] = 1;
        assert_eq!(error_of(&frame), Some(DecodeError::CellCountMismatch));
    }

    #[test]
    fn an_overlay_replays_the_writes_it_absorbed_in_order() {
        let mut overlay = PendingOverlay::new(6, 2);
        assert!(!overlay.is_dirty());
        overlay.merge_row(0, 0, &run("aaaa"));
        overlay.merge_row(0, 2, &run("bb"));
        overlay.merge_row(1, 4, &run("zz"));
        assert!(overlay.is_dirty());

        let frame = overlay.drain(7);
        assert!(!overlay.is_dirty());
        let view = decode(&frame).unwrap();
        assert_eq!(view.header().seq, 7);
        let cells = grid(6, 2, &frame);
        assert_eq!(text_of(&cells, 6, 0), "aabb  ");
        assert_eq!(text_of(&cells, 6, 1), "    zz");
    }

    #[test]
    fn overlapping_and_adjacent_spans_are_unioned_into_one_record() {
        let mut overlay = PendingOverlay::new(8, 1);
        overlay.merge_row(0, 0, &run("ab"));
        overlay.merge_row(0, 2, &run("cd"));
        overlay.merge_row(0, 1, &run("XY"));
        let frame = overlay.drain(0);
        let view = decode(&frame).unwrap();
        let rows: Vec<_> = view.rows().collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].0,
            RowRecord {
                y: 0,
                x0: 0,
                len: 4
            }
        );
        assert_eq!(text_of(&grid(8, 1, &frame), 8, 0), "aXYd    ");
    }

    #[test]
    fn disjoint_spans_stay_separate_so_untouched_cells_are_never_repainted() {
        let mut overlay = PendingOverlay::new(8, 1);
        overlay.merge_row(0, 0, &run("ab"));
        overlay.merge_row(0, 6, &run("yz"));
        let frame = overlay.drain(0);
        let view = decode(&frame).unwrap();
        let records: Vec<RowRecord> = view.rows().map(|(record, _)| record).collect();
        assert_eq!(
            records,
            vec![
                RowRecord {
                    y: 0,
                    x0: 0,
                    len: 2
                },
                RowRecord {
                    y: 0,
                    x0: 6,
                    len: 2
                }
            ]
        );
    }

    #[test]
    fn an_overlay_absorbs_whole_frames_including_flags_cursor_and_sideband() {
        let mut first = FrameBuilder::new(6, 2, 1);
        first.set_clear(true);
        first.push_row(0, 0, &run("one"));
        first.sideband_mut().extend_from_slice(b"AA");
        let first = first.finish();

        let mut second = FrameBuilder::new(6, 2, 2);
        second.set_full_repaint(true);
        second.push_row(1, 0, &run("two"));
        second.set_cursor(CursorState {
            x: 5,
            y: 1,
            shape: CURSOR_SHAPE_UNDERLINE,
            visible: true,
        });
        second.sideband_mut().extend_from_slice(b"BB");
        let second = second.finish();

        let mut overlay = PendingOverlay::new(6, 2);
        overlay.merge_frame(&decode(&first).unwrap());
        overlay.merge_frame(&decode(&second).unwrap());

        let drained = overlay.drain(9);
        let view = decode(&drained).unwrap();
        assert!(view.header().full_repaint());
        assert!(view.header().clear());
        assert_eq!(view.header().seq, 9);
        assert_eq!(view.sideband(), b"AABB");
        assert_eq!(
            view.cursor(),
            CursorState {
                x: 5,
                y: 1,
                shape: CURSOR_SHAPE_UNDERLINE,
                visible: true
            }
        );
        let cells = grid(6, 2, &drained);
        assert_eq!(text_of(&cells, 6, 0), "      ");
        assert_eq!(text_of(&cells, 6, 1), "two   ");
    }

    #[test]
    fn rows_absorbed_before_a_clear_are_not_replayed_after_it() {
        let mut first = FrameBuilder::new(6, 2, 1);
        first.push_row(0, 0, &run("stale"));
        first.push_row(1, 0, &run("old"));
        let first = first.finish();

        let mut second = FrameBuilder::new(6, 2, 2);
        second.set_clear(true);
        second.push_row(0, 0, &run("ab"));
        let second = second.finish();

        let mut overlay = PendingOverlay::new(6, 2);
        overlay.merge_frame(&decode(&first).unwrap());
        overlay.merge_frame(&decode(&second).unwrap());

        let drained = overlay.drain(3);
        let view = decode(&drained).unwrap();
        assert!(view.header().clear());
        let records: Vec<RowRecord> = view.rows().map(|(record, _)| record).collect();
        assert_eq!(
            records,
            vec![RowRecord {
                y: 0,
                x0: 0,
                len: 2
            }]
        );
    }

    #[test]
    fn an_overlay_that_absorbs_a_reshaped_frame_adopts_the_new_dimensions() {
        let mut builder = FrameBuilder::new(3, 1, 1);
        builder.push_row(0, 0, &run("abc"));
        let frame = builder.finish();

        let mut overlay = PendingOverlay::new(8, 4);
        overlay.merge_row(3, 0, &run("stale"));
        overlay.merge_frame(&decode(&frame).unwrap());
        assert_eq!(overlay.dimensions(), (3, 1));

        let drained = overlay.drain(2);
        let view = decode(&drained).unwrap();
        assert_eq!((view.header().cols, view.header().rows), (3, 1));
        assert_eq!(text_of(&grid(3, 1, &drained), 3, 0), "abc");
    }

    fn residency(id: u64, medium: GraphicsMedium, data: &[u8]) -> GraphicsRecord {
        GraphicsRecord::Residency {
            id,
            width: 2,
            height: 1,
            format: GRAPHICS_FORMAT_RGBA8,
            medium,
            byte_len: 8,
            data: data.to_vec(),
        }
    }

    fn placement(image_id: u64, placement_id: u64, z: i32) -> GraphicsRecord {
        GraphicsRecord::Placement {
            image_id,
            placement_id,
            cell_x: 3,
            cell_y: 4,
            offset_x: 5,
            offset_y: 6,
            source_x: 7,
            source_y: 8,
            source_width: 9,
            source_height: 10,
            z,
        }
    }

    #[test]
    fn every_graphics_record_round_trips_in_emission_order() {
        let records = vec![
            GraphicsRecord::Clear,
            residency(1, GraphicsMedium::Inline, &[1, 2, 3, 4, 5, 6, 7, 8]),
            residency(2, GraphicsMedium::SharedFile, b"/dev/shm/zellij-image"),
            placement(1, 7, -3),
            GraphicsRecord::DeletePlacement {
                image_id: 1,
                placement_id: 7,
            },
            GraphicsRecord::DeleteImage { id: 2 },
            GraphicsRecord::SixelChunk {
                cell_x: 11,
                cell_y: 12,
                pixel_x: 13,
                pixel_y: 14,
                pixel_width: 15,
                pixel_height: 16,
                payload: b"\x1bP0;1;0q#0;2;100;0;0~\x1b\\".to_vec(),
            },
        ];

        let mut builder = FrameBuilder::new(4, 2, 0);
        builder.push_row(0, 0, &run("ab"));
        builder.extend_graphics(records.iter());
        builder
            .sideband_mut()
            .extend_from_slice(b"\x1b]0;title\x07");
        let frame = builder.finish();

        let view = decode(&frame).unwrap();
        assert_eq!(view.graphics(), records);
        assert_eq!(view.sideband(), b"\x1b]0;title\x07");
        assert_eq!(text_of(&grid(4, 2, &frame), 4, 0), "ab  ");
    }

    #[test]
    fn the_graphics_section_starts_on_a_four_byte_boundary() {
        for cells in 0..5u16 {
            let mut builder = FrameBuilder::new(8, 1, 0);
            if cells > 0 {
                builder.push_row(0, 0, &run(&"z".repeat(cells as usize)));
            }
            builder.push_graphics(&GraphicsRecord::DeleteImage { id: 9 });
            let frame = builder.finish();
            let view = decode(&frame).unwrap();
            assert_eq!(view.header().graphics_offset % 4, 0);
            assert_eq!(view.graphics(), vec![GraphicsRecord::DeleteImage { id: 9 }]);
        }
    }

    #[test]
    fn a_graphics_record_that_overruns_its_section_is_rejected() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_graphics(&GraphicsRecord::DeleteImage { id: 1 });
        let mut frame = builder.finish();
        let at = decode(&frame).unwrap().header().graphics_offset as usize;
        frame[at + 4..at + 8].copy_from_slice(&4096u32.to_le_bytes());
        assert_eq!(error_of(&frame), Some(DecodeError::BadGraphicsRecord));
    }

    #[test]
    fn a_graphics_record_of_an_unknown_kind_is_rejected() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_graphics(&GraphicsRecord::DeleteImage { id: 1 });
        let mut frame = builder.finish();
        let at = decode(&frame).unwrap().header().graphics_offset as usize;
        frame[at] = 99;
        assert_eq!(error_of(&frame), Some(DecodeError::BadGraphicsRecord));
    }

    #[test]
    fn an_overlay_replays_the_graphics_records_it_absorbed() {
        let mut first = FrameBuilder::new(4, 1, 1);
        first.push_graphics(&GraphicsRecord::Clear);
        first.push_graphics(&residency(1, GraphicsMedium::Inline, &[9; 8]));
        let first = first.finish();

        let mut second = FrameBuilder::new(4, 1, 2);
        second.push_graphics(&placement(1, 1, 0));
        let second = second.finish();

        let mut overlay = PendingOverlay::new(4, 1);
        assert!(!overlay.is_dirty());
        overlay.merge_frame(&decode(&first).unwrap());
        overlay.merge_frame(&decode(&second).unwrap());
        assert!(overlay.is_dirty());

        let drained = overlay.drain(3);
        assert_eq!(
            decode(&drained).unwrap().graphics(),
            vec![
                GraphicsRecord::Clear,
                residency(1, GraphicsMedium::Inline, &[9; 8]),
                placement(1, 1, 0),
            ]
        );
        assert!(!overlay.is_dirty());
    }

    fn pane(x: u16, y: u16, cols: u16, rows: u16, flags: u8) -> PaneRect {
        PaneRect {
            x,
            y,
            cols,
            rows,
            top: 1,
            bottom: 1,
            left: 1,
            right: 1,
            flags: flags | PANE_FRAMED | PANE_SELECTABLE,
        }
    }

    #[test]
    fn a_geometry_record_round_trips_every_field_of_every_pane() {
        let record = GeometryRecord {
            panes: vec![
                pane(0, 0, 40, 20, PANE_FOCUSED),
                PaneRect {
                    x: 40,
                    y: 0,
                    cols: 40,
                    rows: 20,
                    top: 0,
                    bottom: 0,
                    left: 0,
                    right: 0,
                    flags: PANE_WANTS_MOUSE,
                },
                PaneRect {
                    x: 0,
                    y: 20,
                    cols: 80,
                    rows: 1,
                    top: 0,
                    bottom: 0,
                    left: 0,
                    right: 0,
                    flags: 0,
                },
            ],
        };

        let mut builder = FrameBuilder::new(80, 21, 0);
        builder.push_geometry(&record);
        let frame = builder.finish();

        let view = decode(&frame).unwrap();
        assert_eq!(view.geometry(), Some(record));
        assert!(view.graphics().is_empty());
    }

    #[test]
    fn a_geometry_record_shares_the_record_stream_with_graphics_records() {
        let record = GeometryRecord {
            panes: vec![pane(0, 0, 10, 5, PANE_FOCUSED)],
        };
        let mut builder = FrameBuilder::new(10, 5, 0);
        builder.push_graphics(&GraphicsRecord::Clear);
        builder.push_geometry(&record);
        builder.push_graphics(&GraphicsRecord::DeleteImage { id: 4 });
        let frame = builder.finish();

        let view = decode(&frame).unwrap();
        assert_eq!(view.geometry(), Some(record));
        assert_eq!(
            view.graphics(),
            vec![GraphicsRecord::Clear, GraphicsRecord::DeleteImage { id: 4 }]
        );
    }

    #[test]
    fn a_frame_without_a_geometry_record_reports_none() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_row(0, 0, &run("ab"));
        builder.push_graphics(&GraphicsRecord::Clear);
        let frame = builder.finish();
        assert_eq!(decode(&frame).unwrap().geometry(), None);
    }

    #[test]
    fn an_empty_geometry_record_survives_the_wire_as_an_empty_list() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_geometry(&GeometryRecord::default());
        let frame = builder.finish();
        assert_eq!(
            decode(&frame).unwrap().geometry(),
            Some(GeometryRecord::default())
        );
    }

    #[test]
    fn a_geometry_record_that_names_more_panes_than_it_carries_is_rejected() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_geometry(&GeometryRecord {
            panes: vec![pane(0, 0, 4, 1, 0)],
        });
        let mut frame = builder.finish();
        let at = decode(&frame).unwrap().header().graphics_offset as usize;
        frame[at + GRAPHICS_RECORD_HEADER_LEN..at + GRAPHICS_RECORD_HEADER_LEN + 2]
            .copy_from_slice(&9u16.to_le_bytes());
        assert_eq!(error_of(&frame), Some(DecodeError::BadGraphicsRecord));
    }

    #[test]
    fn a_geometry_record_declaring_a_shorter_entry_than_this_build_knows_is_rejected() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_geometry(&GeometryRecord {
            panes: vec![pane(0, 0, 4, 1, 0)],
        });
        let mut frame = builder.finish();
        let at = decode(&frame).unwrap().header().graphics_offset as usize;
        frame[at + GRAPHICS_RECORD_HEADER_LEN + 2..at + GRAPHICS_RECORD_HEADER_LEN + 4]
            .copy_from_slice(&8u16.to_le_bytes());
        assert_eq!(error_of(&frame), Some(DecodeError::BadGraphicsRecord));
    }

    #[test]
    fn the_content_rectangle_is_the_outer_rectangle_less_its_frame_ring() {
        let framed = pane(10, 4, 20, 8, 0);
        assert_eq!(
            (
                framed.content_x(),
                framed.content_y(),
                framed.content_cols(),
                framed.content_rows()
            ),
            (11, 5, 18, 6)
        );
        assert!(framed.contains(10, 4));
        assert!(!framed.content_contains(10, 4));
        assert!(framed.content_contains(11, 5));
        assert!(framed.content_contains(28, 10));
        assert!(!framed.content_contains(29, 10));
        assert!(!framed.contains(30, 4));
    }

    #[test]
    fn the_topmost_pane_owns_a_cell_two_panes_share() {
        let record = GeometryRecord {
            panes: vec![pane(0, 0, 20, 10, 0), pane(4, 2, 8, 4, PANE_FOCUSED)],
        };
        assert_eq!(record.content_pane_at(1, 1), Some(&record.panes[0]));
        assert_eq!(record.content_pane_at(6, 4), Some(&record.panes[1]));
        assert_eq!(record.content_pane_at(40, 40), None);
    }

    #[test]
    fn clipping_a_pane_trims_the_rectangle_and_the_frame_ring_it_loses() {
        let framed = pane(0, 0, 20, 10, 0);
        let clipped = framed.clip_to(15, 10).unwrap();
        assert_eq!((clipped.x, clipped.cols), (0, 15));
        assert_eq!(clipped.right, 0);
        assert_eq!(clipped.left, 1);
        assert_eq!(clipped.content_cols(), 14);

        let clipped = framed.clip_to(20, 6).unwrap();
        assert_eq!((clipped.y, clipped.rows), (0, 6));
        assert_eq!(clipped.bottom, 0);
        assert_eq!(clipped.content_rows(), 5);

        assert_eq!(framed.clip_to(20, 10), Some(framed));
        assert_eq!(pane(30, 0, 10, 10, 0).clip_to(20, 10), None);
        assert_eq!(pane(0, 30, 10, 10, 0).clip_to(20, 10), None);
    }

    #[test]
    fn clipping_a_record_drops_the_panes_the_viewport_cannot_show() {
        let record = GeometryRecord {
            panes: vec![
                pane(0, 0, 20, 10, 0),
                pane(20, 0, 20, 10, 0),
                pane(0, 10, 40, 10, 0),
            ],
        };
        let clipped = record.clip_to(30, 10);
        assert_eq!(clipped.panes.len(), 2);
        assert_eq!(clipped.panes[0].cols, 20);
        assert_eq!(clipped.panes[1].cols, 10);
    }

    #[test]
    fn a_later_geometry_record_supersedes_an_earlier_one_in_the_overlay() {
        let first = GeometryRecord {
            panes: vec![pane(0, 0, 8, 4, PANE_FOCUSED)],
        };
        let second = GeometryRecord {
            panes: vec![pane(0, 0, 4, 4, 0), pane(4, 0, 4, 4, PANE_FOCUSED)],
        };

        let mut one = FrameBuilder::new(8, 4, 1);
        one.push_geometry(&first);
        one.push_graphics(&GraphicsRecord::Clear);
        let one = one.finish();

        let mut two = FrameBuilder::new(8, 4, 2);
        two.push_geometry(&second);
        let two = two.finish();

        let mut three = FrameBuilder::new(8, 4, 3);
        three.push_graphics(&GraphicsRecord::DeleteImage { id: 1 });
        let three = three.finish();

        let mut overlay = PendingOverlay::new(8, 4);
        assert!(!overlay.is_dirty());
        overlay.merge_frame(&decode(&one).unwrap());
        overlay.merge_frame(&decode(&two).unwrap());
        overlay.merge_frame(&decode(&three).unwrap());

        let drained = overlay.drain(4);
        let view = decode(&drained).unwrap();
        assert_eq!(view.geometry(), Some(second));
        assert_eq!(
            view.graphics(),
            vec![GraphicsRecord::Clear, GraphicsRecord::DeleteImage { id: 1 }]
        );
        assert!(!overlay.is_dirty());
    }

    #[test]
    fn an_overlay_that_absorbed_only_a_geometry_record_is_dirty_and_drains_it_once() {
        let record = GeometryRecord {
            panes: vec![pane(0, 0, 8, 4, 0)],
        };
        let mut builder = FrameBuilder::new(8, 4, 1);
        builder.push_geometry(&record);
        let frame = builder.finish();

        let mut overlay = PendingOverlay::new(8, 4);
        overlay.merge_frame(&decode(&frame).unwrap());
        assert!(overlay.is_dirty());
        assert_eq!(decode(&overlay.drain(2)).unwrap().geometry(), Some(record));
        assert!(!overlay.is_dirty());
        assert_eq!(decode(&overlay.drain(3)).unwrap().geometry(), None);
    }

    fn link(id: u8, uri: &str) -> LinkEntry {
        LinkEntry {
            id,
            uri: uri.to_owned(),
        }
    }

    fn linked(character: char, id: u8) -> WireCell {
        WireCell {
            link: id,
            ..cell(character)
        }
    }

    #[test]
    fn a_link_table_round_trips_every_entry_and_the_cells_that_name_them() {
        let record = LinkRecord {
            entries: vec![
                link(1, "https://example.com/one"),
                link(2, "mailto:someone@example.com"),
                link(255, ""),
            ],
        };
        let mut builder = FrameBuilder::new(6, 1, 7);
        builder.push_row(0, 0, &[linked('a', 1), linked('b', 2), cell('c')]);
        builder.push_links(&record);
        let frame = builder.finish();

        let view = decode(&frame).expect("the frame should decode");
        assert_eq!(view.links(), record);
        let cells = grid(6, 1, &frame);
        assert_eq!(cells[0].link, 1);
        assert_eq!(cells[1].link, 2);
        assert_eq!(cells[2].link, LINK_NONE);
        assert_eq!(cells[3].link, LINK_NONE);
    }

    #[test]
    fn a_link_table_shares_the_record_stream_with_geometry_and_graphics() {
        let links = LinkRecord {
            entries: vec![link(3, "file:///tmp/x")],
        };
        let geometry = GeometryRecord {
            panes: vec![pane(0, 0, 8, 4, PANE_FRAMED)],
        };
        let mut builder = FrameBuilder::new(8, 4, 1);
        builder.push_geometry(&geometry);
        builder.push_links(&links);
        builder.extend_graphics([GraphicsRecord::Clear].iter());
        let frame = builder.finish();

        let view = decode(&frame).expect("the frame should decode");
        assert_eq!(view.links(), links);
        assert_eq!(view.geometry(), Some(geometry));
        assert_eq!(view.graphics(), vec![GraphicsRecord::Clear]);
    }

    #[test]
    fn a_frame_without_a_link_table_reports_an_empty_one() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_row(0, 0, &run("abcd"));
        let frame = builder.finish();
        assert!(decode(&frame).unwrap().links().is_empty());
    }

    #[test]
    fn an_empty_link_table_is_never_written_to_the_wire() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_links(&LinkRecord::default());
        let frame = builder.finish();
        assert_eq!(decode(&frame).unwrap().header().graphics_len, 0);
    }

    fn one_link_frame() -> (Vec<u8>, usize) {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_links(&LinkRecord {
            entries: vec![link(1, "https://example.com")],
        });
        let frame = builder.finish();
        let payload =
            decode(&frame).unwrap().header().graphics_offset as usize + GRAPHICS_RECORD_HEADER_LEN;
        (frame, payload)
    }

    #[test]
    fn a_link_table_that_names_more_entries_than_it_carries_is_rejected() {
        let (mut frame, payload) = one_link_frame();
        frame[payload] = 2;
        assert_eq!(error_of(&frame), Some(DecodeError::BadGraphicsRecord));
    }

    #[test]
    fn a_link_entry_header_shorter_than_this_build_knows_is_rejected() {
        let (mut frame, payload) = one_link_frame();
        frame[payload + 2] = (LINK_ENTRY_HEADER_LEN - 1) as u8;
        assert_eq!(error_of(&frame), Some(DecodeError::BadGraphicsRecord));
    }

    #[test]
    fn a_link_entry_header_wider_than_this_build_knows_is_read_and_skipped() {
        let uri = "https://example.com/widened";
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u16.to_le_bytes());
        payload.extend_from_slice(&((LINK_ENTRY_HEADER_LEN + 4) as u16).to_le_bytes());
        put_u32(&mut payload, 0);
        payload.push(9);
        payload.push(0);
        payload.extend_from_slice(&(uri.len() as u16).to_le_bytes());
        payload.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        payload.extend_from_slice(uri.as_bytes());

        let record = LinkRecord::decode(&payload).expect("a widened entry should be readable");
        assert_eq!(record.entries, vec![link(9, uri)]);
    }

    #[test]
    fn a_link_entry_claiming_the_no_link_id_is_rejected() {
        let (mut frame, payload) = one_link_frame();
        frame[payload + LINKS_HEADER_LEN] = LINK_NONE;
        assert_eq!(error_of(&frame), Some(DecodeError::BadGraphicsRecord));
    }

    #[test]
    fn a_link_entry_whose_uri_is_not_text_is_rejected() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u16.to_le_bytes());
        payload.extend_from_slice(&(LINK_ENTRY_HEADER_LEN as u16).to_le_bytes());
        put_u32(&mut payload, 0);
        payload.push(1);
        payload.push(0);
        payload.extend_from_slice(&2u16.to_le_bytes());
        payload.extend_from_slice(&[0xff, 0xfe]);
        assert_eq!(LinkRecord::decode(&payload), None);
    }

    #[test]
    fn a_link_table_that_carries_no_entries_decodes_to_an_empty_one() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.extend_from_slice(&(LINK_ENTRY_HEADER_LEN as u16).to_le_bytes());
        put_u32(&mut payload, 0);
        assert_eq!(LinkRecord::decode(&payload), Some(LinkRecord::default()));
    }

    #[test]
    fn an_overlay_merges_link_tables_and_a_later_entry_supersedes_an_earlier_one() {
        let mut first = FrameBuilder::new(8, 2, 1);
        first.push_links(&LinkRecord {
            entries: vec![link(1, "https://first"), link(2, "https://kept")],
        });
        let mut second = FrameBuilder::new(8, 2, 2);
        second.push_links(&LinkRecord {
            entries: vec![link(1, "https://second")],
        });

        let mut overlay = PendingOverlay::new(8, 2);
        overlay.merge_frame(&decode(&first.finish()).unwrap());
        overlay.merge_frame(&decode(&second.finish()).unwrap());
        assert!(overlay.is_dirty());

        let drained = overlay.drain(3);
        assert_eq!(
            decode(&drained).unwrap().links().entries,
            vec![link(1, "https://second"), link(2, "https://kept")]
        );
        assert!(!overlay.is_dirty());
        let again = overlay.drain(4);
        assert!(decode(&again).unwrap().links().is_empty());
    }

    #[test]
    fn a_drained_frame_carries_the_entry_a_later_row_still_names() {
        let mut first = FrameBuilder::new(4, 2, 1);
        first.push_row(0, 0, &[linked('a', 1), linked('b', 1)]);
        first.push_links(&LinkRecord {
            entries: vec![link(1, "https://example.com/held")],
        });
        let mut second = FrameBuilder::new(4, 2, 2);
        second.push_row(1, 0, &[linked('c', 1), linked('d', 1)]);

        let mut overlay = PendingOverlay::new(4, 2);
        overlay.merge_frame(&decode(&first.finish()).unwrap());
        overlay.merge_frame(&decode(&second.finish()).unwrap());

        let drained = overlay.drain(3);
        let view = decode(&drained).unwrap();
        assert_eq!(
            view.links().entries,
            vec![link(1, "https://example.com/held")],
            "the row that arrived later names an id only the earlier frame stated"
        );
        let mut cells = vec![WireCell::BLANK; 8];
        view.apply(&mut cells, 4, 2);
        assert_eq!(cells[0].link, 1);
        assert_eq!(cells[4].link, 1);
    }

    #[test]
    fn an_overlay_drops_the_links_it_holds_when_it_absorbs_a_full_repaint() {
        let mut first = FrameBuilder::new(8, 2, 1);
        first.push_links(&LinkRecord {
            entries: vec![link(1, "https://before"), link(2, "https://gone")],
        });
        let mut second = FrameBuilder::new(8, 2, 2);
        second.set_full_repaint(true);
        second.push_links(&LinkRecord {
            entries: vec![link(1, "https://after")],
        });

        let mut overlay = PendingOverlay::new(8, 2);
        overlay.merge_frame(&decode(&first.finish()).unwrap());
        overlay.merge_frame(&decode(&second.finish()).unwrap());

        let drained = overlay.drain(3);
        assert_eq!(
            decode(&drained).unwrap().links().entries,
            vec![link(1, "https://after")]
        );
    }

    #[test]
    fn applying_a_frame_to_a_differently_shaped_grid_writes_nothing() {
        let mut builder = FrameBuilder::new(4, 1, 0);
        builder.push_row(0, 0, &run("abcd"));
        let frame = builder.finish();
        let view = decode(&frame).unwrap();
        let mut destination = vec![WireCell::BLANK; 16];
        view.apply(&mut destination, 8, 2);
        assert!(destination.iter().all(|cell| *cell == WireCell::BLANK));
    }
}

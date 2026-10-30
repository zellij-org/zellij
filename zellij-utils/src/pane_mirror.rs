use crate::client_server_contract::client_server_contract as proto;
use crate::data::{KeyWithModifier, MirrorClosedReason, PaneAddress, PaneId};
use crate::input::mouse::MouseEvent;
use crate::ipc::{
    ClientToServerMsg, IpcReceiveError, IpcReceiverWithContext, IpcSenderWithContext,
    ServerToClientMsg,
};
use crate::pane_size::Size;
use crate::structured_render::{
    self, CursorState, DecodeError, FrameBuilder, GeometryRecord, LinkEntry, LinkRecord, PaneRect,
    WireCell, LINK_NONE, MAX_LINK_ID, MAX_LINK_URI_LEN, PANE_WANTS_MOUSE,
};
use prost::Message;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

pub const PANE_MIRROR_VERSION: u32 = 1;
pub const MAX_UNACKED_FRAMES: usize = 2;
pub const MIRROR_CAPABILITIES: &[&str] =
    &["structured-frame", "key", "bytes", "paste", "mouse", "pin"];
pub const MAX_MIRROR_COLS: usize = 1000;
pub const MAX_MIRROR_ROWS: usize = 500;
pub const MIRROR_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const MIRROR_PIN_RENEW_INTERVAL: Duration = Duration::from_secs(2);
pub const MIRROR_PIN_LEASE_INTERVALS: u32 = 3;
pub const MIRROR_PIN_LEASE_ENV_VAR: &str = "ZELLIJ_PANE_MIRROR_PIN_LEASE_MS";

pub fn clamp_mirror_size(size: Size) -> Size {
    Size {
        cols: size.cols.clamp(1, MAX_MIRROR_COLS),
        rows: size.rows.clamp(1, MAX_MIRROR_ROWS),
    }
}

pub fn mirror_pin_lease() -> Duration {
    std::env::var(MIRROR_PIN_LEASE_ENV_VAR)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|millis| *millis > 0)
        .map(Duration::from_millis)
        .unwrap_or(MIRROR_PIN_RENEW_INTERVAL * MIRROR_PIN_LEASE_INTERVALS)
}

pub fn mirror_pin_renew_interval() -> Duration {
    (mirror_pin_lease() / MIRROR_PIN_LEASE_INTERVALS).max(Duration::from_millis(1))
}

pub fn capabilities() -> Vec<String> {
    MIRROR_CAPABILITIES.iter().map(|c| c.to_string()).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct MirrorAttach {
    pub version: u32,
    pub capabilities: Vec<String>,
    pub target_pane_id: PaneId,
    pub viewer_size: Size,
    pub focused: bool,
    pub resize_allowed: bool,
    pub view_only: bool,
    pub viewer_sessions: Vec<String>,
}

impl MirrorAttach {
    pub fn new(target_pane_id: PaneId, viewer_size: Size) -> Self {
        MirrorAttach {
            version: PANE_MIRROR_VERSION,
            capabilities: capabilities(),
            target_pane_id,
            viewer_size,
            focused: false,
            resize_allowed: false,
            view_only: false,
            viewer_sessions: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MirrorInput {
    Key {
        key: KeyWithModifier,
        raw_bytes: Vec<u8>,
        is_kitty_keyboard_protocol: bool,
    },
    Bytes(Vec<u8>),
    Paste(Vec<u8>),
    Mouse(MouseEvent),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViewerMessage {
    Attach(MirrorAttach),
    Input(MirrorInput),
    Resize {
        size: Size,
        focused: bool,
        resize_allowed: bool,
    },
    FrameAck {
        seq: u64,
    },
    Detach,
    Park,
    Unpark {
        resize_allowed: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SourceMessage {
    Attached {
        version: u32,
        capabilities: Vec<String>,
        pane_size: Size,
    },
    Frame {
        frame: Vec<u8>,
        alternate_screen: bool,
    },
    Closed {
        reason: MirrorClosedReason,
        detail: String,
    },
    PinState {
        pinned: bool,
        pane_size: Size,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirrorDecodeError {
    Undecodable(String),
    Empty,
    Malformed(&'static str),
}

impl std::fmt::Display for MirrorDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirrorDecodeError::Undecodable(e) => write!(f, "undecodable mirror message: {}", e),
            MirrorDecodeError::Empty => write!(f, "empty mirror message"),
            MirrorDecodeError::Malformed(what) => write!(f, "malformed mirror message: {}", what),
        }
    }
}

fn size_to_proto(size: Size) -> proto::Size {
    proto::Size {
        cols: size.cols as u32,
        rows: size.rows as u32,
    }
}

fn size_from_proto(size: Option<proto::Size>) -> Size {
    size.map(|size| Size {
        cols: size.cols as usize,
        rows: size.rows as usize,
    })
    .unwrap_or_default()
}

fn close_reason_to_proto(reason: MirrorClosedReason) -> proto::MirrorCloseReason {
    match reason {
        MirrorClosedReason::PaneClosed => proto::MirrorCloseReason::PaneClosed,
        MirrorClosedReason::SessionEnding => proto::MirrorCloseReason::SessionEnding,
        MirrorClosedReason::Rejected => proto::MirrorCloseReason::Rejected,
        MirrorClosedReason::VersionMismatch => proto::MirrorCloseReason::VersionMismatch,
        MirrorClosedReason::PaneNotFound => proto::MirrorCloseReason::PaneNotFound,
        MirrorClosedReason::ContainsViewer => proto::MirrorCloseReason::ContainsViewer,
        MirrorClosedReason::UnsupportedHost
        | MirrorClosedReason::ConnectionFailed
        | MirrorClosedReason::ConnectionLost
        | MirrorClosedReason::ProtocolError => proto::MirrorCloseReason::Unspecified,
    }
}

fn close_reason_from_proto(value: i32) -> MirrorClosedReason {
    match proto::MirrorCloseReason::try_from(value) {
        Ok(proto::MirrorCloseReason::PaneClosed) => MirrorClosedReason::PaneClosed,
        Ok(proto::MirrorCloseReason::SessionEnding) => MirrorClosedReason::SessionEnding,
        Ok(proto::MirrorCloseReason::VersionMismatch) => MirrorClosedReason::VersionMismatch,
        Ok(proto::MirrorCloseReason::PaneNotFound) => MirrorClosedReason::PaneNotFound,
        Ok(proto::MirrorCloseReason::ContainsViewer) => MirrorClosedReason::ContainsViewer,
        Ok(proto::MirrorCloseReason::Rejected)
        | Ok(proto::MirrorCloseReason::Unspecified)
        | Err(_) => MirrorClosedReason::Rejected,
    }
}

impl ViewerMessage {
    pub fn encode(&self) -> Vec<u8> {
        use proto::pane_mirror_viewer_message::Message as M;
        let message = match self {
            ViewerMessage::Attach(attach) => M::Attach(proto::MirrorAttachMsg {
                version: attach.version,
                capabilities: attach.capabilities.clone(),
                target_pane_id: Some(attach.target_pane_id.into()),
                viewer_size: Some(size_to_proto(attach.viewer_size)),
                focused: attach.focused,
                resize_allowed: attach.resize_allowed,
                view_only: attach.view_only,
                viewer_sessions: attach.viewer_sessions.clone(),
            }),
            ViewerMessage::Input(input) => {
                use proto::mirror_input_msg::Input as I;
                let input = match input {
                    MirrorInput::Key {
                        key,
                        raw_bytes,
                        is_kitty_keyboard_protocol,
                    } => I::Key(proto::MirrorKeyInput {
                        key: Some(key.clone().into()),
                        raw_bytes: raw_bytes.clone(),
                        is_kitty_keyboard_protocol: *is_kitty_keyboard_protocol,
                    }),
                    MirrorInput::Bytes(bytes) => I::Bytes(bytes.clone()),
                    MirrorInput::Paste(bytes) => I::Paste(bytes.clone()),
                    MirrorInput::Mouse(event) => I::Mouse((*event).into()),
                };
                M::Input(proto::MirrorInputMsg { input: Some(input) })
            },
            ViewerMessage::Resize {
                size,
                focused,
                resize_allowed,
            } => M::Resize(proto::MirrorResizeMsg {
                size: Some(size_to_proto(*size)),
                focused: *focused,
                resize_allowed: *resize_allowed,
            }),
            ViewerMessage::FrameAck { seq } => M::FrameAck(proto::MirrorFrameAckMsg { seq: *seq }),
            ViewerMessage::Detach => M::Detach(proto::MirrorDetachMsg {}),
            ViewerMessage::Park => M::Park(proto::MirrorParkMsg {}),
            ViewerMessage::Unpark { resize_allowed } => M::Unpark(proto::MirrorUnparkMsg {
                resize_allowed: *resize_allowed,
            }),
        };
        proto::PaneMirrorViewerMessage {
            message: Some(message),
        }
        .encode_to_vec()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, MirrorDecodeError> {
        use proto::pane_mirror_viewer_message::Message as M;
        let message = proto::PaneMirrorViewerMessage::decode(bytes)
            .map_err(|e| MirrorDecodeError::Undecodable(e.to_string()))?;
        match message.message.ok_or(MirrorDecodeError::Empty)? {
            M::Attach(attach) => Ok(ViewerMessage::Attach(MirrorAttach {
                version: attach.version,
                capabilities: attach.capabilities,
                target_pane_id: attach
                    .target_pane_id
                    .ok_or(MirrorDecodeError::Malformed("attach without a target pane"))?
                    .try_into()
                    .map_err(|_| MirrorDecodeError::Malformed("attach target pane"))?,
                viewer_size: size_from_proto(attach.viewer_size),
                focused: attach.focused,
                resize_allowed: attach.resize_allowed,
                view_only: attach.view_only,
                viewer_sessions: attach.viewer_sessions,
            })),
            M::Input(input) => {
                use proto::mirror_input_msg::Input as I;
                let input = match input.input.ok_or(MirrorDecodeError::Empty)? {
                    I::Key(key) => MirrorInput::Key {
                        key: key
                            .key
                            .ok_or(MirrorDecodeError::Malformed("key input without a key"))?
                            .try_into()
                            .map_err(|_| MirrorDecodeError::Malformed("key input"))?,
                        raw_bytes: key.raw_bytes,
                        is_kitty_keyboard_protocol: key.is_kitty_keyboard_protocol,
                    },
                    I::Bytes(bytes) => MirrorInput::Bytes(bytes),
                    I::Paste(bytes) => MirrorInput::Paste(bytes),
                    I::Mouse(event) => MirrorInput::Mouse(
                        event
                            .try_into()
                            .map_err(|_| MirrorDecodeError::Malformed("mouse input"))?,
                    ),
                };
                Ok(ViewerMessage::Input(input))
            },
            M::Resize(resize) => Ok(ViewerMessage::Resize {
                size: size_from_proto(resize.size),
                focused: resize.focused,
                resize_allowed: resize.resize_allowed,
            }),
            M::FrameAck(ack) => Ok(ViewerMessage::FrameAck { seq: ack.seq }),
            M::Detach(_) => Ok(ViewerMessage::Detach),
            M::Park(_) => Ok(ViewerMessage::Park),
            M::Unpark(unpark) => Ok(ViewerMessage::Unpark {
                resize_allowed: unpark.resize_allowed,
            }),
        }
    }
}

impl SourceMessage {
    pub fn encode(&self) -> Vec<u8> {
        use proto::pane_mirror_source_message::Message as M;
        let message = match self {
            SourceMessage::Attached {
                version,
                capabilities,
                pane_size,
            } => M::Attached(proto::MirrorAttachedMsg {
                version: *version,
                capabilities: capabilities.clone(),
                pane_size: Some(size_to_proto(*pane_size)),
            }),
            SourceMessage::Frame {
                frame,
                alternate_screen,
            } => M::Frame(proto::MirrorFrameMsg {
                frame: frame.clone(),
                alternate_screen: *alternate_screen,
            }),
            SourceMessage::Closed { reason, detail } => M::Closed(proto::MirrorClosedMsg {
                reason: close_reason_to_proto(*reason) as i32,
                detail: detail.clone(),
            }),
            SourceMessage::PinState { pinned, pane_size } => {
                M::PinState(proto::MirrorPinStateMsg {
                    pinned: *pinned,
                    pane_size: Some(size_to_proto(*pane_size)),
                })
            },
        };
        proto::PaneMirrorSourceMessage {
            message: Some(message),
        }
        .encode_to_vec()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, MirrorDecodeError> {
        use proto::pane_mirror_source_message::Message as M;
        let message = proto::PaneMirrorSourceMessage::decode(bytes)
            .map_err(|e| MirrorDecodeError::Undecodable(e.to_string()))?;
        match message.message.ok_or(MirrorDecodeError::Empty)? {
            M::Attached(attached) => Ok(SourceMessage::Attached {
                version: attached.version,
                capabilities: attached.capabilities,
                pane_size: size_from_proto(attached.pane_size),
            }),
            M::Frame(frame) => Ok(SourceMessage::Frame {
                frame: frame.frame,
                alternate_screen: frame.alternate_screen,
            }),
            M::Closed(closed) => Ok(SourceMessage::Closed {
                reason: close_reason_from_proto(closed.reason),
                detail: closed.detail,
            }),
            M::PinState(pin_state) => Ok(SourceMessage::PinState {
                pinned: pin_state.pinned,
                pane_size: size_from_proto(pin_state.pane_size),
            }),
        }
    }

    pub fn closed(reason: MirrorClosedReason) -> Self {
        SourceMessage::Closed {
            reason,
            detail: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorCell {
    pub cell: WireCell,
    pub text: Option<String>,
    pub link: Option<String>,
}

impl Default for MirrorCell {
    fn default() -> Self {
        MirrorCell {
            cell: WireCell::BLANK,
            text: None,
            link: None,
        }
    }
}

impl MirrorCell {
    pub fn plain(cell: WireCell) -> Self {
        MirrorCell {
            cell: WireCell {
                link: LINK_NONE,
                ..cell
            },
            text: None,
            link: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorScreen {
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<MirrorCell>,
    pub cursor: CursorState,
    pub wants_mouse: bool,
    pub alternate_screen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorFrameError {
    Decode(DecodeError),
    TooLarge { cols: u16, rows: u16 },
}

impl std::fmt::Display for MirrorFrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MirrorFrameError::Decode(e) => write!(f, "{}", e),
            MirrorFrameError::TooLarge { cols, rows } => write!(
                f,
                "the frame is {}x{}, larger than the {}x{} limit",
                cols, rows, MAX_MIRROR_COLS, MAX_MIRROR_ROWS
            ),
        }
    }
}

impl From<DecodeError> for MirrorFrameError {
    fn from(e: DecodeError) -> Self {
        MirrorFrameError::Decode(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppliedFrame {
    pub seq: u64,
    pub full: bool,
    pub changed_rows: BTreeSet<u16>,
}

impl MirrorScreen {
    pub fn blank(cols: u16, rows: u16) -> Self {
        MirrorScreen {
            cols,
            rows,
            cells: vec![MirrorCell::default(); cols as usize * rows as usize],
            cursor: CursorState::default(),
            wants_mouse: false,
            alternate_screen: false,
        }
    }

    pub fn size(&self) -> Size {
        Size {
            cols: self.cols as usize,
            rows: self.rows as usize,
        }
    }

    pub fn cell(&self, x: u16, y: u16) -> Option<&MirrorCell> {
        if x >= self.cols || y >= self.rows {
            return None;
        }
        self.cells.get(y as usize * self.cols as usize + x as usize)
    }

    pub fn row(&self, y: u16) -> &[MirrorCell] {
        let start = y as usize * self.cols as usize;
        let end = (start + self.cols as usize).min(self.cells.len());
        self.cells.get(start..end).unwrap_or(&[])
    }

    pub fn from_frame(frame: &[u8]) -> Result<Self, MirrorFrameError> {
        let mut screen = MirrorScreen::blank(0, 0);
        screen.apply_frame(frame)?;
        Ok(screen)
    }

    pub fn apply_frame(&mut self, frame: &[u8]) -> Result<AppliedFrame, MirrorFrameError> {
        let view = structured_render::decode(frame)?;
        let header = *view.header();
        if header.cols as usize > MAX_MIRROR_COLS || header.rows as usize > MAX_MIRROR_ROWS {
            return Err(MirrorFrameError::TooLarge {
                cols: header.cols,
                rows: header.rows,
            });
        }
        let resized = header.cols != self.cols || header.rows != self.rows;
        let full = header.full_repaint() || header.clear() || resized;
        if full {
            *self = MirrorScreen {
                wants_mouse: self.wants_mouse,
                alternate_screen: self.alternate_screen,
                ..MirrorScreen::blank(header.cols, header.rows)
            };
        }
        let links: HashMap<u8, String> = view
            .links()
            .entries
            .into_iter()
            .map(|entry| (entry.id, entry.uri))
            .collect();
        let clusters = view.clusters();
        let mut applied = AppliedFrame {
            seq: header.seq,
            full,
            changed_rows: BTreeSet::new(),
        };
        for (record, cells) in view.rows() {
            applied.changed_rows.insert(record.y);
            for (offset, wire) in cells.into_iter().enumerate() {
                let x = record.x0 as usize + offset;
                if x >= self.cols as usize {
                    break;
                }
                let index = record.y as usize * self.cols as usize + x;
                let text = wire.cluster_id().map(|_| wire.text(&clusters).into_owned());
                let link = if wire.link == LINK_NONE {
                    None
                } else {
                    links.get(&wire.link).cloned()
                };
                let cell = WireCell {
                    ch: if text.is_some() { 0 } else { wire.ch },
                    link: LINK_NONE,
                    ..wire
                };
                if let Some(slot) = self.cells.get_mut(index) {
                    *slot = MirrorCell { cell, text, link };
                }
            }
        }
        if full {
            applied.changed_rows = (0..self.rows).collect();
        }
        self.cursor = view.cursor();
        if let Some(geometry) = view.geometry() {
            self.wants_mouse = geometry
                .panes
                .first()
                .map(|pane| pane.wants_mouse())
                .unwrap_or(false);
        }
        Ok(applied)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    pub seq: u64,
    pub full: bool,
    pub alternate_screen: bool,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct MirrorFrameEncoder {
    last: Option<MirrorScreen>,
    next_seq: u64,
    force_full: bool,
}

impl MirrorFrameEncoder {
    pub fn new() -> Self {
        MirrorFrameEncoder::default()
    }

    pub fn request_full_repaint(&mut self) {
        self.force_full = true;
    }

    pub fn encode(&mut self, current: &MirrorScreen) -> Option<EncodedFrame> {
        let previous = self
            .last
            .as_ref()
            .filter(|last| last.cols == current.cols && last.rows == current.rows);
        let full = self.force_full || previous.is_none();
        let mut spans: Vec<(u16, u16, u16)> = Vec::new();
        for y in 0..current.rows {
            let row = current.row(y);
            match previous {
                Some(previous) if !full => {
                    let old = previous.row(y);
                    let first = row.iter().zip(old).position(|(new, old)| new != old);
                    if let Some(first) = first {
                        let last = row
                            .iter()
                            .zip(old)
                            .rposition(|(new, old)| new != old)
                            .unwrap_or(first);
                        spans.push((y, first as u16, last as u16 + 1));
                    }
                },
                _ => {
                    if current.cols > 0 {
                        spans.push((y, 0, current.cols));
                    }
                },
            }
        }
        let cursor_changed = previous.map(|p| p.cursor != current.cursor).unwrap_or(true);
        let mouse_changed = previous
            .map(|p| p.wants_mouse != current.wants_mouse)
            .unwrap_or(true);
        let alternate_screen_changed = previous
            .map(|p| p.alternate_screen != current.alternate_screen)
            .unwrap_or(true);
        if !full
            && spans.is_empty()
            && !cursor_changed
            && !mouse_changed
            && !alternate_screen_changed
        {
            return None;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        let mut builder = FrameBuilder::new(current.cols, current.rows, seq);
        builder.set_full_repaint(full);
        builder.set_clear(full);
        builder.set_cursor(current.cursor);
        if full || mouse_changed {
            builder.push_geometry(&GeometryRecord {
                panes: vec![PaneRect {
                    x: 0,
                    y: 0,
                    cols: current.cols,
                    rows: current.rows,
                    flags: if current.wants_mouse {
                        PANE_WANTS_MOUSE
                    } else {
                        0
                    },
                    ..Default::default()
                }],
            });
        }
        let mut link_ids: HashMap<String, u8> = HashMap::new();
        let mut links = LinkRecord::default();
        let mut run: Vec<WireCell> = Vec::new();
        for (y, start, end) in spans {
            run.clear();
            for cell in &current.row(y)[start as usize..end as usize] {
                let mut wire = cell.cell;
                if let Some(text) = cell.text.as_deref() {
                    let first = text.chars().next().unwrap_or(' ') as u32;
                    wire.ch = builder.cluster(text).unwrap_or(first);
                }
                wire.link = match cell.link.as_deref() {
                    Some(uri) if !uri.is_empty() && uri.len() <= MAX_LINK_URI_LEN => {
                        match link_ids.get(uri) {
                            Some(id) => *id,
                            None if link_ids.len() < MAX_LINK_ID as usize => {
                                let id = link_ids.len() as u8 + 1;
                                link_ids.insert(uri.to_owned(), id);
                                links.entries.push(LinkEntry {
                                    id,
                                    uri: uri.to_owned(),
                                });
                                id
                            },
                            None => LINK_NONE,
                        }
                    },
                    _ => LINK_NONE,
                };
                run.push(wire);
            }
            builder.push_row(y, start, &run);
        }
        builder.push_links(&links);
        self.force_full = false;
        self.last = Some(current.clone());
        Some(EncodedFrame {
            seq,
            full,
            alternate_screen: current.alternate_screen,
            bytes: builder.finish(),
        })
    }
}

#[derive(Debug)]
pub struct MirrorFramePacer {
    encoder: MirrorFrameEncoder,
    in_flight: VecDeque<u64>,
    skipped: bool,
    limit: usize,
    sent_change: Option<u64>,
}

impl Default for MirrorFramePacer {
    fn default() -> Self {
        MirrorFramePacer::new(MAX_UNACKED_FRAMES)
    }
}

impl MirrorFramePacer {
    pub fn new(limit: usize) -> Self {
        MirrorFramePacer {
            encoder: MirrorFrameEncoder::new(),
            in_flight: VecDeque::new(),
            skipped: false,
            limit: limit.max(1),
            sent_change: None,
        }
    }

    pub fn unacknowledged(&self) -> usize {
        self.in_flight.len()
    }

    pub fn has_skipped(&self) -> bool {
        self.skipped
    }

    fn is_full(&self) -> bool {
        self.in_flight.len() >= self.limit
    }

    pub fn wants_frame(&mut self, change: u64) -> bool {
        let pending = self.sent_change != Some(change) || self.skipped || self.encoder.force_full;
        if !pending {
            return false;
        }
        if self.is_full() {
            self.skipped = true;
            return false;
        }
        true
    }

    pub fn request_full_repaint(&mut self) {
        self.encoder.request_full_repaint();
    }

    pub fn next_frame(&mut self, change: u64, current: &MirrorScreen) -> Option<EncodedFrame> {
        if self.is_full() {
            self.skipped = true;
            return None;
        }
        if self.skipped {
            self.skipped = false;
            self.encoder.request_full_repaint();
        }
        self.sent_change = Some(change);
        let frame = self.encoder.encode(current)?;
        self.in_flight.push_back(frame.seq);
        Some(frame)
    }

    pub fn acknowledge(&mut self, seq: u64) -> bool {
        self.in_flight.retain(|in_flight| *in_flight > seq);
        self.skipped && !self.is_full()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MirrorTransportEvent {
    Message(SourceMessage),
    ConnectFailed(String),
    Unreadable(String),
    Disconnected,
}

pub type MirrorEventSink = Arc<dyn Fn(MirrorTransportEvent) + Send + Sync>;

pub trait MirrorTransport: Send {
    fn send(&self, message: ViewerMessage) -> bool;
    fn close(&mut self);
}

pub trait MirrorConnector: Send + Sync {
    fn connect(
        &self,
        address: &PaneAddress,
        attach: MirrorAttach,
        sink: MirrorEventSink,
    ) -> Box<dyn MirrorTransport>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LocalSocketConnector;

pub struct LocalSocketTransport {
    outgoing: Option<Sender<ViewerMessage>>,
}

impl MirrorTransport for LocalSocketTransport {
    fn send(&self, message: ViewerMessage) -> bool {
        self.outgoing
            .as_ref()
            .map(|outgoing| outgoing.send(message).is_ok())
            .unwrap_or(false)
    }
    fn close(&mut self) {
        if let Some(outgoing) = self.outgoing.take() {
            let _ = outgoing.send(ViewerMessage::Detach);
        }
    }
}

impl Drop for LocalSocketTransport {
    fn drop(&mut self) {
        self.close();
    }
}

impl MirrorConnector for LocalSocketConnector {
    fn connect(
        &self,
        address: &PaneAddress,
        attach: MirrorAttach,
        sink: MirrorEventSink,
    ) -> Box<dyn MirrorTransport> {
        let (outgoing, receiver) = channel();
        let session = address.session.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("pane_mirror_{}", session))
            .spawn({
                let sink = sink.clone();
                move || run_local_connection(session, attach, receiver, sink)
            });
        if let Err(e) = spawned {
            sink(MirrorTransportEvent::ConnectFailed(e.to_string()));
        }
        Box::new(LocalSocketTransport {
            outgoing: Some(outgoing),
        })
    }
}

fn wrap_viewer_message(message: &ViewerMessage) -> ClientToServerMsg {
    ClientToServerMsg::PaneMirror {
        payload: message.encode(),
    }
}

fn run_local_connection(
    session: String,
    attach: MirrorAttach,
    outgoing: Receiver<ViewerMessage>,
    sink: MirrorEventSink,
) {
    let socket_path = crate::consts::session_socket_path(&session);
    let stream = match crate::consts::ipc_connect(&socket_path) {
        Ok(stream) => stream,
        Err(e) => {
            sink(MirrorTransportEvent::ConnectFailed(e.to_string()));
            return;
        },
    };
    #[cfg(windows)]
    let reply_stream = match crate::consts::ipc_connect_reply(&socket_path) {
        Ok(stream) => stream,
        Err(e) => {
            sink(MirrorTransportEvent::ConnectFailed(e.to_string()));
            return;
        },
    };
    let mut sender = IpcSenderWithContext::<ClientToServerMsg>::new(stream);
    #[cfg(windows)]
    let receiver: IpcReceiverWithContext<ServerToClientMsg> =
        IpcReceiverWithContext::new(reply_stream);
    #[cfg(not(windows))]
    let receiver: IpcReceiverWithContext<ServerToClientMsg> = sender.get_receiver();
    if let Err(e) = sender.send_client_msg(wrap_viewer_message(&ViewerMessage::Attach(attach))) {
        sink(MirrorTransportEvent::ConnectFailed(e.to_string()));
        return;
    }
    let reader_sink = sink.clone();
    let reader = std::thread::Builder::new()
        .name(format!("pane_mirror_reader_{}", session))
        .spawn(move || read_source_messages(receiver, reader_sink));
    if reader.is_err() {
        sink(MirrorTransportEvent::Disconnected);
        return;
    }
    let mut detached = false;
    for message in outgoing.iter() {
        let is_detach = matches!(message, ViewerMessage::Detach);
        if sender
            .send_client_msg(wrap_viewer_message(&message))
            .is_err()
        {
            return;
        }
        if is_detach {
            detached = true;
            break;
        }
    }
    if !detached {
        let _ = sender.send_client_msg(wrap_viewer_message(&ViewerMessage::Detach));
    }
}

fn read_source_messages(
    mut receiver: IpcReceiverWithContext<ServerToClientMsg>,
    sink: MirrorEventSink,
) {
    loop {
        match receiver.try_recv_server_msg() {
            Ok((ServerToClientMsg::PaneMirror { payload }, _)) => {
                match SourceMessage::decode(&payload) {
                    Ok(message) => {
                        let closed = matches!(message, SourceMessage::Closed { .. });
                        sink(MirrorTransportEvent::Message(message));
                        if closed {
                            return;
                        }
                    },
                    Err(e) => {
                        sink(MirrorTransportEvent::Unreadable(e.to_string()));
                        return;
                    },
                }
            },
            Ok(_) => {},
            Err(IpcReceiveError::Undecodable) => {
                sink(MirrorTransportEvent::Unreadable(
                    "undecodable message from the source session".to_owned(),
                ));
                return;
            },
            Err(IpcReceiveError::Disconnected) => {
                sink(MirrorTransportEvent::Disconnected);
                return;
            },
        }
    }
}

#[cfg(test)]
#[path = "./unit/pane_mirror_tests.rs"]
mod pane_mirror_tests;

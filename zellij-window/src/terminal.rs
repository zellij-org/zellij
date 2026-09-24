use zellij_utils::structured_render::{
    self, DecodeError, GeometryRecord, GraphicsMedium, GraphicsRecord, GRAPHICS_FORMAT_RGBA8,
};

use std::collections::HashMap;

#[cfg(test)]
use crate::dump;
use crate::kitty::{
    self, ApcFilter, Graphics, Image, Kind, Notification, NotificationChunk, OscSignal, Placement,
};
use crate::retained::Damage;
use crate::screen_buffer::TermSize;
use crate::screen_buffer::{ApplyError, Blink, CursorShape, Occupancy, ScreenBuffer};
use crate::sixel::{CellSize, Sixels};

const DEFAULT_CELL: CellSize = CellSize {
    width: 10,
    height: 20,
};

#[derive(Debug)]
pub enum FrameError {
    Undecodable(DecodeError),
    Unapplicable { seq: u64, error: ApplyError },
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Undecodable(e) => write!(f, "{}", e),
            FrameError::Unapplicable { error, .. } => write!(f, "{}", error),
        }
    }
}

impl std::error::Error for FrameError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameApplied {
    pub seq: u64,
    pub damage: Damage,
}

const MAX_PENDING_NOTIFICATIONS: usize = 64;
const MAX_NOTIFICATION_BYTES: usize = 4096;

#[derive(Default)]
struct Signals {
    clipboard: Option<String>,
    title: Option<Option<String>>,
    bells: usize,
    notifications: Vec<Notification>,
}

#[derive(Default)]
struct NotificationAssembly {
    pending: HashMap<String, (String, String)>,
    order: Vec<String>,
}

impl NotificationAssembly {
    fn feed(&mut self, chunk: NotificationChunk) -> Option<Notification> {
        let Some(id) = chunk.id else {
            return assembled(String::new(), chunk.text);
        };
        if !self.pending.contains_key(&id) {
            if self.order.len() >= MAX_PENDING_NOTIFICATIONS {
                let oldest = self.order.remove(0);
                self.pending.remove(&oldest);
            }
            self.order.push(id.clone());
        }
        let entry = self.pending.entry(id.clone()).or_default();
        let destination = match chunk.title {
            true => &mut entry.0,
            false => &mut entry.1,
        };
        let room = MAX_NOTIFICATION_BYTES.saturating_sub(destination.len());
        destination.push_str(&truncated(&chunk.text, room));
        if !chunk.done {
            return None;
        }
        let (title, body) = self.pending.remove(&id)?;
        self.order.retain(|pending| pending != &id);
        assembled(title, body)
    }
}

fn truncated(text: &str, room: usize) -> String {
    if text.len() <= room {
        return text.to_owned();
    }
    let mut end = room;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

fn assembled(title: String, body: String) -> Option<Notification> {
    match (title.is_empty(), body.is_empty()) {
        (true, true) => None,
        (false, true) => Some(Notification {
            title: None,
            body: title,
        }),
        (true, false) => Some(Notification { title: None, body }),
        (false, false) => Some(Notification {
            title: Some(title),
            body,
        }),
    }
}

pub struct TerminalState {
    screen: ScreenBuffer,
    filter: ApcFilter,
    graphics: Graphics,
    sixels: Sixels,
    signals: Signals,
    assembly: NotificationAssembly,
    geometry: GeometryRecord,
    geometry_stamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsStamp {
    geometry: u64,
    sixels: u64,
    kitty: u64,
}

impl TerminalState {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            screen: ScreenBuffer::new(rows, cols),
            filter: ApcFilter::new(),
            graphics: Graphics::new(),
            sixels: Sixels::new(DEFAULT_CELL),
            signals: Signals::default(),
            assembly: NotificationAssembly::default(),
            geometry: GeometryRecord::default(),
            geometry_stamp: 0,
        }
    }

    pub fn geometry(&self) -> &GeometryRecord {
        &self.geometry
    }

    pub fn graphics_stamp(&self) -> GraphicsStamp {
        GraphicsStamp {
            geometry: self.geometry_stamp,
            sixels: self.sixels.stamp(),
            kitty: self.graphics.stamp(),
        }
    }

    pub fn take_copied_text(&mut self) -> Option<String> {
        self.signals.clipboard.take()
    }

    pub fn take_title(&mut self) -> Option<Option<String>> {
        self.signals.title.take()
    }

    pub fn take_bells(&mut self) -> usize {
        std::mem::take(&mut self.signals.bells)
    }

    pub fn take_notifications(&mut self) -> Vec<Notification> {
        std::mem::take(&mut self.signals.notifications)
    }

    pub fn apply_frame(&mut self, frame: &[u8]) -> Result<FrameApplied, FrameError> {
        let view = structured_render::decode(frame).map_err(FrameError::Undecodable)?;
        let applied = self
            .screen
            .apply(&view)
            .map_err(|error| FrameError::Unapplicable {
                seq: view.header().seq,
                error,
            })?;
        if applied.cleared {
            self.graphics.clear();
            self.sixels.clear();
        }
        let mut everything = applied.cleared || applied.reshaped;
        if let Some(geometry) = view.geometry() {
            if geometry != self.geometry {
                everything = true;
                self.geometry_stamp += 1;
            }
            self.geometry = geometry;
        }
        self.consume_sideband(view.sideband());
        self.consume_graphics(&view.graphics());
        self.sixels.cut_spans(&applied.written);
        self.sixels.commit();

        let damage = if everything {
            Damage::Everything
        } else {
            let mut rows: Vec<usize> = applied.written.iter().map(|span| span.line).collect();
            if let Some((left, entered)) = applied.cursor_rows {
                rows.push(left);
                rows.push(entered);
            }
            Damage::Rows(rows)
        };
        Ok(FrameApplied {
            seq: applied.seq,
            damage,
        })
    }

    fn consume_graphics(&mut self, records: &[GraphicsRecord]) {
        for record in records {
            match record {
                GraphicsRecord::Clear => {
                    self.graphics.clear();
                    self.sixels.clear();
                },
                GraphicsRecord::Residency {
                    id,
                    width,
                    height,
                    format,
                    medium,
                    byte_len,
                    data,
                } => {
                    if *format != GRAPHICS_FORMAT_RGBA8 {
                        eprintln!(
                            "zellij-window: image {} arrived in unknown format {}",
                            id, format
                        );
                        continue;
                    }
                    let expected = *byte_len as usize;
                    let pixels = match medium {
                        GraphicsMedium::Inline => {
                            (data.len() >= expected).then(|| data[..expected].to_vec())
                        },
                        GraphicsMedium::SharedFile => match std::str::from_utf8(data) {
                            Ok(path) => kitty::read_shared_media(path, expected),
                            Err(_) => None,
                        },
                    };
                    let Some(pixels) = pixels else {
                        continue;
                    };
                    if pixels.len() < *width as usize * *height as usize * 4 {
                        eprintln!(
                            "zellij-window: image {} carried {} bytes for {}x{}",
                            id,
                            pixels.len(),
                            width,
                            height
                        );
                        continue;
                    }
                    self.graphics.insert_image(
                        *id as u32,
                        Image {
                            width: *width,
                            height: *height,
                            pixels,
                        },
                    );
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
                } => self.graphics.insert_placement(Placement {
                    image_id: *image_id as u32,
                    placement_id: *placement_id as u32,
                    cell_x: *cell_x as usize,
                    cell_y: *cell_y as usize,
                    offset_x: *offset_x,
                    offset_y: *offset_y,
                    source_x: *source_x,
                    source_y: *source_y,
                    source_width: *source_width,
                    source_height: *source_height,
                    z: *z,
                }),
                GraphicsRecord::DeleteImage { id } => self.graphics.remove_image(*id as u32),
                GraphicsRecord::DeletePlacement {
                    image_id,
                    placement_id,
                } => self
                    .graphics
                    .remove_placement(*image_id as u32, *placement_id as u32),
                GraphicsRecord::SixelChunk {
                    cell_x,
                    cell_y,
                    payload,
                    ..
                } => self
                    .sixels
                    .execute(payload, (*cell_y as usize, *cell_x as usize)),
            }
        }
    }

    fn consume_sideband(&mut self, sideband: &[u8]) {
        for byte in sideband {
            let step = self.filter.advance(*byte);
            match step.kind {
                Kind::Forward | Kind::Cleared => {},
                Kind::Boundary => {
                    if step.bytes.as_slice() == [0x07] {
                        self.signals.bells += 1;
                    }
                },
                Kind::Signal(signal) => match signal {
                    OscSignal::Notification(chunk) => {
                        if let Some(notification) = self.assembly.feed(chunk) {
                            self.signals.notifications.push(notification);
                        }
                    },
                    OscSignal::Title(title) => self.signals.title = Some(title),
                    OscSignal::Clipboard(text) => self.signals.clipboard = Some(text),
                },
            }
        }
    }

    pub fn set_cell_size(&mut self, width: u32, height: u32) {
        self.sixels.set_cell_size(CellSize::new(width, height));
    }

    pub fn size(&self) -> TermSize {
        self.screen.size()
    }

    pub fn screen(&self) -> &ScreenBuffer {
        &self.screen
    }

    pub fn cursor_is_visible(&self) -> bool {
        self.screen.cursor_is_visible()
    }

    pub fn cursor_position(&self) -> (usize, usize) {
        self.screen.cursor_position()
    }

    pub fn cursor_shape(&self) -> CursorShape {
        self.screen.cursor_shape()
    }

    pub fn cursor_is_blinking(&self) -> bool {
        self.screen.cursor_is_blinking()
    }

    pub fn occupancy(&self, row: usize, col: usize) -> Occupancy {
        self.screen.occupancy(row, col)
    }

    pub fn graphics(&self) -> &Graphics {
        &self.graphics
    }

    pub fn sixels(&self) -> &Sixels {
        &self.sixels
    }

    pub fn blink_at(&self, row: usize, col: usize) -> Blink {
        self.screen.blink_at(row, col)
    }

    pub fn has_blinking_cells(&self) -> bool {
        self.screen.has_blinking_cells()
    }

    #[cfg(test)]
    pub fn dump(&self) -> String {
        dump::dump(self)
    }
}

pub fn clamped(rows: usize, cols: usize) -> TermSize {
    TermSize {
        rows: rows.max(1),
        cols: cols.max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::structured_render::{CursorState, FrameBuilder, WireCell, CURSOR_SHAPE_BEAM};

    fn run(text: &str) -> Vec<WireCell> {
        text.chars()
            .map(|character| WireCell {
                ch: character as u32,
                ..WireCell::BLANK
            })
            .collect()
    }

    fn frame_of(cols: u16, rows: u16, seq: u64, sideband: &[u8]) -> Vec<u8> {
        let mut builder = FrameBuilder::new(cols, rows, seq);
        builder.sideband_mut().extend_from_slice(sideband);
        builder.finish()
    }

    fn sixel_image(width: usize, bands: usize) -> Vec<u8> {
        let mut body = format!("\u{1b}P0;1;0q\"1;1;{};{}", width, bands * 6);
        for _ in 0..bands {
            body.push_str(&format!("#0;2;100;0;0!{}~-", width));
        }
        body.push_str("\u{1b}\\");
        body.into_bytes()
    }

    fn row_text(state: &TerminalState, row: usize) -> String {
        (0..state.size().cols)
            .map(|col| state.screen().cell(row, col).character())
            .collect()
    }

    #[test]
    fn a_frame_paints_the_grid_and_reports_its_sequence_number() {
        let mut state = TerminalState::new(2, 8);
        let mut builder = FrameBuilder::new(8, 2, 11);
        builder.push_row(0, 0, &run("hello"));
        builder.set_cursor(CursorState {
            x: 5,
            y: 0,
            shape: CURSOR_SHAPE_BEAM,
            visible: true,
        });
        assert_eq!(state.apply_frame(&builder.finish()).unwrap().seq, 11);
        assert_eq!(row_text(&state, 0), "hello   ");
        assert_eq!(state.cursor_position(), (0, 5));
        assert!(state.cursor_is_visible());
        assert_eq!(state.cursor_shape(), CursorShape::Beam);
    }

    #[test]
    fn a_malformed_frame_is_refused_rather_than_partly_applied() {
        let mut state = TerminalState::new(2, 8);
        let mut builder = FrameBuilder::new(8, 2, 1);
        builder.push_row(0, 0, &run("kept"));
        state.apply_frame(&builder.finish()).unwrap();

        assert!(matches!(
            state.apply_frame(b"nonsense"),
            Err(FrameError::Undecodable(_))
        ));
        let mut reshaped = FrameBuilder::new(4, 1, 2);
        reshaped.push_row(0, 0, &run("no"));
        assert!(matches!(
            state.apply_frame(&reshaped.finish()),
            Err(FrameError::Unapplicable { seq: 2, .. })
        ));
        assert_eq!(row_text(&state, 0), "kept    ");
    }

    #[test]
    fn the_frame_header_rather_than_the_client_decides_the_viewport() {
        let mut state = TerminalState::new(2, 8);
        let mut builder = FrameBuilder::new(20, 5, 0);
        builder.set_full_repaint(true);
        builder.push_row(4, 0, &run("wider"));
        state.apply_frame(&builder.finish()).unwrap();

        assert_eq!(state.size(), TermSize { rows: 5, cols: 20 });
        assert_eq!(row_text(&state, 4), "wider               ");
    }

    #[test]
    fn a_frame_claiming_an_empty_viewport_is_dropped_rather_than_collapsing_the_grid() {
        let mut state = TerminalState::new(2, 8);
        let mut builder = FrameBuilder::new(0, 0, 4);
        builder.set_full_repaint(true);
        assert!(matches!(
            state.apply_frame(&builder.finish()),
            Err(FrameError::Unapplicable { seq: 4, .. })
        ));
        assert_eq!(state.size(), TermSize { rows: 2, cols: 8 });
    }

    #[test]
    fn a_title_reaches_the_signals_through_the_sideband() {
        let mut state = TerminalState::new(2, 8);
        state
            .apply_frame(&frame_of(8, 2, 0, b"\x1b]0;zellij - fish\x07"))
            .unwrap();
        assert_eq!(state.take_title(), Some(Some("zellij - fish".to_owned())));
        assert_eq!(state.take_title(), None);
    }

    #[test]
    fn an_empty_title_resets_it() {
        let mut state = TerminalState::new(2, 8);
        state
            .apply_frame(&frame_of(8, 2, 0, b"\x1b]2;\x1b\\"))
            .unwrap();
        assert_eq!(state.take_title(), Some(None));
    }

    #[test]
    fn bells_are_counted_and_drained() {
        let mut state = TerminalState::new(2, 8);
        state.apply_frame(&frame_of(8, 2, 0, b"\x07\x07")).unwrap();
        assert_eq!(state.take_bells(), 2);
        assert_eq!(state.take_bells(), 0);
    }

    #[test]
    fn osc_9_and_osc_99_notifications_are_captured_with_their_bodies() {
        let mut state = TerminalState::new(2, 8);
        state
            .apply_frame(&frame_of(8, 2, 0, b"\x1b]9;a job finished\x07"))
            .unwrap();
        state
            .apply_frame(&frame_of(8, 2, 1, b"\x1b]99;i=1:p=body;the body\x1b\\"))
            .unwrap();
        assert_eq!(
            state.take_notifications(),
            vec![
                Notification {
                    title: None,
                    body: "a job finished".to_owned()
                },
                Notification {
                    title: None,
                    body: "the body".to_owned()
                }
            ]
        );
        assert!(state.take_notifications().is_empty());
    }

    #[test]
    fn an_osc_99_title_and_body_arrive_as_one_notification() {
        let mut state = TerminalState::new(2, 8);
        state
            .apply_frame(&frame_of(
                8,
                2,
                0,
                b"\x1b]99;i=7:d=0:p=title;Build\x1b\\\x1b]99;i=7:p=body;finished\x1b\\",
            ))
            .unwrap();
        assert_eq!(
            state.take_notifications(),
            vec![Notification {
                title: Some("Build".to_owned()),
                body: "finished".to_owned()
            }],
            "the two chunks of one identifier are one notification, not two"
        );
    }

    #[test]
    fn an_unfinished_osc_99_notification_waits_for_its_last_chunk() {
        let mut state = TerminalState::new(2, 8);
        state
            .apply_frame(&frame_of(8, 2, 0, b"\x1b]99;i=3:d=0:p=body;half \x1b\\"))
            .unwrap();
        assert!(
            state.take_notifications().is_empty(),
            "a chunk that says more is coming must not be shown on its own"
        );
        state
            .apply_frame(&frame_of(8, 2, 1, b"\x1b]99;i=3:p=body;done\x1b\\"))
            .unwrap();
        assert_eq!(
            state.take_notifications(),
            vec![Notification {
                title: None,
                body: "half done".to_owned()
            }]
        );
    }

    #[test]
    fn an_osc_99_body_encoded_in_base64_is_decoded() {
        let mut state = TerminalState::new(2, 8);
        state
            .apply_frame(&frame_of(8, 2, 0, b"\x1b]99;i=1:e=1:p=body;ZG9uZQ==\x1b\\"))
            .unwrap();
        assert_eq!(
            state.take_notifications(),
            vec![Notification {
                title: None,
                body: "done".to_owned()
            }]
        );
    }

    #[test]
    fn an_osc_99_notification_that_is_only_a_title_is_shown_as_a_body() {
        let mut state = TerminalState::new(2, 8);
        state
            .apply_frame(&frame_of(8, 2, 0, b"\x1b]99;;Build: finished\x1b\\"))
            .unwrap();
        assert_eq!(
            state.take_notifications(),
            vec![Notification {
                title: None,
                body: "Build: finished".to_owned()
            }],
            "the server joins title and body when it re-encodes, and there is nothing to split"
        );
    }

    #[test]
    fn an_osc_99_control_request_is_not_a_notification() {
        let mut state = TerminalState::new(2, 8);
        state
            .apply_frame(&frame_of(8, 2, 0, b"\x1b]99;i=1:p=close;\x1b\\"))
            .unwrap();
        assert!(
            state.take_notifications().is_empty(),
            "a close or query is a protocol request, not something to show"
        );
    }

    #[test]
    fn an_osc_52_store_reaches_the_clipboard_and_drains_once() {
        let mut state = TerminalState::new(2, 8);
        state
            .apply_frame(&frame_of(8, 2, 0, b"\x1b]52;c;Y29waWVk\x1b\\"))
            .unwrap();
        assert_eq!(state.take_copied_text().as_deref(), Some("copied"));
        assert_eq!(state.take_copied_text(), None);
    }

    #[test]
    fn an_osc_52_read_request_is_not_mistaken_for_a_store() {
        let mut state = TerminalState::new(2, 8);
        state
            .apply_frame(&frame_of(8, 2, 0, b"\x1b]52;c;?\x1b\\"))
            .unwrap();
        assert_eq!(state.take_copied_text(), None);
    }

    fn rgba(pixel: [u8; 4], count: usize) -> Vec<u8> {
        pixel.repeat(count)
    }

    fn residency(id: u64, medium: GraphicsMedium, data: Vec<u8>) -> GraphicsRecord {
        GraphicsRecord::Residency {
            id,
            width: 2,
            height: 2,
            format: GRAPHICS_FORMAT_RGBA8,
            medium,
            byte_len: 16,
            data,
        }
    }

    fn placement(image_id: u64, placement_id: u64) -> GraphicsRecord {
        GraphicsRecord::Placement {
            image_id,
            placement_id,
            cell_x: 2,
            cell_y: 3,
            offset_x: 0,
            offset_y: 0,
            source_x: 0,
            source_y: 0,
            source_width: 2,
            source_height: 2,
            z: 0,
        }
    }

    fn apply_graphics(state: &mut TerminalState, records: &[GraphicsRecord]) {
        let size = state.size();
        let mut builder = FrameBuilder::new(size.cols as u16, size.rows as u16, 0);
        builder.extend_graphics(records.iter());
        state
            .apply_frame(&builder.finish())
            .expect("a usable frame");
    }

    #[test]
    fn an_inline_residency_and_its_placement_reach_the_window_as_pixels() {
        let mut state = TerminalState::new(6, 12);
        let pixels = rgba([200, 10, 10, 255], 4);
        apply_graphics(
            &mut state,
            &[
                residency(7, GraphicsMedium::Inline, pixels.clone()),
                placement(7, 1),
            ],
        );

        let image = state.graphics().image(7).expect("no resident image");
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(image.pixels, pixels);
        let placed = *state.graphics().placements().next().expect("no placement");
        assert_eq!((placed.cell_y, placed.cell_x), (3, 2));
        assert_eq!((placed.source_width, placed.source_height), (2, 2));
    }

    #[test]
    fn a_shared_file_residency_is_read_from_the_path_it_names() {
        let directory = std::env::temp_dir();
        let path = directory.join(format!("zellij-window-test-{}", std::process::id()));
        let pixels = rgba([0, 0, 255, 255], 4);
        std::fs::write(&path, &pixels).unwrap();

        let mut state = TerminalState::new(6, 12);
        apply_graphics(
            &mut state,
            &[
                residency(
                    9,
                    GraphicsMedium::SharedFile,
                    path.to_string_lossy().into_owned().into_bytes(),
                ),
                placement(9, 1),
            ],
        );
        let _ = std::fs::remove_file(&path);

        assert_eq!(state.graphics().image(9).unwrap().pixels, pixels);
        assert_eq!(state.graphics().placements().count(), 1);
    }

    #[test]
    fn a_residency_naming_a_path_outside_the_media_directories_is_refused() {
        let mut state = TerminalState::new(6, 12);
        apply_graphics(
            &mut state,
            &[residency(
                9,
                GraphicsMedium::SharedFile,
                b"/etc/passwd".to_vec(),
            )],
        );
        assert!(state.graphics().image(9).is_none());
    }

    #[test]
    fn a_placement_of_an_image_that_never_arrived_is_dropped() {
        let mut state = TerminalState::new(6, 12);
        apply_graphics(&mut state, &[placement(3, 1)]);
        assert_eq!(state.graphics().placements().count(), 0);
    }

    #[test]
    fn deletions_retire_placements_and_images_independently() {
        let mut state = TerminalState::new(6, 12);
        apply_graphics(
            &mut state,
            &[
                residency(1, GraphicsMedium::Inline, rgba([1, 2, 3, 255], 4)),
                placement(1, 1),
                placement(1, 2),
            ],
        );
        assert_eq!(state.graphics().placements().count(), 2);

        apply_graphics(
            &mut state,
            &[GraphicsRecord::DeletePlacement {
                image_id: 1,
                placement_id: 2,
            }],
        );
        assert_eq!(state.graphics().placements().count(), 1);
        assert!(state.graphics().image(1).is_some());

        apply_graphics(&mut state, &[GraphicsRecord::DeleteImage { id: 1 }]);
        assert_eq!(state.graphics().placements().count(), 0);
        assert!(state.graphics().image(1).is_none());
    }

    #[test]
    fn a_clear_record_retires_every_image_and_placement() {
        let mut state = TerminalState::new(6, 12);
        apply_graphics(
            &mut state,
            &[
                residency(1, GraphicsMedium::Inline, rgba([1, 2, 3, 255], 4)),
                placement(1, 1),
            ],
        );
        apply_graphics(&mut state, &[GraphicsRecord::Clear]);
        assert!(state.graphics().resident_ids().is_empty());
        assert_eq!(state.graphics().placements().count(), 0);
    }

    #[test]
    fn a_typed_sixel_chunk_lands_at_the_cell_its_record_names() {
        let mut state = TerminalState::new(6, 12);
        state.set_cell_size(8, 20);
        apply_graphics(
            &mut state,
            &[GraphicsRecord::SixelChunk {
                cell_x: 3,
                cell_y: 2,
                pixel_x: 0,
                pixel_y: 0,
                pixel_width: 16,
                pixel_height: 6,
                payload: sixel_image(16, 1),
            }],
        );

        let chunk = state.sixels().chunks().next().expect("no chunk");
        assert_eq!((chunk.cell_y, chunk.cell_x), (2, 3));
        assert_eq!((chunk.width, chunk.height), (16, 6));
    }

    #[test]
    fn a_residency_whose_pixels_fall_short_of_its_dimensions_is_refused() {
        let mut state = TerminalState::new(6, 12);
        apply_graphics(
            &mut state,
            &[GraphicsRecord::Residency {
                id: 5,
                width: 8,
                height: 8,
                format: GRAPHICS_FORMAT_RGBA8,
                medium: GraphicsMedium::Inline,
                byte_len: 16,
                data: rgba([1, 2, 3, 255], 4),
            }],
        );
        assert!(state.graphics().image(5).is_none());
    }

    fn sixel_at(cell_y: u32, cell_x: u32, width: u32, bands: u32) -> GraphicsRecord {
        GraphicsRecord::SixelChunk {
            cell_x,
            cell_y,
            pixel_x: 0,
            pixel_y: 0,
            pixel_width: width,
            pixel_height: bands * 6,
            payload: sixel_image(width as usize, bands as usize),
        }
    }

    #[test]
    fn a_full_repaint_retires_the_graphics_the_server_stopped_referencing() {
        let mut state = TerminalState::new(6, 12);
        apply_graphics(
            &mut state,
            &[
                residency(1, GraphicsMedium::Inline, rgba([1, 2, 3, 255], 4)),
                placement(1, 1),
            ],
        );
        assert_eq!(state.graphics().resident_ids(), vec![1]);

        let mut second = FrameBuilder::new(12, 6, 1);
        second.set_full_repaint(true);
        second.extend_graphics(
            [
                residency(2, GraphicsMedium::Inline, rgba([4, 5, 6, 255], 4)),
                placement(2, 1),
            ]
            .iter(),
        );
        state.apply_frame(&second.finish()).unwrap();

        assert_eq!(state.graphics().resident_ids(), vec![2]);
        let placements: Vec<_> = state.graphics().placements().collect();
        assert_eq!(placements.len(), 1);
        assert_eq!((placements[0].cell_y, placements[0].cell_x), (3, 2));
    }

    #[test]
    fn a_row_record_erases_the_part_of_a_sixel_it_covers() {
        let mut state = TerminalState::new(6, 12);
        state.set_cell_size(8, 20);
        apply_graphics(&mut state, &[sixel_at(0, 0, 16, 7)]);
        let before = state.sixels().chunks().next().unwrap().revision;

        let mut builder = FrameBuilder::new(12, 6, 1);
        builder.push_row(1, 0, &run("covered"));
        state.apply_frame(&builder.finish()).unwrap();

        let chunk = state.sixels().chunks().next().expect("the chunk vanished");
        assert!(chunk.revision > before, "the band was not erased");
        let stride = (chunk.width * 4) as usize;
        assert_eq!(&chunk.image.pixels[..4], &[255, 0, 0, 255]);
        assert_eq!(&chunk.image.pixels[stride * 20..stride * 20 + 4], &[0; 4]);
    }

    #[test]
    fn a_row_record_beside_a_sixel_leaves_it_alone() {
        let mut state = TerminalState::new(6, 20);
        state.set_cell_size(8, 20);
        apply_graphics(&mut state, &[sixel_at(0, 0, 16, 7)]);
        let before = state.sixels().chunks().next().unwrap().revision;

        let mut builder = FrameBuilder::new(20, 6, 1);
        builder.push_row(1, 4, &run("neighbour"));
        state.apply_frame(&builder.finish()).unwrap();

        assert_eq!(
            state.sixels().chunks().next().unwrap().revision,
            before,
            "a write in the next pane's columns erased the image"
        );
    }

    #[test]
    fn a_sixel_carried_by_the_frame_that_repaints_around_it_survives() {
        let mut state = TerminalState::new(6, 20);
        state.set_cell_size(8, 20);
        let mut builder = FrameBuilder::new(20, 6, 0);
        builder.push_row(0, 0, &run("status"));
        builder.extend_graphics([sixel_at(0, 0, 16, 2)].iter());
        state.apply_frame(&builder.finish()).unwrap();

        assert_eq!(state.sixels().chunks().count(), 1);
    }

    #[test]
    fn a_reshaping_frame_retires_the_sixel_positioned_against_the_old_grid() {
        let mut state = TerminalState::new(6, 12);
        state.set_cell_size(8, 20);
        apply_graphics(&mut state, &[sixel_at(1, 0, 16, 1)]);
        assert_eq!(state.sixels().chunks().count(), 1);

        let mut reshaped = FrameBuilder::new(20, 8, 1);
        reshaped.set_full_repaint(true);
        state.apply_frame(&reshaped.finish()).unwrap();
        assert!(state.sixels().chunks().next().is_none());
        assert_eq!(state.size(), TermSize { rows: 8, cols: 20 });
    }

    #[test]
    fn a_clear_from_a_smaller_client_attaching_leaves_nothing_outside_the_new_area() {
        let mut state = TerminalState::new(4, 10);
        let mut before = FrameBuilder::new(10, 4, 0);
        before.set_full_repaint(true);
        for row in 0..4 {
            before.push_row(row, 0, &run("wide-pane!"));
        }
        state.apply_frame(&before.finish()).unwrap();

        let mut shrunk = FrameBuilder::new(10, 4, 1);
        shrunk.set_clear(true);
        shrunk.push_row(0, 0, &run("small"));
        shrunk.push_row(1, 0, &run("small"));
        state.apply_frame(&shrunk.finish()).unwrap();

        let text = |row: usize| -> String {
            (0..10)
                .map(|col| state.screen().cell(row, col).character())
                .collect()
        };
        assert_eq!(text(0), "small     ");
        assert_eq!(text(1), "small     ");
        assert_eq!(text(2), "          ");
        assert_eq!(text(3), "          ");
    }

    #[test]
    fn degenerate_dimensions_are_clamped_rather_than_panicking() {
        let state = TerminalState::new(0, 0);
        assert_eq!(state.size(), TermSize { rows: 1, cols: 1 });
    }
}

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use zellij_server::panes::{LinkHandler, TerminalPane};
use zellij_utils::data::{Palette, Style};
use zellij_utils::pane_size::{Dimension, PaneGeom, Size, SizeInPixels};

#[derive(Default)]
struct ReceivedBytes {
    bytes: Vec<u8>,
    generation: u64,
}

#[derive(Default)]
struct ReceivedBytesWithChangeSignal {
    received_bytes: Mutex<ReceivedBytes>,
    change_signal: Condvar,
}

#[derive(Clone)]
pub struct ClientScreen {
    inner: Arc<ReceivedBytesWithChangeSignal>,
    size: Arc<Mutex<Size>>,
}

impl ClientScreen {
    pub fn new(size: Arc<Mutex<Size>>) -> Self {
        ClientScreen {
            inner: Arc::new(ReceivedBytesWithChangeSignal::default()),
            size,
        }
    }

    pub fn writer(&self) -> Box<dyn std::io::Write> {
        Box::new(ClientScreenWriter {
            inner: self.inner.clone(),
        })
    }

    pub fn snapshot(&self) -> GridSnapshot {
        let size = *self.size.lock().unwrap();
        let bytes = self.inner.received_bytes.lock().unwrap().bytes.clone();
        render_bytes(&bytes, size)
    }

    pub fn wait_until(
        &self,
        what: &str,
        predicate: impl Fn(&GridSnapshot) -> bool,
    ) -> GridSnapshot {
        let deadline = Instant::now() + crate::default_timeout();
        let mut received_bytes = self.inner.received_bytes.lock().unwrap();
        loop {
            let size = *self.size.lock().unwrap();
            let grid_snapshot = render_bytes(&received_bytes.bytes, size);
            if predicate(&grid_snapshot) {
                return grid_snapshot;
            }
            let now = Instant::now();
            if now >= deadline {
                panic!(
                    "timed out waiting for: {}\ncursor: {:?}\nlast rendered grid:\n{}\n=== (received {} stdout bytes, generation {}) ===\n=== zellij log tail ({}) ===\n{}",
                    what,
                    grid_snapshot.cursor,
                    grid_snapshot.text,
                    received_bytes.bytes.len(),
                    received_bytes.generation,
                    crate::test_env::log_file_path().display(),
                    crate::test_env::log_tail(40),
                );
            }
            let last_generation = received_bytes.generation;
            let (guard, _) = self
                .inner
                .change_signal
                .wait_timeout(received_bytes, deadline - now)
                .unwrap();
            received_bytes = guard;
            if received_bytes.generation == last_generation {
                continue;
            }
        }
    }
}

struct ClientScreenWriter {
    inner: Arc<ReceivedBytesWithChangeSignal>,
}

impl std::io::Write for ClientScreenWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut received_bytes = self.inner.received_bytes.lock().unwrap();
        received_bytes.bytes.extend_from_slice(buf);
        received_bytes.generation += 1;
        self.inner.change_signal.notify_all();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorPosition {
    pub x: usize,
    pub y: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coord {
    pub col: usize,
    pub row: usize,
}

pub fn col(col: usize) -> CoordBuilder {
    CoordBuilder { col }
}

pub struct CoordBuilder {
    col: usize,
}

impl CoordBuilder {
    pub fn row(self, row: usize) -> Coord {
        Coord { col: self.col, row }
    }
}

#[derive(Clone, Debug)]
pub struct GridSnapshot {
    pub text: String,
    pub cursor: Option<CursorPosition>,
}

impl GridSnapshot {
    pub fn contains(&self, needle: &str) -> bool {
        self.text.contains(needle)
    }
    pub fn cursor_is_at(&self, coord: Coord) -> bool {
        self.cursor
            == Some(CursorPosition {
                x: coord.col,
                y: coord.row,
            })
    }
    pub fn status_bar_appears(&self) -> bool {
        self.text.contains("Ctrl +") && self.text.contains("LOCK")
    }
    pub fn tab_bar_appears(&self) -> bool {
        self.text.contains("Tab #1")
    }
    pub fn lines(&self) -> Vec<String> {
        self.text.lines().map(|l| l.to_owned()).collect()
    }
}

impl std::fmt::Display for GridSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

fn render_bytes(bytes: &[u8], win_size: Size) -> GridSnapshot {
    let mut terminal_pane = build_terminal_pane(win_size);
    let mut vte_parser = vte::Parser::new();
    for &byte in bytes {
        vte_parser.advance(&mut terminal_pane.grid, byte);
    }

    let cursor = terminal_pane
        .cursor_coordinates()
        .and_then(|(x, y, visible)| {
            if visible {
                Some(CursorPosition { x, y })
            } else {
                None
            }
        });
    let mut text = String::new();
    let output_lines = terminal_pane.read_buffer_as_lines();
    for (line_index, line) in output_lines.iter().enumerate() {
        for (character_index, terminal_character) in line.iter().enumerate() {
            let character_position = CursorPosition {
                x: character_index,
                y: line_index,
            };
            if cursor == Some(character_position) {
                text.push('█');
                continue;
            }
            text.push(terminal_character.character);
        }
        if line_index != output_lines.len() - 1 {
            text.push('\n');
        }
    }
    GridSnapshot { text, cursor }
}

fn build_terminal_pane(win_size: Size) -> TerminalPane {
    let mut rows = Dimension::fixed(win_size.rows);
    let mut cols = Dimension::fixed(win_size.cols);
    rows.set_inner(win_size.rows);
    cols.set_inner(win_size.cols);
    let position_and_size = PaneGeom {
        x: 0,
        y: 0,
        rows,
        cols,
        stacked: None,
        is_pinned: false,
        logical_position: None,
    };
    let pid = 0;
    let pane_index = 0;
    let pane_name = String::new();
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        height: 21,
        width: 8,
    })));
    let sixel_image_store = Rc::new(RefCell::new(Default::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let initial_pane_title = None;
    let invoked_with = None;
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_keyboard_protocol = false;
    let notification_end = None;
    TerminalPane::new(
        pid,
        position_and_size,
        Style::default(),
        pane_index,
        pane_name,
        link_handler,
        character_cell_size,
        sixel_image_store,
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        initial_pane_title,
        invoked_with,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_keyboard_protocol,
        notification_end,
    )
}

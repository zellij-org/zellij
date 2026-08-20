use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use zellij_server::panes::kitty_graphics::KittyImageStore;
pub use zellij_server::panes::terminal_character::AnsiCode;
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
    stdout_tap: Mutex<Option<crossbeam::channel::Sender<Vec<u8>>>>,
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

    pub fn set_stdout_tap(&self, sender: crossbeam::channel::Sender<Vec<u8>>) {
        *self.inner.stdout_tap.lock().unwrap() = Some(sender);
    }

    pub fn snapshot(&self) -> GridSnapshot {
        let size = *self.size.lock().unwrap();
        let bytes = self.inner.received_bytes.lock().unwrap().bytes.clone();
        render_bytes(&bytes, size)
    }

    pub fn raw_bytes(&self) -> Vec<u8> {
        self.inner.received_bytes.lock().unwrap().bytes.clone()
    }

    pub fn wait_until_raw_output(&self, what: &str, predicate: impl Fn(&[u8]) -> bool) -> Vec<u8> {
        let deadline = Instant::now() + crate::default_timeout();
        let mut received_bytes = self.inner.received_bytes.lock().unwrap();
        loop {
            if predicate(&received_bytes.bytes) {
                return received_bytes.bytes.clone();
            }
            let now = Instant::now();
            if now >= deadline {
                let size = *self.size.lock().unwrap();
                let grid_snapshot = render_bytes(&received_bytes.bytes, size);
                panic!(
                    "timed out waiting for: {}\nlast rendered grid:\n{}\n=== (received {} stdout bytes, generation {}) ===\n=== zellij log tail ({}) ===\n{}",
                    what,
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
        if let Some(stdout_tap) = self.inner.stdout_tap.lock().unwrap().as_ref() {
            let _ = stdout_tap.send(buf.to_vec());
        }
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellStyle {
    pub dim: bool,
    pub italic: bool,
    pub bold: bool,
    pub foreground: Option<AnsiCode>,
}

#[derive(Clone, Debug)]
pub struct GridSnapshot {
    pub text: String,
    pub cursor: Option<CursorPosition>,
    pub styles: Vec<Vec<CellStyle>>,
}

impl GridSnapshot {
    pub fn contains(&self, needle: &str) -> bool {
        self.text.contains(needle)
    }
    pub fn cell_style(&self, x: usize, y: usize) -> CellStyle {
        self.styles
            .get(y)
            .and_then(|row| row.get(x))
            .copied()
            .unwrap_or_default()
    }
    pub fn cell_foreground(&self, x: usize, y: usize) -> Option<AnsiCode> {
        self.cell_style(x, y).foreground
    }
    pub fn char_is_dim(&self, x: usize, y: usize) -> bool {
        self.cell_style(x, y).dim
    }
    pub fn char_is_italic(&self, x: usize, y: usize) -> bool {
        self.cell_style(x, y).italic
    }
    pub fn char_is_bold(&self, x: usize, y: usize) -> bool {
        self.cell_style(x, y).bold
    }
    pub fn row_has_italic(&self, y: usize) -> bool {
        self.styles
            .get(y)
            .map_or(false, |row| row.iter().any(|cell| cell.italic))
    }
    pub fn row_has_bold(&self, y: usize) -> bool {
        self.styles
            .get(y)
            .map_or(false, |row| row.iter().any(|cell| cell.bold))
    }
    pub fn row_count(&self) -> usize {
        self.styles.len()
    }
    pub fn row_of_line(&self, needle: &str) -> Option<usize> {
        self.text.lines().position(|line| line.contains(needle))
    }
    pub fn region_has_dim(
        &self,
        x_range: std::ops::Range<usize>,
        y_range: std::ops::Range<usize>,
    ) -> bool {
        for y in y_range {
            for x in x_range.clone() {
                if self.char_is_dim(x, y) {
                    return true;
                }
            }
        }
        false
    }
    pub fn line_has_dim(&self, needle: &str) -> bool {
        match self.row_of_line(needle) {
            Some(y) => self
                .styles
                .get(y)
                .map_or(false, |row| row.iter().any(|cell| cell.dim)),
            None => false,
        }
    }
    pub fn char_dim_of(&self, needle: &str) -> Option<bool> {
        self.text.lines().enumerate().find_map(|(y, line)| {
            line.find(needle)
                .map(|byte_index| line[..byte_index].chars().count())
                .map(|x| self.char_is_dim(x, y))
        })
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
    vte_parser.advance(&mut terminal_pane.grid, bytes);

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
    let mut styles: Vec<Vec<CellStyle>> = Vec::new();
    let output_lines = terminal_pane.read_buffer_as_lines();
    for (line_index, line) in output_lines.iter().enumerate() {
        let mut style_row: Vec<CellStyle> = Vec::with_capacity(line.len());
        for (character_index, terminal_character) in line.iter().enumerate() {
            let character_style = &terminal_character.styles;
            style_row.push(CellStyle {
                dim: matches!(character_style.dim, Some(AnsiCode::On)),
                italic: matches!(character_style.italic, Some(AnsiCode::On)),
                bold: matches!(character_style.bold, Some(AnsiCode::On)),
                foreground: character_style.foreground,
            });
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
        styles.push(style_row);
        if line_index != output_lines.len() - 1 {
            text.push('\n');
        }
    }
    GridSnapshot {
        text,
        cursor,
        styles,
    }
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
        Rc::new(RefCell::new(KittyImageStore::default())),
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

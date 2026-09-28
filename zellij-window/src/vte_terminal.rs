use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;

use crate::kitty::{ApcFilter, Kind};
use crate::sixel::Span;

const SCROLLBACK_LINES: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermSize {
    pub rows: usize,
    pub cols: usize,
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

#[derive(Clone, Default)]
pub struct ClipboardSink;

impl EventListener for ClipboardSink {
    fn send_event(&self, _event: Event) {}
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Blink {
    pub slow: bool,
    pub fast: bool,
}

#[derive(Default)]
struct BlinkMap {
    rows: usize,
    cols: usize,
    cells: Vec<Blink>,
}

impl BlinkMap {
    fn sized(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            cells: vec![Blink::default(); rows * cols],
        }
    }

    fn get(&self, row: usize, col: usize) -> Blink {
        if row >= self.rows || col >= self.cols {
            return Blink::default();
        }
        self.cells[row * self.cols + col]
    }

    fn paint(&mut self, span: &Span, blink: Blink) {
        if span.line >= self.rows {
            return;
        }
        let last = span.right.min(self.cols.saturating_sub(1));
        for col in span.left..=last {
            self.cells[span.line * self.cols + col] = blink;
        }
    }

    fn clear(&mut self) {
        self.cells
            .iter_mut()
            .for_each(|cell| *cell = Blink::default());
    }
}

pub struct VteTerminal {
    term: Term<ClipboardSink>,
    parser: Processor,
    filter: ApcFilter,
    run_start: (usize, usize),
    blink: Blink,
    blinking: BlinkMap,
}

impl VteTerminal {
    pub fn new(rows: usize, cols: usize) -> Self {
        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
            ..Default::default()
        };
        let size = clamped(rows, cols);
        Self {
            term: Term::new(config, &size, ClipboardSink),
            parser: Processor::new(),
            filter: ApcFilter::new(),
            run_start: (0, 0),
            blink: Blink::default(),
            blinking: BlinkMap::sized(size.rows, size.cols),
        }
    }

    pub fn apply(&mut self, payload: &str) {
        self.apply_bytes(payload.as_bytes());
    }

    pub fn apply_bytes(&mut self, payload: &[u8]) {
        let mut forwarded: Vec<u8> = Vec::with_capacity(payload.len());
        let mut written: Vec<Span> = Vec::new();
        for byte in payload {
            let step = self.filter.advance(*byte);
            match step.kind {
                Kind::Forward => forwarded.extend_from_slice(step.bytes.as_slice()),
                Kind::Cleared => {
                    forwarded.extend_from_slice(step.bytes.as_slice());
                    self.parser.advance(&mut self.term, &forwarded);
                    forwarded.clear();
                    self.close_run(&mut written);
                    self.blinking.clear();
                    written.clear();
                },
                Kind::Boundary => {
                    self.parser.advance(&mut self.term, &forwarded);
                    forwarded.clear();
                    self.close_run(&mut written);
                    self.parser.advance(&mut self.term, step.bytes.as_slice());
                    if step.bytes.as_slice() == b"m" {
                        self.blink = blink_after(self.blink, self.filter.params());
                    }
                    self.run_start = self.cursor_cell();
                },
                Kind::Signal(_) => {
                    forwarded.extend_from_slice(step.bytes.as_slice());
                    self.parser.advance(&mut self.term, &forwarded);
                    forwarded.clear();
                    self.close_run(&mut written);
                    self.run_start = self.cursor_cell();
                },
            }
        }
        if !forwarded.is_empty() {
            self.parser.advance(&mut self.term, &forwarded);
        }
        self.close_run(&mut written);
    }

    fn close_run(&mut self, written: &mut Vec<Span>) {
        let end = self.cursor_cell();
        let start = std::mem::replace(&mut self.run_start, end);
        if end == start {
            return;
        }
        let before = written.len();
        let last_column = self.term.columns().saturating_sub(1);
        if end.0 == start.0 {
            if end.1 > start.1 {
                written.push(Span::new(start.0, start.1, end.1 - 1));
            }
        } else {
            written.push(Span::new(start.0, start.1, last_column));
            for line in start.0 + 1..end.0 {
                written.push(Span::new(line, 0, last_column));
            }
            if end.1 > 0 {
                written.push(Span::new(end.0, 0, end.1 - 1));
            }
        }
        let blink = self.blink;
        for span in &written[before..] {
            self.blinking.paint(span, blink);
        }
    }

    fn cursor_cell(&self) -> (usize, usize) {
        let cursor = &self.term.grid().cursor;
        let column = cursor.point.column.0 + usize::from(cursor.input_needs_wrap);
        (
            cursor.point.line.0.max(0) as usize,
            column.min(self.term.columns()),
        )
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        let size = clamped(rows, cols);
        self.term.resize(size);
        self.blinking = BlinkMap::sized(size.rows, size.cols);
        self.blink = Blink::default();
        self.run_start = self.cursor_cell();
    }

    pub fn size(&self) -> TermSize {
        TermSize {
            rows: self.term.screen_lines(),
            cols: self.term.columns(),
        }
    }

    pub fn cursor_is_visible(&self) -> bool {
        self.term.mode().contains(TermMode::SHOW_CURSOR)
    }

    pub fn term(&self) -> &Term<ClipboardSink> {
        &self.term
    }

    pub fn blink_at(&self, row: usize, col: usize) -> Blink {
        self.blinking.get(row, col)
    }

    pub fn history_len(&self) -> usize {
        self.term.grid().history_size()
    }
}

fn blink_after(current: Blink, params: &[u8]) -> Blink {
    let mut blink = current;
    let mut tokens = params.split(|byte| *byte == b';');
    while let Some(token) = tokens.next() {
        if token.contains(&b':') {
            continue;
        }
        match token {
            b"" | b"0" => blink = Blink::default(),
            b"5" => blink.slow = true,
            b"6" => blink.fast = true,
            b"25" => blink = Blink::default(),
            b"38" | b"48" | b"58" => {
                let following = match tokens.next() {
                    Some(b"5") => 1,
                    Some(b"2") => 3,
                    _ => 0,
                };
                for _ in 0..following {
                    tokens.next();
                }
            },
            _ => {},
        }
    }
    blink
}

pub fn clamped(rows: usize, cols: usize) -> TermSize {
    TermSize {
        rows: rows.max(alacritty_terminal::term::MIN_SCREEN_LINES),
        cols: cols.max(alacritty_terminal::term::MIN_COLUMNS),
    }
}

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use zellij_server::output::{
    CharacterChunk, HostKittyState, KittyImageChunk, Output, RenderPayload, SixelImageChunk,
    StructuredClientState,
};
use zellij_server::panes::grid::Grid;
use zellij_server::panes::kitty_graphics::interceptor::{InterceptorResult, KittyApcInterceptor};
use zellij_server::panes::kitty_graphics::store::KittyImageStore;
use zellij_server::panes::link_handler::LinkHandler;
use zellij_server::panes::sixel::SixelImageStore;
use zellij_server::panes::terminal_character::{
    AnsiCode, AnsiStyledUnderline, CharacterStyles, LinkAnchor, NamedColor, TerminalCharacter,
};
use zellij_server::panes::PaneId;
use zellij_utils::data::Style as ZellijStyle;
use zellij_utils::pane_size::{Size, SizeInPixels};

use crate::projection::{Cell, Cursor, Occupancy, Projection, DEFAULT_COLOR};

pub const CELL_WIDTH: usize = 8;
pub const CELL_HEIGHT: usize = 20;
const CLIENT: u16 = 1;

struct Stores {
    links: Rc<RefCell<LinkHandler>>,
    sixels: Rc<RefCell<SixelImageStore>>,
    kitty: Rc<RefCell<KittyImageStore>>,
    cell_size: Rc<RefCell<Option<SizeInPixels>>>,
}

impl Stores {
    fn new() -> Self {
        Self {
            links: Rc::new(RefCell::new(LinkHandler::new())),
            sixels: Rc::new(RefCell::new(SixelImageStore::default())),
            kitty: Rc::new(RefCell::new(KittyImageStore::default())),
            cell_size: Rc::new(RefCell::new(Some(SizeInPixels {
                width: CELL_WIDTH,
                height: CELL_HEIGHT,
            }))),
        }
    }

    fn grid(&self, rows: usize, cols: usize) -> Grid {
        Grid::new(
            rows,
            cols,
            Rc::new(RefCell::new(Default::default())),
            Rc::new(RefCell::new(HashMap::new())),
            self.links.clone(),
            self.cell_size.clone(),
            self.sixels.clone(),
            self.kitty.clone(),
            ZellijStyle::default(),
            false,
            true,
            true,
            true,
            false,
        )
    }
}

fn feed(
    grid: &mut Grid,
    parser: &mut vte::Parser,
    interceptor: &mut KittyApcInterceptor,
    bytes: &[u8],
) {
    let mut forwarded: Vec<u8> = Vec::with_capacity(bytes.len());
    for byte in bytes {
        match interceptor.advance(*byte) {
            InterceptorResult::Forward(bytes) => forwarded.extend_from_slice(bytes.as_slice()),
            InterceptorResult::Swallow => {},
            InterceptorResult::Captured(command) => {
                parser.advance(grid, &forwarded);
                forwarded.clear();
                grid.handle_kitty_apc(&command);
            },
        }
    }
    parser.advance(grid, &forwarded);
}

pub struct Reference {
    grid: Grid,
    parser: vte::Parser,
    interceptor: KittyApcInterceptor,
    stores: Stores,
    rows: usize,
    cols: usize,
}

impl Reference {
    pub fn new(rows: usize, cols: usize) -> Self {
        let stores = Stores::new();
        let grid = stores.grid(rows, cols);
        Self {
            grid,
            parser: vte::Parser::new(),
            interceptor: KittyApcInterceptor::new(),
            stores,
            rows,
            cols,
        }
    }

    pub fn advance(&mut self, bytes: &[u8]) {
        feed(
            &mut self.grid,
            &mut self.parser,
            &mut self.interceptor,
            bytes,
        );
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.grid.change_size(rows, cols);
        self.grid.render_full_viewport();
        self.rows = rows;
        self.cols = cols;
    }

    pub fn project(&self) -> Projection {
        project_grid(
            &self.grid,
            &self.stores.links.borrow(),
            self.rows,
            self.cols,
        )
    }
}

type KittyHostStates = Rc<RefCell<HashMap<u16, HostKittyState>>>;

pub struct Emitter {
    grid: Grid,
    parser: vte::Parser,
    interceptor: KittyApcInterceptor,
    stores: Stores,
    cleared: bool,
    rows: usize,
    cols: usize,
    ansi_kitty_state: KittyHostStates,
    frame_kitty_state: KittyHostStates,
    structured_clients: Rc<RefCell<HashMap<u16, StructuredClientState>>>,
    pending_signals: Vec<String>,
}

impl Emitter {
    pub fn new(rows: usize, cols: usize) -> Self {
        let stores = Stores::new();
        let grid = stores.grid(rows, cols);
        Self {
            grid,
            parser: vte::Parser::new(),
            interceptor: KittyApcInterceptor::new(),
            stores,
            cleared: false,
            rows,
            cols,
            ansi_kitty_state: Rc::new(RefCell::new(HashMap::new())),
            frame_kitty_state: Rc::new(RefCell::new(HashMap::new())),
            structured_clients: Rc::new(RefCell::new(HashMap::from([(
                CLIENT,
                StructuredClientState {
                    enabled: true,
                    size: Size { rows, cols },
                    ..Default::default()
                },
            )]))),
            pending_signals: Vec::new(),
        }
    }

    pub fn advance(&mut self, bytes: &[u8]) {
        feed(
            &mut self.grid,
            &mut self.parser,
            &mut self.interceptor,
            bytes,
        );
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.grid.change_size(rows, cols);
        self.grid.render_full_viewport();
        self.rows = rows;
        self.cols = cols;
        self.cleared = false;
        if let Some(state) = self.structured_clients.borrow_mut().get_mut(&CLIENT) {
            state.reset_viewport(Size { rows, cols });
        }
    }

    pub fn frame(&mut self) -> Option<String> {
        match self.dialects()? {
            (RenderPayload::Ansi(content), _) => Some(content),
            _ => None,
        }
    }

    pub fn dialects(&mut self) -> Option<(RenderPayload, RenderPayload)> {
        let (character_chunks, _raw, sixel_chunks, kitty_chunks) = self
            .grid
            .render(0, 0, &ZellijStyle::default())
            .ok()
            .flatten()?;

        if character_chunks.is_empty() && sixel_chunks.is_empty() && kitty_chunks.is_empty() {
            return None;
        }

        let clear = !self.cleared;
        self.cleared = true;
        let ansi = self.serialize_one(
            false,
            clear,
            character_chunks.clone(),
            sixel_chunks.clone(),
            kitty_chunks.clone(),
        )?;
        let frame =
            self.serialize_one(true, clear, character_chunks, sixel_chunks, kitty_chunks)?;
        self.pending_signals.clear();
        Some((ansi, frame))
    }

    fn serialize_one(
        &mut self,
        structured: bool,
        clear: bool,
        character_chunks: Vec<CharacterChunk>,
        sixel_chunks: Vec<SixelImageChunk>,
        kitty_chunks: Vec<KittyImageChunk>,
    ) -> Option<RenderPayload> {
        let (kitty_state, structured_clients) = if structured {
            (
                self.frame_kitty_state.clone(),
                self.structured_clients.clone(),
            )
        } else {
            (
                self.ansi_kitty_state.clone(),
                Rc::new(RefCell::new(HashMap::new())),
            )
        };
        let mut output = Output::new(
            self.stores.sixels.clone(),
            self.stores.cell_size.clone(),
            true,
            true,
            self.stores.kitty.clone(),
            Rc::new(RefCell::new(HashMap::from([(CLIENT, true)]))),
            Rc::new(RefCell::new(HashMap::new())),
            kitty_state,
            Rc::new(RefCell::new(HashMap::from([(CLIENT, true)]))),
            structured_clients,
        );
        output.add_clients(&HashSet::from([CLIENT]), self.stores.links.clone(), None);
        output
            .add_character_chunks_to_client(CLIENT, character_chunks, None)
            .ok()?;
        output.add_sixel_image_chunks_to_client(CLIENT, sixel_chunks, None);
        output.add_kitty_image_chunks_to_client(CLIENT, PaneId::Terminal(1), kitty_chunks, None);

        for signal in &self.pending_signals {
            output.add_post_vte_instruction_to_client(CLIENT, signal);
        }
        output.add_pre_vte_instruction_to_client(CLIENT, "\u{1b}[?25l");
        if clear {
            output.add_pre_vte_instruction_to_client(CLIENT, "\u{1b}[m\u{1b}[2J");
            output.mark_host_display_cleared_for_client(CLIENT);
        }
        let shape_csi = self.grid.cursor_shape().get_csi_str().to_owned();
        match self.grid.cursor_coordinates() {
            Some((x, y, visible)) => {
                output.set_client_cursor(CLIENT, x, y, visible, &shape_csi);
                if visible {
                    output.add_post_vte_instruction_to_client(CLIENT, "\u{1b}[?25h");
                    output.add_post_vte_instruction_to_client(
                        CLIENT,
                        &format!("\u{1b}[{};{}H\u{1b}[m", y + 1, x + 1),
                    );
                } else {
                    output.add_post_vte_instruction_to_client(CLIENT, "\u{1b}[?25l");
                }
            },
            None => {
                output.set_client_cursor(CLIENT, 0, 0, false, "");
                output.add_post_vte_instruction_to_client(CLIENT, "\u{1b}[?25l");
            },
        }

        output
            .serialize()
            .ok()
            .and_then(|mut per_client| per_client.remove(&CLIENT))
    }

    pub fn project(&self) -> Projection {
        project_grid(
            &self.grid,
            &self.stores.links.borrow(),
            self.rows,
            self.cols,
        )
    }

    pub fn scrollback_len(&self) -> usize {
        self.grid.scrollback_position_and_length().1
    }

    pub fn scroll_up(&mut self, rows: usize) -> usize {
        let before = self.grid.scrollback_position_and_length().0;
        self.grid.move_viewport_up(rows);
        self.grid.scrollback_position_and_length().0 - before
    }

    pub fn project_scrolled(&self) -> Projection {
        crate::projection::without_blink_or_cursor(self.project())
    }
}

fn project_grid(grid: &Grid, links: &LinkHandler, rows: usize, cols: usize) -> Projection {
    let defaults = PaneDefaults {
        foreground: grid.pane_default_fg,
        background: grid.pane_default_bg,
    };
    let cells = grid
        .as_character_lines()
        .into_iter()
        .map(|line| project_line(&line, links, cols, defaults))
        .collect();

    let cursor = match grid.cursor_coordinates() {
        Some((col, row, visible)) => Cursor {
            position: Some((col, row)),
            visible: Some(visible),
        },
        None => Cursor {
            position: None,
            visible: None,
        },
    };

    Projection {
        rows,
        cols,
        cursor,
        cells,
        wrapped_rows: Vec::new(),
    }
}

#[derive(Clone, Copy, Default)]
struct PaneDefaults {
    foreground: Option<(u8, u8, u8)>,
    background: Option<(u8, u8, u8)>,
}

fn project_line(
    line: &[TerminalCharacter],
    links: &LinkHandler,
    cols: usize,
    defaults: PaneDefaults,
) -> Vec<Cell> {
    let mut cells = Vec::with_capacity(cols);
    for character in line {
        if cells.len() >= cols {
            break;
        }
        let mut cell = project_character(character, links, defaults);
        if character.width() >= 2 {
            cell.occupancy = Occupancy::WideHead;
            let mut tail = cell.clone();
            tail.text = " ".to_owned();
            tail.occupancy = Occupancy::WideTail;
            cells.push(cell);
            cells.push(tail);
        } else {
            cells.push(cell);
        }
    }
    cells.truncate(cols);
    cells.resize(cols, blank_cell(defaults));
    cells
}

fn blank_cell(defaults: PaneDefaults) -> Cell {
    Cell {
        text: " ".to_owned(),
        occupancy: Occupancy::Single,
        fg: resolved(DEFAULT_COLOR.to_owned(), defaults.foreground),
        bg: resolved(DEFAULT_COLOR.to_owned(), defaults.background),
        underline: None,
        attrs: BTreeSet::new(),
        link: None,
    }
}

fn resolved(color: String, default: Option<(u8, u8, u8)>) -> String {
    match (color.as_str(), default) {
        (DEFAULT_COLOR, Some((r, g, b))) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        _ => color,
    }
}

fn project_character(
    character: &TerminalCharacter,
    links: &LinkHandler,
    defaults: PaneDefaults,
) -> Cell {
    let styles: &CharacterStyles = &character.styles;
    let mut attrs = BTreeSet::new();
    for (present, name) in [
        (is_on(styles.bold), "bold"),
        (is_on(styles.dim), "dim"),
        (is_on(styles.italic), "italic"),
        (is_on(styles.reverse), "inverse"),
        (is_on(styles.hidden), "hidden"),
        (is_on(styles.strike), "strike"),
        (is_on(styles.slow_blink), "blink-slow"),
        (is_on(styles.fast_blink), "blink-fast"),
    ] {
        if present {
            attrs.insert(name);
        }
    }
    if let Some(underline) = underline_attr(styles.underline) {
        attrs.insert(underline);
    }

    Cell {
        text: character.character.to_string(),
        occupancy: Occupancy::Single,
        fg: resolved(zellij_color(styles.foreground), defaults.foreground),
        bg: resolved(zellij_color(styles.background), defaults.background),
        underline: match styles.underline_color {
            None | Some(AnsiCode::Reset) => None,
            other => Some(zellij_color(other)),
        },
        attrs,
        link: link_uri(styles.link_anchor, links),
    }
}

fn is_on(code: Option<AnsiCode>) -> bool {
    matches!(code, Some(AnsiCode::On))
}

fn underline_attr(code: Option<AnsiCode>) -> Option<&'static str> {
    match code {
        Some(AnsiCode::Underline(None)) => Some("underline"),
        Some(AnsiCode::Underline(Some(AnsiStyledUnderline::Double))) => Some("double-underline"),
        Some(AnsiCode::Underline(Some(AnsiStyledUnderline::Undercurl))) => Some("undercurl"),
        Some(AnsiCode::Underline(Some(AnsiStyledUnderline::Underdotted))) => {
            Some("dotted-underline")
        },
        Some(AnsiCode::Underline(Some(AnsiStyledUnderline::Underdashed))) => {
            Some("dashed-underline")
        },
        _ => None,
    }
}

fn zellij_color(code: Option<AnsiCode>) -> String {
    match code {
        None | Some(AnsiCode::Reset) => DEFAULT_COLOR.to_owned(),
        Some(AnsiCode::NamedColor(named)) => named_color(named),
        Some(AnsiCode::ColorIndex(index)) => format!("idx{}", index),
        Some(AnsiCode::RgbCode((r, g, b))) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        Some(other) => format!("unexpected({:?})", other),
    }
}

fn named_color(named: NamedColor) -> String {
    format!("{:?}", named).to_lowercase()
}

fn link_uri(anchor: Option<LinkAnchor>, links: &LinkHandler) -> Option<String> {
    match anchor {
        Some(LinkAnchor::Start(_)) => links.output_osc8(anchor).and_then(|osc8| {
            let mut parts = osc8.splitn(3, ';');
            parts.next()?;
            parts.next()?;
            Some(parts.next()?.trim_end_matches("\u{1b}\\").to_owned())
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::{compare, of_window};
    use crate::vte_terminal::VteTerminal;

    fn agreement(rows: usize, cols: usize, stream: &str) -> Vec<String> {
        let mut reference = Reference::new(rows, cols);
        reference.advance(stream.as_bytes());
        let mut state = VteTerminal::new(rows, cols);
        state.apply(stream);
        compare(&reference.project(), &of_window(&state))
            .into_iter()
            .map(|divergence| divergence.describe())
            .collect()
    }

    fn assert_agrees(rows: usize, cols: usize, stream: &str) {
        let divergences = agreement(rows, cols, stream);
        assert!(divergences.is_empty(), "{}", divergences.join("\n"));
    }

    #[test]
    fn plain_text_agrees() {
        assert_agrees(2, 8, "\u{1b}[1;1H\u{1b}[mhello");
    }

    #[test]
    fn every_emitted_color_form_agrees() {
        assert_agrees(
            1,
            10,
            "\u{1b}[1;1H\u{1b}[m\u{1b}[31ma\u{1b}[38;5;208mb\u{1b}[38;2;1;2;3mc\
             \u{1b}[39m\u{1b}[41md\u{1b}[48;5;99me\u{1b}[48;2;9;8;7mf\u{1b}[49mg",
        );
    }

    #[test]
    fn every_emitted_attribute_agrees() {
        assert_agrees(
            1,
            12,
            "\u{1b}[1;1H\u{1b}[m\u{1b}[1ma\u{1b}[2mb\u{1b}[3mc\u{1b}[4md\u{1b}[7me\
             \u{1b}[8mf\u{1b}[9mg\u{1b}[5mh\u{1b}[6mi",
        );
    }

    #[test]
    fn styled_underlines_agree() {
        assert_agrees(
            1,
            10,
            "\u{1b}[1;1H\u{1b}[m\u{1b}[4:1ma\u{1b}[4:2mb\u{1b}[4:3mc\u{1b}[4:4md\
             \u{1b}[4:5me\u{1b}[58:2::1:2:3mf\u{1b}[59mg",
        );
    }

    #[test]
    fn a_wide_character_agrees_on_both_of_its_columns() {
        assert_agrees(1, 8, "\u{1b}[1;1H\u{1b}[m\u{4f60}\u{597d}x");
    }

    #[test]
    fn a_hyperlink_agrees() {
        assert_agrees(
            1,
            16,
            "\u{1b}[1;1H\u{1b}[m\u{1b}]8;id=1;https://example.com\u{1b}\\link\
             \u{1b}]8;;\u{1b}\\plain",
        );
    }

    #[test]
    fn the_emitter_round_trips_its_own_grid_through_the_wire() {
        let mut emitter = Emitter::new(4, 20);
        emitter.advance(b"\x1b[1;31mred\x1b[m plain");
        let frame = emitter.frame().expect("a frame");
        let mut reference = Reference::new(4, 20);
        reference.advance(frame.as_bytes());
        let divergences = compare(&emitter.project(), &reference.project());
        assert!(
            divergences.is_empty(),
            "{}",
            divergences
                .iter()
                .map(|divergence| divergence.describe())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn the_emitter_produces_no_frame_when_nothing_changed() {
        let mut emitter = Emitter::new(4, 20);
        emitter.advance(b"hello");
        assert!(emitter.frame().is_some());
        assert!(emitter.frame().is_none());
    }
}

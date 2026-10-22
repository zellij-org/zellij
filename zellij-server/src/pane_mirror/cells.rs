use super::view::MirrorRedraw;
use crate::output::CharacterChunk;
use crate::panes::terminal_character::{
    AnsiCode, AnsiStyledUnderline, CharacterStyles, LinkAnchor, NamedColor, TerminalCharacter,
    EMPTY_TERMINAL_CHARACTER, RESET_STYLES,
};
use unicode_width::UnicodeWidthChar;
use zellij_utils::pane_mirror::{MirrorCell, MirrorScreen};
use zellij_utils::structured_render::{
    WireCell, WireColor, ATTR_BOLD, ATTR_DIM, ATTR_FAST_BLINK, ATTR_GROUP_CONTINUE, ATTR_HIDDEN,
    ATTR_ITALIC, ATTR_REVERSE, ATTR_SLOW_BLINK, ATTR_STRIKE, CURSOR_BLINKING, CURSOR_SHAPE_BEAM,
    CURSOR_SHAPE_BLOCK, CURSOR_SHAPE_UNDERLINE, UNDERLINE_CURLY, UNDERLINE_DASHED,
    UNDERLINE_DOTTED, UNDERLINE_DOUBLE, UNDERLINE_STRAIGHT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LiveViewRect {
    pub x: usize,
    pub y: usize,
    pub cols: usize,
    pub rows: usize,
}

impl LiveViewRect {
    pub fn new(x: usize, y: usize, cols: usize, rows: usize) -> Self {
        LiveViewRect { x, y, cols, rows }
    }

    pub fn clamped_to(&self, cols: usize, rows: usize) -> LiveViewRect {
        let x = self.x.min(cols);
        let y = self.y.min(rows);
        LiveViewRect {
            x,
            y,
            cols: self.cols.min(cols - x),
            rows: self.rows.min(rows - y),
        }
    }

    pub fn placed_in(&self, area: &LiveViewRect) -> LiveViewRect {
        let clamped = self.clamped_to(area.cols, area.rows);
        LiveViewRect {
            x: area.x + clamped.x,
            y: area.y + clamped.y,
            ..clamped
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cols == 0 || self.rows == 0
    }

    pub fn contains(&self, column: usize, line: usize) -> bool {
        column >= self.x
            && column < self.x + self.cols
            && line >= self.y
            && line < self.y + self.rows
    }

    pub fn size(&self) -> zellij_utils::pane_size::Size {
        zellij_utils::pane_size::Size {
            cols: self.cols,
            rows: self.rows,
        }
    }
}

pub fn ansi_color(packed: u32) -> AnsiCode {
    match WireColor::unpack(packed) {
        WireColor::Default => AnsiCode::Reset,
        WireColor::Named(index) => AnsiCode::NamedColor(named_color(index)),
        WireColor::Indexed(index) => AnsiCode::ColorIndex(index),
        WireColor::Rgb(r, g, b) => AnsiCode::RgbCode((r, g, b)),
    }
}

fn named_color(index: u8) -> NamedColor {
    match index {
        0 => NamedColor::Black,
        1 => NamedColor::Red,
        2 => NamedColor::Green,
        3 => NamedColor::Yellow,
        4 => NamedColor::Blue,
        5 => NamedColor::Magenta,
        6 => NamedColor::Cyan,
        7 => NamedColor::White,
        8 => NamedColor::BrightBlack,
        9 => NamedColor::BrightRed,
        10 => NamedColor::BrightGreen,
        11 => NamedColor::BrightYellow,
        12 => NamedColor::BrightBlue,
        13 => NamedColor::BrightMagenta,
        14 => NamedColor::BrightCyan,
        _ => NamedColor::BrightWhite,
    }
}

fn on_or_reset(cell: &WireCell, attr: u16) -> Option<AnsiCode> {
    if cell.has(attr) {
        Some(AnsiCode::On)
    } else {
        Some(AnsiCode::Reset)
    }
}

pub fn cell_styles(
    cell: &WireCell,
    link_anchor: Option<LinkAnchor>,
    styled_underlines: bool,
) -> CharacterStyles {
    let underline = match cell.underline_style() {
        UNDERLINE_STRAIGHT => AnsiCode::Underline(None),
        UNDERLINE_DOUBLE => AnsiCode::Underline(Some(AnsiStyledUnderline::Double)),
        UNDERLINE_CURLY => AnsiCode::Underline(Some(AnsiStyledUnderline::Undercurl)),
        UNDERLINE_DOTTED => AnsiCode::Underline(Some(AnsiStyledUnderline::Underdotted)),
        UNDERLINE_DASHED => AnsiCode::Underline(Some(AnsiStyledUnderline::Underdashed)),
        _ => AnsiCode::Reset,
    };
    CharacterStyles {
        foreground: Some(ansi_color(cell.fg)),
        background: Some(ansi_color(cell.bg)),
        underline_color: Some(ansi_color(cell.underline_color)),
        strike: on_or_reset(cell, ATTR_STRIKE),
        hidden: on_or_reset(cell, ATTR_HIDDEN),
        reverse: on_or_reset(cell, ATTR_REVERSE),
        slow_blink: on_or_reset(cell, ATTR_SLOW_BLINK),
        fast_blink: on_or_reset(cell, ATTR_FAST_BLINK),
        underline: Some(underline),
        bold: on_or_reset(cell, ATTR_BOLD),
        dim: on_or_reset(cell, ATTR_DIM),
        italic: on_or_reset(cell, ATTR_ITALIC),
        link_anchor: Some(link_anchor.unwrap_or(LinkAnchor::End)),
        styled_underlines_enabled: styled_underlines,
    }
}

pub fn terminal_character(
    cell: &MirrorCell,
    width: usize,
    styles: CharacterStyles,
) -> TerminalCharacter {
    let width = width.min(u8::MAX as usize);
    if let Some(text) = cell.text.as_deref() {
        if let Some(character) = TerminalCharacter::from_cluster(text, width, styles.into()) {
            return character;
        }
    }
    let character = match char::from_u32(cell.cell.ch) {
        Some(character) if cell.cell.ch != 0 && !character.is_control() => character,
        _ => ' ',
    };
    TerminalCharacter::new_with_width(character, width.max(1) as u8, styles.into())
}

fn blank_characters(count: usize) -> impl Iterator<Item = TerminalCharacter> {
    std::iter::repeat(EMPTY_TERMINAL_CHARACTER).take(count)
}

pub fn blank_row_chunk(x: usize, y: usize, cols: usize) -> CharacterChunk {
    CharacterChunk::new(blank_characters(cols).collect(), x, y)
}

pub fn screen_row_chunk(
    screen: &MirrorScreen,
    row: u16,
    x: usize,
    y: usize,
    cols: usize,
    link_for: &mut dyn FnMut(&str) -> Option<LinkAnchor>,
    styled_underlines: bool,
) -> CharacterChunk {
    let cells = screen.row(row);
    let mut characters = Vec::with_capacity(cols);
    let mut may_have_groups = false;
    let mut column = 0;
    while column < cols {
        let Some(cell) = cells.get(column) else {
            break;
        };
        let width = (cell.cell.width as usize).max(1);
        if column + width > cols {
            break;
        }
        if cell.text.is_some() || cell.cell.has(ATTR_GROUP_CONTINUE) {
            may_have_groups = true;
        }
        let link_anchor = cell.link.as_deref().and_then(|uri| link_for(uri));
        let styles = cell_styles(&cell.cell, link_anchor, styled_underlines);
        characters.push(terminal_character(cell, width, styles));
        column += width;
    }
    characters.extend(blank_characters(cols - column));
    let mut chunk = CharacterChunk::new(characters, x, y);
    chunk.may_have_groups = may_have_groups;
    chunk
}

pub fn row_offset(screen_rows: usize, cursor_row: usize, view_rows: usize) -> usize {
    if view_rows == 0 || screen_rows <= view_rows {
        return 0;
    }
    (cursor_row + 1)
        .saturating_sub(view_rows)
        .min(screen_rows - view_rows)
}

pub fn screen_chunks(
    screen: &MirrorScreen,
    area: &LiveViewRect,
    redraw: &MirrorRedraw,
    link_for: &mut dyn FnMut(&str) -> Option<LinkAnchor>,
    styled_underlines: bool,
) -> Vec<CharacterChunk> {
    screen_chunks_from(screen, area, redraw, 0, link_for, styled_underlines)
}

pub fn screen_chunks_from(
    screen: &MirrorScreen,
    area: &LiveViewRect,
    redraw: &MirrorRedraw,
    row_offset: usize,
    link_for: &mut dyn FnMut(&str) -> Option<LinkAnchor>,
    styled_underlines: bool,
) -> Vec<CharacterChunk> {
    let rows: Vec<usize> = match redraw {
        MirrorRedraw::Nothing => vec![],
        MirrorRedraw::Full => (0..area.rows).collect(),
        MirrorRedraw::Rows(rows) => rows
            .iter()
            .map(|row| *row as usize)
            .filter(|row| *row >= row_offset && *row < row_offset + area.rows)
            .map(|row| row - row_offset)
            .collect(),
    };
    rows.into_iter()
        .map(|row| {
            let source_row = row + row_offset;
            if source_row < screen.rows as usize {
                screen_row_chunk(
                    screen,
                    source_row as u16,
                    area.x,
                    area.y + row,
                    area.cols,
                    link_for,
                    styled_underlines,
                )
            } else {
                blank_row_chunk(area.x, area.y + row, area.cols)
            }
        })
        .collect()
}

fn truncate_to_width(text: &str, width: usize) -> (String, usize) {
    let mut out = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = character.width().unwrap_or(0);
        if used + character_width > width {
            break;
        }
        used += character_width;
        out.push(character);
    }
    (out, used)
}

pub fn placeholder_chunks(lines: &[String], area: &LiveViewRect) -> Vec<CharacterChunk> {
    if area.is_empty() {
        return vec![];
    }
    let first_row = area.rows.saturating_sub(lines.len()) / 2;
    (0..area.rows)
        .map(|row| {
            let line = row
                .checked_sub(first_row)
                .and_then(|index| lines.get(index).map(|line| (index, line)));
            let Some((index, line)) = line else {
                return blank_row_chunk(area.x, area.y + row, area.cols);
            };
            let (text, text_width) = truncate_to_width(line, area.cols);
            let left = (area.cols - text_width) / 2;
            let mut styles = RESET_STYLES;
            if index == 0 {
                styles.bold = Some(AnsiCode::On);
            } else {
                styles.dim = Some(AnsiCode::On);
            }
            let styles: crate::panes::terminal_character::RcCharacterStyles = styles.into();
            let mut characters: Vec<TerminalCharacter> = blank_characters(left).collect();
            for character in text.chars() {
                let width = character.width().unwrap_or(0);
                if width == 0 {
                    continue;
                }
                characters.push(TerminalCharacter::new_with_width(
                    character,
                    width as u8,
                    styles.clone(),
                ));
            }
            characters.extend(blank_characters(area.cols - left - text_width));
            CharacterChunk::new(characters, area.x, area.y + row)
        })
        .collect()
}

pub fn cursor_shape_csi(shape: u8) -> &'static str {
    let blinking = shape & CURSOR_BLINKING != 0;
    match (shape & !CURSOR_BLINKING, blinking) {
        (CURSOR_SHAPE_BLOCK, true) => "\u{1b}[1 q",
        (CURSOR_SHAPE_BLOCK, false) => "\u{1b}[2 q",
        (CURSOR_SHAPE_UNDERLINE, true) => "\u{1b}[3 q",
        (CURSOR_SHAPE_UNDERLINE, false) => "\u{1b}[4 q",
        (CURSOR_SHAPE_BEAM, true) => "\u{1b}[5 q",
        (CURSOR_SHAPE_BEAM, false) => "\u{1b}[6 q",
        _ => "\u{1b}[0 q",
    }
}

#[cfg(test)]
#[path = "./unit/cells_tests.rs"]
mod cells_tests;

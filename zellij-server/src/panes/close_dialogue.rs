// TODO: temporary hard-coded dialogue; replace with a popup once popups are merged
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;
use zellij_utils::data::{BareKey, KeyModifier, KeyWithModifier, Style};

use crate::output::CharacterChunk;
use crate::panes::terminal_character::{
    AnsiCode, RcCharacterStyles, TerminalCharacter, RESET_STYLES,
};
use crate::panes::PaneId;

pub const DETACH_OPTION: usize = 0;
pub const QUIT_OPTION: usize = 1;
pub const CANCEL_OPTION: usize = 2;
const OPTION_COUNT: usize = 3;
const NARROWEST: usize = 20;
const HINT: &str = "<↓↑> select  <Enter> confirm  <1-3> direct  <y> detach  <q> quit  <Esc> cancel";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseDialogue {
    pub pane_id: PaneId,
    pub selected: usize,
    pub session_name: String,
}

impl CloseDialogue {
    pub fn new(pane_id: PaneId, session_name: String) -> Self {
        Self {
            pane_id,
            selected: DETACH_OPTION,
            session_name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDialogueOutcome {
    Detach,
    Cancel,
    Quit,
}

impl CloseDialogueOutcome {
    pub fn of_option(option: usize) -> Self {
        match option {
            DETACH_OPTION => CloseDialogueOutcome::Detach,
            QUIT_OPTION => CloseDialogueOutcome::Quit,
            _ => CloseDialogueOutcome::Cancel,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDialogueInput {
    Select(usize),
    Choose(CloseDialogueOutcome),
    Swallow,
}

enum Gesture {
    Previous,
    Next,
    First,
    Second,
    Third,
    Confirm,
    Quit,
    Yes,
    No,
}

fn gesture_of_key(key: &KeyWithModifier) -> Option<Gesture> {
    let plain = key
        .key_modifiers
        .iter()
        .all(|modifier| *modifier == KeyModifier::Shift);
    if !plain {
        return None;
    }
    match key.bare_key {
        BareKey::Up | BareKey::Char('k') => Some(Gesture::Previous),
        BareKey::Down | BareKey::Char('j') => Some(Gesture::Next),
        BareKey::Char('1') => Some(Gesture::First),
        BareKey::Char('2') => Some(Gesture::Second),
        BareKey::Char('3') => Some(Gesture::Third),
        BareKey::Char('q') | BareKey::Char('Q') => Some(Gesture::Quit),
        BareKey::Enter => Some(Gesture::Confirm),
        BareKey::Char('y') | BareKey::Char('Y') => Some(Gesture::Yes),
        BareKey::Char('n') | BareKey::Char('N') | BareKey::Esc => Some(Gesture::No),
        _ => None,
    }
}

fn gesture_of_bytes(raw_bytes: &[u8]) -> Option<Gesture> {
    match raw_bytes {
        b"\x1b[A" | b"\x1bOA" | b"k" => Some(Gesture::Previous),
        b"\x1b[B" | b"\x1bOB" | b"j" => Some(Gesture::Next),
        b"1" => Some(Gesture::First),
        b"2" => Some(Gesture::Second),
        b"3" => Some(Gesture::Third),
        b"q" | b"Q" => Some(Gesture::Quit),
        b"\r" | b"\n" | b"\r\n" => Some(Gesture::Confirm),
        b"y" | b"Y" => Some(Gesture::Yes),
        b"n" | b"N" | b"\x1b" => Some(Gesture::No),
        _ => None,
    }
}

pub fn close_dialogue_input(
    key: Option<&KeyWithModifier>,
    raw_bytes: &[u8],
    selected: usize,
) -> CloseDialogueInput {
    let gesture = match key {
        Some(key) => gesture_of_key(key),
        None => gesture_of_bytes(raw_bytes),
    };
    match gesture {
        Some(Gesture::Previous) => {
            CloseDialogueInput::Select((selected + OPTION_COUNT - 1) % OPTION_COUNT)
        },
        Some(Gesture::Next) => CloseDialogueInput::Select((selected + 1) % OPTION_COUNT),
        Some(Gesture::First) => CloseDialogueInput::Choose(CloseDialogueOutcome::Detach),
        Some(Gesture::Second) | Some(Gesture::Quit) => {
            CloseDialogueInput::Choose(CloseDialogueOutcome::Quit)
        },
        Some(Gesture::Third) => CloseDialogueInput::Choose(CloseDialogueOutcome::Cancel),
        Some(Gesture::Confirm) => {
            CloseDialogueInput::Choose(CloseDialogueOutcome::of_option(selected))
        },
        Some(Gesture::Yes) => CloseDialogueInput::Choose(CloseDialogueOutcome::Detach),
        Some(Gesture::No) => CloseDialogueInput::Choose(CloseDialogueOutcome::Cancel),
        None => CloseDialogueInput::Swallow,
    }
}

type Segment = (String, RcCharacterStyles);

struct Styles {
    fill: RcCharacterStyles,
    title: RcCharacterStyles,
    session: RcCharacterStyles,
    keycode: RcCharacterStyles,
    option_marker: RcCharacterStyles,
    option_title: RcCharacterStyles,
    selected_marker: RcCharacterStyles,
    selected_title: RcCharacterStyles,
    selected_fill: RcCharacterStyles,
}

fn styles_of(style: &Style) -> Styles {
    let base = style.colors.text_unselected;
    let selected = style.colors.text_selected;
    let styled = |foreground, background, bold: bool| -> RcCharacterStyles {
        let styles = RESET_STYLES
            .foreground(Some(AnsiCode::from(foreground)))
            .background(Some(AnsiCode::from(background)));
        if bold {
            styles.bold(Some(AnsiCode::On)).into()
        } else {
            styles.into()
        }
    };
    Styles {
        fill: styled(base.base, base.background, false),
        title: styled(base.emphasis_0, base.background, true),
        session: styled(base.emphasis_2, base.background, true),
        keycode: styled(base.emphasis_3, base.background, true),
        option_marker: styled(base.base, base.background, true),
        option_title: styled(base.emphasis_1, base.background, true),
        selected_marker: styled(selected.base, selected.background, true),
        selected_title: styled(selected.emphasis_1, selected.background, true),
        selected_fill: styled(selected.base, selected.background, false),
    }
}

fn row_width(segments: &[Segment]) -> usize {
    segments.iter().map(|(text, _)| text.width()).sum()
}

fn hint_segments(
    hint: &str,
    fill: &RcCharacterStyles,
    keycode: &RcCharacterStyles,
) -> Vec<Segment> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut buffer = String::new();
    for character in hint.chars() {
        match character {
            '<' => {
                if !buffer.is_empty() {
                    segments.push((std::mem::take(&mut buffer), fill.clone()));
                }
                buffer.push('<');
            },
            '>' => {
                buffer.push('>');
                segments.push((std::mem::take(&mut buffer), keycode.clone()));
            },
            _ => buffer.push(character),
        }
    }
    if !buffer.is_empty() {
        segments.push((buffer, fill.clone()));
    }
    segments
}

fn wrapped(words: &[Segment], width: usize) -> Vec<Vec<Segment>> {
    let width = width.max(1);
    let mut lines: Vec<Vec<Segment>> = Vec::new();
    let mut current: Vec<Segment> = Vec::new();
    let mut current_width = 0;
    for (word, styles) in words {
        let word_width = word.width();
        if !current.is_empty() && current_width + 1 + word_width > width {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        if !current.is_empty() {
            current.push((" ".to_string(), styles.clone()));
            current_width += 1;
        }
        current.push((word.clone(), styles.clone()));
        current_width += word_width;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

struct Row {
    segments: Vec<Segment>,
    block_fill: RcCharacterStyles,
}

struct Layout {
    rows: Vec<Row>,
    block_width: usize,
    option_rows: Vec<usize>,
    left_pad: usize,
    start_row: usize,
}

fn centred(block_width: usize, columns: usize) -> usize {
    columns.saturating_sub(block_width) / 2
}

fn fallback_layout(columns: usize, rows: usize, styles: &Styles) -> Layout {
    let segments: Vec<Segment> = vec![
        ("Close this window? ".to_string(), styles.title.clone()),
        ("<y>".to_string(), styles.keycode.clone()),
        (" detach ".to_string(), styles.fill.clone()),
        ("<q>".to_string(), styles.keycode.clone()),
        (" quit ".to_string(), styles.fill.clone()),
        ("<Esc>".to_string(), styles.keycode.clone()),
        (" cancel".to_string(), styles.fill.clone()),
    ];
    let block_width = row_width(&segments).min(columns);
    Layout {
        rows: vec![Row {
            segments,
            block_fill: styles.fill.clone(),
        }],
        block_width,
        option_rows: Vec::new(),
        left_pad: centred(block_width, columns),
        start_row: rows / 2,
    }
}

fn layout(
    columns: usize,
    rows: usize,
    styles: &Styles,
    session_name: &str,
    selected: usize,
) -> Layout {
    if columns < NARROWEST {
        return fallback_layout(columns, rows, styles);
    }
    let title: Vec<Segment> = vec![("Close this window?".to_string(), styles.title.clone())];
    let hint = hint_segments(HINT, &styles.fill, &styles.keycode);
    let options = [
        ("1. ", "Detach and leave running in the background."),
        ("2. ", "Quit and shut down the entire session."),
        ("3. ", "Cancel"),
    ];

    let mut words: Vec<Segment> = vec![("The session".to_string(), styles.fill.clone())];
    words.push((format!("\"{}\"", session_name), styles.session.clone()));
    for word in "keeps running and can be attached again.".split_whitespace() {
        words.push((word.to_string(), styles.fill.clone()));
    }

    let mut block_width = row_width(&title).max(row_width(&hint));
    for (number, option_title) in options.iter() {
        block_width = block_width.max(2 + number.width() + option_title.width());
    }
    block_width = block_width.min(columns);

    let mut content: Vec<Row> = Vec::new();
    let blank = |styles: &Styles| Row {
        segments: Vec::new(),
        block_fill: styles.fill.clone(),
    };
    content.push(Row {
        segments: title,
        block_fill: styles.fill.clone(),
    });
    content.push(blank(styles));
    for line in wrapped(&words, block_width) {
        content.push(Row {
            segments: line,
            block_fill: styles.fill.clone(),
        });
    }
    content.push(blank(styles));
    let mut option_rows = Vec::new();
    for (index, (number, option_title)) in options.iter().enumerate() {
        let is_selected = index == selected;
        let (marker, marker_styles, title_styles, block_fill) = if is_selected {
            (
                "> ",
                styles.selected_marker.clone(),
                styles.selected_title.clone(),
                styles.selected_fill.clone(),
            )
        } else {
            (
                "  ",
                styles.option_marker.clone(),
                styles.option_title.clone(),
                styles.fill.clone(),
            )
        };
        option_rows.push(content.len());
        content.push(Row {
            segments: vec![
                (format!("{}{}", marker, number), marker_styles),
                (option_title.to_string(), title_styles),
            ],
            block_fill,
        });
    }
    content.push(blank(styles));
    content.push(Row {
        segments: hint,
        block_fill: styles.fill.clone(),
    });

    if rows < content.len() {
        return fallback_layout(columns, rows, styles);
    }
    Layout {
        start_row: (rows - content.len()) / 2,
        rows: content,
        block_width,
        option_rows,
        left_pad: centred(block_width, columns),
    }
}

#[allow(clippy::too_many_arguments)]
fn segment_row(
    segments: &[Segment],
    left_pad: usize,
    block_width: usize,
    columns: usize,
    content_x: usize,
    absolute_y: usize,
    outer_fill: &RcCharacterStyles,
    block_fill: &RcCharacterStyles,
) -> CharacterChunk {
    let mut characters: Vec<TerminalCharacter> = Vec::with_capacity(columns);
    let block_end = (left_pad + block_width).min(columns);
    let mut used = 0;
    while used < left_pad.min(columns) {
        characters.push(TerminalCharacter::new_singlewidth_styled(
            ' ',
            outer_fill.clone(),
        ));
        used += 1;
    }
    'segments: for (text, styles) in segments {
        for character in text.chars() {
            let width = character.width().unwrap_or(0).max(1);
            if used + width > block_end {
                break 'segments;
            }
            characters.push(TerminalCharacter::new_styled(character, styles.clone()));
            used += width;
        }
    }
    while used < block_end {
        characters.push(TerminalCharacter::new_singlewidth_styled(
            ' ',
            block_fill.clone(),
        ));
        used += 1;
    }
    while used < columns {
        characters.push(TerminalCharacter::new_singlewidth_styled(
            ' ',
            outer_fill.clone(),
        ));
        used += 1;
    }
    CharacterChunk::new(characters, content_x, absolute_y)
}

pub fn close_dialogue_chunks(
    columns: usize,
    rows: usize,
    content_x: usize,
    content_y: usize,
    style: &Style,
    session_name: &str,
    selected: usize,
) -> Vec<CharacterChunk> {
    if rows < 1 || columns < 1 {
        return Vec::new();
    }
    let styles = styles_of(style);
    let layout = layout(columns, rows, &styles, session_name, selected);
    (0..rows)
        .map(|row| {
            let placed = row
                .checked_sub(layout.start_row)
                .and_then(|offset| layout.rows.get(offset));
            match placed {
                Some(placed) => segment_row(
                    &placed.segments,
                    layout.left_pad,
                    layout.block_width,
                    columns,
                    content_x,
                    content_y + row,
                    &styles.fill,
                    &placed.block_fill,
                ),
                None => segment_row(
                    &[],
                    0,
                    0,
                    columns,
                    content_x,
                    content_y + row,
                    &styles.fill,
                    &styles.fill,
                ),
            }
        })
        .collect()
}

pub fn close_dialogue_option_at_content_row(
    rows: usize,
    columns: usize,
    row: usize,
    style: &Style,
    session_name: &str,
    selected: usize,
) -> Option<usize> {
    let styles = styles_of(style);
    let layout = layout(columns, rows, &styles, session_name, selected);
    layout
        .option_rows
        .iter()
        .position(|option_row| layout.start_row + option_row == row)
}

#[cfg(test)]
#[path = "./unit/close_dialogue_tests.rs"]
mod close_dialogue_tests;

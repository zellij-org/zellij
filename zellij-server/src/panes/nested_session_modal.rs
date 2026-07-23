use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;
use zellij_utils::data::Style;

use crate::output::CharacterChunk;
use crate::panes::terminal_character::{
    AnsiCode, RcCharacterStyles, TerminalCharacter, RESET_STYLES,
};

#[derive(Debug, Clone, Default)]
pub struct GuestModalShortcuts {
    pub zoom: Vec<String>,
    pub ascend: Vec<String>,
    pub descend: Vec<String>,
}

type GuestModalSegment = (String, RcCharacterStyles);

fn guest_modal_row_width(segments: &[GuestModalSegment]) -> usize {
    segments.iter().map(|(text, _)| text.width()).sum()
}

fn guest_modal_empty_row(
    columns: usize,
    content_x: usize,
    absolute_y: usize,
    fill_styles: RcCharacterStyles,
) -> CharacterChunk {
    guest_modal_segment_row(
        &[],
        0,
        0,
        columns,
        content_x,
        absolute_y,
        fill_styles.clone(),
        fill_styles,
    )
}

#[allow(clippy::too_many_arguments)]
fn guest_modal_segment_row(
    segments: &[GuestModalSegment],
    left_pad: usize,
    block_width: usize,
    columns: usize,
    content_x: usize,
    absolute_y: usize,
    outer_fill: RcCharacterStyles,
    block_fill: RcCharacterStyles,
) -> CharacterChunk {
    let mut characters: Vec<TerminalCharacter> = Vec::with_capacity(columns);
    let block_end = (left_pad + block_width).min(columns);
    let mut used = 0;
    while used < left_pad && used < columns {
        characters.push(TerminalCharacter::new_singlewidth_styled(
            ' ',
            outer_fill.clone(),
        ));
        used += 1;
    }
    for (text, styles) in segments {
        for character in text.chars() {
            let width = character.width().unwrap_or(0).max(1);
            if used + width > columns {
                break;
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

fn guest_modal_hint_segments(
    hint: &str,
    fill_styles: RcCharacterStyles,
    keycode_styles: RcCharacterStyles,
) -> Vec<GuestModalSegment> {
    let mut segments: Vec<GuestModalSegment> = Vec::new();
    let mut buffer = String::new();
    let mut in_keycode = false;
    for character in hint.chars() {
        match character {
            '<' => {
                if !buffer.is_empty() {
                    segments.push((std::mem::take(&mut buffer), fill_styles.clone()));
                }
                buffer.push('<');
                in_keycode = true;
            },
            '>' => {
                buffer.push('>');
                segments.push((std::mem::take(&mut buffer), keycode_styles.clone()));
                in_keycode = false;
            },
            _ => buffer.push(character),
        }
    }
    if !buffer.is_empty() {
        let styles = if in_keycode {
            keycode_styles
        } else {
            fill_styles
        };
        segments.push((buffer, styles));
    }
    segments
}

struct GuestModalStyles {
    fill: RcCharacterStyles,
    title: RcCharacterStyles,
    session: RcCharacterStyles,
    keycode: RcCharacterStyles,
    prompt: RcCharacterStyles,
    option_marker: RcCharacterStyles,
    option_title: RcCharacterStyles,
    selected_marker: RcCharacterStyles,
    selected_title: RcCharacterStyles,
    selected_fill: RcCharacterStyles,
    selected_keycode: RcCharacterStyles,
}

fn guest_modal_keycode_words(
    keys: &[String],
    fill_styles: RcCharacterStyles,
    keycode_styles: RcCharacterStyles,
) -> Vec<GuestModalSegment> {
    if keys.is_empty() {
        return vec![("<unbound>".to_string(), fill_styles)];
    }
    let mut words: Vec<GuestModalSegment> = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        if index > 0 {
            words.push(("+".to_string(), fill_styles.clone()));
        }
        words.push((format!("<{}>", key), keycode_styles.clone()));
    }
    words
}

fn guest_modal_text_words(text: &str, styles: RcCharacterStyles) -> Vec<GuestModalSegment> {
    text.split_whitespace()
        .map(|word| (word.to_string(), styles.clone()))
        .collect()
}

fn guest_modal_wrap_words(
    words: &[GuestModalSegment],
    indent: usize,
    indent_styles: RcCharacterStyles,
    width: usize,
) -> Vec<Vec<GuestModalSegment>> {
    let content_width = width.saturating_sub(indent).max(1);
    let mut lines: Vec<Vec<GuestModalSegment>> = Vec::new();
    let mut current: Vec<GuestModalSegment> = Vec::new();
    let mut current_width = 0;
    for (word, styles) in words {
        let word_width = word.width();
        let extra = if current.is_empty() {
            word_width
        } else {
            1 + word_width
        };
        if !current.is_empty() && current_width + extra > content_width {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        if current.is_empty() {
            current.push((word.clone(), styles.clone()));
            current_width = word_width;
        } else {
            current.push((" ".to_string(), styles.clone()));
            current.push((word.clone(), styles.clone()));
            current_width += 1 + word_width;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    for line in lines.iter_mut() {
        line.insert(0, (" ".repeat(indent), indent_styles.clone()));
    }
    lines
}

pub fn guest_modal_chunks(
    columns: usize,
    rows: usize,
    content_x: usize,
    content_y: usize,
    style: &Style,
    session_name: &str,
    selected: usize,
    shortcuts: &GuestModalShortcuts,
) -> Vec<CharacterChunk> {
    if rows < 1 || columns < 1 {
        return vec![];
    }
    let styles = guest_modal_styles(style);
    let fill_styles = styles.fill.clone();

    let mut chunks: Vec<CharacterChunk> = Vec::with_capacity(rows);
    for row in 0..rows {
        chunks.push(guest_modal_empty_row(
            columns,
            content_x,
            content_y + row,
            fill_styles.clone(),
        ));
    }

    let layout = guest_modal_layout(columns, rows, &styles, session_name, selected, shortcuts);
    for (offset, row) in layout.rows.iter().enumerate() {
        let current_row = layout.start_row + offset;
        if current_row >= rows {
            break;
        }
        chunks[current_row] = guest_modal_segment_row(
            &row.segments,
            layout.left_pad,
            layout.block_width,
            columns,
            content_x,
            content_y + current_row,
            fill_styles.clone(),
            row.block_fill.clone(),
        );
    }

    chunks
}

fn guest_modal_styles(style: &Style) -> GuestModalStyles {
    let base_declaration = style.colors.text_unselected;
    let selected_declaration = style.colors.text_selected;
    let fill: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.base)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .into();
    let title: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.emphasis_0)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let session: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.emphasis_2)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let keycode: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.emphasis_3)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let prompt: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.base)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let option_marker: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.base)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let option_title: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.emphasis_1)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let selected_marker: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(selected_declaration.base)))
        .background(Some(AnsiCode::from(selected_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let selected_title: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(selected_declaration.emphasis_1)))
        .background(Some(AnsiCode::from(selected_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let selected_fill: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(selected_declaration.base)))
        .background(Some(AnsiCode::from(selected_declaration.background)))
        .into();
    let selected_keycode: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(selected_declaration.emphasis_3)))
        .background(Some(AnsiCode::from(selected_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    GuestModalStyles {
        fill,
        title,
        session,
        keycode,
        prompt,
        option_marker,
        option_title,
        selected_marker,
        selected_title,
        selected_fill,
        selected_keycode,
    }
}

struct GuestModalRow {
    segments: Vec<GuestModalSegment>,
    block_fill: RcCharacterStyles,
}

struct GuestModalLayout {
    rows: Vec<GuestModalRow>,
    block_width: usize,
    option_rows: Vec<usize>,
    left_pad: usize,
    start_row: usize,
}

fn guest_modal_fallback_layout(
    columns: usize,
    rows: usize,
    styles: &GuestModalStyles,
    session_name: &str,
) -> GuestModalLayout {
    let fallback: Vec<GuestModalSegment> = vec![
        ("Nested Zellij session: ".to_string(), styles.title.clone()),
        (session_name.to_string(), styles.session.clone()),
        (" ".to_string(), styles.fill.clone()),
        ("<Esc>".to_string(), styles.keycode.clone()),
        (" dismiss".to_string(), styles.fill.clone()),
    ];
    let block_width = guest_modal_row_width(&fallback).min(columns);
    let left_pad = center_modal_pad(block_width, columns);
    GuestModalLayout {
        rows: vec![GuestModalRow {
            segments: fallback,
            block_fill: styles.fill.clone(),
        }],
        block_width,
        option_rows: vec![],
        left_pad,
        start_row: rows / 2,
    }
}

fn guest_modal_layout(
    columns: usize,
    rows: usize,
    styles: &GuestModalStyles,
    session_name: &str,
    selected: usize,
    shortcuts: &GuestModalShortcuts,
) -> GuestModalLayout {
    if columns < 20 {
        return guest_modal_fallback_layout(columns, rows, styles, session_name);
    }

    let title: Vec<GuestModalSegment> = vec![
        (
            "Nested Zellij session detected: ".to_string(),
            styles.title.clone(),
        ),
        (session_name.to_string(), styles.session.clone()),
    ];
    let prompt: Vec<GuestModalSegment> = vec![(
        "What would you like to do?".to_string(),
        styles.prompt.clone(),
    )];
    let options = [
        ("1. ", "Zoom in and control this session"),
        ("2. ", "Control this session on focus"),
    ];
    let explanation_texts = [
        "The session will take up the whole screen. Toggle this on/off with",
        "When this pane gains focus, keybindings will be sent to this session. Ascend to the current session with",
    ];
    let shortcut_keys = [&shortcuts.zoom, &shortcuts.ascend];
    let hint = "<↓↑> select  <Enter> confirm  <1-2> direct  <Esc> dismiss";
    let hint_segments =
        guest_modal_hint_segments(hint, styles.fill.clone(), styles.keycode.clone());

    let fixed_lines: Vec<&[GuestModalSegment]> = vec![
        title.as_slice(),
        prompt.as_slice(),
        hint_segments.as_slice(),
    ];
    let mut block_width = fixed_lines
        .iter()
        .map(|segments| guest_modal_row_width(segments))
        .max()
        .unwrap_or(0);
    for (number, option_title) in options.iter() {
        block_width = block_width.max(2 + number.width() + option_title.width());
    }
    let block_width = block_width.min(columns);

    let mut rows_content: Vec<GuestModalRow> = Vec::new();
    let mut option_rows: Vec<usize> = Vec::new();
    rows_content.push(GuestModalRow {
        segments: title,
        block_fill: styles.fill.clone(),
    });
    rows_content.push(GuestModalRow {
        segments: Vec::new(),
        block_fill: styles.fill.clone(),
    });
    rows_content.push(GuestModalRow {
        segments: prompt,
        block_fill: styles.fill.clone(),
    });
    rows_content.push(GuestModalRow {
        segments: Vec::new(),
        block_fill: styles.fill.clone(),
    });
    for (index, (number, option_title)) in options.iter().enumerate() {
        let is_selected = index == selected;
        let (marker_styles, title_styles, text_styles, keycode_styles, block_fill) = if is_selected
        {
            (
                styles.selected_marker.clone(),
                styles.selected_title.clone(),
                styles.selected_fill.clone(),
                styles.selected_keycode.clone(),
                styles.selected_fill.clone(),
            )
        } else {
            (
                styles.option_marker.clone(),
                styles.option_title.clone(),
                styles.fill.clone(),
                styles.keycode.clone(),
                styles.fill.clone(),
            )
        };
        let marker = if is_selected { "> " } else { "  " };
        option_rows.push(rows_content.len());
        rows_content.push(GuestModalRow {
            segments: vec![
                (format!("{}{}", marker, number), marker_styles),
                (option_title.to_string(), title_styles),
            ],
            block_fill: block_fill.clone(),
        });
        let mut words = guest_modal_text_words(explanation_texts[index], text_styles.clone());
        words.extend(guest_modal_keycode_words(
            shortcut_keys[index],
            text_styles.clone(),
            keycode_styles,
        ));
        for line in guest_modal_wrap_words(&words, 5, text_styles.clone(), block_width) {
            rows_content.push(GuestModalRow {
                segments: line,
                block_fill: block_fill.clone(),
            });
        }
    }
    rows_content.push(GuestModalRow {
        segments: Vec::new(),
        block_fill: styles.fill.clone(),
    });
    rows_content.push(GuestModalRow {
        segments: hint_segments,
        block_fill: styles.fill.clone(),
    });

    let block_height = rows_content.len();
    if rows < block_height {
        return guest_modal_fallback_layout(columns, rows, styles, session_name);
    }
    let left_pad = center_modal_pad(block_width, columns);
    let start_row = rows.saturating_sub(block_height) / 2;

    GuestModalLayout {
        rows: rows_content,
        block_width,
        option_rows,
        left_pad,
        start_row,
    }
}

pub fn guest_modal_option_at_content_row(
    rows: usize,
    columns: usize,
    row: usize,
    style: &Style,
    session_name: &str,
    selected: usize,
    shortcuts: &GuestModalShortcuts,
) -> Option<usize> {
    if columns < 20 {
        return None;
    }
    let styles = guest_modal_styles(style);
    let layout = guest_modal_layout(columns, rows, &styles, session_name, selected, shortcuts);
    if layout.option_rows.is_empty() {
        return None;
    }
    layout
        .option_rows
        .iter()
        .position(|&option_row| layout.start_row + option_row == row)
}

fn center_modal_pad(block_width: usize, columns: usize) -> usize {
    if block_width >= columns {
        return 0;
    }
    (columns - block_width) / 2
}

#[cfg(test)]
#[path = "./unit/nested_session_modal_tests.rs"]
mod nested_session_modal_tests;

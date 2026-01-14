use super::{is_too_high, is_too_wide, stringify_text, Coordinates, Text};
use crate::panes::{
    terminal_character::{AnsiCode, RESET_STYLES},
    CharacterStyles,
};
use std::collections::BTreeMap;
use zellij_utils::{data::Style, shared::ansi_len};

pub fn table(
    columns: usize,
    _rows: usize,
    contents: Vec<Text>,
    style: &Style,
    coordinates: Option<Coordinates>,
) -> Vec<u8> {
    let mut stringified = String::new();
    // we first arrange the data by columns so that we can pad them by the widest one
    let stringified_columns = stringify_table_columns(contents, columns);
    let stringified_rows = stringify_table_rows(stringified_columns, &coordinates);
    for (row_index, (_, row)) in stringified_rows.into_iter().enumerate() {
        let is_title_row = row_index == 0;
        if is_too_high(row_index + 1, &coordinates) {
            break;
        }
        let cell_count = row.iter().len();
        for (cell_index, cell) in row.into_iter().enumerate() {
            let declaration = if is_title_row {
                style.colors.table_title
            } else {
                if cell.selected {
                    style.colors.table_cell_selected
                } else {
                    style.colors.table_cell_unselected
                }
            };

            let text_style = if cell.opaque || cell.selected {
                CharacterStyles::from(declaration).background(Some(declaration.background.into()))
            } else {
                CharacterStyles::from(declaration)
            };

            // Default: bold
            let text_style = text_style.bold(Some(AnsiCode::On));

            // here we intentionally don't pass our coordinates even if we have them, because
            // these cells have already been padded and truncated
            let (text, _text_width) =
                stringify_text(&cell, None, &None, &declaration, &style.colors, text_style);
            if cell_index == cell_count.saturating_sub(1) {
                // do not add padding between columns for the last cell
                stringified.push_str(&format!("{}{}{}", text_style, text, RESET_STYLES));
            } else {
                // add padding between columns
                stringified.push_str(&format!("{}{} {}", text_style, text, RESET_STYLES));
            }
        }
        let next_row_instruction = coordinates
            .as_ref()
            .map(|c| c.stringify_with_y_offset(row_index + 1))
            .unwrap_or_else(|| format!("\n\r"));
        stringified.push_str(&next_row_instruction);
    }
    if let Some(coordinates) = coordinates {
        format!("{}{}", coordinates, stringified)
            .as_bytes()
            .to_vec()
    } else {
        stringified.as_bytes().to_vec()
    }
}

fn stringify_table_columns(contents: Vec<Text>, columns: usize) -> BTreeMap<usize, Vec<Text>> {
    let mut stringified_columns: BTreeMap<usize, Vec<Text>> = BTreeMap::new();
    for (i, cell) in contents.into_iter().enumerate() {
        let column_index = i % columns;
        stringified_columns
            .entry(column_index)
            .or_insert_with(Vec::new)
            .push(cell);
    }
    stringified_columns
}

fn max_table_column_width(column: &Vec<Text>) -> usize {
    let mut max_column_width = 0;
    for cell in column {
        let cell_width = ansi_len(&cell.text);
        if cell_width > max_column_width {
            max_column_width = cell_width;
        }
    }
    max_column_width
}

fn stringify_table_rows(
    stringified_columns: BTreeMap<usize, Vec<Text>>,
    coordinates: &Option<Coordinates>,
) -> BTreeMap<usize, Vec<Text>> {
    let mut stringified_rows: BTreeMap<usize, Vec<Text>> = BTreeMap::new();
    let mut row_width = 0;
    for (_, column) in stringified_columns.into_iter() {
        let max_column_width = max_table_column_width(&column);
        if is_too_wide(max_column_width + 1, row_width, &coordinates) {
            break;
        }
        row_width += max_column_width + 1;
        for (row_index, mut cell) in column.into_iter().enumerate() {
            cell.pad_text(max_column_width);
            stringified_rows
                .entry(row_index)
                .or_insert_with(Vec::new)
                .push(cell);
        }
    }
    stringified_rows
}

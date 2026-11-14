use super::text::text_cell_width;
use super::{stringify_text, Coordinates, Text};
use crate::panes::{
    terminal_character::{AnsiCode, RESET_STYLES},
    CharacterStyles,
};
use std::collections::BTreeMap;
use zellij_utils::data::Style;

pub fn table(
    columns: usize,
    _rows: usize,
    contents: Vec<Text>,
    style: &Style,
    coordinates: Option<Coordinates>,
    max_columns: usize,
    max_rows: usize,
) -> Vec<u8> {
    let mut stringified = String::new();
    // what is not drawn is not padded either (a padded cell is as wide as the widest one of its
    // column, so it is the product of the two that grows; the widths still come from all the
    // cells, so that the columns do not move with the rows that are shown)
    // (the explicit width and height are already clamped to the pane; the pane's own width gets
    // one more column since each column is counted with its separator, which the last has not)
    let max_columns = coordinates
        .as_ref()
        .and_then(|c| c.width)
        .unwrap_or(max_columns.saturating_add(1));
    let max_rows = coordinates
        .as_ref()
        .and_then(|c| c.height)
        .unwrap_or(max_rows);
    // we first arrange the data by columns so that we can pad them by the widest one
    let stringified_columns = stringify_table_columns(contents, columns);
    let stringified_rows = stringify_table_rows(stringified_columns, max_columns, max_rows);
    for (row_index, (_, row)) in stringified_rows.into_iter().enumerate() {
        let is_title_row = row_index == 0;
        if row_index + 1 > max_rows {
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
        let cell_width = text_cell_width(&cell.text);
        if cell_width > max_column_width {
            max_column_width = cell_width;
        }
    }
    max_column_width
}

fn stringify_table_rows(
    stringified_columns: BTreeMap<usize, Vec<Text>>,
    max_width: usize,
    max_rows: usize,
) -> BTreeMap<usize, Vec<Text>> {
    let mut stringified_rows: BTreeMap<usize, Vec<Text>> = BTreeMap::new();
    let mut row_width = 0;
    for (_, column) in stringified_columns.into_iter() {
        let max_column_width = max_table_column_width(&column);
        if row_width + max_column_width + 1 > max_width {
            break;
        }
        row_width += max_column_width + 1;
        for (row_index, mut cell) in column.into_iter().take(max_rows).enumerate() {
            cell.pad_text(max_column_width);
            stringified_rows
                .entry(row_index)
                .or_insert_with(Vec::new)
                .push(cell);
        }
    }
    stringified_rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(count: usize, text: &str) -> Vec<Text> {
        (0..count).map(|_| Text::plain(text)).collect()
    }

    #[test]
    fn cells_are_not_padded_to_a_column_wider_than_the_pane() {
        // the cells of a column are padded to the widest one of them: with many cells and one
        // wide one the padding is their product
        for max_rows in [10, 50] {
            let mut contents = vec![Text::plain("b".repeat(5000))];
            contents.extend(cells(2000, "a"));
            let output = table(1, 0, contents, &Style::default(), None, 40, max_rows);
            assert!(output.len() < 10_000, "{} bytes", output.len());
        }
    }

    #[test]
    fn only_the_rows_of_the_pane_are_arranged() {
        let output = table(1, 0, cells(3000, "a"), &Style::default(), None, 40, 10);
        let text = String::from_utf8(output).unwrap();
        assert_eq!(text.matches('a').count(), 10);
    }

    #[test]
    fn a_table_as_wide_as_the_pane_keeps_its_last_column() {
        // "a" and "b" with a separator after the first: 3 columns, in a pane of 3
        let contents = vec![Text::plain("a"), Text::plain("b")];
        let output = table(2, 0, contents, &Style::default(), None, 3, 10);
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains('a') && text.contains('b'));
    }
}

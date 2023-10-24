use zellij_utils::errors::prelude::*;
use std::collections::BTreeMap;
use crate::panes::{
    grid::Grid,
    terminal_character::{render_first_run_banner, TerminalCharacter, EMPTY_TERMINAL_CHARACTER, AnsiCode, CharacterStyles, RESET_STYLES},
};
use zellij_utils::{
    data::{InputMode, Palette, PaletteColor, Style},
    vte,
    shared::{version_number, ansi_len},
};
use crate::ui::boundaries::boundary_type;
#[derive(Debug)]
pub struct UiComponentParser <'a>{
    grid: &'a mut Grid,
    style: Style,
}

impl <'a> UiComponentParser <'a> {
    pub fn new(grid: &'a mut Grid, style: Style) -> Self {
        UiComponentParser {
            grid,
            style
        }
    }
    pub fn parse(&mut self, bytes: Vec<u8>) -> Result<()> {
        // The stages of parsing:
        // 1. We decode the bytes to utf8 and get something like (as a String): `component_name;111;222;333`
        // 2. We split this string by `;` to get at the parameters themselves
        // 3. We extract the component name, and then behave according to the component
        // 4. Some components interpret their parameters as bytes, and so have another layer of
        //    utf8 decoding, others would take them verbatim, some will act depending on their
        //    placement (eg. the `table` component treats the first two parameters as integers for
        //    the columns/rows of the table, and then treats the rest of the component as utf8
        //    encoded bytes, each one representing one cell in the table)
        // 5. Each component parses its parameters, creating a String of ANSI instructions of its
        //    own representing instructions to create the component
        // 6. Finally, we take this string, encode it back into bytes and pass it back through the ANSI
        //    parser (our `Grid`) in order to create a representation of it on screen
        let params: Vec<String> = String::from_utf8_lossy(&bytes)
            .to_string()
            .split(';')
            .map(|c| c.to_owned())
            .collect();
        let mut params_iter = params.iter();
        let component_name = params_iter
            .next()
            .with_context(|| format!("ui component must have a name"))?;

        if component_name == &"table" {
            let columns = params_iter.next()
                .and_then(|stringified_columns| stringified_columns.parse::<usize>().ok())
                .with_context(|| format!("table must have columns"))?;
            let rows = params_iter.next()
                .and_then(|stringified_rows| stringified_rows.parse::<usize>().ok())
                .with_context(|| format!("table must have rows"))?;
            let stringified_params = params_iter
                .flat_map(|stringified| {
                    let mut utf8 = vec![];
                    for stringified_character in stringified.split(',') {
                        utf8.push(
                            stringified_character
                                .to_string()
                                .parse::<u8>()
                                .map_err(|e| format!("Failed to parse utf8: {:?}", e))?
                        );
                    }
                    Ok::<String, String>(String::from_utf8_lossy(&utf8).to_string())
                })
                .collect::<Vec<String>>().into_iter();
            let mut vte_parser = vte::Parser::new();
            let encoded_table = table(columns, rows, stringified_params, Some(self.style.colors.green));
            for &byte in &encoded_table {
                vte_parser.advance(self.grid, byte);
            }
            Ok(())
        } else {
            Err(anyhow!("Unknown component: {}", component_name))
        }
    }
}

// UI COMPONENTS
fn table(columns: usize, rows: usize, contents: impl Iterator<Item=String>, title_color: Option<PaletteColor>) -> Vec<u8> {
    let mut stringified = String::new();

    // we first arrange the data by columns so that we can pad them by the widest one
    let mut stringified_columns: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (i, cell) in contents.enumerate() {
        let column_index = i % columns;
        stringified_columns.entry(column_index).or_insert_with(Vec::new).push(cell.to_owned());
    }

    // we pad the columns by the widest one (taking wide characters into account and not counting
    // any ANSI)
    let mut stringified_rows: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut row_width = 0;
    for stringified_column in stringified_columns.values() {
        let mut max_column_width = 0;
        for cell in stringified_column {
            let cell_width = ansi_len(cell);
            if cell_width > max_column_width {
                max_column_width = cell_width;
            }
        }
        row_width += max_column_width + 1;
        for (row_index, cell) in stringified_column.into_iter().enumerate() {
            let mut padded = cell.to_owned();
            for _ in ansi_len(cell)..max_column_width {
                padded.push(' ');
            }
            stringified_rows.entry(row_index).or_insert_with(Vec::new).push(padded);
        }

    }
    // default styles for titles and cells, since we do not drop any ANSI styling provided to us,
    // these can be overriden or added to
    let title_styles = CharacterStyles::new()
        .foreground(title_color.map(|t| t.into()))
        .bold(Some(AnsiCode::On));
    let cell_styles = CharacterStyles::new()
        .bold(Some(AnsiCode::On));
    for (row_index, row) in stringified_rows.values().into_iter().enumerate() {
        let is_title_row = row_index == 0;
        let is_last_row = row_index == rows.saturating_sub(1);
        for cell in row {
            if is_title_row {
                stringified.push_str(&format!("{}{}{} ", title_styles, cell, RESET_STYLES));
            } else {
                stringified.push_str(&format!("{}{}{} ", cell_styles, cell, RESET_STYLES));
            }
        }
        let mut title_underline = String::new();
        for _ in 0..row_width.saturating_sub(1) { // removing 1 because the last cell doesn't have
                                                  // padding
            title_underline.push_str(boundary_type::HORIZONTAL);
        }
        if !is_last_row {
            stringified.push_str("\n\r");
        }
        if is_title_row {
            stringified.push_str(&format!("{}\n\r", title_underline));
        }
    }
    stringified.as_bytes().to_vec()
}

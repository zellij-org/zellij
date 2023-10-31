use zellij_utils::errors::prelude::*;
use std::collections::BTreeMap;
use crate::panes::{
    grid::Grid,
    terminal_character::{AnsiCode, CharacterStyles, RESET_STYLES},
};
use zellij_utils::{
    data::{PaletteColor, Style},
    vte,
    shared::ansi_len,
    regex::Regex,
    lazy_static::lazy_static,
};
use zellij_utils::pane_size::{Dimension, PaneGeom, Size, SizeInPixels};

use crate::ui::boundaries::boundary_type;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

static ARROW_SEPARATOR: &str = "";

pub fn emphasis_variants(style: &Style) -> [PaletteColor;4] {
    [style.colors.orange, style.colors.cyan, style.colors.green, style.colors.magenta]
}

pub fn emphasis_variants_for_ribbon(style: &Style) -> [PaletteColor;4] {
    [style.colors.red, style.colors.white, style.colors.blue, style.colors.magenta]
}

pub fn emphasis_variants_for_selected_ribbon(style: &Style) -> [PaletteColor;4] {
    [style.colors.red, style.colors.orange, style.colors.magenta, style.colors.blue]
}

#[derive(Debug, Clone)]
struct NestedListItem {
    text: String,
    indentation_level: usize,
    selected: bool,
    indices: Vec<Vec<usize>>,
}

impl NestedListItem {
    pub fn style_of_index(&self, index: usize, style: &Style) -> Option<PaletteColor> {
        let index_variant_styles = emphasis_variants(style);
        for i in (0..=3).rev() { // we do this in reverse to give precedence to the last applied
                                 // style
            if let Some(indices) = self.indices.get(i) {
                if indices.contains(&index) {
                    return Some(index_variant_styles[i])
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
struct Text {
    text: String,
    selected: bool,
    indices: Vec<Vec<usize>>,
}

impl Text {
    pub fn pad_text(&mut self, max_column_width: usize) {
        // let mut padded = self.text.to_owned();
        for _ in ansi_len(&self.text)..max_column_width {
            self.text.push(' ');
        }
    }
    pub fn style_of_index(&self, index: usize, style: &Style) -> Option<PaletteColor> {
        let index_variant_styles = emphasis_variants(style);
        for i in (0..=3).rev() { // we do this in reverse to give precedence to the last applied
                                 // style
            if let Some(indices) = self.indices.get(i) {
                if indices.contains(&index) {
                    return Some(index_variant_styles[i])
                }
            }
        }
        None
    }
    pub fn style_of_index_for_ribbon(&self, index: usize, style: &Style) -> Option<PaletteColor> {
        let index_variant_styles = emphasis_variants_for_ribbon(style);
        for i in (0..=3).rev() { // we do this in reverse to give precedence to the last applied
                                 // style
            if let Some(indices) = self.indices.get(i) {
                if indices.contains(&index) {
                    return Some(index_variant_styles[i])
                }
            }
        }
        None
    }
    pub fn style_of_index_for_selected_ribbon(&self, index: usize, style: &Style) -> Option<PaletteColor> {
        let index_variant_styles = emphasis_variants_for_selected_ribbon(style);
        for i in (0..=3).rev() { // we do this in reverse to give precedence to the last applied
                                 // style
            if let Some(indices) = self.indices.get(i) {
                if indices.contains(&index) {
                    return Some(index_variant_styles[i])
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
struct Coordinates {
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
}

#[derive(Debug)]
pub struct UiComponentParser <'a>{
    grid: &'a mut Grid,
    style: Style,
    arrow_fonts: bool,
}

impl <'a> UiComponentParser <'a> {
    pub fn new(grid: &'a mut Grid, style: Style, arrow_fonts: bool) -> Self {
        UiComponentParser {
            grid,
            style,
            arrow_fonts,
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
        let mut params: Vec<String> = String::from_utf8_lossy(&bytes)
            .to_string()
            .split(';')
            .map(|c| c.to_owned())
            .collect();
        let mut params_iter = params.iter_mut().peekable();
        let component_name = params_iter
            .next()
            .with_context(|| format!("ui component must have a name"))?;


        // parse coordinates
        let mut component_coordinates = None;
        if let Some(coordinates) = params_iter.peek() {
            lazy_static! {
                static ref RE: Regex = Regex::new(r"(\d*)/(\d*)/(\d*)/(\d*)").unwrap();
            }
            if let Some(captures) = RE.captures_iter(&coordinates).next() {
                let x = captures[1].parse::<usize>()
                    .with_context(|| format!("Failed to parse x coordinates for string: {:?}", coordinates))?;
                let y = captures[2].parse::<usize>()
                    .with_context(|| format!("Failed to parse y coordinates for string: {:?}", coordinates))?;
                let width = captures[3].parse::<usize>().ok();
                let height = captures[4].parse::<usize>().ok();
                component_coordinates = Some(Coordinates {
                    x,
                    y,
                    width,
                    height,
                });
                let _ = params_iter.next(); // we just peeked, let's consume the coords now
            }
        }

        macro_rules! stringify_nested_list_items {
            ($params_iter:expr) => {{
                    $params_iter
                        .flat_map(|stringified| {
                            let mut utf8 = vec![];

                            // parse indentation level
                            let mut indentation_level = 0;
                            loop {
                                if stringified.is_empty() {
                                    break;
                                }
                                if stringified.chars().next() == Some('|') {
                                    stringified.remove(0);
                                    indentation_level += 1;
                                } else {
                                    break;
                                }
                            }

                            // parse selected
                            let mut selected = false;
                            if stringified.chars().next() == Some('x') {
                                selected = true;
                                stringified.remove(0);
                            }

                            // parse indices
                            let indices: Vec<Vec<usize>> = stringified
                                .chars()
                                .collect::<Vec<_>>()
                                .iter()
                                .rposition(|c| c == &'$')
                                .map(|last_position| {
                                    stringified.drain(0..=last_position).collect::<String>()
                                })
                                .map(|indices_string| {
                                    let mut all_indices = vec![];
                                    let raw_indices_for_each_variant = indices_string.split('$');
                                    for index_string in raw_indices_for_each_variant {
                                        let indices_for_variant = index_string.split(',').filter_map(|s| s.parse::<usize>().ok()).collect();
                                        all_indices.push(indices_for_variant)
                                    }
                                    all_indices
                                })
                                .unwrap_or_default();
                            for stringified_character in stringified.split(',') {
                                utf8.push(
                                    stringified_character
                                        .to_string()
                                        .parse::<u8>()
                                        .map_err(|e| format!("Failed to parse utf8: {:?}", e))?
                                );
                            }
                            let text = String::from_utf8_lossy(&utf8).to_string();
                            Ok::<NestedListItem, String>(NestedListItem { text, indentation_level, selected, indices })
                        })
                        .collect::<Vec<NestedListItem>>().into_iter()
            }}
        }
        macro_rules! stringify_text_params {
            ($params_iter:expr) => {{
                    $params_iter
                        .flat_map(|stringified| {
                            let mut utf8 = vec![];

                            // parse selected
                            let mut selected = false;
                            if stringified.chars().next() == Some('x') {
                                selected = true;
                                stringified.remove(0);
                            }

                            // parse indices
                            let indices: Vec<Vec<usize>> = stringified
                                .chars()
                                .collect::<Vec<_>>()
                                .iter()
                                .rposition(|c| c == &'$')
                                .map(|last_position| {
                                    stringified.drain(0..=last_position).collect::<String>()
                                })
                                .map(|indices_string| {
                                    let mut all_indices = vec![];
                                    let raw_indices_for_each_variant = indices_string.split('$');
                                    for index_string in raw_indices_for_each_variant {
                                        let indices_for_variant = index_string.split(',').filter_map(|s| s.parse::<usize>().ok()).collect();
                                        all_indices.push(indices_for_variant)
                                    }
                                    all_indices
                                })
                                .unwrap_or_default();
                            for stringified_character in stringified.split(',') {
                                utf8.push(
                                    stringified_character
                                        .to_string()
                                        .parse::<u8>()
                                        .map_err(|e| format!("Failed to parse utf8: {:?}", e))?
                                );
                            }
                            let text = String::from_utf8_lossy(&utf8).to_string();
                            Ok::<Text, String>(Text { text, selected, indices })
                        })
                        .collect::<Vec<Text>>().into_iter()
            }}
        }
        macro_rules! parse_next_param {
            ($next_param:expr, $type:ident, $component_name:expr, $item_name:expr) => {{
                $next_param
                    .and_then(|stringified_param| stringified_param.parse::<$type>().ok())
                    .with_context(|| format!("{} must have {}", $component_name, $item_name))?
            }}
        }
        macro_rules! parse_vte_bytes{
            ($self:expr, $encoded_component:expr) => {{
                let mut vte_parser = vte::Parser::new();
                for &byte in &$encoded_component {
                    vte_parser.advance($self.grid, byte);
                }
            }}
        }
        if component_name == &"table" {
            let columns = parse_next_param!(params_iter.next(), usize, "table", "columns");
            let rows = parse_next_param!(params_iter.next(), usize, "table", "rows");
            let stringified_params = stringify_text_params!(params_iter);
            let encoded_table = table(columns, rows, stringified_params, Some(self.style.colors.green), &self.style, component_coordinates);
            parse_vte_bytes!(self, encoded_table);
            Ok(())
        } else if component_name == &"ribbon" {
            let stringified_params = stringify_text_params!(params_iter).next().with_context(|| format!("a ribbon must have text"))?;
            let encoded_text = ribbon(stringified_params, &self.style, self.arrow_fonts, component_coordinates);
            parse_vte_bytes!(self, encoded_text);
            Ok(())
        } else if component_name == &"nested_list" {
            let nested_list_items = stringify_nested_list_items!(params_iter);
            let encoded_nested_list = nested_list(nested_list_items.collect(), &self.style, component_coordinates);
            parse_vte_bytes!(self, encoded_nested_list);
            Ok(())
        } else if component_name == &"text" {
            let stringified_params = stringify_text_params!(params_iter).next().with_context(|| format!("text must have, well, text..."))?;
            let encoded_text = text(stringified_params, &self.style, component_coordinates);
            parse_vte_bytes!(self, encoded_text);
            Ok(())

        } else {
            Err(anyhow!("Unknown component: {}", component_name))
        }
    }
}

// UI COMPONENTS
fn table(columns: usize, rows: usize, contents: impl Iterator<Item=Text>, title_color: Option<PaletteColor>, style: &Style, coordinates: Option<Coordinates>) -> Vec<u8> {
    let mut stringified = String::new();

    // we first arrange the data by columns so that we can pad them by the widest one
    let mut stringified_columns: BTreeMap<usize, Vec<Text>> = BTreeMap::new();
    for (i, cell) in contents.enumerate() {
        let column_index = i % columns;
        stringified_columns.entry(column_index).or_insert_with(Vec::new).push(cell);
    }

    // we pad the columns by the widest one (taking wide characters into account and not counting
    // any ANSI)
    let mut stringified_rows: BTreeMap<usize, Vec<Text>> = BTreeMap::new();
    let mut row_width = 0;
    for (_, stringified_column) in stringified_columns.into_iter() {
        let mut max_column_width = 0;
        for cell in &stringified_column {
            let cell_width = ansi_len(&cell.text);
            if cell_width > max_column_width {
                max_column_width = cell_width;
            }
        }
        if let Some(max_table_width) = coordinates.as_ref().and_then(|c| c.width) {
            if row_width + max_column_width + 1 > max_table_width {
                break;
            }
        }
        row_width += max_column_width + 1;
        for (row_index, mut cell) in stringified_column.into_iter().enumerate() {
            cell.pad_text(max_column_width);
            stringified_rows.entry(row_index).or_insert_with(Vec::new).push(cell);
        }

    }
    // default styles for titles and cells, since we do not drop any ANSI styling provided to us,
    // these can be overriden or added to
    let title_styles = RESET_STYLES
        .foreground(title_color.map(|t| t.into()))
        .bold(Some(AnsiCode::On));
    let cell_styles = RESET_STYLES
        .bold(Some(AnsiCode::On));
    for (row_index, (_, row)) in stringified_rows.into_iter().enumerate() {
        let is_title_row = row_index == 0;
        if let Some(max_table_height) = coordinates.as_ref().and_then(|c| c.height) {
            let current_height = row_index + 1;
            if current_height > max_table_height {
                break;
            }
        }
        for cell in row {
            let mut reset_styles_for_item = RESET_STYLES;
            let mut text_style = if is_title_row { title_styles } else { cell_styles };
            if cell.selected {
                reset_styles_for_item.background = None;
                text_style = text_style.background(Some(style.colors.bg.into()));
            }
            let text = if !cell.indices.is_empty() {
                let mut text = String::new();
                for (i, character) in cell.text.chars().enumerate() {
                    let character_style = cell.style_of_index(i, style)
                        .map(|foreground_style| text_style.foreground(Some(foreground_style.into())))
                        .unwrap_or(text_style);
                    text.push_str(&format!("{}{}{}", character_style, character, text_style));
                }
                text
            } else {
                format!("{}{}", text_style, cell.text)
            };
            if is_title_row {
                stringified.push_str(&format!("{}{}{} ", title_styles, text, reset_styles_for_item));
            } else {
                stringified.push_str(&format!("{}{}{} ", cell_styles, text, reset_styles_for_item));
            }
        }
        let next_row_instruction = if let Some(coordinates) = coordinates.as_ref() {
            format!("\u{1b}[{};{}H", coordinates.y + row_index + 1, coordinates.x)
        } else {
            format!("\n\r")
        };
        stringified.push_str(&next_row_instruction);
    }
    if let Some(coordinates) = coordinates {
        let x = coordinates.x;
        let y = coordinates.y;
        format!("\u{1b}[{};{}H{}", y, x, stringified).as_bytes().to_vec()
    } else {
        stringified.as_bytes().to_vec()
    }
}

fn text(content: Text, style: &Style, component_coordinates: Option<Coordinates>) -> Vec<u8> {
    let mut text_style = RESET_STYLES
        .bold(Some(AnsiCode::On));
    if content.selected {
        text_style = text_style.background(Some(style.colors.bg.into()));
    }
    let mut text = String::new();
    let mut text_width = 0;
    for (i, character) in content.text.chars().enumerate() {
        let character_width = character.width().unwrap_or(0);
        if let Some(max_width) = component_coordinates.as_ref().and_then(|p| p.width) {
            if text_width + character_width > max_width {
                break;
            }
        }
        if !content.indices.is_empty() {
            let character_style = content.style_of_index(i, style)
                .map(|foreground_style| text_style.foreground(Some(foreground_style.into())))
                .unwrap_or(text_style);
            text.push_str(&format!("{}{}{}", character_style, character, text_style));
        } else {
            text.push(character);
        }
        text_width += character_width;
    }
    if let Some(component_coordinates) = component_coordinates {
        let x = component_coordinates.x;
        let y = component_coordinates.y;
        format!("\u{1b}[{};{}H{}{}", y, x, text_style, text).as_bytes().to_vec()
    } else {
        format!("{}{}", text_style, text).as_bytes().to_vec()
    }
}

fn ribbon(content: Text, style: &Style, arrow_fonts: bool, component_coordinates: Option<Coordinates>) -> Vec<u8> {
    let (first_arrow_styles, text_style, last_arrow_styles) = if content.selected {
        (
            RESET_STYLES
                .foreground(Some(style.colors.black.into()))
                .background(Some(style.colors.green.into()))
                .bold(Some(AnsiCode::On)),
            RESET_STYLES
                .foreground(Some(style.colors.black.into()))
                .background(Some(style.colors.green.into()))
                .bold(Some(AnsiCode::On)),
            RESET_STYLES
                .foreground(Some(style.colors.green.into()))
                .background(Some(style.colors.black.into()))
                .bold(Some(AnsiCode::On)),
        )
    } else {
        (
            RESET_STYLES
                .foreground(Some(style.colors.black.into()))
                .background(Some(style.colors.fg.into()))
                .bold(Some(AnsiCode::On)),
            RESET_STYLES
                .foreground(Some(style.colors.black.into()))
                .background(Some(style.colors.fg.into()))
                .bold(Some(AnsiCode::On)),
            RESET_STYLES
                .foreground(Some(style.colors.fg.into()))
                .background(Some(style.colors.black.into()))
                .bold(Some(AnsiCode::On)),
        )
    };
    let mut text = String::new();
    let mut text_width = 0;
    for (i, character) in content.text.chars().enumerate() {
        let character_width = character.width().unwrap_or(0);
        if let Some(max_width) = component_coordinates.as_ref().and_then(|p| p.width) {
            let total_width = if arrow_fonts { text_width + 4 } else { text_width + 2 };
            if total_width + character_width > max_width {
                break;
            }
        }
        if !content.indices.is_empty() {
            let character_style = if content.selected {
                content.style_of_index_for_selected_ribbon(i, style)
                    .map(|foreground_style| text_style.foreground(Some(foreground_style.into())))
                    .unwrap_or(text_style)
            } else {
                content.style_of_index_for_ribbon(i, style)
                    .map(|foreground_style| text_style.foreground(Some(foreground_style.into())))
                    .unwrap_or(text_style)
            };
            text.push_str(&format!("{}{}{}", character_style, character, text_style));
        } else {
            text.push(character);
        }
        text_width += character_width;
    }
    let mut go_to_location = String::new();
    if let Some(coordinates) = component_coordinates.as_ref() {
        go_to_location = format!("\u{1b}[{};{}H", coordinates.y + 1, coordinates.x + 1);
    }
    let stringified = if arrow_fonts {
        format!("{}{}{}{}{} {} {}{}{}", RESET_STYLES, go_to_location, first_arrow_styles, ARROW_SEPARATOR, text_style, text, last_arrow_styles, ARROW_SEPARATOR, RESET_STYLES)
    } else {
        format!("{}{}{} {} {}", RESET_STYLES, go_to_location, text_style, text, RESET_STYLES)
    };
    stringified.as_bytes().to_vec()
}

fn nested_list(mut contents: Vec<NestedListItem>, style: &Style, coordinates: Option<Coordinates>) -> Vec<u8> {
    let mut stringified = String::new();
    let mut width_of_longest_line = 0;
    for line_item in contents.iter() {
        let mut line_item_text_width = 0;
        for character in line_item.text.chars() {
            let character_width = character.width().unwrap_or(0);
            line_item_text_width += character_width;
        }
        let bulletin_width = 2;
        let padding = line_item.indentation_level * 2 + 1;
        let total_width = line_item_text_width + bulletin_width + padding;
        if width_of_longest_line < total_width {
            width_of_longest_line = total_width;
        }
    }
    if let Some(max_width) = coordinates.as_ref().and_then(|c| c.width) {
        if width_of_longest_line > max_width {
            width_of_longest_line = max_width;
        }
    }
    for (line_index, line_item) in contents.drain(..).enumerate() {
        if let Some(max_height) = coordinates.as_ref().and_then(|c| c.height) {
            if line_index + 1 > max_height {
                break;
            }
        }
        let mut reset_styles_for_item = RESET_STYLES;
        if line_item.selected {
            reset_styles_for_item.background = None;
        };
        let padding = line_item.indentation_level * 2 + 1;
        let bulletin = if line_item.indentation_level % 2 == 0 { "> " } else { "- " };
        let text_style = reset_styles_for_item.bold(Some(AnsiCode::On));
        let mut text_width = 0;
        let mut text = if !line_item.indices.is_empty() {
            let mut text = String::new();
            for (i, character) in line_item.text.chars().enumerate() {
                let character_width = character.width().unwrap_or(0);
                if let Some(max_width) = coordinates.as_ref().and_then(|c| c.width) {
                    if padding + 2 + text_width + character_width > max_width {
                        break;
                    }
                }
                text_width += character_width;
                let character_style = line_item.style_of_index(i, style)
                    .map(|foreground_style| text_style.foreground(Some(foreground_style.into())))
                    .unwrap_or(text_style);
                text.push_str(&format!("{}{}{}", character_style, character, text_style));
            }
            text
        } else {
            line_item.text
        };
        let selected_background = RESET_STYLES.background(Some(style.colors.bg.into()));
        if width_of_longest_line > text_width + padding + 2 { // 2 is the bulletin
            let end_padding = width_of_longest_line.saturating_sub(text_width + padding + 2);
            log::info!("width_of_longest_line: {:?}", width_of_longest_line);
            log::info!("text_width + padding + 2: {:?}", text_width + padding + 2);
            log::info!("end_padding: {:?}", end_padding);
            let background = if line_item.selected { selected_background } else { RESET_STYLES };
            text = format!("{}{}{:end_padding$}", text, background, " ");
        }
        let go_to_row_instruction = if let Some(coordinates) = coordinates.as_ref() {
            format!("\u{1b}[{};{}H", coordinates.y + line_index + 1, coordinates.x)
        } else if line_index != 0 {
            format!("\n\r")
        } else {
            "".to_owned()
        };
        if line_item.selected {
            stringified.push_str(&format!("{}{}{}{:padding$}{bulletin}{}{text}{}", go_to_row_instruction, selected_background, reset_styles_for_item, " ", text_style, RESET_STYLES));
        } else {
            stringified.push_str(&format!("{}{}{:padding$}{bulletin}{}{text}{}", go_to_row_instruction, reset_styles_for_item, " ", text_style, RESET_STYLES));
        }
    }
    stringified.as_bytes().to_vec()
}

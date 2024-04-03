mod component_coordinates;
mod nested_list;
mod ribbon;
mod table;
mod text;

use crate::panes::grid::Grid;
use zellij_utils::errors::prelude::*;
use zellij_utils::{data::Style, lazy_static::lazy_static, regex::Regex, vte};

use component_coordinates::{is_too_high, is_too_wide, Coordinates};
use nested_list::{nested_list, parse_nested_list_items};
use ribbon::{emphasis_variants_for_ribbon, emphasis_variants_for_selected_ribbon, ribbon};
use table::table;
use text::{parse_text, parse_text_params, stringify_text, text, Text};

macro_rules! parse_next_param {
    ($next_param:expr, $type:ident, $component_name:expr, $item_name:expr) => {{
        $next_param
            .and_then(|stringified_param| stringified_param.parse::<$type>().ok())
            .with_context(|| format!("{} must have {}", $component_name, $item_name))?
    }};
}

macro_rules! parse_vte_bytes {
    ($self:expr, $encoded_component:expr) => {{
        let mut vte_parser = vte::Parser::new();
        for &byte in &$encoded_component {
            vte_parser.advance($self.grid, byte);
        }
    }};
}

#[derive(Debug)]
pub struct UiComponentParser<'a> {
    grid: &'a mut Grid,
    style: Style,
    arrow_fonts: bool,
}

impl<'a> UiComponentParser<'a> {
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
            component_coordinates = self.parse_coordinates(coordinates)?;
            if component_coordinates.is_some() {
                let _ = params_iter.next(); // we just peeked, let's consume the coords now
            }
        }

        if component_name == &"table" {
            let columns = parse_next_param!(params_iter.next(), usize, "table", "columns");
            let rows = parse_next_param!(params_iter.next(), usize, "table", "rows");
            let stringified_params = parse_text_params(params_iter);
            let encoded_table = table(
                columns,
                rows,
                stringified_params,
                Some(self.style.colors.table_title[0]),
                &self.style,
                component_coordinates,
            );
            parse_vte_bytes!(self, encoded_table);
            Ok(())
        } else if component_name == &"ribbon" {
            let stringified_params = parse_text_params(params_iter)
                .into_iter()
                .next()
                .with_context(|| format!("a ribbon must have text"))?;
            let encoded_text = ribbon(
                stringified_params,
                &self.style,
                self.arrow_fonts,
                component_coordinates,
            );
            parse_vte_bytes!(self, encoded_text);
            Ok(())
        } else if component_name == &"nested_list" {
            let nested_list_items = parse_nested_list_items(params_iter);
            let encoded_nested_list =
                nested_list(nested_list_items, &self.style, component_coordinates);
            parse_vte_bytes!(self, encoded_nested_list);
            Ok(())
        } else if component_name == &"text" {
            let stringified_params = parse_text_params(params_iter)
                .into_iter()
                .next()
                .with_context(|| format!("text must have, well, text..."))?;
            let encoded_text = text(stringified_params, &self.style, component_coordinates);
            parse_vte_bytes!(self, encoded_text);
            Ok(())
        } else {
            Err(anyhow!("Unknown component: {}", component_name))
        }
    }
    fn parse_coordinates(&self, coordinates: &str) -> Result<Option<Coordinates>> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"(\d*)/(\d*)/(\d*)/(\d*)").unwrap();
        }
        if let Some(captures) = RE.captures_iter(&coordinates).next() {
            let x = captures[1].parse::<usize>().with_context(|| {
                format!(
                    "Failed to parse x coordinates for string: {:?}",
                    coordinates
                )
            })?;
            let y = captures[2].parse::<usize>().with_context(|| {
                format!(
                    "Failed to parse y coordinates for string: {:?}",
                    coordinates
                )
            })?;
            let width = captures[3].parse::<usize>().ok();
            let height = captures[4].parse::<usize>().ok();
            Ok(Some(Coordinates {
                x,
                y,
                width,
                height,
            }))
        } else {
            Ok(None)
        }
    }
}

fn parse_selected(stringified: &mut String) -> bool {
    let mut selected = false;
    if stringified.chars().next() == Some('x') {
        selected = true;
        stringified.remove(0);
    }
    selected
}

fn parse_indices(stringified: &mut String) -> Vec<Vec<usize>> {
    stringified
        .chars()
        .collect::<Vec<_>>()
        .iter()
        .rposition(|c| c == &'$')
        .map(|last_position| stringified.drain(0..=last_position).collect::<String>())
        .map(|indices_string| {
            let mut all_indices = vec![];
            let raw_indices_for_each_variant = indices_string.split('$');
            for index_string in raw_indices_for_each_variant {
                let indices_for_variant = index_string
                    .split(',')
                    .filter_map(|s| s.parse::<usize>().ok())
                    .collect();
                all_indices.push(indices_for_variant)
            }
            all_indices
        })
        .unwrap_or_default()
}

mod button;
mod component_coordinates;
mod dialog;
mod dropdown;
mod menu;
mod nested_list;
mod ribbon;
mod scroll_indicator;
mod side_menu;
mod stepper;
mod table;
mod text;
mod text_input;
mod toggle;
mod widget_common;

use crate::panes::grid::Grid;
use crate::vt_parser;
use lazy_static::lazy_static;
use regex::Regex;
use zellij_utils::data::Style;
use zellij_utils::errors::prelude::*;

use component_coordinates::{is_too_high, is_too_wide, Coordinates};
use nested_list::{nested_list, parse_nested_list_items};
use ribbon::ribbon;
use table::table;
use text::{parse_text, parse_text_params, stringify_text, text, Text};
use widget_common::{WidgetRenderer, WidgetState};

fn widget_renderer(component_name: &str) -> Option<WidgetRenderer> {
    match component_name {
        "button" => Some(button::button),
        "dialog" => Some(dialog::dialog),
        "dropdown" => Some(dropdown::dropdown),
        "menu" => Some(menu::menu),
        "scroll_indicator" => Some(scroll_indicator::scroll_indicator),
        "side_menu" => Some(side_menu::side_menu),
        "stepper" => Some(stepper::stepper),
        "text_input" => Some(text_input::text_input),
        "toggle" => Some(toggle::toggle),
        _ => None,
    }
}

macro_rules! parse_next_param {
    ($next_param:expr, $type:ident, $component_name:expr, $item_name:expr) => {{
        $next_param
            .and_then(|stringified_param| stringified_param.parse::<$type>().ok())
            .with_context(|| format!("{} must have {}", $component_name, $item_name))?
    }};
}

macro_rules! parse_vte_bytes {
    ($self:expr, $encoded_component:expr) => {{
        let mut vte_parser = vt_parser::Parser::new();
        vte_parser.feed($self.grid, &$encoded_component);
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
            component_coordinates = component_coordinates.map(|c| self.clamp_to_pane(c));
            if component_coordinates.is_some() {
                let _ = params_iter.next(); // we just peeked, let's consume the coords now
            }
        }

        if component_name == &"table" {
            let columns = parse_next_param!(params_iter.next(), usize, "table", "columns");
            let rows = parse_next_param!(params_iter.next(), usize, "table", "rows");
            if columns == 0 {
                return Err(anyhow!("a table must have at least one column"));
            }
            let stringified_params = parse_text_params(params_iter);
            let (max_columns, max_rows) = self.room_for(&component_coordinates);
            let encoded_table = table(
                columns,
                rows,
                stringified_params,
                &self.style,
                component_coordinates,
                max_columns,
                max_rows,
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
            let mut nested_list_items = parse_nested_list_items(params_iter);
            for item in nested_list_items.iter_mut() {
                // indenting by more than the width of the pane shows nothing more
                item.indentation_level = std::cmp::min(item.indentation_level, self.grid.width);
            }
            let (max_columns, max_rows) = self.room_for(&component_coordinates);
            let encoded_nested_list = nested_list(
                nested_list_items,
                &self.style,
                component_coordinates,
                max_columns,
                max_rows,
            );
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
        } else if let Some(render_widget) = widget_renderer(component_name.as_str()) {
            let coordinates = component_coordinates
                .with_context(|| format!("{} must have coordinates", component_name))?;
            let mut state = params_iter
                .next()
                .map(|raw_state| WidgetState::parse(raw_state))
                .unwrap_or_default();
            // sizes beyond the pane cannot be shown (and would be allocated and drawn)
            state.limit("lw", self.grid.width);
            state.limit("bx", self.grid.width);
            state.limit("fr", self.grid.height);
            let fields: Vec<String> = params_iter.map(|field| field.clone()).collect();
            let encoded_widget = render_widget(&state, &fields, &self.style, &coordinates);
            parse_vte_bytes!(self, encoded_widget);
            Ok(())
        } else {
            Err(anyhow!("Unknown component: {}", component_name))
        }
    }
    /// The columns and rows a component may take up: what is left of the pane from its origin, or
    /// the whole pane if it has no coordinates (and is drawn from the cursor)
    fn room_for(&self, coordinates: &Option<Coordinates>) -> (usize, usize) {
        coordinates
            .as_ref()
            .map(|c| (c.max_width, c.max_height))
            .unwrap_or((self.grid.width, self.grid.height))
    }
    /// Coordinates come from the program drawing the component and can be anything: keep the
    /// component inside the pane, so that it cannot ask for more rows or columns than the pane
    /// has (and render, allocate and loop for each of them).
    fn clamp_to_pane(&self, coordinates: Coordinates) -> Coordinates {
        let (pane_columns, pane_rows) = (self.grid.width, self.grid.height);
        let x = std::cmp::min(coordinates.x, pane_columns.saturating_sub(1));
        let y = std::cmp::min(coordinates.y, pane_rows.saturating_sub(1));
        Coordinates {
            x,
            y,
            width: coordinates
                .width
                .map(|width| std::cmp::min(width, pane_columns - x)),
            height: coordinates
                .height
                .map(|height| std::cmp::min(height, pane_rows - y)),
            max_width: pane_columns - x,
            max_height: pane_rows - y,
        }
    }
    fn parse_coordinates(&self, coordinates: &str) -> Result<Option<Coordinates>> {
        lazy_static! {
            // SAFETY: fixed, compile-time-valid regex literal
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
                max_width: usize::MAX,
                max_height: usize::MAX,
            }))
        } else {
            Ok(None)
        }
    }
}

fn parse_flag(stringified: &mut String, flag: char) -> bool {
    const FLAG_CHARS: [char; 3] = ['x', 'z', 'd'];
    let prefix_len = stringified
        .chars()
        .take_while(|c| FLAG_CHARS.contains(c))
        .count();
    if let Some(position) = stringified[..prefix_len].find(flag) {
        stringified.remove(position);
        true
    } else {
        false
    }
}

fn parse_selected(stringified: &mut String) -> bool {
    parse_flag(stringified, 'x')
}

fn parse_opaque(stringified: &mut String) -> bool {
    parse_flag(stringified, 'z')
}

fn parse_disabled(stringified: &mut String) -> bool {
    parse_flag(stringified, 'd')
}

fn parse_indices(stringified: &mut String) -> Vec<Vec<usize>> {
    let Some(last_byte) = stringified.rfind('$') else {
        return Vec::new();
    };
    let indices_string: String = stringified.drain(0..=last_byte).collect();
    let mut all_indices = vec![];
    for index_string in indices_string.split('$') {
        let indices_for_variant = index_string
            .split(',')
            .filter_map(|s| s.parse::<usize>().ok())
            .collect();
        all_indices.push(indices_for_variant);
    }
    all_indices
}

#[cfg(test)]
mod parse_indices_tests {
    use super::parse_indices;

    #[test]
    fn parse_indices_uses_byte_offsets_around_unicode() {
        // Char index of '$' is 2; its byte index is 5. Feeding the char
        // index into String::drain would panic on a mid-character boundary.
        let mut stringified = String::from("éé$rest");
        let indices = parse_indices(&mut stringified);
        // "éé$" splits into a non-numeric group and the empty group after `$`.
        let expected: Vec<Vec<usize>> = vec![vec![], vec![]];
        assert_eq!(indices, expected);
        assert_eq!(stringified, "rest");
    }

    #[test]
    fn parse_indices_keeps_numeric_groups() {
        let mut stringified = String::from("0,2$1$hello");
        let indices = parse_indices(&mut stringified);
        let expected: Vec<Vec<usize>> = vec![vec![0, 2], vec![1], vec![]];
        assert_eq!(indices, expected);
        assert_eq!(stringified, "hello");
    }
}

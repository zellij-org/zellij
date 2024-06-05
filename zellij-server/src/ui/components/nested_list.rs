use super::{
    is_too_high, parse_indices, parse_selected, parse_text, stringify_text, Coordinates, Text,
};
use crate::panes::terminal_character::{AnsiCode, CharacterStyles, RESET_STYLES};
use zellij_utils::data::{PaletteColor, Style};

use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone)]
pub struct NestedListItem {
    pub text: Text,
    pub indentation_level: usize,
}

pub fn nested_list(
    mut contents: Vec<NestedListItem>,
    style: &Style,
    coordinates: Option<Coordinates>,
) -> Vec<u8> {
    let mut stringified = String::new();
    let max_width = coordinates
        .as_ref()
        .and_then(|c| c.width)
        .unwrap_or_else(|| max_nested_item_width(&contents));
    for (line_index, line_item) in contents.drain(..).enumerate() {
        if is_too_high(line_index + 1, &coordinates) {
            break;
        }
        let style_declaration = if line_item.text.selected {
            style.colors.list_selected
        } else {
            style.colors.list_unselected
        };
        let padding = line_item.indentation_level * 2 + 1;
        let bulletin = if line_item.indentation_level % 2 == 0 {
            "> "
        } else {
            "- "
        };
        let text_style = CharacterStyles::from(style_declaration);
        let (mut text, text_width) = stringify_text(
            &line_item.text,
            Some(padding + bulletin.len()),
            &coordinates,
            &style_declaration,
            text_style.bold(Some(AnsiCode::On)),
        );
        text = pad_line(text, max_width, padding, text_width);
        let go_to_row_instruction = coordinates
            .as_ref()
            .map(|c| c.stringify_with_y_offset(line_index))
            .unwrap_or_else(|| {
                if line_index != 0 {
                    format!("\n\r")
                } else {
                    "".to_owned()
                }
            });
        stringified.push_str(&format!(
            "{}{}{:padding$}{bulletin}{}{text}{}",
            go_to_row_instruction,
            text_style,
            " ",
            text_style.bold(Some(AnsiCode::On)),
            RESET_STYLES
        ));
    }
    stringified.as_bytes().to_vec()
}

pub fn parse_nested_list_items<'a>(
    params_iter: impl Iterator<Item = &'a mut String>,
) -> Vec<NestedListItem> {
    params_iter
        .flat_map(|mut stringified| {
            let indentation_level = parse_indentation_level(&mut stringified);
            let selected = parse_selected(&mut stringified);
            let indices = parse_indices(&mut stringified);
            let text = parse_text(&mut stringified).map_err(|e| e.to_string())?;
            let text = Text {
                text,
                selected,
                indices,
            };
            Ok::<NestedListItem, String>(NestedListItem {
                text,
                indentation_level,
            })
        })
        .collect::<Vec<NestedListItem>>()
}

fn parse_indentation_level(stringified: &mut String) -> usize {
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
    indentation_level
}

fn max_nested_item_width(contents: &Vec<NestedListItem>) -> usize {
    let mut width_of_longest_line = 0;
    for line_item in contents.iter() {
        let mut line_item_text_width = 0;
        for character in line_item.text.text.chars() {
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
    width_of_longest_line
}

fn pad_line(text: String, max_width: usize, padding: usize, text_width: usize) -> String {
    if max_width > text_width + padding + 2 {
        // 2 is the bulletin
        let end_padding = max_width.saturating_sub(text_width + padding + 2);
        return format!("{}{:end_padding$}", text, " ");
    }
    text
}

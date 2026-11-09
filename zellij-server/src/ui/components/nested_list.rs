use super::{
    is_too_high, parse_indices, parse_opaque, parse_selected, parse_text, stringify_text,
    Coordinates, Text,
};
use crate::panes::terminal_character::{AnsiCode, CharacterStyles, RESET_STYLES};
use zellij_utils::data::Style;
use zellij_utils::graphemes;

#[derive(Debug, Clone)]
pub struct NestedListItem {
    pub text: Text,
    pub indentation_level: usize,
}

pub fn nested_list(
    mut contents: Vec<NestedListItem>,
    style: &Style,
    coordinates: Option<Coordinates>,
    max_columns: usize,
    max_rows: usize,
) -> Vec<u8> {
    let mut stringified = String::new();
    // the rows that do not fit the pane are not drawn, and the items are not padded to be wider
    // than the pane, whatever the widest of them is (it is the product of the two that grows)
    // (the width still comes from all the items, so that it does not change with the ones shown)
    let max_width = coordinates
        .as_ref()
        .and_then(|c| c.width)
        .unwrap_or_else(|| std::cmp::min(max_nested_item_width(&contents), max_columns));
    contents.truncate(max_rows);
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
        let text_style = if line_item.text.opaque || line_item.text.selected {
            CharacterStyles::from(style_declaration)
                .background(Some(style_declaration.background.into()))
        } else {
            CharacterStyles::from(style_declaration)
        };
        let (mut text, text_width) = stringify_text(
            &line_item.text,
            Some(padding + bulletin.len()),
            &coordinates,
            &style_declaration,
            &style.colors,
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
            "{}{}{}{bulletin}{}{text}{}",
            go_to_row_instruction,
            text_style,
            " ".repeat(padding),
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
            let opaque = parse_opaque(&mut stringified);
            let indices = parse_indices(&mut stringified);
            let text = parse_text(&mut stringified).map_err(|e| e.to_string())?;
            let text = Text {
                text,
                opaque,
                selected,
                disabled: false,
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
    // one pass over the pipes and one drain: removing them one at a time moves the
    // rest of the string for each of them
    let indentation_level = stringified.bytes().take_while(|byte| *byte == b'|').count();
    stringified.drain(..indentation_level);
    indentation_level
}

fn max_nested_item_width(contents: &Vec<NestedListItem>) -> usize {
    let mut width_of_longest_line = 0;
    for line_item in contents.iter() {
        let line_item_text_width = graphemes::display_width(&line_item.text.text);
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
        return format!("{}{}", text, " ".repeat(end_padding));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(text: String) -> NestedListItem {
        NestedListItem {
            text: Text::plain(text),
            indentation_level: 0,
        }
    }

    #[test]
    fn leading_pipes_are_counted_in_one_pass() {
        // removing them one at a time moves the rest of the string for each of them: this many
        // took tens of seconds
        let mut stringified = format!("{}104,105", "|".repeat(1_500_000));
        let started = std::time::Instant::now();
        assert_eq!(parse_indentation_level(&mut stringified), 1_500_000);
        assert_eq!(stringified, "104,105");
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        let mut without_pipes = "104|".to_owned();
        assert_eq!(parse_indentation_level(&mut without_pipes), 0);
        assert_eq!(without_pipes, "104|");
        let mut only_pipes = "|||".to_owned();
        assert_eq!(parse_indentation_level(&mut only_pipes), 3);
        assert_eq!(only_pipes, "");
    }

    #[test]
    fn items_are_not_padded_to_the_longest_one_past_the_pane() {
        // 2000 short items next to one of 5000 characters: padding every item to the width of
        // the longest one writes their product (which grows without bound with the input)
        for max_rows in [10, 50] {
            let mut items = vec![item("b".repeat(5000))];
            items.extend((0..2000).map(|_| item("a".to_owned())));
            let output = nested_list(items, &Style::default(), None, 40, max_rows);
            // (the long item itself is written once, since it is among the rows)
            assert!(output.len() < 20_000, "{} bytes", output.len());
        }
    }

    #[test]
    fn only_the_rows_of_the_pane_are_drawn() {
        let items: Vec<NestedListItem> = (0..2000).map(|_| item("a".to_owned())).collect();
        let output = nested_list(items, &Style::default(), None, 40, 10);
        let text = String::from_utf8(output).unwrap();
        assert_eq!(text.matches('>').count(), 10);
    }
}

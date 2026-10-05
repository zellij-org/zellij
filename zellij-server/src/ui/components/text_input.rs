use super::widget_common::{
    accented_field_bracket_styles, colored, decode_styled, decode_text, dimmed, field_styles, fit,
    label_width, move_to, paint, render_label, reversed, text_width, WidgetState,
};
use super::Coordinates;
use crate::panes::terminal_character::{AnsiCode, CharacterStyles};
use unicode_width::UnicodeWidthChar;
use zellij_utils::data::Style;

const MIN_FIELD_WIDTH: usize = 5;

pub fn text_input(
    state: &WidgetState,
    fields: &[String],
    style: &Style,
    coordinates: &Coordinates,
) -> Vec<u8> {
    let focused = state.flag("f");
    let disabled = state.flag("d");
    let hovered = state.flag("h");
    let placeholder = state.flag("ph");
    let invalid = state.flag("err");
    let cursor = if focused && !disabled {
        state.number("cur")
    } else {
        None
    };
    let label = decode_styled(fields.get(0));
    let text = fields.get(1).map(|f| decode_text(f)).unwrap_or_default();
    let error = fields.get(2).map(|f| decode_text(f)).unwrap_or_default();
    let suffix = fields.get(3).map(|f| decode_text(f)).unwrap_or_default();
    let label_width = label_width(state, &label.text);
    let field_width = coordinates
        .width
        .map(|w| w.saturating_sub(label_width))
        .unwrap_or(text_width(&text) + MIN_FIELD_WIDTH)
        .max(MIN_FIELD_WIDTH);
    let inner_width = field_width - 4;
    let suffix_width = if suffix.is_empty() || text_width(&suffix) + 2 > inner_width {
        0
    } else {
        text_width(&suffix) + 1
    };
    let text_area_width = inner_width - suffix_width;
    let bracket_styles =
        accented_field_bracket_styles(style, focused, hovered, disabled, invalid, state.flag("ab"));
    let value_styles = field_styles(style, focused, hovered, disabled);
    let text_styles = if placeholder {
        value_styles
            .italic(Some(AnsiCode::On))
            .bold(Some(AnsiCode::Reset))
            .dim(Some(AnsiCode::On))
    } else {
        value_styles
    };
    let mut output = move_to(coordinates, 0, 0);
    output.push_str(&render_label(&label, label_width, focused, disabled, style));
    output.push_str(&paint(bracket_styles, "["));
    output.push_str(&paint(value_styles, " "));
    output.push_str(&render_text_area(
        &text,
        text_area_width,
        if placeholder {
            cursor.map(|_| 0)
        } else {
            cursor
        },
        text_styles,
        value_styles,
    ));
    if suffix_width > 0 {
        output.push_str(&paint(
            dimmed(value_styles.bold(Some(AnsiCode::Reset))),
            &format!(" {}", suffix),
        ));
    }
    output.push_str(&paint(value_styles, " "));
    output.push_str(&paint(bracket_styles, "]"));
    let height = coordinates
        .height
        .unwrap_or(if error.is_empty() { 1 } else { 2 });
    if height > 1 {
        output.push_str(&move_to(coordinates, 1, label_width));
        if error.is_empty() {
            output.push_str(&" ".repeat(field_width));
        } else {
            output.push_str(&paint(
                colored(style.colors.exit_code_error.base, None),
                &fit(&format!("✗ {}", error), field_width),
            ));
        }
    }
    output.into_bytes()
}

fn render_text_area(
    text: &str,
    width: usize,
    cursor: Option<usize>,
    text_styles: CharacterStyles,
    value_styles: CharacterStyles,
) -> String {
    let mut output = String::new();
    let mut used_width = 0;
    let mut cursor_drawn = false;
    for (index, character) in text.chars().enumerate() {
        let character_width = character.width().unwrap_or(0);
        if used_width + character_width > width {
            break;
        }
        used_width += character_width;
        if cursor == Some(index) {
            output.push_str(&paint(reversed(value_styles), &character.to_string()));
            cursor_drawn = true;
        } else {
            output.push_str(&paint(text_styles, &character.to_string()));
        }
    }
    if let Some(_) = cursor.filter(|_| !cursor_drawn && used_width < width) {
        output.push_str(&paint(reversed(value_styles), " "));
        used_width += 1;
    }
    output.push_str(&paint(value_styles, &fit("", width - used_width)));
    output
}

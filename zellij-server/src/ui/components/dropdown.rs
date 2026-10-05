use super::widget_common::{
    accented_field_bracket_styles, bold, decode_styled, decode_text, field_styles, fit,
    label_width, move_to, paint, render_label, selected_row_styles, WidgetState,
};
use super::Coordinates;
use zellij_utils::data::Style;

const MIN_FIELD_WIDTH: usize = 6;

pub fn dropdown(
    state: &WidgetState,
    fields: &[String],
    style: &Style,
    coordinates: &Coordinates,
) -> Vec<u8> {
    let focused = state.flag("f");
    let disabled = state.flag("d");
    let hovered = state.flag("h");
    let open = state.flag("o");
    let label = decode_styled(fields.get(0));
    let value = fields.get(1).map(|f| decode_text(f)).unwrap_or_default();
    let label_width = label_width(state, &label.text);
    let field_width = coordinates
        .width
        .map(|w| w.saturating_sub(label_width))
        .unwrap_or(value.chars().count() + MIN_FIELD_WIDTH)
        .max(MIN_FIELD_WIDTH);
    let active = focused || open;
    let bracket_styles =
        accented_field_bracket_styles(style, active, hovered, disabled, false, state.flag("ab"));
    let value_styles = if active && !hovered && !disabled {
        selected_row_styles(style)
    } else {
        field_styles(style, active, hovered, disabled)
    };
    let arrow_styles = if disabled {
        value_styles
    } else if active {
        bold(value_styles.foreground(Some(style.colors.frame_selected.base.into())))
    } else {
        value_styles
    };
    let arrow = if open { "▴" } else { "▾" };
    let mut output = move_to(coordinates, 0, 0);
    output.push_str(&render_label(&label, label_width, active, disabled, style));
    output.push_str(&paint(bracket_styles, "["));
    output.push_str(&paint(
        value_styles,
        &format!(" {} ", fit(&value, field_width - MIN_FIELD_WIDTH)),
    ));
    output.push_str(&paint(arrow_styles, arrow));
    output.push_str(&paint(value_styles, " "));
    output.push_str(&paint(bracket_styles, "]"));
    output.into_bytes()
}

use super::widget_common::{
    bold, colored, decode_text, disabled_look, field_bracket_styles, fit, gray, label_width, move_to,
    paint, plain_styles, render_label, WidgetState,
};
use super::Coordinates;
use zellij_utils::data::Style;

const TOGGLE_WIDTH: usize = 5;

pub fn toggle(
    state: &WidgetState,
    fields: &[String],
    style: &Style,
    coordinates: &Coordinates,
) -> Vec<u8> {
    let focused = state.flag("f");
    let disabled = state.flag("d");
    let hovered = state.flag("h");
    let on = state.flag("on");
    let label = fields.get(0).map(|f| decode_text(f)).unwrap_or_default();
    let label_width = label_width(state, &label);
    let width = coordinates
        .width
        .unwrap_or(label_width + TOGGLE_WIDTH)
        .max(label_width + TOGGLE_WIDTH);
    let bracket_styles = field_bracket_styles(style, focused, hovered, disabled, false);
    let background = if hovered && !disabled {
        Some(gray(style))
    } else {
        None
    };
    let inner_styles = if disabled {
        disabled_look(plain_styles(style))
    } else if on {
        bold(colored(style.colors.text_unselected.emphasis_2, background))
    } else {
        colored(style.colors.text_unselected.base, background)
    };
    let inner_styles = if focused && !disabled {
        bold(inner_styles)
    } else {
        inner_styles
    };
    let inner_width = width - label_width - 2;
    let inner = if inner_width > TOGGLE_WIDTH - 2 {
        format!(" {} ", fit(if on { "on" } else { "off" }, inner_width - 2))
    } else {
        fit(if on { "on" } else { "off" }, inner_width)
    };
    let mut output = move_to(coordinates, 0, 0);
    output.push_str(&render_label(&label, label_width, focused, disabled, style));
    output.push_str(&paint(bracket_styles, "["));
    output.push_str(&paint(inner_styles, &inner));
    output.push_str(&paint(bracket_styles, "]"));
    output.into_bytes()
}

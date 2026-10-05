use super::widget_common::{
    bold, center, colored, decode_styled, decode_text, dimmed, disabled_look, field_styles, fit,
    hovered_row_styles, label_width, move_to, paint, plain_styles, render_label, reversed,
    text_width, WidgetState,
};
use super::Coordinates;
use zellij_utils::data::Style;

const MIN_FIELD_WIDTH: usize = 5;

pub fn stepper(
    state: &WidgetState,
    fields: &[String],
    style: &Style,
    coordinates: &Coordinates,
) -> Vec<u8> {
    let focused = state.flag("f");
    let disabled = state.flag("d");
    let hovered = state.flag("h");
    let editing = state.flag("e") && focused && !disabled;
    let label = decode_styled(fields.get(0));
    let value = fields.get(1).map(|f| decode_text(f)).unwrap_or_default();
    let label_width = label_width(state, &label.text);
    let field_width = coordinates
        .width
        .map(|w| w.saturating_sub(label_width))
        .unwrap_or(text_width(&value) + 4)
        .max(MIN_FIELD_WIDTH);
    let inner_width = field_width - 4;
    let value_styles = field_styles(style, focused, hovered, disabled);
    let arrow_styles = |at_limit: bool, arrow_hovered: bool| {
        if disabled {
            disabled_look(plain_styles(style))
        } else if at_limit {
            dimmed(plain_styles(style))
        } else if arrow_hovered {
            bold(hovered_row_styles(style))
        } else if focused {
            bold(colored(style.colors.frame_selected.base, None))
        } else {
            plain_styles(style)
        }
    };
    let mut output = move_to(coordinates, 0, 0);
    output.push_str(&render_label(&label, label_width, focused, disabled, style));
    output.push_str(&paint(
        arrow_styles(state.flag("lmin"), state.flag("hdec")),
        "‹",
    ));
    output.push_str(&paint(value_styles, " "));
    if editing {
        let cursor = state.number("cur").unwrap_or(value.chars().count());
        let mut used = 0;
        for (index, character) in value.chars().enumerate() {
            if used >= inner_width {
                break;
            }
            let styles = if index == cursor {
                reversed(value_styles)
            } else {
                value_styles
            };
            output.push_str(&paint(styles, &character.to_string()));
            used += 1;
        }
        if cursor >= value.chars().count() && used < inner_width {
            output.push_str(&paint(reversed(value_styles), " "));
            used += 1;
        }
        output.push_str(&paint(value_styles, &fit("", inner_width - used)));
    } else {
        output.push_str(&paint(value_styles, &center(&value, inner_width)));
    }
    output.push_str(&paint(value_styles, " "));
    output.push_str(&paint(
        arrow_styles(state.flag("lmax"), state.flag("hinc")),
        "›",
    ));
    output.into_bytes()
}

use super::widget_common::{
    button_cells_styled, button_width, decode_styled, move_to, ButtonLook, WidgetState,
};
use super::Coordinates;
use zellij_utils::data::Style;

pub fn button(
    state: &WidgetState,
    fields: &[String],
    style: &Style,
    coordinates: &Coordinates,
) -> Vec<u8> {
    let label = decode_styled(fields.get(0));
    let width = coordinates
        .width
        .unwrap_or_else(|| button_width(&label.text));
    let mut output = move_to(coordinates, 0, 0);
    output.push_str(&button_cells_styled(
        &label,
        width,
        ButtonLook::from_state(state),
        state.flag("ab"),
        state.flag("la"),
        style,
    ));
    output.into_bytes()
}

use super::widget_common::{
    button_cells_styled, button_width, decode_text, move_to, ButtonLook, WidgetState,
};
use super::Coordinates;
use zellij_utils::data::Style;

pub fn button(
    state: &WidgetState,
    fields: &[String],
    style: &Style,
    coordinates: &Coordinates,
) -> Vec<u8> {
    let label = fields.get(0).map(|f| decode_text(f)).unwrap_or_default();
    let width = coordinates.width.unwrap_or_else(|| button_width(&label));
    let mut output = move_to(coordinates, 0, 0);
    let label_colors: Vec<Option<usize>> = fields
        .get(1)
        .map(|f| decode_text(f))
        .unwrap_or_default()
        .chars()
        .map(|level| level.to_digit(10).map(|level| level as usize))
        .collect();
    output.push_str(&button_cells_styled(
        &label,
        width,
        ButtonLook::from_state(state),
        state.flag("ab"),
        state.flag("la"),
        &label_colors,
        style,
    ));
    output.into_bytes()
}

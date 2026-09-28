use super::widget_common::{
    button_cells, button_width, decode_text, move_to, ButtonLook, WidgetState,
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
    output.push_str(&button_cells(
        &label,
        width,
        ButtonLook::from_state(state),
        style,
    ));
    output.into_bytes()
}

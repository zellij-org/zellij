use super::widget_common::{
    bold, colored, decode_text, fit, frame_color, gray, hovered_row_styles, indicator_row, move_to,
    paint, plain_styles, selected_row_styles, text_width, WidgetState,
};
use super::Coordinates;
use zellij_utils::data::Style;

pub fn side_menu(
    state: &WidgetState,
    fields: &[String],
    style: &Style,
    coordinates: &Coordinates,
) -> Vec<u8> {
    let focused = state.flag("f");
    let indicators = state.flag("ind");
    let selected = state.number("sel");
    let hovered = state.number("hov");
    let items: Vec<String> = fields.iter().map(|f| decode_text(f)).collect();
    let width = coordinates
        .width
        .unwrap_or_else(|| items.iter().map(|i| text_width(i)).max().unwrap_or(0) + 3)
        .max(2);
    let height = coordinates
        .height
        .unwrap_or(items.len() + if indicators { 2 } else { 0 });
    let row_width = width - 1;
    let selected_styles = if focused {
        bold(colored(
            style.colors.text_unselected.emphasis_0,
            Some(gray(style)),
        ))
    } else {
        selected_row_styles(style)
    };
    let border_styles = colored(frame_color(style, focused), None);
    let first_item_line = if indicators { 1 } else { 0 };
    let mut output = String::new();
    for line in 0..height {
        output.push_str(&move_to(coordinates, line, 0));
        if indicators && line == 0 {
            output.push_str(&indicator_row(
                row_width,
                true,
                state.count("above"),
                state.flag("hu"),
                false,
                style,
            ));
        } else if indicators && line + 1 == height {
            output.push_str(&indicator_row(
                row_width,
                false,
                state.count("below"),
                state.flag("hd"),
                false,
                style,
            ));
        } else {
            let index = line - first_item_line;
            match items.get(index) {
                Some(item) => {
                    let styles = if selected == Some(index) {
                        selected_styles
                    } else if hovered == Some(index) {
                        hovered_row_styles(style)
                    } else {
                        plain_styles(style)
                    };
                    output.push_str(&paint(styles, &fit(&format!(" {}", item), row_width)));
                },
                None => output.push_str(&" ".repeat(row_width)),
            }
        }
        output.push_str(&paint(border_styles, "│"));
    }
    output.into_bytes()
}

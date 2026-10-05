use super::widget_common::{
    bold, colored, dimmed, fit, frame_color, indicator_label, indicator_row, indicator_styles,
    move_to, paint, plain_styles, scrollbar_cells, WidgetState,
};
use super::Coordinates;
use zellij_utils::data::Style;

pub fn scroll_indicator(
    state: &WidgetState,
    _fields: &[String],
    style: &Style,
    coordinates: &Coordinates,
) -> Vec<u8> {
    let width = coordinates.width.unwrap_or(10);
    let height = coordinates.height.unwrap_or(2);
    let right_aligned = state.flag("r");
    let mut output = String::new();
    if height == 0 {
        return output.into_bytes();
    }
    if state.flag("bar") {
        let cells = scrollbar_cells(height, state.count("tot"), state.count("off"));
        let track_styles = dimmed(plain_styles(style));
        let thumb_styles = if state.flag("f") {
            bold(colored(frame_color(style, true), None))
        } else {
            plain_styles(style)
        };
        let label_width = width.saturating_sub(1);
        for (row, is_thumb) in cells.iter().enumerate() {
            output.push_str(&move_to(coordinates, row, 0));
            if *is_thumb {
                output.push_str(&paint(thumb_styles, "┃"));
            } else {
                output.push_str(&paint(track_styles, "│"));
            }
            let indicator = if row == 0 && state.count("above") > 0 {
                Some((true, state.count("above"), state.flag("hu")))
            } else if row + 1 == height && state.count("below") > 0 {
                Some((false, state.count("below"), state.flag("hd")))
            } else {
                None
            };
            match indicator {
                Some((up, count, hovered)) => output.push_str(&paint(
                    indicator_styles(style, hovered),
                    &fit(&format!(" {}", indicator_label(up, count)), label_width),
                )),
                None => output.push_str(&" ".repeat(label_width)),
            }
        }
        return output.into_bytes();
    }
    output.push_str(&move_to(coordinates, 0, 0));
    output.push_str(&indicator_row(
        width,
        true,
        state.count("above"),
        state.flag("hu"),
        right_aligned,
        style,
    ));
    if height > 1 {
        output.push_str(&move_to(coordinates, height - 1, 0));
        output.push_str(&indicator_row(
            width,
            false,
            state.count("below"),
            state.flag("hd"),
            right_aligned,
            style,
        ));
    }
    output.into_bytes()
}

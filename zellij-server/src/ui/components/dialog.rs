use super::widget_common::{
    bold, button_cells, button_width, colored, decode_text, move_to, paint, paint_text,
    text_width, truncate, ButtonLook, WidgetState,
};
use super::text::{decode_text_field, Text};
use super::Coordinates;
use zellij_utils::data::Style;

const MIN_WIDTH: usize = 8;

pub fn dialog(
    state: &WidgetState,
    fields: &[String],
    style: &Style,
    coordinates: &Coordinates,
) -> Vec<u8> {
    let colors = style.colors;
    let button_count = state
        .number("nb")
        .unwrap_or(0)
        .min(fields.len().saturating_sub(1));
    let selected = state.number("sel").unwrap_or(0);
    let title = fields.get(0).map(|f| decode_text(f)).unwrap_or_default();
    let buttons: Vec<String> = fields
        .iter()
        .skip(1)
        .take(button_count)
        .map(|f| decode_text(f))
        .collect();
    let lines: Vec<Text> = fields
        .iter()
        .skip(1 + button_count)
        .map(|f| decode_text_field(f))
        .collect();
    let width = coordinates.width.unwrap_or(40).max(MIN_WIDTH);
    let height = coordinates.height.unwrap_or(lines.len() + 5).max(4);
    let inner_width = width - 2;
    let hovered = state.number("hb");
    let border_styles = colored(colors.frame_selected.base, None);
    let title_styles = bold(colored(colors.table_title.base, None));
    let body_styles = colored(colors.text_unselected.base, None);
    let buttons_width: usize = buttons.iter().map(|b| button_width(b)).sum::<usize>()
        + buttons.len().saturating_sub(1) * 2;
    let buttons_start = state
        .number("bx")
        .unwrap_or_else(|| (width.saturating_sub(buttons_width)) / 2)
        .max(1)
        .saturating_sub(1);
    let mut output = move_to(coordinates, 0, 0);
    let title = truncate(&title, inner_width.saturating_sub(4));
    if title.is_empty() {
        output.push_str(&paint(
            border_styles,
            &format!("╭{}╮", "─".repeat(inner_width)),
        ));
    } else {
        let rest = inner_width - text_width(&title) - 3;
        output.push_str(&paint(border_styles, "╭─ "));
        output.push_str(&paint(title_styles, &title));
        output.push_str(&paint(border_styles, &format!(" {}╮", "─".repeat(rest))));
    }
    let footer_rows = state.count("fr");
    let centered = state.flag("c");
    let buttons_row = height.saturating_sub(2 + footer_rows).max(1);
    for row in 1..height - 1 {
        output.push_str(&move_to(coordinates, row, 0));
        output.push_str(&paint(border_styles, "│"));
        if row == buttons_row {
            output.push_str(&render_buttons(
                &buttons,
                selected,
                hovered,
                buttons_start,
                inner_width,
                body_styles,
                style,
            ));
        } else {
            let empty = Text::plain("");
            let line = row
                .checked_sub(2)
                .filter(|_| row < buttons_row)
                .and_then(|index| lines.get(index))
                .unwrap_or(&empty);
            let text_room = inner_width - 2;
            let lead = if centered {
                text_room.saturating_sub(text_width(&line.text)) / 2
            } else {
                0
            };
            output.push_str(&paint(body_styles, &" ".repeat(1 + lead)));
            output.push_str(&paint_text(
                line,
                body_styles,
                text_room - lead,
                true,
                style,
            ));
            output.push_str(&paint(body_styles, " "));
        }
        output.push_str(&paint(border_styles, "│"));
    }
    output.push_str(&move_to(coordinates, height - 1, 0));
    output.push_str(&paint(
        border_styles,
        &format!("╰{}╯", "─".repeat(inner_width)),
    ));
    output.into_bytes()
}

fn render_buttons(
    buttons: &[String],
    selected: usize,
    hovered: Option<usize>,
    start: usize,
    inner_width: usize,
    body_styles: crate::panes::terminal_character::CharacterStyles,
    style: &Style,
) -> String {
    let mut output = String::new();
    let mut used = 0;
    let lead = start.min(inner_width);
    output.push_str(&paint(body_styles, &" ".repeat(lead)));
    used += lead;
    for (index, label) in buttons.iter().enumerate() {
        if index > 0 {
            let gap = 2.min(inner_width - used);
            output.push_str(&paint(body_styles, &" ".repeat(gap)));
            used += gap;
        }
        let width = button_width(label).min(inner_width - used);
        if width == 0 {
            break;
        }
        let look = if index == selected {
            ButtonLook::Focused
        } else if hovered == Some(index) {
            ButtonLook::Hovered
        } else {
            ButtonLook::Normal
        };
        output.push_str(&button_cells(label, width, look, style));
        used += width;
    }
    output.push_str(&paint(body_styles, &" ".repeat(inner_width - used)));
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message_row(centered: bool) -> String {
        let mut state = vec!["nb=1".to_owned()];
        if centered {
            state.push("c".to_owned());
        }
        let state = WidgetState::parse(&state.join(","));
        let fields = vec![String::new(), "79".to_owned(), "72,105".to_owned()];
        let coordinates = Coordinates {
            x: 0,
            y: 0,
            width: Some(20),
            height: Some(6),
        };
        let output =
            String::from_utf8(dialog(&state, &fields, &Style::default(), &coordinates)).unwrap();
        let mut stripped = String::new();
        let mut characters = output.chars();
        while let Some(character) = characters.next() {
            if character == '\u{1b}' {
                for next in characters.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                stripped.push(character);
            }
        }
        stripped
            .split('│')
            .find(|cell| cell.contains("Hi"))
            .unwrap()
            .to_owned()
    }

    #[test]
    fn a_styled_message_line_is_painted_with_its_colours() {
        let state = WidgetState::parse("nb=0,c");
        let fields = vec![String::new(), "$$$0$72,105".to_owned()];
        let coordinates = Coordinates {
            x: 0,
            y: 0,
            width: Some(20),
            height: Some(5),
        };
        let style = Style::default();
        let output =
            String::from_utf8(dialog(&state, &fields, &style, &coordinates)).unwrap();
        let emphasis = bold(
            colored(style.colors.text_unselected.base, None)
                .foreground(Some(style.colors.text_unselected.emphasis_3.into())),
        );
        assert!(output.contains(&paint(emphasis, "H")));
        assert!(!output.contains(&paint(emphasis, "Hi")));
    }

    #[test]
    fn the_message_is_centered_only_when_asked() {
        assert_eq!(message_row(false), " Hi               ");
        assert_eq!(message_row(true), "        Hi        ");
    }
}

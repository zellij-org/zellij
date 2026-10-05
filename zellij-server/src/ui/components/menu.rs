use super::widget_common::{
    bold, colored, decode_text, dimmed, disabled_look, fit, frame_color, indicator_label,
    indicator_row, indicator_styles, move_to, paint, plain_styles, selected_row_styles, text_width,
    truncate, WidgetState,
};
use super::Coordinates;
use crate::panes::terminal_character::{AnsiCode, CharacterStyles};
use zellij_utils::data::Style;

#[derive(Debug, Clone, PartialEq)]
pub enum MenuRow {
    Separator,
    Item {
        label: String,
        shortcut: Option<String>,
        disabled: bool,
        marked: bool,
        matched: Vec<usize>,
    },
}

pub fn parse_menu_row(raw: &str) -> MenuRow {
    if raw == "-" {
        return MenuRow::Separator;
    }
    let flags: String = raw
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    let rest = &raw[flags.len()..];
    let (rest, matched) = match rest.split_once('/') {
        Some((rest, matched)) => (
            rest,
            matched
                .split(',')
                .filter_map(|index| index.parse::<usize>().ok())
                .collect(),
        ),
        None => (rest, vec![]),
    };
    let (label, shortcut) = match rest.split_once(':') {
        Some((label, shortcut)) => (decode_text(label), Some(decode_text(shortcut))),
        None => (decode_text(rest), None),
    };
    MenuRow::Item {
        label,
        shortcut: shortcut.filter(|s| !s.is_empty()),
        disabled: flags.contains('d'),
        marked: flags.contains('m'),
        matched,
    }
}

pub fn menu(
    state: &WidgetState,
    fields: &[String],
    style: &Style,
    coordinates: &Coordinates,
) -> Vec<u8> {
    let rows: Vec<MenuRow> = fields.iter().map(|f| parse_menu_row(f)).collect();
    let border = state.flag("b");
    let indicators = state.flag("ind") && !border;
    let focused = state.flag("f");
    let highlighted = state.number("hl");
    let above = state.count("above");
    let below = state.count("below");
    let border_width = if border { 1 } else { 0 };
    let width = coordinates
        .width
        .unwrap_or_else(|| natural_width(&rows) + border_width * 2)
        .max(border_width * 2 + 1);
    let inner_width = width - border_width * 2;
    let base_styles = plain_styles(style);
    let highlight_styles = selected_row_styles(style);
    let disabled_styles = disabled_look(plain_styles(style));
    let border_styles = colored(frame_color(style, focused), None);
    let separator_styles = dimmed(plain_styles(style));
    let mut output = String::new();
    let mut line = 0;
    if border {
        output.push_str(&move_to(coordinates, line, 0));
        output.push_str(&border_line(
            "┌",
            "┐",
            inner_width,
            indicator(true, above),
            border_styles,
            indicator_styles(style, state.flag("hu")),
        ));
        line += 1;
    } else if indicators {
        output.push_str(&move_to(coordinates, line, 0));
        output.push_str(&indicator_row(
            width,
            true,
            above,
            state.flag("hu"),
            false,
            style,
        ));
        line += 1;
    }
    for (index, row) in rows.iter().enumerate() {
        output.push_str(&move_to(coordinates, line, 0));
        let is_separator = row == &MenuRow::Separator;
        if border {
            output.push_str(&paint(border_styles, if is_separator { "├" } else { "│" }));
        }
        match row {
            MenuRow::Separator => {
                let styles = if border {
                    border_styles
                } else {
                    separator_styles
                };
                output.push_str(&paint(styles, &"─".repeat(inner_width)));
            },
            MenuRow::Item {
                label,
                shortcut,
                disabled,
                marked,
                matched,
            } => {
                let is_highlighted = highlighted == Some(index) && !disabled;
                let row_styles = if *disabled {
                    disabled_styles
                } else if is_highlighted {
                    highlight_styles
                } else {
                    base_styles
                };
                output.push_str(&render_item(
                    label,
                    shortcut.as_deref(),
                    *marked,
                    matched,
                    inner_width,
                    row_styles,
                    bold(row_styles),
                    dimmed(row_styles.bold(Some(AnsiCode::Reset))),
                    match_styles(style, row_styles),
                ));
            },
        }
        if border {
            output.push_str(&paint(border_styles, if is_separator { "┤" } else { "│" }));
        }
        line += 1;
    }
    if border {
        output.push_str(&move_to(coordinates, line, 0));
        output.push_str(&border_line(
            "└",
            "┘",
            inner_width,
            indicator(false, below),
            border_styles,
            indicator_styles(style, state.flag("hd")),
        ));
    } else if indicators {
        output.push_str(&move_to(coordinates, line, 0));
        output.push_str(&indicator_row(
            width,
            false,
            below,
            state.flag("hd"),
            false,
            style,
        ));
    }
    output.into_bytes()
}

fn indicator(up: bool, count: usize) -> Option<String> {
    if count > 0 {
        Some(indicator_label(up, count))
    } else {
        None
    }
}

fn border_line(
    left: &str,
    right: &str,
    inner_width: usize,
    indicator: Option<String>,
    border_styles: CharacterStyles,
    indicator_styles: CharacterStyles,
) -> String {
    match indicator.filter(|label| text_width(label) + 3 <= inner_width) {
        Some(label) => {
            let rest = inner_width - text_width(&label) - 3;
            format!(
                "{}{}{}",
                paint(border_styles, &format!("{}─", left)),
                paint(indicator_styles, &format!(" {} ", label)),
                paint(border_styles, &format!("{}{}", "─".repeat(rest), right)),
            )
        },
        None => paint(
            border_styles,
            &format!("{}{}{}", left, "─".repeat(inner_width), right),
        ),
    }
}

fn match_styles(style: &Style, row_styles: CharacterStyles) -> CharacterStyles {
    bold(row_styles.foreground(Some(style.colors.text_unselected.emphasis_0.into())))
}

fn paint_with_matches(
    text: &str,
    matched: &[usize],
    row_styles: CharacterStyles,
    match_styles: CharacterStyles,
) -> String {
    if matched.is_empty() {
        return paint(row_styles, text);
    }
    let mut output = String::new();
    let mut run = String::new();
    let mut run_is_match = false;
    for (index, character) in text.chars().enumerate() {
        let is_match = matched.contains(&index);
        if is_match != run_is_match && !run.is_empty() {
            output.push_str(&paint(
                if run_is_match {
                    match_styles
                } else {
                    row_styles
                },
                &run,
            ));
            run.clear();
        }
        run_is_match = is_match;
        run.push(character);
    }
    output.push_str(&paint(
        if run_is_match {
            match_styles
        } else {
            row_styles
        },
        &run,
    ));
    output
}

fn render_item(
    label: &str,
    shortcut: Option<&str>,
    marked: bool,
    matched: &[usize],
    width: usize,
    row_styles: CharacterStyles,
    mark_styles: CharacterStyles,
    shortcut_styles: CharacterStyles,
    match_styles: CharacterStyles,
) -> String {
    if width < 5 {
        return paint(row_styles, &fit(label, width));
    }
    let shortcut = shortcut
        .map(|s| truncate(s, (width - 4) / 2))
        .filter(|s| !s.is_empty());
    let shortcut_width = shortcut.as_ref().map(|s| text_width(s) + 1).unwrap_or(0);
    let label_width = width - 4 - shortcut_width;
    let mut output = String::new();
    output.push_str(&paint(row_styles, " "));
    output.push_str(&paint(mark_styles, if marked { "●" } else { " " }));
    output.push_str(&paint(row_styles, " "));
    output.push_str(&paint_with_matches(
        &fit(label, label_width),
        matched,
        row_styles,
        match_styles,
    ));
    if let Some(shortcut) = shortcut {
        output.push_str(&paint(row_styles, " "));
        output.push_str(&paint(shortcut_styles, &shortcut));
    }
    output.push_str(&paint(row_styles, " "));
    output
}

fn natural_width(rows: &[MenuRow]) -> usize {
    rows.iter()
        .map(|row| match row {
            MenuRow::Separator => 0,
            MenuRow::Item {
                label, shortcut, ..
            } => text_width(label) + 4 + shortcut.as_ref().map(|s| text_width(s) + 1).unwrap_or(0),
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_rows_carry_matched_character_positions() {
        let row = parse_menu_row("m109,97,105,110/0,2");
        assert_eq!(
            row,
            MenuRow::Item {
                label: "main".to_owned(),
                shortcut: None,
                disabled: false,
                marked: true,
                matched: vec![0, 2],
            }
        );
        let row = parse_menu_row("109,97:67");
        assert_eq!(
            row,
            MenuRow::Item {
                label: "ma".to_owned(),
                shortcut: Some("C".to_owned()),
                disabled: false,
                marked: false,
                matched: vec![],
            }
        );
    }

    #[test]
    fn matched_characters_use_emphasis_0() {
        let style = Style::default();
        let row = crate::panes::terminal_character::RESET_STYLES;
        assert_eq!(
            match_styles(&style, row),
            bold(row.foreground(Some(style.colors.text_unselected.emphasis_0.into())))
        );
    }

    #[test]
    fn matched_characters_are_painted_separately() {
        let plain = crate::panes::terminal_character::RESET_STYLES;
        let emphasis = bold(plain);
        let painted = paint_with_matches("abc", &[1], plain, emphasis);
        assert_eq!(
            painted,
            format!(
                "{}{}{}",
                paint(plain, "a"),
                paint(emphasis, "b"),
                paint(plain, "c")
            )
        );
    }
}

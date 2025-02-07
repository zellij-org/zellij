use crate::{Screen, WIDTH_BREAKPOINTS};
use zellij_tile::prelude::*;

pub fn top_tab_menu(cols: usize, current_screen: &Screen, colors: &Styling) {
    let background = colors.text_unselected.background;
    let bg_color = match background {
        PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[48;2;{};{};{}m\u{1b}[0K", r, g, b),
        PaletteColor::EightBit(color) => format!("\u{1b}[48;5;{}m\u{1b}[0K", color),
    };
    let first_ribbon_text_long = "Rebind leader keys";
    let second_ribbon_text_long = "Change mode behavior";
    let first_ribbon_text_short = "Rebind keys";
    let second_ribbon_text_short = "Mode behavior";
    let (first_ribbon_is_selected, second_ribbon_is_selected) = match current_screen {
        Screen::RebindLeaders(_) => (true, false),
        Screen::Presets(_) => (false, true),
    };
    let (first_ribbon_text, second_ribbon_text, starting_positions) = if cols
        >= first_ribbon_text_long.chars().count() + second_ribbon_text_long.chars().count() + 14
    {
        (first_ribbon_text_long, second_ribbon_text_long, (6, 28))
    } else {
        (first_ribbon_text_short, second_ribbon_text_short, (6, 21))
    };
    let mut first_ribbon = Text::new(first_ribbon_text);
    let mut second_ribbon = Text::new(second_ribbon_text);
    if first_ribbon_is_selected {
        first_ribbon = first_ribbon.selected();
    }
    if second_ribbon_is_selected {
        second_ribbon = second_ribbon.selected();
    }
    let switch_key = Text::new("<TAB>").color_range(3, ..).opaque();
    print_text_with_coordinates(switch_key, 0, 0, None, None);
    print!("\u{1b}[{};{}H{}", 0, starting_positions.0, bg_color);
    print_ribbon_with_coordinates(first_ribbon, starting_positions.0, 0, None, None);
    print_ribbon_with_coordinates(second_ribbon, starting_positions.1, 0, None, None);
}

pub fn back_to_presets() {
    let esc = Text::new("<ESC>").color_range(3, ..);
    let first_ribbon = Text::new("Back to Presets");
    print_text_with_coordinates(esc, 0, 0, None, None);
    print_ribbon_with_coordinates(first_ribbon, 6, 0, None, None);
}

pub fn info_line(
    rows: usize,
    cols: usize,
    ui_size: usize,
    notification: &Option<String>,
    warning_text: &Option<String>,
    widths: Option<(usize, usize, usize)>,
) {
    let top_coordinates = if rows > 14 {
        (rows.saturating_sub(ui_size) / 2) + 14
    } else {
        (rows.saturating_sub(ui_size) / 2) + 10
    };
    let left_padding = if let Some(widths) = widths {
        if cols >= widths.0 {
            cols.saturating_sub(widths.0) / 2
        } else if cols >= widths.1 {
            cols.saturating_sub(widths.1) / 2
        } else {
            cols.saturating_sub(widths.2) / 2
        }
    } else {
        if cols >= WIDTH_BREAKPOINTS.0 {
            cols.saturating_sub(WIDTH_BREAKPOINTS.0) / 2
        } else {
            cols.saturating_sub(WIDTH_BREAKPOINTS.1) / 2
        }
    };
    if let Some(notification) = &notification {
        print_text_with_coordinates(
            Text::new(notification).color_range(3, ..),
            left_padding,
            top_coordinates,
            None,
            None,
        );
    } else if let Some(warning_text) = warning_text {
        print_text_with_coordinates(
            Text::new(warning_text).color_range(3, ..),
            left_padding,
            top_coordinates,
            None,
            None,
        );
    }
}

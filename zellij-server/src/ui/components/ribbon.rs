use super::{is_too_wide, Coordinates, Text};
use crate::panes::terminal_character::{AnsiCode, CharacterStyles, RESET_STYLES};
use unicode_width::UnicodeWidthChar;
use zellij_utils::data::{PaletteColor, Style};

static ARROW_SEPARATOR: &str = "";

pub fn ribbon(
    content: Text,
    style: &Style,
    arrow_fonts: bool,
    component_coordinates: Option<Coordinates>,
) -> Vec<u8> {
    let colors = style.colors;
    let background = colors.text_unselected.background;
    let (first_arrow_styles, text_style, last_arrow_styles) = if content.selected {
        (
            character_style(background, colors.ribbon_selected.background),
            character_style(colors.ribbon_selected.base, colors.ribbon_selected.background),
            character_style(colors.ribbon_selected.background, background),
        )
    } else {
        (
            character_style(background, colors.ribbon_unselected.background),
            character_style(colors.ribbon_unselected.base, colors.ribbon_unselected.background),
            character_style(colors.ribbon_unselected.background, background),
        )
    };
    let (text, _text_width) =
        stringify_ribbon_text(&content, &component_coordinates, style, text_style);
    let mut stringified = component_coordinates
        .map(|c| c.to_string())
        .unwrap_or_else(|| String::new());
    let arrow = if arrow_fonts { ARROW_SEPARATOR } else { "" };
    stringified.push_str(&format!(
        "{}{}{}{} {} {}{}{}",
        RESET_STYLES,
        first_arrow_styles,
        arrow,
        text_style,
        text,
        last_arrow_styles,
        arrow,
        RESET_STYLES
    ));
    stringified.as_bytes().to_vec()
}

pub fn emphasis_variants_for_ribbon(style: &Style) -> [PaletteColor; 4] {
    [
        style.colors.ribbon_unselected.emphasis_1,
        style.colors.ribbon_unselected.emphasis_2,
        style.colors.ribbon_unselected.emphasis_3,
        style.colors.ribbon_unselected.emphasis_4,
    ]
}

pub fn emphasis_variants_for_selected_ribbon(style: &Style) -> [PaletteColor; 4] {
    [
        style.colors.ribbon_selected.emphasis_1,
        style.colors.ribbon_selected.emphasis_2,
        style.colors.ribbon_selected.emphasis_3,
        style.colors.ribbon_selected.emphasis_4,
    ]
}

fn stringify_ribbon_text(
    text: &Text,
    coordinates: &Option<Coordinates>,
    style: &Style,
    text_style: CharacterStyles,
) -> (String, usize) {
    let mut stringified = String::new();
    let mut text_width = 0;
    for (i, character) in text.text.chars().enumerate() {
        let character_width = character.width().unwrap_or(0);
        if is_too_wide(character_width, text_width, &coordinates) {
            break;
        }
        if !text.indices.is_empty() {
            let character_with_styling =
                color_ribbon_index_character(character, i, &text, style, text_style);
            stringified.push_str(&character_with_styling);
        } else {
            stringified.push(character);
        }
        text_width += character_width;
    }
    (stringified, text_width)
}

fn color_ribbon_index_character(
    character: char,
    index: usize,
    text: &Text,
    style: &Style,
    base_style: CharacterStyles,
) -> String {
    let character_style = if text.selected {
        text.style_of_index_for_selected_ribbon(index, style)
            .map(|foreground_style| base_style.foreground(Some(foreground_style.into())))
            .unwrap_or(base_style)
    } else {
        text.style_of_index_for_ribbon(index, style)
            .map(|foreground_style| base_style.foreground(Some(foreground_style.into())))
            .unwrap_or(base_style)
    };
    format!("{}{}{}", character_style, character, base_style)
}

fn character_style(foreground: PaletteColor, background: PaletteColor) -> CharacterStyles {
    RESET_STYLES
        .foreground(Some(foreground.into()))
        .background(Some(background.into()))
        .bold(Some(AnsiCode::On))
}

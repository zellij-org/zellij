use super::{text::stringify_text, Coordinates, Text};
use crate::panes::terminal_character::{AnsiCode, CharacterStyles, RESET_STYLES};
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
    let declaration = if content.selected {
        colors.ribbon_selected
    } else {
        colors.ribbon_unselected
    };
    let (first_arrow_styles, text_style, last_arrow_styles) = (
        character_style(background, declaration.background),
        character_style(declaration.base, declaration.background),
        character_style(declaration.background, background),
    );
    let (text, _text_width) = stringify_text(
        &content,
        None,
        &component_coordinates,
        &declaration,
        text_style,
    );
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

fn character_style(foreground: PaletteColor, background: PaletteColor) -> CharacterStyles {
    RESET_STYLES
        .foreground(Some(foreground.into()))
        .background(Some(background.into()))
        .bold(Some(AnsiCode::On))
}

use super::{is_too_wide, parse_indices, parse_opaque, parse_selected, Coordinates};
use crate::panes::{terminal_character::CharacterStyles, AnsiCode};
use zellij_utils::{
    data::{PaletteColor, Style, StyleDeclaration},
    shared::ansi_len,
};

use unicode_width::UnicodeWidthChar;
use zellij_utils::errors::prelude::*;

pub fn text(content: Text, style: &Style, component_coordinates: Option<Coordinates>) -> Vec<u8> {
    let declaration = if content.selected {
        style.colors.text_selected
    } else {
        style.colors.text_unselected
    };

    // Start with base style from declaration
    let base_text_style = CharacterStyles::from(declaration).bold(Some(AnsiCode::On));

    let (text, _text_width) = stringify_text(
        &content,
        None,
        &component_coordinates,
        &declaration,
        &style.colors,
        base_text_style,
    );
    match component_coordinates {
        Some(component_coordinates) => {
            format!("{}{}{}", component_coordinates, base_text_style, text)
                .as_bytes()
                .to_vec()
        },
        None => format!("{}{}", base_text_style, text).as_bytes().to_vec(),
    }
}

pub fn stringify_text(
    text: &Text,
    left_padding: Option<usize>,
    coordinates: &Option<Coordinates>,
    style: &StyleDeclaration,
    styling: &zellij_utils::data::Styling,
    component_text_style: CharacterStyles,
) -> (String, usize) {
    let mut text_width = 0;
    let mut stringified = String::new();
    let base_text_style = if text.opaque || text.selected {
        component_text_style.background(Some(style.background.into()))
    } else {
        component_text_style
    };
    stringified.push_str(&format!("{}", base_text_style));
    for (i, character) in text.text.chars().enumerate() {
        let character_width = character.width().unwrap_or(0);
        if is_too_wide(
            character_width,
            left_padding.unwrap_or(0) + text_width,
            &coordinates,
        ) {
            break;
        }
        text_width += character_width;

        if text.selected || text.opaque {
            // we do this so that selected text will appear selected
            // even if it does not have color indices
            stringified.push_str(&format!("{}", base_text_style));
        }

        if !text.indices.is_empty() || text.selected || text.opaque {
            let character_with_styling =
                color_index_character(character, i, &text, style, styling, base_text_style);
            stringified.push_str(&character_with_styling);
        } else {
            stringified.push(character)
        }
    }
    let coordinates_width = coordinates.as_ref().and_then(|c| c.width);
    match (coordinates_width, base_text_style.background) {
        (Some(coordinates_width), Some(_background_style)) => {
            let text_width_with_left_padding = text_width + left_padding.unwrap_or(0);
            let background_padding_length =
                coordinates_width.saturating_sub(text_width_with_left_padding);
            if text_width_with_left_padding < coordinates_width {
                // here we pad the string with whitespace until the end so that the background
                // style will apply the whole length of the coordinates
                stringified.push_str(&format!(
                    "{:width$}",
                    " ",
                    width = background_padding_length
                ));
            }
            text_width += background_padding_length;
        },
        _ => {},
    }
    (stringified, text_width)
}

pub fn color_index_character(
    character: char,
    index: usize,
    text: &Text,
    declaration: &StyleDeclaration,
    styling: &zellij_utils::data::Styling,
    base_text_style: CharacterStyles,
) -> String {
    let mut character_style = text
        .style_of_index(index, declaration, styling)
        .map(|foreground_style| base_text_style.foreground(Some(foreground_style.into())))
        .unwrap_or(base_text_style);

    // Apply dim and unbold per-character based on index levels 4 and 5
    if text.is_unbold_at(index) {
        // Remove bold for this character
        character_style = character_style.bold(Some(AnsiCode::Reset));
    } else if text.is_dimmed_at(index) {
        // Apply dim for this character
        character_style = character_style
            .foreground(Some(AnsiCode::Reset)) // some terminals (eg. alacritty) do not support dimming non 16
            // colors, so we have to defer to the terminal's default here
            .dim(Some(AnsiCode::On));
    } else {
        character_style = character_style
            .bold(Some(AnsiCode::On))
            .dim(Some(AnsiCode::Reset)); // default, to reset any
                                         // possible dim/bold values
                                         // from previous indices
    }

    format!("{}{}{}", character_style, character, base_text_style)
}

pub fn parse_text_params<'a>(params_iter: impl Iterator<Item = &'a mut String>) -> Vec<Text> {
    params_iter
        .flat_map(|mut stringified| {
            let selected = parse_selected(&mut stringified);
            let opaque = parse_opaque(&mut stringified);
            let indices = parse_indices(&mut stringified);
            let text = parse_text(&mut stringified).map_err(|e| e.to_string())?;
            Ok::<Text, String>(Text {
                text,
                opaque,
                selected,
                indices,
            })
        })
        .collect::<Vec<Text>>()
}

#[derive(Debug, Clone)]
pub struct Text {
    pub text: String,
    pub selected: bool,
    pub opaque: bool,
    pub indices: Vec<Vec<usize>>,
}

impl Text {
    pub fn pad_text(&mut self, max_column_width: usize) {
        for _ in ansi_len(&self.text)..max_column_width {
            self.text.push(' ');
        }
    }

    pub fn is_dimmed_at(&self, index: usize) -> bool {
        const DIM_LEVEL: usize = 4;
        self.indices
            .get(DIM_LEVEL)
            .map(|indices| indices.contains(&index))
            .unwrap_or(false)
    }

    pub fn is_unbold_at(&self, index: usize) -> bool {
        const UNBOLD_LEVEL: usize = 5;
        self.indices
            .get(UNBOLD_LEVEL)
            .map(|indices| indices.contains(&index))
            .unwrap_or(false)
    }

    pub fn style_of_index(
        &self,
        index: usize,
        style: &StyleDeclaration,
        styling: &zellij_utils::data::Styling,
    ) -> Option<PaletteColor> {
        const ERROR_COLOR_LEVEL: usize = 6;
        const SUCCESS_COLOR_LEVEL: usize = 7;

        // Check error color first (highest precedence)
        if let Some(indices) = self.indices.get(ERROR_COLOR_LEVEL) {
            if indices.contains(&index) {
                return Some(styling.exit_code_error.base);
            }
        }

        // Check success color (second highest precedence)
        if let Some(indices) = self.indices.get(SUCCESS_COLOR_LEVEL) {
            if indices.contains(&index) {
                return Some(styling.exit_code_success.base);
            }
        }

        // Check regular emphasis levels (existing code)
        let index_variant_styles = [
            style.emphasis_0,
            style.emphasis_1,
            style.emphasis_2,
            style.emphasis_3,
        ];
        for i in (0..=3).rev() {
            // we do this in reverse to give precedence to the last applied
            // style
            if let Some(indices) = self.indices.get(i) {
                if indices.contains(&index) {
                    return Some(index_variant_styles[i]);
                }
            }
        }
        Some(style.base)
    }
}

pub fn parse_text(stringified: &mut String) -> Result<String> {
    let mut utf8 = vec![];
    for stringified_character in stringified.split(',') {
        utf8.push(
            stringified_character
                .to_string()
                .parse::<u8>()
                .with_context(|| format!("Failed to parse utf8"))?,
        );
    }
    Ok(String::from_utf8_lossy(&utf8).to_string())
}

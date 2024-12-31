use super::{
    emphasis_variants_for_ribbon, emphasis_variants_for_selected_ribbon, is_too_wide,
    parse_indices, parse_opaque, parse_selected, Coordinates,
};
use crate::panes::terminal_character::{AnsiCode, CharacterStyles, RESET_STYLES};
use zellij_utils::{
    data::{PaletteColor, Style},
    shared::ansi_len,
};

use unicode_width::UnicodeWidthChar;
use zellij_utils::errors::prelude::*;

pub fn text(content: Text, style: &Style, component_coordinates: Option<Coordinates>) -> Vec<u8> {
    let mut text_style = RESET_STYLES
        .bold(Some(AnsiCode::On))
        .foreground(Some(style.colors.white.into()));

    if content.selected {
        text_style = text_style.background(Some(style.colors.bg.into()));
    } else if content.opaque {
        text_style = text_style.background(Some(style.colors.black.into()));
    }
    let (text, _text_width) = stringify_text(
        &content,
        None,
        &component_coordinates,
        style,
        text_style,
        content.selected,
    );
    match component_coordinates {
        Some(component_coordinates) => format!("{}{}{}", component_coordinates, text_style, text)
            .as_bytes()
            .to_vec(),
        None => format!("{}{}", text_style, text).as_bytes().to_vec(),
    }
}

pub fn stringify_text(
    text: &Text,
    left_padding: Option<usize>,
    coordinates: &Option<Coordinates>,
    style: &Style,
    text_style: CharacterStyles,
    is_selected: bool,
) -> (String, usize) {
    let mut text_width = 0;
    let mut stringified = String::new();
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
        if !text.indices.is_empty() {
            let character_with_styling =
                color_index_character(character, i, &text, style, text_style, is_selected);
            stringified.push_str(&character_with_styling);
        } else {
            stringified.push(character);
        }
    }
    (stringified, text_width)
}

pub fn color_index_character(
    character: char,
    index: usize,
    text: &Text,
    style: &Style,
    base_text_style: CharacterStyles,
    is_selected: bool,
) -> String {
    let character_style = text
        .style_of_index(index, style)
        .map(|foreground_style| {
            let mut character_style = base_text_style.foreground(Some(foreground_style.into()));
            if is_selected {
                character_style = character_style.background(Some(style.colors.bg.into()));
            };
            character_style
        })
        .unwrap_or(base_text_style);
    format!("{}{}{}", character_style, character, base_text_style)
}

pub fn emphasis_variants(style: &Style) -> [PaletteColor; 4] {
    [
        style.colors.orange,
        style.colors.cyan,
        style.colors.green,
        style.colors.magenta,
    ]
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
    pub fn style_of_index(&self, index: usize, style: &Style) -> Option<PaletteColor> {
        let index_variant_styles = emphasis_variants(style);
        for i in (0..=3).rev() {
            // we do this in reverse to give precedence to the last applied
            // style
            if let Some(indices) = self.indices.get(i) {
                if indices.contains(&index) {
                    return Some(index_variant_styles[i]);
                }
            }
        }
        None
    }
    pub fn style_of_index_for_ribbon(&self, index: usize, style: &Style) -> Option<PaletteColor> {
        let index_variant_styles = emphasis_variants_for_ribbon(style);
        for i in (0..=3).rev() {
            // we do this in reverse to give precedence to the last applied
            // style
            if let Some(indices) = self.indices.get(i) {
                if indices.contains(&index) {
                    return Some(index_variant_styles[i]);
                }
            }
        }
        None
    }
    pub fn style_of_index_for_selected_ribbon(
        &self,
        index: usize,
        style: &Style,
    ) -> Option<PaletteColor> {
        let index_variant_styles = emphasis_variants_for_selected_ribbon(style);
        for i in (0..=3).rev() {
            // we do this in reverse to give precedence to the last applied
            // style
            if let Some(indices) = self.indices.get(i) {
                if indices.contains(&index) {
                    return Some(index_variant_styles[i]);
                }
            }
        }
        None
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

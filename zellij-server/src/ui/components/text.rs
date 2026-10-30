use super::{
    is_too_wide, parse_disabled, parse_indices, parse_opaque, parse_selected, Coordinates,
};
use crate::panes::terminal_character::RESET_STYLES;
use crate::panes::{terminal_character::CharacterStyles, AnsiCode};
use zellij_utils::{
    data::{PaletteColor, Style, StyleDeclaration},
    shared::ansi_len,
};

use super::widget_common::text_clusters;
use zellij_utils::errors::prelude::*;
use zellij_utils::graphemes;

pub fn text(content: Text, style: &Style, component_coordinates: Option<Coordinates>) -> Vec<u8> {
    if content.disabled {
        let declaration = style.colors.text_unselected;
        let mut base_text_style = RESET_STYLES
            .foreground(Some(declaration.base.into()))
            .italic(Some(AnsiCode::On));
        if content.opaque || content.selected {
            base_text_style = base_text_style.background(Some(declaration.background.into()));
        }
        let disabled_content = content.into_disabled();
        let (text, _text_width) = stringify_text(
            &disabled_content,
            None,
            &component_coordinates,
            &declaration,
            &style.colors,
            base_text_style,
        );
        return match component_coordinates {
            Some(component_coordinates) => {
                format!("{}{}{}", component_coordinates, base_text_style, text)
                    .as_bytes()
                    .to_vec()
            },
            None => format!("{}{}", base_text_style, text).as_bytes().to_vec(),
        };
    }
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
    let styled = !text.indices.is_empty() || text.selected || text.opaque;
    for (cluster, cluster_width, index) in text_clusters(&text.text) {
        if is_too_wide(
            cluster_width,
            left_padding.unwrap_or(0) + text_width,
            &coordinates,
        ) {
            break;
        }
        text_width += cluster_width;
        if text.selected || text.opaque {
            stringified.push_str(&format!("{}", base_text_style));
        }
        if styled {
            let cluster_with_styling =
                color_index_cluster(cluster, index, &text, style, styling, base_text_style);
            stringified.push_str(&cluster_with_styling);
        } else {
            stringified.push_str(cluster)
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

pub fn color_index_cluster(
    cluster: &str,
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

    if text.is_italic_at(index) {
        character_style = character_style.italic(Some(AnsiCode::On));
    }

    format!("{}{}{}", character_style, cluster, base_text_style)
}

pub fn parse_text_params<'a>(params_iter: impl Iterator<Item = &'a mut String>) -> Vec<Text> {
    params_iter
        .flat_map(|mut stringified| {
            let selected = parse_selected(&mut stringified);
            let opaque = parse_opaque(&mut stringified);
            let disabled = parse_disabled(&mut stringified);
            let indices = parse_indices(&mut stringified);
            let text = parse_text(&mut stringified).map_err(|e| e.to_string())?;
            Ok::<Text, String>(Text {
                text,
                opaque,
                selected,
                disabled,
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
    pub disabled: bool,
    pub indices: Vec<Vec<usize>>,
}

pub fn decode_text_field(raw: &str) -> Text {
    let mut stringified = raw.to_owned();
    let selected = parse_selected(&mut stringified);
    let opaque = parse_opaque(&mut stringified);
    let disabled = parse_disabled(&mut stringified);
    let indices = parse_indices(&mut stringified);
    let text = if stringified.is_empty() {
        String::new()
    } else {
        parse_text(&mut stringified).unwrap_or_default()
    };
    Text {
        text,
        selected,
        opaque,
        disabled,
        indices,
    }
}

impl Text {
    pub fn plain(text: impl Into<String>) -> Self {
        Text {
            text: text.into(),
            selected: false,
            opaque: false,
            disabled: false,
            indices: vec![],
        }
    }
    pub fn is_styled_at(&self, index: usize) -> bool {
        self.indices.iter().any(|indices| indices.contains(&index))
    }
    pub fn into_disabled(mut self) -> Self {
        self.selected = false;
        self.opaque = false;
        self.disabled = true;
        self.indices = vec![];
        self
    }

    pub fn pad_text(&mut self, max_column_width: usize) {
        for _ in text_cell_width(&self.text)..max_column_width {
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

    pub fn is_italic_at(&self, index: usize) -> bool {
        const ITALIC_LEVEL: usize = 8;
        self.indices
            .get(ITALIC_LEVEL)
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
        const CUSTOM_COLOR_LEVEL: usize = 9;
        const CUSTOM_COLOR_MARK: usize = 1 << 30;

        for level in self.indices.iter().skip(CUSTOM_COLOR_LEVEL) {
            let Some(mark) = level.iter().find(|value| **value >= CUSTOM_COLOR_MARK) else {
                continue;
            };
            if level.contains(&index) {
                let rgb = mark - CUSTOM_COLOR_MARK;
                return Some(PaletteColor::Rgb((
                    ((rgb >> 16) & 0xff) as u8,
                    ((rgb >> 8) & 0xff) as u8,
                    (rgb & 0xff) as u8,
                )));
            }
        }

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

pub fn text_cell_width(text: &str) -> usize {
    if text.contains('\u{1b}') {
        ansi_len(text)
    } else {
        graphemes::display_width(text)
    }
}

pub fn parse_text(stringified: &mut String) -> Result<String> {
    if stringified.is_empty() {
        return Ok(String::new());
    }
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

#[cfg(test)]
mod empty_text_tests {
    use super::*;

    #[test]
    fn an_empty_text_component_is_kept() {
        let mut params = vec!["z".to_owned()];
        let parsed = parse_text_params(params.iter_mut());
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].text.is_empty());
        assert!(parsed[0].opaque);
    }

    #[test]
    fn a_custom_colour_level_paints_its_indices_in_that_colour() {
        let mark = (1usize << 30) + 0x8e4ec6;
        let mut indices = vec![vec![]; 9];
        indices.push(vec![mark, 0]);
        let text = Text {
            text: "ab".to_owned(),
            selected: false,
            opaque: false,
            disabled: false,
            indices,
        };
        let style = StyleDeclaration::default();
        let styling = zellij_utils::data::Styling::default();
        assert_eq!(
            text.style_of_index(0, &style, &styling),
            Some(PaletteColor::Rgb((0x8e, 0x4e, 0xc6)))
        );
        assert_ne!(
            text.style_of_index(1, &style, &styling),
            Some(PaletteColor::Rgb((0x8e, 0x4e, 0xc6)))
        );
    }
}

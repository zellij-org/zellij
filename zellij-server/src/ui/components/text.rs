use super::{
    is_too_wide, parse_disabled, parse_indices, parse_links, parse_opaque, parse_selected,
    Coordinates,
};
use crate::panes::terminal_character::RESET_STYLES;
use crate::panes::{terminal_character::CharacterStyles, AnsiCode};
use std::collections::BTreeMap;
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

// OSC 8 hyperlinks, which the grid keeps on the cells they cover.
const LINK_END: &str = "\u{1b}]8;;\u{1b}\\";

fn link_start(url: &str) -> String {
    format!("\u{1b}]8;;{}\u{1b}\\", url)
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
    let mut open_link: Option<&str> = None;
    for (cluster, cluster_width, index) in text_clusters(&text.text) {
        if is_too_wide(
            cluster_width,
            left_padding.unwrap_or(0) + text_width,
            &coordinates,
        ) {
            break;
        }
        let link = text.link_at(index);
        if link != open_link {
            if open_link.is_some() {
                stringified.push_str(LINK_END);
            }
            if let Some(url) = link {
                stringified.push_str(&link_start(url));
            }
            open_link = link;
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
    if open_link.is_some() {
        stringified.push_str(LINK_END);
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
            let links = parse_links(&stringified);
            let indices = parse_indices(&mut stringified);
            let text = parse_text(&mut stringified).map_err(|e| e.to_string())?;
            Ok::<Text, String>(Text {
                text,
                opaque,
                selected,
                disabled,
                indices,
                links,
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
    pub links: BTreeMap<usize, String>, // index level -> URL
}

pub fn decode_text_field(raw: &str) -> Text {
    let mut stringified = raw.to_owned();
    let selected = parse_selected(&mut stringified);
    let opaque = parse_opaque(&mut stringified);
    let disabled = parse_disabled(&mut stringified);
    let links = parse_links(&stringified);
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
        links,
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
            links: BTreeMap::new(),
        }
    }
    pub fn is_styled_at(&self, index: usize) -> bool {
        self.indices
            .iter()
            .enumerate()
            .any(|(level, indices)| !self.links.contains_key(&level) && indices.contains(&index))
    }
    pub fn link_at(&self, index: usize) -> Option<&str> {
        self.links
            .iter()
            .find(|(level, _)| {
                self.indices
                    .get(**level)
                    .is_some_and(|indices| indices.contains(&index))
            })
            .map(|(_, url)| url.as_str())
    }
    pub fn into_disabled(mut self) -> Self {
        self.selected = false;
        self.opaque = false;
        self.disabled = true;
        self.indices = vec![];
        self.links = BTreeMap::new();
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
            links: BTreeMap::new(),
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

#[cfg(test)]
mod link_tests {
    use super::super::nested_list::parse_nested_list_items;
    use super::super::parse_indices;
    use super::*;

    const URL: &str = "https://zellij.dev/documentation/inbox-integrations.html";
    const OTHER_URL: &str = "https://zellij.dev/";

    fn hex(url: &str) -> String {
        url.bytes().map(|byte| format!("{:02x}", byte)).collect()
    }

    fn body(text: &str) -> String {
        text.bytes()
            .map(|byte| byte.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    // What zellij-tile writes for Text::from(text).link_range(url, range).
    fn linked(text: &str, url: &str, range: std::ops::Range<usize>) -> String {
        let positions: Vec<String> = range.map(|index| index.to_string()).collect();
        format!(
            "{}l{},{}${}",
            "$".repeat(9),
            hex(url),
            positions.join(","),
            body(text)
        )
    }

    fn parsed(serialized: &str) -> Text {
        let mut params = vec![serialized.to_owned()];
        parse_text_params(params.iter_mut()).remove(0)
    }

    // The drawn text with its style sequences removed, keeping the link tags.
    fn rendered(text: &Text, width: Option<usize>) -> String {
        let coordinates = width.map(|width| Coordinates {
            x: 0,
            y: 0,
            width: Some(width),
            height: None,
        });
        let (stringified, _) = stringify_text(
            text,
            None,
            &coordinates,
            &StyleDeclaration::default(),
            &zellij_utils::data::Styling::default(),
            RESET_STYLES,
        );
        let mut kept = String::new();
        let mut characters = stringified.chars().peekable();
        while let Some(character) = characters.next() {
            if character == '\u{1b}' && characters.peek() == Some(&'[') {
                for character in characters.by_ref() {
                    if character.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                kept.push(character);
            }
        }
        kept
    }

    #[test]
    fn a_link_mark_becomes_a_link_on_its_level() {
        let text = parsed(&linked("see docs", URL, 4..8));
        assert_eq!(text.text, "see docs");
        assert_eq!(text.links.get(&9).map(String::as_str), Some(URL));
        assert_eq!(text.link_at(3), None);
        assert_eq!(text.link_at(4), Some(URL));
        assert_eq!(text.link_at(7), Some(URL));
    }

    #[test]
    fn a_bad_link_is_dropped_but_its_text_is_kept() {
        let too_long = format!("https://{}", "a".repeat(2100));
        for url in [
            "no-scheme-here",
            "https://a b",
            "https://a\u{1b}]8;;",
            "1https://a",
            "https:",
            too_long.as_str(),
        ] {
            let text = parsed(&linked("ab", url, 0..2));
            assert!(text.links.is_empty(), "{:?}", url);
            assert_eq!(text.text, "ab");
        }
        for token in ["l123", "lff", "lzz"] {
            let text = parsed(&format!("{}{},0,1${}", "$".repeat(9), token, body("ab")));
            assert!(text.links.is_empty(), "{:?}", token);
            assert_eq!(text.text, "ab");
        }
    }

    #[test]
    fn older_versions_read_a_link_level_as_unstyled_positions() {
        // parse_indices is all that versions without links run on this.
        let mut serialized = linked("ab", URL, 0..2);
        let indices = parse_indices(&mut serialized);
        assert_eq!(indices[9], vec![0, 1]);
        assert_eq!(serialized, body("ab"));
        let as_older_versions_see_it = Text {
            indices,
            ..Text::plain("ab")
        };
        let style = StyleDeclaration::default();
        let styling = zellij_utils::data::Styling::default();
        assert_eq!(
            as_older_versions_see_it.style_of_index(0, &style, &styling),
            Some(style.base)
        );
    }

    #[test]
    fn the_link_tag_wraps_exactly_the_linked_characters() {
        let text = parsed(&linked("see docs.", URL, 4..8));
        assert_eq!(
            rendered(&text, None),
            format!("see {}docs{}.", link_start(URL), LINK_END)
        );
    }

    #[test]
    fn a_cut_off_link_is_still_closed() {
        let text = parsed(&linked("see docs", URL, 4..8));
        assert_eq!(
            rendered(&text, Some(6)),
            format!("see {}do{}", link_start(URL), LINK_END)
        );
    }

    #[test]
    fn background_padding_is_not_linked() {
        let text = parsed(&format!("z{}", linked("ab", URL, 0..2)));
        assert_eq!(
            rendered(&text, Some(5)),
            format!("{}ab{}   ", link_start(URL), LINK_END)
        );
    }

    #[test]
    fn two_links_side_by_side_stay_separate() {
        let text = parsed(&format!(
            "{}l{},0$l{},1${}",
            "$".repeat(9),
            hex(URL),
            hex(OTHER_URL),
            body("ab")
        ));
        assert_eq!(
            rendered(&text, None),
            format!(
                "{}a{}{}b{}",
                link_start(URL),
                LINK_END,
                link_start(OTHER_URL),
                LINK_END
            )
        );
    }

    #[test]
    fn a_link_level_is_not_a_style() {
        let text = parsed(&linked("ab", URL, 0..2));
        assert!(!text.is_styled_at(0));
        assert_eq!(text.into_disabled().link_at(0), None);
    }

    #[test]
    fn a_nested_list_item_keeps_its_link() {
        let mut params = vec![format!("|{}", linked("ab", URL, 0..2))];
        let items = parse_nested_list_items(params.iter_mut());
        assert_eq!(items[0].text.link_at(1), Some(URL));
    }
}

use crate::data::{PaletteColor, Styling};
use crate::input::config_blocks::{slot_colour, MULTIPLAYER_COLOURS, PLAYER_COLOURS};

pub const TEXT_CONTRAST: f64 = 4.5;
pub const LINE_CONTRAST: f64 = 3.0;
pub const PAGE_BACKGROUND: (&str, &str) = ("text_unselected", "background");

const SYSTEM_COLOURS: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (205, 0, 0),
    (0, 205, 0),
    (205, 205, 0),
    (0, 0, 238),
    (205, 0, 205),
    (0, 205, 205),
    (229, 229, 229),
    (127, 127, 127),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (92, 92, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

pub fn rgb(colour: PaletteColor) -> (u8, u8, u8) {
    match colour {
        PaletteColor::Rgb(rgb) => rgb,
        PaletteColor::EightBit(index) if index < 16 => SYSTEM_COLOURS[index as usize],
        PaletteColor::EightBit(index) if index < 232 => {
            let cube = index - 16;
            (
                CUBE_LEVELS[(cube / 36) as usize],
                CUBE_LEVELS[((cube / 6) % 6) as usize],
                CUBE_LEVELS[(cube % 6) as usize],
            )
        },
        PaletteColor::EightBit(index) => {
            let level = 8 + (index - 232) * 10;
            (level, level, level)
        },
    }
}

fn channel(value: u8) -> f64 {
    let value = value as f64 / 255.0;
    if value <= 0.03928 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub fn luminance(colour: PaletteColor) -> f64 {
    let (r, g, b) = rgb(colour);
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

pub fn contrast(first: PaletteColor, second: PaletteColor) -> f64 {
    let (a, b) = (luminance(first), luminance(second));
    let (light, dark) = if a > b { (a, b) } else { (b, a) };
    (light + 0.05) / (dark + 0.05)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrawnOn {
    Slot(&'static str, &'static str),
    CursorText,
}

impl DrawnOn {
    pub fn describe(&self) -> String {
        match self {
            DrawnOn::Slot(style, component) => format!("{}.{}", style, component),
            DrawnOn::CursorText => "the black cursor text".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnPair {
    pub foreground: (&'static str, &'static str),
    pub background: DrawnOn,
    pub target: f64,
}

const TEXT_PARTS: [&str; 5] = ["base", "emphasis_0", "emphasis_1", "emphasis_2", "emphasis_3"];

const STYLES_ON_THEIR_OWN_BACKGROUND: [&str; 7] = [
    "text_unselected",
    "text_selected",
    "table_title",
    "table_cell_unselected",
    "table_cell_selected",
    "list_unselected",
    "list_selected",
];

const STYLES_ALSO_ON_THE_PAGE: [&str; 3] = ["table_title", "table_cell_unselected", "list_unselected"];

const LINES_ON_THE_PAGE: [(&str, &str); 8] = [
    ("frame_unselected", "base"),
    ("frame_selected", "base"),
    ("frame_highlight", "base"),
    ("frame_highlight", "emphasis_0"),
    ("frame_highlight", "emphasis_1"),
    ("exit_code_success", "base"),
    ("exit_code_success", "emphasis_0"),
    ("exit_code_error", "base"),
];

fn pair(
    foreground: (&'static str, &'static str),
    background: (&'static str, &'static str),
    target: f64,
) -> DrawnPair {
    DrawnPair {
        foreground,
        background: DrawnOn::Slot(background.0, background.1),
        target,
    }
}

pub fn drawn_pairs() -> Vec<DrawnPair> {
    let mut pairs = vec![];
    for style in STYLES_ON_THEIR_OWN_BACKGROUND {
        for part in TEXT_PARTS {
            pairs.push(pair((style, part), (style, "background"), TEXT_CONTRAST));
        }
    }
    for style in STYLES_ALSO_ON_THE_PAGE {
        for part in TEXT_PARTS {
            pairs.push(pair((style, part), PAGE_BACKGROUND, TEXT_CONTRAST));
        }
    }
    pairs.push(pair(
        ("text_unselected", "base"),
        ("text_selected", "background"),
        TEXT_CONTRAST,
    ));
    pairs.push(pair(
        ("frame_selected", "base"),
        ("text_selected", "background"),
        LINE_CONTRAST,
    ));
    for part in ["base", "emphasis_0", "emphasis_3"] {
        pairs.push(pair(
            ("ribbon_unselected", part),
            ("ribbon_unselected", "background"),
            TEXT_CONTRAST,
        ));
        pairs.push(pair(
            ("ribbon_selected", part),
            ("ribbon_selected", "background"),
            TEXT_CONTRAST,
        ));
    }
    for part in ["base", "emphasis_0"] {
        pairs.push(pair(
            ("ribbon_unselected", part),
            ("ribbon_unselected", "emphasis_1"),
            TEXT_CONTRAST,
        ));
    }
    for line in LINES_ON_THE_PAGE {
        pairs.push(pair(line, PAGE_BACKGROUND, LINE_CONTRAST));
    }
    for player in PLAYER_COLOURS {
        pairs.push(pair(
            (MULTIPLAYER_COLOURS, player),
            PAGE_BACKGROUND,
            LINE_CONTRAST,
        ));
        pairs.push(DrawnPair {
            foreground: (MULTIPLAYER_COLOURS, player),
            background: DrawnOn::CursorText,
            target: LINE_CONTRAST,
        });
    }
    pairs
}

#[derive(Debug, Clone, PartialEq)]
pub struct PairContrast {
    pub pair: DrawnPair,
    pub foreground: PaletteColor,
    pub background: PaletteColor,
    pub ratio: f64,
}

impl PairContrast {
    pub fn passes(&self) -> bool {
        self.ratio >= self.pair.target
    }
    pub fn margin(&self) -> f64 {
        self.ratio / self.pair.target
    }
}

pub fn background_colour(styling: &Styling, background: DrawnOn) -> Option<PaletteColor> {
    match background {
        DrawnOn::Slot(style, component) => slot_colour(styling, style, component),
        DrawnOn::CursorText => Some(PaletteColor::Rgb((0, 0, 0))),
    }
}

pub fn pair_contrasts(styling: &Styling) -> Vec<PairContrast> {
    drawn_pairs()
        .into_iter()
        .filter_map(|pair| {
            let foreground = slot_colour(styling, pair.foreground.0, pair.foreground.1)?;
            let background = background_colour(styling, pair.background)?;
            Some(PairContrast {
                pair,
                foreground,
                background,
                ratio: contrast(foreground, background),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::DEFAULT_STYLES;

    #[test]
    fn every_pair_names_known_slots() {
        let slots = crate::input::config_blocks::theme_slots();
        for pair in drawn_pairs() {
            assert!(slots.contains(&pair.foreground), "{:?}", pair.foreground);
            if let DrawnOn::Slot(style, component) = pair.background {
                assert!(slots.contains(&(style, component)), "{}.{}", style, component);
            }
        }
    }

    #[test]
    fn the_compiled_in_styles_meet_every_target() {
        for found in pair_contrasts(&DEFAULT_STYLES) {
            assert!(found.passes(), "{:?}", found);
        }
    }
}

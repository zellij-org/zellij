use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::{
    slot_colour, styling_from_colours, theme_slots, MULTIPLAYER_COLOURS,
};

pub const LOW_CONTRAST: f64 = 3.0;
pub const GOOD_CONTRAST: f64 = 4.5;

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

const PAGE_BACKGROUND_STYLES: [&str; 6] = [
    "table_title",
    "frame_unselected",
    "frame_selected",
    "frame_highlight",
    "exit_code_success",
    "exit_code_error",
];

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

fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

pub fn contrast(first: PaletteColor, second: PaletteColor) -> f64 {
    let (a, b) = (luminance(rgb(first)), luminance(rgb(second)));
    let (light, dark) = if a > b { (a, b) } else { (b, a) };
    (light + 0.05) / (dark + 0.05)
}

fn blend(from: (u8, u8, u8), to: (u8, u8, u8), amount: f64) -> (u8, u8, u8) {
    let mix = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * amount).round() as u8;
    (mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2))
}

pub fn suggest(foreground: PaletteColor, background: PaletteColor, target: f64) -> PaletteColor {
    let start = rgb(foreground);
    let mut best: Option<(f64, (u8, u8, u8))> = None;
    for end in [(255, 255, 255), (0, 0, 0)] {
        for step in 0..=50 {
            let candidate = blend(start, end, step as f64 / 50.0);
            let ratio = contrast(PaletteColor::Rgb(candidate), background);
            if ratio >= target {
                let distance = step as f64;
                if best.map(|(previous, _)| distance < previous).unwrap_or(true) {
                    best = Some((distance, candidate));
                }
                break;
            }
        }
    }
    let fallback = || {
        let white = PaletteColor::Rgb((255, 255, 255));
        let black = PaletteColor::Rgb((0, 0, 0));
        if contrast(white, background) >= contrast(black, background) {
            (255, 255, 255)
        } else {
            (0, 0, 0)
        }
    };
    PaletteColor::Rgb(best.map(|(_, colour)| colour).unwrap_or_else(fallback))
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlotContrast {
    pub ratio: f64,
    pub foreground: PaletteColor,
    pub background: PaletteColor,
    pub background_slot: String,
}

pub fn background_slot(style: &str) -> String {
    if PAGE_BACKGROUND_STYLES.contains(&style) {
        "text_unselected.background".to_owned()
    } else {
        format!("{}.background", style)
    }
}

pub fn slot_contrasts(colours: &[String]) -> Vec<Option<SlotContrast>> {
    let styling = styling_from_colours(colours);
    theme_slots()
        .iter()
        .map(|(style, component)| {
            if *style == MULTIPLAYER_COLOURS || *component == "background" {
                return None;
            }
            let foreground = slot_colour(&styling, style, component)?;
            let background_slot = background_slot(style);
            let (background_style, background_component) = background_slot.split_once('.')?;
            let background = slot_colour(&styling, background_style, background_component)?;
            Some(SlotContrast {
                ratio: contrast(foreground, background),
                foreground,
                background,
                background_slot,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: PaletteColor = PaletteColor::Rgb((0, 0, 0));
    const WHITE: PaletteColor = PaletteColor::Rgb((255, 255, 255));

    #[test]
    fn black_on_white_has_the_highest_contrast() {
        assert!((contrast(BLACK, WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(WHITE, BLACK) - 21.0).abs() < 0.01);
        assert!((contrast(WHITE, WHITE) - 1.0).abs() < 0.01);
    }

    #[test]
    fn palette_numbers_are_turned_into_rgb() {
        assert_eq!(rgb(PaletteColor::EightBit(1)), (205, 0, 0));
        assert_eq!(rgb(PaletteColor::EightBit(16)), (0, 0, 0));
        assert_eq!(rgb(PaletteColor::EightBit(196)), (255, 0, 0));
        assert_eq!(rgb(PaletteColor::EightBit(231)), (255, 255, 255));
        assert_eq!(rgb(PaletteColor::EightBit(232)), (8, 8, 8));
        assert_eq!(rgb(PaletteColor::EightBit(255)), (238, 238, 238));
    }

    #[test]
    fn a_suggestion_reaches_the_target_and_keeps_its_direction() {
        let background = PaletteColor::Rgb((30, 30, 30));
        let dim = PaletteColor::Rgb((60, 70, 80));
        let suggested = suggest(dim, background, GOOD_CONTRAST);
        assert!(contrast(suggested, background) >= GOOD_CONTRAST);
        assert!(rgb(suggested).0 > 60);
        let light_background = PaletteColor::Rgb((240, 240, 240));
        let pale = PaletteColor::Rgb((200, 210, 220));
        let suggested = suggest(pale, light_background, GOOD_CONTRAST);
        assert!(contrast(suggested, light_background) >= GOOD_CONTRAST);
        assert!(rgb(suggested).0 < 200);
    }

    #[test]
    fn an_impossible_target_falls_back_to_black_or_white() {
        let grey = PaletteColor::Rgb((128, 128, 128));
        assert_eq!(suggest(grey, grey, 30.0), BLACK);
    }

    #[test]
    fn page_level_styles_are_checked_against_the_text_background() {
        assert_eq!(background_slot("frame_selected"), "text_unselected.background");
        assert_eq!(background_slot("table_title"), "text_unselected.background");
        assert_eq!(background_slot("list_selected"), "list_selected.background");
    }

    #[test]
    fn backgrounds_and_player_colors_are_not_checked() {
        let contrasts = slot_contrasts(&[]);
        for ((style, component), found) in theme_slots().iter().zip(contrasts.iter()) {
            let skipped = *style == MULTIPLAYER_COLOURS
                || *component == "background"
                || *style == "frame_unselected";
            assert_eq!(found.is_none(), skipped, "{}.{}", style, component);
        }
    }
}

use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::{styling_from_colours, theme_slots};
pub use zellij_utils::input::theme_contrast::{contrast, rgb};
use zellij_utils::input::theme_contrast::{pair_contrasts, PairContrast};

pub const LOW_CONTRAST: f64 = 3.0;
#[cfg(test)]
pub const GOOD_CONTRAST: f64 = 4.5;

fn blend(from: (u8, u8, u8), to: (u8, u8, u8), amount: f64) -> (u8, u8, u8) {
    let mix = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * amount).round() as u8;
    (mix(from.0, to.0), mix(from.1, to.1), mix(from.2, to.2))
}

#[cfg(test)]
pub fn suggest(foreground: PaletteColor, background: PaletteColor, target: f64) -> PaletteColor {
    suggest_for_all(foreground, &[(background, target)])
}

fn worst_margin(candidate: (u8, u8, u8), backgrounds: &[(PaletteColor, f64)]) -> f64 {
    backgrounds
        .iter()
        .map(|(background, target)| contrast(PaletteColor::Rgb(candidate), *background) / target)
        .fold(f64::INFINITY, f64::min)
}

pub fn suggest_for_all(
    foreground: PaletteColor,
    backgrounds: &[(PaletteColor, f64)],
) -> PaletteColor {
    let start = rgb(foreground);
    let mut closest_passing: Option<(usize, (u8, u8, u8))> = None;
    let mut best_effort: Option<(f64, usize, (u8, u8, u8))> = None;
    for end in [(255, 255, 255), (0, 0, 0)] {
        for step in 0..=50 {
            let candidate = blend(start, end, step as f64 / 50.0);
            let margin = worst_margin(candidate, backgrounds);
            if margin >= 1.0 {
                if closest_passing
                    .map(|(previous, _)| step < previous)
                    .unwrap_or(true)
                {
                    closest_passing = Some((step, candidate));
                }
                break;
            }
            let better = best_effort
                .map(|(previous, previous_step, _)| {
                    margin > previous + f64::EPSILON
                        || ((margin - previous).abs() <= f64::EPSILON && step < previous_step)
                })
                .unwrap_or(true);
            if better {
                best_effort = Some((margin, step, candidate));
            }
        }
    }
    let chosen = closest_passing
        .map(|(_, colour)| colour)
        .or(best_effort.map(|(_, _, colour)| colour))
        .unwrap_or(start);
    PaletteColor::Rgb(chosen)
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlotContrast {
    pub ratio: f64,
    pub target: f64,
    pub foreground: PaletteColor,
    pub background: PaletteColor,
    pub background_slot: String,
    pub pairs: Vec<PairContrast>,
}

impl SlotContrast {
    pub fn backgrounds(&self) -> Vec<(PaletteColor, f64)> {
        self.pairs
            .iter()
            .map(|found| (found.background, found.pair.target))
            .collect()
    }
    pub fn fails_on(&self, slot: &str, below: f64) -> bool {
        self.pairs
            .iter()
            .any(|found| found.pair.background.describe() == slot && found.ratio < below)
    }
}

pub fn slot_contrasts(colours: &[String]) -> Vec<Option<SlotContrast>> {
    let styling = styling_from_colours(colours);
    let all_pairs = pair_contrasts(&styling);
    theme_slots()
        .iter()
        .map(|slot| {
            let pairs: Vec<PairContrast> = all_pairs
                .iter()
                .filter(|found| found.pair.foreground == *slot)
                .cloned()
                .collect();
            let worst = pairs
                .iter()
                .min_by(|a, b| a.margin().total_cmp(&b.margin()))?
                .clone();
            Some(SlotContrast {
                ratio: worst.ratio,
                target: worst.pair.target,
                foreground: worst.foreground,
                background: worst.background,
                background_slot: worst.pair.background.describe(),
                pairs,
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
    fn an_impossible_target_gets_as_close_as_it_can() {
        let grey = PaletteColor::Rgb((128, 128, 128));
        assert_eq!(suggest(grey, grey, 30.0), BLACK);
    }

    fn slot(
        colours: &[Option<SlotContrast>],
        style: &str,
        component: &str,
    ) -> Option<SlotContrast> {
        let index = theme_slots()
            .iter()
            .position(|found| *found == (style, component))
            .unwrap();
        colours[index].clone()
    }

    #[test]
    fn the_hover_fill_is_checked_as_a_background_and_not_as_text() {
        let contrasts = slot_contrasts(&[]);
        assert!(slot(&contrasts, "ribbon_unselected", "emphasis_1").is_none());
        let shortcut = slot(&contrasts, "ribbon_unselected", "emphasis_0").unwrap();
        assert!(shortcut
            .pairs
            .iter()
            .any(|found| found.pair.background.describe() == "ribbon_unselected.emphasis_1"));
    }

    #[test]
    fn slots_that_are_never_drawn_get_no_check() {
        let contrasts = slot_contrasts(&[]);
        assert!(slot(&contrasts, "ribbon_selected", "emphasis_1").is_none());
        assert!(slot(&contrasts, "ribbon_selected", "emphasis_2").is_none());
        assert!(slot(&contrasts, "exit_code_success", "emphasis_1").is_none());
        assert!(slot(&contrasts, "text_unselected", "background").is_none());
    }

    #[test]
    fn text_with_two_backgrounds_reports_the_worse_one() {
        let mut colours = zellij_utils::input::config_blocks::styling_colours(
            &zellij_utils::data::DEFAULT_STYLES,
        );
        let index = |style: &str, component: &str| {
            theme_slots()
                .iter()
                .position(|found| *found == (style, component))
                .unwrap()
        };
        colours[index("text_unselected", "background")] = "#000000".to_owned();
        colours[index("text_selected", "background")] = "#ffffff".to_owned();
        colours[index("text_unselected", "base")] = "#f0f0f0".to_owned();
        let found = slot(&slot_contrasts(&colours), "text_unselected", "base").unwrap();
        assert_eq!(found.background_slot, "text_selected.background");
        assert!(found.ratio < LOW_CONTRAST);
    }

    #[test]
    fn a_suggestion_for_two_backgrounds_passes_on_both() {
        let backgrounds = [
            (PaletteColor::Rgb((0, 0, 0)), LOW_CONTRAST),
            (PaletteColor::Rgb((255, 255, 255)), LOW_CONTRAST),
        ];
        let suggested = suggest_for_all(PaletteColor::Rgb((250, 250, 250)), &backgrounds);
        for (background, target) in backgrounds {
            assert!(contrast(suggested, background) >= target);
        }
    }

    #[test]
    fn player_colors_are_checked_on_the_page_and_under_the_cursor() {
        let found = slot(&slot_contrasts(&[]), "multiplayer_user_colors", "player_1").unwrap();
        let backgrounds: Vec<String> = found
            .pairs
            .iter()
            .map(|found| found.pair.background.describe())
            .collect();
        assert!(backgrounds.contains(&"text_unselected.background".to_owned()));
        assert!(backgrounds.contains(&"the black cursor text".to_owned()));
    }
}

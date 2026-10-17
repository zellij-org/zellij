use zellij_utils::structured_render::WireColor;

pub type Srgb = [u8; 3];

pub const DEFAULT_FOREGROUND: Srgb = [229, 229, 229];
pub const DEFAULT_BACKGROUND: Srgb = [0, 0, 0];
#[cfg(test)]
pub const DEFAULT_CURSOR: Srgb = DEFAULT_FOREGROUND;

pub const ANSI_16: [Srgb; 16] = [
    [0, 0, 0],
    [205, 0, 0],
    [0, 205, 0],
    [205, 205, 0],
    [0, 0, 238],
    [205, 0, 205],
    [0, 205, 205],
    [229, 229, 229],
    [127, 127, 127],
    [255, 0, 0],
    [0, 255, 0],
    [255, 255, 0],
    [92, 92, 255],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
];

const DIM_NUMERATOR: u32 = 2;
const DIM_DENOMINATOR: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paints {
    pub foreground: Srgb,
    pub background: Srgb,
    pub cursor: Option<Srgb>,
    pub ansi: [Srgb; 16],
}

impl Default for Paints {
    fn default() -> Self {
        Self {
            foreground: DEFAULT_FOREGROUND,
            background: DEFAULT_BACKGROUND,
            cursor: None,
            ansi: ANSI_16,
        }
    }
}

impl Paints {
    pub fn cursor_over(&self, cell_foreground: Srgb) -> Srgb {
        self.cursor.unwrap_or(cell_foreground)
    }

    pub fn beam_cursor(&self) -> Srgb {
        self.cursor.unwrap_or(self.foreground)
    }

    pub fn indexed(&self, index: u8) -> Srgb {
        match index {
            0..=15 => self.ansi[index as usize],
            _ => extended(index),
        }
    }

    pub fn foreground(&self, packed: u32) -> Srgb {
        self.wire(packed, self.foreground)
    }

    pub fn background(&self, packed: u32) -> Srgb {
        self.wire(packed, self.background)
    }

    fn wire(&self, packed: u32, default: Srgb) -> Srgb {
        match WireColor::unpack(packed) {
            WireColor::Default => default,
            WireColor::Named(index) => self.ansi[(index as usize).min(15)],
            WireColor::Indexed(index) => self.indexed(index),
            WireColor::Rgb(r, g, b) => [r, g, b],
        }
    }
}

#[cfg(test)]
pub fn indexed(index: u8) -> Srgb {
    Paints::default().indexed(index)
}

fn extended(index: u8) -> Srgb {
    match index {
        0..=15 => ANSI_16[index as usize],
        16..=231 => {
            let offset = (index - 16) as u32;
            let level = |step: u32| {
                if step == 0 {
                    0u8
                } else {
                    (55 + step * 40) as u8
                }
            };
            [
                level(offset / 36),
                level((offset / 6) % 6),
                level(offset % 6),
            ]
        },
        232..=255 => {
            let level = 8u8 + 10 * (index - 232);
            [level, level, level]
        },
    }
}

pub fn dim(color: Srgb) -> Srgb {
    color.map(|channel| ((channel as u32 * DIM_NUMERATOR) / DIM_DENOMINATOR) as u8)
}

const WHITE: Srgb = [255, 255, 255];
const BLACK: Srgb = [0, 0, 0];
const BLEND_STEPS: u32 = 255;
const MAX_CACHED_PAIRS: usize = 4096;

fn linear(channel: u8) -> f64 {
    let value = channel as f64 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub fn relative_luminance(color: Srgb) -> f64 {
    0.2126 * linear(color[0]) + 0.7152 * linear(color[1]) + 0.0722 * linear(color[2])
}

pub fn contrast_ratio(first: Srgb, second: Srgb) -> f64 {
    let (a, b) = (relative_luminance(first), relative_luminance(second));
    let (light, dark) = if a >= b { (a, b) } else { (b, a) };
    (light + 0.05) / (dark + 0.05)
}

fn blend(from: Srgb, to: Srgb, step: u32) -> Srgb {
    let mut out = from;
    for channel in 0..3 {
        let (start, end) = (from[channel] as i32, to[channel] as i32);
        out[channel] = (start
            + ((end - start) * step as i32 + BLEND_STEPS as i32 / 2) / BLEND_STEPS as i32)
            as u8;
    }
    out
}

fn steps_to_reach(foreground: Srgb, background: Srgb, target: Srgb, minimum: f64) -> Option<u32> {
    if contrast_ratio(target, background) < minimum {
        return None;
    }
    (0..=BLEND_STEPS)
        .find(|step| contrast_ratio(blend(foreground, target, *step), background) >= minimum)
}

pub fn with_minimum_contrast(foreground: Srgb, background: Srgb, minimum: f64) -> Srgb {
    if minimum <= 1.0 || contrast_ratio(foreground, background) >= minimum {
        return foreground;
    }
    let lighter = steps_to_reach(foreground, background, WHITE, minimum);
    let darker = steps_to_reach(foreground, background, BLACK, minimum);
    match (lighter, darker) {
        (Some(up), Some(down)) if down < up => blend(foreground, BLACK, down),
        (Some(up), _) => blend(foreground, WHITE, up),
        (None, Some(down)) => blend(foreground, BLACK, down),
        (None, None) => {
            if contrast_ratio(WHITE, background) >= contrast_ratio(BLACK, background) {
                WHITE
            } else {
                BLACK
            }
        },
    }
}

#[derive(Debug, Default)]
pub struct Contrast {
    minimum: f32,
    cache: std::cell::RefCell<std::collections::HashMap<(Srgb, Srgb), Srgb>>,
}

impl Contrast {
    pub fn new(minimum: f32) -> Self {
        Self {
            minimum,
            cache: Default::default(),
        }
    }

    pub fn minimum(&self) -> f32 {
        self.minimum
    }

    pub fn is_active(&self) -> bool {
        self.minimum > 1.0
    }

    pub fn apply(&self, foreground: Srgb, background: Srgb) -> Srgb {
        if !self.is_active() {
            return foreground;
        }
        let mut cache = self.cache.borrow_mut();
        if let Some(adjusted) = cache.get(&(foreground, background)) {
            return *adjusted;
        }
        if cache.len() >= MAX_CACHED_PAIRS {
            cache.clear();
        }
        let adjusted = with_minimum_contrast(foreground, background, self.minimum as f64);
        cache.insert((foreground, background), adjusted);
        adjusted
    }

    #[cfg(test)]
    pub fn cached_pairs(&self) -> usize {
        self.cache.borrow().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paints() -> Paints {
        Paints::default()
    }

    #[test]
    fn the_extended_palette_matches_the_xterm_table() {
        assert_eq!(indexed(16), [0, 0, 0]);
        assert_eq!(indexed(196), [255, 0, 0]);
        assert_eq!(indexed(231), [255, 255, 255]);
        assert_eq!(indexed(232), [8, 8, 8]);
        assert_eq!(indexed(255), [238, 238, 238]);
    }

    #[test]
    fn the_low_palette_is_the_same_table_the_wire_seed_uses() {
        for index in 0u8..16 {
            assert_eq!(indexed(index), ANSI_16[index as usize]);
        }
    }

    #[test]
    fn the_wire_default_is_the_foreground_or_the_background_by_position() {
        let default = WireColor::Default.pack();
        assert_eq!(paints().foreground(default), DEFAULT_FOREGROUND);
        assert_eq!(paints().background(default), DEFAULT_BACKGROUND);
    }

    #[test]
    fn every_wire_color_kind_resolves_to_its_table_entry() {
        let paints = paints();
        assert_eq!(paints.foreground(WireColor::Named(1).pack()), ANSI_16[1]);
        assert_eq!(paints.foreground(WireColor::Named(14).pack()), ANSI_16[14]);
        assert_eq!(
            paints.foreground(WireColor::Indexed(208).pack()),
            indexed(208)
        );
        assert_eq!(paints.foreground(WireColor::Rgb(9, 8, 7).pack()), [9, 8, 7]);
        assert_eq!(paints.background(WireColor::Rgb(9, 8, 7).pack()), [9, 8, 7]);
    }

    #[test]
    fn a_configured_table_answers_named_and_indexed_colors_alike() {
        let mut paints = paints();
        paints.ansi[1] = [1, 2, 3];
        assert_eq!(paints.foreground(WireColor::Named(1).pack()), [1, 2, 3]);
        assert_eq!(paints.foreground(WireColor::Indexed(1).pack()), [1, 2, 3]);
        assert_eq!(
            paints.foreground(WireColor::Indexed(208).pack()),
            indexed(208),
            "the extended palette is not affected by the low table"
        );
    }

    #[test]
    fn configured_defaults_answer_the_wire_default() {
        let paints = Paints {
            foreground: [1, 1, 1],
            background: [2, 2, 2],
            ..Paints::default()
        };
        let default = WireColor::Default.pack();
        assert_eq!(paints.foreground(default), [1, 1, 1]);
        assert_eq!(paints.background(default), [2, 2, 2]);
    }

    #[test]
    fn an_unconfigured_cursor_takes_the_color_it_is_drawn_over() {
        let paints = paints();
        assert_eq!(paints.cursor_over([1, 2, 3]), [1, 2, 3]);
        assert_eq!(paints.beam_cursor(), DEFAULT_CURSOR);
    }

    #[test]
    fn a_configured_cursor_is_used_whatever_it_is_drawn_over() {
        let paints = Paints {
            cursor: Some([9, 9, 9]),
            ..Paints::default()
        };
        assert_eq!(paints.cursor_over([1, 2, 3]), [9, 9, 9]);
        assert_eq!(paints.beam_cursor(), [9, 9, 9]);
    }

    #[test]
    fn contrast_ratios_match_the_wcag_reference_values() {
        assert!((contrast_ratio(WHITE, BLACK) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(BLACK, WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio([120, 30, 200], [120, 30, 200]) - 1.0).abs() < 1e-9);
        assert!((contrast_ratio([0x77, 0x77, 0x77], WHITE) - 4.48).abs() < 0.01);
        assert!((contrast_ratio([0xff, 0x00, 0x00], WHITE) - 4.0).abs() < 0.01);
        assert!((relative_luminance([0x80, 0x80, 0x80]) - 0.2159).abs() < 0.001);
    }

    #[test]
    fn low_contrast_text_is_moved_just_far_enough_to_reach_the_target() {
        for (foreground, background, minimum) in [
            ([60, 60, 60], [40, 40, 40], 4.5),
            ([200, 200, 210], [230, 230, 230], 3.0),
            ([0, 0, 238], [0, 0, 0], 7.0),
            ([128, 128, 128], [128, 128, 128], 4.5),
            ([10, 10, 10], [118, 118, 118], 7.0),
        ] {
            let adjusted = with_minimum_contrast(foreground, background, minimum);
            let ratio = contrast_ratio(adjusted, background);
            let best = contrast_ratio(WHITE, background).max(contrast_ratio(BLACK, background));
            assert!(
                ratio >= minimum || adjusted == WHITE || adjusted == BLACK,
                "{:?} on {:?} became {:?} at {}",
                foreground,
                background,
                adjusted,
                ratio
            );
            if best >= minimum {
                assert!(ratio >= minimum, "{:?} on {:?}", foreground, background);
            }
        }
    }

    #[test]
    fn the_smaller_move_wins_and_unreachable_targets_take_the_better_extreme() {
        let lightened = with_minimum_contrast([90, 90, 90], [60, 60, 60], 3.0);
        assert!(lightened[0] > 90, "{:?}", lightened);
        let darkened = with_minimum_contrast([170, 170, 170], [200, 200, 200], 3.0);
        assert!(darkened[0] < 170, "{:?}", darkened);
        let unreachable = with_minimum_contrast([118, 118, 118], [118, 118, 118], 21.0);
        let mid = [118, 118, 118];
        let expected = if contrast_ratio(WHITE, mid) >= contrast_ratio(BLACK, mid) {
            WHITE
        } else {
            BLACK
        };
        assert_eq!(unreachable, expected);
    }

    #[test]
    fn high_contrast_text_and_an_inactive_minimum_change_nothing() {
        assert_eq!(
            with_minimum_contrast([229, 229, 229], BLACK, 4.5),
            [229, 229, 229]
        );
        assert_eq!(
            with_minimum_contrast([40, 40, 40], BLACK, 1.0),
            [40, 40, 40]
        );
        let off = Contrast::new(1.0);
        assert_eq!(off.apply([40, 40, 40], BLACK), [40, 40, 40]);
        assert_eq!(off.cached_pairs(), 0);
    }

    #[test]
    fn adjusted_pairs_are_cached() {
        let contrast = Contrast::new(4.5);
        let first = contrast.apply([40, 40, 40], BLACK);
        assert_eq!(contrast.cached_pairs(), 1);
        assert_eq!(contrast.apply([40, 40, 40], BLACK), first);
        assert_eq!(contrast.cached_pairs(), 1);
    }

    #[test]
    fn dimming_darkens_by_a_third() {
        assert_eq!(dim([255, 255, 255]), [170, 170, 170]);
        assert_eq!(dim([0, 0, 0]), [0, 0, 0]);
    }
}

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
    fn dimming_darkens_by_a_third() {
        assert_eq!(dim([255, 255, 255]), [170, 170, 170]);
        assert_eq!(dim([0, 0, 0]), [0, 0, 0]);
    }
}

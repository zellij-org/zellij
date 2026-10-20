use crate::ui::boundaries::boundary_type;
use zellij_utils::data::LineStyle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

pub fn horizontal(line_style: LineStyle) -> &'static str {
    match line_style {
        LineStyle::Single => "─",
        LineStyle::Double => "═",
        LineStyle::Heavy => "━",
        LineStyle::Dashed => "┄",
        LineStyle::HeavyDashed => "┅",
    }
}

pub fn vertical(line_style: LineStyle) -> &'static str {
    match line_style {
        LineStyle::Single => "│",
        LineStyle::Double => "║",
        LineStyle::Heavy => "┃",
        LineStyle::Dashed => "┆",
        LineStyle::HeavyDashed => "┇",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Weight {
    Light,
    Double,
    Heavy,
}

fn weight(line_style: LineStyle) -> Weight {
    match line_style {
        LineStyle::Single | LineStyle::Dashed => Weight::Light,
        LineStyle::Double => Weight::Double,
        LineStyle::Heavy | LineStyle::HeavyDashed => Weight::Heavy,
    }
}

pub fn corner(
    corner: Corner,
    horizontal_style: LineStyle,
    vertical_style: LineStyle,
    rounded: bool,
) -> &'static str {
    let horizontal_weight = weight(horizontal_style);
    let mut vertical_weight = weight(vertical_style);
    // there are no glyphs mixing a double arm with a heavy arm, in this case we render both arms
    // in the weight of the horizontal arm
    if (horizontal_weight == Weight::Double && vertical_weight == Weight::Heavy)
        || (horizontal_weight == Weight::Heavy && vertical_weight == Weight::Double)
    {
        vertical_weight = horizontal_weight;
    }
    if rounded && horizontal_weight == Weight::Light && vertical_weight == Weight::Light {
        return match corner {
            Corner::TopLeft => boundary_type::TOP_LEFT_ROUND,
            Corner::TopRight => boundary_type::TOP_RIGHT_ROUND,
            Corner::BottomLeft => boundary_type::BOTTOM_LEFT_ROUND,
            Corner::BottomRight => boundary_type::BOTTOM_RIGHT_ROUND,
        };
    }
    match (corner, horizontal_weight, vertical_weight) {
        (Corner::TopLeft, Weight::Light, Weight::Light) => "┌",
        (Corner::TopLeft, Weight::Double, Weight::Light) => "╒",
        (Corner::TopLeft, Weight::Light, Weight::Double) => "╓",
        (Corner::TopLeft, Weight::Double, Weight::Double) => "╔",
        (Corner::TopLeft, Weight::Heavy, Weight::Light) => "┍",
        (Corner::TopLeft, Weight::Light, Weight::Heavy) => "┎",
        (Corner::TopLeft, Weight::Heavy, Weight::Heavy) => "┏",

        (Corner::TopRight, Weight::Light, Weight::Light) => "┐",
        (Corner::TopRight, Weight::Double, Weight::Light) => "╕",
        (Corner::TopRight, Weight::Light, Weight::Double) => "╖",
        (Corner::TopRight, Weight::Double, Weight::Double) => "╗",
        (Corner::TopRight, Weight::Heavy, Weight::Light) => "┑",
        (Corner::TopRight, Weight::Light, Weight::Heavy) => "┒",
        (Corner::TopRight, Weight::Heavy, Weight::Heavy) => "┓",

        (Corner::BottomLeft, Weight::Light, Weight::Light) => "└",
        (Corner::BottomLeft, Weight::Double, Weight::Light) => "╘",
        (Corner::BottomLeft, Weight::Light, Weight::Double) => "╙",
        (Corner::BottomLeft, Weight::Double, Weight::Double) => "╚",
        (Corner::BottomLeft, Weight::Heavy, Weight::Light) => "┕",
        (Corner::BottomLeft, Weight::Light, Weight::Heavy) => "┖",
        (Corner::BottomLeft, Weight::Heavy, Weight::Heavy) => "┗",

        (Corner::BottomRight, Weight::Light, Weight::Light) => "┘",
        (Corner::BottomRight, Weight::Double, Weight::Light) => "╛",
        (Corner::BottomRight, Weight::Light, Weight::Double) => "╜",
        (Corner::BottomRight, Weight::Double, Weight::Double) => "╝",
        (Corner::BottomRight, Weight::Heavy, Weight::Light) => "┙",
        (Corner::BottomRight, Weight::Light, Weight::Heavy) => "┚",
        (Corner::BottomRight, Weight::Heavy, Weight::Heavy) => "┛",

        (_, Weight::Double, Weight::Heavy) | (_, Weight::Heavy, Weight::Double) => {
            unreachable!("double/heavy mixes are normalized above")
        },
    }
}

pub fn title_separator_left(horizontal_style: LineStyle) -> &'static str {
    match weight(horizontal_style) {
        Weight::Light => boundary_type::VERTICAL_LEFT,
        Weight::Double => "╡",
        Weight::Heavy => "┥",
    }
}

pub fn title_separator_right(horizontal_style: LineStyle) -> &'static str {
    match weight(horizontal_style) {
        Weight::Light => boundary_type::VERTICAL_RIGHT,
        Weight::Double => "╞",
        Weight::Heavy => "┝",
    }
}

pub fn remap_light_glyph(glyph: &'static str, line_style: LineStyle) -> &'static str {
    match line_style {
        LineStyle::Single => glyph,
        LineStyle::Dashed => match glyph {
            boundary_type::HORIZONTAL => "┄",
            boundary_type::VERTICAL => "┆",
            _ => glyph,
        },
        LineStyle::Double => match glyph {
            boundary_type::HORIZONTAL => "═",
            boundary_type::VERTICAL => "║",
            boundary_type::TOP_LEFT => "╔",
            boundary_type::TOP_LEFT_ROUND => "╔",
            boundary_type::TOP_RIGHT => "╗",
            boundary_type::TOP_RIGHT_ROUND => "╗",
            boundary_type::BOTTOM_LEFT => "╚",
            boundary_type::BOTTOM_LEFT_ROUND => "╚",
            boundary_type::BOTTOM_RIGHT => "╝",
            boundary_type::BOTTOM_RIGHT_ROUND => "╝",
            boundary_type::VERTICAL_LEFT => "╣",
            boundary_type::VERTICAL_RIGHT => "╠",
            boundary_type::HORIZONTAL_DOWN => "╦",
            boundary_type::HORIZONTAL_UP => "╩",
            boundary_type::CROSS => "╬",
            _ => glyph,
        },
        LineStyle::Heavy | LineStyle::HeavyDashed => {
            let heavy_dashed = line_style == LineStyle::HeavyDashed;
            match glyph {
                boundary_type::HORIZONTAL => {
                    if heavy_dashed {
                        "┅"
                    } else {
                        "━"
                    }
                },
                boundary_type::VERTICAL => {
                    if heavy_dashed {
                        "┇"
                    } else {
                        "┃"
                    }
                },
                boundary_type::TOP_LEFT => "┏",
                boundary_type::TOP_LEFT_ROUND => "┏",
                boundary_type::TOP_RIGHT => "┓",
                boundary_type::TOP_RIGHT_ROUND => "┓",
                boundary_type::BOTTOM_LEFT => "┗",
                boundary_type::BOTTOM_LEFT_ROUND => "┗",
                boundary_type::BOTTOM_RIGHT => "┛",
                boundary_type::BOTTOM_RIGHT_ROUND => "┛",
                boundary_type::VERTICAL_LEFT => "┫",
                boundary_type::VERTICAL_RIGHT => "┣",
                boundary_type::HORIZONTAL_DOWN => "┳",
                boundary_type::HORIZONTAL_UP => "┻",
                boundary_type::CROSS => "╋",
                _ => glyph,
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [LineStyle; 5] = [
        LineStyle::Single,
        LineStyle::Double,
        LineStyle::Heavy,
        LineStyle::Dashed,
        LineStyle::HeavyDashed,
    ];

    const CORNERS: [Corner; 4] = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];

    #[test]
    fn every_edge_style_has_its_own_glyph() {
        let horizontals: Vec<&str> = ALL.iter().map(|s| horizontal(*s)).collect();
        let verticals: Vec<&str> = ALL.iter().map(|s| vertical(*s)).collect();
        assert_eq!(horizontals, vec!["─", "═", "━", "┄", "┅"]);
        assert_eq!(verticals, vec!["│", "║", "┃", "┆", "┇"]);
    }

    #[test]
    fn every_corner_combination_resolves_to_a_single_width_glyph() {
        for corner in CORNERS {
            for horizontal_style in ALL {
                for vertical_style in ALL {
                    for rounded in [false, true] {
                        let glyph =
                            super::corner(corner, horizontal_style, vertical_style, rounded);
                        assert_eq!(
                            glyph.chars().count(),
                            1,
                            "{:?} {:?}/{:?} rounded={} produced {:?}",
                            corner,
                            horizontal_style,
                            vertical_style,
                            rounded,
                            glyph
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn corners_only_round_when_both_arms_are_single_width() {
        assert_eq!(
            corner(Corner::TopLeft, LineStyle::Single, LineStyle::Single, true),
            "╭"
        );
        assert_eq!(
            corner(Corner::TopLeft, LineStyle::Dashed, LineStyle::Dashed, true),
            "╭"
        );
        assert_eq!(
            corner(Corner::TopLeft, LineStyle::Double, LineStyle::Single, true),
            "╒"
        );
        assert_eq!(
            corner(Corner::TopLeft, LineStyle::Single, LineStyle::Heavy, true),
            "┎"
        );
    }

    #[test]
    fn mixed_weight_corners_use_the_dedicated_glyphs() {
        assert_eq!(
            corner(
                Corner::BottomRight,
                LineStyle::Double,
                LineStyle::Single,
                false
            ),
            "╛"
        );
        assert_eq!(
            corner(
                Corner::BottomRight,
                LineStyle::Single,
                LineStyle::Double,
                false
            ),
            "╜"
        );
        assert_eq!(
            corner(
                Corner::BottomRight,
                LineStyle::Heavy,
                LineStyle::Single,
                false
            ),
            "┙"
        );
        assert_eq!(
            corner(
                Corner::BottomRight,
                LineStyle::Single,
                LineStyle::Heavy,
                false
            ),
            "┚"
        );
    }

    #[test]
    fn a_double_and_heavy_corner_uses_the_horizontal_arm_for_both() {
        assert_eq!(
            corner(Corner::TopLeft, LineStyle::Double, LineStyle::Heavy, false),
            "╔"
        );
        assert_eq!(
            corner(Corner::TopLeft, LineStyle::Heavy, LineStyle::Double, false),
            "┏"
        );
    }

    #[test]
    fn title_separators_follow_the_horizontal_weight() {
        assert_eq!(title_separator_left(LineStyle::Single), "┤");
        assert_eq!(title_separator_right(LineStyle::Single), "├");
        assert_eq!(title_separator_left(LineStyle::Double), "╡");
        assert_eq!(title_separator_right(LineStyle::Double), "╞");
        assert_eq!(title_separator_left(LineStyle::HeavyDashed), "┥");
        assert_eq!(title_separator_right(LineStyle::HeavyDashed), "┝");
    }

    #[test]
    fn remapping_covers_every_light_glyph_for_double_and_heavy() {
        let light = [
            boundary_type::HORIZONTAL,
            boundary_type::VERTICAL,
            boundary_type::TOP_LEFT,
            boundary_type::TOP_RIGHT,
            boundary_type::BOTTOM_LEFT,
            boundary_type::BOTTOM_RIGHT,
            boundary_type::VERTICAL_LEFT,
            boundary_type::VERTICAL_RIGHT,
            boundary_type::HORIZONTAL_DOWN,
            boundary_type::HORIZONTAL_UP,
            boundary_type::CROSS,
        ];
        for glyph in light {
            for line_style in [LineStyle::Double, LineStyle::Heavy] {
                assert_ne!(
                    remap_light_glyph(glyph, line_style),
                    glyph,
                    "{:?} left {} unmapped",
                    line_style,
                    glyph
                );
            }
        }
    }

    #[test]
    fn dashed_remapping_only_touches_the_straight_runs() {
        assert_eq!(
            remap_light_glyph(boundary_type::HORIZONTAL, LineStyle::Dashed),
            "┄"
        );
        assert_eq!(
            remap_light_glyph(boundary_type::VERTICAL, LineStyle::Dashed),
            "┆"
        );
        assert_eq!(
            remap_light_glyph(boundary_type::CROSS, LineStyle::Dashed),
            boundary_type::CROSS
        );
        assert_eq!(
            remap_light_glyph(boundary_type::HORIZONTAL, LineStyle::HeavyDashed),
            "┅"
        );
        assert_eq!(
            remap_light_glyph(boundary_type::CROSS, LineStyle::HeavyDashed),
            "╋"
        );
    }

    #[test]
    fn a_single_style_remap_is_the_identity() {
        assert_eq!(
            remap_light_glyph(boundary_type::CROSS, LineStyle::Single),
            boundary_type::CROSS
        );
    }
}

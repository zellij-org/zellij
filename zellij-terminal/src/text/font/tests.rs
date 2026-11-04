use super::clusters::*;
use super::test_helpers::*;
use super::*;
use crate::text::font::lookup::choose;
use crate::text::font::raster::confine;
use crate::text::font::raster::downscale;

fn fonts() -> FontStack {
    FontStack::embedded(DEFAULT_FONT_SIZE).unwrap()
}

fn glyph_of(fonts: &mut FontStack, style: FaceStyle, character: char) -> Glyph {
    fonts
        .lookup(style, character)
        .unwrap_or_else(|| panic!("{:?} has no glyph anywhere in the stack", character))
}

#[test]
fn the_embedded_faces_all_parse() {
    let mut fonts = fonts();
    for style in [
        FaceStyle::Regular,
        FaceStyle::Bold,
        FaceStyle::Italic,
        FaceStyle::BoldItalic,
    ] {
        assert!(fonts.lookup(style, 'A').is_some(), "{:?}", style);
    }
}

#[test]
fn the_cell_is_the_documented_size_at_the_default_font_size() {
    assert_eq!(
        fonts().metrics(),
        CellMetrics {
            width: 8,
            height: 20,
            baseline: 15,
            underline_top: 16,
            strikeout_top: 11,
            stroke: 1,
        }
    );
}

#[test]
fn every_face_shares_one_cell_size() {
    let regular = fonts().metrics();
    let advance = |style: FaceStyle| {
        let fonts = fonts();
        let font = fonts.faces[style.index()];
        font.glyph_metrics(&[])
            .scale(DEFAULT_FONT_SIZE)
            .advance_width(font.charmap().map('M'))
    };
    for style in [FaceStyle::Bold, FaceStyle::Italic, FaceStyle::BoldItalic] {
        assert_eq!(advance(style).round() as u32, regular.width, "{:?}", style);
    }
}

#[test]
fn the_cell_scales_with_the_font_size() {
    let doubled = FontStack::embedded(DEFAULT_FONT_SIZE * 2.0)
        .unwrap()
        .metrics();
    assert_eq!(doubled.width, 16);
    assert_eq!(doubled.height, 40);
}

fn synthetic_discovery() -> Option<Discovery> {
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
    let discovery = Discovery::synthetic(&directory);
    if discovery.is_none() {
        eprintln!("fontconfig is unavailable here; family selection is untested");
    }
    discovery
}

fn requested(family: &str) -> FontOptions {
    FontOptions {
        family: Some(family.to_owned()),
        size: DEFAULT_FONT_SIZE,
        system_fonts: true,
        ligatures: DEFAULT_LIGATURES,
        ..FontOptions::default()
    }
}

fn adjusted(cell: CellAdjust) -> FontStack {
    FontStack::build_with(
        &FontOptions {
            system_fonts: false,
            cell,
            ..FontOptions::default()
        },
        None,
    )
    .unwrap()
}

#[test]
fn line_height_grows_the_cell_and_moves_the_baseline_down_by_half_the_growth() {
    let natural = fonts().metrics();
    let stack = adjusted(CellAdjust {
        line_height: 1.5,
        ..CellAdjust::default()
    });
    let metrics = stack.metrics();
    assert_eq!(metrics.width, natural.width);
    assert_eq!(metrics.height, 30);
    assert_eq!(metrics.baseline, natural.baseline + 5);
    assert_eq!(metrics.underline_top, natural.underline_top + 5);
    assert_eq!(metrics.strikeout_top, natural.strikeout_top + 5);
}

#[test]
fn a_shorter_line_still_keeps_its_lines_inside_the_cell() {
    let stack = adjusted(CellAdjust {
        line_height: 0.5,
        ..CellAdjust::default()
    });
    let metrics = stack.metrics();
    assert_eq!(metrics.height, 10);
    assert!(metrics.baseline >= 1 && metrics.baseline <= metrics.height);
    assert!(metrics.underline_top + stack.placement().underline_stroke <= metrics.height);
    assert!(metrics.strikeout_top + metrics.stroke <= metrics.height);
}

#[test]
fn cell_width_widens_the_cell_and_centres_the_glyph_in_it() {
    let natural = fonts().metrics();
    let stack = adjusted(CellAdjust {
        cell_width: 1.5,
        ..CellAdjust::default()
    });
    assert_eq!(stack.metrics().width, 12);
    assert_eq!(stack.metrics().height, natural.height);
    assert_eq!(stack.placement().natural_width, 8);
    assert_eq!(stack.placement().inset, 2);

    let mut plain = fonts();
    let mut wide = stack;
    let glyph = glyph_of(&mut plain, FaceStyle::Regular, 'M');
    let at_natural = plain.rasterize(glyph, 1).unwrap();
    let at_wide = wide.rasterize(glyph, 1).unwrap();
    assert_eq!(at_wide.left, at_natural.left + 2);
    assert_eq!(at_wide.width, at_natural.width);
}

#[test]
fn shaped_glyphs_land_on_their_own_widened_cells() {
    let mut wide = FontStack::build_with(
        &FontOptions {
            system_fonts: false,
            ligatures: true,
            cell: CellAdjust {
                cell_width: 1.5,
                ..CellAdjust::default()
            },
            ..FontOptions::default()
        },
        None,
    )
    .unwrap();
    let font = wide.primary_of(FaceStyle::Regular);
    let shaped = wide.shape(font, "->");
    assert!(!shaped.is_empty());
    for glyph in shaped.iter() {
        let within = glyph.dx.rem_euclid(12);
        assert!(
            within <= 1 || within >= 11,
            "{:?} does not start on a 12 px cell",
            glyph
        );
    }
}

#[test]
fn a_large_baseline_offset_is_held_inside_the_cell() {
    for offset in [-64.0, 64.0] {
        let metrics = adjusted(CellAdjust {
            baseline_offset: offset,
            ..CellAdjust::default()
        })
        .metrics();
        assert!(metrics.baseline >= 1 && metrics.baseline <= metrics.height);
    }
    let lowered = adjusted(CellAdjust {
        baseline_offset: 2.0,
        ..CellAdjust::default()
    })
    .metrics();
    assert_eq!(lowered.baseline, fonts().metrics().baseline + 2);
}

#[test]
fn underline_adjustments_apply_on_top_of_the_font_and_stay_inside_the_cell() {
    let natural = fonts().metrics();
    let moved = adjusted(CellAdjust {
        underline_offset: 1.0,
        underline_thickness: 1.0,
        ..CellAdjust::default()
    });
    assert_eq!(moved.metrics().underline_top, natural.underline_top + 1);
    assert_eq!(moved.placement().underline_stroke, natural.stroke + 1);
    assert_eq!(moved.metrics().stroke, natural.stroke);

    for (offset, thickness) in [(64.0, 64.0), (-64.0, 0.0), (64.0, -64.0)] {
        let stack = adjusted(CellAdjust {
            underline_offset: offset,
            underline_thickness: thickness,
            ..CellAdjust::default()
        });
        let (metrics, placement) = (stack.metrics(), stack.placement());
        assert!(placement.underline_stroke >= 1);
        assert!(
            metrics.underline_top + placement.underline_stroke <= metrics.height,
            "{} {}: {:?} {:?}",
            offset,
            thickness,
            metrics,
            placement
        );
    }
}

#[test]
fn default_adjustments_leave_the_cell_as_the_font_measures_it() {
    let stack = adjusted(CellAdjust::default());
    assert_eq!(stack.metrics(), fonts().metrics());
    assert_eq!(stack.placement().inset, 0);
    assert_eq!(stack.placement().underline_stroke, stack.metrics().stroke);
}

#[test]
fn a_weight_request_on_a_static_font_changes_nothing() {
    let stack = FontStack::build_with(
        &FontOptions {
            system_fonts: false,
            weight: 500,
            ..FontOptions::default()
        },
        None,
    )
    .unwrap();
    assert_eq!(stack.metrics(), fonts().metrics());
    assert!(stack.coords.iter().all(Vec::is_empty));
}

#[test]
fn a_heavy_weight_on_the_bundled_font_draws_regular_text_with_the_bold_face() {
    let stack = |weight| {
        FontStack::build_with(
            &FontOptions {
                system_fonts: false,
                weight,
                ..FontOptions::default()
            },
            None,
        )
        .unwrap()
    };
    let regular = stack(400);
    assert_eq!(regular.primary_of(FaceStyle::Regular), FontId(0));
    assert_eq!(regular.primary_of(FaceStyle::Italic), FontId(2));
    let heavy = stack(600);
    assert_eq!(heavy.primary_of(FaceStyle::Regular), FontId(1));
    assert_eq!(heavy.primary_of(FaceStyle::Italic), FontId(3));
    assert_eq!(heavy.primary_of(FaceStyle::Bold), FontId(1));
    let light = stack(200);
    assert_eq!(light.primary_of(FaceStyle::Bold), FontId(1));
}

#[test]
fn bold_is_three_hundred_heavier_and_capped() {
    assert_eq!(bold_weight(400), 700);
    assert_eq!(bold_weight(500), 800);
    assert_eq!(bold_weight(700), 900);
    assert_eq!(
        FaceRequest::weighted(FaceStyle::BoldItalic, 300).weight,
        600
    );
    assert!(FaceRequest::weighted(FaceStyle::BoldItalic, 300).italic);
}

#[test]
fn features_turn_shaping_on_and_keep_ligatures_off_when_asked() {
    let zero = FontFeature::parse("zero").unwrap();
    let stack = FontStack::build_with(
        &FontOptions {
            system_fonts: false,
            ligatures: false,
            features: vec![zero, FontFeature::parse("+calt").unwrap()],
            ..FontOptions::default()
        },
        None,
    )
    .unwrap();
    assert!(stack.shapes_runs());
    let tags: Vec<(&[u8; 4], u16)> = stack
        .features
        .iter()
        .map(|feature| (&feature.tag, feature.value))
        .collect();
    assert!(tags.contains(&(b"liga", 0)));
    assert!(tags.contains(&(b"dlig", 0)));
    assert!(!tags.contains(&(b"calt", 0)));
    assert!(tags.contains(&(b"calt", 1)));
    assert!(tags.contains(&(b"zero", 1)));

    let unshaped = FontStack::build_with(
        &FontOptions {
            system_fonts: false,
            ligatures: false,
            ..FontOptions::default()
        },
        None,
    )
    .unwrap();
    assert!(!unshaped.shapes_runs());
}

fn arrow_ligates(ligatures: bool, features: &[&str]) -> bool {
    let mut stack = FontStack::build_with(
        &FontOptions {
            system_fonts: false,
            ligatures,
            features: features
                .iter()
                .map(|feature| FontFeature::parse(feature).unwrap())
                .collect(),
            ..FontOptions::default()
        },
        None,
    )
    .unwrap();
    let font = stack.primary_of(FaceStyle::Regular);
    let nominal: Vec<GlyphId> = "->"
        .chars()
        .map(|character| glyph_of(&mut stack, FaceStyle::Regular, character).glyph)
        .collect();
    let shaped: Vec<GlyphId> = stack.shape(font, "->").iter().map(|g| g.glyph).collect();
    shaped != nominal
}

#[test]
fn a_listed_feature_changes_what_the_shaper_draws() {
    assert!(
        arrow_ligates(true, &[]),
        "the arrow no longer ligates at all"
    );
    assert!(
        !arrow_ligates(true, &["-calt"]),
        "turning calt off left the arrow joined"
    );
    assert!(
        arrow_ligates(false, &["+calt"]),
        "listing calt with ligatures off did not join the arrow"
    );
    assert!(
        !arrow_ligates(false, &["zero"]),
        "an unrelated feature with ligatures off still joined the arrow"
    );
}

#[test]
fn a_requested_weight_picks_the_matching_installed_face() {
    let Some(discovery) = synthetic_discovery() else {
        return;
    };
    let weight_of = |stack: &FontStack, style: FaceStyle| {
        stack.faces[stack.primary_of(style).0]
            .attributes()
            .weight()
            .0
    };
    let regular = FontStack::build_with(&requested("Iosevka Term"), Some(discovery)).unwrap();
    assert_eq!(weight_of(&regular, FaceStyle::Regular), 400);
    assert_eq!(weight_of(&regular, FaceStyle::Bold), 700);

    let heavy = FontStack::build_with(
        &FontOptions {
            weight: 700,
            ..requested("Iosevka Term")
        },
        Some(discovery),
    )
    .unwrap();
    assert_eq!(
        weight_of(&heavy, FaceStyle::Regular),
        700,
        "a bold regular weight did not pick the bold file"
    );
}

#[test]
fn feature_tags_parse_with_an_optional_sign() {
    assert_eq!(
        FontFeature::parse("-liga"),
        Some(FontFeature {
            tag: *b"liga",
            value: 0
        })
    );
    assert_eq!(
        FontFeature::parse("ss01"),
        Some(FontFeature {
            tag: *b"ss01",
            value: 1
        })
    );
    assert_eq!(FontFeature::parse("ss1"), None);
    assert_eq!(FontFeature::parse("--liga"), None);
}

const EMBEDDED_PRIMARY: [FontId; 4] = [FontId(0), FontId(1), FontId(2), FontId(3)];

#[test]
fn a_resolved_family_replaces_the_primary_and_measures_the_cell() {
    let Some(discovery) = synthetic_discovery() else {
        return;
    };
    let mut stack = FontStack::build_with(&requested("Iosevka Term"), Some(discovery)).unwrap();
    assert!(
        stack.primary.iter().all(|font| font.0 >= 6),
        "the requested family did not replace the embedded primary: {:?}",
        stack.primary
    );
    assert_eq!(
        stack.metrics(),
        fonts().metrics(),
        "the requested family is the same Iosevka and must measure the same cell"
    );
    let glyph = glyph_of(&mut stack, FaceStyle::Regular, 'A');
    assert!(stack.is_primary(glyph.font));
}

#[test]
fn a_family_that_does_not_resolve_keeps_the_embedded_primary() {
    let Some(discovery) = synthetic_discovery() else {
        return;
    };
    let stack =
        FontStack::build_with(&requested("No Such Family At All"), Some(discovery)).unwrap();
    assert_eq!(stack.primary, EMBEDDED_PRIMARY);
    assert_eq!(stack.metrics(), fonts().metrics());
}

#[test]
fn a_family_request_without_discovery_falls_back_rather_than_failing() {
    let mut stack = FontStack::build_with(&requested("Iosevka Term"), None).unwrap();
    assert_eq!(stack.primary, EMBEDDED_PRIMARY);
    assert!(stack.lookup(FaceStyle::Regular, 'A').is_some());
}

#[test]
fn a_resolved_family_still_falls_back_for_what_it_does_not_carry() {
    let Some(discovery) = synthetic_discovery() else {
        return;
    };
    let mut stack = FontStack::build_with(&requested("Iosevka Term"), Some(discovery)).unwrap();
    let glyph = glyph_of(&mut stack, FaceStyle::Regular, '\u{1f600}');
    assert!(!stack.is_primary(glyph.font));
}

#[test]
fn the_primary_answers_latin_before_any_fallback() {
    let mut fonts = fonts();
    let glyph = glyph_of(&mut fonts, FaceStyle::Regular, 'A');
    assert!(fonts.is_primary(glyph.font));
}

#[test]
fn a_codepoint_the_primary_lacks_falls_back() {
    let mut fonts = fonts();
    let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{4f60}');
    assert!(!fonts.is_primary(glyph.font));
}

#[test]
fn every_style_reaches_the_same_fallback_face() {
    let mut fonts = fonts();
    let regular = glyph_of(&mut fonts, FaceStyle::Regular, '\u{4f60}');
    let bold = glyph_of(&mut fonts, FaceStyle::Bold, '\u{4f60}');
    assert_eq!(regular.font, bold.font);
}

#[test]
fn an_emoji_resolves_to_a_color_glyph() {
    let mut fonts = fonts();
    let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{1f600}');
    let bitmap = fonts.rasterize(glyph, 2).unwrap();
    assert_eq!(bitmap.content, GlyphContent::Color);
    assert_eq!(
        bitmap.coverage.len(),
        (bitmap.width * bitmap.height * 4) as usize
    );
    assert!(bitmap.coverage.chunks(4).any(|pixel| pixel[3] > 0));
}

#[test]
fn a_fallback_glyph_is_confined_to_its_cell_budget() {
    let mut fonts = fonts();
    let metrics = fonts.metrics();
    for (character, budget) in [('\u{4f60}', 2u32), ('\u{1f600}', 2), ('\u{3042}', 2)] {
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, character);
        let bitmap = fonts.rasterize(glyph, budget).unwrap();
        assert!(
            bitmap.width <= metrics.width * budget,
            "{:?} is {} px wide",
            character,
            bitmap.width
        );
        assert!(
            bitmap.height <= metrics.height,
            "{:?} is {} px tall",
            character,
            bitmap.height
        );
        assert!(bitmap.left >= 0);
    }
}

#[test]
fn a_narrow_budget_shrinks_the_same_glyph_further() {
    let mut fonts = fonts();
    let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{1f600}');
    let wide = fonts.rasterize(glyph, 2).unwrap();
    let narrow = fonts.rasterize(glyph, 1).unwrap();
    assert!(
        narrow.width < wide.width,
        "{} vs {}",
        narrow.width,
        wide.width
    );
    assert!(narrow.width <= fonts.metrics().width);
}

#[test]
fn a_codepoint_no_face_carries_resolves_to_nothing() {
    assert!(fonts().lookup(FaceStyle::Regular, '\u{10fffd}').is_none());
}

#[test]
fn a_rendered_glyph_fits_inside_the_cell_box() {
    let mut fonts = fonts();
    let metrics = fonts.metrics();
    let glyph = glyph_of(&mut fonts, FaceStyle::Regular, 'M');
    let bitmap = fonts.rasterize(glyph, 1).unwrap();
    assert!(bitmap.width <= metrics.width, "{}", bitmap.width);
    assert!(bitmap.height <= metrics.height, "{}", bitmap.height);
    assert_eq!(
        bitmap.coverage.len(),
        (bitmap.width * bitmap.height) as usize
    );
    assert!(bitmap.coverage.iter().any(|&value| value > 0));
}

#[test]
fn a_blank_glyph_rasterizes_to_nothing() {
    let mut fonts = fonts();
    let glyph = glyph_of(&mut fonts, FaceStyle::Regular, ' ');
    assert!(fonts.rasterize(glyph, 1).is_none());
}

#[test]
fn rasterization_is_repeatable() {
    let mut fonts = fonts();
    let glyph = glyph_of(&mut fonts, FaceStyle::Bold, 'g');
    let first = fonts.rasterize(glyph, 1).unwrap();
    let second = fonts.rasterize(glyph, 1).unwrap();
    assert_eq!(first.coverage, second.coverage);
    assert_eq!((first.left, first.top), (second.left, second.top));
}

#[test]
fn a_point_is_a_ninety_sixth_of_an_inch_off_macos_and_a_pixel_on_it() {
    let expected = if cfg!(target_os = "macos") {
        12.0
    } else {
        16.0
    };
    assert_eq!(points_to_pixels(DEFAULT_FONT_POINTS), expected);
    assert_eq!(pixels_to_points(points_to_pixels(17.5)), 17.5);
}

#[test]
fn the_default_point_size_matches_the_cell_the_window_has_always_drawn_off_macos() {
    if cfg!(target_os = "macos") {
        return;
    }
    assert_eq!(points_to_pixels(DEFAULT_FONT_POINTS), DEFAULT_FONT_SIZE);
}

#[test]
fn an_out_of_range_font_size_is_rejected() {
    assert!(FontStack::embedded(0.0).is_err());
    assert!(FontStack::embedded(f32::NAN).is_err());
    assert!(FontStack::embedded(10_000.0).is_err());
}

#[test]
fn style_selection_covers_the_four_faces() {
    assert_eq!(FaceStyle::of(false, false), FaceStyle::Regular);
    assert_eq!(FaceStyle::of(true, false), FaceStyle::Bold);
    assert_eq!(FaceStyle::of(false, true), FaceStyle::Italic);
    assert_eq!(FaceStyle::of(true, true), FaceStyle::BoldItalic);
}

#[test]
fn the_embedded_stack_consults_no_system_fonts() {
    assert!(!fonts().has_discovery());
}

fn candidate(id: usize, color: bool) -> (Glyph, bool) {
    (
        Glyph {
            font: FontId(id),
            glyph: 1,
        },
        color,
    )
}

#[test]
fn a_color_capable_system_face_is_taken_immediately() {
    let chosen = choose(Some(candidate(9, true)), &[candidate(5, true)]);
    assert_eq!(chosen.unwrap().font, FontId(9));
}

#[test]
fn a_monochrome_system_face_loses_to_a_color_capable_fallback() {
    let chosen = choose(Some(candidate(9, false)), &[candidate(5, true)]);
    assert_eq!(chosen.unwrap().font, FontId(5));
}

#[test]
fn a_monochrome_system_face_still_beats_a_monochrome_fallback() {
    let chosen = choose(Some(candidate(9, false)), &[candidate(4, false)]);
    assert_eq!(chosen.unwrap().font, FontId(9));
}

#[test]
fn the_first_fallback_wins_when_nothing_was_discovered() {
    let chosen = choose(None, &[candidate(4, false), candidate(5, true)]);
    assert_eq!(chosen.unwrap().font, FontId(4));
}

#[test]
fn a_color_fallback_is_reached_past_a_monochrome_one() {
    let chosen = choose(
        Some(candidate(9, false)),
        &[candidate(4, false), candidate(5, true)],
    );
    assert_eq!(chosen.unwrap().font, FontId(5));
}

#[test]
fn nerd_font_icons_resolve_to_the_embedded_symbols_font() {
    let mut fonts = fonts();
    for icon in ['\u{e62b}', '\u{f07b}', '\u{f0520}', '\u{f16a0}', '\u{e0b0}'] {
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, icon);
        assert!(
            fonts.rasterize(glyph, 1).is_some(),
            "{:?} rendered nothing",
            icon
        );
    }
    let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{f16a0}');
    assert_eq!(glyph.font, FontId(6));
}

#[test]
fn private_use_icons_skip_system_font_discovery() {
    let mut fonts = FontStack::build(&FontOptions::default()).unwrap();
    for icon in ['\u{e62b}', '\u{f07b}', '\u{f0520}', '\u{f16a0}'] {
        for style in [FaceStyle::Regular, FaceStyle::Bold] {
            let glyph = glyph_of(&mut fonts, style, icon);
            assert_eq!(glyph.font, SYMBOLS_FONT);
        }
    }
    assert_eq!(
        fonts.discovery_queries, 0,
        "system fonts were searched for icons the bundled symbols font has"
    );
    glyph_of(&mut fonts, FaceStyle::Regular, '\u{4f60}');
    fonts.lookup(FaceStyle::Regular, '\u{10fffd}');
    assert!(
        fonts.discovery_queries > 0,
        "characters outside the icon ranges must still reach system fonts"
    );
    assert!(is_private_use('\u{e62b}'));
    assert!(!is_private_use('\u{4f60}'));
}

#[test]
fn nothing_anywhere_chooses_nothing() {
    assert!(choose(None, &[]).is_none());
}

#[test]
fn the_emoji_face_is_the_only_color_capable_embedded_one() {
    let fonts = fonts();
    let capable: Vec<usize> = fonts
        .color_capable
        .iter()
        .enumerate()
        .filter(|(_, capable)| **capable)
        .map(|(index, _)| index)
        .collect();
    assert_eq!(capable, vec![5]);
}

#[test]
fn an_emoji_stays_in_color_even_when_system_fonts_are_consulted() {
    let mut fonts = FontStack::new(DEFAULT_FONT_SIZE, true).unwrap();
    let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{1f600}');
    assert!(
        fonts.has_color(glyph.font),
        "a monochrome face won the emoji"
    );
    assert_eq!(
        fonts.rasterize(glyph, 2).unwrap().content,
        GlyphContent::Color
    );
}

#[test]
fn a_system_face_is_still_preferred_where_no_color_is_at_stake() {
    let mut fonts = FontStack::new(DEFAULT_FONT_SIZE, true).unwrap();
    if !fonts.has_discovery() {
        return;
    }
    let Some(glyph) = fonts.lookup(FaceStyle::Regular, '\u{2801}') else {
        return;
    };
    assert!(!fonts.is_primary(glyph.font));
    assert!(!fonts.has_color(glyph.font));
    assert!(glyph.font.0 >= 6, "braille came from an embedded face");
}

#[test]
fn a_box_filter_averages_the_source_it_covers() {
    let source = vec![0u8, 100, 200, 255];
    assert_eq!(downscale(&source, 2, 2, 1, 1, 1), vec![138]);
}

#[test]
fn a_box_filter_keeps_channels_separate() {
    let source = vec![10u8, 20, 30, 40, 50, 60, 70, 80];
    assert_eq!(downscale(&source, 2, 1, 4, 1, 1), vec![30, 40, 50, 60]);
}

#[test]
fn a_box_filter_halves_a_quadrant_at_a_time() {
    let source: Vec<u8> = (0..16).map(|value| value * 16).collect();
    assert_eq!(downscale(&source, 4, 4, 1, 2, 2), vec![40, 72, 168, 200]);
}

#[test]
fn a_non_integer_ratio_covers_every_source_pixel_exactly_once() {
    let source = vec![255u8; 9];
    let scaled = downscale(&source, 3, 3, 1, 2, 2);
    assert_eq!(scaled, vec![255; 4]);
    assert_eq!(downscale(&vec![0u8; 9], 3, 3, 1, 2, 2), vec![0; 4]);
}

fn fit() -> Fit {
    Fit::new(fonts().metrics(), 1)
}

fn mask(left: i32, top: i32, width: u32, height: u32) -> GlyphBitmap {
    GlyphBitmap {
        left,
        top,
        width,
        height,
        coverage: vec![200; (width * height) as usize],
        content: GlyphContent::Mask,
    }
}

#[test]
fn an_oversized_bitmap_is_confined_by_its_tallest_axis() {
    let confined = confine(mask(0, 40, 10, 100), 16, 20);
    assert_eq!(confined.height, 20);
    assert_eq!(confined.width, 2);
    assert_eq!(
        confined.coverage.len(),
        (confined.width * confined.height) as usize
    );
    assert!(confined.coverage.iter().all(|value| *value == 200));
    assert_eq!(confined.top, 8, "the bearing shrinks with the bitmap");
}

#[test]
fn a_fallback_glyph_keeps_its_own_bearing_inside_its_centred_advance() {
    let fit = fit();
    let placed = fit.place(mask(1, 12, 4, 10), 6.0);
    assert_eq!((placed.width, placed.height), (4, 10));
    assert_eq!(placed.left, 1 + 1);
    assert_eq!(placed.top, 12, "the glyph stays on the text baseline");
}

#[test]
fn glyphs_drawn_on_one_grid_keep_their_shared_dots_aligned() {
    let fit = fit();
    let advance = 7.0;
    let spinner = [
        mask(1, 13, 5, 7),
        mask(1, 13, 1, 11),
        mask(1, 9, 5, 7),
        mask(5, 13, 1, 3),
    ];
    let lefts: Vec<i32> = spinner
        .iter()
        .take(3)
        .map(|frame| fit.place(frame.clone(), advance).left)
        .collect();
    assert!(
        lefts.iter().all(|left| *left == lefts[0]),
        "a dot in the first column moved between frames: {:?}",
        lefts
    );
    let tops: Vec<i32> = [&spinner[0], &spinner[1], &spinner[3]]
        .into_iter()
        .map(|frame| fit.place(frame.clone(), advance).top)
        .collect();
    assert!(
        tops.iter().all(|top| *top == tops[0]),
        "a dot in the top row moved between frames: {:?}",
        tops
    );
}

#[test]
fn a_glyph_family_is_scaled_by_its_advance_not_by_each_glyphs_ink() {
    let fit = fit();
    let room = fit.box_width as f32 * FALLBACK_OVERHANG;
    assert_eq!(fit.advance_scale(fit.box_width as f32), 1.0);
    assert_eq!(fit.advance_scale(room), 1.0);
    let wide = 13.4;
    assert!((fit.advance_scale(wide) - room / wide).abs() < f32::EPSILON);
    assert_eq!(fit.advance_scale(f32::NAN), 1.0);
}

#[test]
fn a_symbol_with_a_blank_neighbour_keeps_its_size_and_centres_across_both_cells() {
    let fit = fit().spilling(1);
    let two_cells = (fit.box_width * 2) as f32;
    assert_eq!(fit.advance_scale(two_cells), 1.0);
    assert!(fit.advance_scale(two_cells * 2.0) < 1.0);
    assert_eq!(fit.ink_scale(&mask(0, 10, fit.box_width * 2, 8)), 1.0);
    let placed = fit.place(mask(1, 10, 12, 8), 14.0);
    assert_eq!(placed.left, 1 + 1);
    let narrow = fit.place(mask(1, 10, 4, 8), 6.0);
    assert_eq!(
        narrow.left,
        1 + 1,
        "a symbol that fits its own cell stays there"
    );
}

#[test]
fn a_monochrome_symbol_may_overhang_a_little_rather_than_shrink() {
    let fit = fit();
    let slightly_wide = mask(0, 10, fit.box_width + 1, 8);
    assert_eq!(fit.ink_scale(&slightly_wide), 1.0);
    let color = GlyphBitmap {
        content: GlyphContent::Color,
        coverage: vec![200; ((fit.box_width + 1) * 8 * 4) as usize],
        ..mask(0, 10, fit.box_width + 1, 8)
    };
    assert!(
        fit.ink_scale(&color) < 1.0,
        "an emoji must stay inside its box"
    );
    let far_too_wide = mask(0, 10, fit.box_width * 2, 8);
    assert!(fit.ink_scale(&far_too_wide) < 1.0);
}

#[test]
fn a_glyph_poking_out_of_the_cell_is_nudged_back_in() {
    let fit = fit();
    let metrics = fonts().metrics();
    let above = fit.place(mask(0, metrics.baseline as i32 + 3, 4, 6), 8.0);
    assert_eq!(above.top, metrics.baseline as i32);
    let below = fit.place(mask(0, -6, 4, 6), 8.0);
    assert_eq!(
        metrics.baseline as i32 - below.top + below.height as i32,
        metrics.height as i32
    );
}

#[test]
fn a_full_width_fallback_glyph_keeps_the_fonts_own_bearing() {
    let mut fonts = fonts();
    let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{4f60}');
    let font = fonts.faces[glyph.font.0];
    let raw = fonts
        .render(font, &[], glyph.glyph, DEFAULT_FONT_SIZE, FALLBACK_SOURCES)
        .unwrap();
    let placed = fonts.rasterize(glyph, 2).unwrap();
    assert_eq!((placed.left, placed.top), (raw.left, raw.top));
    assert_eq!((placed.width, placed.height), (raw.width, raw.height));
}

#[test]
fn ideographic_punctuation_keeps_the_place_its_font_gives_it() {
    let mut fonts = fonts();
    let metrics = fonts.metrics();
    let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{3001}');
    let comma = fonts.rasterize(glyph, 2).unwrap();
    let centre = comma.left + comma.width as i32 / 2;
    assert!(
        centre < metrics.width as i32,
        "the ideographic comma was pulled to the middle of its cells: {}",
        centre
    );
    let bottom = metrics.baseline as i32 - comma.top + comma.height as i32;
    assert!(
        bottom >= metrics.baseline as i32 - 1,
        "the ideographic comma was lifted off the baseline: its ink ends at {}",
        bottom
    );
}

const FAMILY: &str = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
const SKIN_TONE: &str = "\u{1f44d}\u{1f3fd}";
const FLAG: &str = "\u{1f1e9}\u{1f1ea}";
const RAINBOW_FLAG: &str = "\u{1f3f3}\u{fe0f}\u{200d}\u{1f308}";
const ENGLAND_FLAG: &str = "\u{1f3f4}\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}";
const KEYCAP: &str = "1\u{fe0f}\u{20e3}";

#[test]
fn the_script_of_a_cluster_comes_from_its_text() {
    assert_eq!(script_of("\u{915}\u{94d}\u{937}"), Script::Devanagari);
    assert_eq!(script_of("\u{e01}\u{e49}\u{e33}"), Script::Thai);
    assert_eq!(script_of("\u{1100}\u{1161}\u{11a8}"), Script::Hangul);
    assert_eq!(script_of(FAMILY), Script::Common);
    assert_eq!(script_of("e\u{301}"), Script::Latin);
    assert_eq!(script_of("\u{301}e"), Script::Latin);
}

#[test]
fn joining_controls_do_not_decide_coverage() {
    for control in [
        '\u{200d}',
        '\u{fe0f}',
        '\u{fe0e}',
        '\u{e0067}',
        '\u{e0100}',
        '\u{ad}',
    ] {
        assert!(is_joining_control(control), "{:?}", control);
    }
    for visible in ['a', '\u{301}', '\u{1f468}', '\u{94d}'] {
        assert!(!is_joining_control(visible), "{:?}", visible);
    }
}

#[test]
fn emoji_sequences_ask_for_colour_and_marks_do_not() {
    for sequence in [FAMILY, SKIN_TONE, FLAG, RAINBOW_FLAG, ENGLAND_FLAG, KEYCAP] {
        assert!(wants_color(sequence), "{:?}", sequence);
    }
    for text in ["e\u{301}", "\u{915}\u{94d}\u{937}", "a\u{200d}b"] {
        assert!(!wants_color(text), "{:?}", text);
    }
}

#[test]
fn the_font_chosen_for_a_cluster_covers_all_of_it() {
    let mut fonts = fonts();
    for text in [
        "e\u{301}",
        "a\u{308}\u{30a}",
        "\u{1100}\u{1161}\u{11a8}",
        "\u{915}\u{94d}\u{937}",
        "\u{e01}\u{e49}\u{e33}",
        FAMILY,
        SKIN_TONE,
    ] {
        if let Some(font) = fonts.cluster_font(FaceStyle::Regular, text) {
            assert!(
                fonts.covers(font, &drawn_text(text)),
                "{:?} chose {:?}",
                text,
                font
            );
        }
    }
    assert!(fonts.cluster_font(FaceStyle::Regular, "e\u{301}").is_some());
    assert_eq!(
        fonts.cluster_font(FaceStyle::Regular, "\u{1100}\u{1161}\u{11a8}"),
        Some(FontId(4)),
        "conjoining jamo are drawn from the bundled CJK font's composed syllable"
    );
    assert!(fonts
        .cluster_font(FaceStyle::Regular, "\u{10fffd}\u{301}")
        .is_none());
}

const PRESENTED: [&str; 3] = ["\u{26a0}\u{fe0f}", "\u{2764}\u{fe0f}", "\u{1f44d}\u{fe0f}"];

#[test]
fn an_emoji_cluster_is_given_to_the_colour_face_even_when_another_face_covers_it() {
    let mut fonts = fonts();
    for sequence in PRESENTED {
        let font = fonts
            .cluster_font(FaceStyle::Regular, sequence)
            .unwrap_or_else(|| panic!("{:?} found no face", sequence));
        assert!(fonts.has_color(font), "{:?} went to {:?}", sequence, font);
    }
    let primary = fonts.primary_of(FaceStyle::Regular);
    assert!(
        fonts.covers(primary, "\u{26a0}\u{fe0f}"),
        "the primary face no longer carries the warning sign, so this test proves nothing"
    );
}

#[test]
fn an_emoji_with_its_presentation_selector_is_one_colour_sprite() {
    let mut fonts = fonts();
    for sequence in PRESENTED {
        match fonts.rasterize_cluster(FaceStyle::Regular, sequence, 2) {
            ClusterRaster::Sprite(bitmap) => {
                assert_eq!(bitmap.content, GlyphContent::Color, "{:?}", sequence);
                assert!(bitmap.width <= fonts.metrics().width * 2, "{:?}", sequence);
            },
            _ => panic!("{:?} did not rasterize as one sprite", sequence),
        }
    }
    assert!(fonts.shaped_clusters() > 0);
    assert_eq!(
        fonts.shaped_runs(),
        0,
        "cluster shaping must not count as ligature shaping"
    );
}

#[test]
fn the_bundled_emoji_font_joins_each_sequence_into_one_colour_glyph() {
    let mut fonts = fonts();
    let metrics = fonts.metrics();
    for sequence in [FAMILY, SKIN_TONE, FLAG, RAINBOW_FLAG, ENGLAND_FLAG, KEYCAP] {
        let font = fonts
            .cluster_font(FaceStyle::Regular, sequence)
            .unwrap_or_else(|| panic!("{:?} found no face", sequence));
        assert!(fonts.has_color(font), "{:?} went to {:?}", sequence, font);
        let shaped_text = fonts.shaping_text(font, sequence).into_owned();
        let glyphs = fonts.shape_cluster(font, &shaped_text);
        assert_eq!(glyphs.len(), 1, "{:?} shaped into {:?}", sequence, glyphs);
        assert_ne!(glyphs[0].glyph, 0, "{:?}", sequence);
        match fonts.rasterize_cluster(FaceStyle::Regular, sequence, 2) {
            ClusterRaster::Sprite(bitmap) => {
                assert_eq!(bitmap.content, GlyphContent::Color, "{:?}", sequence);
                assert!(bitmap.left >= 0, "{:?}", sequence);
                assert!(
                    bitmap.left as u32 + bitmap.width <= metrics.width * 2,
                    "{:?} is wider than its two columns",
                    sequence
                );
                assert!(bitmap.height <= metrics.height, "{:?}", sequence);
            },
            _ => panic!("{:?} did not rasterize as one sprite", sequence),
        }
    }
}

#[test]
fn a_joined_sequence_differs_from_its_first_piece() {
    let mut fonts = fonts();
    for (sequence, first) in [
        (SKIN_TONE, "\u{1f44d}"),
        (FLAG, "\u{1f1e9}"),
        (FAMILY, "\u{1f468}"),
        (RAINBOW_FLAG, "\u{1f3f3}"),
        (ENGLAND_FLAG, "\u{1f3f4}"),
        (KEYCAP, "1"),
    ] {
        let font = fonts.cluster_font(FaceStyle::Regular, sequence).unwrap();
        let shaped_text = fonts.shaping_text(font, sequence).into_owned();
        let joined = fonts.shape_cluster(font, &shaped_text);
        let alone = fonts.shape_cluster(font, first);
        assert_ne!(
            joined[0].glyph, alone[0].glyph,
            "{:?} did not join",
            sequence
        );
    }
}

#[test]
fn a_letter_with_a_mark_is_one_sprite_inside_its_cell() {
    let mut fonts = fonts();
    let metrics = fonts.metrics();
    let ClusterRaster::Sprite(bitmap) = fonts.rasterize_cluster(FaceStyle::Regular, "e\u{301}", 1)
    else {
        panic!("e with an acute accent did not rasterize");
    };
    assert_eq!(bitmap.content, GlyphContent::Mask);
    assert!(bitmap.top <= metrics.baseline as i32);
    assert!(bitmap.height <= metrics.height);
    let plain = glyph_of(&mut fonts, FaceStyle::Regular, 'e');
    let plain = fonts.rasterize(plain, 1).unwrap();
    assert!(bitmap.top > plain.top, "the accent is not above the letter");
}

#[test]
fn a_tall_stack_of_marks_is_cropped_to_its_row() {
    let mut fonts = fonts();
    let metrics = fonts.metrics();
    let zalgo: String = std::iter::once('a')
        .chain(std::iter::repeat('\u{301}').take(15))
        .chain(std::iter::repeat('\u{316}').take(15))
        .collect();
    match fonts.rasterize_cluster(FaceStyle::Regular, &zalgo, 1) {
        ClusterRaster::Sprite(bitmap) => {
            assert!(bitmap.height <= metrics.height, "{}", bitmap.height);
            assert!(bitmap.top <= metrics.baseline as i32);
            assert!(
                bitmap.top - bitmap.height as i32
                    >= metrics.baseline as i32 - metrics.height as i32
            );
        },
        ClusterRaster::Blank => panic!("the stack drew nothing"),
        ClusterRaster::Uncovered => {},
    }
}

#[test]
fn a_cluster_no_face_carries_is_left_to_the_cells() {
    let mut fonts = fonts();
    assert!(matches!(
        fonts.rasterize_cluster(FaceStyle::Regular, "\u{10fffd}\u{301}", 1),
        ClusterRaster::Uncovered
    ));
}

#[test]
fn an_unjoined_sequence_is_refused_rather_than_squeezed() {
    let mut fonts = fonts();
    let three = "\u{1f600}\u{1f600}\u{1f600}";
    assert!(matches!(
        fonts.rasterize_cluster(FaceStyle::Regular, three, 1),
        ClusterRaster::Uncovered
    ));
}

#[test]
fn conjoining_jamo_compose_into_their_syllable() {
    assert_eq!(compose_hangul("\u{1100}\u{1161}\u{11a8}"), Some('\u{ac01}'));
    assert_eq!(compose_hangul("\u{1100}\u{1161}"), Some('\u{ac00}'));
    assert_eq!(compose_hangul("\u{1112}\u{1175}\u{11c2}"), Some('\u{d7a3}'));
    assert_eq!(compose_hangul("\u{1100}"), None);
    assert_eq!(compose_hangul("\u{1100}\u{1161}\u{11a8}\u{11a8}"), None);
    assert_eq!(compose_hangul("e\u{301}"), None);
    assert_eq!(compose_hangul("\u{ac01}"), None);
}

#[test]
fn a_hangul_syllable_spelled_in_pieces_is_drawn_like_the_composed_syllable() {
    let mut fonts = fonts();
    let ClusterRaster::Sprite(pieces) =
        fonts.rasterize_cluster(FaceStyle::Regular, "\u{1100}\u{1161}\u{11a8}", 2)
    else {
        panic!("the jamo did not draw");
    };
    let ClusterRaster::Sprite(composed) =
        fonts.rasterize_cluster(FaceStyle::Regular, "\u{ac01}", 2)
    else {
        panic!("the composed syllable did not draw");
    };
    assert_eq!(
        (pieces.left, pieces.top, pieces.width, pieces.height),
        (composed.left, composed.top, composed.width, composed.height)
    );
    assert!(pieces.coverage == composed.coverage);
    assert!(pieces.width <= fonts.metrics().width * 2);
}

#[test]
fn a_text_presentation_selector_keeps_an_emoji_out_of_colour() {
    let mut fonts = fonts();
    match fonts.rasterize_cluster(FaceStyle::Regular, "\u{2614}\u{fe0e}", 1) {
        ClusterRaster::Sprite(bitmap) => {
            assert_eq!(bitmap.content, GlyphContent::Mask);
            assert_eq!(
                bitmap.coverage.len(),
                (bitmap.width * bitmap.height) as usize
            );
        },
        _ => panic!("the umbrella with VS15 did not draw"),
    }
    match fonts.rasterize_cluster(FaceStyle::Regular, "\u{2614}\u{fe0f}", 2) {
        ClusterRaster::Sprite(bitmap) => assert_eq!(bitmap.content, GlyphContent::Color),
        _ => panic!("the umbrella with VS16 did not draw"),
    }
    let primary = fonts.primary_of(FaceStyle::Regular);
    assert_eq!(
        fonts.cluster_font(FaceStyle::Regular, "\u{231a}\u{fe0e}"),
        Some(primary),
        "a text-style watch comes from the text font"
    );
}

#[test]
fn a_text_presentation_selector_prefers_a_monochrome_system_font() {
    let installed = std::path::Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf");
    if !installed.exists() {
        return;
    }
    let directory =
        std::env::temp_dir().join(format!("zellij-terminal-vs15-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::copy(installed, directory.join("DejaVuSansMono.ttf")).unwrap();
    let Some(discovery) = Discovery::synthetic(&directory) else {
        return;
    };
    let mut fonts = FontStack::build_with(
        &FontOptions {
            system_fonts: true,
            ..FontOptions::default()
        },
        Some(discovery),
    )
    .unwrap();
    let font = fonts
        .cluster_font(FaceStyle::Regular, "\u{2614}\u{fe0e}")
        .expect("the umbrella found no face");
    assert!(
        !fonts.has_color(font),
        "a colour face won the text-style umbrella"
    );
    assert!(
        font.0 > SYMBOLS_FONT.0,
        "the umbrella came from a bundled face"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_text_presentation_selector_draws_a_silhouette_when_only_colour_faces_have_it() {
    let Some(discovery) = synthetic_discovery() else {
        return;
    };
    let mut fonts = FontStack::build_with(
        &FontOptions {
            system_fonts: true,
            ..FontOptions::default()
        },
        Some(discovery),
    )
    .unwrap();
    let font = fonts
        .cluster_font(FaceStyle::Regular, "\u{2614}\u{fe0e}")
        .expect("the umbrella found no face");
    assert!(
        fonts.has_color(font),
        "only colour faces carry the umbrella in the bundled set, so it is drawn as a silhouette"
    );
    assert!(matches!(
        fonts.rasterize_cluster(FaceStyle::Regular, "\u{2614}\u{fe0e}", 1),
        ClusterRaster::Sprite(GlyphBitmap {
            content: GlyphContent::Mask,
            ..
        })
    ));
}

#[test]
fn stacked_marks_are_placed_one_above_the_other() {
    let mut fonts = fonts();
    let font = fonts.primary_of(FaceStyle::Regular);
    let one = fonts.shape_cluster(font, "x\u{301}");
    let two = fonts.shape_cluster(font, "x\u{301}\u{302}");
    assert_eq!(two.len(), 3);
    assert!(
        two[2].dy > one[1].dy,
        "the second mark sits on the first: {:?}",
        two
    );
    let ClusterRaster::Sprite(single) = fonts.rasterize_cluster(FaceStyle::Regular, "x\u{301}", 1)
    else {
        panic!("a with one mark did not draw");
    };
    let ClusterRaster::Sprite(stacked) =
        fonts.rasterize_cluster(FaceStyle::Regular, "x\u{301}\u{302}", 1)
    else {
        panic!("a with two marks did not draw");
    };
    assert!(stacked.height > single.height);
}

#[cfg(unix)]
#[test]
fn a_font_file_this_user_can_write_is_read_rather_than_mapped() {
    let file = tempfile::NamedTempFile::new().unwrap();
    assert!(!super::files::safe_to_map(file.path()));
    let mut permissions = std::fs::metadata(file.path()).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(file.path(), permissions).unwrap();
    let running_as_root = unsafe { libc::geteuid() } == 0;
    assert_eq!(super::files::safe_to_map(file.path()), !running_as_root);
}

#[test]
fn a_font_file_is_loaded_once_per_process() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut file, REGULAR).unwrap();
    let first = font_file(file.path()).unwrap();
    let second = font_file(file.path()).unwrap();
    assert_eq!(first.as_ptr(), second.as_ptr());
    assert_eq!(first, REGULAR);
}

#[test]
fn an_installed_colour_emoji_font_is_preferred_over_the_bundled_one() {
    let Some(discovery) = synthetic_discovery() else {
        return;
    };
    let mut fonts = FontStack::build_with(
        &FontOptions {
            system_fonts: true,
            ..FontOptions::default()
        },
        Some(discovery),
    )
    .unwrap();
    let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{1f600}');
    assert_ne!(glyph.font, EMOJI_FONT, "the bundled emoji font won");
    assert!(fonts.has_color(glyph.font));
    let family = fonts
        .cluster_font(FaceStyle::Regular, FAMILY)
        .expect("the family found no face");
    assert_eq!(family, glyph.font);
    assert!(matches!(
        fonts.rasterize_cluster(FaceStyle::Regular, FAMILY, 2),
        ClusterRaster::Sprite(GlyphBitmap {
            content: GlyphContent::Color,
            ..
        })
    ));
}

#[test]
fn without_system_fonts_the_bundled_emoji_font_draws_emoji() {
    let mut fonts = fonts();
    assert_eq!(
        glyph_of(&mut fonts, FaceStyle::Regular, '\u{1f600}').font,
        EMOJI_FONT
    );
    assert_eq!(
        fonts.cluster_font(FaceStyle::Regular, FAMILY),
        Some(EMOJI_FONT)
    );
}

#[test]
fn a_cluster_shape_is_cached_like_a_run() {
    let mut fonts = fonts();
    let font = fonts.cluster_font(FaceStyle::Regular, "e\u{301}").unwrap();
    let first = fonts.shape_cluster(font, "e\u{301}");
    let count = fonts.shaped_clusters();
    let second = fonts.shape_cluster(font, "e\u{301}");
    assert_eq!(first, second);
    assert_eq!(fonts.shaped_clusters(), count);
}

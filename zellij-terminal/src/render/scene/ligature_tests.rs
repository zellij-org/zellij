use super::test_helpers::*;
use super::tests::{cache, composing, composing_state, entry_of};
use super::*;
use crate::render::atlas::ClusterLookup;
use crate::render::atlas::GlyphKey;
use crate::render::atlas::Lookup;
use crate::render::scene::runs::clipped;
use crate::testing::painter::Painter;
use crate::text::font::ShapedGlyph;
use crate::text::font::{FontOptions, FontStack, DEFAULT_FONT_SIZE};
use zellij_utils::structured_render::ATTR_STRIKE;
use zellij_utils::structured_render::{PaneRect, PANE_FRAMED, PANE_SELECTABLE};

fn stack(ligatures: bool) -> FontStack {
    FontStack::build(&FontOptions {
        family: None,
        size: DEFAULT_FONT_SIZE,
        system_fonts: false,
        ligatures,
        ..crate::text::font::FontOptions::default()
    })
    .unwrap()
}

fn cache_with(ligatures: bool) -> GlyphCache {
    GlyphCache::new(stack(ligatures))
}

fn scene_through(
    cache: &mut GlyphCache,
    rows: usize,
    cols: usize,
    paint: impl FnOnce(&mut Painter),
) -> Scene {
    let state = Painter::state(rows, cols, paint);
    build_at(
        &state,
        cache,
        BlinkPhase::On,
        &Paints::default(),
        CursorOptions::default(),
        None,
        None,
    )
}

fn scene_with(
    ligatures: bool,
    rows: usize,
    cols: usize,
    paint: impl FnOnce(&mut Painter),
) -> Scene {
    scene_through(&mut cache_with(ligatures), rows, cols, paint)
}

fn nominal_in(cache: &mut GlyphCache, character: char) -> AtlasEntry {
    match cache.glyph(FaceStyle::Regular, character, 1) {
        Lookup::Rendered(rendered) => rendered.entry,
        other => panic!("{:?} did not rasterize: {:?}", character, other),
    }
}

fn draws(scene: &Scene, entry: AtlasEntry) -> bool {
    scene.glyphs.iter().any(|quad| quad.entry == entry)
}

fn columns(scene: &Scene) -> Vec<i32> {
    let mut columns: Vec<i32> = scene.glyphs.iter().map(|quad| quad.x).collect();
    columns.sort_unstable();
    columns
}

fn run(start: usize, end: usize, text: &str, font: FontId) -> Run {
    Run {
        start,
        end,
        text: text.to_owned(),
        font,
    }
}

fn uniform_cells(count: usize, character: char) -> Vec<RowCell> {
    let cell = WireCell {
        ch: character as u32,
        ..WireCell::BLANK
    };
    (0..count)
        .map(|_| RowCell {
            cell,
            occupancy: Occupancy::Single,
            paint: paint_of(cell, Occupancy::Single, &Paints::default(), None),
            explicit_background: false,
            on_cursor: false,
            inverted_by_cursor: false,
            composing: false,
            held: false,
            cluster: false,
            continuation: false,
            complex: false,
            unit_end: 0,
        })
        .collect()
}

fn empty_scene(_cells: usize, _metrics: CellMetrics) -> RowScene {
    RowScene::default()
}

#[test]
fn an_arrow_becomes_ligature_pieces_over_the_same_two_cells() {
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 4, |painter| painter.text(0, 0, "a->b"));
    let dash = nominal_in(&mut cache, '-');
    let greater = nominal_in(&mut cache, '>');
    let a = nominal_in(&mut cache, 'a');

    let unshaped = scene_with(false, 1, 4, |painter| painter.text(0, 0, "a->b"));
    assert_eq!(
        columns(&scene),
        columns(&unshaped),
        "a ligature moved a glyph off its cell"
    );
    assert_eq!(scene.glyphs.len(), 4);
    assert!(draws(&scene, a), "the letters outside the sequence moved");
    assert!(!draws(&scene, dash), "the hyphen was drawn nominally");
    assert!(
        !draws(&scene, greater),
        "the greater-than was drawn nominally"
    );
}

#[test]
fn with_the_option_off_the_same_text_draws_the_nominal_glyphs() {
    let mut cache = cache_with(false);
    let scene = scene_through(&mut cache, 1, 4, |painter| painter.text(0, 0, "a->b"));
    let dash = nominal_in(&mut cache, '-');
    let greater = nominal_in(&mut cache, '>');

    assert_eq!(scene.glyphs.len(), 4);
    assert!(draws(&scene, dash));
    assert!(draws(&scene, greater));
}

#[test]
fn a_run_without_a_ligating_sequence_renders_exactly_as_it_did_unshaped() {
    for text in ["hello", "&&", "++", "abc123", "x_y", "www"] {
        let plain = scene_with(false, 1, 8, |painter| painter.text(0, 0, text));
        let ligated = scene_with(true, 1, 8, |painter| painter.text(0, 0, text));
        assert_eq!(plain, ligated, "{:?} moved with ligatures on", text);
    }
}

#[test]
fn a_colour_change_inside_a_sequence_keeps_the_ligature_and_paints_each_cell_its_own_colour() {
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.styled(0, 0, "=", |cell| {
            cell.fg = WireColor::Named(1).pack();
        });
        painter.styled(0, 1, ">", |cell| {
            cell.fg = WireColor::Named(6).pack();
        });
    });
    let equals = nominal_in(&mut cache, '=');
    let paints = Paints::default();

    assert!(
        !draws(&scene, equals),
        "a colour change broke the ligature: {:?}",
        scene.glyphs
    );
    assert!(
        scene.glyphs.iter().any(|quad| quad.color == paints.ansi[1]),
        "no piece was painted in the first cell's colour"
    );
    assert!(
        scene.glyphs.iter().any(|quad| quad.color == paints.ansi[6]),
        "no piece was painted in the second cell's colour"
    );
    for quad in &scene.glyphs {
        let cell = if quad.color == paints.ansi[1] { 0 } else { 1 };
        let (left, right) = (cell * 8, (cell + 1) * 8);
        assert!(
            quad.x >= left && quad.x + quad.entry.width as i32 <= right,
            "a piece painted in one cell's colour drew outside that cell: {:?}",
            quad
        );
    }
}

#[test]
fn a_selection_over_half_a_ligature_leaves_the_glyph_whole() {
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.styled(0, 0, "=", |cell| {
            cell.bg = WireColor::Named(4).pack();
        });
        painter.text(0, 1, ">");
    });
    let equals = nominal_in(&mut cache, '=');
    assert!(
        !draws(&scene, equals),
        "a background change broke the ligature"
    );
    assert!(
        scene
            .rects
            .iter()
            .any(|rect| rect.color == Paints::default().ansi[4] && rect.width == 8),
        "the selected cell lost its background"
    );
}

#[test]
fn a_bold_change_inside_a_sequence_breaks_the_run() {
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.text(0, 0, "=");
        painter.styled(0, 1, ">", |cell| {
            cell.attrs |= ATTR_BOLD;
        });
    });
    let equals = nominal_in(&mut cache, '=');
    assert!(
        draws(&scene, equals),
        "the run ligated across a face change"
    );
}

#[test]
fn an_underline_or_strike_change_does_not_break_the_run() {
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.text(0, 0, "=");
        painter.styled(0, 1, ">", |cell| {
            cell.attrs |= ATTR_STRIKE;
        });
    });
    let equals = nominal_in(&mut cache, '=');
    let stroke = cache.metrics().stroke;
    assert!(
        !draws(&scene, equals),
        "a strike on one cell broke the ligature"
    );
    assert!(
        scene
            .rects
            .iter()
            .any(|rect| rect.height == stroke && rect.x == 8),
        "the struck cell lost its line"
    );
}

#[test]
fn a_link_change_does_not_break_the_run() {
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.links(&[(1, "https://example.com")]);
        painter.text(0, 0, "=");
        painter.linked(0, 1, ">", 1);
    });
    let equals = nominal_in(&mut cache, '=');
    assert!(!draws(&scene, equals), "a link boundary broke the ligature");
}

fn pane(x: u16, cols: u16, left: u8, flags: u8) -> PaneRect {
    PaneRect {
        x,
        y: 0,
        cols,
        rows: 1,
        top: 0,
        bottom: 0,
        left,
        right: 0,
        flags,
    }
}

#[test]
fn a_pane_boundary_breaks_the_run() {
    let geometry = GeometryRecord {
        panes: vec![
            pane(0, 1, 0, PANE_SELECTABLE),
            pane(1, 3, 0, PANE_SELECTABLE),
        ],
    };
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.geometry(&geometry);
        painter.text(0, 0, "->");
    });
    let dash = nominal_in(&mut cache, '-');
    assert!(
        draws(&scene, dash),
        "the run ligated across a pane boundary"
    );
}

#[test]
fn a_run_inside_one_pane_still_ligates_when_a_geometry_record_is_present() {
    let geometry = GeometryRecord {
        panes: vec![pane(0, 4, 0, PANE_SELECTABLE)],
    };
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.geometry(&geometry);
        painter.text(0, 0, "->");
    });
    let dash = nominal_in(&mut cache, '-');
    assert!(!draws(&scene, dash), "a run inside one pane did not ligate");
}

#[test]
fn a_frame_cell_is_not_part_of_any_run() {
    let geometry = GeometryRecord {
        panes: vec![pane(0, 4, 1, PANE_FRAMED | PANE_SELECTABLE)],
    };
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.geometry(&geometry);
        painter.text(0, 0, "->");
    });
    let dash = nominal_in(&mut cache, '-');
    assert!(draws(&scene, dash), "a frame cell joined a run");
}

#[test]
fn a_wide_spacer_breaks_the_run() {
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 6, |painter| {
        painter.text(0, 0, "-");
        painter.wide(0, 1, '\u{4f60}');
        painter.text(0, 3, ">");
    });
    let dash = nominal_in(&mut cache, '-');
    let greater = nominal_in(&mut cache, '>');
    assert!(draws(&scene, dash) && draws(&scene, greater));
}

#[test]
fn the_cursor_cell_breaks_the_run_and_the_ligature_reforms_when_it_leaves() {
    let mut cache = cache_with(true);
    let under_cursor = scene_through(&mut cache, 1, 4, |painter| {
        painter.text(0, 0, "->");
        painter.cursor(0, 1, CursorShape::Beam);
    });
    let dash = nominal_in(&mut cache, '-');
    assert!(
        draws(&under_cursor, dash),
        "the ligature survived the cursor sitting in it"
    );

    let cursor_away = scene_through(&mut cache, 1, 4, |painter| {
        painter.text(0, 0, "->");
        painter.cursor(0, 3, CursorShape::Beam);
    });
    assert!(
        !draws(&cursor_away, dash),
        "the ligature did not reform once the cursor left"
    );
}

#[test]
fn an_invisible_cursor_leaves_the_ligature_alone() {
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.text(0, 0, "->");
    });
    let dash = nominal_in(&mut cache, '-');
    assert!(!draws(&scene, dash));
}

#[test]
fn a_fallback_face_is_never_shaped_with_the_primary() {
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 6, |painter| {
        painter.text(0, 0, "->");
        painter.wide(0, 2, '\u{6f22}');
    });
    let dash = nominal_in(&mut cache, '-');
    let han = match cache.glyph(FaceStyle::Regular, '\u{6f22}', 2) {
        Lookup::Rendered(rendered) => rendered.entry,
        other => panic!("the fallback glyph did not rasterize: {:?}", other),
    };
    assert!(!draws(&scene, dash), "the ASCII run did not ligate");
    assert!(draws(&scene, han), "the fallback glyph was not drawn");
}

#[test]
fn a_run_of_arabic_is_left_to_the_unshaped_path() {
    let plain = scene_with(false, 1, 6, |painter| {
        painter.text(0, 0, "\u{645}\u{631}\u{62d}\u{628}\u{627}")
    });
    let ligated = scene_with(true, 1, 6, |painter| {
        painter.text(0, 0, "\u{645}\u{631}\u{62d}\u{628}\u{627}")
    });
    assert_eq!(plain, ligated);
}

#[test]
fn a_run_of_cyrillic_is_left_to_the_unshaped_path() {
    let plain = scene_with(false, 1, 6, |painter| painter.text(0, 0, "\u{434}\u{430}"));
    let ligated = scene_with(true, 1, 6, |painter| painter.text(0, 0, "\u{434}\u{430}"));
    assert_eq!(plain, ligated);
}

#[test]
fn a_three_cell_cluster_draws_one_glyph_over_three_cells() {
    let mut cache = cache_with(true);
    let font = cache.primary_of(FaceStyle::Regular);
    let metrics = cache.metrics();
    let nominal = nominal_in(&mut cache, 'M');
    let id = stack(true).lookup(FaceStyle::Regular, 'M').unwrap().glyph;

    let mut scene = empty_scene(4, metrics);
    let mut shaped = vec![false; 4];
    push_run(
        &mut scene,
        &mut cache,
        ShapedRun {
            run: &run(1, 3, "===", font),
            glyphs: &[ShapedGlyph {
                cell: 0,
                span: 3,
                glyph: id,
                dx: 0,
                dy: 0,
                advance: 0,
            }],
        },
        &uniform_cells(4, '='),
        0,
        metrics,
        &mut shaped,
    );

    assert_eq!(scene.glyphs.len(), 1, "{:?}", scene.glyphs);
    assert_eq!(
        scene.glyphs[0].x,
        metrics.width as i32 + nominal.left,
        "the cluster was not placed at its first cell"
    );
    assert_eq!(scene.glyphs[0].entry, nominal);
    assert_eq!(shaped, vec![false, true, true, true]);
}

#[test]
fn a_cluster_mapping_that_does_not_tile_the_run_is_refused() {
    let mut cache = cache_with(true);
    let font = cache.primary_of(FaceStyle::Regular);
    let metrics = cache.metrics();
    let id = stack(true).lookup(FaceStyle::Regular, 'M').unwrap().glyph;
    let mut scene = empty_scene(3, metrics);
    let mut shaped = vec![false; 3];
    push_run(
        &mut scene,
        &mut cache,
        ShapedRun {
            run: &run(0, 2, "===", font),
            glyphs: &[ShapedGlyph {
                cell: 0,
                span: 2,
                glyph: id,
                dx: 0,
                dy: 0,
                advance: 0,
            }],
        },
        &uniform_cells(3, '='),
        0,
        metrics,
        &mut shaped,
    );
    assert!(scene.glyphs.is_empty());
    assert_eq!(shaped, vec![false, false, false]);
}

#[test]
fn a_glyph_wider_than_its_run_is_clipped_to_it() {
    let wide = AtlasEntry {
        x: 100,
        y: 0,
        width: 40,
        height: 10,
        left: -4,
        top: 8,
    };
    assert_eq!(
        clipped(wide, 0, 0, 16),
        Some((0, AtlasEntry { width: 16, ..wide }))
    );
    assert_eq!(
        clipped(wide, -4, 0, 16),
        Some((
            0,
            AtlasEntry {
                x: 104,
                width: 16,
                ..wide
            }
        ))
    );
    assert_eq!(
        clipped(wide, 8, 0, 16),
        Some((8, AtlasEntry { width: 8, ..wide }))
    );
    assert_eq!(clipped(wide, 20, 0, 16), None);
    assert_eq!(clipped(wide, -60, 0, 16), None);
}

#[test]
fn a_glyph_inside_its_run_is_not_clipped() {
    let narrow = AtlasEntry {
        x: 4,
        y: 4,
        width: 6,
        height: 10,
        left: 1,
        top: 8,
    };
    assert_eq!(clipped(narrow, 9, 8, 24), Some((9, narrow)));
}

#[test]
fn the_shape_cache_answers_the_second_time_without_reshaping() {
    let mut fonts = stack(true);
    let font = fonts.primary_of(FaceStyle::Regular);
    assert_eq!(fonts.shaped_runs(), 0);
    let first = fonts.shape(font, "->");
    assert_eq!(fonts.shaped_runs(), 1);
    let second = fonts.shape(font, "->");
    assert_eq!(fonts.shaped_runs(), 1);
    assert_eq!(first, second);
}

#[test]
fn a_font_rebuild_starts_the_shape_cache_empty() {
    let mut fonts = stack(true);
    let font = fonts.primary_of(FaceStyle::Regular);
    fonts.shape(font, "->");
    assert_eq!(fonts.shaped_runs(), 1);
    let rebuilt = stack(true);
    assert_eq!(
        rebuilt.shaped_runs(),
        0,
        "a rebuilt stack inherited shaped runs"
    );
}

#[test]
fn the_toggle_decides_whether_runs_are_shaped_at_all() {
    assert!(cache_with(true).shapes_runs());
    assert!(!cache_with(false).shapes_runs());
}

#[test]
fn the_differential_lane_sees_two_cells_where_the_scene_draws_a_ligature() {
    let state = Painter::state(1, 4, |painter| painter.text(0, 0, "a->b"));
    let mut cache = cache_with(true);
    let scene = build_at(
        &state,
        &mut cache,
        BlinkPhase::On,
        &Paints::default(),
        CursorOptions::default(),
        None,
        None,
    );
    let dash = nominal_in(&mut cache, '-');
    assert!(!draws(&scene, dash), "the sequence did not ligate");

    let projected = crate::testing::equivalence::project_screen_buffer(state.screen());
    assert_eq!(projected.cells[0][1].text, "-");
    assert_eq!(projected.cells[0][2].text, ">");
    assert_eq!(
        projected.cells[0].len(),
        4,
        "shaping changed what the lane counts"
    );
}

#[test]
fn a_hidden_run_draws_its_pieces_in_the_background_colour() {
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.styled(0, 0, "->", |cell| {
            cell.attrs |= ATTR_HIDDEN;
        });
    });
    assert!(scene
        .glyphs
        .iter()
        .all(|quad| quad.color == Paints::default().background));
}

const MAN: &str = "\u{1f468}\u{200d}";
const WOMAN: &str = "\u{1f469}\u{200d}";
const GIRL: &str = "\u{1f467}";

fn within_columns(quad: &GlyphQuad, first: usize, last: usize, metrics: CellMetrics) -> bool {
    let left = (first as u32 * metrics.width) as i32;
    let right = ((last + 1) as u32 * metrics.width) as i32;
    quad.x >= left && quad.x + quad.entry.width as i32 <= right
}

#[test]
fn a_single_cell_cluster_draws_one_glyph_inside_its_cell() {
    let mut cache = cache();
    let metrics = cache.metrics();
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.cluster(0, 0, "e\u{301}", 1);
    });
    assert_eq!(scene.glyphs.len(), 1, "{:?}", scene.glyphs);
    assert!(scene.color_glyphs.is_empty());
    assert!(within_columns(&scene.glyphs[0], 0, 0, metrics));
    assert!(cache.cluster_entries() >= 1);
}

#[test]
fn a_combining_mark_is_drawn_over_its_base_rather_than_beside_it() {
    let mut cache = cache();
    let metrics = cache.metrics();
    let plain = entry_of(&mut cache, FaceStyle::Regular, 'e');
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.cluster(0, 0, "e\u{301}", 1);
    });
    let quad = scene.glyphs[0];
    assert_ne!(quad.entry, plain, "the mark was dropped");
    assert!(
        quad.entry.height > plain.height,
        "the accent does not sit above the letter: {:?} vs {:?}",
        quad.entry,
        plain
    );
    assert!(within_columns(&quad, 0, 0, metrics));
}

fn assert_drawn_joined(members: &[(&str, usize)]) {
    let mut cache = cache();
    let metrics = cache.metrics();
    let columns: usize = members.iter().map(|(_, width)| width).sum();
    let scene = scene_through(&mut cache, 1, columns + 2, |painter| {
        painter.group(0, 0, members);
    });
    let joined: String = members.iter().map(|(text, _)| *text).collect();
    let parts: Vec<&str> = members.iter().map(|(text, _)| *text).collect();
    assert_eq!(
        scene.color_glyphs.len(),
        1,
        "{:?} was not drawn as one colour glyph: {:?}",
        parts,
        scene.color_glyphs
    );
    assert!(
        scene.glyphs.is_empty(),
        "{:?} drew pieces: {:?}",
        parts,
        scene.glyphs
    );
    assert!(
        scene.rects.is_empty(),
        "{:?} drew boxes: {:?}",
        parts,
        scene.rects
    );
    assert!(
        within_columns(&scene.color_glyphs[0], 0, columns - 1, metrics),
        "{:?} spilled out of its {} columns: {:?}",
        parts,
        columns,
        scene.color_glyphs[0]
    );
    assert!(matches!(
        cache.cluster_glyph(FaceStyle::Regular, &GlyphKey::of(&joined), 2),
        ClusterLookup::Sprite(_)
    ));
}

#[test]
fn a_family_group_is_drawn_as_one_joined_glyph() {
    assert_drawn_joined(&[(MAN, 2), (WOMAN, 2), (GIRL, 2)]);
}

#[test]
fn a_skin_tone_group_is_drawn_as_one_joined_glyph() {
    assert_drawn_joined(&[("\u{1f44d}", 2), ("\u{1f3fd}", 2)]);
}

#[test]
fn a_flag_group_is_drawn_as_one_joined_glyph() {
    assert_drawn_joined(&[("\u{1f1e9}", 1), ("\u{1f1ea}", 1)]);
}

#[test]
fn a_rainbow_flag_group_is_drawn_as_one_joined_glyph() {
    assert_drawn_joined(&[("\u{1f3f3}\u{fe0f}\u{200d}", 1), ("\u{1f308}", 2)]);
}

#[test]
fn an_england_tag_flag_is_drawn_as_one_joined_glyph() {
    assert_drawn_joined(&[(
        "\u{1f3f4}\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}",
        2,
    )]);
}

#[test]
fn a_mode_2027_keycap_is_drawn_as_one_joined_glyph() {
    assert_drawn_joined(&[("1\u{fe0f}\u{20e3}", 2)]);
}

#[test]
fn a_mode_2027_cluster_cell_draws_one_joined_glyph_over_its_columns() {
    let mut cache = cache();
    let metrics = cache.metrics();
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.cluster(0, 0, "\u{26a0}\u{fe0f}", 2);
    });
    assert_eq!(scene.color_glyphs.len(), 1, "{:?}", scene.color_glyphs);
    assert!(scene.glyphs.is_empty(), "{:?}", scene.glyphs);
    assert!(within_columns(&scene.color_glyphs[0], 0, 1, metrics));

    let family = format!("{}{}{}", MAN, WOMAN, GIRL);
    let joined = scene_through(&mut cache, 1, 4, |painter| {
        painter.cluster(0, 0, &family, 2);
    });
    assert_eq!(joined.color_glyphs.len(), 1, "{:?}", joined.color_glyphs);
    assert!(joined.glyphs.is_empty(), "{:?}", joined.glyphs);
    assert!(joined.rects.is_empty(), "{:?}", joined.rects);
    assert!(within_columns(&joined.color_glyphs[0], 0, 1, metrics));
}

#[test]
fn a_cluster_no_font_covers_falls_back_to_drawing_each_cell() {
    let mut cache = cache();
    let metrics = cache.metrics();
    let a = entry_of(&mut cache, FaceStyle::Regular, 'a');
    let scene = scene_through(&mut cache, 1, 4, |painter| {
        painter.group(0, 0, &[("a", 1), ("\u{10fffd}", 1)]);
    });
    assert!(
        scene
            .glyphs
            .iter()
            .any(|quad| quad.entry == a && within_columns(quad, 0, 0, metrics)),
        "the covered cell was not drawn on its own: {:?}",
        scene.glyphs
    );
    assert!(
        scene
            .rects
            .iter()
            .any(|rect| rect.x >= metrics.width as i32 && rect.x < 2 * metrics.width as i32),
        "the uncovered cell did not fall back to its tofu box: {:?}",
        scene.rects
    );
}

#[test]
fn an_ascii_ligature_next_to_a_cluster_still_ligates() {
    let mut cache = cache_with(true);
    let scene = scene_through(&mut cache, 1, 6, |painter| {
        painter.text(0, 0, "->");
        painter.cluster(0, 3, "e\u{301}", 1);
    });
    let dash = nominal_in(&mut cache, '-');
    assert!(
        !draws(&scene, dash),
        "the run beside the cluster stopped ligating"
    );
    assert_eq!(cache.shaped_runs(), 1);
}

#[test]
fn a_cluster_is_never_part_of_an_ascii_run() {
    let mut cache = cache_with(true);
    scene_through(&mut cache, 1, 6, |painter| {
        painter.text(0, 0, "-");
        painter.group(0, 1, &[(">", 1), ("\u{903}", 1)]);
    });
    assert_eq!(
        cache.shaped_runs(),
        0,
        "a group head was shaped into a run with its neighbour"
    );
}

#[test]
fn overwriting_the_middle_of_a_group_leaves_no_trace_of_the_joined_glyph() {
    let mut cache = cache();
    let metrics = cache.metrics();
    let mut state = Painter::state(1, 8, |painter| {
        painter.group(0, 0, &[(MAN, 2), (WOMAN, 2), (GIRL, 2)]);
    });
    Painter::apply(&mut state, |painter| {
        painter.cluster(0, 0, MAN, 2);
        painter.text(0, 2, "x ");
        painter.wide(0, 4, '\u{1f467}');
    });
    let overwritten = build(&state, &mut cache);
    let fresh_state = Painter::state(1, 8, |painter| {
        painter.cluster(0, 0, MAN, 2);
        painter.text(0, 2, "x ");
        painter.wide(0, 4, '\u{1f467}');
    });
    let fresh = build(&fresh_state, &mut cache);
    assert_eq!(overwritten, fresh);
    for quad in overwritten
        .glyphs
        .iter()
        .chain(overwritten.color_glyphs.iter())
    {
        assert!(
            within_columns(quad, 0, 1, metrics)
                || within_columns(quad, 2, 2, metrics)
                || within_columns(quad, 4, 5, metrics),
            "{:?} crosses a cell of the overwritten row",
            quad
        );
    }
    let x = entry_of(&mut cache, FaceStyle::Regular, 'x');
    assert!(overwritten.glyphs.iter().any(|quad| quad.entry == x));
}

#[test]
fn a_cluster_is_painted_in_its_own_cell_colour() {
    let mut cache = cache();
    let state = Painter::state(1, 8, |painter| {
        painter.cluster_styled(0, 0, "e\u{301}", 1, |cell| {
            cell.fg = WireColor::Named(1).pack();
        });
    });
    let scene = build(&state, &mut cache);
    assert_eq!(scene.glyphs.len(), 1);
    assert_eq!(scene.glyphs[0].color, Paints::default().ansi[1]);
}

#[test]
fn a_composed_cluster_is_drawn_through_the_unit_path() {
    let state = composing_state();
    let mut cache = cache();
    let metrics = cache.metrics();
    let preedit = Preedit::new("e\u{301}", Some((3, 3)));
    let scene = composing(&state, &mut cache, Some(&preedit));
    let in_composition: Vec<&GlyphQuad> = scene
        .glyphs
        .iter()
        .filter(|quad| quad.y < metrics.height as i32 && within_columns(quad, 2, 2, metrics))
        .collect();
    assert_eq!(in_composition.len(), 1, "{:?}", scene.glyphs);
    assert!(cache.cluster_entries() >= 1);

    let warning = "\u{26a0}\u{fe0f}";
    let end = warning.len();
    let preedit = Preedit::new(warning, Some((end, end)));
    let wide_state = Painter::state(1, 10, |painter| {
        painter.cursor(0, 0, CursorShape::Block);
    });
    let scene = composing(&wide_state, &mut cache, Some(&preedit));
    assert!(
        scene.color_glyphs.is_empty(),
        "a one-column composition must show the text style: {:?}",
        scene.color_glyphs
    );
    let sign = entry_of(&mut cache, FaceStyle::Regular, '\u{26a0}');
    assert!(scene
        .glyphs
        .iter()
        .any(|quad| quad.entry == sign && within_columns(quad, 0, 0, metrics)));
}

#[test]
fn a_width_three_cell_is_followed_by_two_blank_columns() {
    let mut cache = cache();
    let scene = scene_through(&mut cache, 1, 6, |painter| {
        painter.cluster(0, 0, "\u{17d8}", 3);
        painter.text(0, 3, "z");
    });
    let z = entry_of(&mut cache, FaceStyle::Regular, 'z');
    assert!(scene
        .glyphs
        .iter()
        .any(|quad| quad.entry == z && quad.x >= 3 * cache.metrics().width as i32));
}

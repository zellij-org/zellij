use super::images::clip_to_owning_pane;
use super::test_helpers::*;
use super::*;
use crate::emulation::color::{DEFAULT_BACKGROUND, DEFAULT_FOREGROUND};
use crate::render::atlas::Lookup;
use crate::testing::painter::{self, Painter, Place};
use crate::text::font::{FontOptions, FontStack, DEFAULT_FONT_SIZE};
use zellij_utils::structured_render::ATTR_STRIKE;
use zellij_utils::structured_render::UNDERLINE_CURLY;
use zellij_utils::structured_render::UNDERLINE_DASHED;
use zellij_utils::structured_render::UNDERLINE_DOTTED;
use zellij_utils::structured_render::UNDERLINE_DOUBLE;
use zellij_utils::structured_render::{GraphicsRecord, PaneRect, PANE_SELECTABLE};

pub(super) fn cache() -> GlyphCache {
    GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap())
}

fn scene_of(rows: usize, cols: usize, paint: impl FnOnce(&mut Painter)) -> Scene {
    scene_at(rows, cols, paint, BlinkPhase::On)
}

fn scene_at(
    rows: usize,
    cols: usize,
    paint: impl FnOnce(&mut Painter),
    phase: BlinkPhase,
) -> Scene {
    let state = Painter::state(rows, cols, paint);
    build_at(
        &state,
        &mut cache(),
        phase,
        &Paints::default(),
        CursorOptions::default(),
        None,
        None,
    )
}

fn scene_painted_with(
    rows: usize,
    cols: usize,
    paint: impl FnOnce(&mut Painter),
    paints: &Paints,
) -> Scene {
    let state = Painter::state(rows, cols, paint);
    build_at(
        &state,
        &mut cache(),
        BlinkPhase::On,
        paints,
        CursorOptions::default(),
        None,
        None,
    )
}

fn scene_with_cursor(
    rows: usize,
    cols: usize,
    paint: impl FnOnce(&mut Painter),
    cursor: CursorOptions,
    phase: BlinkPhase,
) -> Scene {
    let state = Painter::state(rows, cols, paint);
    build_at(
        &state,
        &mut cache(),
        phase,
        &Paints::default(),
        cursor,
        None,
        None,
    )
}

fn cursor_block(scene: &Scene) -> Option<&Rect> {
    scene
        .rects
        .iter()
        .find(|rect| rect.width == 8 && rect.height == 20)
}

fn hollow(shape: CursorShape, phase: BlinkPhase) -> Scene {
    scene_with_cursor(
        1,
        4,
        |painter| {
            painter.text(0, 0, "A");
            painter.blinking_cursor(0, 0, shape);
        },
        CursorOptions {
            shape: None,
            blink: None,
            hollow: true,
        },
        phase,
    )
}

#[test]
fn an_unfocused_cursor_is_an_outline_that_leaves_the_cell_colours_alone() {
    for shape in [
        CursorShape::Block,
        CursorShape::Beam,
        CursorShape::Underline,
    ] {
        let scene = hollow(shape, BlinkPhase::On);
        assert!(
            cursor_block(&scene).is_none(),
            "{:?}: the cell was filled",
            shape
        );
        let outline: Vec<&Rect> = scene
            .rects
            .iter()
            .filter(|rect| rect.x < 8 && rect.y < 20)
            .collect();
        assert_eq!(outline.len(), 4, "{:?}: {:?}", shape, scene.rects);
        assert!(outline.iter().all(|rect| rect.color == DEFAULT_FOREGROUND));
        assert!(outline
            .iter()
            .any(|rect| (rect.x, rect.y, rect.width, rect.height) == (0, 0, 8, 1)));
        assert!(outline
            .iter()
            .any(|rect| (rect.x, rect.y, rect.width, rect.height) == (0, 19, 8, 1)));
        assert!(outline
            .iter()
            .any(|rect| (rect.x, rect.y, rect.width, rect.height) == (0, 0, 1, 20)));
        assert!(outline
            .iter()
            .any(|rect| (rect.x, rect.y, rect.width, rect.height) == (7, 0, 1, 20)));
        assert_eq!(
            scene.glyphs[0].color, DEFAULT_FOREGROUND,
            "{:?}: the glyph was recoloured",
            shape
        );
    }
}

#[test]
fn an_unfocused_cursor_does_not_blink() {
    let off = hollow(CursorShape::Block, BlinkPhase::Off);
    assert_eq!(
        off.rects.iter().filter(|rect| rect.x < 8).count(),
        4,
        "{:?}",
        off.rects
    );
}

#[test]
fn a_focused_cursor_is_drawn_as_before() {
    let focused = scene_with_cursor(
        1,
        4,
        |painter| {
            painter.text(0, 0, "A");
            painter.cursor(0, 0, CursorShape::Block);
        },
        CursorOptions::default(),
        BlinkPhase::On,
    );
    let block = cursor_block(&focused).expect("a focused block cursor fills the cell");
    assert_eq!(block.color, DEFAULT_FOREGROUND);
    assert_eq!(focused.glyphs[0].color, DEFAULT_BACKGROUND);
}

fn contrasted(minimum: f32, paint: impl FnOnce(&mut Painter)) -> Scene {
    let state = Painter::state(1, 4, paint);
    let mut cache = cache();
    let contrast = Contrast::new(minimum);
    let paints = Paints::default();
    let context = RowContext::new(
        SceneInputs {
            state: &state,
            phase: BlinkPhase::On,
            paints: &paints,
            cursor: CursorOptions::default(),
            hovered_link: None,
            preedit: None,
        },
        &cache,
    )
    .with_contrast(&contrast);
    let mut scratch = RowScratch::default();
    let mut row = RowScene::default();
    build_row(&context, &mut cache, 0, &mut scratch, &mut row);
    Scene {
        width: 0,
        height: 0,
        clear: paints.background,
        rects: row.rects,
        glyphs: row.glyphs,
        color_glyphs: row.color_glyphs,
        images: Vec::new(),
        resident_images: Vec::new(),
    }
}

fn dark_on_dark(painter: &mut Painter) {
    painter.styled(0, 0, "A", |cell| {
        cell.fg = WireColor::Rgb(40, 40, 40).pack();
        cell.bg = WireColor::Rgb(20, 20, 20).pack();
    });
}

#[test]
fn low_contrast_text_is_lifted_to_the_minimum() {
    let scene = contrasted(4.5, dark_on_dark);
    let glyph = scene.glyphs[0].color;
    assert!(
        color::contrast_ratio(glyph, [20, 20, 20]) >= 4.5,
        "{:?}",
        glyph
    );
    assert_eq!(
        scene.rects[0].color,
        [20, 20, 20],
        "the background must not move"
    );
}

#[test]
fn a_minimum_of_one_changes_nothing() {
    let scene = contrasted(1.0, dark_on_dark);
    assert_eq!(scene.glyphs[0].color, [40, 40, 40]);
}

#[test]
fn readable_and_hidden_text_are_left_alone() {
    let readable = contrasted(4.5, |painter| painter.text(0, 0, "A"));
    assert_eq!(readable.glyphs[0].color, DEFAULT_FOREGROUND);
    let hidden = contrasted(4.5, |painter| {
        painter.styled(0, 0, "A", |cell| {
            cell.fg = WireColor::Rgb(40, 40, 40).pack();
            cell.bg = WireColor::Rgb(20, 20, 20).pack();
            cell.attrs |= ATTR_HIDDEN;
        });
    });
    assert!(hidden
        .glyphs
        .iter()
        .all(|glyph| glyph.color == [20, 20, 20]));
}

#[test]
fn an_underline_colour_is_held_to_the_minimum_too() {
    let scene = contrasted(4.5, |painter| {
        painter.styled(0, 0, "A", |cell| {
            cell.attrs |= UNDERLINE_STRAIGHT << zellij_utils::structured_render::UNDERLINE_SHIFT;
            cell.underline_color = WireColor::Rgb(10, 10, 10).pack();
        });
    });
    let metrics = cache().metrics();
    let underline = scene
        .rects
        .iter()
        .find(|rect| rect.y == metrics.underline_top as i32)
        .expect("the underline is drawn");
    assert!(color::contrast_ratio(underline.color, DEFAULT_BACKGROUND) >= 4.5);
}

#[test]
fn text_under_a_block_cursor_is_held_to_the_minimum() {
    let state = Painter::state(1, 4, |painter| {
        painter.text(0, 0, "A");
        painter.cursor(0, 0, CursorShape::Block);
    });
    let mut cache = cache();
    let contrast = Contrast::new(4.5);
    let paints = Paints {
        cursor: Some([30, 30, 30]),
        ..Paints::default()
    };
    let context = RowContext::new(
        SceneInputs {
            state: &state,
            phase: BlinkPhase::On,
            paints: &paints,
            cursor: CursorOptions::default(),
            hovered_link: None,
            preedit: None,
        },
        &cache,
    )
    .with_contrast(&contrast);
    let mut scratch = RowScratch::default();
    let mut row = RowScene::default();
    build_row(&context, &mut cache, 0, &mut scratch, &mut row);
    assert!(color::contrast_ratio(row.glyphs[0].color, [30, 30, 30]) >= 4.5);
}

#[test]
fn a_configured_shape_replaces_the_one_the_session_did_not_ask_for() {
    let scene = scene_with_cursor(
        1,
        4,
        |painter| {
            painter.text(0, 0, "A");
            painter.cursor(0, 0, CursorShape::Default);
        },
        CursorOptions {
            shape: Some(CursorShape::Beam),
            blink: None,
            hollow: false,
        },
        BlinkPhase::On,
    );
    assert!(cursor_block(&scene).is_none(), "{:?}", scene.rects);
    assert!(
        scene
            .rects
            .iter()
            .any(|rect| rect.width == 1 && rect.height == 20),
        "no beam was drawn: {:?}",
        scene.rects
    );
}

#[test]
fn a_shape_the_session_asked_for_is_not_overridden() {
    let scene = scene_with_cursor(
        1,
        4,
        |painter| {
            painter.text(0, 0, "A");
            painter.cursor(0, 0, CursorShape::Block);
        },
        CursorOptions {
            shape: Some(CursorShape::Beam),
            blink: None,
            hollow: false,
        },
        BlinkPhase::On,
    );
    assert!(
        cursor_block(&scene).is_some(),
        "an application asking for a block was overruled: {:?}",
        scene.rects
    );
}

#[test]
fn a_blinking_cursor_is_absent_from_the_off_phase_and_present_in_the_on_phase() {
    let blinking = |phase| {
        scene_with_cursor(
            1,
            4,
            |painter| {
                painter.text(0, 0, "A");
                painter.cursor(0, 0, CursorShape::Block);
            },
            CursorOptions {
                shape: None,
                blink: Some(true),
                hollow: false,
            },
            phase,
        )
    };
    assert!(cursor_block(&blinking(BlinkPhase::On)).is_some());
    assert!(cursor_block(&blinking(BlinkPhase::Off)).is_none());
}

#[test]
fn a_cursor_told_not_to_blink_stays_through_both_phases() {
    let steady = |phase| {
        scene_with_cursor(
            1,
            4,
            |painter| {
                painter.text(0, 0, "A");
                painter.blinking_cursor(0, 0, CursorShape::Block);
            },
            CursorOptions {
                shape: None,
                blink: Some(false),
                hollow: false,
            },
            phase,
        )
    };
    assert!(cursor_block(&steady(BlinkPhase::On)).is_some());
    assert!(
        cursor_block(&steady(BlinkPhase::Off)).is_some(),
        "a cursor the configuration steadied still blinked"
    );
}

#[test]
fn a_cursor_the_session_blinks_blinks_when_nothing_is_configured() {
    let asked = |phase| {
        scene_with_cursor(
            1,
            4,
            |painter| {
                painter.text(0, 0, "A");
                painter.blinking_cursor(0, 0, CursorShape::Block);
            },
            CursorOptions::default(),
            phase,
        )
    };
    assert!(cursor_block(&asked(BlinkPhase::On)).is_some());
    assert!(cursor_block(&asked(BlinkPhase::Off)).is_none());
}

#[test]
fn the_scene_is_the_grid_scaled_by_the_cell_size() {
    let scene = scene_of(4, 10, |_| {});
    assert_eq!((scene.width, scene.height), (80, 80));
    assert_eq!(scene.clear, color::DEFAULT_BACKGROUND);
}

#[test]
fn configured_paints_reach_the_clear_the_glyphs_and_the_table() {
    let paints = Paints {
        foreground: [11, 12, 13],
        background: [21, 22, 23],
        cursor: Some([31, 32, 33]),
        ansi: {
            let mut ansi = color::ANSI_16;
            ansi[1] = [41, 42, 43];
            ansi
        },
    };
    let scene = scene_painted_with(
        1,
        4,
        |painter| {
            painter.text(0, 0, "A");
            painter.styled(0, 1, "B", |cell| cell.fg = WireColor::Named(1).pack());
        },
        &paints,
    );
    assert_eq!(scene.clear, [21, 22, 23]);
    assert_eq!(scene.glyphs[0].color, [11, 12, 13]);
    assert_eq!(scene.glyphs[1].color, [41, 42, 43]);
}

#[test]
fn a_configured_cursor_paints_the_beam() {
    let paints = Paints {
        cursor: Some([31, 32, 33]),
        ..Paints::default()
    };
    let scene = scene_painted_with(
        1,
        4,
        |painter| {
            painter.text(0, 0, "A");
            painter.cursor(0, 0, CursorShape::Beam);
        },
        &paints,
    );
    assert!(
        scene.rects.iter().any(|rect| rect.color == [31, 32, 33]),
        "{:?}",
        scene.rects
    );
}

#[test]
fn a_configured_cursor_paints_the_block_the_default_shape_draws() {
    let paints = Paints {
        cursor: Some([31, 32, 33]),
        ..Paints::default()
    };
    let scene = scene_painted_with(
        1,
        4,
        |painter| {
            painter.text(0, 0, "A");
            painter.cursor(0, 0, CursorShape::Block);
        },
        &paints,
    );
    let block = scene
        .rects
        .iter()
        .find(|rect| rect.width == 8 && rect.height == 20)
        .expect("no cursor block");
    assert_eq!(block.color, [31, 32, 33]);
    assert_eq!(
        scene.glyphs[0].color,
        color::DEFAULT_BACKGROUND,
        "the character under the block keeps the cell's background"
    );
}

#[test]
fn an_unconfigured_block_cursor_still_inverts_the_cell() {
    let scene = scene_of(1, 4, |painter| {
        painter.styled(0, 0, "A", |cell| cell.fg = WireColor::Named(1).pack());
        painter.cursor(0, 0, CursorShape::Block);
    });
    let block = scene
        .rects
        .iter()
        .find(|rect| rect.width == 8 && rect.height == 20)
        .expect("no cursor block");
    assert_eq!(block.color, color::ANSI_16[1]);
    assert_eq!(scene.glyphs[0].color, color::DEFAULT_BACKGROUND);
}

#[test]
fn default_background_cells_emit_no_rectangles() {
    let scene = scene_of(2, 4, |painter| painter.text(0, 0, "ab"));
    assert!(
        scene
            .rects
            .iter()
            .all(|rect| rect.width < 8 || rect.color != color::DEFAULT_BACKGROUND),
        "{:?}",
        scene.rects
    );
}

#[test]
fn a_colored_cell_emits_a_background_rectangle_at_its_origin() {
    let scene = scene_of(2, 4, |painter| {
        painter.styled(0, 0, "X", |cell| cell.bg = WireColor::Named(1).pack());
    });
    let rect = scene
        .rects
        .iter()
        .find(|rect| rect.color == color::ANSI_16[1])
        .expect("no red rectangle");
    assert_eq!((rect.x, rect.y), (0, 0));
    assert_eq!((rect.width, rect.height), (8, 20));
}

#[test]
fn glyphs_are_emitted_for_printable_cells_only() {
    let scene = scene_of(1, 6, |painter| painter.text(0, 0, "a b"));
    assert_eq!(scene.glyphs.len(), 2);
    assert!(scene.glyphs[0].x < scene.glyphs[1].x);
}

#[test]
fn inverse_swaps_foreground_and_background() {
    let plain = scene_of(1, 2, |painter| {
        painter.styled(0, 0, "A", |cell| cell.fg = WireColor::Named(1).pack());
    });
    let inverse = scene_of(1, 2, |painter| {
        painter.styled(0, 0, "A", |cell| {
            cell.fg = WireColor::Named(1).pack();
            cell.attrs |= ATTR_REVERSE;
        });
    });
    assert_eq!(plain.glyphs[0].color, color::ANSI_16[1]);
    assert_eq!(inverse.glyphs[0].color, color::DEFAULT_BACKGROUND);
    assert!(inverse
        .rects
        .iter()
        .any(|rect| rect.color == color::ANSI_16[1]));
}

#[test]
fn dim_darkens_the_foreground() {
    let scene = scene_of(1, 2, |painter| {
        painter.styled(0, 0, "A", |cell| {
            cell.fg = WireColor::Named(1).pack();
            cell.attrs |= ATTR_DIM;
        });
    });
    assert_eq!(scene.glyphs[0].color, color::dim(color::ANSI_16[1]));
}

#[test]
fn hidden_paints_the_glyph_in_the_background_color() {
    let scene = scene_of(1, 2, |painter| {
        painter.styled(0, 0, "A", |cell| {
            cell.fg = WireColor::Named(1).pack();
            cell.attrs |= ATTR_HIDDEN;
        });
    });
    assert_eq!(scene.glyphs[0].color, color::DEFAULT_BACKGROUND);
}

#[test]
fn a_blinking_cell_is_painted_out_on_the_off_phase_and_drawn_on_the_on_phase() {
    let blinking = |painter: &mut Painter| {
        painter.styled(0, 0, "A", |cell| {
            cell.fg = WireColor::Named(1).pack();
            cell.attrs |= zellij_utils::structured_render::ATTR_SLOW_BLINK;
        });
    };
    let on = scene_at(1, 2, blinking, BlinkPhase::On);
    let off = scene_at(1, 2, blinking, BlinkPhase::Off);
    assert_eq!(on.glyphs[0].color, color::ANSI_16[1]);
    assert_eq!(off.glyphs[0].color, color::DEFAULT_BACKGROUND);
}

#[test]
fn a_cell_that_does_not_blink_is_unaffected_by_the_phase() {
    let plain = |painter: &mut Painter| {
        painter.styled(0, 0, "A", |cell| cell.fg = WireColor::Named(1).pack());
    };
    let on = scene_at(1, 2, plain, BlinkPhase::On);
    let off = scene_at(1, 2, plain, BlinkPhase::Off);
    assert_eq!(on.glyphs[0].color, off.glyphs[0].color);
}

#[test]
fn the_cursor_cell_keeps_its_inversion_through_the_off_phase() {
    let off = scene_at(
        1,
        2,
        |painter| {
            painter.styled(0, 0, "A", |cell| {
                cell.fg = WireColor::Named(1).pack();
                cell.attrs |= zellij_utils::structured_render::ATTR_SLOW_BLINK;
            });
            painter.cursor(0, 0, CursorShape::Block);
        },
        BlinkPhase::Off,
    );
    assert_eq!(off.glyphs[0].color, color::DEFAULT_BACKGROUND);
    assert!(off.rects.iter().any(|rect| rect.color == color::ANSI_16[1]));
}

#[test]
fn build_renders_the_on_phase() {
    let blinking = |painter: &mut Painter| {
        painter.styled(0, 0, "A", |cell| {
            cell.fg = WireColor::Named(1).pack();
            cell.attrs |= zellij_utils::structured_render::ATTR_SLOW_BLINK;
        });
    };
    let state = Painter::state(1, 2, blinking);
    assert_eq!(
        build(&state, &mut cache()).glyphs[0].color,
        scene_at(1, 2, blinking, BlinkPhase::On).glyphs[0].color
    );
}

pub(super) fn entry_of(cache: &mut GlyphCache, style: FaceStyle, character: char) -> AtlasEntry {
    match cache.glyph(style, character, 1) {
        Lookup::Rendered(rendered) => rendered.entry,
        other => panic!("expected a rendered glyph, got {:?}", other),
    }
}

#[test]
fn bold_and_italic_select_different_faces() {
    let mut cache = cache();
    let regular = entry_of(&mut cache, FaceStyle::Regular, 'A');
    let bold = entry_of(&mut cache, FaceStyle::Bold, 'A');
    let state = Painter::state(1, 4, |painter| {
        painter.text(0, 0, "A");
        painter.styled(0, 1, "A", |cell| cell.attrs |= ATTR_BOLD);
    });
    let scene = build(&state, &mut cache);
    assert_eq!(scene.glyphs[0].entry, regular);
    assert_eq!(scene.glyphs[1].entry, bold);
}

fn underlined(style: u16) -> impl Fn(&mut Painter) {
    move |painter: &mut Painter| {
        painter.styled(0, 0, "abcd", |cell| {
            cell.attrs |= style << zellij_utils::structured_render::UNDERLINE_SHIFT;
        });
    }
}

fn hovered_scene(rows: usize, cols: usize, run: Option<&LinkRun>) -> Scene {
    let state = Painter::state(rows, cols, |painter| {
        painter.links(&[(1, "https://example.com")]);
        painter.linked(0, 0, "ab", 1);
        painter.text(0, 2, "cd");
    });
    build_at(
        &state,
        &mut cache(),
        BlinkPhase::On,
        &Paints::default(),
        CursorOptions::default(),
        run,
        None,
    )
}

#[test]
fn a_link_is_undecorated_until_it_is_hovered() {
    let metrics = cache().metrics();
    let plain = hovered_scene(1, 4, None);
    assert!(!plain
        .rects
        .iter()
        .any(|rect| rect.y == metrics.underline_top as i32));

    let run = LinkRun {
        id: 1,
        uri: "https://example.com".to_owned(),
        spans: vec![(0, 0, 1)],
    };
    let hovered = hovered_scene(1, 4, Some(&run));
    let strokes: Vec<_> = hovered
        .rects
        .iter()
        .filter(|rect| {
            rect.y == metrics.underline_top as i32
                && rect.width == metrics.width
                && rect.height == metrics.stroke.max(1)
        })
        .collect();
    assert_eq!(strokes.len(), 2, "{:?}", hovered.rects);
    assert_eq!(strokes[0].x, 0);
    assert_eq!(strokes[1].x, metrics.width as i32);
}

#[test]
fn a_hovered_run_that_wrapped_is_underlined_on_both_of_its_rows() {
    let state = Painter::state(2, 4, |painter| {
        painter.links(&[(1, "https://example.com")]);
        painter.linked(0, 2, "ab", 1);
        painter.linked(1, 0, "cd", 1);
    });
    let run = LinkRun {
        id: 1,
        uri: "https://example.com".to_owned(),
        spans: vec![(0, 2, 3), (1, 0, 1)],
    };
    let scene = build_at(
        &state,
        &mut cache(),
        BlinkPhase::On,
        &Paints::default(),
        CursorOptions::default(),
        Some(&run),
        None,
    );
    let metrics = cache().metrics();
    let strokes: Vec<(i32, i32)> = scene
        .rects
        .iter()
        .filter(|rect| rect.width == metrics.width && rect.height == metrics.stroke.max(1))
        .map(|rect| (rect.x, rect.y))
        .collect();
    let first = metrics.underline_top as i32;
    let second = metrics.height as i32 + first;
    assert_eq!(
        strokes,
        vec![
            (2 * metrics.width as i32, first),
            (3 * metrics.width as i32, first),
            (0, second),
            (metrics.width as i32, second),
        ],
        "only the four cells of the wrapped run, on both rows"
    );
}

pub(super) fn composing(
    state: &TerminalState,
    cache: &mut GlyphCache,
    preedit: Option<&Preedit>,
) -> Scene {
    build_at(
        state,
        cache,
        BlinkPhase::On,
        &Paints::default(),
        CursorOptions::default(),
        None,
        preedit,
    )
}

pub(super) fn composing_state() -> TerminalState {
    Painter::state(2, 8, |painter| {
        painter.text(0, 0, "ab");
        painter.text(1, 0, "second");
        painter.cursor(0, 2, CursorShape::Block);
    })
}

#[test]
fn a_preedit_is_drawn_at_the_cursor_and_underlined() {
    let state = composing_state();
    let mut cache = cache();
    let metrics = cache.metrics();
    let preedit = Preedit::new("ni", Some((2, 2)));
    let scene = composing(&state, &mut cache, Some(&preedit));

    for (offset, character) in [(0, 'n'), (1, 'i')] {
        let entry = entry_of(&mut cache, FaceStyle::Regular, character);
        let x = ((2 + offset) * metrics.width) as i32;
        assert!(
            scene.glyphs.iter().any(|quad| quad.entry == entry
                && quad.x >= x
                && quad.x < x + metrics.width as i32),
            "{} is not drawn in the cell the composition occupies",
            character
        );
        assert!(
            scene.rects.iter().any(|rect| rect.x == x
                && rect.y == metrics.underline_top as i32
                && rect.width == metrics.width
                && rect.height == metrics.stroke.max(1)),
            "{} is not underlined as composing text",
            character
        );
    }
}

#[test]
fn the_cells_under_a_preedit_are_untouched() {
    let state = composing_state();
    let mut cache = cache();
    let preedit = Preedit::new("ni", Some((2, 2)));
    let scene = composing(&state, &mut cache, Some(&preedit));
    let plain = composing(&state, &mut cache, None);

    assert_eq!(
        state.screen().cell(0, 2).character(),
        ' ',
        "the pane's own cells must not learn about a composition"
    );
    let metrics = cache.metrics();
    let untouched = |scene: &Scene| -> Vec<i32> {
        scene
            .glyphs
            .iter()
            .filter(|quad| quad.y >= metrics.height as i32)
            .map(|quad| quad.x)
            .collect()
    };
    assert_eq!(
        untouched(&scene),
        untouched(&plain),
        "a composition changed a row it does not sit on"
    );
    let entry = entry_of(&mut cache, FaceStyle::Regular, 'a');
    assert!(
        scene.glyphs.iter().any(|quad| quad.entry == entry),
        "the text the pane painted is still drawn beneath the composition"
    );
}

#[test]
fn the_cursor_follows_the_composition_rather_than_the_pane() {
    let state = composing_state();
    let mut cache = cache();
    let metrics = cache.metrics();
    let preedit = Preedit::new("ni", Some((2, 2)));
    let scene = composing(&state, &mut cache, Some(&preedit));
    let block = |scene: &Scene| -> Vec<i32> {
        scene
            .rects
            .iter()
            .filter(|rect| rect.height == metrics.height && rect.color == color::DEFAULT_FOREGROUND)
            .map(|rect| rect.x)
            .collect()
    };
    assert_eq!(
        block(&scene),
        vec![(4 * metrics.width) as i32],
        "the cursor sits after the composed text, not under it"
    );
}

#[test]
fn a_preedit_without_a_cursor_range_hides_the_cursor() {
    let state = composing_state();
    let mut cache = cache();
    let metrics = cache.metrics();
    let preedit = Preedit::new("ni", None);
    let scene = composing(&state, &mut cache, Some(&preedit));
    assert!(
        !scene
            .rects
            .iter()
            .any(|rect| rect.height == metrics.height && rect.color == color::DEFAULT_FOREGROUND),
        "winit asks for the cursor to be hidden when it reports no range"
    );
}

#[test]
fn a_wide_composed_character_takes_two_cells() {
    let state = composing_state();
    let mut cache = cache();
    let metrics = cache.metrics();
    let preedit = Preedit::new("你", Some((3, 3)));
    let scene = composing(&state, &mut cache, Some(&preedit));
    let strokes: Vec<i32> = scene
        .rects
        .iter()
        .filter(|rect| {
            rect.y == metrics.underline_top as i32 && rect.height == metrics.stroke.max(1)
        })
        .map(|rect| rect.x)
        .collect();
    assert_eq!(
        strokes,
        vec![(2 * metrics.width) as i32, (3 * metrics.width) as i32],
        "a wide character underlines both of the cells it occupies"
    );
}

#[test]
fn a_preedit_is_clipped_at_the_end_of_its_row() {
    let state = Painter::state(1, 4, |painter| {
        painter.cursor(0, 2, CursorShape::Block);
    });
    let mut cache = cache();
    let metrics = cache.metrics();
    let preedit = Preedit::new("abcd", Some((4, 4)));
    let scene = composing(&state, &mut cache, Some(&preedit));
    let strokes: Vec<i32> = scene
        .rects
        .iter()
        .filter(|rect| {
            rect.y == metrics.underline_top as i32 && rect.height == metrics.stroke.max(1)
        })
        .map(|rect| rect.x)
        .collect();
    assert_eq!(
        strokes,
        vec![(2 * metrics.width) as i32, (3 * metrics.width) as i32],
        "composing text never wraps onto the next row"
    );
}

#[test]
fn underline_emits_a_stroke_at_the_underline_position() {
    let scene = scene_of(1, 2, |painter| {
        painter.styled(0, 0, "A", |cell| {
            cell.attrs |= UNDERLINE_STRAIGHT << zellij_utils::structured_render::UNDERLINE_SHIFT;
        });
    });
    let metrics = cache().metrics();
    assert!(scene
        .rects
        .iter()
        .any(|rect| rect.y == metrics.underline_top as i32
            && rect.width == metrics.width
            && rect.height == metrics.stroke));
}

#[test]
fn a_double_underline_emits_two_strokes() {
    let scene = scene_of(1, 2, |painter| {
        painter.styled(0, 0, "A", |cell| {
            cell.attrs |= UNDERLINE_DOUBLE << zellij_utils::structured_render::UNDERLINE_SHIFT;
        });
    });
    let strokes: Vec<_> = scene
        .rects
        .iter()
        .filter(|rect| rect.width == 8 && rect.height == 1)
        .collect();
    assert_eq!(strokes.len(), 2, "{:?}", scene.rects);
}

#[test]
fn an_underline_color_overrides_the_foreground() {
    let scene = scene_of(1, 2, |painter| {
        painter.styled(0, 0, "A", |cell| {
            cell.attrs |= UNDERLINE_STRAIGHT << zellij_utils::structured_render::UNDERLINE_SHIFT;
            cell.underline_color = WireColor::Indexed(208).pack();
        });
    });
    assert!(scene
        .rects
        .iter()
        .any(|rect| rect.color == color::test_helpers::indexed(208)));
}

fn thick_underline_cache() -> GlyphCache {
    GlyphCache::new(
        FontStack::build(&FontOptions {
            system_fonts: false,
            cell: crate::text::font::CellAdjust {
                underline_thickness: 2.0,
                ..crate::text::font::CellAdjust::default()
            },
            ..FontOptions::default()
        })
        .unwrap(),
    )
}

#[test]
fn a_thicker_underline_is_drawn_thicker_and_leaves_the_strikeout_alone() {
    let mut cache = thick_underline_cache();
    let metrics = cache.metrics();
    let thick = metrics.stroke + 2;
    let state = Painter::state(1, 2, |painter| {
        painter.styled(0, 0, "A", |cell| {
            cell.attrs |= (UNDERLINE_STRAIGHT << zellij_utils::structured_render::UNDERLINE_SHIFT)
                | ATTR_STRIKE;
        });
    });
    let scene = build(&state, &mut cache);
    assert!(
        scene
            .rects
            .iter()
            .any(|rect| rect.y == metrics.underline_top as i32
                && rect.height == thick
                && rect.width == metrics.width),
        "{:?}",
        scene.rects
    );
    assert!(
        scene
            .rects
            .iter()
            .any(|rect| rect.y == metrics.strikeout_top as i32 && rect.height == metrics.stroke),
        "{:?}",
        scene.rects
    );
    assert!(scene
        .rects
        .iter()
        .all(|rect| rect.y + rect.height as i32 <= metrics.height as i32));
}

#[test]
fn a_hovered_link_is_underlined_at_the_adjusted_thickness() {
    let mut cache = thick_underline_cache();
    let metrics = cache.metrics();
    let state = Painter::state(1, 4, |painter| {
        painter.links(&[(1, "https://example.com")]);
        painter.linked(0, 0, "ab", 1);
    });
    let run = LinkRun {
        id: 1,
        uri: "https://example.com".to_owned(),
        spans: vec![(0, 0, 1)],
    };
    let scene = build_at(
        &state,
        &mut cache,
        BlinkPhase::On,
        &Paints::default(),
        CursorOptions::default(),
        Some(&run),
        None,
    );
    let strokes = scene
        .rects
        .iter()
        .filter(|rect| rect.height == metrics.stroke + 2 && rect.width == metrics.width)
        .count();
    assert_eq!(strokes, 2, "{:?}", scene.rects);
}

fn edge_state() -> TerminalState {
    Painter::state(2, 3, |painter| {
        painter.styled(0, 0, "a", |cell| {
            cell.bg = WireColor::Indexed(1).pack();
        });
        painter.text(0, 1, "b");
        painter.styled(1, 2, "c", |cell| {
            cell.bg = WireColor::Indexed(4).pack();
        });
    })
}

fn margins() -> Margins {
    Margins {
        left: 5,
        top: 3,
        right: 7,
        bottom: 2,
    }
}

fn painted(rects: &[Rect], x: i32, y: i32) -> Option<Srgb> {
    rects
        .iter()
        .rev()
        .find(|rect| {
            x >= rect.x
                && x < rect.x + rect.width as i32
                && y >= rect.y
                && y < rect.y + rect.height as i32
        })
        .map(|rect| rect.color)
}

#[test]
fn extended_padding_continues_the_colour_of_the_nearest_edge_cell() {
    let state = edge_state();
    let metrics = cache().metrics();
    let rects = padding_rects(
        &state,
        &Paints::default(),
        metrics,
        Transparency::OPAQUE,
        margins(),
    );
    let (w, h) = (metrics.width as i32, metrics.height as i32);
    let red = color::test_helpers::indexed(1);
    let blue = color::test_helpers::indexed(4);
    assert_eq!(painted(&rects, -1, 0), Some(red), "left of the red cell");
    assert_eq!(painted(&rects, -5, -3), Some(red), "top-left corner");
    assert_eq!(painted(&rects, 0, -1), Some(red), "above the red cell");
    assert_eq!(painted(&rects, w, -1), None, "above a default cell");
    assert_eq!(
        painted(&rects, 3 * w, h),
        Some(blue),
        "right of the blue cell"
    );
    assert_eq!(
        painted(&rects, 2 * w, 2 * h),
        Some(blue),
        "below the blue cell"
    );
    assert_eq!(
        painted(&rects, 3 * w + 6, 2 * h + 1),
        Some(blue),
        "bottom-right corner"
    );
    assert_eq!(painted(&rects, -1, h), None, "left of a default cell");
    assert!(rects.iter().all(|rect| {
        let inside_x = rect.x >= 0 && rect.x + rect.width as i32 <= 3 * w;
        let inside_y = rect.y >= 0 && rect.y + rect.height as i32 <= 2 * h;
        !(inside_x && inside_y)
    }));
}

#[test]
fn see_through_padding_paints_only_cells_with_their_own_background() {
    let state = Painter::state(1, 2, |painter| {
        painter.styled(0, 0, "a", |cell| {
            cell.bg = WireColor::Indexed(1).pack();
        });
        painter.text(0, 1, "b");
    });
    let mut paints = Paints::default();
    paints.background = color::test_helpers::indexed(1);
    let metrics = cache().metrics();
    let see_through = Transparency {
        opacity: 0.5,
        mode: OpacityMode::Background,
    };
    let rects = padding_rects(&state, &paints, metrics, see_through, margins());
    assert_eq!(
        painted(&rects, -1, 0),
        Some(color::test_helpers::indexed(1))
    );
    assert_eq!(
        painted(&rects, 2 * metrics.width as i32, 0),
        None,
        "a default-background edge must stay see-through like the grid"
    );

    let opaque = padding_rects(&state, &paints, metrics, Transparency::OPAQUE, margins());
    assert_eq!(
        painted(&opaque, -1, 0),
        None,
        "an opaque window leaves a cell matching the clear colour to the clear"
    );
}

#[test]
fn padding_with_no_margins_draws_nothing() {
    let rects = padding_rects(
        &edge_state(),
        &Paints::default(),
        cache().metrics(),
        Transparency::OPAQUE,
        Margins::default(),
    );
    assert!(rects.is_empty(), "{:?}", rects);
}

fn decoration_rows(scene: &Scene) -> Vec<i32> {
    let mut rows: Vec<i32> = scene.rects.iter().map(|rect| rect.y).collect();
    rows.sort_unstable();
    rows.dedup();
    rows
}

#[test]
fn a_dotted_underline_is_one_stroke_on_and_two_off() {
    let scene = scene_of(1, 8, underlined(UNDERLINE_DOTTED));
    let metrics = cache().metrics();
    assert!(!scene.rects.is_empty());
    for rect in &scene.rects {
        assert_eq!(rect.width, 1, "{:?}", rect);
        assert_eq!(rect.x % 3, 0, "the dots left their period: {:?}", rect);
        assert_eq!(rect.y, metrics.underline_top as i32);
    }
}

#[test]
fn a_dotted_pattern_runs_on_across_a_cell_boundary() {
    let scene = scene_of(1, 8, underlined(UNDERLINE_DOTTED));
    let metrics = cache().metrics();
    let columns: Vec<u32> = scene.rects.iter().map(|rect| rect.x as u32).collect();
    let spans: Vec<u32> = columns.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(
        spans.iter().all(|span| *span == 3),
        "the pattern restarted at a cell boundary: {:?}",
        columns
    );
    assert!(columns.iter().any(|x| *x >= metrics.width));
}

#[test]
fn a_dashed_underline_is_half_a_cell_on_and_half_off() {
    let scene = scene_of(1, 8, underlined(UNDERLINE_DASHED));
    let metrics = cache().metrics();
    let dash = metrics.width / 2;
    for rect in &scene.rects {
        assert!(
            rect.x as u32 % metrics.width < dash,
            "a dash landed in the gap: {:?}",
            rect
        );
    }
    assert_eq!(scene.rects.len() as u32, dash * 4);
}

#[test]
fn an_undercurl_swings_evenly_around_the_underline_position() {
    let scene = scene_of(1, 8, underlined(UNDERLINE_CURLY));
    let metrics = cache().metrics();
    let rows = decoration_rows(&scene);
    let centre = metrics.underline_top as i32;
    let amplitude = (metrics.height / 10).max(2 * metrics.stroke) as i32;
    assert_eq!(rows.first().copied(), Some(centre - amplitude));
    assert_eq!(rows.last().copied(), Some(centre + amplitude));
    assert!(rows.contains(&centre));
}

#[test]
fn an_undercurl_completes_one_cycle_per_cell() {
    let scene = scene_of(1, 8, |painter| {
        painter.styled(0, 0, "ab", |cell| {
            cell.attrs |= UNDERLINE_CURLY << zellij_utils::structured_render::UNDERLINE_SHIFT;
        });
    });
    let metrics = cache().metrics();
    let trough = scene
        .rects
        .iter()
        .filter(|rect| rect.y == (metrics.underline_top + 2) as i32)
        .count();
    assert_eq!(trough, 2, "expected one trough per cell");
}

#[test]
fn every_underline_style_stays_inside_its_own_row() {
    let metrics = cache().metrics();
    for style in [
        UNDERLINE_STRAIGHT,
        UNDERLINE_DOUBLE,
        UNDERLINE_CURLY,
        UNDERLINE_DOTTED,
        UNDERLINE_DASHED,
    ] {
        let scene = scene_of(2, 8, underlined(style));
        for rect in &scene.rects {
            assert!(rect.y >= 0, "{} left the frame: {:?}", style, rect);
            assert!(
                rect.y + rect.height as i32 <= metrics.height as i32,
                "{} bled into the row below: {:?}",
                style,
                rect
            );
        }
    }
    let struck = scene_of(2, 8, |painter| {
        painter.styled(0, 0, "abcd", |cell| cell.attrs |= ATTR_STRIKE);
    });
    for rect in &struck.rects {
        assert!(rect.y >= 0 && rect.y + rect.height as i32 <= metrics.height as i32);
    }
}

#[test]
fn a_doubled_font_size_doubles_the_decoration_geometry() {
    let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE * 2.0).unwrap());
    let metrics = cache.metrics();
    let state = Painter::state(1, 8, underlined(UNDERLINE_CURLY));
    let scene = build(&state, &mut cache);
    let rows = decoration_rows(&scene);
    let amplitude = (metrics.height / 10).max(2 * metrics.stroke) as i32;
    assert_eq!(amplitude, 4);
    assert_eq!(
        rows.last().copied().unwrap() - rows.first().copied().unwrap(),
        amplitude * 2
    );
}

#[test]
fn strikeout_sits_above_the_underline() {
    let scene = scene_of(1, 2, |painter| {
        painter.styled(0, 0, "A", |cell| cell.attrs |= ATTR_STRIKE);
    });
    let metrics = cache().metrics();
    let stroke = scene
        .rects
        .iter()
        .find(|rect| rect.height == metrics.stroke)
        .unwrap();
    assert_eq!(stroke.y, metrics.strikeout_top as i32);
    assert!(stroke.y < metrics.underline_top as i32);
}

#[test]
fn a_block_cursor_inverts_its_cell() {
    let scene = scene_of(1, 4, |painter| {
        painter.text(0, 0, "A");
        painter.cursor(0, 0, CursorShape::Block);
    });
    let metrics = cache().metrics();
    assert!(scene.rects.iter().any(|rect| rect.x == 0
        && rect.width == metrics.width
        && rect.height == metrics.height
        && rect.color == color::DEFAULT_FOREGROUND));
    assert_eq!(scene.glyphs[0].color, color::DEFAULT_BACKGROUND);
}

#[test]
fn a_beam_cursor_is_a_narrow_full_height_stroke() {
    let scene = scene_of(1, 4, |painter| painter.cursor(0, 0, CursorShape::Beam));
    let metrics = cache().metrics();
    let beam = scene
        .rects
        .iter()
        .find(|rect| rect.height == metrics.height)
        .unwrap();
    assert_eq!(beam.width, metrics.stroke);
    assert_eq!(beam.x, 0);
}

#[test]
fn an_underline_cursor_is_a_stroke_along_the_bottom_of_its_cell() {
    let scene = scene_of(1, 4, |painter| painter.cursor(0, 0, CursorShape::Underline));
    let metrics = cache().metrics();
    let rect = scene.rects.first().expect("no cursor rectangle");
    assert_eq!(rect.width, metrics.width);
    assert_eq!(rect.height, metrics.stroke);
    assert_eq!(rect.y, (metrics.height - metrics.stroke) as i32);
}

#[test]
fn a_hidden_cursor_emits_nothing() {
    let scene = scene_of(1, 4, |_| {});
    assert!(scene.rects.is_empty(), "{:?}", scene.rects);
}

#[test]
fn a_wide_fallback_symbol_spreads_into_a_blank_neighbour_instead_of_shrinking() {
    let glyph_of = |text: &str| {
        let scene = scene_of(1, 4, |painter| painter.text(0, 0, text));
        scene
            .glyphs
            .iter()
            .find(|quad| quad.x < 8)
            .copied()
            .expect("the symbol drew nothing")
    };
    let spread = glyph_of("\u{3012} ");
    let crowded = glyph_of("\u{3012}x");
    assert!(
        spread.entry.height > crowded.entry.height,
        "{:?} vs {:?}",
        spread.entry,
        crowded.entry
    );
    assert!(spread.x >= 0 && spread.x + spread.entry.width as i32 <= 16);
    assert!(crowded.entry.width <= 10, "{:?}", crowded.entry);
}

#[test]
fn nothing_spreads_past_the_last_column_or_into_a_neighbour_with_text() {
    let spill = |text: &str, col: usize| {
        let state = Painter::state(1, 3, |painter| painter.text(0, 0, text));
        let paints = Paints::default();
        let cells: Vec<RowCell> = (0..3)
            .map(|col| {
                let cell = state.screen().cell(0, col);
                let occupancy = state.occupancy(0, col);
                RowCell {
                    cell,
                    occupancy,
                    paint: paint_of(cell, occupancy, &paints, None),
                    explicit_background: false,
                    on_cursor: false,
                    inverted_by_cursor: false,
                    composing: false,
                    held: false,
                    cluster: false,
                    continuation: false,
                    complex: false,
                    unit_end: col + 1,
                }
            })
            .collect();
        spill_room(&cells, state.geometry(), 0, col)
    };
    assert_eq!(spill("ab ", 0), 0);
    assert_eq!(spill("a  ", 0), 1);
    assert_eq!(spill("ab ", 2), 0);
}

#[test]
fn a_wide_character_spacer_draws_no_glyph_of_its_own() {
    let scene = scene_of(1, 6, |painter| painter.wide(0, 0, '\u{2500}'));
    assert_eq!(scene.glyphs.len(), 1);
}

#[test]
fn a_cjk_character_is_drawn_once_across_two_cells() {
    let scene = scene_of(1, 6, |painter| painter.wide(0, 0, '\u{4f60}'));
    assert_eq!(scene.glyphs.len(), 1);
    let quad = scene.glyphs[0];
    assert!(quad.entry.width > 8, "{:?}", quad.entry);
    assert!(quad.entry.width <= 16, "{:?}", quad.entry);
    assert!(quad.x >= 0 && quad.x + quad.entry.width as i32 <= 16);
}

#[test]
fn an_emoji_lands_in_the_color_list_rather_than_the_mask_list() {
    let scene = scene_of(1, 6, |painter| painter.wide(0, 0, '\u{1f600}'));
    assert!(scene.glyphs.is_empty(), "{:?}", scene.glyphs);
    assert_eq!(scene.color_glyphs.len(), 1);
    assert_eq!(scene.color_glyphs[0].color, OPAQUE);
}

#[test]
fn a_dimmed_emoji_is_tinted_rather_than_recolored() {
    let scene = scene_of(1, 6, |painter| {
        painter.wide_styled(0, 0, '\u{1f600}', |cell| cell.attrs |= ATTR_DIM);
    });
    assert_eq!(scene.color_glyphs[0].color, color::dim(OPAQUE));
}

#[test]
fn a_hidden_emoji_is_not_drawn_at_all() {
    let scene = scene_of(1, 6, |painter| {
        painter.wide_styled(0, 0, '\u{1f600}', |cell| cell.attrs |= ATTR_HIDDEN);
    });
    assert!(scene.color_glyphs.is_empty(), "{:?}", scene.color_glyphs);
}

#[test]
fn a_codepoint_no_face_carries_is_drawn_as_a_box() {
    let scene = scene_of(1, 6, |painter| painter.text(0, 0, "\u{10fffd}"));
    assert!(scene.glyphs.is_empty(), "{:?}", scene.glyphs);
    assert_eq!(scene.rects.len(), 4, "{:?}", scene.rects);
    assert!(scene
        .rects
        .iter()
        .all(|rect| rect.color == color::DEFAULT_FOREGROUND));
}

#[test]
fn a_variation_selector_is_not_drawn() {
    let scene = scene_of(1, 6, |painter| painter.text(0, 0, "A\u{fe0f}"));
    assert_eq!(scene.glyphs.len(), 1);
    assert!(scene.rects.is_empty(), "{:?}", scene.rects);
}

fn image(id: u64, width: u32, height: u32) -> GraphicsRecord {
    painter::residency(id, width, height, [255, 128, 0, 255])
}

fn scene_with_graphics(rows: usize, cols: usize, records: &[GraphicsRecord]) -> Scene {
    let state = Painter::state(rows, cols, |painter| painter.graphics(records));
    build(&state, &mut cache())
}

#[test]
fn a_placement_becomes_a_quad_at_the_cell_it_names() {
    let scene = scene_with_graphics(
        4,
        8,
        &[
            image(1, 16, 40),
            Place::of(1, 16, 40).at(1, 2).offset(2, 3).record(),
        ],
    );
    assert_eq!(scene.images.len(), 1);
    let quad = &scene.images[0];
    assert_eq!((quad.x, quad.y), (2 * 8 + 2, 20 + 3));
    assert_eq!((quad.width, quad.height), (16, 40));
    assert_eq!((quad.source_x, quad.source_y), (0, 0));
    assert_eq!(scene.resident_images, vec![ImageKey::Kitty(1)]);
}

#[test]
fn a_cropped_placement_carries_the_source_offset_and_extent() {
    let scene = scene_with_graphics(
        4,
        8,
        &[
            image(1, 16, 40),
            Place::of(1, 16, 40).cropped(4, 6, 8, 10).record(),
        ],
    );
    let quad = &scene.images[0];
    assert_eq!((quad.source_x, quad.source_y), (4, 6));
    assert_eq!((quad.width, quad.height), (8, 10));
}

fn clipping_pane(x: u16, y: u16, cols: u16, rows: u16) -> PaneRect {
    PaneRect {
        x,
        y,
        cols,
        rows,
        top: 0,
        bottom: 0,
        left: 0,
        right: 0,
        flags: PANE_SELECTABLE,
    }
}

fn quad_at(x: i32, y: i32, width: u32, height: u32) -> ImageQuad {
    ImageQuad {
        key: ImageKey::Kitty(1),
        revision: 0,
        x,
        y,
        width,
        height,
        source_x: 0,
        source_y: 0,
        z: 0,
        image: Rc::new(Image {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        }),
    }
}

fn metrics_of(width: u32, height: u32) -> CellMetrics {
    CellMetrics {
        width,
        height,
        ..cache().metrics()
    }
}

#[test]
fn an_image_inside_its_pane_is_left_alone() {
    let geometry = GeometryRecord {
        panes: vec![clipping_pane(0, 0, 8, 4)],
    };
    let quad = quad_at(16, 20, 16, 20);
    let clipped =
        clip_to_owning_pane(quad.clone(), 2, 1, &geometry, metrics_of(8, 20)).expect("kept");
    assert_eq!(clipped, quad);
}

#[test]
fn an_image_straddling_a_pane_edge_is_cut_at_the_edge() {
    let geometry = GeometryRecord {
        panes: vec![clipping_pane(0, 0, 4, 4), clipping_pane(4, 0, 4, 4)],
    };
    let metrics = metrics_of(8, 20);
    let clipped = clip_to_owning_pane(quad_at(16, 0, 32, 20), 2, 0, &geometry, metrics)
        .expect("the part inside the pane survives");
    assert_eq!(
        (clipped.x, clipped.width),
        (16, 16),
        "an image starting at column 2 of a four-column pane keeps two columns"
    );
    assert_eq!(
        (clipped.source_x, clipped.source_y),
        (0, 0),
        "cutting the right edge does not move the source origin"
    );
    assert_eq!((clipped.y, clipped.height), (0, 20));
}

#[test]
fn an_image_cut_on_its_left_and_top_carries_the_source_origin_with_it() {
    let geometry = GeometryRecord {
        panes: vec![clipping_pane(2, 1, 6, 3)],
    };
    let metrics = metrics_of(8, 20);
    let clipped = clip_to_owning_pane(quad_at(0, 0, 64, 80), 3, 1, &geometry, metrics)
        .expect("the part inside the pane survives");
    assert_eq!((clipped.x, clipped.y), (16, 20));
    assert_eq!((clipped.source_x, clipped.source_y), (16, 20));
    assert_eq!((clipped.width, clipped.height), (48, 60));
}

#[test]
fn an_image_wholly_outside_its_pane_is_dropped() {
    let geometry = GeometryRecord {
        panes: vec![clipping_pane(0, 0, 4, 4)],
    };
    assert_eq!(
        clip_to_owning_pane(quad_at(64, 0, 32, 20), 0, 0, &geometry, metrics_of(8, 20)),
        None
    );
}

#[test]
fn an_image_whose_cell_belongs_to_no_pane_is_left_unclipped() {
    let geometry = GeometryRecord {
        panes: vec![clipping_pane(0, 0, 4, 4)],
    };
    let quad = quad_at(64, 0, 32, 20);
    assert_eq!(
        clip_to_owning_pane(quad.clone(), 8, 0, &geometry, metrics_of(8, 20)),
        Some(quad),
        "a placement the layout does not describe keeps the behaviour it had"
    );
}

#[test]
fn an_image_under_a_floating_pane_is_clipped_to_the_pane_its_origin_falls_in() {
    let geometry = GeometryRecord {
        panes: vec![clipping_pane(0, 0, 8, 4), clipping_pane(4, 0, 4, 4)],
    };
    let metrics = metrics_of(8, 20);
    let under = clip_to_owning_pane(quad_at(0, 0, 64, 20), 0, 0, &geometry, metrics)
        .expect("the tiled pane still owns the image");
    assert_eq!(
        (under.x, under.width),
        (0, 64),
        "the floating pane above does not clip an image the pane below owns"
    );
    let over = clip_to_owning_pane(quad_at(32, 0, 64, 20), 4, 0, &geometry, metrics)
        .expect("the floating pane owns an image placed inside it");
    assert_eq!(
        (over.x, over.width),
        (32, 32),
        "an image placed in the floating pane is cut at the floating pane's edge"
    );
}

#[test]
fn an_image_at_a_pane_edge_is_clipped_on_the_way_into_the_scene() {
    let state = Painter::state(4, 8, |painter| {
        painter.geometry(&GeometryRecord {
            panes: vec![clipping_pane(0, 0, 4, 4), clipping_pane(4, 0, 4, 4)],
        });
        painter.graphics(&[image(1, 64, 20), Place::of(1, 64, 20).at(0, 2).record()]);
    });
    let scene = build(&state, &mut cache());
    assert_eq!(scene.images.len(), 1);
    let quad = &scene.images[0];
    assert_eq!(
        (quad.x, quad.width),
        (2 * 8, 2 * 8),
        "the image is cut at the right edge of the pane it was placed in"
    );
}

#[test]
fn quads_are_ordered_by_z_index() {
    let scene = scene_with_graphics(
        4,
        8,
        &[
            image(1, 8, 20),
            image(2, 8, 20),
            Place::of(1, 8, 20).depth(5).record(),
            Place::of(2, 8, 20).depth(-3).record(),
        ],
    );
    assert_eq!(
        scene.images.iter().map(|quad| quad.z).collect::<Vec<_>>(),
        vec![-3, 5]
    );
    assert_eq!(
        scene.resident_images,
        vec![ImageKey::Kitty(1), ImageKey::Kitty(2)]
    );
}

#[test]
fn a_resident_image_with_no_placement_emits_no_quad() {
    let scene = scene_with_graphics(2, 4, &[image(1, 8, 20)]);
    assert!(scene.images.is_empty());
    assert_eq!(scene.resident_images, vec![ImageKey::Kitty(1)]);
}

#[test]
fn a_deleted_image_leaves_neither_quad_nor_residency() {
    let scene = scene_with_graphics(
        2,
        4,
        &[
            image(1, 8, 20),
            Place::of(1, 8, 20).record(),
            GraphicsRecord::DeleteImage { id: 1 },
        ],
    );
    assert!(scene.images.is_empty());
    assert!(scene.resident_images.is_empty());
}

fn sixel(width: usize, height: usize) -> String {
    format!(
        "\u{1b}P0;1;0q\"1;1;{};{}#0;2;100;0;0!{}~\u{1b}\\",
        width, height, width
    )
}

#[test]
fn a_sixel_chunk_becomes_a_quad_at_the_cell_its_record_names() {
    let scene = scene_with_graphics(4, 8, &[painter::sixel_chunk(1, 2, 24, 6, &sixel(24, 6))]);
    assert_eq!(scene.images.len(), 1);
    let quad = &scene.images[0];
    assert_eq!((quad.x, quad.y), (2 * 8, 20));
    assert_eq!((quad.width, quad.height), (24, 6));
    assert_eq!((quad.source_x, quad.source_y), (0, 0));
    assert_eq!(quad.z, 0);
    assert_eq!(scene.resident_images, vec![ImageKey::Sixel(1)]);
}

#[test]
fn a_sixel_chunk_is_drawn_under_the_kitty_images_that_share_its_z_index() {
    let scene = scene_with_graphics(
        4,
        16,
        &[
            image(1, 8, 20),
            Place::of(1, 8, 20).record(),
            painter::sixel_chunk(2, 0, 8, 6, &sixel(8, 6)),
        ],
    );
    assert_eq!(
        scene.images.iter().map(|quad| quad.key).collect::<Vec<_>>(),
        vec![ImageKey::Sixel(1), ImageKey::Kitty(1)]
    );
    assert_eq!(
        scene.resident_images,
        vec![ImageKey::Sixel(1), ImageKey::Kitty(1)]
    );
}

#[test]
fn the_scene_is_deterministic_for_one_frame() {
    let paint = |painter: &mut Painter| {
        painter.styled(0, 0, "heading", |cell| {
            cell.fg = WireColor::Named(3).pack();
            cell.attrs |= ATTR_BOLD;
        });
        painter.styled(1, 0, "body", |cell| {
            cell.attrs |= UNDERLINE_STRAIGHT << zellij_utils::structured_render::UNDERLINE_SHIFT;
        });
    };
    assert_eq!(scene_of(4, 20, paint), scene_of(4, 20, paint));
}

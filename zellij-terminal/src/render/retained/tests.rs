use super::*;
use crate::emulation::color::Paints;
use crate::render::scene::test_helpers::Scene;
use crate::render::scene::BlinkPhase;
use crate::render::scene::SceneInputs;
use crate::testing::painter::Painter;
use crate::text::font::{FontStack, DEFAULT_FONT_SIZE};
use zellij_utils::structured_render::{
    FrameBuilder, GraphicsMedium, GraphicsRecord, GRAPHICS_FORMAT_RGBA8,
};

fn cache() -> GlyphCache {
    GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).expect("no embedded font"))
}

fn refreshed(scene: &mut RetainedScene, state: &TerminalState, cache: &mut GlyphCache) {
    scene.refresh(
        SceneInputs {
            state: state,
            phase: BlinkPhase::On,
            paints: &Paints::default(),
            cursor: CursorOptions::default(),
            hovered_link: None,
            preedit: None,
        },
        cache,
    );
}

fn rebuilt(state: &TerminalState, cache: &mut GlyphCache) -> Scene {
    scene::test_helpers::build_at(
        state,
        cache,
        BlinkPhase::On,
        &Paints::default(),
        CursorOptions::default(),
        None,
        None,
    )
}

fn painted() -> TerminalState {
    Painter::state(4, 8, |painter| painter.text(0, 0, "hello"))
}

#[test]
fn a_retained_scene_starts_by_building_every_row() {
    let mut cache = cache();
    let state = painted();
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    assert!(retained.rebuilt_everything());
    assert_eq!(retained.flatten(), rebuilt(&state, &mut cache));
}

#[test]
fn an_unmarked_refresh_rebuilds_nothing() {
    let mut cache = cache();
    let state = painted();
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    refreshed(&mut retained, &state, &mut cache);
    assert!(!retained.rebuilt_everything());
    assert!(retained.rebuilt_rows().is_empty());
    assert_eq!(retained.flatten(), rebuilt(&state, &mut cache));
}

#[test]
fn only_the_rows_a_frame_touched_are_rebuilt() {
    let mut cache = cache();
    let mut state = painted();
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);

    Painter::apply_marking(&mut state, &mut retained, |painter| {
        painter.text(2, 0, "second")
    });
    refreshed(&mut retained, &state, &mut cache);
    assert!(!retained.rebuilt_everything());
    assert_eq!(retained.rebuilt_rows(), [2]);
    assert_eq!(retained.flatten(), rebuilt(&state, &mut cache));
}

#[test]
fn a_new_glyph_cache_rebuilds_every_row() {
    let mut cache = cache();
    let state = painted();
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    refreshed(&mut retained, &state, &mut cache);
    assert!(!retained.rebuilt_everything());

    let mut rebuilt_cache = self::cache();
    refreshed(&mut retained, &state, &mut rebuilt_cache);
    assert!(retained.rebuilt_everything());
}

#[test]
fn a_selection_paints_its_cells_in_reversed_colours_and_rebuilds_only_its_rows() {
    let mut cache = cache();
    let state = Painter::state(4, 8, |painter| {
        painter.text(1, 0, "hello");
        painter.text(2, 0, "world");
    });
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    let width = cache.metrics().width as i32;
    let height = cache.metrics().height as i32;
    let foreground = Paints::default().foreground;
    let filled = |scene: &Scene, row: usize, col: usize| {
        let y = row as i32 * height;
        scene
            .rects
            .iter()
            .any(|rect| rect.x == col as i32 * width && rect.y == y && rect.color == foreground)
    };
    assert!(!filled(&retained.flatten(), 1, 2));

    retained.set_selection(Some(Selection::at(1, 2).extended_to(2, 1)));
    refreshed(&mut retained, &state, &mut cache);
    assert_eq!(retained.rebuilt_rows(), [1, 2]);
    let selected = retained.flatten();
    assert!(filled(&selected, 1, 2));
    assert!(filled(&selected, 1, 7));
    assert!(filled(&selected, 2, 0));
    assert!(filled(&selected, 2, 1));
    assert!(!filled(&selected, 1, 1));
    assert!(!filled(&selected, 2, 2));

    retained.set_selection(None);
    refreshed(&mut retained, &state, &mut cache);
    assert_eq!(retained.rebuilt_rows(), [1, 2]);
    assert_eq!(retained.flatten(), rebuilt(&state, &mut cache));
}

#[test]
fn a_row_index_past_the_viewport_marks_everything() {
    let mut cache = cache();
    let state = painted();
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    retained.mark(&Damage::of([9]));
    refreshed(&mut retained, &state, &mut cache);
    assert!(retained.rebuilt_everything());
}

#[test]
fn a_cursor_that_moves_marks_the_row_it_left_and_the_row_it_entered() {
    let mut cache = cache();
    let mut state = Painter::state(4, 8, |painter| {
        painter.text(0, 0, "hello");
        painter.cursor(0, 0, crate::emulation::screen_buffer::CursorShape::Block);
    });
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);

    Painter::apply_marking(&mut state, &mut retained, |painter| {
        painter.cursor(3, 4, crate::emulation::screen_buffer::CursorShape::Block)
    });
    refreshed(&mut retained, &state, &mut cache);
    assert_eq!(retained.rebuilt_rows(), [0, 3]);
    assert_eq!(retained.flatten(), rebuilt(&state, &mut cache));
}

#[test]
fn a_blink_flip_damages_only_the_rows_that_blink() {
    let mut cache = cache();
    let state = Painter::state(4, 8, |painter| {
        painter.text(0, 0, "steady");
        painter.styled(2, 0, "blinks", |cell| {
            cell.attrs |= zellij_utils::structured_render::ATTR_SLOW_BLINK
        });
    });
    let damage = blink_damage(&state, CursorOptions::default());
    assert_eq!(damage, Damage::Rows(vec![2]));

    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    retained.mark(&damage);
    retained.refresh(
        SceneInputs {
            state: &state,
            phase: BlinkPhase::Off,
            paints: &Paints::default(),
            cursor: CursorOptions::default(),
            hovered_link: None,
            preedit: None,
        },
        &mut cache,
    );
    assert_eq!(retained.rebuilt_rows(), [2]);
    assert_eq!(
        retained.flatten(),
        scene::test_helpers::build_at(
            &state,
            &mut cache,
            BlinkPhase::Off,
            &Paints::default(),
            CursorOptions::default(),
            None,
            None,
        )
    );
}

fn tiny_cache() -> GlyphCache {
    GlyphCache::tiny(FontStack::embedded(DEFAULT_FONT_SIZE).expect("no embedded font"))
}

fn crowded() -> crate::emulation::terminal::TerminalState {
    Painter::state(4, 40, |painter| {
        painter.text(0, 0, "abcdefghijklmnopqrstuvwxyz0123456789");
        painter.text(1, 0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ!?#$%&*+-=");
    })
}

#[test]
fn a_refresh_that_fills_the_atlas_flushes_it_and_rebuilds_against_the_fresh_one() {
    let mut cache = tiny_cache();
    let state = crowded();
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    assert_eq!(cache.flushes(), 1, "a full atlas must be renewed once");
    assert!(retained.rebuilt_everything());
    assert_eq!(retained.flatten(), rebuilt(&state, &mut tiny_cache()));
}

fn overflowing() -> crate::emulation::terminal::TerminalState {
    Painter::state(6, 40, |painter| {
        painter.text(0, 0, "abcdefghijklmnopqrstuvwxyz0123456789");
        painter.text(1, 0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ!?#$%&*+-=");
        painter.text(
            2,
            0,
            "\u{4f60}\u{597d}\u{4e16}\u{754c}\u{6f22}\u{5b57}\u{4e2d}\u{6587}\u{65e5}\u{672c}",
        );
        painter.text(
            3,
            0,
            "\u{3042}\u{3044}\u{3046}\u{3048}\u{304a}\u{30a2}\u{30a4}\u{30a6}\u{30a8}\u{30aa}",
        );
    })
}

#[test]
fn a_screen_the_atlas_cannot_hold_is_not_flushed_on_every_refresh() {
    let mut cache = tiny_cache();
    let state = overflowing();
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    assert!(
        cache.exhausted(),
        "the screen must overflow even a freshly flushed tiny atlas for this test to mean anything"
    );
    let flushes = cache.flushes();
    for _ in 0..20 {
        retained.mark(&Damage::Everything);
        refreshed(&mut retained, &state, &mut cache);
    }
    assert_eq!(
        cache.flushes(),
        flushes,
        "an overflowing screen flushed the atlas again straight away"
    );
}

#[test]
fn a_screen_the_atlas_holds_easily_is_never_flushed() {
    let mut cache = tiny_cache();
    let state = Painter::state(4, 8, |painter| painter.text(0, 0, "aaa"));
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    assert_eq!(cache.flushes(), 0);
}

fn graphics(state: &mut crate::emulation::terminal::TerminalState, records: &[GraphicsRecord]) {
    let size = state.size();
    let mut builder = FrameBuilder::new(size.cols as u16, size.rows as u16, 0);
    builder.extend_graphics(records.iter());
    state
        .apply_frame(&builder.finish())
        .expect("a usable frame");
}

fn placement(cell_x: u32) -> GraphicsRecord {
    GraphicsRecord::Placement {
        image_id: 1,
        placement_id: 1,
        cell_x,
        cell_y: 1,
        offset_x: 0,
        offset_y: 0,
        source_x: 0,
        source_y: 0,
        source_width: 2,
        source_height: 2,
        z: 0,
    }
}

fn with_an_image(cell_x: u32) -> crate::emulation::terminal::TerminalState {
    let mut state = crate::emulation::terminal::TerminalState::new(4, 8);
    state.set_cell_size(8, 20);
    graphics(
        &mut state,
        &[
            GraphicsRecord::Residency {
                id: 1,
                width: 2,
                height: 2,
                format: GRAPHICS_FORMAT_RGBA8,
                medium: GraphicsMedium::Inline,
                byte_len: 16,
                data: [10, 20, 30, 255].repeat(4),
            },
            placement(cell_x),
        ],
    );
    state
}

fn see_through() -> Transparency {
    Transparency {
        opacity: 0.8,
        mode: zellij_utils::input::terminal::OpacityMode::Background,
    }
}

fn rects_in_row(scene: &RetainedScene, row: usize) -> usize {
    scene.rows()[row].rects.len()
}

#[test]
fn a_see_through_background_draws_only_the_backgrounds_a_program_chose() {
    use zellij_utils::structured_render::{WireColor, ATTR_REVERSE};
    let mut cache = cache();
    let state = Painter::state(4, 8, |painter| {
        painter.text(0, 0, "plain");
        painter.styled(1, 0, " ", |cell| cell.bg = WireColor::Rgb(0, 0, 0).pack());
        painter.styled(2, 0, " ", |cell| cell.attrs |= ATTR_REVERSE);
        painter.styled(3, 0, " ", |cell| cell.bg = WireColor::Named(1).pack());
    });

    let mut opaque = RetainedScene::new();
    refreshed(&mut opaque, &state, &mut cache);
    assert_eq!(rects_in_row(&opaque, 0), 0);
    assert_eq!(
        rects_in_row(&opaque, 1),
        0,
        "a solid window folds an explicit copy of the default into the clear"
    );

    let mut clear = RetainedScene::new();
    clear.set_transparency(see_through());
    refreshed(&mut clear, &state, &mut cache);
    assert_eq!(
        rects_in_row(&clear, 0),
        0,
        "a default cell must let the clear through"
    );
    assert_eq!(
        rects_in_row(&clear, 1),
        1,
        "an explicit background equal to the default"
    );
    assert_eq!(rects_in_row(&clear, 2), 1, "a reverse-video cell");
    assert_eq!(rects_in_row(&clear, 3), 1, "a colored cell");
}

#[test]
fn hidden_text_on_a_see_through_background_leaves_no_ink() {
    use zellij_utils::structured_render::{
        WireColor, ATTR_HIDDEN, UNDERLINE_SHIFT, UNDERLINE_STRAIGHT,
    };
    let mut cache = cache();
    let state = Painter::state(2, 8, |painter| {
        painter.styled(0, 0, "MM", |cell| {
            cell.attrs |= ATTR_HIDDEN | (UNDERLINE_STRAIGHT << UNDERLINE_SHIFT)
        });
        painter.styled(1, 0, "MM", |cell| {
            cell.attrs |= ATTR_HIDDEN;
            cell.bg = WireColor::Named(1).pack();
        });
    });
    let mut opaque = RetainedScene::new();
    refreshed(&mut opaque, &state, &mut cache);
    assert!(!opaque.rows()[0].glyphs.is_empty() || !opaque.rows()[0].rects.is_empty());

    let mut clear = RetainedScene::new();
    clear.set_transparency(see_through());
    refreshed(&mut clear, &state, &mut cache);
    assert!(clear.rows()[0].glyphs.is_empty(), "hidden glyphs showed");
    assert!(
        clear.rows()[0].rects.is_empty(),
        "a hidden underline showed"
    );
    assert_eq!(clear.rows()[1].rects.len(), 2);
}

#[test]
fn a_change_of_transparency_rebuilds_every_row() {
    let mut cache = cache();
    let state = painted();
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    refreshed(&mut retained, &state, &mut cache);
    assert!(!retained.rebuilt_everything());

    retained.set_transparency(see_through());
    refreshed(&mut retained, &state, &mut cache);
    assert!(retained.rebuilt_everything());
    assert_eq!(retained.transparency(), see_through());

    retained.set_transparency(see_through());
    refreshed(&mut retained, &state, &mut cache);
    assert!(
        !retained.rebuilt_everything(),
        "an unchanged setting rebuilt"
    );

    retained.set_transparency(Transparency {
        mode: zellij_utils::input::terminal::OpacityMode::Everything,
        ..see_through()
    });
    refreshed(&mut retained, &state, &mut cache);
    assert!(
        retained.rebuilt_everything(),
        "a mode change did not rebuild"
    );
}

#[test]
fn a_graphics_state_that_has_not_moved_keeps_the_image_list_it_emitted() {
    let mut cache = cache();
    let state = with_an_image(1);
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    assert!(retained.replaced_images());
    assert_eq!(retained.images().len(), 1);

    refreshed(&mut retained, &state, &mut cache);
    assert!(
        !retained.replaced_images(),
        "nothing about the graphics changed, so the placements must not be re-emitted"
    );
    assert_eq!(retained.images().len(), 1);
}

#[test]
fn a_placement_that_moves_is_re_emitted() {
    let mut cache = cache();
    let mut state = with_an_image(1);
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    let before = retained.images()[0].x;

    graphics(&mut state, &[placement(4)]);
    refreshed(&mut retained, &state, &mut cache);
    assert!(retained.replaced_images());
    assert_ne!(retained.images()[0].x, before);
}

#[test]
fn a_cell_size_that_changes_re_emits_the_placements() {
    let mut cache = cache();
    let state = with_an_image(1);
    let mut retained = RetainedScene::new();
    refreshed(&mut retained, &state, &mut cache);
    refreshed(&mut retained, &state, &mut cache);
    assert!(!retained.replaced_images());

    let mut zoomed =
        GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE * 2.0).expect("no embedded font"));
    refreshed(&mut retained, &state, &mut zoomed);
    assert!(
        retained.replaced_images(),
        "an image is placed in pixels, so a new cell size moves it"
    );
}

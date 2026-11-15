use super::{
    crop_rgba, scale_rgba, KittyGrid, KittyPlacement, KittyVerticalAnchor, MAX_KITTY_PLACEMENTS,
};
use crate::panes::kitty_graphics::parser::{
    DecodedImage, KittyCommand, KittyError, KittyErrorCode, KittyFormat,
};
use crate::panes::kitty_graphics::store::KittyImageStore;
use crate::panes::sixel::PixelRect;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zellij_utils::pane_size::SizeInPixels;

fn rgba_pixel(value: u8) -> [u8; 4] {
    [value, value, value, 255]
}

fn raster(values: &[u8]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| rgba_pixel(*value).to_vec())
        .collect()
}

fn test_grid() -> KittyGrid {
    KittyGrid::new(
        Rc::new(RefCell::new(Some(SizeInPixels {
            width: 10,
            height: 20,
        }))),
        Rc::new(RefCell::new(KittyImageStore::default())),
    )
}

fn test_placement(y: isize, height: usize) -> KittyPlacement {
    KittyPlacement {
        image_id: 1,
        placement_id: 0,
        placement_uid: 0,
        internal_id: 1,
        display_rect: PixelRect {
            x: 0,
            y,
            width: 20,
            height,
        },
        source_rect: PixelRect {
            x: 0,
            y: 0,
            width: 20,
            height,
        },
        emit_x: 0,
        emit_y: 0,
        scaled_px: None,
        dest_cells: (2, 2),
        cell_offset: (0, 0),
        z_index: 0,
        vertical_anchor: KittyVerticalAnchor {
            canonical_line: 0,
            offset_px_from_line_start: 0,
        },
    }
}

#[test]
fn scale_rgba_identity() {
    let src = raster(&[10, 20, 30, 40]);
    let out = scale_rgba(&src, 2, 2, 2, 2);
    assert_eq!(out, src);
}

#[test]
fn scale_rgba_upscales_constant_pixel() {
    let src = raster(&[123]);
    let out = scale_rgba(&src, 1, 1, 3, 3);
    assert_eq!(out, raster(&[123; 9]));
}

#[test]
fn scale_rgba_bilinear_gradient() {
    let src = raster(&[0, 255]);
    let out = scale_rgba(&src, 2, 1, 4, 1);
    let expected: Vec<u8> = vec![
        0, 0, 0, 255, 64, 64, 64, 255, 191, 191, 191, 255, 255, 255, 255, 255,
    ];
    assert_eq!(out, expected);
}

#[test]
fn crop_rgba_extracts_sub_rect() {
    let src = raster(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    let out = crop_rgba(&src, 4, 4, 1, 1, 2, 2);
    assert_eq!(out, raster(&[5, 6, 9, 10]));
}

#[test]
fn changed_chunks_for_placement_straddling_changed_rect() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(30, 40));
    let mut changed_rects = HashMap::new();
    changed_rects.insert(1, 1);
    let chunks = grid.changed_kitty_chunks_in_viewport(changed_rects, 0, 4, 0, 0);
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(chunk.cell_x, 0);
    assert_eq!(chunk.cell_y, 1);
    assert_eq!(chunk.source_px_x, 0);
    assert_eq!(chunk.source_px_y, 0);
    assert_eq!(chunk.source_px_width, 20);
    assert_eq!(chunk.source_px_height, 10);
    assert_eq!(chunk.scaled_image, None);
}

#[test]
fn clear_visible_placements_keeps_history_resident_images() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(0, 40));
    grid.placements.push(test_placement(400, 40));
    grid.clear_visible_placements(20);
    assert_eq!(grid.placement_count(), 1);
    assert_eq!(grid.placements[0].display_rect.y, 0);
}

#[test]
fn clear_all_placements_removes_history_resident_images() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(0, 40));
    grid.placements.push(test_placement(400, 40));
    grid.clear_all_placements();
    assert_eq!(grid.placement_count(), 0);
}

#[test]
fn changed_chunks_for_placement_with_top_scrolled_above_viewport() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(-20, 60));
    let chunks = grid.viewport_kitty_chunks(3, 0, 4, 0, 0);
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(chunk.cell_x, 0);
    assert_eq!(chunk.cell_y, 0);
    assert_eq!(chunk.source_px_x, 0);
    assert_eq!(chunk.source_px_y, 20);
    assert_eq!(chunk.source_px_width, 20);
    assert_eq!(chunk.source_px_height, 40);
}

#[test]
fn changed_chunks_shift_cell_coordinates_by_viewport_offsets() {
    let mut grid = test_grid();
    let cell_width = 10usize;
    let cell_height = 20usize;
    let placement_x = 0usize;
    let placement_y = 30usize;
    grid.placements
        .push(test_placement(placement_y as isize, 40));
    let mut changed_rects = HashMap::new();
    changed_rects.insert(1, 1);
    let viewport_x_offset = 5usize;
    let viewport_y_offset = 7usize;
    let chunks = grid.changed_kitty_chunks_in_viewport(
        changed_rects,
        0,
        4,
        viewport_x_offset,
        viewport_y_offset,
    );
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    let expected_cell_x = viewport_x_offset + placement_x / cell_width;
    let expected_cell_y = viewport_y_offset + placement_y / cell_height;
    assert_eq!(chunk.cell_x, expected_cell_x);
    assert_eq!(chunk.cell_y, expected_cell_y);
    assert_eq!(chunk.source_px_x, 0);
    assert_eq!(chunk.source_px_y, 0);
    assert_eq!(chunk.source_px_width, 20);
    assert_eq!(chunk.source_px_height, 10);
}

#[test]
fn changed_chunks_for_scaled_placement_use_variant_space() {
    let mut grid = test_grid();
    let mut placement = test_placement(0, 40);
    placement.scaled_px = Some((20, 40));
    placement.dest_cells = (2, 2);
    grid.placements.push(placement);
    let mut changed_rects = HashMap::new();
    changed_rects.insert(1, 1);
    let chunks = grid.changed_kitty_chunks_in_viewport(changed_rects, 0, 4, 0, 0);
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(chunk.cell_y, 1);
    assert_eq!(chunk.source_px_y, 20);
    assert_eq!(chunk.source_px_height, 20);
    assert_eq!(chunk.scaled_image.unwrap().size, (20, 40));
}

#[test]
fn changed_chunks_propagate_emit_offsets_of_clipped_placement() {
    let mut grid = test_grid();
    let mut placement = test_placement(100, 20);
    placement.emit_y = 20;
    grid.placements.push(placement);
    let mut changed_rects = HashMap::new();
    changed_rects.insert(5, 1);
    let chunks = grid.changed_kitty_chunks_in_viewport(changed_rects, 0, 4, 0, 0);
    assert_eq!(chunks.len(), 1);
    let chunk = &chunks[0];
    assert_eq!(chunk.cell_y, 5);
    assert_eq!(chunk.source_px_y, 20);
    assert_eq!(chunk.source_px_height, 20);
}

#[test]
fn region_scroll_alt_full_screen_moves_and_clips_at_top() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(40, 40));
    grid.placements.push(test_placement(0, 40));
    grid.placements.push(test_placement(180, 20));
    grid.apply_region_scroll(0, 200, 20, 0, 0, false);
    assert_eq!(grid.placements.len(), 3);
    assert_eq!(grid.placements[0].display_rect.y, 20);
    assert_eq!(grid.placements[0].display_rect.height, 40);
    assert_eq!(grid.placements[0].emit_y, 0);
    assert_eq!(grid.placements[1].display_rect.y, 0);
    assert_eq!(grid.placements[1].display_rect.height, 20);
    assert_eq!(grid.placements[1].emit_y, 20);
    assert_eq!(grid.placements[2].display_rect.y, 160);
}

#[test]
fn region_scroll_top_zero_non_alt_keeps_inside_and_shifts_below() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(0, 20));
    grid.placements.push(test_placement(60, 40));
    grid.placements.push(test_placement(160, 20));
    grid.apply_region_scroll(40, 140, 20, 20, 40, false);
    assert_eq!(grid.placements.len(), 3);
    assert_eq!(grid.placements[0].display_rect.y, 0);
    assert_eq!(grid.placements[1].display_rect.y, 60);
    assert_eq!(grid.placements[1].display_rect.height, 40);
    assert_eq!(grid.placements[2].display_rect.y, 180);
}

#[test]
fn region_scroll_inner_region_moves_inside_and_clips_straddler() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(40, 20));
    grid.placements.push(test_placement(80, 40));
    grid.placements.push(test_placement(0, 20));
    grid.apply_region_scroll(20, 100, 20, 0, 0, false);
    assert_eq!(grid.placements.len(), 3);
    assert_eq!(grid.placements[0].display_rect.y, 20);
    assert_eq!(grid.placements[0].display_rect.height, 20);
    assert_eq!(grid.placements[1].display_rect.y, 100);
    assert_eq!(grid.placements[1].display_rect.height, 20);
    assert_eq!(grid.placements[1].emit_y, 20);
    assert_eq!(grid.placements[2].display_rect.y, 0);
}

#[test]
fn region_scroll_reaps_placement_scrolled_out_of_region() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(20, 20));
    grid.apply_region_scroll(20, 100, 20, 0, 0, false);
    assert_eq!(grid.placements.len(), 0);
}

#[test]
fn reverse_index_region_scroll_clips_bottom_of_straddler() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(40, 40));
    grid.apply_region_scroll(0, 80, -20, 0, 0, false);
    assert_eq!(grid.placements.len(), 1);
    let placement = &grid.placements[0];
    assert_eq!(placement.display_rect.y, 60);
    assert_eq!(placement.display_rect.height, 20);
    assert_eq!(placement.emit_y, 0);
    assert_eq!(placement.source_rect.y, 0);
}

#[test]
fn reverse_index_region_scroll_reaps_fully_below_margin() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(60, 20));
    grid.apply_region_scroll(0, 80, -20, 0, 0, false);
    assert_eq!(grid.placements.len(), 0);
}

fn transmitted(
    grid: &mut KittyGrid,
    width: u32,
    height: u32,
) -> (u32, super::super::store::InternalImageId) {
    let command = KittyCommand {
        image_id: Some(1),
        pixel_width: Some(width),
        pixel_height: Some(height),
        ..Default::default()
    };
    let image = DecodedImage {
        bytes: vec![0u8; (width * height * 4) as usize],
        width,
        height,
        format: KittyFormat::Rgba32,
    };
    let pane_image_id = grid.transmit(&command, image).expect("a transmission");
    let internal = grid.pane_image_id_map()[&pane_image_id];
    (pane_image_id, internal)
}

fn transmitted_with_number(
    grid: &mut KittyGrid,
    image_number: u32,
) -> (u32, super::super::store::InternalImageId) {
    let command = KittyCommand {
        image_number: Some(image_number),
        ..Default::default()
    };
    let image = DecodedImage {
        bytes: vec![0u8; 16],
        width: 2,
        height: 2,
        format: KittyFormat::Rgba32,
    };
    let pane_image_id = grid.transmit(&command, image).expect("a transmission");
    let internal = grid.pane_image_id_map()[&pane_image_id];
    (pane_image_id, internal)
}

#[test]
fn inserting_a_canonical_line_at_the_front_invalidates_the_start_index() {
    let mut grid = test_grid();
    grid.set_canonical_starts(vec![0, 2, 5]);
    assert!(grid.canonical_starts_valid());
    grid.placements.push(test_placement(0, 20));

    grid.insert_canonical_anchor_line_at_front();

    assert!(!grid.canonical_starts_valid());
    assert_eq!(grid.canonical_start_count(), 0);
    assert_eq!(grid.placements()[0].vertical_anchor.canonical_line, 1);
}

#[test]
fn a_placement_touching_but_not_overlapping_a_changed_rect_emits_no_chunk() {
    let mut grid = test_grid();
    grid.placements.push(test_placement(20, 20));
    let mut changed_rects = HashMap::new();
    changed_rects.insert(0, 1);

    let chunks = grid.changed_kitty_chunks_in_viewport(changed_rects, 0, 40, 0, 0);

    assert!(
        chunks.is_empty(),
        "a zero-height intersection must not create a chunk"
    );
}

#[test]
fn freeing_placements_prunes_stale_image_ids_and_numbers() {
    let mut grid = test_grid();
    for image_number in 1..=3u32 {
        let (pane_image_id, internal) = transmitted_with_number(&mut grid, image_number);
        let command = KittyCommand {
            image_number: Some(image_number),
            ..Default::default()
        };
        grid.place(
            pane_image_id,
            internal,
            &command,
            (0, 0),
            SizeInPixels {
                width: 10,
                height: 20,
            },
            KittyVerticalAnchor {
                canonical_line: 0,
                offset_px_from_line_start: 0,
            },
        )
        .expect("a placement");
    }
    assert_eq!(grid.image_ids.len(), 3);
    assert_eq!(grid.image_numbers.len(), 3);

    grid.offset_grid_top();

    assert!(grid.placements().is_empty());
    assert_eq!(grid.kitty_image_store.borrow().image_count(), 0);
    assert!(grid.image_ids.is_empty());
    assert!(grid.image_numbers.is_empty());
}

#[test]
fn store_quota_eviction_prunes_transmit_mappings() {
    let store = Rc::new(RefCell::new(KittyImageStore::with_quota(1100)));
    let mut grid = KittyGrid::new(
        Rc::new(RefCell::new(Some(SizeInPixels {
            width: 10,
            height: 20,
        }))),
        store.clone(),
    );
    for image_number in 1..=3u32 {
        transmitted_with_number(&mut grid, image_number);
    }

    assert_eq!(store.borrow().image_count(), 2);
    assert_eq!(grid.image_ids.len(), 2);
    let live_ids: std::collections::HashSet<u32> = grid.image_ids.keys().copied().collect();
    assert!(grid
        .image_numbers
        .values()
        .flatten()
        .all(|pane_image_id| live_ids.contains(pane_image_id)));
}

#[test]
fn an_unscaled_placement_emits_from_its_source_rectangle() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 64, 64);
    let command = KittyCommand {
        image_id: Some(1),
        source_x: 8,
        source_y: 10,
        source_w: 16,
        source_h: 20,
        ..Default::default()
    };
    let cell = SizeInPixels {
        width: 10,
        height: 20,
    };
    grid.place(
        pane_image_id,
        internal,
        &command,
        (0, 0),
        cell,
        KittyVerticalAnchor {
            canonical_line: 0,
            offset_px_from_line_start: 0,
        },
    )
    .expect("a placement");

    let mut changed_rects = HashMap::new();
    changed_rects.insert(0, 2);
    let chunks = grid.changed_kitty_chunks_in_viewport(changed_rects, 0, 40, 0, 0);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].source_px_x, 8);
    assert_eq!(chunks[0].source_px_y, 10);
    assert_eq!(chunks[0].source_px_width, 16);
    assert_eq!(chunks[0].source_px_height, 20);
}

#[test]
fn a_scaled_placement_emits_from_the_origin_of_its_cropped_variant() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 64, 64);
    let command = KittyCommand {
        image_id: Some(1),
        source_x: 8,
        source_y: 10,
        source_w: 16,
        source_h: 20,
        columns: 2,
        rows: 2,
        ..Default::default()
    };
    let cell = SizeInPixels {
        width: 10,
        height: 20,
    };
    grid.place(
        pane_image_id,
        internal,
        &command,
        (0, 0),
        cell,
        KittyVerticalAnchor {
            canonical_line: 0,
            offset_px_from_line_start: 0,
        },
    )
    .expect("a placement");

    let placement = &grid.placements()[0];
    assert_eq!(placement.emit_x, 0);
    assert_eq!(placement.emit_y, 0);
    assert_eq!(placement.source_rect.x, 8);
    assert_eq!(placement.source_rect.y, 10);
}

fn place_with(
    grid: &mut KittyGrid,
    pane_image_id: u32,
    internal: super::super::store::InternalImageId,
    columns: u32,
    rows: u32,
) -> Result<(u16, u16), KittyError> {
    let command = KittyCommand {
        image_id: Some(1),
        columns,
        rows,
        ..Default::default()
    };
    let cell = SizeInPixels {
        width: 10,
        height: 20,
    };
    grid.place(
        pane_image_id,
        internal,
        &command,
        (0, 0),
        cell,
        KittyVerticalAnchor {
            canonical_line: 0,
            offset_px_from_line_start: 0,
        },
    )
}

#[test]
fn a_placement_with_columns_and_rows_overflowing_the_target_size_is_rejected() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 4, 4);
    for (columns, rows) in [(u32::MAX, u32::MAX), (100_000, 100_000), (2000, 1000)] {
        let error = place_with(&mut grid, pane_image_id, internal, columns, rows)
            .expect_err("a placement beyond the size limit");
        assert_eq!(error.code, KittyErrorCode::Einval);
    }
    assert!(grid.placements().is_empty());
}

#[test]
fn a_placement_with_columns_only_is_bounded_on_the_derived_height() {
    // 1x10000 image: the height derived from the aspect ratio is 10000 times
    // the target width
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 1, 10_000);
    for columns in [u32::MAX, 1000, 100] {
        let error = place_with(&mut grid, pane_image_id, internal, columns, 0)
            .expect_err("a placement beyond the size limit");
        assert_eq!(error.code, KittyErrorCode::Einval);
    }
    assert!(grid.placements().is_empty());
}

#[test]
fn a_placement_with_rows_only_is_bounded_on_the_derived_width() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 10_000, 1);
    for rows in [u32::MAX, 1000, 100] {
        let error = place_with(&mut grid, pane_image_id, internal, 0, rows)
            .expect_err("a placement beyond the size limit");
        assert_eq!(error.code, KittyErrorCode::Einval);
    }
    assert!(grid.placements().is_empty());
}

#[test]
fn a_placement_within_the_target_size_limit_is_scaled() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 4, 4);
    place_with(&mut grid, pane_image_id, internal, 100, 50).expect("a placement");
    let placement = &grid.placements()[0];
    assert_eq!(placement.scaled_px, Some((1000, 1000)));
}

#[test]
fn scale_rgba_refuses_targets_beyond_the_limits_without_allocating() {
    let src = raster(&[1]);
    assert!(scale_rgba(&src, 1, 1, usize::MAX, usize::MAX).is_empty());
    assert!(scale_rgba(&src, 1, 1, 20_000, 20_000).is_empty());
    assert!(scale_rgba(&src, 1, 1, 10_000, 10_000).is_empty());
}

#[test]
fn a_cell_size_change_drops_a_placement_whose_new_target_is_oversized() {
    let cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 10,
        height: 20,
    })));
    let store = Rc::new(RefCell::new(KittyImageStore::default()));
    let mut grid = KittyGrid::new(cell_size.clone(), store.clone());
    let (pane_image_id, internal) = transmitted(&mut grid, 4, 4);
    // 9000 x 2000 pixels: within the limits at 10x20 pixel cells
    place_with(&mut grid, pane_image_id, internal, 900, 100).expect("a placement");
    assert_eq!(grid.placements().len(), 1);

    // 18000 pixels wide at 20 pixel wide cells
    *cell_size.borrow_mut() = Some(SizeInPixels {
        width: 20,
        height: 20,
    });
    grid.character_cell_size_possibly_changed();

    assert!(grid.placements().is_empty());
    // the dropped placement released its image; no empty raster is cached
    assert_eq!(store.borrow().total_bytes(), 0);
}

#[test]
fn a_cell_size_change_keeps_a_placement_that_still_fits() {
    let cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 10,
        height: 20,
    })));
    let store = Rc::new(RefCell::new(KittyImageStore::default()));
    let mut grid = KittyGrid::new(cell_size.clone(), store);
    let (pane_image_id, internal) = transmitted(&mut grid, 4, 4);
    place_with(&mut grid, pane_image_id, internal, 10, 10).expect("a placement");
    *cell_size.borrow_mut() = Some(SizeInPixels {
        width: 20,
        height: 40,
    });
    grid.character_cell_size_possibly_changed();
    assert_eq!(grid.placements().len(), 1);
    assert_eq!(grid.placements()[0].scaled_px, Some((200, 400)));
}

#[test]
fn a_cell_size_change_scales_one_shared_destination_once() {
    let cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        width: 10,
        height: 20,
    })));
    let store = Rc::new(RefCell::new(KittyImageStore::default()));
    let mut grid = KittyGrid::new(cell_size.clone(), store);
    let (pane_image_id, internal) = transmitted(&mut grid, 4, 4);
    place_with(&mut grid, pane_image_id, internal, 2, 2).expect("first");
    place_with(&mut grid, pane_image_id, internal, 2, 2).expect("second");
    assert_eq!(grid.placements().len(), 2);
    let before = grid.kitty_image_store.borrow().scale_count(internal);
    *cell_size.borrow_mut() = Some(SizeInPixels {
        width: 20,
        height: 40,
    });
    grid.character_cell_size_possibly_changed();
    assert_eq!(grid.placements().len(), 2);
    assert_eq!(
        grid.kitty_image_store.borrow().scale_count(internal),
        before + 1,
        "two placements of one destination must share one regenerated raster"
    );
}

#[test]
fn anonymous_placement_id_zero_is_capped_without_copying_the_raster() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 1, 1);
    let bytes_after_transmit = grid.kitty_image_store.borrow().total_bytes();
    let image_count = grid.kitty_image_store.borrow().image_count();
    let cell = SizeInPixels {
        width: 10,
        height: 20,
    };
    let anchor = KittyVerticalAnchor {
        canonical_line: 0,
        offset_px_from_line_start: 0,
    };
    // a=p,i=<id>,p=0,C=1: placement id 0 is a new placement, and C=1
    // would leave the cursor where it is. `place` does not move it.
    let command = KittyCommand {
        image_id: Some(pane_image_id),
        placement_id: Some(0),
        suppress_cursor_movement: true,
        ..Default::default()
    };
    for _ in 0..MAX_KITTY_PLACEMENTS {
        grid.place(pane_image_id, internal, &command, (0, 0), cell, anchor)
            .expect("a placement within the cap");
    }
    let kept: Vec<_> = grid
        .placements()
        .iter()
        .map(|placement| placement.placement_uid)
        .collect();

    let error = grid
        .place(pane_image_id, internal, &command, (0, 0), cell, anchor)
        .expect_err("the placement past the cap");

    assert_eq!(error.code, KittyErrorCode::Einval);
    assert_eq!(MAX_KITTY_PLACEMENTS, 1024);
    assert_eq!(grid.placements().len(), 1024);
    assert_eq!(
        grid.placements()
            .iter()
            .map(|placement| placement.placement_uid)
            .collect::<Vec<_>>(),
        kept,
        "a refused placement must leave the stored records unchanged"
    );
    assert_eq!(grid.kitty_image_store.borrow().image_count(), image_count);
    assert_eq!(
        grid.kitty_image_store.borrow().total_bytes(),
        bytes_after_transmit,
        "sharing one raster must not admit another copy per placement"
    );
    assert!(grid
        .placements()
        .iter()
        .all(|placement| { placement.placement_id == 0 && placement.internal_id == internal }));
}

fn place_scaled(
    grid: &mut KittyGrid,
    pane_image_id: u32,
    internal: super::super::store::InternalImageId,
    placement_id: u32,
    columns: u32,
    rows: u32,
    source_x: u32,
    source_w: u32,
) -> super::super::store::ScaledImageKey {
    let command = KittyCommand {
        image_id: Some(pane_image_id),
        placement_id: Some(placement_id),
        columns,
        rows,
        source_x,
        source_w,
        ..Default::default()
    };
    // 8×16 pixel cells, the cell size from the repeated-placement report.
    let cell = SizeInPixels {
        width: 8,
        height: 16,
    };
    grid.place(
        pane_image_id,
        internal,
        &command,
        (0, 0),
        cell,
        KittyVerticalAnchor {
            canonical_line: 0,
            offset_px_from_line_start: 0,
        },
    )
    .expect("a placement");
    grid.placements()
        .iter()
        .find(|placement| placement.placement_id == placement_id)
        .and_then(|placement| placement.scaled_image())
        .expect("a scaled placement")
}

fn scaled_buffer(
    grid: &KittyGrid,
    internal: super::super::store::InternalImageId,
    key: super::super::store::ScaledImageKey,
) -> Option<(usize, usize)> {
    let store = grid.kitty_image_store.borrow();
    store
        .scaled_variant(internal, key)
        .map(|bytes| (bytes.as_ptr() as usize, bytes.len()))
}

#[test]
fn repeated_place_at_the_same_cell_size_reuses_the_scaled_buffer() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 1, 1);
    // 4×2 cells at 8×16 px is 32×32. Same request shape as placing one pixel
    // at 640×320 (5120×5120), without allocating that raster in the test.
    let key = place_scaled(&mut grid, pane_image_id, internal, 1, 4, 2, 0, 0);
    let (ptr, len) = scaled_buffer(&grid, internal, key).expect("cached scaled raster");
    assert_eq!(len, 32 * 32 * 4);
    assert_eq!(grid.kitty_image_store.borrow().scale_count(internal), 1);

    place_scaled(&mut grid, pane_image_id, internal, 1, 4, 2, 0, 0);
    let (again_ptr, again_len) =
        scaled_buffer(&grid, internal, key).expect("the cached raster is still there");
    assert_eq!(
        again_ptr, ptr,
        "the second place must not allocate another scaled buffer"
    );
    assert_eq!(again_len, len);
    assert_eq!(grid.kitty_image_store.borrow().scale_count(internal), 1);

    let other = place_scaled(&mut grid, pane_image_id, internal, 1, 5, 2, 0, 0);
    assert_ne!(other.size, key.size);
    assert_eq!(grid.kitty_image_store.borrow().scale_count(internal), 2);
    let (still_ptr, _) = scaled_buffer(&grid, internal, key).expect("the first size stays cached");
    assert_eq!(still_ptr, ptr);

    place_scaled(&mut grid, pane_image_id, internal, 1, 4, 2, 0, 0);
    let (restored_ptr, _) = scaled_buffer(&grid, internal, key).expect("the first size is reused");
    assert_eq!(restored_ptr, ptr);
    assert_eq!(grid.kitty_image_store.borrow().scale_count(internal), 2);
}

#[test]
fn the_same_destination_size_with_a_different_source_is_scaled_again() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 4, 1);
    let first = place_scaled(&mut grid, pane_image_id, internal, 1, 2, 1, 0, 2);
    let second = place_scaled(&mut grid, pane_image_id, internal, 1, 2, 1, 2, 2);
    assert_eq!(first.size, second.size);
    assert_ne!(first.source, second.source);
    let (first_ptr, first_len) = scaled_buffer(&grid, internal, first).expect("first crop");
    let (second_ptr, second_len) = scaled_buffer(&grid, internal, second).expect("second crop");
    assert_eq!(first_len, second_len);
    assert_ne!(first_ptr, second_ptr);
    assert_eq!(grid.kitty_image_store.borrow().scale_count(internal), 2);
}

#[test]
fn a_new_scaled_size_drops_the_least_recently_used_variant_past_the_cap() {
    use super::super::store::MAX_SCALED_VARIANTS_PER_IMAGE;

    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 1, 1);
    let mut keys = Vec::new();
    for columns in 1..=MAX_SCALED_VARIANTS_PER_IMAGE as u32 {
        keys.push(place_scaled(
            &mut grid,
            pane_image_id,
            internal,
            1,
            columns,
            1,
            0,
            0,
        ));
    }
    assert_eq!(
        grid.kitty_image_store.borrow().scale_count(internal),
        MAX_SCALED_VARIANTS_PER_IMAGE as u64
    );
    for key in &keys {
        assert!(grid
            .kitty_image_store
            .borrow()
            .has_scaled_variant(internal, *key));
    }

    let extra = place_scaled(&mut grid, pane_image_id, internal, 1, 100, 1, 0, 0);
    assert!(
        !grid
            .kitty_image_store
            .borrow()
            .has_scaled_variant(internal, keys[0]),
        "the least-recently-used size is the one dropped"
    );
    for key in keys.iter().skip(1) {
        assert!(grid
            .kitty_image_store
            .borrow()
            .has_scaled_variant(internal, *key));
    }
    assert!(grid
        .kitty_image_store
        .borrow()
        .has_scaled_variant(internal, extra));

    // Placing columns=2 again refreshes that variant, so the next new size
    // drops columns=3 instead.
    place_scaled(&mut grid, pane_image_id, internal, 1, 2, 1, 0, 0);
    assert_eq!(
        grid.kitty_image_store.borrow().scale_count(internal),
        MAX_SCALED_VARIANTS_PER_IMAGE as u64 + 1,
        "reusing a stored size must not scale"
    );
    let another = place_scaled(&mut grid, pane_image_id, internal, 1, 101, 1, 0, 0);
    assert!(grid
        .kitty_image_store
        .borrow()
        .has_scaled_variant(internal, keys[1]));
    assert!(
        !grid
            .kitty_image_store
            .borrow()
            .has_scaled_variant(internal, keys[2]),
        "the variant that was not reused is dropped"
    );
    assert!(grid
        .kitty_image_store
        .borrow()
        .has_scaled_variant(internal, another));
}

fn place_cells(
    grid: &mut KittyGrid,
    pane_image_id: u32,
    internal: super::super::store::InternalImageId,
    placement_id: u32,
    columns: u32,
    rows: u32,
) -> Result<(u16, u16), KittyError> {
    let command = KittyCommand {
        image_id: Some(pane_image_id),
        placement_id: Some(placement_id),
        columns,
        rows,
        ..Default::default()
    };
    // 8×16 pixel cells, the cell size from the repeated-placement report.
    let cell = SizeInPixels {
        width: 8,
        height: 16,
    };
    grid.place(
        pane_image_id,
        internal,
        &command,
        (0, 0),
        cell,
        KittyVerticalAnchor {
            canonical_line: 0,
            offset_px_from_line_start: 0,
        },
    )
}

#[test]
fn a_destination_the_store_cannot_retain_is_refused_before_scaling() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 1, 1);
    let retained = grid.kitty_image_store.borrow().total_bytes();
    let max = grid
        .kitty_image_store
        .borrow()
        .max_synchronous_scaled_bytes(internal);
    // 640×320 cells at 8×16 px is 5120×5120 RGBA, about 100 MiB.
    assert!(
        5120 * 5120 * 4 > max,
        "the review destination must be over the retained share ({max} bytes)"
    );
    let scales_before = grid.kitty_image_store.borrow().scale_count(internal);

    for _ in 0..2 {
        let error = place_cells(&mut grid, pane_image_id, internal, 1, 640, 320)
            .expect_err("a destination the store cannot retain");
        assert_eq!(error.code, KittyErrorCode::Einval);
    }
    assert_eq!(
        grid.kitty_image_store.borrow().scale_count(internal),
        scales_before,
        "the same refused key must not scale again"
    );
    assert!(grid
        .kitty_image_store
        .borrow()
        .scale_was_refused(internal, refused_key(5120, 5120)));
    assert!(grid
        .kitty_image_store
        .borrow()
        .scaled_variant(internal, refused_key(5120, 5120))
        .is_none());
    assert_eq!(grid.kitty_image_store.borrow().total_bytes(), retained);
    assert!(grid.placements().is_empty());

    // Cycling 637–640 columns by 320 rows is the same class of destination.
    // None of them may enter scale_rgba.
    for columns in 637u32..=640 {
        let error = place_cells(&mut grid, pane_image_id, internal, 1, columns, 320)
            .expect_err("a cycling destination the store cannot retain");
        assert_eq!(error.code, KittyErrorCode::Einval);
        place_cells(&mut grid, pane_image_id, internal, 1, columns, 320)
            .expect_err("the refused key is remembered");
    }
    assert_eq!(
        grid.kitty_image_store.borrow().scale_count(internal),
        scales_before
    );
    assert_eq!(grid.kitty_image_store.borrow().total_bytes(), retained);
}

fn refused_key(width: usize, height: usize) -> super::super::store::ScaledImageKey {
    super::super::store::ScaledImageKey {
        source: PixelRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
        size: (width, height),
    }
}

#[test]
fn a_small_destination_that_fits_is_cached_and_matches_bytes() {
    let mut grid = test_grid();
    let (pane_image_id, internal) = transmitted(&mut grid, 1, 1);
    let max = grid
        .kitty_image_store
        .borrow()
        .max_synchronous_scaled_bytes(internal);
    assert!(32 * 32 * 4 <= max);
    let key = place_scaled(&mut grid, pane_image_id, internal, 1, 4, 2, 0, 0);
    let first = grid
        .kitty_image_store
        .borrow()
        .scaled_variant(internal, key)
        .expect("cached scaled raster")
        .to_vec();
    assert_eq!(first.len(), 32 * 32 * 4);
    assert_eq!(grid.kitty_image_store.borrow().scale_count(internal), 1);

    place_scaled(&mut grid, pane_image_id, internal, 1, 4, 2, 0, 0);
    let second = grid
        .kitty_image_store
        .borrow()
        .scaled_variant(internal, key)
        .expect("the cached raster is still there")
        .to_vec();
    assert_eq!(second, first);
    assert_eq!(grid.kitty_image_store.borrow().scale_count(internal), 1);
}

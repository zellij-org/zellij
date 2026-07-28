use super::{crop_rgba, scale_rgba, KittyGrid, KittyPlacement};
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
    assert_eq!(chunk.scaled_px, None);
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
    assert_eq!(chunk.scaled_px, Some((20, 40)));
    assert_eq!(chunk.dest_cells, (2, 2));
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
    grid.apply_region_scroll(0, 200, 20, 0, 0);
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
    grid.apply_region_scroll(40, 140, 20, 20, 40);
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
    grid.apply_region_scroll(20, 100, 20, 0, 0);
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
    grid.apply_region_scroll(20, 100, 20, 0, 0);
    assert_eq!(grid.placements.len(), 0);
}

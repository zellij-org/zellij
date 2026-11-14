use super::*;
use crate::render::renderer::rows::lay_out;
use crate::render::renderer::rows::write_slot;
use crate::render::scene::{Rect, RowScene};

fn row(rects: usize) -> RowScene {
    RowScene {
        rects: (0..rects)
            .map(|at| Rect {
                x: at as i32,
                y: 0,
                width: 1,
                height: 1,
                color: [1, 2, 3],
            })
            .collect(),
        glyphs: Vec::new(),
        color_glyphs: Vec::new(),
    }
}

fn solids(rows: &[RowScene]) -> RowVertices {
    let mut buffer = RowVertices::new(SOLID_STRIDE);
    lay_out(&mut buffer, rows, &mut |row, out| {
        push_solid_vertices(out, &row.rects)
    });
    buffer
}

fn live(buffer: &RowVertices, row: usize) -> Vec<f32> {
    let slot = &buffer.slots[row];
    buffer.data[slot.offset..slot.offset + slot.len].to_vec()
}

#[test]
fn the_layout_holds_every_row_in_order_with_room_to_grow() {
    let rows = [row(1), row(0), row(30)];
    let buffer = solids(&rows);
    let mut expected = Vec::new();
    push_solid_vertices(&mut expected, &rows[2].rects);
    assert_eq!(live(&buffer, 2), expected);
    assert!(buffer.slots[0].capacity > buffer.slots[0].len);
    assert_eq!(buffer.slots[1].len, 0);
    for pair in buffer.slots.windows(2) {
        assert_eq!(pair[0].offset + pair[0].capacity, pair[1].offset);
    }
    assert_eq!(
        buffer.data.len(),
        buffer.slots.iter().map(|slot| slot.capacity).sum::<usize>()
    );
}

#[test]
fn a_row_that_grows_inside_its_slot_is_written_in_place() {
    let mut buffer = solids(&[row(1), row(1)]);
    let (offset, capacity) = (buffer.slots[0].offset, buffer.slots[0].capacity);
    let mut vertices = Vec::new();
    push_solid_vertices(&mut vertices, &row(SLOT_QUADS).rects);
    assert_eq!(
        write_slot(&mut buffer, 0, &vertices),
        Some((offset, capacity))
    );
    assert_eq!(live(&buffer, 0), vertices);
    assert!(
        buffer.data[offset + vertices.len()..offset + capacity]
            .iter()
            .all(|value| *value == 0.0),
        "the unused tail of a slot must be a degenerate quad"
    );
}

#[test]
fn a_row_that_outgrows_its_slot_asks_for_a_new_layout() {
    let mut buffer = solids(&[row(1)]);
    let mut vertices = Vec::new();
    push_solid_vertices(&mut vertices, &row(200).rects);
    assert_eq!(write_slot(&mut buffer, 0, &vertices), None);
}

#[test]
fn a_shrinking_row_leaves_no_vertex_of_the_old_content_behind() {
    let mut buffer = solids(&[row(16)]);
    let mut vertices = Vec::new();
    push_solid_vertices(&mut vertices, &row(2).rects);
    write_slot(&mut buffer, 0, &vertices).expect("a smaller row must fit");
    assert_eq!(live(&buffer, 0), vertices);
    assert!(buffer.data[vertices.len()..]
        .iter()
        .all(|value| *value == 0.0));
}

use super::*;
use crate::render::atlas::GlyphAtlas;
use crate::render::renderer::atlas_upload::plan_atlas_upload;
use crate::render::renderer::atlas_upload::AtlasUpload;
use crate::render::renderer::vertices::normalized;

#[test]
fn a_quad_is_two_triangles_sharing_a_diagonal() {
    let corners = quad_corners(0.0, 0.0, 2.0, 4.0);
    assert_eq!(corners.len(), 6);
    assert_eq!(corners[0], corners[3]);
    assert_eq!(corners[2], corners[4]);
}

#[test]
fn colors_are_normalized_to_unit_range() {
    assert_eq!(normalized([0, 128, 255]), [0.0, 128.0 / 255.0, 1.0]);
}

#[test]
fn an_image_quad_samples_the_source_rectangle_it_names() {
    let quad = ImageQuad {
        key: ImageKey::Kitty(1),
        revision: 1,
        x: 10,
        y: 20,
        width: 4,
        height: 5,
        source_x: 2,
        source_y: 5,
        z: 0,
        image: std::rc::Rc::new(crate::emulation::kitty::Image {
            width: 8,
            height: 10,
            pixels: vec![0; 8 * 10 * 4],
        }),
    };
    let mut vertices = Vec::new();
    push_image_vertices(&mut vertices, &quad);

    assert_eq!(vertices.len(), 6 * 7);
    assert_eq!(&vertices[..7], &[10.0, 20.0, 0.25, 0.5, 1.0, 1.0, 1.0]);
    assert_eq!(&vertices[14..21], &[14.0, 25.0, 0.75, 1.0, 1.0, 1.0, 1.0]);
}

fn glyph(width: u32, height: u32) -> crate::text::font::GlyphBitmap {
    crate::text::font::GlyphBitmap {
        left: 0,
        top: 0,
        width,
        height,
        coverage: vec![255; (width * height) as usize],
        content: crate::text::font::GlyphContent::Mask,
    }
}

#[test]
fn a_fresh_or_resized_atlas_texture_is_uploaded_whole() {
    let mut atlas = GlyphAtlas::new(1);
    let size = (atlas.width(), atlas.height());
    assert_eq!(plan_atlas_upload(None, (0, 0), &atlas), AtlasUpload::Whole);
    let stamp = atlas.stamp();
    while atlas.height() == size.1 {
        atlas.insert(&glyph(64, 64)).unwrap();
    }
    assert_eq!(
        plan_atlas_upload(Some(stamp), size, &atlas),
        AtlasUpload::Whole
    );
}

#[test]
fn an_atlas_with_nothing_in_it_is_never_uploaded() {
    let other = GlyphAtlas::new(1).stamp();
    let empty = GlyphAtlas::new(4);
    assert_eq!(empty.height(), 0);
    assert_eq!(
        plan_atlas_upload(None, (0, 0), &empty),
        AtlasUpload::Nothing
    );
    assert_eq!(
        plan_atlas_upload(Some(other), (4096, 256), &empty),
        AtlasUpload::Nothing
    );
    assert_eq!(
        plan_atlas_upload(Some(other), (empty.width(), 0), &empty),
        AtlasUpload::Nothing,
        "another cache's empty atlas must not ask for a sub-upload of zero rows"
    );
}

#[test]
fn an_unchanged_atlas_is_not_uploaded_again() {
    let mut atlas = GlyphAtlas::new(1);
    atlas.insert(&glyph(8, 16)).unwrap();
    let size = (atlas.width(), atlas.height());
    assert_eq!(
        plan_atlas_upload(Some(atlas.stamp()), size, &atlas),
        AtlasUpload::Nothing
    );
}

#[test]
fn a_new_glyph_uploads_only_the_rows_it_landed_on() {
    let mut atlas = GlyphAtlas::new(1);
    for _ in 0..200 {
        atlas.insert(&glyph(8, 16)).unwrap();
    }
    let size = (atlas.width(), atlas.height());
    let stamp = atlas.stamp();
    let entry = atlas.insert(&glyph(8, 16)).unwrap();
    assert_eq!(
        plan_atlas_upload(Some(stamp), size, &atlas),
        AtlasUpload::Rows {
            top: entry.y,
            bottom: entry.y + 16
        }
    );
}

#[test]
fn a_screenful_of_new_glyphs_uploads_a_small_part_of_the_atlas() {
    let mut atlas = GlyphAtlas::new(1);
    let size = (atlas.width(), atlas.height());
    let stamp = atlas.stamp();
    for _ in 0..300 {
        atlas.insert(&glyph(8, 20)).unwrap();
    }
    assert_eq!(
        atlas.height(),
        size.1,
        "the atlas grew, pick a smaller batch"
    );
    let AtlasUpload::Rows { top, bottom } = plan_atlas_upload(Some(stamp), size, &atlas) else {
        panic!("new glyphs must be uploaded as a band of rows");
    };
    assert!(
        (bottom - top) * 4 <= size.1,
        "{} of {} rows were uploaded for one screenful",
        bottom - top,
        size.1
    );
}

#[test]
fn a_texture_last_filled_by_another_atlas_is_uploaded_whole_even_if_that_atlas_changed_later() {
    let mut ours = GlyphAtlas::new(1);
    let mut theirs = GlyphAtlas::new(1);
    ours.insert(&glyph(8, 16)).unwrap();
    theirs.insert(&glyph(8, 16)).unwrap();
    theirs.insert(&glyph(8, 16)).unwrap();
    assert!(theirs.revision() > ours.revision());
    let size = (ours.width(), ours.height());
    assert_eq!((theirs.width(), theirs.height()), size);
    assert_eq!(
        plan_atlas_upload(Some(theirs.stamp()), size, &ours),
        AtlasUpload::Whole,
        "a texture holding another cache's atlas must not be patched with our rows only"
    );
}

#[test]
fn the_byte_view_of_vertices_has_four_bytes_per_float() {
    let vertices = vec![1.0f32, 2.0, 3.0];
    assert_eq!(bytemuck_cast(&vertices).len(), 12);
}

use crate::render::atlas::GlyphAtlas;
use crate::render::scene::ImageQuad;

pub(super) fn push_solid_vertices(vertices: &mut Vec<f32>, rects: &[crate::render::scene::Rect]) {
    for rect in rects {
        let [red, green, blue] = normalized(rect.color);
        let (x0, y0) = (rect.x as f32, rect.y as f32);
        let (x1, y1) = (x0 + rect.width as f32, y0 + rect.height as f32);
        for (x, y) in quad_corners(x0, y0, x1, y1) {
            vertices.extend_from_slice(&[x, y, red, green, blue]);
        }
    }
}

pub(super) fn push_image_vertices(vertices: &mut Vec<f32>, quad: &ImageQuad) {
    let (image_width, image_height) = (quad.image.width as f32, quad.image.height as f32);
    let (x0, y0) = (quad.x as f32, quad.y as f32);
    let (x1, y1) = (x0 + quad.width as f32, y0 + quad.height as f32);
    let (u0, v0) = (
        quad.source_x as f32 / image_width,
        quad.source_y as f32 / image_height,
    );
    let (u1, v1) = (
        (quad.source_x + quad.width) as f32 / image_width,
        (quad.source_y + quad.height) as f32 / image_height,
    );
    for ((x, y), (u, v)) in quad_corners(x0, y0, x1, y1)
        .into_iter()
        .zip(quad_corners(u0, v0, u1, v1))
    {
        vertices.extend_from_slice(&[x, y, u, v, 1.0, 1.0, 1.0]);
    }
}

pub(super) fn push_glyph_vertices(
    vertices: &mut Vec<f32>,
    glyphs: &[crate::render::scene::GlyphQuad],
    atlas: &GlyphAtlas,
) {
    let (atlas_width, atlas_height) = (atlas.width() as f32, atlas.height() as f32);
    for glyph in glyphs {
        let [red, green, blue] = normalized(glyph.color);
        let entry = glyph.entry;
        let (x0, y0) = (glyph.x as f32, glyph.y as f32);
        let (x1, y1) = (x0 + entry.width as f32, y0 + entry.height as f32);
        let (u0, v0) = (entry.x as f32 / atlas_width, entry.y as f32 / atlas_height);
        let (u1, v1) = (
            (entry.x + entry.width) as f32 / atlas_width,
            (entry.y + entry.height) as f32 / atlas_height,
        );
        for ((x, y), (u, v)) in quad_corners(x0, y0, x1, y1)
            .into_iter()
            .zip(quad_corners(u0, v0, u1, v1))
        {
            vertices.extend_from_slice(&[x, y, u, v, red, green, blue]);
        }
    }
}

pub(super) fn bytemuck_cast(vertices: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            vertices.as_ptr() as *const u8,
            std::mem::size_of_val(vertices),
        )
    }
}

pub(super) fn quad_corners(x0: f32, y0: f32, x1: f32, y1: f32) -> [(f32, f32); 6] {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y0), (x1, y1), (x0, y1)]
}

pub(super) fn normalized(color: crate::emulation::color::Srgb) -> [f32; 3] {
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
    ]
}

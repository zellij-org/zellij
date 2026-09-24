use std::collections::HashMap;

use anyhow::{anyhow, Result};
use glow::HasContext;

use crate::atlas::{Atlases, GlyphAtlas};
use crate::graphics::ShaderDialect;
use crate::retained::RetainedScene;
#[cfg(test)]
use crate::scene::Scene;
use crate::scene::{ImageKey, ImageQuad, RowScene};

pub(crate) const SOLID_VERTEX: &str = r#"uniform vec2 u_viewport;
in vec2 a_position;
in vec3 a_color;
out vec3 v_color;
void main() {
    v_color = a_color;
    vec2 ndc = vec2(a_position.x / u_viewport.x * 2.0 - 1.0,
                    1.0 - a_position.y / u_viewport.y * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
}
"#;

pub(crate) const SOLID_FRAGMENT: &str = r#"in vec3 v_color;
out vec4 o_color;
void main() {
    o_color = vec4(v_color, 1.0);
}
"#;

pub(crate) const GLYPH_VERTEX: &str = r#"uniform vec2 u_viewport;
in vec2 a_position;
in vec2 a_texcoord;
in vec3 a_color;
out vec2 v_texcoord;
out vec3 v_color;
void main() {
    v_texcoord = a_texcoord;
    v_color = a_color;
    vec2 ndc = vec2(a_position.x / u_viewport.x * 2.0 - 1.0,
                    1.0 - a_position.y / u_viewport.y * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
}
"#;

pub(crate) const GLYPH_FRAGMENT: &str = r#"uniform sampler2D u_atlas;
in vec2 v_texcoord;
in vec3 v_color;
out vec4 o_color;
void main() {
    float coverage = texture(u_atlas, v_texcoord).r;
    o_color = vec4(v_color, coverage);
}
"#;

pub(crate) const COLOR_FRAGMENT: &str = r#"uniform sampler2D u_atlas;
in vec2 v_texcoord;
in vec3 v_color;
out vec4 o_color;
void main() {
    vec4 texel = texture(u_atlas, v_texcoord);
    o_color = vec4(texel.rgb * v_color, texel.a);
}
"#;

const SOLID_STRIDE: i32 = 5 * 4;
const GLYPH_STRIDE: i32 = 7 * 4;
const VERTICES_PER_QUAD: usize = 6;
const SLOT_QUADS: usize = 4;
const WHOLE_BUFFER_DIVISOR: usize = 2;

struct Slot {
    offset: usize,
    capacity: usize,
    len: usize,
}

struct RowVertices {
    stride_floats: usize,
    data: Vec<f32>,
    slots: Vec<Slot>,
    uploaded: usize,
    atlas_size: (u32, u32),
}

impl RowVertices {
    fn new(stride: i32) -> Self {
        Self {
            stride_floats: stride as usize / 4,
            data: Vec::new(),
            slots: Vec::new(),
            uploaded: 0,
            atlas_size: (0, 0),
        }
    }

    fn quad_floats(&self) -> usize {
        self.stride_floats * VERTICES_PER_QUAD
    }

    fn capacity_for(&self, len: usize) -> usize {
        let quad = self.quad_floats();
        let quads = len.div_ceil(quad).max(1);
        quads.div_ceil(SLOT_QUADS) * SLOT_QUADS * quad
    }

    fn vertices(&self) -> i32 {
        (self.data.len() / self.stride_floats) as i32
    }
}

struct Pass {
    program: glow::Program,
    vertex_array: glow::VertexArray,
    buffer: glow::Buffer,
    viewport: Option<glow::UniformLocation>,
}

struct AtlasTexture {
    texture: glow::Texture,
    size: (u32, u32),
    revision: Option<u64>,
    format: u32,
    internal_format: i32,
}

struct ImageTexture {
    texture: glow::Texture,
    revision: u64,
}

struct ImageBatch {
    key: ImageKey,
    below_text: bool,
    first: i32,
    count: i32,
}

pub struct Renderer {
    gl: glow::Context,
    solid: Pass,
    glyph: Pass,
    color: Pass,
    image: Pass,
    mask_atlas: AtlasTexture,
    color_atlas: AtlasTexture,
    images: HashMap<ImageKey, ImageTexture>,
    #[cfg(test)]
    solid_vertices: Vec<f32>,
    #[cfg(test)]
    glyph_vertices: Vec<f32>,
    #[cfg(test)]
    color_vertices: Vec<f32>,
    image_vertices: Vec<f32>,
    image_batches: Vec<ImageBatch>,
    solid_rows: RowVertices,
    glyph_rows: RowVertices,
    color_rows: RowVertices,
    row_scratch: Vec<f32>,
    retained: Option<u64>,
}

impl Renderer {
    pub fn new(gl: glow::Context) -> Result<Self> {
        let version = gl.version();
        let dialect =
            ShaderDialect::for_context(version.is_embedded, version.major, version.minor)?;
        unsafe {
            let solid_program = link(&gl, dialect, SOLID_VERTEX, SOLID_FRAGMENT)?;
            let glyph_program = link(&gl, dialect, GLYPH_VERTEX, GLYPH_FRAGMENT)?;
            let color_program = link(&gl, dialect, GLYPH_VERTEX, COLOR_FRAGMENT)?;

            let solid = Pass {
                viewport: gl.get_uniform_location(solid_program, "u_viewport"),
                program: solid_program,
                vertex_array: gl.create_vertex_array().map_err(|e| anyhow!(e))?,
                buffer: gl.create_buffer().map_err(|e| anyhow!(e))?,
            };
            gl.bind_vertex_array(Some(solid.vertex_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(solid.buffer));
            attribute(&gl, solid_program, "a_position", 2, SOLID_STRIDE, 0);
            attribute(&gl, solid_program, "a_color", 3, SOLID_STRIDE, 2 * 4);

            let glyph = Pass {
                viewport: gl.get_uniform_location(glyph_program, "u_viewport"),
                program: glyph_program,
                vertex_array: gl.create_vertex_array().map_err(|e| anyhow!(e))?,
                buffer: gl.create_buffer().map_err(|e| anyhow!(e))?,
            };
            gl.bind_vertex_array(Some(glyph.vertex_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(glyph.buffer));
            attribute(&gl, glyph_program, "a_position", 2, GLYPH_STRIDE, 0);
            attribute(&gl, glyph_program, "a_texcoord", 2, GLYPH_STRIDE, 2 * 4);
            attribute(&gl, glyph_program, "a_color", 3, GLYPH_STRIDE, 4 * 4);

            let color = Pass {
                viewport: gl.get_uniform_location(color_program, "u_viewport"),
                program: color_program,
                vertex_array: gl.create_vertex_array().map_err(|e| anyhow!(e))?,
                buffer: gl.create_buffer().map_err(|e| anyhow!(e))?,
            };
            gl.bind_vertex_array(Some(color.vertex_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(color.buffer));
            attribute(&gl, color_program, "a_position", 2, GLYPH_STRIDE, 0);
            attribute(&gl, color_program, "a_texcoord", 2, GLYPH_STRIDE, 2 * 4);
            attribute(&gl, color_program, "a_color", 3, GLYPH_STRIDE, 4 * 4);

            let image = Pass {
                viewport: gl.get_uniform_location(color_program, "u_viewport"),
                program: color_program,
                vertex_array: gl.create_vertex_array().map_err(|e| anyhow!(e))?,
                buffer: gl.create_buffer().map_err(|e| anyhow!(e))?,
            };
            gl.bind_vertex_array(Some(image.vertex_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(image.buffer));
            attribute(&gl, color_program, "a_position", 2, GLYPH_STRIDE, 0);
            attribute(&gl, color_program, "a_texcoord", 2, GLYPH_STRIDE, 2 * 4);
            attribute(&gl, color_program, "a_color", 3, GLYPH_STRIDE, 4 * 4);

            let mask_atlas = atlas_texture(&gl, glow::RED, glow::R8 as i32)?;
            let color_atlas = atlas_texture(&gl, glow::RGBA, glow::RGBA8 as i32)?;

            gl.bind_vertex_array(None);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(
                glow::SRC_ALPHA,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ZERO,
                glow::ONE,
            );
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);

            Ok(Self {
                gl,
                solid,
                glyph,
                color,
                image,
                mask_atlas,
                color_atlas,
                images: HashMap::new(),
                #[cfg(test)]
                solid_vertices: Vec::new(),
                #[cfg(test)]
                glyph_vertices: Vec::new(),
                #[cfg(test)]
                color_vertices: Vec::new(),
                image_vertices: Vec::new(),
                image_batches: Vec::new(),
                solid_rows: RowVertices::new(SOLID_STRIDE),
                glyph_rows: RowVertices::new(GLYPH_STRIDE),
                color_rows: RowVertices::new(GLYPH_STRIDE),
                row_scratch: Vec::new(),
                retained: None,
            })
        }
    }

    #[cfg(test)]
    pub fn gl(&self) -> &glow::Context {
        &self.gl
    }

    #[cfg(test)]
    pub fn draw(&mut self, scene: &Scene, atlases: Atlases<'_>, target: (u32, u32)) {
        upload_atlas(&self.gl, &mut self.mask_atlas, atlases.mask);
        upload_atlas(&self.gl, &mut self.color_atlas, atlases.color);
        self.sync_images(&scene.images, &scene.resident_images);
        self.sync_image_vertices(&scene.images);
        self.build_vertices(scene, &atlases);
        self.retained = None;

        self.clear_target(scene.clear, target);
        let viewport = (target.0 as f32, target.1 as f32);
        unsafe {
            draw_pass(
                &self.gl,
                &self.solid,
                &self.solid_vertices,
                SOLID_STRIDE,
                viewport,
            );
            self.gl.active_texture(glow::TEXTURE0);
            self.draw_images(viewport, true);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.mask_atlas.texture));
            draw_pass(
                &self.gl,
                &self.glyph,
                &self.glyph_vertices,
                GLYPH_STRIDE,
                viewport,
            );
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.color_atlas.texture));
            draw_pass(
                &self.gl,
                &self.color,
                &self.color_vertices,
                GLYPH_STRIDE,
                viewport,
            );
            self.draw_images(viewport, false);
            self.gl.bind_vertex_array(None);
        }
    }

    pub fn draw_retained(
        &mut self,
        scene: &RetainedScene,
        atlases: Atlases<'_>,
        target: (u32, u32),
    ) {
        upload_atlas(&self.gl, &mut self.mask_atlas, atlases.mask);
        upload_atlas(&self.gl, &mut self.color_atlas, atlases.color);

        let everything = self.retained != Some(scene.identity()) || scene.rebuilt_everything();
        self.retained = Some(scene.identity());
        if everything || scene.replaced_images() {
            self.sync_images(scene.images(), scene.resident_images());
            self.sync_image_vertices(scene.images());
        }
        let (rows, rebuilt) = (scene.rows(), scene.rebuilt_rows());
        let mask = (atlases.mask.width(), atlases.mask.height());
        let color = (atlases.color.width(), atlases.color.height());

        sync_rows(
            &self.gl,
            &self.solid,
            &mut self.solid_rows,
            &mut self.row_scratch,
            rows,
            rebuilt,
            everything,
            (0, 0),
            |row, out| push_solid_vertices(out, &row.rects),
        );
        sync_rows(
            &self.gl,
            &self.glyph,
            &mut self.glyph_rows,
            &mut self.row_scratch,
            rows,
            rebuilt,
            everything,
            mask,
            |row, out| push_glyph_vertices(out, &row.glyphs, atlases.mask),
        );
        sync_rows(
            &self.gl,
            &self.color,
            &mut self.color_rows,
            &mut self.row_scratch,
            rows,
            rebuilt,
            everything,
            color,
            |row, out| push_glyph_vertices(out, &row.color_glyphs, atlases.color),
        );

        self.clear_target(scene.clear(), target);
        let viewport = (target.0 as f32, target.1 as f32);
        unsafe {
            draw_rows(&self.gl, &self.solid, &self.solid_rows, viewport);
            self.gl.active_texture(glow::TEXTURE0);
            self.draw_images(viewport, true);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.mask_atlas.texture));
            draw_rows(&self.gl, &self.glyph, &self.glyph_rows, viewport);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.color_atlas.texture));
            draw_rows(&self.gl, &self.color, &self.color_rows, viewport);
            self.draw_images(viewport, false);
            self.gl.bind_vertex_array(None);
        }
    }

    fn clear_target(&self, clear: crate::color::Srgb, target: (u32, u32)) {
        let [red, green, blue] = clear;
        unsafe {
            self.gl.viewport(0, 0, target.0 as i32, target.1 as i32);
            self.gl.clear_color(
                red as f32 / 255.0,
                green as f32 / 255.0,
                blue as f32 / 255.0,
                1.0,
            );
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    fn sync_image_vertices(&mut self, images: &[ImageQuad]) {
        let floats_per_vertex = GLYPH_STRIDE as usize / 4;
        self.image_vertices.clear();
        self.image_batches.clear();
        let mut start = 0usize;
        while start < images.len() {
            let (key, below_text) = (images[start].key, images[start].z < 0);
            let first = (self.image_vertices.len() / floats_per_vertex) as i32;
            let mut end = start;
            while end < images.len() && images[end].key == key && (images[end].z < 0) == below_text
            {
                push_image_vertices(&mut self.image_vertices, &images[end]);
                end += 1;
            }
            self.image_batches.push(ImageBatch {
                key,
                below_text,
                first,
                count: (self.image_vertices.len() / floats_per_vertex) as i32 - first,
            });
            start = end;
        }
        if self.image_vertices.is_empty() {
            return;
        }
        unsafe {
            self.gl.bind_vertex_array(Some(self.image.vertex_array));
            self.gl
                .bind_buffer(glow::ARRAY_BUFFER, Some(self.image.buffer));
            self.gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck_cast(&self.image_vertices),
                glow::DYNAMIC_DRAW,
            );
        }
    }

    unsafe fn draw_images(&mut self, viewport: (f32, f32), below_text: bool) {
        if !self
            .image_batches
            .iter()
            .any(|batch| batch.below_text == below_text)
        {
            return;
        }
        self.gl.use_program(Some(self.image.program));
        self.gl
            .uniform_2_f32(self.image.viewport.as_ref(), viewport.0, viewport.1);
        self.gl.bind_vertex_array(Some(self.image.vertex_array));
        self.gl
            .bind_buffer(glow::ARRAY_BUFFER, Some(self.image.buffer));
        for batch in &self.image_batches {
            if batch.below_text != below_text || batch.count == 0 {
                continue;
            }
            let Some(image) = self.images.get(&batch.key) else {
                continue;
            };
            self.gl.bind_texture(glow::TEXTURE_2D, Some(image.texture));
            self.gl
                .draw_arrays(glow::TRIANGLES, batch.first, batch.count);
        }
    }

    fn sync_images(&mut self, images: &[ImageQuad], resident: &[ImageKey]) {
        let stale: Vec<ImageKey> = self
            .images
            .keys()
            .copied()
            .filter(|key| resident.binary_search(key).is_err())
            .collect();
        for key in stale {
            if let Some(image) = self.images.remove(&key) {
                unsafe { self.gl.delete_texture(image.texture) };
            }
        }

        for quad in images {
            let current = self.images.get(&quad.key).map(|image| image.revision);
            if current == Some(quad.revision) {
                continue;
            }
            match self.upload_image(quad) {
                Ok(image) => {
                    if let Some(previous) = self.images.insert(quad.key, image) {
                        unsafe { self.gl.delete_texture(previous.texture) };
                    }
                },
                Err(e) => eprintln!("zellij-window: image upload failed: {}", e),
            }
        }
    }

    fn upload_image(&self, quad: &ImageQuad) -> Result<ImageTexture> {
        unsafe {
            let texture = self.gl.create_texture().map_err(|e| anyhow!(e))?;
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            for (parameter, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32),
                (glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32),
            ] {
                self.gl
                    .tex_parameter_i32(glow::TEXTURE_2D, parameter, value);
            }
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                quad.image.width as i32,
                quad.image.height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&quad.image.pixels)),
            );
            Ok(ImageTexture {
                texture,
                revision: quad.revision,
            })
        }
    }

    #[cfg(test)]
    fn build_vertices(&mut self, scene: &Scene, atlases: &Atlases<'_>) {
        self.solid_vertices.clear();
        self.glyph_vertices.clear();
        self.color_vertices.clear();

        push_solid_vertices(&mut self.solid_vertices, &scene.rects);
        push_glyph_vertices(&mut self.glyph_vertices, &scene.glyphs, atlases.mask);
        push_glyph_vertices(&mut self.color_vertices, &scene.color_glyphs, atlases.color);
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_rows(
    gl: &glow::Context,
    pass: &Pass,
    buffer: &mut RowVertices,
    scratch: &mut Vec<f32>,
    rows: &[RowScene],
    rebuilt: &[usize],
    everything: bool,
    atlas_size: (u32, u32),
    mut convert: impl FnMut(&RowScene, &mut Vec<f32>),
) {
    let relaid = everything || buffer.slots.len() != rows.len() || buffer.atlas_size != atlas_size;
    buffer.atlas_size = atlas_size;
    if relaid || rebuilt.len() * WHOLE_BUFFER_DIVISOR > rows.len() {
        lay_out(buffer, rows, &mut convert);
        upload_whole(gl, pass, buffer);
        return;
    }
    if rebuilt.is_empty() {
        return;
    }

    let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(rebuilt.len());
    for row in rebuilt {
        scratch.clear();
        convert(&rows[*row], scratch);
        match write_slot(buffer, *row, scratch) {
            Some(range) => ranges.push(range),
            None => {
                lay_out(buffer, rows, &mut convert);
                upload_whole(gl, pass, buffer);
                return;
            },
        }
    }

    if buffer.uploaded != buffer.data.len() {
        upload_whole(gl, pass, buffer);
        return;
    }
    unsafe {
        gl.bind_vertex_array(Some(pass.vertex_array));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(pass.buffer));
        for (offset, len) in ranges {
            gl.buffer_sub_data_u8_slice(
                glow::ARRAY_BUFFER,
                (offset * 4) as i32,
                bytemuck_cast(&buffer.data[offset..offset + len]),
            );
        }
    }
}

fn lay_out(
    buffer: &mut RowVertices,
    rows: &[RowScene],
    convert: &mut impl FnMut(&RowScene, &mut Vec<f32>),
) {
    let held: Vec<usize> = if buffer.slots.len() == rows.len() {
        buffer.slots.iter().map(|slot| slot.capacity).collect()
    } else {
        vec![0; rows.len()]
    };
    let mut data = std::mem::take(&mut buffer.data);
    data.clear();
    buffer.slots.clear();
    for (row, scene) in rows.iter().enumerate() {
        let offset = data.len();
        convert(scene, &mut data);
        let len = data.len() - offset;
        let capacity = buffer.capacity_for(len).max(held[row]);
        data.resize(offset + capacity, 0.0);
        buffer.slots.push(Slot {
            offset,
            capacity,
            len,
        });
    }
    buffer.data = data;
}

fn write_slot(buffer: &mut RowVertices, row: usize, vertices: &[f32]) -> Option<(usize, usize)> {
    let slot = buffer.slots.get_mut(row)?;
    if vertices.len() > slot.capacity {
        return None;
    }
    let (offset, capacity) = (slot.offset, slot.capacity);
    slot.len = vertices.len();
    buffer.data[offset..offset + vertices.len()].copy_from_slice(vertices);
    buffer.data[offset + vertices.len()..offset + capacity].fill(0.0);
    Some((offset, capacity))
}

fn upload_whole(gl: &glow::Context, pass: &Pass, buffer: &mut RowVertices) {
    buffer.uploaded = buffer.data.len();
    if buffer.data.is_empty() {
        return;
    }
    unsafe {
        gl.bind_vertex_array(Some(pass.vertex_array));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(pass.buffer));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            bytemuck_cast(&buffer.data),
            glow::DYNAMIC_DRAW,
        );
    }
}

unsafe fn draw_rows(gl: &glow::Context, pass: &Pass, buffer: &RowVertices, viewport: (f32, f32)) {
    if buffer.data.is_empty() {
        return;
    }
    gl.use_program(Some(pass.program));
    gl.uniform_2_f32(pass.viewport.as_ref(), viewport.0, viewport.1);
    gl.bind_vertex_array(Some(pass.vertex_array));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(pass.buffer));
    gl.draw_arrays(glow::TRIANGLES, 0, buffer.vertices());
}

fn push_solid_vertices(vertices: &mut Vec<f32>, rects: &[crate::scene::Rect]) {
    for rect in rects {
        let [red, green, blue] = normalized(rect.color);
        let (x0, y0) = (rect.x as f32, rect.y as f32);
        let (x1, y1) = (x0 + rect.width as f32, y0 + rect.height as f32);
        for (x, y) in quad_corners(x0, y0, x1, y1) {
            vertices.extend_from_slice(&[x, y, red, green, blue]);
        }
    }
}

fn push_image_vertices(vertices: &mut Vec<f32>, quad: &ImageQuad) {
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

fn push_glyph_vertices(
    vertices: &mut Vec<f32>,
    glyphs: &[crate::scene::GlyphQuad],
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

unsafe fn atlas_texture(
    gl: &glow::Context,
    format: u32,
    internal_format: i32,
) -> Result<AtlasTexture> {
    let texture = gl.create_texture().map_err(|e| anyhow!(e))?;
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    for (parameter, value) in [
        (glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32),
        (glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32),
        (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32),
        (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32),
    ] {
        gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, value);
    }

    Ok(AtlasTexture {
        texture,
        size: (0, 0),
        revision: None,
        format,
        internal_format,
    })
}

#[cfg(test)]
impl Renderer {
    pub fn image_textures(&self) -> usize {
        self.images.len()
    }

    pub fn forget_uploads(&mut self) {
        for (_, image) in self.images.drain() {
            unsafe { self.gl.delete_texture(image.texture) };
        }
        self.mask_atlas.revision = None;
        self.color_atlas.revision = None;
        self.retained = None;
        self.image_batches.clear();
    }
}

fn upload_atlas(gl: &glow::Context, target: &mut AtlasTexture, atlas: &GlyphAtlas) {
    if target.revision == Some(atlas.revision()) {
        return;
    }
    let size = (atlas.width(), atlas.height());
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(target.texture));
        if size == target.size {
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                size.0 as i32,
                size.1 as i32,
                target.format,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(atlas.coverage())),
            );
        } else {
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                target.internal_format,
                size.0 as i32,
                size.1 as i32,
                0,
                target.format,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(atlas.coverage())),
            );
        }
    }
    target.size = size;
    target.revision = Some(atlas.revision());
}

unsafe fn draw_pass(
    gl: &glow::Context,
    pass: &Pass,
    vertices: &[f32],
    stride: i32,
    viewport: (f32, f32),
) {
    if vertices.is_empty() {
        return;
    }
    gl.use_program(Some(pass.program));
    gl.uniform_2_f32(pass.viewport.as_ref(), viewport.0, viewport.1);
    gl.bind_vertex_array(Some(pass.vertex_array));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(pass.buffer));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck_cast(vertices),
        glow::DYNAMIC_DRAW,
    );
    let count = vertices.len() as i32 / (stride / 4);
    gl.draw_arrays(glow::TRIANGLES, 0, count);
}

fn bytemuck_cast(vertices: &[f32]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            vertices.as_ptr() as *const u8,
            std::mem::size_of_val(vertices),
        )
    }
}

fn quad_corners(x0: f32, y0: f32, x1: f32, y1: f32) -> [(f32, f32); 6] {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y0), (x1, y1), (x0, y1)]
}

fn normalized(color: crate::color::Srgb) -> [f32; 3] {
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
    ]
}

unsafe fn attribute(
    gl: &glow::Context,
    program: glow::Program,
    name: &str,
    size: i32,
    stride: i32,
    offset: i32,
) {
    let location = gl.get_attrib_location(program, name);
    if let Some(location) = location {
        gl.enable_vertex_attrib_array(location);
        gl.vertex_attrib_pointer_f32(location, size, glow::FLOAT, false, stride, offset);
    }
}

unsafe fn link(
    gl: &glow::Context,
    dialect: ShaderDialect,
    vertex: &str,
    fragment: &str,
) -> Result<glow::Program> {
    let program = gl.create_program().map_err(|e| anyhow!(e))?;
    let mut shaders = Vec::new();

    for (kind, source) in [
        (glow::VERTEX_SHADER, vertex),
        (glow::FRAGMENT_SHADER, fragment),
    ] {
        let shader = gl.create_shader(kind).map_err(|e| anyhow!(e))?;
        gl.shader_source(shader, &dialect.source(source));
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            return Err(anyhow!(
                "shader compilation failed: {}",
                gl.get_shader_info_log(shader)
            ));
        }
        gl.attach_shader(program, shader);
        shaders.push(shader);
    }

    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        return Err(anyhow!(
            "program link failed: {}",
            gl.get_program_info_log(program)
        ));
    }
    for shader in shaders {
        gl.detach_shader(program, shader);
        gl.delete_shader(shader);
    }

    Ok(program)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            image: std::rc::Rc::new(crate::kitty::Image {
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

    #[test]
    fn the_byte_view_of_vertices_has_four_bytes_per_float() {
        let vertices = vec![1.0f32, 2.0, 3.0];
        assert_eq!(bytemuck_cast(&vertices).len(), 12);
    }
}

#[cfg(test)]
mod row_buffers {
    use super::*;
    use crate::scene::{Rect, RowScene};

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
}

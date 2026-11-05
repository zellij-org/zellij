use std::collections::HashMap;

use anyhow::{anyhow, Result};
use glow::HasContext;

use crate::render::atlas::Atlases;
use crate::render::graphics::ShaderDialect;
use crate::render::retained::RetainedScene;
use crate::render::scene::{ImageKey, ImageQuad};

mod atlas_upload;
mod images;
pub(crate) mod programs;
mod rows;
mod vertices;

use atlas_upload::{atlas_texture, upload_atlas, AtlasTexture};
use images::{ImageBatch, ImageTexture};
use programs::{
    attribute, draw_pass, link, premultiplied_blending, solid_pass, textured_pass, COLOR_FRAGMENT,
    GLYPH_FRAGMENT, GLYPH_VERTEX, SOLID_FRAGMENT, SOLID_VERTEX,
};
use rows::{draw_rows, sync_rows, RowChanges, RowVertices};
use vertices::{
    bytemuck_cast, push_glyph_vertices, push_image_vertices, push_solid_vertices, quad_corners,
};

const SOLID_STRIDE: i32 = 5 * 4;
const GLYPH_STRIDE: i32 = 7 * 4;
const VERTICES_PER_QUAD: usize = 6;
const SLOT_QUADS: usize = 4;
const WHOLE_BUFFER_DIVISOR: usize = 2;

#[derive(Debug, Clone, Copy, Default)]
pub struct Frame<'a> {
    pub origin: (i32, i32),
    pub margins: &'a [crate::render::scene::Rect],
    pub layer: Option<&'a crate::render::scroll_animation::ScrollLayer>,
}

struct Pass {
    program: glow::Program,
    vertex_array: glow::VertexArray,
    buffer: glow::Buffer,
    viewport: Option<glow::UniformLocation>,
    origin: Option<glow::UniformLocation>,
}

pub struct Renderer {
    gl: glow::Context,
    solid: Pass,
    glyph: Pass,
    color: Pass,
    image: Pass,
    fade: Pass,
    layer_solid: Pass,
    layer_glyph: Pass,
    layer_color: Pass,
    layer_image: Pass,
    layer_vertices: Vec<f32>,
    mask_atlas: AtlasTexture,
    color_atlas: AtlasTexture,
    images: HashMap<ImageKey, ImageTexture>,
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
                origin: gl.get_uniform_location(solid_program, "u_origin"),
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
                origin: gl.get_uniform_location(glyph_program, "u_origin"),
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
                origin: gl.get_uniform_location(color_program, "u_origin"),
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
                origin: gl.get_uniform_location(color_program, "u_origin"),
                program: color_program,
                vertex_array: gl.create_vertex_array().map_err(|e| anyhow!(e))?,
                buffer: gl.create_buffer().map_err(|e| anyhow!(e))?,
            };
            gl.bind_vertex_array(Some(image.vertex_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(image.buffer));
            attribute(&gl, color_program, "a_position", 2, GLYPH_STRIDE, 0);
            attribute(&gl, color_program, "a_texcoord", 2, GLYPH_STRIDE, 2 * 4);
            attribute(&gl, color_program, "a_color", 3, GLYPH_STRIDE, 4 * 4);

            let fade = Pass {
                viewport: gl.get_uniform_location(solid_program, "u_viewport"),
                origin: gl.get_uniform_location(solid_program, "u_origin"),
                program: solid_program,
                vertex_array: gl.create_vertex_array().map_err(|e| anyhow!(e))?,
                buffer: gl.create_buffer().map_err(|e| anyhow!(e))?,
            };
            gl.bind_vertex_array(Some(fade.vertex_array));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(fade.buffer));
            attribute(&gl, solid_program, "a_position", 2, SOLID_STRIDE, 0);
            attribute(&gl, solid_program, "a_color", 3, SOLID_STRIDE, 2 * 4);

            let layer_solid = solid_pass(&gl, solid_program)?;
            let layer_glyph = textured_pass(&gl, glyph_program)?;
            let layer_color = textured_pass(&gl, color_program)?;
            let layer_image = textured_pass(&gl, color_program)?;

            let mask_atlas = atlas_texture(&gl, glow::RED, glow::R8 as i32)?;
            let color_atlas = atlas_texture(&gl, glow::RGBA, glow::RGBA8 as i32)?;

            gl.bind_vertex_array(None);
            gl.disable(glow::DEPTH_TEST);
            gl.enable(glow::BLEND);
            premultiplied_blending(&gl);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);

            Ok(Self {
                gl,
                solid,
                glyph,
                color,
                image,
                fade,
                layer_solid,
                layer_glyph,
                layer_color,
                layer_image,
                layer_vertices: Vec::new(),
                mask_atlas,
                color_atlas,
                images: HashMap::new(),
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

    pub fn draw_retained(
        &mut self,
        scene: &RetainedScene,
        atlases: Atlases<'_>,
        target: (u32, u32),
        frame: Frame<'_>,
    ) {
        upload_atlas(&self.gl, &mut self.mask_atlas, atlases.mask);
        upload_atlas(&self.gl, &mut self.color_atlas, atlases.color);

        let everything = self.retained != Some(scene.identity()) || scene.rebuilt_everything();
        self.retained = Some(scene.identity());
        if everything || scene.replaced_images() {
            self.sync_images(scene.all_images(), scene.resident_images());
            self.sync_image_vertices(scene.images());
        }
        let layer = frame.layer.filter(|layer| !layer.is_empty());
        let (rows, rebuilt) = (scene.rows(), scene.rebuilt_rows());
        let mask = (atlases.mask.width(), atlases.mask.height());
        let color = (atlases.color.width(), atlases.color.height());

        let changes = RowChanges {
            rows,
            rebuilt,
            everything,
        };
        sync_rows(
            &self.gl,
            &self.solid,
            &mut self.solid_rows,
            &mut self.row_scratch,
            changes,
            (0, 0),
            |row, out| push_solid_vertices(out, &row.rects),
        );
        sync_rows(
            &self.gl,
            &self.glyph,
            &mut self.glyph_rows,
            &mut self.row_scratch,
            changes,
            mask,
            |row, out| push_glyph_vertices(out, &row.glyphs, atlases.mask),
        );
        sync_rows(
            &self.gl,
            &self.color,
            &mut self.color_rows,
            &mut self.row_scratch,
            changes,
            color,
            |row, out| push_glyph_vertices(out, &row.color_glyphs, atlases.color),
        );

        let transparency = scene.transparency();
        self.clear_target(scene.clear(), transparency.clear_alpha(), target);
        let viewport = (target.0 as f32, target.1 as f32);
        let origin = (frame.origin.0 as f32, frame.origin.1 as f32);
        unsafe {
            if !frame.margins.is_empty() {
                let mut margins = Vec::with_capacity(
                    frame.margins.len() * VERTICES_PER_QUAD * SOLID_STRIDE as usize / 4,
                );
                push_solid_vertices(&mut margins, frame.margins);
                draw_pass(
                    &self.gl,
                    &self.fade,
                    &margins,
                    SOLID_STRIDE,
                    viewport,
                    origin,
                );
            }
            draw_rows(&self.gl, &self.solid, &self.solid_rows, viewport, origin);
            if let Some(layer) = layer {
                self.layer_vertices.clear();
                push_solid_vertices(&mut self.layer_vertices, &layer.rects);
                draw_pass(
                    &self.gl,
                    &self.layer_solid,
                    &self.layer_vertices,
                    SOLID_STRIDE,
                    viewport,
                    origin,
                );
            }
            self.gl.active_texture(glow::TEXTURE0);
            self.draw_images(viewport, origin, true);
            if let Some(layer) = layer {
                self.draw_layer_images(&layer.images, viewport, origin, true);
            }
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.mask_atlas.texture));
            draw_rows(&self.gl, &self.glyph, &self.glyph_rows, viewport, origin);
            if let Some(layer) = layer {
                self.layer_vertices.clear();
                push_glyph_vertices(&mut self.layer_vertices, &layer.glyphs, atlases.mask);
                draw_pass(
                    &self.gl,
                    &self.layer_glyph,
                    &self.layer_vertices,
                    GLYPH_STRIDE,
                    viewport,
                    origin,
                );
            }
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.color_atlas.texture));
            draw_rows(&self.gl, &self.color, &self.color_rows, viewport, origin);
            if let Some(layer) = layer {
                self.layer_vertices.clear();
                push_glyph_vertices(&mut self.layer_vertices, &layer.color_glyphs, atlases.color);
                draw_pass(
                    &self.gl,
                    &self.layer_color,
                    &self.layer_vertices,
                    GLYPH_STRIDE,
                    viewport,
                    origin,
                );
            }
            self.draw_images(viewport, origin, false);
            if let Some(layer) = layer {
                self.draw_layer_images(&layer.images, viewport, origin, false);
            }
            if let Some(opacity) = transparency.fade() {
                self.fade_everything(opacity, viewport);
            }
            self.gl.bind_vertex_array(None);
        }
    }

    unsafe fn fade_everything(&self, opacity: f32, viewport: (f32, f32)) {
        let (width, height) = viewport;
        let mut vertices = Vec::with_capacity(VERTICES_PER_QUAD * SOLID_STRIDE as usize / 4);
        for (x, y) in quad_corners(0.0, 0.0, width, height) {
            vertices.extend_from_slice(&[x, y, 0.0, 0.0, 0.0]);
        }
        self.gl.blend_color(0.0, 0.0, 0.0, opacity);
        self.gl.blend_func_separate(
            glow::ZERO,
            glow::CONSTANT_ALPHA,
            glow::ZERO,
            glow::CONSTANT_ALPHA,
        );
        draw_pass(
            &self.gl,
            &self.fade,
            &vertices,
            SOLID_STRIDE,
            viewport,
            (0.0, 0.0),
        );
        premultiplied_blending(&self.gl);
    }

    fn clear_target(&self, clear: crate::emulation::color::Srgb, alpha: f32, target: (u32, u32)) {
        let [red, green, blue] = clear;
        unsafe {
            self.gl.viewport(0, 0, target.0 as i32, target.1 as i32);
            self.gl.clear_color(
                red as f32 / 255.0 * alpha,
                green as f32 / 255.0 * alpha,
                blue as f32 / 255.0 * alpha,
                alpha,
            );
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    unsafe fn draw_images(&mut self, viewport: (f32, f32), origin: (f32, f32), below_text: bool) {
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
        self.gl
            .uniform_2_f32(self.image.origin.as_ref(), origin.0, origin.1);
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

    unsafe fn draw_layer_images(
        &mut self,
        images: &[ImageQuad],
        viewport: (f32, f32),
        origin: (f32, f32),
        below_text: bool,
    ) {
        for quad in images {
            if (quad.z < 0) != below_text {
                continue;
            }
            let Some(texture) = self.images.get(&quad.key).map(|image| image.texture) else {
                continue;
            };
            self.layer_vertices.clear();
            push_image_vertices(&mut self.layer_vertices, quad);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            draw_pass(
                &self.gl,
                &self.layer_image,
                &self.layer_vertices,
                GLYPH_STRIDE,
                viewport,
                origin,
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod test_helpers;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod row_buffers;

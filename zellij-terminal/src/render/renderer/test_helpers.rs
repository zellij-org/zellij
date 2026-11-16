use super::*;
use crate::render::scene::test_helpers::Scene;

impl Renderer {
    pub fn gl(&self) -> &glow::Context {
        &self.gl
    }

    pub fn draw(&mut self, scene: &Scene, atlases: Atlases<'_>, target: (u32, u32)) {
        upload_atlas(&self.gl, &mut self.mask_atlas, atlases.mask);
        upload_atlas(&self.gl, &mut self.color_atlas, atlases.color);
        self.sync_images(&scene.images, &scene.resident_images);
        self.sync_image_vertices(&scene.images);
        let mut solid_vertices = Vec::new();
        let mut glyph_vertices = Vec::new();
        let mut color_vertices = Vec::new();
        push_solid_vertices(&mut solid_vertices, &scene.rects);
        push_glyph_vertices(&mut glyph_vertices, &scene.glyphs, atlases.mask);
        push_glyph_vertices(&mut color_vertices, &scene.color_glyphs, atlases.color);
        self.retained = None;

        self.clear_target(
            scene.clear,
            crate::render::scene::Transparency::OPAQUE.clear_alpha(),
            target,
        );
        let viewport = (target.0 as f32, target.1 as f32);
        let origin = (0.0, 0.0);
        unsafe {
            draw_pass(
                &self.gl,
                &self.solid,
                &solid_vertices,
                SOLID_STRIDE,
                viewport,
                origin,
            );
            self.gl.active_texture(glow::TEXTURE0);
            self.draw_images(viewport, origin, true);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.mask_atlas.texture));
            draw_pass(
                &self.gl,
                &self.glyph,
                &glyph_vertices,
                GLYPH_STRIDE,
                viewport,
                origin,
            );
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.color_atlas.texture));
            draw_pass(
                &self.gl,
                &self.color,
                &color_vertices,
                GLYPH_STRIDE,
                viewport,
                origin,
            );
            self.draw_images(viewport, origin, false);
            self.gl.bind_vertex_array(None);
        }
    }

    pub fn image_textures(&self) -> usize {
        self.images.len()
    }

    pub fn forget_uploads(&mut self) {
        for (_, image) in self.images.drain() {
            unsafe { self.gl.delete_texture(image.texture) };
        }
        self.mask_atlas.uploaded = None;
        self.color_atlas.uploaded = None;
        self.retained = None;
        self.image_batches.clear();
    }
}

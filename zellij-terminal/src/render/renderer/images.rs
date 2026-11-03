use crate::render::renderer::bytemuck_cast;
use crate::render::renderer::push_image_vertices;
use crate::render::renderer::Renderer;
use crate::render::renderer::GLYPH_STRIDE;
use crate::render::scene::{ImageKey, ImageQuad};
use anyhow::{anyhow, Result};
use glow::HasContext;

pub(super) struct ImageTexture {
    pub(super) texture: glow::Texture,
    pub(super) revision: u64,
}

pub(super) struct ImageBatch {
    pub(super) key: ImageKey,
    pub(super) below_text: bool,
    pub(super) first: i32,
    pub(super) count: i32,
}

impl Renderer {
    pub(super) fn sync_image_vertices(&mut self, images: &[ImageQuad]) {
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

    pub(super) fn sync_images(&mut self, images: &[ImageQuad], resident: &[ImageKey]) {
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
                Err(e) => report!("image upload failed: {}", e),
            }
        }
    }

    pub(super) fn upload_image(&self, quad: &ImageQuad) -> Result<ImageTexture> {
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
}

use super::*;
use crate::render::scene::test_helpers::Scene;

impl RetainedScene {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn flatten(&self) -> Scene {
        let mut scene = Scene {
            width: self.width,
            height: self.height,
            clear: self.clear,
            rects: Vec::new(),
            glyphs: Vec::new(),
            color_glyphs: Vec::new(),
            images: self.images().to_vec(),
            resident_images: self.resident_images.clone(),
        };
        for row in &self.rows {
            scene.rects.extend_from_slice(&row.rects);
            scene.glyphs.extend_from_slice(&row.glyphs);
            scene.color_glyphs.extend_from_slice(&row.color_glyphs);
        }
        scene
    }
}

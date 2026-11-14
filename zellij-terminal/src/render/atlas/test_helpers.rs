use super::*;

impl GlyphAtlas {
    pub fn bytes(&self) -> usize {
        self.coverage.len()
    }
}

impl GlyphCache {
    pub fn tiny(fonts: FontStack) -> Self {
        const SIDE: u32 = 64;
        Self::bounded(
            fonts,
            GlyphAtlas::sized(1, SIDE, SIDE, (SIDE * SIDE) as usize),
            GlyphAtlas::sized(4, SIDE, SIDE, (SIDE * SIDE * 4) as usize),
        )
    }

    pub fn bytes(&self) -> usize {
        self.mask.bytes() + self.color.bytes()
    }

    pub fn entries(&self) -> usize {
        self.entries.len() + self.clusters.len()
    }

    pub fn cluster_entries(&self) -> usize {
        self.clusters.len()
    }

    pub fn flushes(&self) -> u64 {
        self.flushes
    }

    pub fn nominal_of(&mut self, style: FaceStyle, character: char) -> Option<GlyphId> {
        self.fonts.lookup(style, character).map(|glyph| glyph.glyph)
    }

    pub fn shaped_runs(&self) -> usize {
        self.fonts.shaped_runs()
    }

    pub fn cluster_font(&mut self, style: FaceStyle, text: &str) -> Option<FontId> {
        self.fonts.cluster_font(style, text)
    }

    pub fn glyph(&mut self, style: FaceStyle, character: char, budget: u32) -> Lookup {
        self.glyph_with_room(style, character, budget, 0)
    }
}

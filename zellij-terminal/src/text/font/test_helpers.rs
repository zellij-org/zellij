use super::*;

impl FontStack {
    pub fn embedded(size: f32) -> Result<Self> {
        Self::new(size, false)
    }

    pub fn new(size: f32, system_fonts: bool) -> Result<Self> {
        Self::build(&FontOptions {
            family: None,
            size,
            system_fonts,
            ..FontOptions::default()
        })
    }

    pub fn build_with(options: &FontOptions, discovery: Option<Discovery>) -> Result<Self> {
        Self::assemble(options, discovery)
    }

    pub fn shaped_runs(&self) -> usize {
        self.shaped
            .iter()
            .filter(|((_, shaping), _)| *shaping == Shaping::Run)
            .map(|(_, runs)| runs.len())
            .sum()
    }

    pub fn shaped_clusters(&self) -> usize {
        self.shaped
            .iter()
            .filter(|((_, shaping), _)| *shaping == Shaping::Cluster)
            .map(|(_, runs)| runs.len())
            .sum()
    }

    pub fn has_discovery(&self) -> bool {
        self.discovery.is_some()
    }

    pub fn rasterize(&mut self, glyph: Glyph, budget: u32) -> Option<GlyphBitmap> {
        self.rasterize_with_room(glyph, budget, 0)
    }
}

pub fn pixels_to_points(pixels: f32) -> f32 {
    pixels * POINTS_PER_INCH / PIXELS_PER_INCH
}

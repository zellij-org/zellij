use crate::text::font::CellMetrics;
use crate::text::font::FontStack;
use crate::text::font::Glyph;
use crate::text::font::GlyphBitmap;
use crate::text::font::GlyphContent;
use crate::text::font::FALLBACK_OVERHANG;
use crate::text::font::FALLBACK_SOURCES;
use crate::text::font::PRIMARY_SOURCES;
use swash::scale::image::Content;
use swash::scale::{Render, Source};
use swash::zeno::Format;
use swash::FontRef;
use swash::GlyphId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Fit {
    pub(super) box_width: u32,
    pub(super) cell_width: u32,
    pub(super) spill_width: u32,
    pub(super) cell_height: u32,
    pub(super) baseline: u32,
}

impl Fit {
    pub(super) fn new(metrics: CellMetrics, budget: u32) -> Self {
        Self {
            box_width: metrics.width * budget,
            cell_width: metrics.width,
            spill_width: 0,
            cell_height: metrics.height,
            baseline: metrics.baseline,
        }
    }

    pub(super) fn spilling(self, cells: u32) -> Self {
        Self {
            spill_width: self.cell_width * cells,
            ..self
        }
    }

    pub(super) fn room(&self) -> f32 {
        if self.spill_width > 0 {
            (self.box_width + self.spill_width) as f32
        } else {
            self.box_width as f32 * FALLBACK_OVERHANG
        }
    }

    pub(super) fn widest(&self, content: GlyphContent) -> u32 {
        match content {
            GlyphContent::Mask => (self.room().floor() as u32).max(self.box_width),
            GlyphContent::Color => self.box_width,
        }
    }

    pub(super) fn advance_scale(&self, advance: f32) -> f32 {
        if advance.is_finite() && advance > self.room() {
            self.room() / advance
        } else {
            1.0
        }
    }

    pub(super) fn ink_scale(&self, bitmap: &GlyphBitmap) -> f32 {
        (self.widest(bitmap.content) as f32 / bitmap.width as f32)
            .min(self.cell_height as f32 / bitmap.height as f32)
            .min(1.0)
    }

    pub(super) fn place(&self, bitmap: GlyphBitmap, advance: f32) -> GlyphBitmap {
        let widest = self.widest(bitmap.content);
        let bitmap = confine(bitmap, widest, self.cell_height);
        let span = if self.spill_width > 0 && advance > self.box_width as f32 {
            self.box_width + self.spill_width
        } else {
            self.box_width
        };
        let centring = if advance.is_finite() {
            ((span as f32 - advance) / 2.0).round() as i32
        } else {
            0
        };
        let mut left = centring + bitmap.left;
        if bitmap.content == GlyphContent::Color {
            left = left.clamp(0, self.box_width.saturating_sub(bitmap.width) as i32);
        }
        let highest = self.baseline as i32;
        let lowest = highest - self.cell_height as i32 + bitmap.height as i32;
        GlyphBitmap {
            left,
            top: bitmap.top.clamp(lowest.min(highest), highest),
            ..bitmap
        }
    }
}

pub(super) fn confine(bitmap: GlyphBitmap, max_width: u32, max_height: u32) -> GlyphBitmap {
    if bitmap.width <= max_width && bitmap.height <= max_height {
        return bitmap;
    }
    let scale =
        (max_width as f32 / bitmap.width as f32).min(max_height as f32 / bitmap.height as f32);
    let width = ((bitmap.width as f32 * scale) as u32).clamp(1, max_width);
    let height = ((bitmap.height as f32 * scale) as u32).clamp(1, max_height);
    let coverage = downscale(
        &bitmap.coverage,
        bitmap.width,
        bitmap.height,
        bitmap.content.channels(),
        width,
        height,
    );
    GlyphBitmap {
        left: (bitmap.left as f32 * scale).round() as i32,
        top: (bitmap.top as f32 * scale).round() as i32,
        width,
        height,
        coverage,
        content: bitmap.content,
    }
}

pub(super) fn crop_to_cell(bitmap: GlyphBitmap, metrics: CellMetrics) -> Option<GlyphBitmap> {
    let highest = metrics.baseline as i32;
    let lowest = highest - metrics.height as i32;
    let top = bitmap.top.min(highest);
    let bottom = (bitmap.top - bitmap.height as i32).max(lowest);
    if top <= bottom {
        return None;
    }
    if top == bitmap.top && bottom == bitmap.top - bitmap.height as i32 {
        return Some(bitmap);
    }
    let skip = (bitmap.top - top) as usize;
    let rows = (top - bottom) as usize;
    let stride = bitmap.width as usize * bitmap.content.channels() as usize;
    let coverage = bitmap.coverage[skip * stride..(skip + rows) * stride].to_vec();
    Some(GlyphBitmap {
        top,
        height: rows as u32,
        coverage,
        ..bitmap
    })
}

pub(super) fn as_drawn(bitmap: GlyphBitmap, silhouette: bool) -> GlyphBitmap {
    if !silhouette || bitmap.content != GlyphContent::Color {
        return bitmap;
    }
    let luminance = |pixel: &[u8]| {
        (2126 * pixel[0] as u32 + 7152 * pixel[1] as u32 + 722 * pixel[2] as u32) / 10000
    };
    let brightest = bitmap
        .coverage
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 0)
        .map(luminance)
        .max()
        .unwrap_or(0)
        .max(1);
    GlyphBitmap {
        coverage: bitmap
            .coverage
            .chunks_exact(4)
            .map(|pixel| (pixel[3] as u32 * luminance(pixel).min(brightest) / brightest) as u8)
            .collect(),
        content: GlyphContent::Mask,
        ..bitmap
    }
}

pub(super) fn downscale(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    channels: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut target = vec![0u8; (width * height * channels) as usize];

    for y in 0..height {
        let y0 = y * source_height / height;
        let y1 = (((y + 1) * source_height + height - 1) / height).max(y0 + 1);
        for x in 0..width {
            let x0 = x * source_width / width;
            let x1 = (((x + 1) * source_width + width - 1) / width).max(x0 + 1);
            for channel in 0..channels {
                let mut total = 0u32;
                let mut count = 0u32;
                for sample_y in y0..y1.min(source_height) {
                    for sample_x in x0..x1.min(source_width) {
                        let offset =
                            ((sample_y * source_width + sample_x) * channels + channel) as usize;
                        total += source[offset] as u32;
                        count += 1;
                    }
                }
                let offset = ((y * width + x) * channels + channel) as usize;
                target[offset] = if count == 0 { 0 } else { (total / count) as u8 };
            }
        }
    }

    target
}

impl FontStack {
    pub fn rasterize_with_room(
        &mut self,
        glyph: Glyph,
        budget: u32,
        spill: u32,
    ) -> Option<GlyphBitmap> {
        let font = *self.faces.get(glyph.font.0)?;
        let coords = self.coords[glyph.font.0].clone();
        if self.is_primary(glyph.font) {
            let mut bitmap = self.render(font, &coords, glyph.glyph, self.size, PRIMARY_SOURCES)?;
            bitmap.left += self.placement.inset * budget.max(1) as i32;
            return Some(bitmap);
        }

        let fit = Fit::new(self.metrics, budget.max(1)).spilling(spill);
        let advance_at = |size: f32| {
            font.glyph_metrics(&coords)
                .scale(size)
                .advance_width(glyph.glyph)
        };
        let mut size = self.size * fit.advance_scale(advance_at(self.size));
        let mut bitmap = self.render(font, &coords, glyph.glyph, size, FALLBACK_SOURCES)?;
        let shrink = fit.ink_scale(&bitmap);
        if shrink < 1.0 {
            size *= shrink;
            bitmap = self.render(font, &coords, glyph.glyph, size, FALLBACK_SOURCES)?;
        }
        Some(fit.place(bitmap, advance_at(size)))
    }

    pub(super) fn render(
        &mut self,
        font: FontRef<'static>,
        coords: &[i16],
        glyph: GlyphId,
        size: f32,
        sources: &[Source],
    ) -> Option<GlyphBitmap> {
        let mut scaler = self
            .context
            .builder(font)
            .size(size)
            .hint(false)
            .normalized_coords(coords.iter())
            .build();
        let image = Render::new(sources)
            .format(Format::Alpha)
            .render(&mut scaler, glyph)?;

        let placement = image.placement;
        if placement.width == 0 || placement.height == 0 {
            return None;
        }

        let content = match image.content {
            Content::Color => GlyphContent::Color,
            _ => GlyphContent::Mask,
        };
        let bitmap = GlyphBitmap {
            left: placement.left,
            top: placement.top,
            width: placement.width,
            height: placement.height,
            coverage: image.data,
            content,
        };
        (bitmap.coverage.len() >= (bitmap.width * bitmap.height * content.channels()) as usize)
            .then_some(bitmap)
    }
}

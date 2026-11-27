use crate::text::font::as_drawn;
use crate::text::font::crop_to_cell;
use crate::text::font::is_joining_control;
use crate::text::font::is_private_use;
use crate::text::font::wants_color;
use crate::text::font::ClusterRaster;
use crate::text::font::FaceStyle;
use crate::text::font::Fit;
use crate::text::font::FontId;
use crate::text::font::FontStack;
use crate::text::font::GlyphBitmap;
use crate::text::font::GlyphContent;
use crate::text::font::ShapedGlyph;
use crate::text::font::CLUSTER_ADVANCE_TOLERANCE;
use crate::text::font::EMOJI_FONT;
use crate::text::font::EMOJI_PROBE;
use crate::text::font::FALLBACK_SOURCES;
use crate::text::font::MAX_CLUSTER_FONTS;
use crate::text::font::PRIMARY_SOURCES;
use crate::text::font::SYMBOLS_FONT;
use crate::text::font::TEXT_PRESENTATION;
use anyhow::Result;
use std::collections::HashMap;
use swash::scale::Source;

pub(super) fn drawn_text(text: &str) -> std::borrow::Cow<'_, str> {
    match compose_hangul(text) {
        Some(syllable) => std::borrow::Cow::Owned(syllable.to_string()),
        None => std::borrow::Cow::Borrowed(text),
    }
}

pub fn compose_hangul(text: &str) -> Option<char> {
    let mut pieces = text
        .chars()
        .filter(|character| !is_joining_control(*character));
    let leading = pieces.next()?;
    let vowel = pieces.next()?;
    let trailing = pieces.next();
    if pieces.next().is_some()
        || !('\u{1100}'..='\u{1112}').contains(&leading)
        || !('\u{1161}'..='\u{1175}').contains(&vowel)
    {
        return None;
    }
    let trailing_index = match trailing {
        None => 0,
        Some(trailing) if ('\u{11a8}'..='\u{11c2}').contains(&trailing) => trailing as u32 - 0x11a7,
        Some(_) => return None,
    };
    char::from_u32(
        0xac00 + ((leading as u32 - 0x1100) * 21 + (vowel as u32 - 0x1161)) * 28 + trailing_index,
    )
}

impl FontStack {
    pub fn cluster_font(&mut self, style: FaceStyle, text: &str) -> Option<FontId> {
        if let Some(found) = self
            .cluster_fonts
            .get(&style)
            .and_then(|fonts| fonts.get(text))
        {
            return *found;
        }
        let found = self.search_cluster(style, text);
        if self.cluster_fonts.values().map(HashMap::len).sum::<usize>() >= MAX_CLUSTER_FONTS {
            self.cluster_fonts.clear();
        }
        self.cluster_fonts
            .entry(style)
            .or_default()
            .insert(text.to_owned(), found);
        found
    }

    pub(super) fn covers(&self, font: FontId, text: &str) -> bool {
        let mut significant = text
            .chars()
            .filter(|character| !is_joining_control(*character))
            .peekable();
        significant.peek().is_some()
            && significant.all(|character| self.map(font, character).is_some())
    }

    pub(super) fn search_cluster(&mut self, style: FaceStyle, text: &str) -> Option<FontId> {
        let drawn = drawn_text(text);
        let text: &str = &drawn;
        let text_style = text.contains(TEXT_PRESENTATION);
        if wants_color(text) && !text_style {
            if let Some(font) = self.colour_font_for(text) {
                return Some(font);
            }
        }
        if let Some(font) = self.faces_for(style).find(|font| self.covers(*font, text)) {
            return Some(font);
        }
        if text.chars().any(is_private_use) && self.covers(SYMBOLS_FONT, text) {
            return Some(SYMBOLS_FONT);
        }
        let fallbacks = self.fallbacks.clone();
        let suits = |stack: &Self, font: FontId| {
            stack.covers(font, text) && !(text_style && stack.has_color(font))
        };
        if let Some(font) = fallbacks.iter().copied().find(|font| suits(self, *font)) {
            return Some(font);
        }
        let first = text
            .chars()
            .find(|character| !is_joining_control(*character))?;
        if let Some(discovered) = self.discovered(style, first) {
            if suits(self, discovered.font) {
                return Some(discovered.font);
            }
        }
        if text_style {
            return fallbacks
                .iter()
                .copied()
                .find(|font| self.covers(*font, text));
        }
        None
    }

    pub(super) fn colour_font_for(&mut self, text: &str) -> Option<FontId> {
        let candidates: Vec<FontId> = self
            .fallbacks
            .clone()
            .into_iter()
            .filter(|font| self.has_color(*font) && self.covers(*font, text))
            .collect();
        candidates
            .iter()
            .copied()
            .find(|font| self.joins(*font, text))
            .or_else(|| candidates.first().copied())
    }

    pub(super) fn joins(&mut self, font: FontId, text: &str) -> bool {
        let shaped_text = self.shaping_text(font, text).into_owned();
        let glyphs = self.shape_cluster(font, &shaped_text);
        glyphs.len() == 1 && glyphs[0].glyph != 0
    }

    pub(super) fn adopt_system_emoji(&mut self) {
        let Some(discovery) = self.discovery else {
            return;
        };
        let Some((path, index)) = discovery.match_emoji(EMOJI_PROBE) else {
            return;
        };
        let Some(font) = self.adopt(path, index, self.weight) else {
            return;
        };
        if !self.has_color(font) || self.map(font, EMOJI_PROBE).is_none() {
            return;
        }
        if self.fallbacks.contains(&font) {
            return;
        }
        let bundled = self
            .fallbacks
            .iter()
            .position(|fallback| *fallback == EMOJI_FONT || *fallback == SYMBOLS_FONT)
            .unwrap_or(self.fallbacks.len());
        self.fallbacks.insert(bundled, font);
    }

    pub fn rasterize_cluster(
        &mut self,
        style: FaceStyle,
        text: &str,
        budget: u32,
    ) -> ClusterRaster {
        let budget = budget.max(1);
        let drawn = drawn_text(text);
        let text: &str = &drawn;
        let Some(font) = self.cluster_font(style, text) else {
            return ClusterRaster::Uncovered;
        };
        let silhouette = text.contains(TEXT_PRESENTATION) && self.has_color(font);
        let shaped_text = self.shaping_text(font, text);
        if shaped_text.is_empty() {
            return ClusterRaster::Uncovered;
        }
        let glyphs = self.shape_cluster(font, &shaped_text);
        if glyphs.is_empty() || glyphs.iter().any(|glyph| glyph.glyph == 0) {
            return ClusterRaster::Uncovered;
        }
        let advance = glyphs.iter().map(|glyph| glyph.advance).sum::<i32>() as f32;
        let box_width = (self.metrics.width * budget) as f32;
        let squeezable = silhouette && glyphs.len() == 1;
        if advance <= 0.0 || (advance > box_width * CLUSTER_ADVANCE_TOLERANCE && !squeezable) {
            return ClusterRaster::Uncovered;
        }
        let primary = self.is_primary(font);
        let sources = if primary {
            PRIMARY_SOURCES
        } else {
            FALLBACK_SOURCES
        };
        if primary && advance <= box_width + 0.5 {
            return match self.composite(font, &glyphs, self.size, sources) {
                Err(()) => ClusterRaster::Uncovered,
                Ok(None) => ClusterRaster::Blank,
                Ok(Some(mut bitmap)) => {
                    bitmap.left += ((box_width - advance) / 2.0).round() as i32;
                    match crop_to_cell(bitmap, self.metrics) {
                        Some(bitmap) => ClusterRaster::Sprite(bitmap),
                        None => ClusterRaster::Blank,
                    }
                },
            };
        }
        let fit = Fit::new(self.metrics, budget);
        let mut size = self.size * fit.advance_scale(advance);
        let mut bitmap = match self.composite(font, &glyphs, size, sources) {
            Err(()) => return ClusterRaster::Uncovered,
            Ok(None) => return ClusterRaster::Blank,
            Ok(Some(bitmap)) => as_drawn(bitmap, silhouette),
        };
        let shrink = fit.ink_scale(&bitmap);
        if shrink < 1.0 {
            size *= shrink;
            bitmap = match self.composite(font, &glyphs, size, sources) {
                Err(()) => return ClusterRaster::Uncovered,
                Ok(None) => return ClusterRaster::Blank,
                Ok(Some(bitmap)) => as_drawn(bitmap, silhouette),
            };
        }
        let placed = fit.place(bitmap, advance * size / self.size);
        match crop_to_cell(placed, self.metrics) {
            Some(bitmap) => ClusterRaster::Sprite(bitmap),
            None => ClusterRaster::Blank,
        }
    }

    pub(super) fn composite(
        &mut self,
        font: FontId,
        glyphs: &[ShapedGlyph],
        size: f32,
        sources: &[Source],
    ) -> Result<Option<GlyphBitmap>, ()> {
        let face = *self.faces.get(font.0).ok_or(())?;
        let coords = self.coords[font.0].clone();
        let scale = size / self.size;
        let mut pieces: Vec<(i32, i32, GlyphBitmap)> = Vec::with_capacity(glyphs.len());
        for glyph in glyphs {
            let Some(bitmap) = self.render(face, &coords, glyph.glyph, size, sources) else {
                continue;
            };
            let x = (glyph.dx as f32 * scale).round() as i32 + bitmap.left;
            let top = bitmap.top + (glyph.dy as f32 * scale).round() as i32;
            pieces.push((x, top, bitmap));
        }
        let Some(content) = pieces.first().map(|(_, _, bitmap)| bitmap.content) else {
            return Ok(None);
        };
        if pieces
            .iter()
            .any(|(_, _, bitmap)| bitmap.content != content)
        {
            return Err(());
        }
        if pieces.len() == 1 {
            let (left, top, bitmap) = pieces.pop().ok_or(())?;
            return Ok(Some(GlyphBitmap {
                left,
                top,
                ..bitmap
            }));
        }
        let left = pieces.iter().map(|(x, _, _)| *x).min().unwrap_or(0);
        let right = pieces
            .iter()
            .map(|(x, _, bitmap)| *x + bitmap.width as i32)
            .max()
            .unwrap_or(0);
        let top = pieces.iter().map(|(_, top, _)| *top).max().unwrap_or(0);
        let bottom = pieces
            .iter()
            .map(|(_, top, bitmap)| *top - bitmap.height as i32)
            .min()
            .unwrap_or(0);
        if right <= left || top <= bottom {
            return Ok(None);
        }
        let width = (right - left) as usize;
        let height = (top - bottom) as usize;
        let channels = content.channels() as usize;
        let mut coverage = vec![0u8; width * height * channels];
        for (x, piece_top, bitmap) in &pieces {
            let offset_x = (*x - left) as usize;
            let offset_y = (top - *piece_top) as usize;
            let piece_width = bitmap.width as usize;
            for row in 0..bitmap.height as usize {
                for col in 0..piece_width {
                    let source = (row * piece_width + col) * channels;
                    let target = ((offset_y + row) * width + offset_x + col) * channels;
                    match content {
                        GlyphContent::Mask => {
                            coverage[target] = coverage[target].max(bitmap.coverage[source]);
                        },
                        GlyphContent::Color => {
                            let alpha = bitmap.coverage[source + 3] as u32;
                            if alpha == 0 {
                                continue;
                            }
                            for channel in 0..4 {
                                let below = coverage[target + channel] as u32;
                                let above = bitmap.coverage[source + channel] as u32;
                                coverage[target + channel] =
                                    (above + below * (255 - alpha) / 255).min(255) as u8;
                            }
                        },
                    }
                }
            }
        }
        Ok(Some(GlyphBitmap {
            left,
            top,
            width: width as u32,
            height: height as u32,
            coverage,
            content,
        }))
    }
}

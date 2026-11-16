use crate::text::font::is_joining_control;
use crate::text::font::script_of;
use crate::text::font::FontFeature;
use crate::text::font::FontId;
use crate::text::font::FontStack;
use crate::text::font::ShapedGlyph;
use crate::text::font::Shaping;
use crate::text::font::LIGATURE_FEATURES;
use crate::text::font::MAX_SHAPED_RUNS;
use std::collections::HashMap;
use std::rc::Rc;
use swash::shape::Direction;
use swash::tag_from_bytes;
use swash::text::cluster::{CharCluster, Parser, Token};
use swash::text::Script;

pub(super) fn effective_features(requested: &[FontFeature], ligatures: bool) -> Vec<FontFeature> {
    if requested.is_empty() {
        return Vec::new();
    }
    let mut features = Vec::new();
    if !ligatures {
        for tag in LIGATURE_FEATURES {
            if !requested.iter().any(|feature| &feature.tag == tag) {
                features.push(FontFeature {
                    tag: *tag,
                    value: 0,
                });
            }
        }
    }
    features.extend_from_slice(requested);
    features
}

impl FontStack {
    pub fn shape(&mut self, font: FontId, text: &str) -> Rc<[ShapedGlyph]> {
        self.shape_as(font, text, Shaping::Run)
    }

    pub fn shape_cluster(&mut self, font: FontId, text: &str) -> Rc<[ShapedGlyph]> {
        self.shape_as(font, text, Shaping::Cluster)
    }

    pub(super) fn shape_as(
        &mut self,
        font: FontId,
        text: &str,
        shaping: Shaping,
    ) -> Rc<[ShapedGlyph]> {
        if let Some(cached) = self
            .shaped
            .get(&(font, shaping))
            .and_then(|runs| runs.get(text))
        {
            return Rc::clone(cached);
        }
        let shaped: Rc<[ShapedGlyph]> = match shaping {
            Shaping::Run => self.shape_uncached(font, text),
            Shaping::Cluster => self.shape_cluster_uncached(font, text),
        }
        .into();
        if self.shaped.values().map(HashMap::len).sum::<usize>() >= MAX_SHAPED_RUNS {
            self.shaped.clear();
        }
        self.shaped
            .entry((font, shaping))
            .or_default()
            .insert(text.to_owned(), Rc::clone(&shaped));
        shaped
    }

    pub(super) fn shape_cluster_uncached(&mut self, font: FontId, text: &str) -> Vec<ShapedGlyph> {
        let Some(face) = self.faces.get(font.0).copied() else {
            return Vec::new();
        };
        let coords = &self.coords[font.0];
        let mut shaper = self
            .shaping
            .builder(face)
            .script(script_of(text))
            .direction(Direction::LeftToRight)
            .size(self.size)
            .normalized_coords(coords.iter())
            .build();
        shaper.add_str(text);
        let mut glyphs = Vec::new();
        let mut pen = 0.0f32;
        shaper.shape_with(|shaped| {
            for glyph in shaped.glyphs {
                glyphs.push(ShapedGlyph {
                    cell: 0,
                    span: 1,
                    glyph: glyph.id,
                    dx: (pen + glyph.x).round() as i32,
                    dy: glyph.y.round() as i32,
                    advance: glyph.advance.round() as i32,
                });
                pen += glyph.advance;
            }
        });
        glyphs
    }

    pub(super) fn shaping_text<'a>(
        &self,
        font: FontId,
        text: &'a str,
    ) -> std::borrow::Cow<'a, str> {
        let unmapped_control =
            |character: char| is_joining_control(character) && self.map(font, character).is_none();
        if text.chars().any(unmapped_control) {
            std::borrow::Cow::Owned(
                text.chars()
                    .filter(|character| !unmapped_control(*character))
                    .collect(),
            )
        } else {
            std::borrow::Cow::Borrowed(text)
        }
    }

    pub(super) fn shape_uncached(&mut self, font: FontId, text: &str) -> Vec<ShapedGlyph> {
        let Some(face) = self.faces.get(font.0).copied() else {
            return Vec::new();
        };
        let charmap = face.charmap();
        let coords = &self.coords[font.0];
        let mut shaper = self
            .shaping
            .builder(face)
            .script(Script::Latin)
            .direction(Direction::LeftToRight)
            .size(self.size)
            .features(
                self.features
                    .iter()
                    .map(|feature| (tag_from_bytes(&feature.tag), feature.value)),
            )
            .normalized_coords(coords.iter())
            .build();
        let natural = self.placement.natural_width as f32;
        let stretch = self.metrics.width as i32 - self.placement.natural_width as i32;

        let mut cluster = CharCluster::new();
        let mut parser = Parser::new(
            Script::Latin,
            text.chars().enumerate().map(|(index, character)| Token {
                ch: character,
                offset: index as u32,
                len: 1,
                info: character.into(),
                data: 0,
            }),
        );
        while parser.next(&mut cluster) {
            cluster.map(|character| charmap.map(character));
            shaper.add_cluster(&cluster);
        }

        let mut glyphs = Vec::new();
        shaper.shape_with(|shaped| {
            let cell = shaped.source.start as usize;
            let span = (shaped.source.end as usize).saturating_sub(cell).max(1);
            let mut pen = 0.0f32;
            for glyph in shaped.glyphs {
                let cells_in = if stretch != 0 && natural > 0.0 {
                    (pen / natural).round() as i32
                } else {
                    0
                };
                glyphs.push(ShapedGlyph {
                    cell,
                    span,
                    glyph: glyph.id,
                    dx: (pen + glyph.x).round() as i32 + cells_in * stretch,
                    dy: glyph.y.round() as i32,
                    advance: glyph.advance.round() as i32,
                });
                pen += glyph.advance;
            }
        });
        glyphs
    }
}

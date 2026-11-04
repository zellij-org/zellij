use crate::text::font::font_file;
use crate::text::font::has_weight_axis;
use crate::text::font::is_color_capable;
use crate::text::font::weight_coords;
use crate::text::font::FaceStyle;
use crate::text::font::FontId;
use crate::text::font::FontStack;
use crate::text::font::Glyph;
use crate::text::font::STATIC_FACE;
use crate::text::font::SYMBOLS_FONT;
use std::path::PathBuf;
use swash::FontRef;

pub(super) fn is_private_use(character: char) -> bool {
    matches!(character, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')
}

pub(super) fn choose(
    discovered: Option<(Glyph, bool)>,
    fallbacks: &[(Glyph, bool)],
) -> Option<Glyph> {
    if let Some((glyph, true)) = discovered {
        return Some(glyph);
    }
    for (glyph, color) in fallbacks {
        if discovered.is_none() || *color {
            return Some(*glyph);
        }
    }
    discovered.map(|(glyph, _)| glyph)
}

impl FontStack {
    pub fn lookup(&mut self, style: FaceStyle, character: char) -> Option<Glyph> {
        if let Some(resolved) = self.resolved.get(&(style, character)) {
            return *resolved;
        }
        let found = self.search(style, character);
        self.resolved.insert((style, character), found);
        found
    }

    pub fn is_primary(&self, font: FontId) -> bool {
        self.primary.contains(&font)
    }

    pub(super) fn search(&mut self, style: FaceStyle, character: char) -> Option<Glyph> {
        if let Some(glyph) = self.map(self.primary[style.index()], character) {
            return Some(glyph);
        }
        if is_private_use(character) {
            if let Some(glyph) = self.map(SYMBOLS_FONT, character) {
                return Some(glyph);
            }
        }

        let discovered = self
            .discovered(style, character)
            .map(|glyph| (glyph, self.has_color(glyph.font)));
        let fallbacks: Vec<(Glyph, bool)> = self
            .fallbacks
            .iter()
            .filter_map(|fallback| self.map(*fallback, character))
            .map(|glyph| (glyph, self.has_color(glyph.font)))
            .collect();

        choose(discovered, &fallbacks)
    }

    pub(super) fn has_color(&self, font: FontId) -> bool {
        self.color_capable.get(font.0).copied().unwrap_or(false)
    }

    pub(super) fn map(&self, font: FontId, character: char) -> Option<Glyph> {
        let glyph = self.faces.get(font.0)?.charmap().map(character);
        (glyph != 0).then_some(Glyph { font, glyph })
    }

    pub(super) fn discovered(&mut self, style: FaceStyle, character: char) -> Option<Glyph> {
        {
            self.discovery_queries += 1;
        }
        let request = self.request(style);
        let (path, index) = self
            .discovery
            .as_ref()?
            .match_codepoint(request, character)?;
        let font = self.adopt(path, index, request.weight)?;
        self.map(font, character)
    }

    pub(super) fn requested_family(&mut self, family: &str) -> Option<[FontId; 4]> {
        let discovery = self.discovery?;
        let resolve = |stack: &mut Self, style: FaceStyle| {
            let request = stack.request(style);
            let (path, index) = discovery.match_family(family, request)?;
            stack.adopt(path, index, request.weight)
        };

        let regular = resolve(self, FaceStyle::Regular)?;
        Some([
            regular,
            resolve(self, FaceStyle::Bold).unwrap_or(regular),
            resolve(self, FaceStyle::Italic).unwrap_or(regular),
            resolve(self, FaceStyle::BoldItalic).unwrap_or(regular),
        ])
    }

    pub(super) fn adopt(&mut self, path: PathBuf, index: u32, weight: u16) -> Option<FontId> {
        if let Some(font) = self.loaded.get(&(path.clone(), index, STATIC_FACE)) {
            return Some(*font);
        }
        if let Some(font) = self.loaded.get(&(path.clone(), index, weight)) {
            return Some(*font);
        }
        let data = match self.files.get(&path) {
            Some(data) => *data,
            None => {
                let data = font_file(&path)?;
                self.files.insert(path.clone(), data);
                data
            },
        };
        let parsed = FontRef::from_index(data, index as usize)?;
        let coords = weight_coords(&parsed, weight);
        let key = if has_weight_axis(&parsed) {
            weight
        } else {
            STATIC_FACE
        };
        let font = FontId(self.faces.len());
        self.color_capable.push(is_color_capable(&parsed));
        self.faces.push(parsed);
        self.coords.push(coords);
        self.loaded.insert((path, index, key), font);
        Some(font)
    }
}

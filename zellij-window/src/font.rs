use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{anyhow, Result};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::shape::{Direction, ShapeContext};
use swash::text::cluster::{CharCluster, Parser, Token};
use swash::text::Script;
use swash::zeno::Format;
use swash::{tag_from_bytes, FontRef};

use crate::discovery::Discovery;

pub use swash::GlyphId;

pub const DEFAULT_FONT_SIZE: f32 = 16.0;
pub const DEFAULT_FONT_POINTS: f32 = 12.0;
#[cfg(target_os = "macos")]
const PIXELS_PER_INCH: f32 = 72.0;
#[cfg(not(target_os = "macos"))]
const PIXELS_PER_INCH: f32 = 96.0;
const POINTS_PER_INCH: f32 = 72.0;
pub const DEFAULT_LIGATURES: bool = true;
pub const DEFAULT_FONT_WEIGHT: u16 = 400;
const BOLD_WEIGHT_STEP: u16 = 300;
const MAX_FONT_WEIGHT: u16 = 900;
const EMBEDDED_BOLD_THRESHOLD: u16 = 500;
const WEIGHT_AXIS: &[u8; 4] = b"wght";
const LIGATURE_FEATURES: [&[u8; 4]; 3] = [b"liga", b"calt", b"dlig"];

const MIN_FONT_SIZE: f32 = 4.0;
const MAX_FONT_SIZE: f32 = 512.0;
const MAX_SHAPED_RUNS: usize = 4096;
const FALLBACK_OVERHANG: f32 = 1.25;

const PRIMARY_SOURCES: &[Source] = &[Source::Outline];
const FALLBACK_SOURCES: &[Source] = &[
    Source::ColorOutline(0),
    Source::ColorBitmap(StrikeWith::BestFit),
    Source::Outline,
];

const REGULAR: &[u8] = include_bytes!("../assets/fonts/IosevkaTerm-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../assets/fonts/IosevkaTerm-Bold.ttf");
const ITALIC: &[u8] = include_bytes!("../assets/fonts/IosevkaTerm-Italic.ttf");
const BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/IosevkaTerm-BoldItalic.ttf");
const CJK: &[u8] = include_bytes!("../assets/fonts/NotoSansMonoCJK-Regular.otf");
const EMOJI: &[u8] = include_bytes!("../assets/fonts/NotoColorEmoji.ttf");
const SYMBOLS: &[u8] = include_bytes!("../assets/fonts/SymbolsNerdFontMono-Regular.ttf");
const SYMBOLS_FONT: FontId = FontId(6);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaceStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FaceStyle {
    pub fn of(bold: bool, italic: bool) -> Self {
        match (bold, italic) {
            (false, false) => FaceStyle::Regular,
            (true, false) => FaceStyle::Bold,
            (false, true) => FaceStyle::Italic,
            (true, true) => FaceStyle::BoldItalic,
        }
    }

    pub fn index(self) -> usize {
        match self {
            FaceStyle::Regular => 0,
            FaceStyle::Bold => 1,
            FaceStyle::Italic => 2,
            FaceStyle::BoldItalic => 3,
        }
    }

    pub fn is_bold(self) -> bool {
        matches!(self, FaceStyle::Bold | FaceStyle::BoldItalic)
    }

    pub fn is_italic(self) -> bool {
        matches!(self, FaceStyle::Italic | FaceStyle::BoldItalic)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellMetrics {
    pub width: u32,
    pub height: u32,
    pub baseline: u32,
    pub underline_top: u32,
    pub strikeout_top: u32,
    pub stroke: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(usize);

impl FontId {
    pub const SPRITES: FontId = FontId(usize::MAX);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Glyph {
    pub font: FontId,
    pub glyph: GlyphId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlyphContent {
    Mask,
    Color,
}

impl GlyphContent {
    pub fn channels(self) -> u32 {
        match self {
            GlyphContent::Mask => 1,
            GlyphContent::Color => 4,
        }
    }
}

#[derive(Clone)]
pub struct GlyphBitmap {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub coverage: Vec<u8>,
    pub content: GlyphContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FaceRequest {
    pub weight: u16,
    pub italic: bool,
}

impl FaceRequest {
    pub fn weighted(style: FaceStyle, regular: u16) -> Self {
        Self {
            weight: if style.is_bold() {
                bold_weight(regular)
            } else {
                regular
            },
            italic: style.is_italic(),
        }
    }
}

impl From<FaceStyle> for FaceRequest {
    fn from(style: FaceStyle) -> Self {
        Self::weighted(style, DEFAULT_FONT_WEIGHT)
    }
}

fn embedded_faces_for(weight: u16) -> [FontId; 4] {
    if weight > EMBEDDED_BOLD_THRESHOLD {
        [FontId(1), FontId(1), FontId(3), FontId(3)]
    } else {
        [FontId(0), FontId(1), FontId(2), FontId(3)]
    }
}

pub fn bold_weight(regular: u16) -> u16 {
    regular
        .saturating_add(BOLD_WEIGHT_STEP)
        .min(MAX_FONT_WEIGHT)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontFeature {
    pub tag: [u8; 4],
    pub value: u16,
}

impl FontFeature {
    pub fn parse(text: &str) -> Option<Self> {
        let (value, tag) = match text.as_bytes().first() {
            Some(b'-') => (0, &text[1..]),
            Some(b'+') => (1, &text[1..]),
            _ => (1, text),
        };
        let tag: [u8; 4] = tag.as_bytes().try_into().ok()?;
        tag.iter()
            .all(|byte| (0x21..=0x7e).contains(byte))
            .then_some(Self { tag, value })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellAdjust {
    pub line_height: f32,
    pub cell_width: f32,
    pub baseline_offset: f32,
    pub underline_offset: f32,
    pub underline_thickness: f32,
}

impl Default for CellAdjust {
    fn default() -> Self {
        Self {
            line_height: 1.0,
            cell_width: 1.0,
            baseline_offset: 0.0,
            underline_offset: 0.0,
            underline_thickness: 0.0,
        }
    }
}

impl CellAdjust {
    fn scaled(&self, scale: f32) -> Self {
        Self {
            baseline_offset: self.baseline_offset * scale,
            underline_offset: self.underline_offset * scale,
            underline_thickness: self.underline_thickness * scale,
            ..*self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPlacement {
    pub natural_width: u32,
    pub inset: i32,
    pub underline_stroke: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontOptions {
    pub family: Option<String>,
    pub size: f32,
    pub system_fonts: bool,
    pub ligatures: bool,
    pub weight: u16,
    pub features: Vec<FontFeature>,
    pub cell: CellAdjust,
}

impl Default for FontOptions {
    fn default() -> Self {
        Self {
            family: None,
            size: DEFAULT_FONT_SIZE,
            system_fonts: true,
            ligatures: DEFAULT_LIGATURES,
            weight: DEFAULT_FONT_WEIGHT,
            features: Vec::new(),
            cell: CellAdjust::default(),
        }
    }
}

pub fn points_to_pixels(points: f32) -> f32 {
    points * PIXELS_PER_INCH / POINTS_PER_INCH
}

#[cfg(test)]
pub fn pixels_to_points(pixels: f32) -> f32 {
    pixels * POINTS_PER_INCH / PIXELS_PER_INCH
}

impl FontOptions {
    pub fn scaled(&self, scale: f64) -> Self {
        Self {
            size: self.size * scale as f32,
            cell: self.cell.scaled(scale as f32),
            ..self.clone()
        }
    }

    pub fn stack(&self, scale: f64) -> Result<FontStack> {
        FontStack::build(&self.scaled(scale))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapedGlyph {
    pub cell: usize,
    pub span: usize,
    pub glyph: GlyphId,
    pub dx: i32,
    pub dy: i32,
}

pub struct FontStack {
    faces: Vec<FontRef<'static>>,
    coords: Vec<Vec<i16>>,
    color_capable: Vec<bool>,
    primary: [FontId; 4],
    embedded_fallbacks: [FontId; 3],
    size: f32,
    weight: u16,
    features: Vec<FontFeature>,
    shapes: bool,
    metrics: CellMetrics,
    placement: CellPlacement,
    context: ScaleContext,
    shaping: ShapeContext,
    discovery: Option<Discovery>,
    files: HashMap<PathBuf, &'static [u8]>,
    loaded: HashMap<(PathBuf, u32, u16), FontId>,
    resolved: HashMap<(FaceStyle, char), Option<Glyph>>,
    shaped: HashMap<FontId, HashMap<String, Rc<[ShapedGlyph]>>>,
    #[cfg(test)]
    discovery_queries: usize,
}

impl FontStack {
    #[cfg(test)]
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

    pub fn build(options: &FontOptions) -> Result<Self> {
        let discovery = if options.system_fonts {
            Discovery::open()
        } else {
            None
        };
        Self::assemble(options, discovery)
    }

    #[cfg(test)]
    pub fn build_with(options: &FontOptions, discovery: Option<Discovery>) -> Result<Self> {
        Self::assemble(options, discovery)
    }

    fn assemble(options: &FontOptions, discovery: Option<Discovery>) -> Result<Self> {
        let size = options.size;
        if !(size.is_finite() && (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&size)) {
            return Err(anyhow!("font size {} is out of range", size));
        }

        let faces = vec![
            face(REGULAR, "IosevkaTerm-Regular")?,
            face(BOLD, "IosevkaTerm-Bold")?,
            face(ITALIC, "IosevkaTerm-Italic")?,
            face(BOLD_ITALIC, "IosevkaTerm-BoldItalic")?,
            face(CJK, "NotoSansMonoCJK-Regular")?,
            face(EMOJI, "NotoColorEmoji")?,
            face(SYMBOLS, "SymbolsNerdFontMono-Regular")?,
        ];
        let color_capable: Vec<bool> = faces.iter().map(is_color_capable).collect();
        let coords = vec![Vec::new(); faces.len()];
        let embedded_primary = embedded_faces_for(options.weight);
        let features = effective_features(&options.features, options.ligatures);
        let (metrics, placement) = derive_metrics(&faces[0], &[], size, &options.cell);

        let mut stack = Self {
            faces,
            coords,
            color_capable,
            primary: embedded_primary,
            embedded_fallbacks: [FontId(4), FontId(5), SYMBOLS_FONT],
            size,
            weight: options.weight,
            shapes: options.ligatures || !features.is_empty(),
            features,
            metrics,
            placement,
            context: ScaleContext::new(),
            shaping: ShapeContext::new(),
            discovery,
            files: HashMap::new(),
            loaded: HashMap::new(),
            resolved: HashMap::new(),
            shaped: HashMap::new(),
            #[cfg(test)]
            discovery_queries: 0,
        };

        if let Some(family) = &options.family {
            match stack.requested_family(family) {
                Some(primary) => stack.primary = primary,
                None => report!(
                    "no font family named {:?} resolved; \
                     rendering with the embedded font instead",
                    family
                ),
            }
        }

        let regular = stack.primary[0].0;
        let (metrics, placement) = derive_metrics(
            &stack.faces[regular],
            &stack.coords[regular],
            size,
            &options.cell,
        );
        stack.metrics = metrics;
        stack.placement = placement;
        Ok(stack)
    }

    pub fn metrics(&self) -> CellMetrics {
        self.metrics
    }

    pub fn placement(&self) -> CellPlacement {
        self.placement
    }

    pub fn shapes_runs(&self) -> bool {
        self.shapes
    }

    fn request(&self, style: FaceStyle) -> FaceRequest {
        FaceRequest::weighted(style, self.weight)
    }

    pub fn primary_of(&self, style: FaceStyle) -> FontId {
        self.primary[style.index()]
    }

    pub fn shape(&mut self, font: FontId, text: &str) -> Rc<[ShapedGlyph]> {
        if let Some(cached) = self.shaped.get(&font).and_then(|runs| runs.get(text)) {
            return Rc::clone(cached);
        }
        let shaped: Rc<[ShapedGlyph]> = self.shape_uncached(font, text).into();
        if self.shaped_runs() >= MAX_SHAPED_RUNS {
            self.shaped.clear();
        }
        self.shaped
            .entry(font)
            .or_default()
            .insert(text.to_owned(), Rc::clone(&shaped));
        shaped
    }

    pub fn shaped_runs(&self) -> usize {
        self.shaped.values().map(HashMap::len).sum()
    }

    fn shape_uncached(&mut self, font: FontId, text: &str) -> Vec<ShapedGlyph> {
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
                });
                pen += glyph.advance;
            }
        });
        glyphs
    }

    #[cfg(test)]
    pub fn has_discovery(&self) -> bool {
        self.discovery.is_some()
    }

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

    #[cfg(test)]
    pub fn rasterize(&mut self, glyph: Glyph, budget: u32) -> Option<GlyphBitmap> {
        self.rasterize_with_room(glyph, budget, 0)
    }

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

    fn render(
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

    fn search(&mut self, style: FaceStyle, character: char) -> Option<Glyph> {
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
            .embedded_fallbacks
            .iter()
            .filter_map(|fallback| self.map(*fallback, character))
            .map(|glyph| (glyph, self.has_color(glyph.font)))
            .collect();

        choose(discovered, &fallbacks)
    }

    fn has_color(&self, font: FontId) -> bool {
        self.color_capable.get(font.0).copied().unwrap_or(false)
    }

    fn map(&self, font: FontId, character: char) -> Option<Glyph> {
        let glyph = self.faces.get(font.0)?.charmap().map(character);
        (glyph != 0).then_some(Glyph { font, glyph })
    }

    fn discovered(&mut self, style: FaceStyle, character: char) -> Option<Glyph> {
        #[cfg(test)]
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

    fn requested_family(&mut self, family: &str) -> Option<[FontId; 4]> {
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

    fn adopt(&mut self, path: PathBuf, index: u32, weight: u16) -> Option<FontId> {
        if let Some(font) = self.loaded.get(&(path.clone(), index, STATIC_FACE)) {
            return Some(*font);
        }
        if let Some(font) = self.loaded.get(&(path.clone(), index, weight)) {
            return Some(*font);
        }
        let data = match self.files.get(&path) {
            Some(data) => *data,
            None => {
                let data = std::fs::read(&path)
                    .map_err(|e| report!("failed to read the font {:?}: {}", path, e))
                    .ok()?;
                let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
                self.files.insert(path.clone(), leaked);
                leaked
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

const STATIC_FACE: u16 = 0;

fn has_weight_axis(font: &FontRef<'_>) -> bool {
    font.variations()
        .find_by_tag(tag_from_bytes(WEIGHT_AXIS))
        .is_some()
}

fn weight_coords(font: &FontRef<'_>, weight: u16) -> Vec<i16> {
    let variations = font.variations();
    let Some(axis) = variations.find_by_tag(tag_from_bytes(WEIGHT_AXIS)) else {
        return Vec::new();
    };
    let value = (weight as f32).clamp(axis.min_value(), axis.max_value());
    let coords: Vec<i16> = variations
        .normalized_coords([(tag_from_bytes(WEIGHT_AXIS), value)])
        .collect();
    if coords.iter().all(|coord| *coord == 0) {
        Vec::new()
    } else {
        coords
    }
}

fn effective_features(requested: &[FontFeature], ligatures: bool) -> Vec<FontFeature> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fit {
    box_width: u32,
    cell_width: u32,
    spill_width: u32,
    cell_height: u32,
    baseline: u32,
}

impl Fit {
    fn new(metrics: CellMetrics, budget: u32) -> Self {
        Self {
            box_width: metrics.width * budget,
            cell_width: metrics.width,
            spill_width: 0,
            cell_height: metrics.height,
            baseline: metrics.baseline,
        }
    }

    fn spilling(self, cells: u32) -> Self {
        Self {
            spill_width: self.cell_width * cells,
            ..self
        }
    }

    fn room(&self) -> f32 {
        if self.spill_width > 0 {
            (self.box_width + self.spill_width) as f32
        } else {
            self.box_width as f32 * FALLBACK_OVERHANG
        }
    }

    fn widest(&self, content: GlyphContent) -> u32 {
        match content {
            GlyphContent::Mask => (self.room().floor() as u32).max(self.box_width),
            GlyphContent::Color => self.box_width,
        }
    }

    fn advance_scale(&self, advance: f32) -> f32 {
        if advance.is_finite() && advance > self.room() {
            self.room() / advance
        } else {
            1.0
        }
    }

    fn ink_scale(&self, bitmap: &GlyphBitmap) -> f32 {
        (self.widest(bitmap.content) as f32 / bitmap.width as f32)
            .min(self.cell_height as f32 / bitmap.height as f32)
            .min(1.0)
    }

    fn place(&self, bitmap: GlyphBitmap, advance: f32) -> GlyphBitmap {
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

fn confine(bitmap: GlyphBitmap, max_width: u32, max_height: u32) -> GlyphBitmap {
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

fn is_private_use(character: char) -> bool {
    matches!(character, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')
}

fn choose(discovered: Option<(Glyph, bool)>, fallbacks: &[(Glyph, bool)]) -> Option<Glyph> {
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

fn is_color_capable(font: &FontRef<'static>) -> bool {
    font.color_palettes().count() > 0 || font.color_strikes().count() > 0
}

fn face(data: &'static [u8], name: &str) -> Result<FontRef<'static>> {
    FontRef::from_index(data, 0).ok_or_else(|| anyhow!("embedded face {} failed to parse", name))
}

fn downscale(
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

fn derive_metrics(
    font: &FontRef<'static>,
    coords: &[i16],
    size: f32,
    adjust: &CellAdjust,
) -> (CellMetrics, CellPlacement) {
    let metrics = font.metrics(coords).scale(size);
    let advance = font
        .glyph_metrics(coords)
        .scale(size)
        .advance_width(font.charmap().map('M'));

    let natural_width = advance.round().max(1.0) as u32;
    let natural_height = (metrics.ascent + metrics.descent + metrics.leading)
        .round()
        .max(1.0) as u32;
    let natural_baseline = (metrics.ascent + metrics.leading / 2.0)
        .round()
        .clamp(1.0, natural_height as f32) as i32;

    let width = scaled_cell(natural_width, adjust.cell_width);
    let height = scaled_cell(natural_height, adjust.line_height);
    let added = height as i32 - natural_height as i32;
    let baseline = (natural_baseline + added.div_euclid(2) + adjust.baseline_offset.round() as i32)
        .clamp(1, height as i32) as u32;
    let stroke = metrics.stroke_size.round().max(1.0) as u32;
    let underline_stroke =
        (stroke as i32 + adjust.underline_thickness.round() as i32).clamp(1, height as i32) as u32;

    let underline_top = ((baseline as f32 - metrics.underline_offset).round()
        + adjust.underline_offset.round())
    .clamp(0.0, (height - underline_stroke.min(height)) as f32) as u32;
    let strikeout_top = (baseline as f32 - metrics.strikeout_offset)
        .round()
        .clamp(0.0, (height - stroke.min(height)) as f32) as u32;

    (
        CellMetrics {
            width,
            height,
            baseline,
            underline_top,
            strikeout_top,
            stroke,
        },
        CellPlacement {
            natural_width,
            inset: (width as i32 - natural_width as i32).div_euclid(2),
            underline_stroke,
        },
    )
}

fn scaled_cell(natural: u32, scale: f32) -> u32 {
    if !scale.is_finite() || scale == 1.0 {
        return natural.max(1);
    }
    (natural as f32 * scale).round().max(1.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> FontStack {
        FontStack::embedded(DEFAULT_FONT_SIZE).unwrap()
    }

    fn glyph_of(fonts: &mut FontStack, style: FaceStyle, character: char) -> Glyph {
        fonts
            .lookup(style, character)
            .unwrap_or_else(|| panic!("{:?} has no glyph anywhere in the stack", character))
    }

    #[test]
    fn the_embedded_faces_all_parse() {
        let mut fonts = fonts();
        for style in [
            FaceStyle::Regular,
            FaceStyle::Bold,
            FaceStyle::Italic,
            FaceStyle::BoldItalic,
        ] {
            assert!(fonts.lookup(style, 'A').is_some(), "{:?}", style);
        }
    }

    #[test]
    fn the_cell_is_the_documented_size_at_the_default_font_size() {
        assert_eq!(
            fonts().metrics(),
            CellMetrics {
                width: 8,
                height: 20,
                baseline: 15,
                underline_top: 16,
                strikeout_top: 11,
                stroke: 1,
            }
        );
    }

    #[test]
    fn every_face_shares_one_cell_size() {
        let regular = fonts().metrics();
        let advance = |style: FaceStyle| {
            let fonts = fonts();
            let font = fonts.faces[style.index()];
            font.glyph_metrics(&[])
                .scale(DEFAULT_FONT_SIZE)
                .advance_width(font.charmap().map('M'))
        };
        for style in [FaceStyle::Bold, FaceStyle::Italic, FaceStyle::BoldItalic] {
            assert_eq!(advance(style).round() as u32, regular.width, "{:?}", style);
        }
    }

    #[test]
    fn the_cell_scales_with_the_font_size() {
        let doubled = FontStack::embedded(DEFAULT_FONT_SIZE * 2.0)
            .unwrap()
            .metrics();
        assert_eq!(doubled.width, 16);
        assert_eq!(doubled.height, 40);
    }

    fn synthetic_discovery() -> Option<Discovery> {
        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
        let discovery = Discovery::synthetic(&directory);
        if discovery.is_none() {
            eprintln!("fontconfig is unavailable here; family selection is untested");
        }
        discovery
    }

    fn requested(family: &str) -> FontOptions {
        FontOptions {
            family: Some(family.to_owned()),
            size: DEFAULT_FONT_SIZE,
            system_fonts: true,
            ligatures: DEFAULT_LIGATURES,
            ..FontOptions::default()
        }
    }

    fn adjusted(cell: CellAdjust) -> FontStack {
        FontStack::build_with(
            &FontOptions {
                system_fonts: false,
                cell,
                ..FontOptions::default()
            },
            None,
        )
        .unwrap()
    }

    #[test]
    fn line_height_grows_the_cell_and_moves_the_baseline_down_by_half_the_growth() {
        let natural = fonts().metrics();
        let stack = adjusted(CellAdjust {
            line_height: 1.5,
            ..CellAdjust::default()
        });
        let metrics = stack.metrics();
        assert_eq!(metrics.width, natural.width);
        assert_eq!(metrics.height, 30);
        assert_eq!(metrics.baseline, natural.baseline + 5);
        assert_eq!(metrics.underline_top, natural.underline_top + 5);
        assert_eq!(metrics.strikeout_top, natural.strikeout_top + 5);
    }

    #[test]
    fn a_shorter_line_still_keeps_its_lines_inside_the_cell() {
        let stack = adjusted(CellAdjust {
            line_height: 0.5,
            ..CellAdjust::default()
        });
        let metrics = stack.metrics();
        assert_eq!(metrics.height, 10);
        assert!(metrics.baseline >= 1 && metrics.baseline <= metrics.height);
        assert!(metrics.underline_top + stack.placement().underline_stroke <= metrics.height);
        assert!(metrics.strikeout_top + metrics.stroke <= metrics.height);
    }

    #[test]
    fn cell_width_widens_the_cell_and_centres_the_glyph_in_it() {
        let natural = fonts().metrics();
        let stack = adjusted(CellAdjust {
            cell_width: 1.5,
            ..CellAdjust::default()
        });
        assert_eq!(stack.metrics().width, 12);
        assert_eq!(stack.metrics().height, natural.height);
        assert_eq!(stack.placement().natural_width, 8);
        assert_eq!(stack.placement().inset, 2);

        let mut plain = fonts();
        let mut wide = stack;
        let glyph = glyph_of(&mut plain, FaceStyle::Regular, 'M');
        let at_natural = plain.rasterize(glyph, 1).unwrap();
        let at_wide = wide.rasterize(glyph, 1).unwrap();
        assert_eq!(at_wide.left, at_natural.left + 2);
        assert_eq!(at_wide.width, at_natural.width);
    }

    #[test]
    fn shaped_glyphs_land_on_their_own_widened_cells() {
        let mut wide = FontStack::build_with(
            &FontOptions {
                system_fonts: false,
                ligatures: true,
                cell: CellAdjust {
                    cell_width: 1.5,
                    ..CellAdjust::default()
                },
                ..FontOptions::default()
            },
            None,
        )
        .unwrap();
        let font = wide.primary_of(FaceStyle::Regular);
        let shaped = wide.shape(font, "->");
        assert!(!shaped.is_empty());
        for glyph in shaped.iter() {
            let within = glyph.dx.rem_euclid(12);
            assert!(
                within <= 1 || within >= 11,
                "{:?} does not start on a 12 px cell",
                glyph
            );
        }
    }

    #[test]
    fn a_large_baseline_offset_is_held_inside_the_cell() {
        for offset in [-64.0, 64.0] {
            let metrics = adjusted(CellAdjust {
                baseline_offset: offset,
                ..CellAdjust::default()
            })
            .metrics();
            assert!(metrics.baseline >= 1 && metrics.baseline <= metrics.height);
        }
        let lowered = adjusted(CellAdjust {
            baseline_offset: 2.0,
            ..CellAdjust::default()
        })
        .metrics();
        assert_eq!(lowered.baseline, fonts().metrics().baseline + 2);
    }

    #[test]
    fn underline_adjustments_apply_on_top_of_the_font_and_stay_inside_the_cell() {
        let natural = fonts().metrics();
        let moved = adjusted(CellAdjust {
            underline_offset: 1.0,
            underline_thickness: 1.0,
            ..CellAdjust::default()
        });
        assert_eq!(moved.metrics().underline_top, natural.underline_top + 1);
        assert_eq!(moved.placement().underline_stroke, natural.stroke + 1);
        assert_eq!(moved.metrics().stroke, natural.stroke);

        for (offset, thickness) in [(64.0, 64.0), (-64.0, 0.0), (64.0, -64.0)] {
            let stack = adjusted(CellAdjust {
                underline_offset: offset,
                underline_thickness: thickness,
                ..CellAdjust::default()
            });
            let (metrics, placement) = (stack.metrics(), stack.placement());
            assert!(placement.underline_stroke >= 1);
            assert!(
                metrics.underline_top + placement.underline_stroke <= metrics.height,
                "{} {}: {:?} {:?}",
                offset,
                thickness,
                metrics,
                placement
            );
        }
    }

    #[test]
    fn default_adjustments_leave_the_cell_as_the_font_measures_it() {
        let stack = adjusted(CellAdjust::default());
        assert_eq!(stack.metrics(), fonts().metrics());
        assert_eq!(stack.placement().inset, 0);
        assert_eq!(stack.placement().underline_stroke, stack.metrics().stroke);
    }

    #[test]
    fn a_weight_request_on_a_static_font_changes_nothing() {
        let stack = FontStack::build_with(
            &FontOptions {
                system_fonts: false,
                weight: 500,
                ..FontOptions::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(stack.metrics(), fonts().metrics());
        assert!(stack.coords.iter().all(Vec::is_empty));
    }

    #[test]
    fn a_heavy_weight_on_the_bundled_font_draws_regular_text_with_the_bold_face() {
        let stack = |weight| {
            FontStack::build_with(
                &FontOptions {
                    system_fonts: false,
                    weight,
                    ..FontOptions::default()
                },
                None,
            )
            .unwrap()
        };
        let regular = stack(400);
        assert_eq!(regular.primary_of(FaceStyle::Regular), FontId(0));
        assert_eq!(regular.primary_of(FaceStyle::Italic), FontId(2));
        let heavy = stack(600);
        assert_eq!(heavy.primary_of(FaceStyle::Regular), FontId(1));
        assert_eq!(heavy.primary_of(FaceStyle::Italic), FontId(3));
        assert_eq!(heavy.primary_of(FaceStyle::Bold), FontId(1));
        let light = stack(200);
        assert_eq!(light.primary_of(FaceStyle::Bold), FontId(1));
    }

    #[test]
    fn bold_is_three_hundred_heavier_and_capped() {
        assert_eq!(bold_weight(400), 700);
        assert_eq!(bold_weight(500), 800);
        assert_eq!(bold_weight(700), 900);
        assert_eq!(
            FaceRequest::weighted(FaceStyle::BoldItalic, 300).weight,
            600
        );
        assert!(FaceRequest::weighted(FaceStyle::BoldItalic, 300).italic);
    }

    #[test]
    fn features_turn_shaping_on_and_keep_ligatures_off_when_asked() {
        let zero = FontFeature::parse("zero").unwrap();
        let stack = FontStack::build_with(
            &FontOptions {
                system_fonts: false,
                ligatures: false,
                features: vec![zero, FontFeature::parse("+calt").unwrap()],
                ..FontOptions::default()
            },
            None,
        )
        .unwrap();
        assert!(stack.shapes_runs());
        let tags: Vec<(&[u8; 4], u16)> = stack
            .features
            .iter()
            .map(|feature| (&feature.tag, feature.value))
            .collect();
        assert!(tags.contains(&(b"liga", 0)));
        assert!(tags.contains(&(b"dlig", 0)));
        assert!(!tags.contains(&(b"calt", 0)));
        assert!(tags.contains(&(b"calt", 1)));
        assert!(tags.contains(&(b"zero", 1)));

        let unshaped = FontStack::build_with(
            &FontOptions {
                system_fonts: false,
                ligatures: false,
                ..FontOptions::default()
            },
            None,
        )
        .unwrap();
        assert!(!unshaped.shapes_runs());
    }

    fn arrow_ligates(ligatures: bool, features: &[&str]) -> bool {
        let mut stack = FontStack::build_with(
            &FontOptions {
                system_fonts: false,
                ligatures,
                features: features
                    .iter()
                    .map(|feature| FontFeature::parse(feature).unwrap())
                    .collect(),
                ..FontOptions::default()
            },
            None,
        )
        .unwrap();
        let font = stack.primary_of(FaceStyle::Regular);
        let nominal: Vec<GlyphId> = "->"
            .chars()
            .map(|character| glyph_of(&mut stack, FaceStyle::Regular, character).glyph)
            .collect();
        let shaped: Vec<GlyphId> = stack.shape(font, "->").iter().map(|g| g.glyph).collect();
        shaped != nominal
    }

    #[test]
    fn a_listed_feature_changes_what_the_shaper_draws() {
        assert!(
            arrow_ligates(true, &[]),
            "the arrow no longer ligates at all"
        );
        assert!(
            !arrow_ligates(true, &["-calt"]),
            "turning calt off left the arrow joined"
        );
        assert!(
            arrow_ligates(false, &["+calt"]),
            "listing calt with ligatures off did not join the arrow"
        );
        assert!(
            !arrow_ligates(false, &["zero"]),
            "an unrelated feature with ligatures off still joined the arrow"
        );
    }

    #[test]
    fn a_requested_weight_picks_the_matching_installed_face() {
        let Some(discovery) = synthetic_discovery() else {
            return;
        };
        let weight_of = |stack: &FontStack, style: FaceStyle| {
            stack.faces[stack.primary_of(style).0]
                .attributes()
                .weight()
                .0
        };
        let regular = FontStack::build_with(&requested("Iosevka Term"), Some(discovery)).unwrap();
        assert_eq!(weight_of(&regular, FaceStyle::Regular), 400);
        assert_eq!(weight_of(&regular, FaceStyle::Bold), 700);

        let heavy = FontStack::build_with(
            &FontOptions {
                weight: 700,
                ..requested("Iosevka Term")
            },
            Some(discovery),
        )
        .unwrap();
        assert_eq!(
            weight_of(&heavy, FaceStyle::Regular),
            700,
            "a bold regular weight did not pick the bold file"
        );
    }

    #[test]
    fn feature_tags_parse_with_an_optional_sign() {
        assert_eq!(
            FontFeature::parse("-liga"),
            Some(FontFeature {
                tag: *b"liga",
                value: 0
            })
        );
        assert_eq!(
            FontFeature::parse("ss01"),
            Some(FontFeature {
                tag: *b"ss01",
                value: 1
            })
        );
        assert_eq!(FontFeature::parse("ss1"), None);
        assert_eq!(FontFeature::parse("--liga"), None);
    }

    const EMBEDDED_PRIMARY: [FontId; 4] = [FontId(0), FontId(1), FontId(2), FontId(3)];

    #[test]
    fn a_resolved_family_replaces_the_primary_and_measures_the_cell() {
        let Some(discovery) = synthetic_discovery() else {
            return;
        };
        let mut stack = FontStack::build_with(&requested("Iosevka Term"), Some(discovery)).unwrap();
        assert!(
            stack.primary.iter().all(|font| font.0 >= 6),
            "the requested family did not replace the embedded primary: {:?}",
            stack.primary
        );
        assert_eq!(
            stack.metrics(),
            fonts().metrics(),
            "the requested family is the same Iosevka and must measure the same cell"
        );
        let glyph = glyph_of(&mut stack, FaceStyle::Regular, 'A');
        assert!(stack.is_primary(glyph.font));
    }

    #[test]
    fn a_family_that_does_not_resolve_keeps_the_embedded_primary() {
        let Some(discovery) = synthetic_discovery() else {
            return;
        };
        let stack =
            FontStack::build_with(&requested("No Such Family At All"), Some(discovery)).unwrap();
        assert_eq!(stack.primary, EMBEDDED_PRIMARY);
        assert_eq!(stack.metrics(), fonts().metrics());
    }

    #[test]
    fn a_family_request_without_discovery_falls_back_rather_than_failing() {
        let mut stack = FontStack::build_with(&requested("Iosevka Term"), None).unwrap();
        assert_eq!(stack.primary, EMBEDDED_PRIMARY);
        assert!(stack.lookup(FaceStyle::Regular, 'A').is_some());
    }

    #[test]
    fn a_resolved_family_still_falls_back_for_what_it_does_not_carry() {
        let Some(discovery) = synthetic_discovery() else {
            return;
        };
        let mut stack = FontStack::build_with(&requested("Iosevka Term"), Some(discovery)).unwrap();
        let glyph = glyph_of(&mut stack, FaceStyle::Regular, '\u{1f600}');
        assert!(!stack.is_primary(glyph.font));
    }

    #[test]
    fn the_primary_answers_latin_before_any_fallback() {
        let mut fonts = fonts();
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, 'A');
        assert!(fonts.is_primary(glyph.font));
    }

    #[test]
    fn a_codepoint_the_primary_lacks_falls_back() {
        let mut fonts = fonts();
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{4f60}');
        assert!(!fonts.is_primary(glyph.font));
    }

    #[test]
    fn every_style_reaches_the_same_fallback_face() {
        let mut fonts = fonts();
        let regular = glyph_of(&mut fonts, FaceStyle::Regular, '\u{4f60}');
        let bold = glyph_of(&mut fonts, FaceStyle::Bold, '\u{4f60}');
        assert_eq!(regular.font, bold.font);
    }

    #[test]
    fn an_emoji_resolves_to_a_color_glyph() {
        let mut fonts = fonts();
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{1f600}');
        let bitmap = fonts.rasterize(glyph, 2).unwrap();
        assert_eq!(bitmap.content, GlyphContent::Color);
        assert_eq!(
            bitmap.coverage.len(),
            (bitmap.width * bitmap.height * 4) as usize
        );
        assert!(bitmap.coverage.chunks(4).any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn a_fallback_glyph_is_confined_to_its_cell_budget() {
        let mut fonts = fonts();
        let metrics = fonts.metrics();
        for (character, budget) in [('\u{4f60}', 2u32), ('\u{1f600}', 2), ('\u{3042}', 2)] {
            let glyph = glyph_of(&mut fonts, FaceStyle::Regular, character);
            let bitmap = fonts.rasterize(glyph, budget).unwrap();
            assert!(
                bitmap.width <= metrics.width * budget,
                "{:?} is {} px wide",
                character,
                bitmap.width
            );
            assert!(
                bitmap.height <= metrics.height,
                "{:?} is {} px tall",
                character,
                bitmap.height
            );
            assert!(bitmap.left >= 0);
        }
    }

    #[test]
    fn a_narrow_budget_shrinks_the_same_glyph_further() {
        let mut fonts = fonts();
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{1f600}');
        let wide = fonts.rasterize(glyph, 2).unwrap();
        let narrow = fonts.rasterize(glyph, 1).unwrap();
        assert!(
            narrow.width < wide.width,
            "{} vs {}",
            narrow.width,
            wide.width
        );
        assert!(narrow.width <= fonts.metrics().width);
    }

    #[test]
    fn a_codepoint_no_face_carries_resolves_to_nothing() {
        assert!(fonts().lookup(FaceStyle::Regular, '\u{10fffd}').is_none());
    }

    #[test]
    fn a_rendered_glyph_fits_inside_the_cell_box() {
        let mut fonts = fonts();
        let metrics = fonts.metrics();
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, 'M');
        let bitmap = fonts.rasterize(glyph, 1).unwrap();
        assert!(bitmap.width <= metrics.width, "{}", bitmap.width);
        assert!(bitmap.height <= metrics.height, "{}", bitmap.height);
        assert_eq!(
            bitmap.coverage.len(),
            (bitmap.width * bitmap.height) as usize
        );
        assert!(bitmap.coverage.iter().any(|&value| value > 0));
    }

    #[test]
    fn a_blank_glyph_rasterizes_to_nothing() {
        let mut fonts = fonts();
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, ' ');
        assert!(fonts.rasterize(glyph, 1).is_none());
    }

    #[test]
    fn rasterization_is_repeatable() {
        let mut fonts = fonts();
        let glyph = glyph_of(&mut fonts, FaceStyle::Bold, 'g');
        let first = fonts.rasterize(glyph, 1).unwrap();
        let second = fonts.rasterize(glyph, 1).unwrap();
        assert_eq!(first.coverage, second.coverage);
        assert_eq!((first.left, first.top), (second.left, second.top));
    }

    #[test]
    fn a_point_is_a_ninety_sixth_of_an_inch_off_macos_and_a_pixel_on_it() {
        let expected = if cfg!(target_os = "macos") {
            12.0
        } else {
            16.0
        };
        assert_eq!(points_to_pixels(DEFAULT_FONT_POINTS), expected);
        assert_eq!(pixels_to_points(points_to_pixels(17.5)), 17.5);
    }

    #[test]
    fn the_default_point_size_matches_the_cell_the_window_has_always_drawn_off_macos() {
        if cfg!(target_os = "macos") {
            return;
        }
        assert_eq!(points_to_pixels(DEFAULT_FONT_POINTS), DEFAULT_FONT_SIZE);
    }

    #[test]
    fn an_out_of_range_font_size_is_rejected() {
        assert!(FontStack::embedded(0.0).is_err());
        assert!(FontStack::embedded(f32::NAN).is_err());
        assert!(FontStack::embedded(10_000.0).is_err());
    }

    #[test]
    fn style_selection_covers_the_four_faces() {
        assert_eq!(FaceStyle::of(false, false), FaceStyle::Regular);
        assert_eq!(FaceStyle::of(true, false), FaceStyle::Bold);
        assert_eq!(FaceStyle::of(false, true), FaceStyle::Italic);
        assert_eq!(FaceStyle::of(true, true), FaceStyle::BoldItalic);
    }

    #[test]
    fn the_embedded_stack_consults_no_system_fonts() {
        assert!(!fonts().has_discovery());
    }

    fn candidate(id: usize, color: bool) -> (Glyph, bool) {
        (
            Glyph {
                font: FontId(id),
                glyph: 1,
            },
            color,
        )
    }

    #[test]
    fn a_color_capable_system_face_is_taken_immediately() {
        let chosen = choose(Some(candidate(9, true)), &[candidate(5, true)]);
        assert_eq!(chosen.unwrap().font, FontId(9));
    }

    #[test]
    fn a_monochrome_system_face_loses_to_a_color_capable_fallback() {
        let chosen = choose(Some(candidate(9, false)), &[candidate(5, true)]);
        assert_eq!(chosen.unwrap().font, FontId(5));
    }

    #[test]
    fn a_monochrome_system_face_still_beats_a_monochrome_fallback() {
        let chosen = choose(Some(candidate(9, false)), &[candidate(4, false)]);
        assert_eq!(chosen.unwrap().font, FontId(9));
    }

    #[test]
    fn the_first_fallback_wins_when_nothing_was_discovered() {
        let chosen = choose(None, &[candidate(4, false), candidate(5, true)]);
        assert_eq!(chosen.unwrap().font, FontId(4));
    }

    #[test]
    fn a_color_fallback_is_reached_past_a_monochrome_one() {
        let chosen = choose(
            Some(candidate(9, false)),
            &[candidate(4, false), candidate(5, true)],
        );
        assert_eq!(chosen.unwrap().font, FontId(5));
    }

    #[test]
    fn nerd_font_icons_resolve_to_the_embedded_symbols_font() {
        let mut fonts = fonts();
        for icon in ['\u{e62b}', '\u{f07b}', '\u{f0520}', '\u{f16a0}', '\u{e0b0}'] {
            let glyph = glyph_of(&mut fonts, FaceStyle::Regular, icon);
            assert!(
                fonts.rasterize(glyph, 1).is_some(),
                "{:?} rendered nothing",
                icon
            );
        }
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{f16a0}');
        assert_eq!(glyph.font, FontId(6));
    }

    #[test]
    fn private_use_icons_skip_system_font_discovery() {
        let mut fonts = FontStack::build(&FontOptions::default()).unwrap();
        for icon in ['\u{e62b}', '\u{f07b}', '\u{f0520}', '\u{f16a0}'] {
            for style in [FaceStyle::Regular, FaceStyle::Bold] {
                let glyph = glyph_of(&mut fonts, style, icon);
                assert_eq!(glyph.font, SYMBOLS_FONT);
            }
        }
        assert_eq!(
            fonts.discovery_queries, 0,
            "system fonts were searched for icons the bundled symbols font has"
        );
        glyph_of(&mut fonts, FaceStyle::Regular, '\u{4f60}');
        fonts.lookup(FaceStyle::Regular, '\u{10fffd}');
        assert!(
            fonts.discovery_queries > 0,
            "characters outside the icon ranges must still reach system fonts"
        );
        assert!(is_private_use('\u{e62b}'));
        assert!(!is_private_use('\u{4f60}'));
    }

    #[test]
    fn nothing_anywhere_chooses_nothing() {
        assert!(choose(None, &[]).is_none());
    }

    #[test]
    fn the_emoji_face_is_the_only_color_capable_embedded_one() {
        let fonts = fonts();
        let capable: Vec<usize> = fonts
            .color_capable
            .iter()
            .enumerate()
            .filter(|(_, capable)| **capable)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(capable, vec![5]);
    }

    #[test]
    fn an_emoji_stays_in_color_even_when_system_fonts_are_consulted() {
        let mut fonts = FontStack::new(DEFAULT_FONT_SIZE, true).unwrap();
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{1f600}');
        assert!(
            fonts.has_color(glyph.font),
            "a monochrome face won the emoji"
        );
        assert_eq!(
            fonts.rasterize(glyph, 2).unwrap().content,
            GlyphContent::Color
        );
    }

    #[test]
    fn a_system_face_is_still_preferred_where_no_color_is_at_stake() {
        let mut fonts = FontStack::new(DEFAULT_FONT_SIZE, true).unwrap();
        if !fonts.has_discovery() {
            return;
        }
        let Some(glyph) = fonts.lookup(FaceStyle::Regular, '\u{2801}') else {
            return;
        };
        assert!(!fonts.is_primary(glyph.font));
        assert!(!fonts.has_color(glyph.font));
        assert!(glyph.font.0 >= 6, "braille came from an embedded face");
    }

    #[test]
    fn a_box_filter_averages_the_source_it_covers() {
        let source = vec![0u8, 100, 200, 255];
        assert_eq!(downscale(&source, 2, 2, 1, 1, 1), vec![138]);
    }

    #[test]
    fn a_box_filter_keeps_channels_separate() {
        let source = vec![10u8, 20, 30, 40, 50, 60, 70, 80];
        assert_eq!(downscale(&source, 2, 1, 4, 1, 1), vec![30, 40, 50, 60]);
    }

    #[test]
    fn a_box_filter_halves_a_quadrant_at_a_time() {
        let source: Vec<u8> = (0..16).map(|value| value * 16).collect();
        assert_eq!(downscale(&source, 4, 4, 1, 2, 2), vec![40, 72, 168, 200]);
    }

    #[test]
    fn a_non_integer_ratio_covers_every_source_pixel_exactly_once() {
        let source = vec![255u8; 9];
        let scaled = downscale(&source, 3, 3, 1, 2, 2);
        assert_eq!(scaled, vec![255; 4]);
        assert_eq!(downscale(&vec![0u8; 9], 3, 3, 1, 2, 2), vec![0; 4]);
    }

    fn fit() -> Fit {
        Fit::new(fonts().metrics(), 1)
    }

    fn mask(left: i32, top: i32, width: u32, height: u32) -> GlyphBitmap {
        GlyphBitmap {
            left,
            top,
            width,
            height,
            coverage: vec![200; (width * height) as usize],
            content: GlyphContent::Mask,
        }
    }

    #[test]
    fn an_oversized_bitmap_is_confined_by_its_tallest_axis() {
        let confined = confine(mask(0, 40, 10, 100), 16, 20);
        assert_eq!(confined.height, 20);
        assert_eq!(confined.width, 2);
        assert_eq!(
            confined.coverage.len(),
            (confined.width * confined.height) as usize
        );
        assert!(confined.coverage.iter().all(|value| *value == 200));
        assert_eq!(confined.top, 8, "the bearing shrinks with the bitmap");
    }

    #[test]
    fn a_fallback_glyph_keeps_its_own_bearing_inside_its_centred_advance() {
        let fit = fit();
        let placed = fit.place(mask(1, 12, 4, 10), 6.0);
        assert_eq!((placed.width, placed.height), (4, 10));
        assert_eq!(placed.left, 1 + 1);
        assert_eq!(placed.top, 12, "the glyph stays on the text baseline");
    }

    #[test]
    fn glyphs_drawn_on_one_grid_keep_their_shared_dots_aligned() {
        let fit = fit();
        let advance = 7.0;
        let spinner = [
            mask(1, 13, 5, 7),
            mask(1, 13, 1, 11),
            mask(1, 9, 5, 7),
            mask(5, 13, 1, 3),
        ];
        let lefts: Vec<i32> = spinner
            .iter()
            .take(3)
            .map(|frame| fit.place(frame.clone(), advance).left)
            .collect();
        assert!(
            lefts.iter().all(|left| *left == lefts[0]),
            "a dot in the first column moved between frames: {:?}",
            lefts
        );
        let tops: Vec<i32> = [&spinner[0], &spinner[1], &spinner[3]]
            .into_iter()
            .map(|frame| fit.place(frame.clone(), advance).top)
            .collect();
        assert!(
            tops.iter().all(|top| *top == tops[0]),
            "a dot in the top row moved between frames: {:?}",
            tops
        );
    }

    #[test]
    fn a_glyph_family_is_scaled_by_its_advance_not_by_each_glyphs_ink() {
        let fit = fit();
        let room = fit.box_width as f32 * FALLBACK_OVERHANG;
        assert_eq!(fit.advance_scale(fit.box_width as f32), 1.0);
        assert_eq!(fit.advance_scale(room), 1.0);
        let wide = 13.4;
        assert!((fit.advance_scale(wide) - room / wide).abs() < f32::EPSILON);
        assert_eq!(fit.advance_scale(f32::NAN), 1.0);
    }

    #[test]
    fn a_symbol_with_a_blank_neighbour_keeps_its_size_and_centres_across_both_cells() {
        let fit = fit().spilling(1);
        let two_cells = (fit.box_width * 2) as f32;
        assert_eq!(fit.advance_scale(two_cells), 1.0);
        assert!(fit.advance_scale(two_cells * 2.0) < 1.0);
        assert_eq!(fit.ink_scale(&mask(0, 10, fit.box_width * 2, 8)), 1.0);
        let placed = fit.place(mask(1, 10, 12, 8), 14.0);
        assert_eq!(placed.left, 1 + 1);
        let narrow = fit.place(mask(1, 10, 4, 8), 6.0);
        assert_eq!(
            narrow.left,
            1 + 1,
            "a symbol that fits its own cell stays there"
        );
    }

    #[test]
    fn a_monochrome_symbol_may_overhang_a_little_rather_than_shrink() {
        let fit = fit();
        let slightly_wide = mask(0, 10, fit.box_width + 1, 8);
        assert_eq!(fit.ink_scale(&slightly_wide), 1.0);
        let color = GlyphBitmap {
            content: GlyphContent::Color,
            coverage: vec![200; ((fit.box_width + 1) * 8 * 4) as usize],
            ..mask(0, 10, fit.box_width + 1, 8)
        };
        assert!(
            fit.ink_scale(&color) < 1.0,
            "an emoji must stay inside its box"
        );
        let far_too_wide = mask(0, 10, fit.box_width * 2, 8);
        assert!(fit.ink_scale(&far_too_wide) < 1.0);
    }

    #[test]
    fn a_glyph_poking_out_of_the_cell_is_nudged_back_in() {
        let fit = fit();
        let metrics = fonts().metrics();
        let above = fit.place(mask(0, metrics.baseline as i32 + 3, 4, 6), 8.0);
        assert_eq!(above.top, metrics.baseline as i32);
        let below = fit.place(mask(0, -6, 4, 6), 8.0);
        assert_eq!(
            metrics.baseline as i32 - below.top + below.height as i32,
            metrics.height as i32
        );
    }

    #[test]
    fn a_full_width_fallback_glyph_keeps_the_fonts_own_bearing() {
        let mut fonts = fonts();
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{4f60}');
        let font = fonts.faces[glyph.font.0];
        let raw = fonts
            .render(font, &[], glyph.glyph, DEFAULT_FONT_SIZE, FALLBACK_SOURCES)
            .unwrap();
        let placed = fonts.rasterize(glyph, 2).unwrap();
        assert_eq!((placed.left, placed.top), (raw.left, raw.top));
        assert_eq!((placed.width, placed.height), (raw.width, raw.height));
    }

    #[test]
    fn ideographic_punctuation_keeps_the_place_its_font_gives_it() {
        let mut fonts = fonts();
        let metrics = fonts.metrics();
        let glyph = glyph_of(&mut fonts, FaceStyle::Regular, '\u{3001}');
        let comma = fonts.rasterize(glyph, 2).unwrap();
        let centre = comma.left + comma.width as i32 / 2;
        assert!(
            centre < metrics.width as i32,
            "the ideographic comma was pulled to the middle of its cells: {}",
            centre
        );
        let bottom = metrics.baseline as i32 - comma.top + comma.height as i32;
        assert!(
            bottom >= metrics.baseline as i32 - 1,
            "the ideographic comma was lifted off the baseline: its ink ends at {}",
            bottom
        );
    }
}

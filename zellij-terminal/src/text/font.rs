use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{anyhow, Result};
use swash::scale::{ScaleContext, Source, StrikeWith};
use swash::shape::ShapeContext;
use swash::text::{Codepoint, Script};
use swash::FontRef;

use crate::text::discovery::Discovery;

pub use swash::GlyphId;

mod clusters;
mod files;
mod lookup;
mod raster;
mod shaping;

use files::{face, font_file, has_weight_axis, is_color_capable, weight_coords, STATIC_FACE};
use lookup::is_private_use;
use raster::{as_drawn, crop_to_cell, Fit};
use shaping::effective_features;

pub const DEFAULT_FONT_SIZE: f32 = 16.0;
pub const DEFAULT_FONT_POINTS: f32 = 12.0;
const PIXELS_PER_INCH: f32 = if cfg!(target_os = "macos") {
    72.0
} else {
    96.0
};
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
const MAX_CLUSTER_FONTS: usize = 4096;
const FALLBACK_OVERHANG: f32 = 1.25;
const CLUSTER_ADVANCE_TOLERANCE: f32 = 1.6;

const PRIMARY_SOURCES: &[Source] = &[Source::Outline];
const FALLBACK_SOURCES: &[Source] = &[
    Source::ColorOutline(0),
    Source::ColorBitmap(StrikeWith::BestFit),
    Source::Outline,
];

const REGULAR: &[u8] = include_bytes!("../../assets/fonts/IosevkaTerm-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../assets/fonts/IosevkaTerm-Bold.ttf");
const ITALIC: &[u8] = include_bytes!("../../assets/fonts/IosevkaTerm-Italic.ttf");
const BOLD_ITALIC: &[u8] = include_bytes!("../../assets/fonts/IosevkaTerm-BoldItalic.ttf");
const CJK: &[u8] = include_bytes!("../../assets/fonts/NotoSansMonoCJK-Regular.otf");
#[cfg(bundled_emoji)]
const EMOJI: Option<&[u8]> = Some(include_bytes!("../../assets/fonts/NotoColorEmoji.ttf"));
#[cfg(not(bundled_emoji))]
const EMOJI: Option<&[u8]> = None;
const SYMBOLS: &[u8] = include_bytes!("../../assets/fonts/SymbolsNerdFontMono-Regular.ttf");
const SYMBOLS_FONT: FontId = FontId(6);
const EMOJI_FONT: FontId = FontId(5);
const EMOJI_PROBE: char = '\u{1f600}';
const TEXT_PRESENTATION: char = '\u{fe0e}';

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

/// A face from a family set for one style. One that measures the same cell
/// as the regular face is drawn as-is, like a primary face; any other is
/// fitted into the cell like a fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StyleFace {
    font: FontId,
    in_cell: bool,
}

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
    /// Families for one style each, taking that style's face from `family`'s
    /// place. A style without one keeps `family`'s face.
    pub bold_family: Option<String>,
    pub italic_family: Option<String>,
    pub bold_italic_family: Option<String>,
    pub size: f32,
    pub system_fonts: bool,
    pub ligatures: bool,
    pub weight: u16,
    pub features: Vec<FontFeature>,
    pub cell: CellAdjust,
}

impl FontOptions {
    /// The family set for `style` alone, if any.
    fn style_family(&self, style: FaceStyle) -> Option<&str> {
        match style {
            FaceStyle::Regular => None,
            FaceStyle::Bold => self.bold_family.as_deref(),
            FaceStyle::Italic => self.italic_family.as_deref(),
            FaceStyle::BoldItalic => self.bold_italic_family.as_deref(),
        }
    }
}

impl Default for FontOptions {
    fn default() -> Self {
        Self {
            family: None,
            bold_family: None,
            italic_family: None,
            bold_italic_family: None,
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
    pub advance: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Shaping {
    Run,
    Cluster,
}

pub enum ClusterRaster {
    Sprite(GlyphBitmap),
    Blank,
    Uncovered,
}

pub fn script_of(text: &str) -> Script {
    text.chars()
        .map(|character| character.script())
        .find(|script| !matches!(script, Script::Common | Script::Inherited | Script::Unknown))
        .unwrap_or(Script::Common)
}

pub fn is_joining_control(character: char) -> bool {
    matches!(character,
        '\u{ad}' | '\u{34f}' | '\u{feff}'
        | '\u{200b}'..='\u{200d}'
        | '\u{2060}'..='\u{2064}'
        | '\u{fe00}'..='\u{fe0f}'
        | '\u{e0020}'..='\u{e007f}'
        | '\u{e0100}'..='\u{e01ef}')
}

pub fn wants_color(text: &str) -> bool {
    let mut zwj = false;
    let mut pictographic = false;
    for character in text.chars() {
        match character {
            '\u{fe0f}'
            | '\u{1f1e6}'..='\u{1f1ff}'
            | '\u{1f3fb}'..='\u{1f3ff}'
            | '\u{e0020}'..='\u{e007f}' => return true,
            '\u{200d}' => zwj = true,
            '\u{1f000}'..='\u{1faff}' => pictographic = true,
            _ => {},
        }
    }
    zwj && pictographic
}

pub struct FontStack {
    faces: Vec<FontRef<'static>>,
    coords: Vec<Vec<i16>>,
    color_capable: Vec<bool>,
    primary: [FontId; 4],
    /// Faces from the families set for one style each, tried before that
    /// style's primary face.
    styled: [Option<StyleFace>; 4],
    fallbacks: Vec<FontId>,
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
    cluster_fonts: HashMap<FaceStyle, HashMap<String, Option<FontId>>>,
    shaped: HashMap<(FontId, Shaping), HashMap<String, Rc<[ShapedGlyph]>>>,
    discovery_queries: usize,
}

impl FontStack {
    pub fn build(options: &FontOptions) -> Result<Self> {
        let discovery = if options.system_fonts {
            Discovery::open()
        } else {
            None
        };
        Self::assemble(options, discovery)
    }

    fn assemble(options: &FontOptions, discovery: Option<Discovery>) -> Result<Self> {
        Self::assemble_with_emoji(options, discovery, EMOJI)
    }

    fn assemble_with_emoji(
        options: &FontOptions,
        discovery: Option<Discovery>,
        emoji: Option<&'static [u8]>,
    ) -> Result<Self> {
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
            match emoji {
                Some(emoji) => face(emoji, "NotoColorEmoji")?,
                None => face(SYMBOLS, "SymbolsNerdFontMono-Regular")?,
            },
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
            styled: [None; 4],
            fallbacks: match emoji {
                Some(_) => vec![FontId(4), EMOJI_FONT, SYMBOLS_FONT],
                None => vec![FontId(4), SYMBOLS_FONT],
            },
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
            cluster_fonts: HashMap::new(),
            shaped: HashMap::new(),
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

        stack.adopt_system_emoji();

        let regular = stack.primary[0].0;
        let (metrics, placement) = derive_metrics(
            &stack.faces[regular],
            &stack.coords[regular],
            size,
            &options.cell,
        );
        stack.metrics = metrics;
        stack.placement = placement;
        stack.styled = stack.requested_styles(options);
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

    /// The face text of `style` is shaped and drawn in: the style's own
    /// family when that shares the cell, else the main family's face.
    pub fn primary_of(&self, style: FaceStyle) -> FontId {
        match self.styled[style.index()] {
            Some(styled) if styled.in_cell => styled.font,
            _ => self.primary[style.index()],
        }
    }
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
pub(crate) mod test_helpers;

#[cfg(test)]
mod tests;

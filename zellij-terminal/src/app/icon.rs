use crate::app::model::{Pill, PillColor, TrayLook};

pub const DOT_BELOW: u32 = 20;
pub const ATTENTION_COLOR: [u8; 3] = [255, 140, 0];
pub const ERROR_COLOR: [u8; 3] = [229, 72, 77];
pub const DONE_COLOR: [u8; 3] = [48, 164, 108];
pub const WORKING_COLOR: [u8; 3] = [62, 130, 247];
pub const PILL_TEXT: [u8; 3] = [255, 255, 255];
#[cfg_attr(not(any(target_os = "macos", windows, test)), allow(dead_code))]
pub const LIGHT_INK: [u8; 3] = [236, 236, 236];
#[cfg_attr(not(any(target_os = "macos", windows, test)), allow(dead_code))]
pub const DARK_INK: [u8; 3] = [24, 24, 24];
const DIMMED: f32 = 0.45;
const SAMPLES: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    fn blank(width: u32, height: u32) -> Self {
        Rgba {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        }
    }

    #[cfg(test)]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[at],
            self.pixels[at + 1],
            self.pixels[at + 2],
            self.pixels[at + 3],
        ]
    }

    fn put(&mut self, x: u32, y: u32, color: [u8; 3], alpha: f32) {
        let at = ((y * self.width + x) * 4) as usize;
        let alpha = alpha.clamp(0.0, 1.0);
        let below = self.pixels[at + 3] as f32 / 255.0;
        let out = alpha + below * (1.0 - alpha);
        if out <= 0.0 {
            return;
        }
        for channel in 0..3 {
            let top = color[channel] as f32;
            let under = self.pixels[at + channel] as f32;
            self.pixels[at + channel] =
                ((top * alpha + under * below * (1.0 - alpha)) / out).round() as u8;
        }
        self.pixels[at + 3] = (out * 255.0).round() as u8;
    }

    #[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
    pub fn to_argb_big_endian(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len());
        for pixel in self.pixels.chunks_exact(4) {
            out.extend_from_slice(&[pixel[3], pixel[0], pixel[1], pixel[2]]);
        }
        out
    }

    #[cfg_attr(not(any(windows, test)), allow(dead_code))]
    pub fn to_bgra(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len());
        for pixel in self.pixels.chunks_exact(4) {
            out.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
        out
    }
}

fn hexagon_contains(x: f32, y: f32, radius: f32) -> bool {
    let (dx, dy) = ((x - 0.5).abs(), (y - 0.5).abs());
    let half_width = radius * 0.866_025_4;
    if dx > half_width {
        return false;
    }
    dy <= radius - dx * 0.577_350_3
}

fn distance_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let (apx, apy) = (p.0 - a.0, p.1 - a.1);
    let length = abx * abx + aby * aby;
    let t = if length == 0.0 {
        0.0
    } else {
        ((apx * abx + apy * aby) / length).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + abx * t - p.0, a.1 + aby * t - p.1);
    (cx * cx + cy * cy).sqrt()
}

fn prompt_contains(x: f32, y: f32) -> bool {
    let stroke = 0.045;
    let chevron = distance_to_segment((x, y), (0.36, 0.38), (0.48, 0.5)) <= stroke
        || distance_to_segment((x, y), (0.48, 0.5), (0.36, 0.62)) <= stroke;
    let underscore = (0.51..=0.66).contains(&x) && (0.57..=0.645).contains(&y);
    chevron || underscore
}

pub fn mark_contains(x: f32, y: f32) -> bool {
    let ring = hexagon_contains(x, y, 0.49) && !hexagon_contains(x, y, 0.4);
    let inner = hexagon_contains(x, y, 0.33) && !prompt_contains(x, y);
    ring || inner
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PillShape {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    scale: u32,
}

impl PillShape {
    fn contains(&self, x: f32, y: f32, grow: f32) -> bool {
        let radius = self.height / 2.0 + grow;
        let (left, right) = (
            self.x + self.height / 2.0,
            self.x + self.width - self.height / 2.0,
        );
        let cy = self.y + self.height / 2.0;
        if y < cy - radius || y > cy + radius {
            return false;
        }
        let cx = x.clamp(left, right.max(left));
        let (dx, dy) = (x - cx, y - cy);
        dx * dx + dy * dy <= radius * radius
    }
}

fn pill_shape(size: u32, label: &str) -> PillShape {
    let size_f = size as f32;
    if size < DOT_BELOW {
        let diameter = (size_f * 0.5).round().max(4.0);
        return PillShape {
            x: size_f - diameter,
            y: 0.0,
            width: diameter,
            height: diameter,
            scale: 0,
        };
    }
    let height = (size_f * 0.56).round();
    let scale = (((height - 2.0) * 0.7) / 5.0).floor().max(1.0) as u32;
    let glyphs = label.chars().count() as u32;
    let text_width = (glyphs * 3 + glyphs.saturating_sub(1)) * scale;
    let width = (text_width as f32 + height * 0.55).max(height).min(size_f);
    PillShape {
        x: size_f - width,
        y: 0.0,
        width,
        height,
        scale,
    }
}

fn glyph(ch: char) -> [u8; 5] {
    match ch {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        _ => [0; 5],
    }
}

fn label_pixels(shape: &PillShape, label: &str) -> Vec<(u32, u32)> {
    if shape.scale == 0 {
        return vec![];
    }
    let scale = shape.scale;
    let glyphs = label.chars().count() as u32;
    let text_width = (glyphs * 3 + glyphs.saturating_sub(1)) * scale;
    let text_height = 5 * scale;
    let left = (shape.x + (shape.width - text_width as f32) / 2.0)
        .round()
        .max(0.0) as u32;
    let top = (shape.y + (shape.height - text_height as f32) / 2.0)
        .round()
        .max(0.0) as u32;
    let mut pixels = vec![];
    for (index, ch) in label.chars().enumerate() {
        let origin = left + index as u32 * 4 * scale;
        for (row, bits) in glyph(ch).iter().enumerate() {
            for column in 0..3u32 {
                if bits & (0b100 >> column) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        pixels.push((origin + column * scale + dx, top + row as u32 * scale + dy));
                    }
                }
            }
        }
    }
    pixels
}

fn pill_color(pill: &Pill) -> [u8; 3] {
    match pill.color {
        PillColor::Attention => ATTENTION_COLOR,
        PillColor::Error => ERROR_COLOR,
        PillColor::Done => DONE_COLOR,
        PillColor::Working => WORKING_COLOR,
    }
}

pub fn render(size: u32, look: &TrayLook, ink: [u8; 3]) -> Rgba {
    let size = size.max(8);
    let mut image = Rgba::blank(size, size);
    let label = look.pill.map(|pill| pill.label()).unwrap_or_default();
    let shape = look.pill.map(|_| pill_shape(size, &label));
    let gap = (size as f32 / 16.0).max(1.0);
    let mark_alpha = if look.dimmed { DIMMED } else { 1.0 };
    let step = 1.0 / SAMPLES as f32;
    for y in 0..size {
        for x in 0..size {
            let mut mark = 0u32;
            let mut pill = 0u32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) * step;
                    let py = y as f32 + (sy as f32 + 0.5) * step;
                    if let Some(shape) = &shape {
                        if shape.contains(px, py, 0.0) {
                            pill += 1;
                            continue;
                        }
                        if shape.contains(px, py, gap) {
                            continue;
                        }
                    }
                    if mark_contains(px / size as f32, py / size as f32) {
                        mark += 1;
                    }
                }
            }
            let total = (SAMPLES * SAMPLES) as f32;
            if mark > 0 {
                image.put(x, y, ink, mark as f32 / total * mark_alpha);
            }
            if pill > 0 {
                if let Some(color) = look.pill.as_ref().map(pill_color) {
                    image.put(x, y, color, pill as f32 / total);
                }
            }
        }
    }
    if let Some(shape) = &shape {
        for (x, y) in label_pixels(shape, &label) {
            if x < size && y < size {
                image.put(x, y, PILL_TEXT, 1.0);
            }
        }
    }
    image
}

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub const SNI_SIZES: [u32; 5] = [16, 22, 24, 32, 48];

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
#[derive(Default)]
pub struct IconCache {
    entries: Vec<((u32, TrayLook, [u8; 3]), Rgba)>,
}

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
impl IconCache {
    pub fn get(&mut self, size: u32, look: &TrayLook, ink: [u8; 3]) -> Rgba {
        let key = (size, look.clone(), ink);
        if let Some((_, image)) = self.entries.iter().find(|(k, _)| k == &key) {
            return image.clone();
        }
        let image = render(size, look, ink);
        if self.entries.len() > 32 {
            self.entries.remove(0);
        }
        self.entries.push((key, image.clone()));
        image
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calm() -> TrayLook {
        TrayLook {
            pill: None,
            dimmed: false,
        }
    }

    fn with_pill(color: PillColor, count: usize) -> TrayLook {
        TrayLook {
            pill: Some(Pill { color, count }),
            dimmed: false,
        }
    }

    fn has_color(image: &Rgba, color: [u8; 3]) -> bool {
        image
            .pixels
            .chunks_exact(4)
            .any(|p| p[3] == 255 && [p[0], p[1], p[2]] == color)
    }

    #[test]
    fn the_calm_mark_is_one_ink_with_a_transparent_corner_and_centre_cutout() {
        for size in SNI_SIZES {
            let image = render(size, &calm(), LIGHT_INK);
            assert_eq!(image.pixels.len(), (size * size * 4) as usize);
            assert_eq!(image.pixel(0, 0)[3], 0, "corners stay clear at {}", size);
            assert!(image
                .pixels
                .chunks_exact(4)
                .all(|p| p[3] == 0 || [p[0], p[1], p[2]] == LIGHT_INK));
            assert!(image.pixels.chunks_exact(4).any(|p| p[3] == 255));
            assert!(!has_color(&image, ATTENTION_COLOR));
        }
    }

    #[test]
    fn needs_you_draws_an_attention_pill_with_digits_from_twenty_pixels() {
        let image = render(32, &with_pill(PillColor::Attention, 3), LIGHT_INK);
        assert!(has_color(&image, ATTENTION_COLOR));
        assert!(has_color(&image, PILL_TEXT));
        assert_eq!(
            image.pixel(23, 1)[3],
            255,
            "the pill sits in the top-right corner"
        );
        let error = render(32, &with_pill(PillColor::Error, 1), LIGHT_INK);
        assert!(has_color(&error, ERROR_COLOR));
        assert!(!has_color(&error, ATTENTION_COLOR));
        let done = render(32, &with_pill(PillColor::Done, 1), LIGHT_INK);
        assert!(has_color(&done, DONE_COLOR));
        let working = render(32, &with_pill(PillColor::Working, 1), LIGHT_INK);
        assert!(has_color(&working, WORKING_COLOR));
    }

    #[test]
    fn below_twenty_pixels_the_pill_is_a_plain_dot() {
        let image = render(16, &with_pill(PillColor::Attention, 7), LIGHT_INK);
        assert!(has_color(&image, ATTENTION_COLOR));
        assert!(
            !image
                .pixels
                .chunks_exact(4)
                .any(|p| p[3] == 255 && [p[0], p[1], p[2]] == PILL_TEXT),
            "digits are unreadable at 16 px, so none are drawn"
        );
    }

    #[test]
    fn nine_plus_is_wider_than_one_digit() {
        let one = pill_shape(32, "1");
        let many = pill_shape(32, "9+");
        assert!(many.width > one.width);
        assert!(many.width <= 32.0);
    }

    #[test]
    fn paused_dims_the_mark_but_not_the_pill() {
        let mut look = with_pill(PillColor::Attention, 2);
        look.dimmed = true;
        let image = render(32, &look, LIGHT_INK);
        let strongest_mark = image
            .pixels
            .chunks_exact(4)
            .filter(|p| [p[0], p[1], p[2]] == LIGHT_INK)
            .map(|p| p[3])
            .max()
            .unwrap();
        assert!(strongest_mark < 128);
        assert!(has_color(&image, ATTENTION_COLOR));
    }

    #[test]
    fn the_cache_hands_back_the_same_pixels_without_redrawing() {
        let mut cache = IconCache::default();
        let first = cache.get(22, &calm(), DARK_INK);
        let second = cache.get(22, &calm(), DARK_INK);
        assert_eq!(first, second);
        assert_eq!(cache.entries.len(), 1);
        cache.get(22, &with_pill(PillColor::Error, 1), DARK_INK);
        assert_eq!(cache.entries.len(), 2);
    }

    #[test]
    fn argb_and_bgra_reorder_the_channels() {
        let mut image = Rgba::blank(1, 1);
        image.pixels = vec![1, 2, 3, 4];
        assert_eq!(image.to_argb_big_endian(), vec![4, 1, 2, 3]);
        assert_eq!(image.to_bgra(), vec![3, 2, 1, 4]);
    }
}

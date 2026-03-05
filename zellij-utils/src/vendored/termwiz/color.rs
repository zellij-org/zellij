//! Colors for attributes
// for FromPrimitive
#![allow(clippy::useless_attribute)]

use num_derive::*;
#[cfg(feature = "use_serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};
pub use wezterm_color_types::{LinearRgba, SrgbaTuple};
use wezterm_dynamic::{FromDynamic, FromDynamicOptions, ToDynamic, Value};

#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq, Eq, FromDynamic, ToDynamic)]
#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[repr(u8)]
/// These correspond to the classic ANSI color indices and are
/// used for convenience/readability in code
pub enum AnsiColor {
    /// "Dark" black
    Black = 0,
    /// Dark red
    Maroon,
    /// Dark green
    Green,
    /// "Dark" yellow
    Olive,
    /// Dark blue
    Navy,
    /// Dark purple
    Purple,
    /// "Dark" cyan
    Teal,
    /// "Dark" white
    Silver,
    /// "Bright" black
    Grey,
    /// Bright red
    Red,
    /// Bright green
    Lime,
    /// Bright yellow
    Yellow,
    /// Bright blue
    Blue,
    /// Bright purple
    Fuchsia,
    /// Bright Cyan/Aqua
    Aqua,
    /// Bright white
    White,
}

impl From<AnsiColor> for u8 {
    fn from(col: AnsiColor) -> u8 {
        col as u8
    }
}

/// Describes a color in the SRGB colorspace using red, green and blue
/// components in the range 0-255.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub struct RgbColor {
    bits: u32,
}

impl Into<SrgbaTuple> for RgbColor {
    fn into(self) -> SrgbaTuple {
        self.to_tuple_rgba()
    }
}

impl RgbColor {
    /// Construct a color from discrete red, green, blue values
    /// in the range 0-255.
    pub const fn new_8bpc(red: u8, green: u8, blue: u8) -> Self {
        Self {
            bits: ((red as u32) << 16) | ((green as u32) << 8) | blue as u32,
        }
    }

    /// Construct a color from discrete red, green, blue values
    /// in the range 0.0-1.0 in the sRGB colorspace.
    pub fn new_f32(red: f32, green: f32, blue: f32) -> Self {
        let red = (red * 255.) as u8;
        let green = (green * 255.) as u8;
        let blue = (blue * 255.) as u8;
        Self::new_8bpc(red, green, blue)
    }

    /// Returns red, green, blue as 8bpc values.
    /// Will convert from 10bpc if that is the internal storage.
    pub fn to_tuple_rgb8(self) -> (u8, u8, u8) {
        (
            (self.bits >> 16) as u8,
            (self.bits >> 8) as u8,
            self.bits as u8,
        )
    }

    /// Returns red, green, blue as floating point values in the range 0.0-1.0.
    /// An alpha channel with the value of 1.0 is included.
    /// The values are in the sRGB colorspace.
    pub fn to_tuple_rgba(self) -> SrgbaTuple {
        SrgbaTuple(
            (self.bits >> 16) as u8 as f32 / 255.0,
            (self.bits >> 8) as u8 as f32 / 255.0,
            self.bits as u8 as f32 / 255.0,
            1.0,
        )
    }

    /// Returns red, green, blue as floating point values in the range 0.0-1.0.
    /// An alpha channel with the value of 1.0 is included.
    /// The values are converted from sRGB to linear colorspace.
    pub fn to_linear_tuple_rgba(self) -> LinearRgba {
        self.to_tuple_rgba().to_linear()
    }

    /// Construct a color from an X11/SVG/CSS3 color name.
    /// Returns None if the supplied name is not recognized.
    /// The list of names can be found here:
    /// <https://en.wikipedia.org/wiki/X11_color_names>
    pub fn from_named(name: &str) -> Option<RgbColor> {
        Some(SrgbaTuple::from_named(name)?.into())
    }

    /// Returns a string of the form `#RRGGBB`
    pub fn to_rgb_string(self) -> String {
        let (red, green, blue) = self.to_tuple_rgb8();
        format!("#{:02x}{:02x}{:02x}", red, green, blue)
    }

    /// Returns a string of the form `rgb:RRRR/GGGG/BBBB`
    pub fn to_x11_16bit_rgb_string(self) -> String {
        let (red, green, blue) = self.to_tuple_rgb8();
        format!(
            "rgb:{:02x}{:02x}/{:02x}{:02x}/{:02x}{:02x}",
            red, red, green, green, blue, blue
        )
    }

    /// Construct a color from a string of the form `#RRGGBB` where
    /// R, G and B are all hex digits.
    /// `hsl:hue sat light` is also accepted, and allows specifying a color
    /// in the HSL color space, where `hue` is measure in degrees and has
    /// a range of 0-360, and both `sat` and `light` are specified in percentage
    /// in the range 0-100.
    pub fn from_rgb_str(s: &str) -> Option<RgbColor> {
        let srgb: SrgbaTuple = s.parse().ok()?;
        Some(srgb.into())
    }

    /// Construct a color from an SVG/CSS3 color name.
    /// or from a string of the form `#RRGGBB` where
    /// R, G and B are all hex digits.
    /// `hsl:hue sat light` is also accepted, and allows specifying a color
    /// in the HSL color space, where `hue` is measure in degrees and has
    /// a range of 0-360, and both `sat` and `light` are specified in percentage
    /// in the range 0-100.
    /// Returns None if the supplied name is not recognized.
    /// The list of names can be found here:
    /// <https://ogeon.github.io/docs/palette/master/palette/named/index.html>
    pub fn from_named_or_rgb_string(s: &str) -> Option<Self> {
        RgbColor::from_rgb_str(&s).or_else(|| RgbColor::from_named(&s))
    }
}

impl From<SrgbaTuple> for RgbColor {
    fn from(srgb: SrgbaTuple) -> RgbColor {
        let SrgbaTuple(r, g, b, _) = srgb;
        Self::new_f32(r, g, b)
    }
}

/// This is mildly unfortunate: in order to round trip RgbColor with serde
/// we need to provide a Serialize impl equivalent to the Deserialize impl
/// below.  We use the impl below to allow more flexible specification of
/// color strings in the config file.  A side effect of doing it this way
/// is that we have to serialize RgbColor as a 7-byte string when we could
/// otherwise serialize it as a 3-byte array.  There's probably a way
/// to make this work more efficiently, but for now this will do.
#[cfg(feature = "use_serde")]
impl Serialize for RgbColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = self.to_rgb_string();
        s.serialize(serializer)
    }
}

#[cfg(feature = "use_serde")]
impl<'de> Deserialize<'de> for RgbColor {
    fn deserialize<D>(deserializer: D) -> Result<RgbColor, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        RgbColor::from_named_or_rgb_string(&s)
            .ok_or_else(|| format!("unknown color name: {}", s))
            .map_err(serde::de::Error::custom)
    }
}

impl ToDynamic for RgbColor {
    fn to_dynamic(&self) -> Value {
        self.to_rgb_string().to_dynamic()
    }
}

impl FromDynamic for RgbColor {
    fn from_dynamic(
        value: &Value,
        options: FromDynamicOptions,
    ) -> Result<Self, wezterm_dynamic::Error> {
        let s = String::from_dynamic(value, options)?;
        Ok(RgbColor::from_named_or_rgb_string(&s)
            .ok_or_else(|| format!("unknown color name: {}", s))?)
    }
}

/// An index into the fixed color palette.
pub type PaletteIndex = u8;

/// Specifies the color to be used when rendering a cell.
/// This differs from `ColorAttribute` in that this type can only
/// specify one of the possible color types at once, whereas the
/// `ColorAttribute` type can specify a TrueColor value and a fallback.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ColorSpec {
    Default,
    /// Use either a raw number, or use values from the `AnsiColor` enum
    PaletteIndex(PaletteIndex),
    TrueColor(SrgbaTuple),
}

impl Default for ColorSpec {
    fn default() -> Self {
        ColorSpec::Default
    }
}

impl From<AnsiColor> for ColorSpec {
    fn from(col: AnsiColor) -> Self {
        ColorSpec::PaletteIndex(col as u8)
    }
}

impl From<RgbColor> for ColorSpec {
    fn from(col: RgbColor) -> Self {
        ColorSpec::TrueColor(col.into())
    }
}

impl From<SrgbaTuple> for ColorSpec {
    fn from(col: SrgbaTuple) -> Self {
        ColorSpec::TrueColor(col)
    }
}

/// Specifies the color to be used when rendering a cell.  This is the
/// type used in the `CellAttributes` struct and can specify an optional
/// TrueColor value, allowing a fallback to a more traditional palette
/// index if TrueColor is not available.
#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, Eq, PartialEq, FromDynamic, ToDynamic, Hash)]
pub enum ColorAttribute {
    /// Use RgbColor when supported, falling back to the specified PaletteIndex.
    TrueColorWithPaletteFallback(SrgbaTuple, PaletteIndex),
    /// Use RgbColor when supported, falling back to the default color
    TrueColorWithDefaultFallback(SrgbaTuple),
    /// Use the specified PaletteIndex
    PaletteIndex(PaletteIndex),
    /// Use the default color
    Default,
}

impl Default for ColorAttribute {
    fn default() -> Self {
        ColorAttribute::Default
    }
}

impl From<AnsiColor> for ColorAttribute {
    fn from(col: AnsiColor) -> Self {
        ColorAttribute::PaletteIndex(col as u8)
    }
}

impl From<ColorSpec> for ColorAttribute {
    fn from(spec: ColorSpec) -> Self {
        match spec {
            ColorSpec::Default => ColorAttribute::Default,
            ColorSpec::PaletteIndex(idx) => ColorAttribute::PaletteIndex(idx),
            ColorSpec::TrueColor(color) => ColorAttribute::TrueColorWithDefaultFallback(color),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn from_hsl() {
        let foo = RgbColor::from_rgb_str("hsl:235 100  50").unwrap();
        assert_eq!(foo.to_rgb_string(), "#0015ff");
    }

    #[test]
    fn from_rgb() {
        assert!(RgbColor::from_rgb_str("").is_none());
        assert!(RgbColor::from_rgb_str("#xyxyxy").is_none());

        let black = RgbColor::from_rgb_str("#FFF").unwrap();
        assert_eq!(black.to_tuple_rgb8(), (0xf0, 0xf0, 0xf0));

        let black = RgbColor::from_rgb_str("#000000").unwrap();
        assert_eq!(black.to_tuple_rgb8(), (0, 0, 0));

        let grey = RgbColor::from_rgb_str("rgb:D6/D6/D6").unwrap();
        assert_eq!(grey.to_tuple_rgb8(), (0xd6, 0xd6, 0xd6));

        let grey = RgbColor::from_rgb_str("rgb:f0f0/f0f0/f0f0").unwrap();
        assert_eq!(grey.to_tuple_rgb8(), (0xf0, 0xf0, 0xf0));
    }

    #[cfg(feature = "use_serde")]
    #[test]
    fn roundtrip_rgbcolor() {
        let data = varbincode::serialize(&RgbColor::from_named("DarkGreen").unwrap()).unwrap();
        eprintln!("serialized as {:?}", data);
        let _decoded: RgbColor = varbincode::deserialize(data.as_slice()).unwrap();
    }
}

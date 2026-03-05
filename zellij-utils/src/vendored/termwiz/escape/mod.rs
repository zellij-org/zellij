// suppress inscrutable useless_attribute clippy that shows up when
// using derive(FromPrimitive)
#![allow(clippy::useless_attribute)]
#![allow(clippy::upper_case_acronyms)]
//! This module provides the ability to parse escape sequences and attach
//! semantic meaning to them.  It can also encode the semantic values as
//! escape sequences.  It provides encoding and decoding functionality
//! only; it does not provide terminal emulation facilities itself.
use crate::vendored::termwiz::tmux_cc::Event;
use num_derive::*;
use std::fmt::{Display, Error as FmtError, Formatter, Write as FmtWrite};
use wezterm_color_types::LinearRgba;

pub mod apc;
pub mod csi;
pub mod esc;
pub mod osc;
pub mod parser;

pub use self::apc::KittyImage;
pub use self::csi::CSI;
pub use self::esc::{Esc, EscCode};
pub use self::osc::OperatingSystemCommand;

use vtparse::CsiParam;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Send a single printable character to the display
    Print(char),
    /// Send a string of printable characters to the display.
    PrintString(String),
    /// A C0 or C1 control code
    Control(ControlCode),
    /// Device control.  This is uncommon wrt. terminal emulation.
    DeviceControl(DeviceControlMode),
    /// A command that typically doesn't change the contents of the
    /// terminal, but rather influences how it displays or otherwise
    /// interacts with the rest of the system
    OperatingSystemCommand(Box<OperatingSystemCommand>),
    CSI(CSI),
    Esc(Esc),
    Sixel(Box<Sixel>),
    /// A list of termcap, terminfo names for which the application
    /// wants information
    XtGetTcap(Vec<String>),
    KittyImage(Box<KittyImage>),
}

impl Action {
    /// Append this `Action` to a `Vec<Action>`.
    /// If this `Action` is `Print` and the last element is `Print` or
    /// `PrintString` then the elements are combined into `PrintString`
    /// to reduce heap utilization.
    pub fn append_to(self, dest: &mut Vec<Self>) {
        if let Action::Print(c) = &self {
            match dest.last_mut() {
                Some(Action::PrintString(s)) => {
                    s.push(*c);
                    return;
                },
                Some(Action::Print(prior)) => {
                    let mut s = prior.to_string();
                    dest.pop();
                    s.push(*c);
                    dest.push(Action::PrintString(s));
                    return;
                },
                _ => {},
            }
        }
        dest.push(self);
    }
}

#[cfg(all(test, target_pointer_width = "64"))]
#[test]
fn action_size() {
    assert_eq!(std::mem::size_of::<Action>(), 32);
    assert_eq!(std::mem::size_of::<DeviceControlMode>(), 16);
    assert_eq!(std::mem::size_of::<ControlCode>(), 1);
    assert_eq!(std::mem::size_of::<CSI>(), 32);
    assert_eq!(std::mem::size_of::<Esc>(), 4);
}

/// Encode self as an escape sequence.  The escape sequence may potentially
/// be clear text with no actual escape sequences.
impl Display for Action {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Action::Print(c) => write!(f, "{}", c),
            Action::PrintString(s) => write!(f, "{}", s),
            Action::Control(c) => f.write_char(*c as u8 as char),
            Action::DeviceControl(c) => c.fmt(f),
            Action::OperatingSystemCommand(osc) => osc.fmt(f),
            Action::CSI(csi) => csi.fmt(f),
            Action::Esc(esc) => esc.fmt(f),
            Action::Sixel(sixel) => sixel.fmt(f),
            Action::XtGetTcap(names) => {
                write!(f, "\x1bP+q")?;
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        write!(f, ";")?;
                    }
                    for &b in name.as_bytes() {
                        write!(f, "{:x}", b)?;
                    }
                }

                Ok(())
            },
            Action::KittyImage(img) => img.fmt(f),
        }
    }
}

/// A fully parsed DCS sequence.
/// The parser emits these for byte/intermediate sequences that are
/// known to be relatively short and self contained (eg: DECRQSS)
/// as opposed to larger ones like Sixel (which is parsed separately),
/// or long lived terminal modes such as the TMUX CC protocol.
#[derive(Clone, PartialEq, Eq)]
pub struct ShortDeviceControl {
    /// Integer parameter values
    pub params: Vec<i64>,
    /// Intermediate bytes to refine the control
    pub intermediates: Vec<u8>,
    /// The final byte
    pub byte: u8,
    /// The data prior to the string terminator
    pub data: Vec<u8>,
}

impl std::fmt::Debug for ShortDeviceControl {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(
            fmt,
            "ShortDeviceControl(params: {:?}, intermediates: [",
            &self.params
        )?;
        for b in &self.intermediates {
            write!(fmt, "{:?} 0x{:x}, ", *b as char, *b)?;
        }
        write!(
            fmt,
            "], byte: {:?} 0x{:x}, data=[",
            self.byte as char, self.byte
        )?;

        for b in &self.data {
            write!(fmt, "{:?} 0x{:x}, ", *b as char, *b)?;
        }

        write!(fmt, ")")
    }
}

impl Display for ShortDeviceControl {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        write!(f, "\x1bP")?;
        for (idx, p) in self.params.iter().enumerate() {
            if idx > 0 {
                write!(f, ";")?;
            }
            write!(f, "{}", p)?;
        }
        for b in &self.intermediates {
            f.write_char(*b as char)?;
        }
        f.write_char(self.byte as char)?;
        for b in &self.data {
            f.write_char(*b as char)?;
        }
        write!(f, "\x1b\\")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct EnterDeviceControlMode {
    /// The final byte in the DCS mode
    pub byte: u8,
    pub params: Vec<i64>,
    pub intermediates: Vec<u8>,
    /// if true, more than two intermediates arrived and the
    /// remaining data was ignored
    pub ignored_extra_intermediates: bool,
}

impl std::fmt::Debug for EnterDeviceControlMode {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(
            fmt,
            "EnterDeviceControlMode(params: {:?}, intermediates: [",
            &self.params
        )?;
        for b in &self.intermediates {
            write!(fmt, "{:?} 0x{:x}, ", *b as char, *b)?;
        }
        write!(
            fmt,
            "], byte: {:?} 0x{:x}, ignored_extra_intermediates={})",
            self.byte as char, self.byte, self.ignored_extra_intermediates
        )
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum DeviceControlMode {
    /// Identify device control mode from the encoded parameters.
    /// This mode is activated and must remain active until
    /// `Exit` is observed.  While the mode is
    /// active, data is made available to the device mode via
    /// the `Data` variant.
    Enter(Box<EnterDeviceControlMode>),
    /// Exit the current device control mode
    Exit,
    /// Data for the device mode to consume
    Data(u8),
    /// A self contained (Enter, Data*, Exit) sequence
    ShortDeviceControl(Box<ShortDeviceControl>),
    /// Tmux parsed events
    TmuxEvents(Box<Vec<Event>>),
}

impl Display for DeviceControlMode {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Self::Enter(mode) => {
                write!(f, "\x1bP")?;
                for (idx, p) in mode.params.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ";")?;
                    }
                    write!(f, "{}", p)?;
                }
                for b in &mode.intermediates {
                    f.write_char(*b as char)?;
                }
                f.write_char(mode.byte as char)
            },
            // We don't need to emit a sequence for the Exit, as we're
            // followed by eg: StringTerminator
            Self::Exit => Ok(()),
            Self::Data(c) => f.write_char(*c as char),
            Self::ShortDeviceControl(s) => s.fmt(f),
            Self::TmuxEvents(_) => write!(f, "tmux event"),
        }
    }
}

impl std::fmt::Debug for DeviceControlMode {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::Enter(mode) => write!(fmt, "Enter({:?})", mode),
            Self::Exit => write!(fmt, "Exit"),
            Self::Data(b) => write!(fmt, "Data({:?} 0x{:x})", *b as char, *b),
            Self::ShortDeviceControl(s) => write!(fmt, "ShortDeviceControl({:?})", s),
            Self::TmuxEvents(_) => write!(fmt, "tmux event"),
        }
    }
}

/// See <https://vt100.net/docs/vt3xx-gp/chapter14.html>
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sixel {
    /// Specifies the numerator for the pixel aspect ratio
    pub pan: i64,

    /// Specifies the denominator for the pixel aspect ratio
    pub pad: i64,

    /// How wide the image is, in pixels
    pub pixel_width: Option<u32>,

    /// How tall the image is, in pixels,
    pub pixel_height: Option<u32>,

    /// When true, pixels with 0 value are left at their
    /// present color, otherwise, they are set to the background
    /// color.
    pub background_is_transparent: bool,

    /// The horizontal spacing between pixels
    pub horizontal_grid_size: Option<i64>,

    /// The sixel data
    pub data: Vec<SixelData>,
}

impl Sixel {
    /// Returns the width, height of the image
    pub fn dimensions(&self) -> (u32, u32) {
        if let (Some(w), Some(h)) = (self.pixel_width, self.pixel_height) {
            return (w, h);
        }

        // Compute it by evaluating the sixel data
        let mut max_x = 0;
        let mut max_y = 0;
        let mut x: u32 = 0;
        let mut rows: u32 = 1;

        for d in &self.data {
            match d {
                SixelData::Data(_) => {
                    max_y = max_y.max(rows * 6);
                    x = x.saturating_add(1);
                    max_x = max_x.max(x);
                },
                SixelData::Repeat { repeat_count, .. } => {
                    max_y = max_y.max(rows * 6);
                    x = x.saturating_add(*repeat_count);
                    max_x = max_x.max(x);
                },
                SixelData::SelectColorMapEntry(_)
                | SixelData::DefineColorMapRGB { .. }
                | SixelData::DefineColorMapHSL { .. } => {},
                SixelData::NewLine => {
                    max_x = max_x.max(x);
                    x = 0;
                    rows = rows.saturating_add(1);
                },
                SixelData::CarriageReturn => {
                    max_x = max_x.max(x);
                    x = 0;
                },
            }
        }

        (max_x, max_y)
    }
}

impl Display for Sixel {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        if self.pixel_width.is_some() {
            write!(
                f,
                "\x1bP;{}{}q\"{};{};{};{}",
                if self.background_is_transparent { 1 } else { 0 },
                match self.horizontal_grid_size {
                    Some(h) => format!(";{}", h),
                    None => "".to_string(),
                },
                self.pan,
                self.pad,
                self.pixel_width.unwrap_or(0),
                self.pixel_height.unwrap_or(0)
            )?;
        } else {
            write!(
                f,
                "\x1bP{};{}{}q",
                match (self.pan, self.pad) {
                    (2, 1) => 0,
                    (5, 1) => 2,
                    (3, 1) => 3,
                    (1, 1) => 7,
                    _ => {
                        eprintln!("bad pad/pan combo: {:?}", self);
                        return Err(std::fmt::Error);
                    },
                },
                if self.background_is_transparent { 1 } else { 0 },
                match self.horizontal_grid_size {
                    Some(h) => format!(";{}", h),
                    None => "".to_string(),
                },
            )?;
        }
        for d in &self.data {
            d.fmt(f)?;
        }
        // The sixel data itself doesn't contain the ST
        // write!(f, "\x1b\\")?;
        Ok(())
    }
}

/// A decoded 6-bit sixel value.
/// Each sixel represents a six-pixel tall bitmap where
/// the least significant bit is the topmost bit.
pub type SixelValue = u8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SixelData {
    /// A single sixel value
    Data(SixelValue),

    /// Run-length encoding; allows repeating a sixel value
    /// the specified number of times
    Repeat { repeat_count: u32, data: SixelValue },

    /// Set the specified color map entry to the specified
    /// linear RGB color value
    DefineColorMapRGB {
        color_number: u16,
        rgb: crate::vendored::termwiz::color::RgbColor,
    },

    DefineColorMapHSL {
        color_number: u16,
        /// 0 to 360 degrees
        hue_angle: u16,
        /// 0 to 100
        lightness: u8,
        /// 0 to 100
        saturation: u8,
    },

    /// Select the numbered color from the color map entry
    SelectColorMapEntry(u16),

    /// Move the x position to the left page border of the
    /// current sixel line.
    CarriageReturn,

    /// Move the x position to the left page border and
    /// the y position down to the next sixel line.
    NewLine,
}

impl Display for SixelData {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Self::Data(value) => write!(f, "{}", (value + 0x3f) as char),
            Self::Repeat { repeat_count, data } => {
                write!(f, "!{}{}", repeat_count, (data + 0x3f) as char)
            },
            Self::DefineColorMapRGB { color_number, rgb } => {
                let LinearRgba(r, g, b, _) = rgb.to_linear_tuple_rgba();
                write!(
                    f,
                    "#{};2;{};{};{}",
                    color_number,
                    (r * 100.) as u8,
                    (g * 100.) as u8,
                    (b * 100.0) as u8
                )
            },
            Self::DefineColorMapHSL {
                color_number,
                hue_angle,
                lightness,
                saturation,
            } => write!(
                f,
                "#{};1;{};{};{}",
                color_number, hue_angle, lightness, saturation
            ),
            Self::SelectColorMapEntry(n) => write!(f, "#{}", n),
            Self::CarriageReturn => write!(f, "$"),
            Self::NewLine => write!(f, "-"),
        }
    }
}

/// C0 or C1 control codes
#[derive(Debug, Copy, Clone, PartialEq, Eq, FromPrimitive)]
#[repr(u8)]
pub enum ControlCode {
    Null = 0,
    StartOfHeading = 1,
    StartOfText = 2,
    EndOfText = 3,
    EndOfTransmission = 4,
    Enquiry = 5,
    Acknowledge = 6,
    Bell = 7,
    Backspace = 8,
    HorizontalTab = b'\t',
    LineFeed = b'\n',
    VerticalTab = 0xb,
    FormFeed = 0xc,
    CarriageReturn = b'\r',
    ShiftOut = 0xe,
    ShiftIn = 0xf,
    DataLinkEscape = 0x10,
    DeviceControlOne = 0x11,
    DeviceControlTwo = 0x12,
    DeviceControlThree = 0x13,
    DeviceControlFour = 0x14,
    NegativeAcknowledge = 0x15,
    SynchronousIdle = 0x16,
    EndOfTransmissionBlock = 0x17,
    Cancel = 0x18,
    EndOfMedium = 0x19,
    Substitute = 0x1a,
    Escape = 0x1b,
    FileSeparator = 0x1c,
    GroupSeparator = 0x1d,
    RecordSeparator = 0x1e,
    UnitSeparator = 0x1f,

    // C1 8-bit values
    BPH = 0x82,
    NBH = 0x83,
    IND = 0x84,
    NEL = 0x85,
    SSA = 0x86,
    ESA = 0x87,
    HTS = 0x88,
    HTJ = 0x89,
    VTS = 0x8a,
    PLD = 0x8b,
    PLU = 0x8c,
    RI = 0x8d,
    SS2 = 0x8e,
    SS3 = 0x8f,
    DCS = 0x90,
    PU1 = 0x91,
    PU2 = 0x92,
    STS = 0x93,
    CCH = 0x94,
    MW = 0x95,
    SPA = 0x96,
    EPA = 0x97,
    SOS = 0x98,
    SCI = 0x9a,
    CSI = 0x9b,
    ST = 0x9c,
    OSC = 0x9d,
    PM = 0x9e,
    APC = 0x9f,
}

/// A helper type to avoid accidentally tripping over problems with
/// 1-based values in escape sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneBased {
    value: u32,
}

impl OneBased {
    pub fn new(value: u32) -> Self {
        debug_assert!(
            value != 0,
            "programmer error: deliberately assigning zero to a OneBased"
        );
        Self { value }
    }

    pub fn from_zero_based(value: u32) -> Self {
        Self { value: value + 1 }
    }

    /// Map a value from an escape sequence parameter.
    /// 0 is equivalent to 1
    pub fn from_esc_param(v: &CsiParam) -> Result<Self, ()> {
        match v {
            CsiParam::Integer(v) if *v == 0 => Ok(Self {
                value: num_traits::one(),
            }),
            CsiParam::Integer(v) if *v > 0 && *v <= i64::from(u32::max_value()) => {
                Ok(Self { value: *v as u32 })
            },
            _ => Err(()),
        }
    }

    /// Map a value from an escape sequence parameter.
    /// 0 is equivalent to max_value.
    pub fn from_esc_param_with_big_default(v: &CsiParam) -> Result<Self, ()> {
        match v {
            CsiParam::Integer(v) if *v == 0 => Ok(Self {
                value: u32::max_value(),
            }),
            CsiParam::Integer(v) if *v > 0 && *v <= i64::from(u32::max_value()) => {
                Ok(Self { value: *v as u32 })
            },
            _ => Err(()),
        }
    }

    /// Map a value from an optional escape sequence parameter
    pub fn from_optional_esc_param(o: Option<&CsiParam>) -> Result<Self, ()> {
        Self::from_esc_param(o.unwrap_or(&CsiParam::Integer(1)))
    }

    /// Return the underlying value as a 0-based value
    pub fn as_zero_based(self) -> u32 {
        self.value.saturating_sub(1)
    }

    pub fn as_one_based(self) -> u32 {
        self.value
    }
}

impl Display for OneBased {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        self.value.fmt(f)
    }
}

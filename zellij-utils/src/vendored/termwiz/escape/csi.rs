use super::OneBased;
use crate::vendored::termwiz::cell::{Blink, Intensity, Underline, VerticalAlign};
use crate::vendored::termwiz::color::{AnsiColor, ColorSpec, RgbColor, SrgbaTuple};
use crate::vendored::termwiz::input::{Modifiers, MouseButtons};
use num_derive::*;
use num_traits::{FromPrimitive, ToPrimitive};
use std::convert::TryInto;
use std::fmt::{Display, Error as FmtError, Formatter};

pub use vtparse::CsiParam;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CSI {
    /// SGR: Set Graphics Rendition.
    /// These values affect how the character is rendered.
    Sgr(Sgr),

    /// CSI codes that relate to the cursor
    Cursor(Cursor),

    Edit(Edit),

    Mode(Mode),

    Device(Box<Device>),

    Mouse(MouseReport),

    Window(Box<Window>),

    Keyboard(Keyboard),

    /// ECMA-48 SCP
    SelectCharacterPath(CharacterPath, i64),

    /// Unknown or unspecified; should be rare and is rather
    /// large, so it is boxed and kept outside of the enum
    /// body to help reduce space usage in the common cases.
    Unspecified(Box<Unspecified>),
}

#[cfg(all(test, target_pointer_width = "64"))]
#[test]
fn csi_size() {
    assert_eq!(std::mem::size_of::<Sgr>(), 24);
    assert_eq!(std::mem::size_of::<Cursor>(), 12);
    assert_eq!(std::mem::size_of::<Edit>(), 8);
    assert_eq!(std::mem::size_of::<Mode>(), 24);
    assert_eq!(std::mem::size_of::<MouseReport>(), 8);
    assert_eq!(std::mem::size_of::<Window>(), 40);
    assert_eq!(std::mem::size_of::<Keyboard>(), 8);
    assert_eq!(std::mem::size_of::<CSI>(), 32);
}

pub use wezterm_input_types::KittyKeyboardFlags;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u16)]
pub enum KittyKeyboardMode {
    AssignAll = 1,
    SetSpecified = 2,
    ClearSpecified = 3,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Keyboard {
    SetKittyState {
        flags: KittyKeyboardFlags,
        mode: KittyKeyboardMode,
    },
    PushKittyState {
        flags: KittyKeyboardFlags,
        mode: KittyKeyboardMode,
    },
    PopKittyState(u32),
    QueryKittySupport,
    ReportKittyState(KittyKeyboardFlags),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CharacterPath {
    /// 0
    ImplementationDefault,
    /// 1
    LeftToRightOrTopToBottom,
    /// 2
    RightToLeftOrBottomToTop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unspecified {
    pub params: Vec<CsiParam>,
    /// if true, more than two intermediates arrived and the
    /// remaining data was ignored
    pub parameters_truncated: bool,
    /// The final character in the CSI sequence; this typically
    /// defines how to interpret the other parameters.
    pub control: char,
}

impl Display for Unspecified {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        for p in &self.params {
            write!(f, "{}", p)?;
        }
        write!(f, "{}", self.control)
    }
}

impl Display for CSI {
    // TODO: data size optimization opportunity: if we could somehow know that we
    // had a run of CSI instances being encoded in sequence, we could
    // potentially collapse them together.  This is a few bytes difference in
    // practice so it may not be worthwhile with modern networks.
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        write!(f, "\x1b[")?;
        match self {
            CSI::Sgr(sgr) => sgr.fmt(f)?,
            CSI::Cursor(c) => c.fmt(f)?,
            CSI::Edit(e) => e.fmt(f)?,
            CSI::Mode(mode) => mode.fmt(f)?,
            CSI::Unspecified(unspec) => unspec.fmt(f)?,
            CSI::Mouse(mouse) => mouse.fmt(f)?,
            CSI::Device(dev) => dev.fmt(f)?,
            CSI::Window(window) => window.fmt(f)?,
            CSI::Keyboard(Keyboard::SetKittyState { flags, mode }) => {
                write!(f, "={};{}u", flags.bits(), *mode as u16)?
            },
            CSI::Keyboard(Keyboard::PushKittyState { flags, mode }) => {
                write!(f, ">{};{}u", flags.bits(), *mode as u16)?
            },
            CSI::Keyboard(Keyboard::PopKittyState(n)) => write!(f, "<{}u", *n)?,
            CSI::Keyboard(Keyboard::QueryKittySupport) => write!(f, "?u")?,
            CSI::Keyboard(Keyboard::ReportKittyState(flags)) => write!(f, "?{}u", flags.bits())?,
            CSI::SelectCharacterPath(path, n) => {
                let a = match path {
                    CharacterPath::ImplementationDefault => 0,
                    CharacterPath::LeftToRightOrTopToBottom => 1,
                    CharacterPath::RightToLeftOrBottomToTop => 2,
                };
                match (a, n) {
                    (0, 0) => write!(f, " k")?,
                    (a, 0) => write!(f, "{} k", a)?,
                    (a, n) => write!(f, "{};{} k", a, n)?,
                }
            },
        };
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum CursorStyle {
    Default = 0,
    BlinkingBlock = 1,
    SteadyBlock = 2,
    BlinkingUnderline = 3,
    SteadyUnderline = 4,
    BlinkingBar = 5,
    SteadyBar = 6,
}

impl Default for CursorStyle {
    fn default() -> CursorStyle {
        CursorStyle::Default
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum DeviceAttributeCodes {
    Columns132 = 1,
    Printer = 2,
    RegisGraphics = 3,
    SixelGraphics = 4,
    SelectiveErase = 6,
    UserDefinedKeys = 8,
    NationalReplacementCharsets = 9,
    TechnicalCharacters = 15,
    UserWindows = 18,
    HorizontalScrolling = 21,
    AnsiColor = 22,
    AnsiTextLocator = 29,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceAttribute {
    Code(DeviceAttributeCodes),
    Unspecified(CsiParam),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceAttributeFlags {
    pub attributes: Vec<DeviceAttribute>,
}

impl DeviceAttributeFlags {
    fn emit(&self, f: &mut Formatter, leader: &str) -> Result<(), FmtError> {
        write!(f, "{}", leader)?;
        for item in &self.attributes {
            match item {
                DeviceAttribute::Code(c) => write!(f, ";{}", c.to_u16().ok_or_else(|| FmtError)?)?,
                DeviceAttribute::Unspecified(param) => write!(f, ";{}", param)?,
            }
        }
        write!(f, "c")?;
        Ok(())
    }

    pub fn new(attributes: Vec<DeviceAttribute>) -> Self {
        Self { attributes }
    }

    fn from_params(params: &[CsiParam]) -> Self {
        let mut attributes = Vec::new();
        for i in params {
            match i {
                CsiParam::Integer(p) => match FromPrimitive::from_i64(*p) {
                    Some(c) => attributes.push(DeviceAttribute::Code(c)),
                    None => attributes.push(DeviceAttribute::Unspecified(i.clone())),
                },
                CsiParam::P(b';') => {},
                _ => attributes.push(DeviceAttribute::Unspecified(i.clone())),
            }
        }
        Self { attributes }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceAttributes {
    Vt100WithAdvancedVideoOption,
    Vt101WithNoOptions,
    Vt102,
    Vt220(DeviceAttributeFlags),
    Vt320(DeviceAttributeFlags),
    Vt420(DeviceAttributeFlags),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XtSmGraphicsItem {
    NumberOfColorRegisters,
    SixelGraphicsGeometry,
    RegisGraphicsGeometry,
    Unspecified(i64),
}

impl Display for XtSmGraphicsItem {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Self::NumberOfColorRegisters => write!(f, "1"),
            Self::SixelGraphicsGeometry => write!(f, "2"),
            Self::RegisGraphicsGeometry => write!(f, "3"),
            Self::Unspecified(n) => write!(f, "{}", n),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XtSmGraphicsAction {
    ReadAttribute,
    ResetToDefault,
    SetToValue,
    ReadMaximumAllowedValue,
}

impl XtSmGraphicsAction {
    pub fn to_i64(&self) -> i64 {
        match self {
            Self::ReadAttribute => 1,
            Self::ResetToDefault => 2,
            Self::SetToValue => 3,
            Self::ReadMaximumAllowedValue => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XtSmGraphicsStatus {
    Success,
    InvalidItem,
    InvalidAction,
    Failure,
}

impl XtSmGraphicsStatus {
    pub fn to_i64(&self) -> i64 {
        match self {
            Self::Success => 0,
            Self::InvalidItem => 1,
            Self::InvalidAction => 2,
            Self::Failure => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XtSmGraphics {
    pub item: XtSmGraphicsItem,
    pub action_or_status: i64,
    pub value: Vec<i64>,
}

impl XtSmGraphics {
    pub fn action(&self) -> Option<XtSmGraphicsAction> {
        match self.action_or_status {
            1 => Some(XtSmGraphicsAction::ReadAttribute),
            2 => Some(XtSmGraphicsAction::ResetToDefault),
            3 => Some(XtSmGraphicsAction::SetToValue),
            4 => Some(XtSmGraphicsAction::ReadMaximumAllowedValue),
            _ => None,
        }
    }

    pub fn status(&self) -> Option<XtSmGraphicsStatus> {
        match self.action_or_status {
            0 => Some(XtSmGraphicsStatus::Success),
            1 => Some(XtSmGraphicsStatus::InvalidItem),
            2 => Some(XtSmGraphicsStatus::InvalidAction),
            3 => Some(XtSmGraphicsStatus::Failure),
            _ => None,
        }
    }

    pub fn parse(params: &[CsiParam]) -> Result<CSI, ()> {
        let params = Cracked::parse(&params[1..])?;
        Ok(CSI::Device(Box::new(Device::XtSmGraphics(XtSmGraphics {
            item: match params.get(0).ok_or(())? {
                CsiParam::Integer(1) => XtSmGraphicsItem::NumberOfColorRegisters,
                CsiParam::Integer(2) => XtSmGraphicsItem::SixelGraphicsGeometry,
                CsiParam::Integer(3) => XtSmGraphicsItem::RegisGraphicsGeometry,
                CsiParam::Integer(n) => XtSmGraphicsItem::Unspecified(*n),
                _ => return Err(()),
            },
            action_or_status: match params.get(1).ok_or(())? {
                CsiParam::Integer(n) => *n,
                _ => return Err(()),
            },
            value: params.params[2..]
                .iter()
                .filter_map(|p| match p {
                    Some(CsiParam::Integer(n)) => Some(*n),
                    _ => None,
                })
                .collect(),
        }))))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Device {
    DeviceAttributes(DeviceAttributes),
    /// DECSTR - https://vt100.net/docs/vt510-rm/DECSTR.html
    SoftReset,
    RequestPrimaryDeviceAttributes,
    RequestSecondaryDeviceAttributes,
    RequestTertiaryDeviceAttributes,
    StatusReport,
    /// https://github.com/mintty/mintty/issues/881
    /// https://gitlab.gnome.org/GNOME/vte/-/issues/235
    RequestTerminalNameAndVersion,
    RequestTerminalParameters(i64),
    XtSmGraphics(XtSmGraphics),
}

impl Display for Device {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Device::DeviceAttributes(DeviceAttributes::Vt100WithAdvancedVideoOption) => {
                write!(f, "?1;2c")?
            },
            Device::DeviceAttributes(DeviceAttributes::Vt101WithNoOptions) => write!(f, "?1;0c")?,
            Device::DeviceAttributes(DeviceAttributes::Vt102) => write!(f, "?6c")?,
            Device::DeviceAttributes(DeviceAttributes::Vt220(attr)) => attr.emit(f, "?62")?,
            Device::DeviceAttributes(DeviceAttributes::Vt320(attr)) => attr.emit(f, "?63")?,
            Device::DeviceAttributes(DeviceAttributes::Vt420(attr)) => attr.emit(f, "?64")?,
            Device::SoftReset => write!(f, "!p")?,
            Device::RequestPrimaryDeviceAttributes => write!(f, "c")?,
            Device::RequestSecondaryDeviceAttributes => write!(f, ">c")?,
            Device::RequestTertiaryDeviceAttributes => write!(f, "=c")?,
            Device::RequestTerminalNameAndVersion => write!(f, ">q")?,
            Device::RequestTerminalParameters(n) => write!(f, "{};1;1;128;128;1;0x", n + 2)?,
            Device::StatusReport => write!(f, "5n")?,
            Device::XtSmGraphics(g) => {
                write!(f, "?{};{}", g.item, g.action_or_status)?;
                for v in &g.value {
                    write!(f, ";{}", v)?;
                }
                write!(f, "S")?;
            },
        };
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseButton {
    Button1Press,
    Button2Press,
    Button3Press,
    Button4Press,
    Button5Press,
    Button6Press,
    Button7Press,
    Button1Release,
    Button2Release,
    Button3Release,
    Button4Release,
    Button5Release,
    Button6Release,
    Button7Release,
    Button1Drag,
    Button2Drag,
    Button3Drag,
    None,
}

impl From<MouseButton> for MouseButtons {
    fn from(button: MouseButton) -> MouseButtons {
        match button {
            MouseButton::Button1Press | MouseButton::Button1Drag => MouseButtons::LEFT,
            MouseButton::Button2Press | MouseButton::Button2Drag => MouseButtons::MIDDLE,
            MouseButton::Button3Press | MouseButton::Button3Drag => MouseButtons::RIGHT,
            MouseButton::Button4Press => MouseButtons::VERT_WHEEL | MouseButtons::WHEEL_POSITIVE,
            MouseButton::Button5Press => MouseButtons::VERT_WHEEL,
            MouseButton::Button6Press => MouseButtons::HORZ_WHEEL | MouseButtons::WHEEL_POSITIVE,
            MouseButton::Button7Press => MouseButtons::HORZ_WHEEL,
            _ => MouseButtons::NONE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Window {
    DeIconify,
    Iconify,
    MoveWindow {
        x: i64,
        y: i64,
    },
    ResizeWindowPixels {
        width: Option<i64>,
        height: Option<i64>,
    },
    RaiseWindow,
    LowerWindow,
    RefreshWindow,
    ResizeWindowCells {
        width: Option<i64>,
        height: Option<i64>,
    },
    RestoreMaximizedWindow,
    MaximizeWindow,
    MaximizeWindowVertically,
    MaximizeWindowHorizontally,
    UndoFullScreenMode,
    ChangeToFullScreenMode,
    ToggleFullScreen,
    ReportWindowState,
    ReportWindowPosition,
    ReportTextAreaPosition,
    ReportTextAreaSizePixels,
    ReportWindowSizePixels,
    ReportScreenSizePixels,
    ReportCellSizePixels,
    ReportCellSizePixelsResponse {
        width: Option<i64>,
        height: Option<i64>,
    },
    ReportTextAreaSizeCells,
    ReportScreenSizeCells,
    ReportIconLabel,
    ReportWindowTitle,
    PushIconAndWindowTitle,
    PushIconTitle,
    PushWindowTitle,
    PopIconAndWindowTitle,
    PopIconTitle,
    PopWindowTitle,
    /// DECRQCRA; used by esctest
    ChecksumRectangularArea {
        request_id: i64,
        page_number: i64,
        top: OneBased,
        left: OneBased,
        bottom: OneBased,
        right: OneBased,
    },
}

fn numstr_or_empty(x: &Option<i64>) -> String {
    match x {
        Some(x) => format!("{}", x),
        None => "".to_owned(),
    }
}

impl Display for Window {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Window::DeIconify => write!(f, "1t"),
            Window::Iconify => write!(f, "2t"),
            Window::MoveWindow { x, y } => write!(f, "3;{};{}t", x, y),
            Window::ResizeWindowPixels { width, height } => write!(
                f,
                "4;{};{}t",
                numstr_or_empty(height),
                numstr_or_empty(width),
            ),
            Window::RaiseWindow => write!(f, "5t"),
            Window::LowerWindow => write!(f, "6t"),
            Window::RefreshWindow => write!(f, "7t"),
            Window::ResizeWindowCells { width, height } => write!(
                f,
                "8;{};{}t",
                numstr_or_empty(height),
                numstr_or_empty(width),
            ),
            Window::RestoreMaximizedWindow => write!(f, "9;0t"),
            Window::MaximizeWindow => write!(f, "9;1t"),
            Window::MaximizeWindowVertically => write!(f, "9;2t"),
            Window::MaximizeWindowHorizontally => write!(f, "9;3t"),
            Window::UndoFullScreenMode => write!(f, "10;0t"),
            Window::ChangeToFullScreenMode => write!(f, "10;1t"),
            Window::ToggleFullScreen => write!(f, "10;2t"),
            Window::ReportWindowState => write!(f, "11t"),
            Window::ReportWindowPosition => write!(f, "13t"),
            Window::ReportTextAreaPosition => write!(f, "13;2t"),
            Window::ReportTextAreaSizePixels => write!(f, "14t"),
            Window::ReportWindowSizePixels => write!(f, "14;2t"),
            Window::ReportScreenSizePixels => write!(f, "15t"),
            Window::ReportCellSizePixels => write!(f, "16t"),
            Window::ReportCellSizePixelsResponse { width, height } => write!(
                f,
                "6;{};{}t",
                numstr_or_empty(height),
                numstr_or_empty(width),
            ),
            Window::ReportTextAreaSizeCells => write!(f, "18t"),
            Window::ReportScreenSizeCells => write!(f, "19t"),
            Window::ReportIconLabel => write!(f, "20t"),
            Window::ReportWindowTitle => write!(f, "21t"),
            Window::PushIconAndWindowTitle => write!(f, "22;0t"),
            Window::PushIconTitle => write!(f, "22;1t"),
            Window::PushWindowTitle => write!(f, "22;2t"),
            Window::PopIconAndWindowTitle => write!(f, "23;0t"),
            Window::PopIconTitle => write!(f, "23;1t"),
            Window::PopWindowTitle => write!(f, "23;2t"),
            Window::ChecksumRectangularArea {
                request_id,
                page_number,
                top,
                left,
                bottom,
                right,
            } => write!(
                f,
                "{};{};{};{};{};{}*y",
                request_id, page_number, top, left, bottom, right,
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseReport {
    SGR1006 {
        x: u16,
        y: u16,
        button: MouseButton,
        modifiers: Modifiers,
    },
    SGR1016 {
        x_pixels: u16,
        y_pixels: u16,
        button: MouseButton,
        modifiers: Modifiers,
    },
}

impl Display for MouseReport {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            MouseReport::SGR1006 {
                x,
                y,
                button,
                modifiers,
            } => {
                let mut b = 0;
                if (*modifiers & Modifiers::SHIFT) != Modifiers::NONE {
                    b |= 4;
                }
                if (*modifiers & Modifiers::ALT) != Modifiers::NONE {
                    b |= 8;
                }
                if (*modifiers & Modifiers::CTRL) != Modifiers::NONE {
                    b |= 16;
                }
                b |= match button {
                    MouseButton::Button1Press | MouseButton::Button1Release => 0,
                    MouseButton::Button2Press | MouseButton::Button2Release => 1,
                    MouseButton::Button3Press | MouseButton::Button3Release => 2,
                    MouseButton::Button4Press | MouseButton::Button4Release => 64,
                    MouseButton::Button5Press | MouseButton::Button5Release => 65,
                    MouseButton::Button6Press | MouseButton::Button6Release => 66,
                    MouseButton::Button7Press | MouseButton::Button7Release => 67,
                    MouseButton::Button1Drag => 32,
                    MouseButton::Button2Drag => 33,
                    MouseButton::Button3Drag => 34,
                    MouseButton::None => 35,
                };
                let trailer = match button {
                    MouseButton::Button1Press
                    | MouseButton::Button2Press
                    | MouseButton::Button3Press
                    | MouseButton::Button4Press
                    | MouseButton::Button5Press
                    | MouseButton::Button1Drag
                    | MouseButton::Button2Drag
                    | MouseButton::Button3Drag
                    | MouseButton::None => 'M',
                    _ => 'm',
                };
                write!(f, "<{};{};{}{}", b, x, y, trailer)
            },
            MouseReport::SGR1016 {
                x_pixels,
                y_pixels,
                button,
                modifiers,
            } => {
                let mut b = 0;
                if (*modifiers & Modifiers::SHIFT) != Modifiers::NONE {
                    b |= 4;
                }
                if (*modifiers & Modifiers::ALT) != Modifiers::NONE {
                    b |= 8;
                }
                if (*modifiers & Modifiers::CTRL) != Modifiers::NONE {
                    b |= 16;
                }
                b |= match button {
                    MouseButton::Button1Press | MouseButton::Button1Release => 0,
                    MouseButton::Button2Press | MouseButton::Button2Release => 1,
                    MouseButton::Button3Press | MouseButton::Button3Release => 2,
                    MouseButton::Button4Press | MouseButton::Button4Release => 64,
                    MouseButton::Button5Press | MouseButton::Button5Release => 65,
                    MouseButton::Button6Press | MouseButton::Button6Release => 66,
                    MouseButton::Button7Press | MouseButton::Button7Release => 67,
                    MouseButton::Button1Drag => 32,
                    MouseButton::Button2Drag => 33,
                    MouseButton::Button3Drag => 34,
                    MouseButton::None => 35,
                };
                let trailer = match button {
                    MouseButton::Button1Press
                    | MouseButton::Button2Press
                    | MouseButton::Button3Press
                    | MouseButton::Button4Press
                    | MouseButton::Button5Press
                    | MouseButton::Button1Drag
                    | MouseButton::Button2Drag
                    | MouseButton::Button3Drag
                    | MouseButton::None => 'M',
                    _ => 'm',
                };
                write!(f, "<{};{};{}{}", b, x_pixels, y_pixels, trailer)
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XtermKeyModifierResource {
    Keyboard,
    CursorKeys,
    FunctionKeys,
    OtherKeys,
}

impl XtermKeyModifierResource {
    pub fn parse(value: i64) -> Option<Self> {
        Some(match value {
            0 => XtermKeyModifierResource::Keyboard,
            1 => XtermKeyModifierResource::CursorKeys,
            2 => XtermKeyModifierResource::FunctionKeys,
            4 => XtermKeyModifierResource::OtherKeys,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    SetDecPrivateMode(DecPrivateMode),
    ResetDecPrivateMode(DecPrivateMode),
    SaveDecPrivateMode(DecPrivateMode),
    RestoreDecPrivateMode(DecPrivateMode),
    QueryDecPrivateMode(DecPrivateMode),
    SetMode(TerminalMode),
    ResetMode(TerminalMode),
    QueryMode(TerminalMode),
    XtermKeyMode {
        resource: XtermKeyModifierResource,
        value: Option<i64>,
    },
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        macro_rules! emit {
            ($flag:expr, $mode:expr) => {{
                let value = match $mode {
                    DecPrivateMode::Code(mode) => mode.to_u16().ok_or_else(|| FmtError)?,
                    DecPrivateMode::Unspecified(mode) => *mode,
                };
                write!(f, "?{}{}", value, $flag)
            }};
        }
        macro_rules! emit_mode {
            ($flag:expr, $mode:expr) => {{
                let value = match $mode {
                    TerminalMode::Code(mode) => mode.to_u16().ok_or_else(|| FmtError)?,
                    TerminalMode::Unspecified(mode) => *mode,
                };
                write!(f, "{}{}", value, $flag)
            }};
        }
        match self {
            Mode::SetDecPrivateMode(mode) => emit!("h", mode),
            Mode::ResetDecPrivateMode(mode) => emit!("l", mode),
            Mode::SaveDecPrivateMode(mode) => emit!("s", mode),
            Mode::RestoreDecPrivateMode(mode) => emit!("r", mode),
            Mode::QueryDecPrivateMode(DecPrivateMode::Code(mode)) => {
                write!(f, "?{}$p", mode.to_u16().ok_or_else(|| FmtError)?)
            },
            Mode::QueryDecPrivateMode(DecPrivateMode::Unspecified(mode)) => {
                write!(f, "?{}$p", mode)
            },
            Mode::SetMode(mode) => emit_mode!("h", mode),
            Mode::ResetMode(mode) => emit_mode!("l", mode),
            Mode::QueryMode(TerminalMode::Code(mode)) => {
                write!(f, "?{}$p", mode.to_u16().ok_or_else(|| FmtError)?)
            },
            Mode::QueryMode(TerminalMode::Unspecified(mode)) => write!(f, "?{}$p", mode),
            Mode::XtermKeyMode { resource, value } => {
                write!(
                    f,
                    ">{}",
                    match resource {
                        XtermKeyModifierResource::Keyboard => 0,
                        XtermKeyModifierResource::CursorKeys => 1,
                        XtermKeyModifierResource::FunctionKeys => 2,
                        XtermKeyModifierResource::OtherKeys => 4,
                    }
                )?;
                if let Some(value) = value {
                    write!(f, ";{}", value)?;
                } else {
                    write!(f, ";")?;
                }
                write!(f, "m")
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecPrivateMode {
    Code(DecPrivateModeCode),
    Unspecified(u16),
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum DecPrivateModeCode {
    /// https://vt100.net/docs/vt510-rm/DECCKM.html
    /// This mode is only effective when the terminal is in keypad application mode (see DECKPAM)
    /// and the ANSI/VT52 mode (DECANM) is set (see DECANM). Under these conditions, if the cursor
    /// key mode is reset, the four cursor function keys will send ANSI cursor control commands. If
    /// cursor key mode is set, the four cursor function keys will send application functions.
    ApplicationCursorKeys = 1,

    /// https://vt100.net/docs/vt510-rm/DECANM.html
    /// Behave like a vt52
    DecAnsiMode = 2,

    /// https://vt100.net/docs/vt510-rm/DECCOLM.html
    Select132Columns = 3,
    /// https://vt100.net/docs/vt510-rm/DECSCLM.html
    SmoothScroll = 4,
    /// https://vt100.net/docs/vt510-rm/DECSCNM.html
    ReverseVideo = 5,
    /// https://vt100.net/docs/vt510-rm/DECOM.html
    /// When OriginMode is enabled, cursor is constrained to the
    /// scroll region and its position is relative to the scroll
    /// region.
    OriginMode = 6,
    /// https://vt100.net/docs/vt510-rm/DECAWM.html
    /// When enabled, wrap to next line, Otherwise replace the last
    /// character
    AutoWrap = 7,
    /// https://vt100.net/docs/vt510-rm/DECARM.html
    AutoRepeat = 8,
    StartBlinkingCursor = 12,
    ShowCursor = 25,

    ReverseWraparound = 45,

    /// https://vt100.net/docs/vt510-rm/DECLRMM.html
    LeftRightMarginMode = 69,

    /// DECSDM - https://vt100.net/dec/ek-vt38t-ug-001.pdf#page=132
    SixelDisplayMode = 80,
    /// Enable mouse button press/release reporting
    MouseTracking = 1000,
    /// Warning: this requires a cooperative and timely response from
    /// the application otherwise the terminal can hang
    HighlightMouseTracking = 1001,
    /// Enable mouse button press/release and drag reporting
    ButtonEventMouse = 1002,
    /// Enable mouse motion, button press/release and drag reporting
    AnyEventMouse = 1003,
    /// Enable FocusIn/FocusOut events
    FocusTracking = 1004,
    Utf8Mouse = 1005,
    /// Use extended coordinate system in mouse reporting.  Does not
    /// enable mouse reporting itself, it just controls how reports
    /// will be encoded.
    SGRMouse = 1006,
    /// Use pixels rather than text cells in mouse reporting.  Does
    /// not enable mouse reporting itself, it just controls how
    /// reports will be encoded.
    SGRPixelsMouse = 1016,

    XTermMetaSendsEscape = 1036,
    XTermAltSendsEscape = 1039,

    /// Save cursor as in DECSC
    SaveCursor = 1048,
    ClearAndEnableAlternateScreen = 1049,
    EnableAlternateScreen = 47,
    OptEnableAlternateScreen = 1047,
    BracketedPaste = 2004,

    /// <https://github.com/contour-terminal/terminal-unicode-core/>
    /// Grapheme clustering mode
    GraphemeClustering = 2027,

    /// Applies to sixel and regis modes
    UsePrivateColorRegistersForEachGraphic = 1070,

    /// <https://gist.github.com/christianparpart/d8a62cc1ab659194337d73e399004036>
    SynchronizedOutput = 2026,

    MinTTYApplicationEscapeKeyMode = 7727,

    /// xterm: adjust cursor positioning after emitting sixel
    SixelScrollsRight = 8452,

    /// Windows Terminal: win32-input-mode
    /// <https://github.com/microsoft/terminal/blob/main/doc/specs/%234999%20-%20Improved%20keyboard%20handling%20in%20Conpty.md>
    Win32InputMode = 9001,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalMode {
    Code(TerminalModeCode),
    Unspecified(u16),
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum TerminalModeCode {
    /// https://vt100.net/docs/vt510-rm/KAM.html
    KeyboardAction = 2,
    /// https://vt100.net/docs/vt510-rm/IRM.html
    Insert = 4,
    /// <https://terminal-wg.pages.freedesktop.org/bidi/recommendation/escape-sequences.html>
    BiDirectionalSupportMode = 8,
    /// https://vt100.net/docs/vt510-rm/SRM.html
    /// But in the MS terminal this is cursor blinking.
    SendReceive = 12,
    /// https://vt100.net/docs/vt510-rm/LNM.html
    AutomaticNewline = 20,
    /// MS terminal cursor visibility
    ShowCursor = 25,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cursor {
    /// CBT Moves cursor to the Ps tabs backward. The default value of Ps is 1.
    BackwardTabulation(u32),

    /// TBC - TABULATION CLEAR
    TabulationClear(TabulationClear),

    /// CHA: Moves cursor to the Ps-th column of the active line. The default
    /// value of Ps is 1.
    CharacterAbsolute(OneBased),

    /// HPA CHARACTER POSITION ABSOLUTE
    /// HPA Moves cursor to the Ps-th column of the active line. The default
    /// value of Ps is 1.
    CharacterPositionAbsolute(OneBased),

    /// HPB - CHARACTER POSITION BACKWARD
    /// HPB Moves cursor to the left Ps columns. The default value of Ps is 1.
    CharacterPositionBackward(u32),

    /// HPR - CHARACTER POSITION FORWARD
    /// HPR Moves cursor to the right Ps columns. The default value of Ps is 1.
    CharacterPositionForward(u32),

    /// HVP - CHARACTER AND LINE POSITION
    /// HVP Moves cursor to the Ps1-th line and to the Ps2-th column. The
    /// default value of Ps1 and Ps2 is 1.
    CharacterAndLinePosition {
        line: OneBased,
        col: OneBased,
    },

    /// VPA - LINE POSITION ABSOLUTE
    /// Move to the corresponding vertical position (line Ps) of the current
    /// column. The default value of Ps is 1.
    LinePositionAbsolute(u32),

    /// VPB - LINE POSITION BACKWARD
    /// Moves cursor up Ps lines in the same column. The default value of Ps is
    /// 1.
    LinePositionBackward(u32),

    /// VPR - LINE POSITION FORWARD
    /// Moves cursor down Ps lines in the same column. The default value of Ps
    /// is 1.
    LinePositionForward(u32),

    /// CHT
    /// Moves cursor to the Ps tabs forward. The default value of Ps is 1.
    ForwardTabulation(u32),

    /// CNL Moves cursor to the first column of Ps-th following line. The
    /// default value of Ps is 1.
    NextLine(u32),

    /// CPL Moves cursor to the first column of Ps-th preceding line. The
    /// default value of Ps is 1.
    PrecedingLine(u32),

    /// CPR - ACTIVE POSITION REPORT
    /// If the DEVICE COMPONENT SELECT MODE (DCSM)
    /// is set to PRESENTATION, CPR is used to report the active presentation
    /// position of the sending device as residing in the presentation
    /// component at the n-th line position according to the line progression
    /// and at the m-th character position according to the character path,
    /// where n equals the value of Pn1 and m equal s the value of Pn2.
    /// If the DEVICE COMPONENT SELECT MODE (DCSM) is set to DATA, CPR is used
    /// to report the active data position of the sending device as
    /// residing in the data component at the n-th line position according
    /// to the line progression and at the m-th character position
    /// according to the character progression, where n equals the value of
    /// Pn1 and m equals the value of Pn2. CPR may be solicited by a DEVICE
    /// STATUS REPORT (DSR) or be sent unsolicited .
    ActivePositionReport {
        line: OneBased,
        col: OneBased,
    },

    /// CPR: this is the request from the client.
    /// The terminal will respond with ActivePositionReport.
    RequestActivePositionReport,

    /// SCP - Save Cursor Position.
    /// Only works when DECLRMM is disabled
    SaveCursor,
    RestoreCursor,

    /// CTC - CURSOR TABULATION CONTROL
    /// CTC causes one or more tabulation stops to be set or cleared in the
    /// presentation component, depending on the parameter values.
    /// In the case of parameter values 0, 2 or 4 the number of lines affected
    /// depends on the setting of the TABULATION STOP MODE (TSM).
    TabulationControl(CursorTabulationControl),

    /// CUB - Cursor Left
    /// Moves cursor to the left Ps columns. The default value of Ps is 1.
    Left(u32),

    /// CUD - Cursor Down
    Down(u32),

    /// CUF - Cursor Right
    Right(u32),

    /// CUP - Cursor Position
    /// Moves cursor to the Ps1-th line and to the Ps2-th column. The default
    /// value of Ps1 and Ps2 is 1.
    Position {
        line: OneBased,
        col: OneBased,
    },

    /// CUU - Cursor Up
    Up(u32),

    /// CVT - Cursor Line Tabulation
    /// CVT causes the active presentation position to be moved to the
    /// corresponding character position of the line corresponding to the n-th
    /// following line tabulation stop in the presentation component, where n
    /// equals the value of Pn.
    LineTabulation(u32),

    /// DECSTBM - Set top and bottom margins.
    SetTopAndBottomMargins {
        top: OneBased,
        bottom: OneBased,
    },

    /// https://vt100.net/docs/vt510-rm/DECSLRM.html
    SetLeftAndRightMargins {
        left: OneBased,
        right: OneBased,
    },

    CursorStyle(CursorStyle),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// DCH - DELETE CHARACTER
    /// Deletes Ps characters from the cursor position to the right. The
    /// default value of Ps is 1. If the DEVICE COMPONENT SELECT MODE
    /// (DCSM) is set to PRESENTATION, DCH causes the contents of the
    /// active presentation position and, depending on the setting of the
    /// CHARACTER EDITING MODE (HEM), the contents of the n-1 preceding or
    /// following character positions to be removed from the presentation
    /// component, where n equals the value of Pn. The resulting gap is
    /// closed by shifting the contents of the adjacent character positions
    /// towards the active presentation position. At the other end of the
    /// shifted part, n character positions are put into the erased state.
    DeleteCharacter(u32),

    /// DL - DELETE LINE
    /// If the DEVICE COMPONENT SELECT MODE (DCSM) is set to PRESENTATION, DL
    /// causes the contents of the active line (the line that contains the
    /// active presentation position) and, depending on the setting of the
    /// LINE EDITING MODE (VEM), the contents of the n-1 preceding or
    /// following lines to be removed from the presentation component, where n
    /// equals the value of Pn. The resulting gap is closed by shifting the
    /// contents of a number of adjacent lines towards the active line. At
    /// the other end of the shifted part, n lines are put into the
    /// erased state.  The active presentation position is moved to the line
    /// home position in the active line. The line home position is
    /// established by the parameter value of SET LINE HOME (SLH). If the
    /// TABULATION STOP MODE (TSM) is set to SINGLE, character tabulation stops
    /// are cleared in the lines that are put into the erased state.  The
    /// extent of the shifted part is established by SELECT EDITING EXTENT
    /// (SEE).  Any occurrences of the start or end of a selected area, the
    /// start or end of a qualified area, or a tabulation stop in the shifted
    /// part, are also shifted.
    DeleteLine(u32),

    /// ECH - ERASE CHARACTER
    /// If the DEVICE COMPONENT SELECT MODE (DCSM) is set to PRESENTATION, ECH
    /// causes the active presentation position and the n-1 following
    /// character positions in the presentation component to be put into
    /// the erased state, where n equals the value of Pn.
    EraseCharacter(u32),

    /// EL - ERASE IN LINE
    /// If the DEVICE COMPONENT SELECT MODE (DCSM) is set to PRESENTATION, EL
    /// causes some or all character positions of the active line (the line
    /// which contains the active presentation position in the presentation
    /// component) to be put into the erased state, depending on the
    /// parameter values
    EraseInLine(EraseInLine),

    /// ICH - INSERT CHARACTER
    /// If the DEVICE COMPONENT SELECT MODE (DCSM) is set to PRESENTATION, ICH
    /// is used to prepare the insertion of n characters, by putting into the
    /// erased state the active presentation position and, depending on the
    /// setting of the CHARACTER EDITING MODE (HEM), the n-1 preceding or
    /// following character positions in the presentation component, where n
    /// equals the value of Pn. The previous contents of the active
    /// presentation position and an adjacent string of character positions are
    /// shifted away from the active presentation position. The contents of n
    /// character positions at the other end of the shifted part are removed.
    /// The active presentation position is moved to the line home position in
    /// the active line. The line home position is established by the parameter
    /// value of SET LINE HOME (SLH).
    InsertCharacter(u32),

    /// IL - INSERT LINE
    /// If the DEVICE COMPONENT SELECT MODE (DCSM) is set to PRESENTATION, IL
    /// is used to prepare the insertion of n lines, by putting into the
    /// erased state in the presentation component the active line (the
    /// line that contains the active presentation position) and, depending on
    /// the setting of the LINE EDITING MODE (VEM), the n-1 preceding or
    /// following lines, where n equals the value of Pn. The previous
    /// contents of the active line and of adjacent lines are shifted away
    /// from the active line. The contents of n lines at the other end of the
    /// shifted part are removed. The active presentation position is moved
    /// to the line home position in the active line. The line home
    /// position is established by the parameter value of SET LINE
    /// HOME (SLH).
    InsertLine(u32),

    /// SD - SCROLL DOWN
    /// SD causes the data in the presentation component to be moved by n line
    /// positions if the line orientation is horizontal, or by n character
    /// positions if the line orientation is vertical, such that the data
    /// appear to move down; where n equals the value of Pn. The active
    /// presentation position is not affected by this control function.
    ///
    /// Also known as Pan Up in DEC:
    /// https://vt100.net/docs/vt510-rm/SD.html
    ScrollDown(u32),

    /// SU - SCROLL UP
    /// SU causes the data in the presentation component to be moved by n line
    /// positions if the line orientation is horizontal, or by n character
    /// positions if the line orientation is vertical, such that the data
    /// appear to move up; where n equals the value of Pn. The active
    /// presentation position is not affected by this control function.
    ScrollUp(u32),

    /// ED - ERASE IN PAGE (XTerm calls this Erase in Display)
    EraseInDisplay(EraseInDisplay),

    /// REP - Repeat the preceding character n times
    Repeat(u32),
}

trait EncodeCSIParam {
    fn write_csi(&self, f: &mut Formatter, control: &str) -> Result<(), FmtError>;
}

impl<T: ParamEnum + PartialEq + ToPrimitive> EncodeCSIParam for T {
    fn write_csi(&self, f: &mut Formatter, control: &str) -> Result<(), FmtError> {
        if *self == ParamEnum::default() {
            write!(f, "{}", control)
        } else {
            let value = self.to_i64().ok_or_else(|| FmtError)?;
            write!(f, "{}{}", value, control)
        }
    }
}

impl EncodeCSIParam for u32 {
    fn write_csi(&self, f: &mut Formatter, control: &str) -> Result<(), FmtError> {
        if *self == 1 {
            write!(f, "{}", control)
        } else {
            write!(f, "{}{}", *self, control)
        }
    }
}

impl EncodeCSIParam for OneBased {
    fn write_csi(&self, f: &mut Formatter, control: &str) -> Result<(), FmtError> {
        if self.as_one_based() == 1 {
            write!(f, "{}", control)
        } else {
            write!(f, "{}{}", *self, control)
        }
    }
}

impl Display for Edit {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Edit::DeleteCharacter(n) => n.write_csi(f, "P")?,
            Edit::DeleteLine(n) => n.write_csi(f, "M")?,
            Edit::EraseCharacter(n) => n.write_csi(f, "X")?,
            Edit::EraseInLine(n) => n.write_csi(f, "K")?,
            Edit::InsertCharacter(n) => n.write_csi(f, "@")?,
            Edit::InsertLine(n) => n.write_csi(f, "L")?,
            Edit::ScrollDown(n) => n.write_csi(f, "T")?,
            Edit::ScrollUp(n) => n.write_csi(f, "S")?,
            Edit::EraseInDisplay(n) => n.write_csi(f, "J")?,
            Edit::Repeat(n) => n.write_csi(f, "b")?,
        }
        Ok(())
    }
}

impl Display for Cursor {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        match self {
            Cursor::BackwardTabulation(n) => n.write_csi(f, "Z")?,
            Cursor::CharacterAbsolute(col) => col.write_csi(f, "G")?,
            Cursor::ForwardTabulation(n) => n.write_csi(f, "I")?,
            Cursor::NextLine(n) => n.write_csi(f, "E")?,
            Cursor::PrecedingLine(n) => n.write_csi(f, "F")?,
            Cursor::ActivePositionReport { line, col } => write!(f, "{};{}R", line, col)?,
            Cursor::Left(n) => n.write_csi(f, "D")?,
            Cursor::Down(n) => n.write_csi(f, "B")?,
            Cursor::Right(n) => n.write_csi(f, "C")?,
            Cursor::Up(n) => n.write_csi(f, "A")?,
            Cursor::Position { line, col } => write!(f, "{};{}H", line, col)?,
            Cursor::LineTabulation(n) => n.write_csi(f, "Y")?,
            Cursor::TabulationControl(n) => n.write_csi(f, "W")?,
            Cursor::TabulationClear(n) => n.write_csi(f, "g")?,
            Cursor::CharacterPositionAbsolute(n) => n.write_csi(f, "`")?,
            Cursor::CharacterPositionBackward(n) => n.write_csi(f, "j")?,
            Cursor::CharacterPositionForward(n) => n.write_csi(f, "a")?,
            Cursor::CharacterAndLinePosition { line, col } => write!(f, "{};{}f", line, col)?,
            Cursor::LinePositionAbsolute(n) => n.write_csi(f, "d")?,
            Cursor::LinePositionBackward(n) => n.write_csi(f, "k")?,
            Cursor::LinePositionForward(n) => n.write_csi(f, "e")?,
            Cursor::SetTopAndBottomMargins { top, bottom } => {
                if top.as_one_based() == 1 && bottom.as_one_based() == u32::max_value() {
                    write!(f, "r")?;
                } else {
                    write!(f, "{};{}r", top, bottom)?;
                }
            },
            Cursor::SetLeftAndRightMargins { left, right } => {
                if left.as_one_based() == 1 && right.as_one_based() == u32::max_value() {
                    write!(f, "s")?;
                } else {
                    write!(f, "{};{}s", left, right)?;
                }
            },
            Cursor::RequestActivePositionReport => write!(f, "6n")?,
            Cursor::SaveCursor => write!(f, "s")?,
            Cursor::RestoreCursor => write!(f, "u")?,
            Cursor::CursorStyle(style) => write!(f, "{} q", *style as u8)?,
        }
        Ok(())
    }
}

/// This trait aids in parsing escape sequences.
/// In many cases we simply want to collect integral values >= 1,
/// but in some we build out an enum.  The trait helps to generalize
/// the parser code while keeping it relatively terse.
trait ParseParams: Sized {
    fn parse_params(params: &[CsiParam]) -> Result<Self, ()>;
}

/// Parse an input parameter into a 1-based unsigned value
impl ParseParams for u32 {
    fn parse_params(params: &[CsiParam]) -> Result<u32, ()> {
        match params {
            [] => Ok(1),
            [p] => to_1b_u32(p),
            _ => Err(()),
        }
    }
}

/// Parse an input parameter into a 1-based unsigned value
impl ParseParams for OneBased {
    fn parse_params(params: &[CsiParam]) -> Result<OneBased, ()> {
        match params {
            [] => Ok(OneBased::new(1)),
            [p] => OneBased::from_esc_param(p),
            _ => Err(()),
        }
    }
}

/// Parse a pair of 1-based unsigned values into a tuple.
/// This is typically used to build a struct comprised of
/// the pair of values.
impl ParseParams for (OneBased, OneBased) {
    fn parse_params(params: &[CsiParam]) -> Result<(OneBased, OneBased), ()> {
        match params {
            [] => Ok((OneBased::new(1), OneBased::new(1))),
            [p] => Ok((OneBased::from_esc_param(p)?, OneBased::new(1))),
            [a, CsiParam::P(b';'), b] => {
                Ok((OneBased::from_esc_param(a)?, OneBased::from_esc_param(b)?))
            },
            [CsiParam::P(b';'), b] => Ok((OneBased::new(1), OneBased::from_esc_param(b)?)),
            _ => Err(()),
        }
    }
}

/// This is ostensibly a marker trait that is used within this module
/// to denote an enum.  It does double duty as a stand-in for Default.
/// We need separate traits for this to disambiguate from a regular
/// primitive integer.
trait ParamEnum: FromPrimitive {
    fn default() -> Self;
}

/// implement ParseParams for the enums that also implement ParamEnum.
impl<T: ParamEnum> ParseParams for T {
    fn parse_params(params: &[CsiParam]) -> Result<Self, ()> {
        match params {
            [] => Ok(ParamEnum::default()),
            [CsiParam::Integer(i)] => FromPrimitive::from_i64(*i).ok_or(()),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, Copy, ToPrimitive)]
pub enum CursorTabulationControl {
    SetCharacterTabStopAtActivePosition = 0,
    SetLineTabStopAtActiveLine = 1,
    ClearCharacterTabStopAtActivePosition = 2,
    ClearLineTabstopAtActiveLine = 3,
    ClearAllCharacterTabStopsAtActiveLine = 4,
    ClearAllCharacterTabStops = 5,
    ClearAllLineTabStops = 6,
}

impl ParamEnum for CursorTabulationControl {
    fn default() -> Self {
        CursorTabulationControl::SetCharacterTabStopAtActivePosition
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, Copy, ToPrimitive)]
pub enum TabulationClear {
    ClearCharacterTabStopAtActivePosition = 0,
    ClearLineTabStopAtActiveLine = 1,
    ClearCharacterTabStopsAtActiveLine = 2,
    ClearAllCharacterTabStops = 3,
    ClearAllLineTabStops = 4,
    ClearAllTabStops = 5,
}

impl ParamEnum for TabulationClear {
    fn default() -> Self {
        TabulationClear::ClearCharacterTabStopAtActivePosition
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, Copy, ToPrimitive)]
pub enum EraseInLine {
    EraseToEndOfLine = 0,
    EraseToStartOfLine = 1,
    EraseLine = 2,
}

impl ParamEnum for EraseInLine {
    fn default() -> Self {
        EraseInLine::EraseToEndOfLine
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, Copy, ToPrimitive)]
pub enum EraseInDisplay {
    /// the active presentation position and the character positions up to the
    /// end of the page are put into the erased state
    EraseToEndOfDisplay = 0,
    /// the character positions from the beginning of the page up to and
    /// including the active presentation position are put into the erased
    /// state
    EraseToStartOfDisplay = 1,
    /// all character positions of the page are put into the erased state
    EraseDisplay = 2,
    /// Clears the scrollback.  This is an Xterm extension to ECMA-48.
    EraseScrollback = 3,
}

impl ParamEnum for EraseInDisplay {
    fn default() -> Self {
        EraseInDisplay::EraseToEndOfDisplay
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sgr {
    /// Resets rendition to defaults.  Typically switches off
    /// all other Sgr options, but may have greater or lesser impact.
    Reset,
    /// Set the intensity/bold level
    Intensity(Intensity),
    Underline(Underline),
    UnderlineColor(ColorSpec),
    Blink(Blink),
    Italic(bool),
    Inverse(bool),
    Invisible(bool),
    StrikeThrough(bool),
    Font(Font),
    Foreground(ColorSpec),
    Background(ColorSpec),
    Overline(bool),
    VerticalAlign(VerticalAlign),
}

#[cfg(all(test, target_pointer_width = "64"))]
#[test]
fn sgr_size() {
    assert_eq!(std::mem::size_of::<Intensity>(), 1);
    assert_eq!(std::mem::size_of::<Underline>(), 1);
    assert_eq!(std::mem::size_of::<ColorSpec>(), 20);
    assert_eq!(std::mem::size_of::<Blink>(), 1);
    assert_eq!(std::mem::size_of::<Font>(), 2);
}

impl Display for Sgr {
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        macro_rules! code {
            ($t:ident) => {
                write!(f, "{}m", SgrCode::$t as i64)?
            };
        }

        macro_rules! ansi_color {
            ($idx:expr, $eightbit:ident, $( ($Ansi:ident, $code:ident) ),*) => {
                if let Some(ansi) = FromPrimitive::from_u8($idx) {
                    match ansi {
                        $(AnsiColor::$Ansi => code!($code) ,)*
                    }
                } else {
                    write!(f, "{}:5:{}m", SgrCode::$eightbit as i64, $idx)?
                }
            }
        }

        match self {
            Sgr::Reset => code!(Reset),
            Sgr::Intensity(Intensity::Bold) => code!(IntensityBold),
            Sgr::Intensity(Intensity::Half) => code!(IntensityDim),
            Sgr::Intensity(Intensity::Normal) => code!(NormalIntensity),
            Sgr::Underline(Underline::Single) => code!(UnderlineOn),
            Sgr::Underline(Underline::Double) => code!(UnderlineDouble),
            Sgr::Underline(Underline::Curly) => write!(f, "4:3m")?,
            Sgr::Underline(Underline::Dotted) => write!(f, "4:4m")?,
            Sgr::Underline(Underline::Dashed) => write!(f, "4:5m")?,
            Sgr::Underline(Underline::None) => code!(UnderlineOff),
            Sgr::Blink(Blink::Slow) => code!(BlinkOn),
            Sgr::Blink(Blink::Rapid) => code!(RapidBlinkOn),
            Sgr::Blink(Blink::None) => code!(BlinkOff),
            Sgr::Italic(true) => code!(ItalicOn),
            Sgr::Italic(false) => code!(ItalicOff),
            Sgr::Inverse(true) => code!(InverseOn),
            Sgr::Inverse(false) => code!(InverseOff),
            Sgr::Invisible(true) => code!(InvisibleOn),
            Sgr::Invisible(false) => code!(InvisibleOff),
            Sgr::StrikeThrough(true) => code!(StrikeThroughOn),
            Sgr::StrikeThrough(false) => code!(StrikeThroughOff),
            Sgr::Overline(true) => code!(OverlineOn),
            Sgr::Overline(false) => code!(OverlineOff),
            Sgr::VerticalAlign(VerticalAlign::BaseLine) => code!(VerticalAlignBaseLine),
            Sgr::VerticalAlign(VerticalAlign::SuperScript) => code!(VerticalAlignSuperScript),
            Sgr::VerticalAlign(VerticalAlign::SubScript) => code!(VerticalAlignSubScript),
            Sgr::Font(Font::Default) => code!(DefaultFont),
            Sgr::Font(Font::Alternate(1)) => code!(AltFont1),
            Sgr::Font(Font::Alternate(2)) => code!(AltFont2),
            Sgr::Font(Font::Alternate(3)) => code!(AltFont3),
            Sgr::Font(Font::Alternate(4)) => code!(AltFont4),
            Sgr::Font(Font::Alternate(5)) => code!(AltFont5),
            Sgr::Font(Font::Alternate(6)) => code!(AltFont6),
            Sgr::Font(Font::Alternate(7)) => code!(AltFont7),
            Sgr::Font(Font::Alternate(8)) => code!(AltFont8),
            Sgr::Font(Font::Alternate(9)) => code!(AltFont9),
            Sgr::Font(_) => { /* there are no other possible font values */ },
            Sgr::Foreground(ColorSpec::Default) => code!(ForegroundDefault),
            Sgr::Background(ColorSpec::Default) => code!(BackgroundDefault),
            Sgr::Foreground(ColorSpec::PaletteIndex(idx)) => ansi_color!(
                *idx,
                ForegroundColor,
                (Black, ForegroundBlack),
                (Maroon, ForegroundRed),
                (Green, ForegroundGreen),
                (Olive, ForegroundYellow),
                (Navy, ForegroundBlue),
                (Purple, ForegroundMagenta),
                (Teal, ForegroundCyan),
                (Silver, ForegroundWhite),
                // Note: these brights are emitted using codes in the 100 range.
                // I don't know how portable this is vs. the 256 color sequences,
                // so we may need to make an adjustment here later.
                (Grey, ForegroundBrightBlack),
                (Red, ForegroundBrightRed),
                (Lime, ForegroundBrightGreen),
                (Yellow, ForegroundBrightYellow),
                (Blue, ForegroundBrightBlue),
                (Fuchsia, ForegroundBrightMagenta),
                (Aqua, ForegroundBrightCyan),
                (White, ForegroundBrightWhite)
            ),
            Sgr::Foreground(ColorSpec::TrueColor(c)) => {
                let (red, green, blue, alpha) = c.to_srgb_u8();
                if alpha == 255 {
                    write!(
                        f,
                        "{}:2::{}:{}:{}m",
                        SgrCode::ForegroundColor as i64,
                        red,
                        green,
                        blue
                    )?
                } else {
                    write!(
                        f,
                        "{}:6::{}:{}:{}:{}m",
                        SgrCode::ForegroundColor as i64,
                        red,
                        green,
                        blue,
                        alpha
                    )?
                }
            },
            Sgr::Background(ColorSpec::PaletteIndex(idx)) => ansi_color!(
                *idx,
                BackgroundColor,
                (Black, BackgroundBlack),
                (Maroon, BackgroundRed),
                (Green, BackgroundGreen),
                (Olive, BackgroundYellow),
                (Navy, BackgroundBlue),
                (Purple, BackgroundMagenta),
                (Teal, BackgroundCyan),
                (Silver, BackgroundWhite),
                // Note: these brights are emitted using codes in the 100 range.
                // I don't know how portable this is vs. the 256 color sequences,
                // so we may need to make an adjustment here later.
                (Grey, BackgroundBrightBlack),
                (Red, BackgroundBrightRed),
                (Lime, BackgroundBrightGreen),
                (Yellow, BackgroundBrightYellow),
                (Blue, BackgroundBrightBlue),
                (Fuchsia, BackgroundBrightMagenta),
                (Aqua, BackgroundBrightCyan),
                (White, BackgroundBrightWhite)
            ),
            Sgr::Background(ColorSpec::TrueColor(c)) => {
                let (red, green, blue, alpha) = c.to_srgb_u8();
                if alpha == 255 {
                    write!(
                        f,
                        "{}:2::{}:{}:{}m",
                        SgrCode::BackgroundColor as i64,
                        red,
                        green,
                        blue
                    )?
                } else {
                    write!(
                        f,
                        "{}:6::{}:{}:{}:{}m",
                        SgrCode::BackgroundColor as i64,
                        red,
                        green,
                        blue,
                        alpha
                    )?
                }
            },
            Sgr::UnderlineColor(ColorSpec::Default) => code!(ResetUnderlineColor),
            Sgr::UnderlineColor(ColorSpec::TrueColor(c)) => {
                let (red, green, blue, alpha) = c.to_srgb_u8();
                if alpha == 255 {
                    write!(
                        f,
                        "{}:2::{}:{}:{}m",
                        SgrCode::UnderlineColor as i64,
                        red,
                        green,
                        blue
                    )?
                } else {
                    write!(
                        f,
                        "{}:6::{}:{}:{}:{}m",
                        SgrCode::UnderlineColor as i64,
                        red,
                        green,
                        blue,
                        alpha
                    )?
                }
            },
            Sgr::UnderlineColor(ColorSpec::PaletteIndex(idx)) => {
                write!(f, "{}:5:{}m", SgrCode::UnderlineColor as i64, *idx)?
            },
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Font {
    Default,
    Alternate(u8),
}

/// Constrol Sequence Initiator (CSI) Parser.
/// Since many sequences allow for composition of actions by separating
/// `;` character, we need to be able to iterate over
/// the set of parsed actions from a given CSI sequence.
/// `CSIParser` implements an Iterator that yields `CSI` instances as
/// it parses them out from the input sequence.
struct CSIParser<'a> {
    /// this flag is set when more than two intermediates
    /// arrived and subsequent characters were ignored.
    parameters_truncated: bool,
    control: char,
    /// While params is_some we have more data to consume.  The advance_by
    /// method updates the slice as we consume data.
    /// In a number of cases an empty params list is used to indicate
    /// default values, especially for SGR, so we need to be careful not
    /// to update params to an empty slice.
    params: Option<&'a [CsiParam]>,
    orig_params: &'a [CsiParam],
}

impl CSI {
    /// Parse a CSI sequence.
    /// Returns an iterator that yields individual CSI actions.
    /// Why not a single?  Because sequences like `CSI [ 1 ; 3 m`
    /// embed two separate actions but are sent as a single unit.
    /// If no semantic meaning is known for a subsequence, the remainder
    /// of the sequence is returned wrapped in a `CSI::Unspecified` container.
    pub fn parse<'a>(
        params: &'a [CsiParam],
        parameters_truncated: bool,
        control: char,
    ) -> impl Iterator<Item = CSI> + 'a {
        CSIParser {
            parameters_truncated,
            control,
            params: Some(params),
            orig_params: params,
        }
    }
}

/// A little helper to convert i64 -> u8 if safe
fn to_u8(v: &CsiParam) -> Result<u8, ()> {
    match v {
        CsiParam::P(_) => Err(()),
        CsiParam::Integer(v) => {
            if *v <= i64::from(u8::max_value()) {
                Ok(*v as u8)
            } else {
                Err(())
            }
        },
    }
}

/// Convert the input value to 1-based u32.
/// The intent is to protect consumers from out of range values
/// when operating on the data, while balancing strictness with
/// practical implementation bugs.  For example, it is common
/// to see 0 values being emitted from existing libraries, and
/// we desire to see the intended output.
/// Ensures that the value is in the range 1..=max_value.
/// If the input is 0 it is treated as 1.  If the value is
/// otherwise outside that range, an error is propagated and
/// that will typically case the sequence to be reported via
/// the Unspecified placeholder.
fn to_1b_u32(v: &CsiParam) -> Result<u32, ()> {
    match v {
        CsiParam::Integer(v) if *v == 0 => Ok(1),
        CsiParam::Integer(v) if *v > 0 && *v <= i64::from(u32::max_value()) => Ok(*v as u32),
        _ => Err(()),
    }
}

struct Cracked {
    params: Vec<Option<CsiParam>>,
}

impl Cracked {
    pub fn parse(params: &[CsiParam]) -> Result<Self, ()> {
        let mut res = vec![];
        let mut iter = params.iter().peekable();
        while let Some(p) = iter.next() {
            match p {
                CsiParam::P(b';') => {
                    res.push(None);
                },
                CsiParam::Integer(_) => {
                    res.push(Some(p.clone()));
                    if let Some(CsiParam::P(b';')) = iter.peek() {
                        iter.next();
                    }
                },
                _ => return Err(()),
            }
        }
        Ok(Self { params: res })
    }

    pub fn get(&self, idx: usize) -> Option<&CsiParam> {
        self.params.get(idx)?.as_ref()
    }

    pub fn opt_int(&self, idx: usize) -> Option<i64> {
        self.get(idx).and_then(CsiParam::as_integer)
    }

    pub fn int(&self, idx: usize) -> Result<i64, ()> {
        self.get(idx).and_then(CsiParam::as_integer).ok_or(())
    }

    pub fn len(&self) -> usize {
        self.params.len()
    }
}

macro_rules! noparams {
    ($ns:ident, $variant:ident, $params:expr) => {{
        if $params.len() != 0 {
            Err(())
        } else {
            Ok(CSI::$ns($ns::$variant))
        }
    }};
}

macro_rules! parse {
    ($ns:ident, $variant:ident, $params:expr) => {{
        let value = ParseParams::parse_params($params)?;
        Ok(CSI::$ns($ns::$variant(value)))
    }};

    ($ns:ident, $variant:ident, $first:ident, $second:ident, $params:expr) => {{
        let (p1, p2): (OneBased, OneBased) = ParseParams::parse_params($params)?;
        Ok(CSI::$ns($ns::$variant {
            $first: p1,
            $second: p2,
        }))
    }};
}

impl<'a> CSIParser<'a> {
    fn parse_next(&mut self, params: &'a [CsiParam]) -> Result<CSI, ()> {
        match (self.control, self.orig_params) {
            ('k', [.., CsiParam::P(b' ')]) => self.select_character_path(params),
            ('q', [.., CsiParam::P(b' ')]) => self.cursor_style(params),
            ('y', [.., CsiParam::P(b'*')]) => self.checksum_area(params),

            ('c', [CsiParam::P(b'='), ..]) => self
                .req_tertiary_device_attributes(params)
                .map(|dev| CSI::Device(Box::new(dev))),
            ('c', [CsiParam::P(b'>'), ..]) => self
                .req_secondary_device_attributes(params)
                .map(|dev| CSI::Device(Box::new(dev))),

            ('m', [CsiParam::P(b'<'), ..]) | ('M', [CsiParam::P(b'<'), ..]) => {
                self.mouse_sgr1006(params).map(CSI::Mouse)
            },

            ('c', [CsiParam::P(b'?'), ..]) => self
                .secondary_device_attributes(params)
                .map(|dev| CSI::Device(Box::new(dev))),

            ('S', [CsiParam::P(b'?'), ..]) => XtSmGraphics::parse(params),
            ('p', [CsiParam::Integer(_), CsiParam::P(b'$')])
            | ('p', [CsiParam::P(b'?'), CsiParam::Integer(_), CsiParam::P(b'$')]) => {
                self.decrqm(params)
            },
            ('h', [CsiParam::P(b'?'), ..]) => self
                .dec(self.focus(params, 1, 0))
                .map(|mode| CSI::Mode(Mode::SetDecPrivateMode(mode))),
            ('l', [CsiParam::P(b'?'), ..]) => self
                .dec(self.focus(params, 1, 0))
                .map(|mode| CSI::Mode(Mode::ResetDecPrivateMode(mode))),
            ('r', [CsiParam::P(b'?'), ..]) => self
                .dec(self.focus(params, 1, 0))
                .map(|mode| CSI::Mode(Mode::RestoreDecPrivateMode(mode))),
            ('q', [CsiParam::P(b'>'), ..]) => self
                .req_terminal_name_and_version(params)
                .map(|dev| CSI::Device(Box::new(dev))),
            ('s', [CsiParam::P(b'?'), ..]) => self
                .dec(self.focus(params, 1, 0))
                .map(|mode| CSI::Mode(Mode::SaveDecPrivateMode(mode))),
            ('m', [CsiParam::P(b'>'), ..]) => self.xterm_key_modifier(params),

            ('p', [CsiParam::P(b'!')]) => Ok(CSI::Device(Box::new(Device::SoftReset))),
            ('u', [CsiParam::P(b'='), CsiParam::Integer(flags)]) => {
                Ok(CSI::Keyboard(Keyboard::SetKittyState {
                    flags: KittyKeyboardFlags::from_bits_truncate(
                        (*flags).try_into().map_err(|_| ())?,
                    ),
                    mode: KittyKeyboardMode::AssignAll,
                }))
            },
            (
                'u',
                [CsiParam::P(b'='), CsiParam::Integer(flags), CsiParam::P(b';'), CsiParam::Integer(mode)],
            ) => Ok(CSI::Keyboard(Keyboard::SetKittyState {
                flags: KittyKeyboardFlags::from_bits_truncate((*flags).try_into().map_err(|_| ())?),
                mode: match *mode {
                    1 => KittyKeyboardMode::AssignAll,
                    2 => KittyKeyboardMode::SetSpecified,
                    3 => KittyKeyboardMode::ClearSpecified,
                    _ => return Err(()),
                },
            })),
            ('u', [CsiParam::P(b'>')]) => Ok(CSI::Keyboard(Keyboard::PushKittyState {
                flags: KittyKeyboardFlags::NONE,
                mode: KittyKeyboardMode::AssignAll,
            })),
            ('u', [CsiParam::P(b'>'), CsiParam::Integer(flags)]) => {
                Ok(CSI::Keyboard(Keyboard::PushKittyState {
                    flags: KittyKeyboardFlags::from_bits_truncate(
                        (*flags).try_into().map_err(|_| ())?,
                    ),
                    mode: KittyKeyboardMode::AssignAll,
                }))
            },
            (
                'u',
                [CsiParam::P(b'>'), CsiParam::Integer(flags), CsiParam::P(b';'), CsiParam::Integer(mode)],
            ) => Ok(CSI::Keyboard(Keyboard::PushKittyState {
                flags: KittyKeyboardFlags::from_bits_truncate((*flags).try_into().map_err(|_| ())?),
                mode: match *mode {
                    1 => KittyKeyboardMode::AssignAll,
                    2 => KittyKeyboardMode::SetSpecified,
                    3 => KittyKeyboardMode::ClearSpecified,
                    _ => return Err(()),
                },
            })),
            ('u', [CsiParam::P(b'?')]) => Ok(CSI::Keyboard(Keyboard::QueryKittySupport)),
            ('u', [CsiParam::P(b'?'), CsiParam::Integer(flags)]) => {
                Ok(CSI::Keyboard(Keyboard::ReportKittyState(
                    KittyKeyboardFlags::from_bits_truncate((*flags).try_into().map_err(|_| ())?),
                )))
            },
            ('u', [CsiParam::P(b'<'), CsiParam::Integer(how_many)]) => Ok(CSI::Keyboard(
                Keyboard::PopKittyState((*how_many).try_into().map_err(|_| ())?),
            )),
            ('u', [CsiParam::P(b'<')]) => Ok(CSI::Keyboard(Keyboard::PopKittyState(1))),

            _ => match self.control {
                'c' => self
                    .req_primary_device_attributes(params)
                    .map(|dev| CSI::Device(Box::new(dev))),

                '@' => parse!(Edit, InsertCharacter, params),
                '`' => parse!(Cursor, CharacterPositionAbsolute, params),
                'A' => parse!(Cursor, Up, params),
                'B' => parse!(Cursor, Down, params),
                'C' => parse!(Cursor, Right, params),
                'D' => parse!(Cursor, Left, params),
                'E' => parse!(Cursor, NextLine, params),
                'F' => parse!(Cursor, PrecedingLine, params),
                'G' => parse!(Cursor, CharacterAbsolute, params),
                'H' => parse!(Cursor, Position, line, col, params),
                'I' => parse!(Cursor, ForwardTabulation, params),
                'J' => parse!(Edit, EraseInDisplay, params),
                'K' => parse!(Edit, EraseInLine, params),
                'L' => parse!(Edit, InsertLine, params),
                'M' => parse!(Edit, DeleteLine, params),
                'P' => parse!(Edit, DeleteCharacter, params),
                'R' => parse!(Cursor, ActivePositionReport, line, col, params),
                'S' => parse!(Edit, ScrollUp, params),
                'T' => parse!(Edit, ScrollDown, params),
                'W' => parse!(Cursor, TabulationControl, params),
                'X' => parse!(Edit, EraseCharacter, params),
                'Y' => parse!(Cursor, LineTabulation, params),
                'Z' => parse!(Cursor, BackwardTabulation, params),

                'a' => parse!(Cursor, CharacterPositionForward, params),
                'b' => parse!(Edit, Repeat, params),
                'd' => parse!(Cursor, LinePositionAbsolute, params),
                'e' => parse!(Cursor, LinePositionForward, params),
                'f' => parse!(Cursor, CharacterAndLinePosition, line, col, params),
                'g' => parse!(Cursor, TabulationClear, params),
                'h' => self
                    .terminal_mode(params)
                    .map(|mode| CSI::Mode(Mode::SetMode(mode))),
                'j' => parse!(Cursor, CharacterPositionBackward, params),
                'k' => parse!(Cursor, LinePositionBackward, params),
                'l' => self
                    .terminal_mode(params)
                    .map(|mode| CSI::Mode(Mode::ResetMode(mode))),

                'm' => self.sgr(params).map(CSI::Sgr),
                'n' => self.dsr(params),
                'r' => self.decstbm(params),
                's' => self.decslrm(params),
                't' => self.window(params).map(|p| CSI::Window(Box::new(p))),
                'u' => noparams!(Cursor, RestoreCursor, params),
                'x' => self
                    .req_terminal_parameters(params)
                    .map(|dev| CSI::Device(Box::new(dev))),

                _ => Err(()),
            },
        }
    }

    /// Consume some number of elements from params and update it.
    /// Take care to avoid setting params back to an empty slice
    /// as this would trigger returning a default value and/or
    /// an unterminated parse loop.
    fn advance_by<T>(&mut self, n: usize, params: &'a [CsiParam], result: T) -> T {
        let n = if matches!(params.get(n), Some(CsiParam::P(b';'))) {
            n + 1
        } else {
            n
        };

        let (_, next) = params.split_at(n);
        if !next.is_empty() {
            self.params = Some(next);
        }
        result
    }

    fn focus(&self, params: &'a [CsiParam], from_start: usize, from_end: usize) -> &'a [CsiParam] {
        if params == self.orig_params {
            let len = params.len();
            &params[from_start..len - from_end]
        } else {
            params
        }
    }

    fn select_character_path(&mut self, params: &'a [CsiParam]) -> Result<CSI, ()> {
        fn path(n: i64) -> Result<CharacterPath, ()> {
            Ok(match n {
                0 => CharacterPath::ImplementationDefault,
                1 => CharacterPath::LeftToRightOrTopToBottom,
                2 => CharacterPath::RightToLeftOrBottomToTop,
                _ => return Err(()),
            })
        }

        match params {
            [CsiParam::P(b' ')] => Ok(self.advance_by(
                1,
                params,
                CSI::SelectCharacterPath(CharacterPath::ImplementationDefault, 0),
            )),
            [CsiParam::Integer(a), CsiParam::P(b' ')] => {
                Ok(self.advance_by(2, params, CSI::SelectCharacterPath(path(*a)?, 0)))
            },
            [CsiParam::Integer(a), CsiParam::P(b';'), CsiParam::Integer(b), CsiParam::P(b' ')] => {
                Ok(self.advance_by(4, params, CSI::SelectCharacterPath(path(*a)?, *b)))
            },
            _ => Err(()),
        }
    }

    fn cursor_style(&mut self, params: &'a [CsiParam]) -> Result<CSI, ()> {
        match params {
            [CsiParam::Integer(p), CsiParam::P(b' ')] => match FromPrimitive::from_i64(*p) {
                None => Err(()),
                Some(style) => {
                    Ok(self.advance_by(2, params, CSI::Cursor(Cursor::CursorStyle(style))))
                },
            },
            _ => Err(()),
        }
    }

    fn checksum_area(&mut self, params: &'a [CsiParam]) -> Result<CSI, ()> {
        let params = Cracked::parse(&params[..params.len() - 1])?;

        let request_id = params.int(0)?;
        let page_number = params.int(1)?;
        let top = OneBased::from_optional_esc_param(params.get(2))?;
        let left = OneBased::from_optional_esc_param(params.get(3))?;
        let bottom = OneBased::from_optional_esc_param(params.get(4))?;
        let right = OneBased::from_optional_esc_param(params.get(5))?;
        Ok(CSI::Window(Box::new(Window::ChecksumRectangularArea {
            request_id,
            page_number,
            top,
            left,
            bottom,
            right,
        })))
    }

    fn dsr(&mut self, params: &'a [CsiParam]) -> Result<CSI, ()> {
        match params {
            [CsiParam::Integer(5)] => {
                Ok(self.advance_by(1, params, CSI::Device(Box::new(Device::StatusReport))))
            },

            [CsiParam::Integer(6)] => {
                Ok(self.advance_by(1, params, CSI::Cursor(Cursor::RequestActivePositionReport)))
            },
            _ => Err(()),
        }
    }

    fn decstbm(&mut self, params: &'a [CsiParam]) -> Result<CSI, ()> {
        match params {
            [] => Ok(CSI::Cursor(Cursor::SetTopAndBottomMargins {
                top: OneBased::new(1),
                bottom: OneBased::new(u32::max_value()),
            })),
            [p] => Ok(self.advance_by(
                1,
                params,
                CSI::Cursor(Cursor::SetTopAndBottomMargins {
                    top: OneBased::from_esc_param(p)?,
                    bottom: OneBased::new(u32::max_value()),
                }),
            )),
            [a, CsiParam::P(b';'), b] => Ok(self.advance_by(
                3,
                params,
                CSI::Cursor(Cursor::SetTopAndBottomMargins {
                    top: OneBased::from_esc_param(a)?,
                    bottom: OneBased::from_esc_param_with_big_default(b)?,
                }),
            )),
            [CsiParam::P(b';'), b] => Ok(self.advance_by(
                2,
                params,
                CSI::Cursor(Cursor::SetTopAndBottomMargins {
                    top: OneBased::new(1),
                    bottom: OneBased::from_esc_param_with_big_default(b)?,
                }),
            )),
            _ => Err(()),
        }
    }

    fn xterm_key_modifier(&mut self, params: &'a [CsiParam]) -> Result<CSI, ()> {
        match params {
            [CsiParam::P(b'>'), a, CsiParam::P(b';'), b] => {
                let resource = XtermKeyModifierResource::parse(a.as_integer().ok_or_else(|| ())?)
                    .ok_or_else(|| ())?;
                Ok(self.advance_by(
                    4,
                    params,
                    CSI::Mode(Mode::XtermKeyMode {
                        resource,
                        value: Some(b.as_integer().ok_or_else(|| ())?),
                    }),
                ))
            },
            [CsiParam::P(b'>'), a, CsiParam::P(b';')] => {
                let resource = XtermKeyModifierResource::parse(a.as_integer().ok_or_else(|| ())?)
                    .ok_or_else(|| ())?;
                Ok(self.advance_by(
                    3,
                    params,
                    CSI::Mode(Mode::XtermKeyMode {
                        resource,
                        value: None,
                    }),
                ))
            },
            [CsiParam::P(b'>'), p] => {
                let resource = XtermKeyModifierResource::parse(p.as_integer().ok_or_else(|| ())?)
                    .ok_or_else(|| ())?;
                Ok(self.advance_by(
                    2,
                    params,
                    CSI::Mode(Mode::XtermKeyMode {
                        resource,
                        value: None,
                    }),
                ))
            },
            _ => Err(()),
        }
    }

    fn decslrm(&mut self, params: &'a [CsiParam]) -> Result<CSI, ()> {
        match params {
            [] => {
                // with no params this is a request to save the cursor
                // and is technically in conflict with SetLeftAndRightMargins.
                // The emulator needs to decide based on DECSLRM mode
                // whether this saves the cursor or is SetLeftAndRightMargins
                // with default parameters!
                Ok(CSI::Cursor(Cursor::SaveCursor))
            },
            [p] => Ok(self.advance_by(
                1,
                params,
                CSI::Cursor(Cursor::SetLeftAndRightMargins {
                    left: OneBased::from_esc_param(p)?,
                    right: OneBased::new(u32::max_value()),
                }),
            )),
            [a, CsiParam::P(b';'), b] => Ok(self.advance_by(
                3,
                params,
                CSI::Cursor(Cursor::SetLeftAndRightMargins {
                    left: OneBased::from_esc_param(a)?,
                    right: OneBased::from_esc_param(b)?,
                }),
            )),
            [CsiParam::P(b';'), b] => Ok(self.advance_by(
                2,
                params,
                CSI::Cursor(Cursor::SetLeftAndRightMargins {
                    left: OneBased::new(1),
                    right: OneBased::from_esc_param(b)?,
                }),
            )),
            _ => Err(()),
        }
    }

    fn req_primary_device_attributes(&mut self, params: &'a [CsiParam]) -> Result<Device, ()> {
        match params {
            [] => Ok(Device::RequestPrimaryDeviceAttributes),
            [CsiParam::Integer(0)] => {
                Ok(self.advance_by(1, params, Device::RequestPrimaryDeviceAttributes))
            },
            _ => Err(()),
        }
    }

    fn req_terminal_name_and_version(&mut self, params: &'a [CsiParam]) -> Result<Device, ()> {
        match params {
            [_] => Ok(Device::RequestTerminalNameAndVersion),

            [_, CsiParam::Integer(0)] => {
                Ok(self.advance_by(2, params, Device::RequestTerminalNameAndVersion))
            },
            _ => Err(()),
        }
    }

    fn req_secondary_device_attributes(&mut self, params: &'a [CsiParam]) -> Result<Device, ()> {
        match params {
            [CsiParam::P(b'>')] => Ok(Device::RequestSecondaryDeviceAttributes),
            [CsiParam::P(b'>'), CsiParam::Integer(0)] => {
                Ok(self.advance_by(2, params, Device::RequestSecondaryDeviceAttributes))
            },
            _ => Err(()),
        }
    }

    fn req_tertiary_device_attributes(&mut self, params: &'a [CsiParam]) -> Result<Device, ()> {
        match params {
            [CsiParam::P(b'=')] => Ok(Device::RequestTertiaryDeviceAttributes),
            [CsiParam::P(b'='), CsiParam::Integer(0)] => {
                Ok(self.advance_by(2, params, Device::RequestTertiaryDeviceAttributes))
            },
            _ => Err(()),
        }
    }

    fn secondary_device_attributes(&mut self, params: &'a [CsiParam]) -> Result<Device, ()> {
        match params {
            [_, CsiParam::Integer(1), CsiParam::P(b';'), CsiParam::Integer(0)] => Ok(self
                .advance_by(
                    4,
                    params,
                    Device::DeviceAttributes(DeviceAttributes::Vt101WithNoOptions),
                )),
            [_, CsiParam::Integer(6)] => {
                Ok(self.advance_by(2, params, Device::DeviceAttributes(DeviceAttributes::Vt102)))
            },
            [_, CsiParam::Integer(1), CsiParam::P(b';'), CsiParam::Integer(2)] => Ok(self
                .advance_by(
                    4,
                    params,
                    Device::DeviceAttributes(DeviceAttributes::Vt100WithAdvancedVideoOption),
                )),
            [_, CsiParam::Integer(62), ..] => Ok(self.advance_by(
                params.len(),
                params,
                Device::DeviceAttributes(DeviceAttributes::Vt220(
                    DeviceAttributeFlags::from_params(&params[2..]),
                )),
            )),
            [_, CsiParam::Integer(63), ..] => Ok(self.advance_by(
                params.len(),
                params,
                Device::DeviceAttributes(DeviceAttributes::Vt320(
                    DeviceAttributeFlags::from_params(&params[2..]),
                )),
            )),
            [_, CsiParam::Integer(64), ..] => Ok(self.advance_by(
                params.len(),
                params,
                Device::DeviceAttributes(DeviceAttributes::Vt420(
                    DeviceAttributeFlags::from_params(&params[2..]),
                )),
            )),
            _ => Err(()),
        }
    }

    fn req_terminal_parameters(&mut self, params: &'a [CsiParam]) -> Result<Device, ()> {
        match params {
            [] | [CsiParam::Integer(0)] => Ok(Device::RequestTerminalParameters(0)),
            [CsiParam::Integer(1)] => Ok(Device::RequestTerminalParameters(1)),
            _ => Err(()),
        }
    }

    /// Parse extended mouse reports known as SGR 1006 mode
    fn mouse_sgr1006(&mut self, params: &'a [CsiParam]) -> Result<MouseReport, ()> {
        let (p0, p1, p2) = match params {
            [CsiParam::P(b'<'), CsiParam::Integer(p0), CsiParam::P(b';'), CsiParam::Integer(p1), CsiParam::P(b';'), CsiParam::Integer(p2)] => {
                (*p0, *p1, *p2)
            },
            _ => return Err(()),
        };

        // 'M' encodes a press, 'm' a release.
        let button = match (self.control, p0 & 0b110_0011) {
            ('M', 0) => MouseButton::Button1Press,
            ('m', 0) => MouseButton::Button1Release,
            ('M', 1) => MouseButton::Button2Press,
            ('m', 1) => MouseButton::Button2Release,
            ('M', 2) => MouseButton::Button3Press,
            ('m', 2) => MouseButton::Button3Release,
            ('M', 64) => MouseButton::Button4Press,
            ('m', 64) => MouseButton::Button4Release,
            ('M', 65) => MouseButton::Button5Press,
            ('m', 65) => MouseButton::Button5Release,
            ('M', 66) => MouseButton::Button6Press,
            ('m', 66) => MouseButton::Button6Release,
            ('M', 67) => MouseButton::Button7Press,
            ('m', 67) => MouseButton::Button7Release,
            ('M', 32) => MouseButton::Button1Drag,
            ('M', 33) => MouseButton::Button2Drag,
            ('M', 34) => MouseButton::Button3Drag,
            // Note that there is some theoretical ambiguity with these None values.
            // The ambiguity stems from alternative encodings of the mouse protocol;
            // when set to SGR1006 mode the variants with the `3` parameter do not
            // occur.  They included here as a reminder for when support for those
            // other encodings is added and this block is likely copied and pasted
            // or refactored for re-use with them.
            ('M', 35) => MouseButton::None, // mouse motion with no buttons
            ('m', 35) => MouseButton::None, // mouse motion with no buttons (in Windows Terminal)
            ('M', 3) => MouseButton::None,  // legacy notification about button release
            ('m', 3) => MouseButton::None,  // release+press doesn't make sense
            _ => {
                return Err(());
            },
        };

        let mut modifiers = Modifiers::NONE;
        if p0 & 4 != 0 {
            modifiers |= Modifiers::SHIFT;
        }
        if p0 & 8 != 0 {
            modifiers |= Modifiers::ALT;
        }
        if p0 & 16 != 0 {
            modifiers |= Modifiers::CTRL;
        }

        Ok(self.advance_by(
            6,
            params,
            MouseReport::SGR1006 {
                x: p1 as u16,
                y: p2 as u16,
                button,
                modifiers,
            },
        ))
    }

    fn decrqm(&mut self, params: &'a [CsiParam]) -> Result<CSI, ()> {
        Ok(CSI::Mode(match params {
            [CsiParam::Integer(p), CsiParam::P(b'$')] => {
                Mode::QueryMode(match FromPrimitive::from_i64(*p) {
                    None => TerminalMode::Unspecified(p.to_u16().ok_or(())?),
                    Some(mode) => TerminalMode::Code(mode),
                })
            },
            [CsiParam::P(b'?'), CsiParam::Integer(p), CsiParam::P(b'$')] => {
                Mode::QueryDecPrivateMode(match FromPrimitive::from_i64(*p) {
                    None => DecPrivateMode::Unspecified(p.to_u16().ok_or(())?),
                    Some(mode) => DecPrivateMode::Code(mode),
                })
            },
            _ => return Err(()),
        }))
    }

    fn dec(&mut self, params: &'a [CsiParam]) -> Result<DecPrivateMode, ()> {
        match params {
            [CsiParam::Integer(p0), ..] => match FromPrimitive::from_i64(*p0) {
                None => Ok(self.advance_by(
                    1,
                    params,
                    DecPrivateMode::Unspecified(p0.to_u16().ok_or(())?),
                )),
                Some(mode) => Ok(self.advance_by(1, params, DecPrivateMode::Code(mode))),
            },
            _ => Err(()),
        }
    }

    fn terminal_mode(&mut self, params: &'a [CsiParam]) -> Result<TerminalMode, ()> {
        let p0 = params
            .get(0)
            .and_then(CsiParam::as_integer)
            .ok_or_else(|| ())?;
        match FromPrimitive::from_i64(p0) {
            None => {
                Ok(self.advance_by(1, params, TerminalMode::Unspecified(p0.to_u16().ok_or(())?)))
            },
            Some(mode) => Ok(self.advance_by(1, params, TerminalMode::Code(mode))),
        }
    }

    fn parse_sgr_color(&mut self, params: &'a [CsiParam]) -> Result<ColorSpec, ()> {
        match params {
            // wezterm extension to support an optional alpha channel in the `:` form only
            [_, CsiParam::P(b':'), CsiParam::Integer(6), CsiParam::P(b':'),
                    CsiParam::Integer(_colorspace), CsiParam::P(b':'),
                    red, CsiParam::P(b':'), green, CsiParam::P(b':'), blue, CsiParam::P(b':'), alpha, ..] => {
                let res: SrgbaTuple = (to_u8(red)?, to_u8(green)?, to_u8(blue)?, to_u8(alpha)?).into();
                Ok(self.advance_by(13, params, res.into()))
            }
            [_, CsiParam::P(b':'), CsiParam::Integer(6), CsiParam::P(b':'),
                    /* empty colorspace */ CsiParam::P(b':'),
                    red, CsiParam::P(b':'), green, CsiParam::P(b':'), blue, CsiParam::P(b':'), alpha, ..] => {
                let res: SrgbaTuple = (to_u8(red)?, to_u8(green)?, to_u8(blue)?, to_u8(alpha)?).into();
                Ok(self.advance_by(12, params, res.into()))
            }
            [_, CsiParam::P(b':'), CsiParam::Integer(6), CsiParam::P(b':'), red, CsiParam::P(b':'), green,
                    CsiParam::P(b':'), blue, CsiParam::P(b':'), alpha, ..] =>
            {
                let res: SrgbaTuple = (to_u8(red)?, to_u8(green)?, to_u8(blue)?, to_u8(alpha)?).into();
                Ok(self.advance_by(11, params, res.into()))
            }

            // standard sgr colors

            [_, CsiParam::P(b':'), CsiParam::Integer(2), CsiParam::P(b':'),
                    CsiParam::Integer(_colorspace), CsiParam::P(b':'),
                    red, CsiParam::P(b':'), green, CsiParam::P(b':'), blue, ..] => {
                let res = RgbColor::new_8bpc(to_u8(red)?, to_u8(green)?, to_u8(blue)?).into();
                Ok(self.advance_by(11, params, res))
            }

            [_, CsiParam::P(b':'), CsiParam::Integer(2), CsiParam::P(b':'), /* empty colorspace */ CsiParam::P(b':'), red, CsiParam::P(b':'), green, CsiParam::P(b':'), blue, ..] => {
                let res = RgbColor::new_8bpc(to_u8(red)?, to_u8(green)?, to_u8(blue)?).into();
                Ok(self.advance_by(10, params, res))
            }

            [_, CsiParam::P(b';'), CsiParam::Integer(2), CsiParam::P(b';'), red, CsiParam::P(b';'), green, CsiParam::P(b';'), blue, ..] |
            [_, CsiParam::P(b':'), CsiParam::Integer(2), CsiParam::P(b':'), red, CsiParam::P(b':'), green, CsiParam::P(b':'), blue, ..] =>
            {
                let res = RgbColor::new_8bpc(to_u8(red)?, to_u8(green)?, to_u8(blue)?).into();
                Ok(self.advance_by(9, params, res))
            }

            [_, CsiParam::P(b';'), CsiParam::Integer(5), CsiParam::P(b';'), idx, ..] |
            [_, CsiParam::P(b':'), CsiParam::Integer(5), CsiParam::P(b':'), idx, ..] => {
                Ok(self.advance_by(5, params, ColorSpec::PaletteIndex(to_u8(idx)?)))
            }
            _ => Err(()),
        }
    }

    fn window(&mut self, params: &'a [CsiParam]) -> Result<Window, ()> {
        let params = Cracked::parse(params)?;

        let p = params.int(0)?;
        let arg1 = params.opt_int(1);
        let arg2 = params.opt_int(2);

        match p {
            1 => Ok(Window::DeIconify),
            2 => Ok(Window::Iconify),
            3 => Ok(Window::MoveWindow {
                x: arg1.unwrap_or(0),
                y: arg2.unwrap_or(0),
            }),
            4 => Ok(Window::ResizeWindowPixels {
                height: arg1,
                width: arg2,
            }),
            5 => Ok(Window::RaiseWindow),
            6 => match params.len() {
                1 => Ok(Window::LowerWindow),
                _ => Ok(Window::ReportCellSizePixelsResponse {
                    height: arg1,
                    width: arg2,
                }),
            },
            7 => Ok(Window::RefreshWindow),
            8 => Ok(Window::ResizeWindowCells {
                height: arg1,
                width: arg2,
            }),
            9 => match arg1 {
                Some(0) => Ok(Window::RestoreMaximizedWindow),
                Some(1) => Ok(Window::MaximizeWindow),
                Some(2) => Ok(Window::MaximizeWindowVertically),
                Some(3) => Ok(Window::MaximizeWindowHorizontally),
                _ => Err(()),
            },
            10 => match arg1 {
                Some(0) => Ok(Window::UndoFullScreenMode),
                Some(1) => Ok(Window::ChangeToFullScreenMode),
                Some(2) => Ok(Window::ToggleFullScreen),
                _ => Err(()),
            },
            11 => Ok(Window::ReportWindowState),
            13 => match arg1 {
                None => Ok(Window::ReportWindowPosition),
                Some(2) => Ok(Window::ReportTextAreaPosition),
                _ => Err(()),
            },
            14 => match arg1 {
                None => Ok(Window::ReportTextAreaSizePixels),
                Some(2) => Ok(Window::ReportWindowSizePixels),
                _ => Err(()),
            },
            15 => Ok(Window::ReportScreenSizePixels),
            16 => Ok(Window::ReportCellSizePixels),
            18 => Ok(Window::ReportTextAreaSizeCells),
            19 => Ok(Window::ReportScreenSizeCells),
            20 => Ok(Window::ReportIconLabel),
            21 => Ok(Window::ReportWindowTitle),
            22 => match arg1 {
                Some(0) => Ok(Window::PushIconAndWindowTitle),
                Some(1) => Ok(Window::PushIconTitle),
                Some(2) => Ok(Window::PushWindowTitle),
                _ => Err(()),
            },
            23 => match arg1 {
                Some(0) => Ok(Window::PopIconAndWindowTitle),
                Some(1) => Ok(Window::PopIconTitle),
                Some(2) => Ok(Window::PopWindowTitle),
                _ => Err(()),
            },
            _ => Err(()),
        }
    }

    fn underline(&mut self, params: &'a [CsiParam]) -> Result<Sgr, ()> {
        let (sgr, n) = match params {
            [_, CsiParam::P(b':'), CsiParam::Integer(0), ..] => {
                (Sgr::Underline(Underline::None), 3)
            },
            [_, CsiParam::P(b':'), CsiParam::Integer(1), ..] => {
                (Sgr::Underline(Underline::Single), 3)
            },
            [_, CsiParam::P(b':'), CsiParam::Integer(2), ..] => {
                (Sgr::Underline(Underline::Double), 3)
            },
            [_, CsiParam::P(b':'), CsiParam::Integer(3), ..] => {
                (Sgr::Underline(Underline::Curly), 3)
            },
            [_, CsiParam::P(b':'), CsiParam::Integer(4), ..] => {
                (Sgr::Underline(Underline::Dotted), 3)
            },
            [_, CsiParam::P(b':'), CsiParam::Integer(5), ..] => {
                (Sgr::Underline(Underline::Dashed), 3)
            },
            _ => (Sgr::Underline(Underline::Single), 1),
        };

        Ok(self.advance_by(n, params, sgr))
    }

    fn sgr(&mut self, params: &'a [CsiParam]) -> Result<Sgr, ()> {
        if params.is_empty() {
            // With no parameters, treat as equivalent to Reset.
            Ok(Sgr::Reset)
        } else {
            for p in params {
                match p {
                    CsiParam::P(b';')
                    | CsiParam::P(b':')
                    | CsiParam::P(b'?')
                    | CsiParam::Integer(_) => {},
                    _ => return Err(()),
                }
            }

            // Consume a single parameter and return the parsed result
            macro_rules! one {
                ($t:expr) => {
                    Ok(self.advance_by(1, params, $t))
                };
            }

            match &params[0] {
                CsiParam::P(b';') => {
                    // Starting with an empty item is equivalent to a reset
                    self.advance_by(1, params, Ok(Sgr::Reset))
                },

                // There are a small number of DEC private SGR parameters that
                // have equivalents in the normal SGR space.
                // We're simply inlining recognizing them here, and mapping them
                // to those SGR equivalents. That makes parsing "lossy" in the
                // sense that the original sequence is lost, but semantically,
                // the result is the same.
                // These codes are taken from the "SGR" section of
                // "Digital ANSI-Compliant Printing Protocol
                // Level 2 Programming Reference Manual"
                // on page 7-78.
                // <https://vaxhaven.com/images/f/f7/EK-PPLV2-PM-B01.pdf>
                /* Withdrawn because xterm introduced a conflict:
                 * <https://github.com/mintty/mintty/issues/1171#issuecomment-1336174469>
                 * <https://github.com/mintty/mintty/issues/1189>
                CsiParam::P(b'?') if params.len() > 1 => match &params[1] {
                    // Consume two parameters and return the parsed result
                    macro_rules! two {
                        ($t:expr) => {
                            Ok(self.advance_by(2, params, $t))
                        };
                    }
                    CsiParam::Integer(i) => match FromPrimitive::from_i64(*i) {
                        None => Err(()),
                        Some(code) => match code {
                            0 => two!(Sgr::Reset),
                            4 => two!(Sgr::VerticalAlign(VerticalAlign::SuperScript)),
                            5 => two!(Sgr::VerticalAlign(VerticalAlign::SubScript)),
                            6 => two!(Sgr::Overline(true)),
                            24 => two!(Sgr::VerticalAlign(VerticalAlign::BaseLine)),
                            26 => two!(Sgr::Overline(false)),
                            _ => Err(()),
                        },
                    },
                    _ => Err(()),
                },
                */
                CsiParam::P(_) => Err(()),
                CsiParam::Integer(i) => match FromPrimitive::from_i64(*i) {
                    None => Err(()),
                    Some(sgr) => match sgr {
                        SgrCode::Reset => one!(Sgr::Reset),
                        SgrCode::IntensityBold => one!(Sgr::Intensity(Intensity::Bold)),
                        SgrCode::IntensityDim => one!(Sgr::Intensity(Intensity::Half)),
                        SgrCode::NormalIntensity => one!(Sgr::Intensity(Intensity::Normal)),
                        SgrCode::UnderlineOn => {
                            self.underline(params) //.map(Sgr::Underline)
                        },
                        SgrCode::UnderlineDouble => one!(Sgr::Underline(Underline::Double)),
                        SgrCode::UnderlineOff => one!(Sgr::Underline(Underline::None)),
                        SgrCode::UnderlineColor => {
                            self.parse_sgr_color(params).map(Sgr::UnderlineColor)
                        },
                        SgrCode::ResetUnderlineColor => {
                            one!(Sgr::UnderlineColor(ColorSpec::default()))
                        },
                        SgrCode::BlinkOn => one!(Sgr::Blink(Blink::Slow)),
                        SgrCode::RapidBlinkOn => one!(Sgr::Blink(Blink::Rapid)),
                        SgrCode::BlinkOff => one!(Sgr::Blink(Blink::None)),
                        SgrCode::ItalicOn => one!(Sgr::Italic(true)),
                        SgrCode::ItalicOff => one!(Sgr::Italic(false)),
                        SgrCode::VerticalAlignSuperScript => {
                            one!(Sgr::VerticalAlign(VerticalAlign::SuperScript))
                        },
                        SgrCode::VerticalAlignSubScript => {
                            one!(Sgr::VerticalAlign(VerticalAlign::SubScript))
                        },
                        SgrCode::VerticalAlignBaseLine => {
                            one!(Sgr::VerticalAlign(VerticalAlign::BaseLine))
                        },
                        SgrCode::ForegroundColor => {
                            self.parse_sgr_color(params).map(Sgr::Foreground)
                        },
                        SgrCode::ForegroundBlack => one!(Sgr::Foreground(AnsiColor::Black.into())),
                        SgrCode::ForegroundRed => one!(Sgr::Foreground(AnsiColor::Maroon.into())),
                        SgrCode::ForegroundGreen => one!(Sgr::Foreground(AnsiColor::Green.into())),
                        SgrCode::ForegroundYellow => one!(Sgr::Foreground(AnsiColor::Olive.into())),
                        SgrCode::ForegroundBlue => one!(Sgr::Foreground(AnsiColor::Navy.into())),
                        SgrCode::ForegroundMagenta => {
                            one!(Sgr::Foreground(AnsiColor::Purple.into()))
                        },
                        SgrCode::ForegroundCyan => one!(Sgr::Foreground(AnsiColor::Teal.into())),
                        SgrCode::ForegroundWhite => one!(Sgr::Foreground(AnsiColor::Silver.into())),
                        SgrCode::ForegroundDefault => one!(Sgr::Foreground(ColorSpec::Default)),
                        SgrCode::ForegroundBrightBlack => {
                            one!(Sgr::Foreground(AnsiColor::Grey.into()))
                        },
                        SgrCode::ForegroundBrightRed => {
                            one!(Sgr::Foreground(AnsiColor::Red.into()))
                        },
                        SgrCode::ForegroundBrightGreen => {
                            one!(Sgr::Foreground(AnsiColor::Lime.into()))
                        },
                        SgrCode::ForegroundBrightYellow => {
                            one!(Sgr::Foreground(AnsiColor::Yellow.into()))
                        },
                        SgrCode::ForegroundBrightBlue => {
                            one!(Sgr::Foreground(AnsiColor::Blue.into()))
                        },
                        SgrCode::ForegroundBrightMagenta => {
                            one!(Sgr::Foreground(AnsiColor::Fuchsia.into()))
                        },
                        SgrCode::ForegroundBrightCyan => {
                            one!(Sgr::Foreground(AnsiColor::Aqua.into()))
                        },
                        SgrCode::ForegroundBrightWhite => {
                            one!(Sgr::Foreground(AnsiColor::White.into()))
                        },

                        SgrCode::BackgroundColor => {
                            self.parse_sgr_color(params).map(Sgr::Background)
                        },
                        SgrCode::BackgroundBlack => one!(Sgr::Background(AnsiColor::Black.into())),
                        SgrCode::BackgroundRed => one!(Sgr::Background(AnsiColor::Maroon.into())),
                        SgrCode::BackgroundGreen => one!(Sgr::Background(AnsiColor::Green.into())),
                        SgrCode::BackgroundYellow => one!(Sgr::Background(AnsiColor::Olive.into())),
                        SgrCode::BackgroundBlue => one!(Sgr::Background(AnsiColor::Navy.into())),
                        SgrCode::BackgroundMagenta => {
                            one!(Sgr::Background(AnsiColor::Purple.into()))
                        },
                        SgrCode::BackgroundCyan => one!(Sgr::Background(AnsiColor::Teal.into())),
                        SgrCode::BackgroundWhite => one!(Sgr::Background(AnsiColor::Silver.into())),
                        SgrCode::BackgroundDefault => one!(Sgr::Background(ColorSpec::Default)),
                        SgrCode::BackgroundBrightBlack => {
                            one!(Sgr::Background(AnsiColor::Grey.into()))
                        },
                        SgrCode::BackgroundBrightRed => {
                            one!(Sgr::Background(AnsiColor::Red.into()))
                        },
                        SgrCode::BackgroundBrightGreen => {
                            one!(Sgr::Background(AnsiColor::Lime.into()))
                        },
                        SgrCode::BackgroundBrightYellow => {
                            one!(Sgr::Background(AnsiColor::Yellow.into()))
                        },
                        SgrCode::BackgroundBrightBlue => {
                            one!(Sgr::Background(AnsiColor::Blue.into()))
                        },
                        SgrCode::BackgroundBrightMagenta => {
                            one!(Sgr::Background(AnsiColor::Fuchsia.into()))
                        },
                        SgrCode::BackgroundBrightCyan => {
                            one!(Sgr::Background(AnsiColor::Aqua.into()))
                        },
                        SgrCode::BackgroundBrightWhite => {
                            one!(Sgr::Background(AnsiColor::White.into()))
                        },

                        SgrCode::InverseOn => one!(Sgr::Inverse(true)),
                        SgrCode::InverseOff => one!(Sgr::Inverse(false)),
                        SgrCode::InvisibleOn => one!(Sgr::Invisible(true)),
                        SgrCode::InvisibleOff => one!(Sgr::Invisible(false)),
                        SgrCode::StrikeThroughOn => one!(Sgr::StrikeThrough(true)),
                        SgrCode::StrikeThroughOff => one!(Sgr::StrikeThrough(false)),
                        SgrCode::OverlineOn => one!(Sgr::Overline(true)),
                        SgrCode::OverlineOff => one!(Sgr::Overline(false)),
                        SgrCode::DefaultFont => one!(Sgr::Font(Font::Default)),
                        SgrCode::AltFont1 => one!(Sgr::Font(Font::Alternate(1))),
                        SgrCode::AltFont2 => one!(Sgr::Font(Font::Alternate(2))),
                        SgrCode::AltFont3 => one!(Sgr::Font(Font::Alternate(3))),
                        SgrCode::AltFont4 => one!(Sgr::Font(Font::Alternate(4))),
                        SgrCode::AltFont5 => one!(Sgr::Font(Font::Alternate(5))),
                        SgrCode::AltFont6 => one!(Sgr::Font(Font::Alternate(6))),
                        SgrCode::AltFont7 => one!(Sgr::Font(Font::Alternate(7))),
                        SgrCode::AltFont8 => one!(Sgr::Font(Font::Alternate(8))),
                        SgrCode::AltFont9 => one!(Sgr::Font(Font::Alternate(9))),
                    },
                },
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive)]
pub enum SgrCode {
    Reset = 0,
    IntensityBold = 1,
    IntensityDim = 2,
    ItalicOn = 3,
    UnderlineOn = 4,
    /// Blinks < 150 times per minute
    BlinkOn = 5,
    /// Blinks > 150 times per minute
    RapidBlinkOn = 6,
    InverseOn = 7,
    InvisibleOn = 8,
    StrikeThroughOn = 9,
    DefaultFont = 10,
    AltFont1 = 11,
    AltFont2 = 12,
    AltFont3 = 13,
    AltFont4 = 14,
    AltFont5 = 15,
    AltFont6 = 16,
    AltFont7 = 17,
    AltFont8 = 18,
    AltFont9 = 19,
    // Fraktur = 20,
    UnderlineDouble = 21,
    NormalIntensity = 22,
    ItalicOff = 23,
    UnderlineOff = 24,
    BlinkOff = 25,
    InverseOff = 27,
    InvisibleOff = 28,
    StrikeThroughOff = 29,
    ForegroundBlack = 30,
    ForegroundRed = 31,
    ForegroundGreen = 32,
    ForegroundYellow = 33,
    ForegroundBlue = 34,
    ForegroundMagenta = 35,
    ForegroundCyan = 36,
    ForegroundWhite = 37,
    ForegroundDefault = 39,
    BackgroundBlack = 40,
    BackgroundRed = 41,
    BackgroundGreen = 42,
    BackgroundYellow = 43,
    BackgroundBlue = 44,
    BackgroundMagenta = 45,
    BackgroundCyan = 46,
    BackgroundWhite = 47,
    BackgroundDefault = 49,
    OverlineOn = 53,
    OverlineOff = 55,

    UnderlineColor = 58,
    ResetUnderlineColor = 59,

    VerticalAlignSuperScript = 73,
    VerticalAlignSubScript = 74,
    VerticalAlignBaseLine = 75,

    ForegroundBrightBlack = 90,
    ForegroundBrightRed = 91,
    ForegroundBrightGreen = 92,
    ForegroundBrightYellow = 93,
    ForegroundBrightBlue = 94,
    ForegroundBrightMagenta = 95,
    ForegroundBrightCyan = 96,
    ForegroundBrightWhite = 97,

    BackgroundBrightBlack = 100,
    BackgroundBrightRed = 101,
    BackgroundBrightGreen = 102,
    BackgroundBrightYellow = 103,
    BackgroundBrightBlue = 104,
    BackgroundBrightMagenta = 105,
    BackgroundBrightCyan = 106,
    BackgroundBrightWhite = 107,

    /// Maybe followed either either a 256 color palette index or
    /// a sequence describing a true color rgb value
    ForegroundColor = 38,
    BackgroundColor = 48,
}

impl<'a> Iterator for CSIParser<'a> {
    type Item = CSI;

    fn next(&mut self) -> Option<CSI> {
        let params = match self.params.take() {
            None => return None,
            Some(params) => params,
        };

        match self.parse_next(&params) {
            Ok(csi) => Some(csi),
            Err(()) => Some(CSI::Unspecified(Box::new(Unspecified {
                params: params.to_vec(),
                parameters_truncated: self.parameters_truncated,
                control: self.control,
            }))),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Write;

    fn parse(control: char, params: &[i64], expected: &str) -> Vec<CSI> {
        let mut cparams = vec![];
        for &p in params {
            if !cparams.is_empty() {
                cparams.push(CsiParam::P(b';'));
            }
            cparams.push(CsiParam::Integer(p));
        }
        let res = CSI::parse(&cparams, false, control).collect();
        println!("parsed -> {:#?}", res);
        assert_eq!(encode(&res), expected);
        res
    }

    fn encode(seq: &Vec<CSI>) -> String {
        let mut res = Vec::new();
        for s in seq {
            write!(res, "{}", s).unwrap();
        }
        String::from_utf8(res).unwrap()
    }

    #[test]
    fn basic() {
        assert_eq!(parse('m', &[], "\x1b[0m"), vec![CSI::Sgr(Sgr::Reset)]);
        assert_eq!(parse('m', &[0], "\x1b[0m"), vec![CSI::Sgr(Sgr::Reset)]);
        assert_eq!(
            parse('m', &[1], "\x1b[1m"),
            vec![CSI::Sgr(Sgr::Intensity(Intensity::Bold))]
        );
        assert_eq!(
            parse('m', &[1, 3], "\x1b[1m\x1b[3m"),
            vec![
                CSI::Sgr(Sgr::Intensity(Intensity::Bold)),
                CSI::Sgr(Sgr::Italic(true)),
            ]
        );

        // Verify that we propagate Unspecified for codes
        // that we don't recognize.
        assert_eq!(
            parse('m', &[1, 3, 1231231], "\x1b[1m\x1b[3m\x1b[1231231m"),
            vec![
                CSI::Sgr(Sgr::Intensity(Intensity::Bold)),
                CSI::Sgr(Sgr::Italic(true)),
                CSI::Unspecified(Box::new(Unspecified {
                    params: [CsiParam::Integer(1231231)].to_vec(),
                    parameters_truncated: false,
                    control: 'm',
                })),
            ]
        );
        assert_eq!(
            parse('m', &[1, 1231231, 3], "\x1b[1m\x1b[1231231;3m"),
            vec![
                CSI::Sgr(Sgr::Intensity(Intensity::Bold)),
                CSI::Unspecified(Box::new(Unspecified {
                    params: [
                        CsiParam::Integer(1231231),
                        CsiParam::P(b';'),
                        CsiParam::Integer(3)
                    ]
                    .to_vec(),
                    parameters_truncated: false,
                    control: 'm',
                })),
            ]
        );
        assert_eq!(
            parse('m', &[1231231, 3], "\x1b[1231231;3m"),
            vec![CSI::Unspecified(Box::new(Unspecified {
                params: [
                    CsiParam::Integer(1231231),
                    CsiParam::P(b';'),
                    CsiParam::Integer(3)
                ]
                .to_vec(),
                parameters_truncated: false,
                control: 'm',
            }))]
        );
    }

    #[test]
    fn blinks() {
        assert_eq!(
            parse('m', &[5], "\x1b[5m"),
            vec![CSI::Sgr(Sgr::Blink(Blink::Slow))]
        );
        assert_eq!(
            parse('m', &[6], "\x1b[6m"),
            vec![CSI::Sgr(Sgr::Blink(Blink::Rapid))]
        );
        assert_eq!(
            parse('m', &[25], "\x1b[25m"),
            vec![CSI::Sgr(Sgr::Blink(Blink::None))]
        );
    }

    #[test]
    fn underlines() {
        assert_eq!(
            parse('m', &[21], "\x1b[21m"),
            vec![CSI::Sgr(Sgr::Underline(Underline::Double))]
        );
        assert_eq!(
            parse('m', &[4], "\x1b[4m"),
            vec![CSI::Sgr(Sgr::Underline(Underline::Single))]
        );
    }

    #[test]
    fn underline_color() {
        assert_eq!(
            parse('m', &[58, 2], "\x1b[58;2m"),
            vec![CSI::Unspecified(Box::new(Unspecified {
                params: [
                    CsiParam::Integer(58),
                    CsiParam::P(b';'),
                    CsiParam::Integer(2)
                ]
                .to_vec(),
                parameters_truncated: false,
                control: 'm',
            }))]
        );

        assert_eq!(
            parse('m', &[58, 2, 255, 255, 255], "\x1b[58:2::255:255:255m"),
            vec![CSI::Sgr(Sgr::UnderlineColor(ColorSpec::TrueColor(
                (255, 255, 255).into(),
            )))]
        );
        assert_eq!(
            parse('m', &[58, 5, 220, 255, 255], "\x1b[58:5:220m\x1b[255;255m"),
            vec![
                CSI::Sgr(Sgr::UnderlineColor(ColorSpec::PaletteIndex(220))),
                CSI::Unspecified(Box::new(Unspecified {
                    params: [
                        CsiParam::Integer(255),
                        CsiParam::P(b';'),
                        CsiParam::Integer(255)
                    ]
                    .to_vec(),
                    parameters_truncated: false,
                    control: 'm',
                })),
            ]
        );
    }

    #[test]
    fn color() {
        assert_eq!(
            parse('m', &[38, 2], "\x1b[38;2m"),
            vec![CSI::Unspecified(Box::new(Unspecified {
                params: [
                    CsiParam::Integer(38),
                    CsiParam::P(b';'),
                    CsiParam::Integer(2)
                ]
                .to_vec(),
                parameters_truncated: false,
                control: 'm',
            }))]
        );

        assert_eq!(
            parse('m', &[38, 2, 255, 255, 255], "\x1b[38:2::255:255:255m"),
            vec![CSI::Sgr(Sgr::Foreground(ColorSpec::TrueColor(
                (255, 255, 255).into(),
            )))]
        );
        assert_eq!(
            parse('m', &[38, 5, 220, 255, 255], "\x1b[38:5:220m\x1b[255;255m"),
            vec![
                CSI::Sgr(Sgr::Foreground(ColorSpec::PaletteIndex(220))),
                CSI::Unspecified(Box::new(Unspecified {
                    params: [
                        CsiParam::Integer(255),
                        CsiParam::P(b';'),
                        CsiParam::Integer(255)
                    ]
                    .to_vec(),
                    parameters_truncated: false,
                    control: 'm',
                })),
            ]
        );
    }

    #[test]
    fn edit() {
        assert_eq!(
            parse('J', &[], "\x1b[J"),
            vec![CSI::Edit(Edit::EraseInDisplay(
                EraseInDisplay::EraseToEndOfDisplay,
            ))]
        );
        assert_eq!(
            parse('J', &[0], "\x1b[J"),
            vec![CSI::Edit(Edit::EraseInDisplay(
                EraseInDisplay::EraseToEndOfDisplay,
            ))]
        );
        assert_eq!(
            parse('J', &[1], "\x1b[1J"),
            vec![CSI::Edit(Edit::EraseInDisplay(
                EraseInDisplay::EraseToStartOfDisplay,
            ))]
        );
    }

    #[test]
    fn window() {
        assert_eq!(
            parse('t', &[6], "\x1b[6t"),
            vec![CSI::Window(Box::new(Window::LowerWindow))]
        );
        assert_eq!(
            parse('t', &[6, 15, 7], "\x1b[6;15;7t"),
            vec![CSI::Window(Box::new(
                Window::ReportCellSizePixelsResponse {
                    width: Some(7),
                    height: Some(15)
                }
            ))]
        );
    }

    #[test]
    fn cursor() {
        assert_eq!(
            parse('C', &[], "\x1b[C"),
            vec![CSI::Cursor(Cursor::Right(1))]
        );
        // check that 0 is treated as 1
        assert_eq!(
            parse('C', &[0], "\x1b[C"),
            vec![CSI::Cursor(Cursor::Right(1))]
        );
        assert_eq!(
            parse('C', &[1], "\x1b[C"),
            vec![CSI::Cursor(Cursor::Right(1))]
        );
        assert_eq!(
            parse('C', &[4], "\x1b[4C"),
            vec![CSI::Cursor(Cursor::Right(4))]
        );

        // Check that we default the second parameter of two
        // when only one is provided
        assert_eq!(
            parse('H', &[2], "\x1b[2;1H"),
            vec![CSI::Cursor(Cursor::Position {
                line: OneBased::new(2),
                col: OneBased::new(1)
            })]
        );
    }

    #[test]
    fn ansiset() {
        assert_eq!(
            parse('h', &[20], "\x1b[20h"),
            vec![CSI::Mode(Mode::SetMode(TerminalMode::Code(
                TerminalModeCode::AutomaticNewline
            )))]
        );
        assert_eq!(
            parse('l', &[20], "\x1b[20l"),
            vec![CSI::Mode(Mode::ResetMode(TerminalMode::Code(
                TerminalModeCode::AutomaticNewline
            )))]
        );
    }

    #[test]
    fn bidi_modes() {
        assert_eq!(
            parse('h', &[8], "\x1b[8h"),
            vec![CSI::Mode(Mode::SetMode(TerminalMode::Code(
                TerminalModeCode::BiDirectionalSupportMode
            )))]
        );
        assert_eq!(
            parse('l', &[8], "\x1b[8l"),
            vec![CSI::Mode(Mode::ResetMode(TerminalMode::Code(
                TerminalModeCode::BiDirectionalSupportMode
            )))]
        );
    }

    #[test]
    fn mouse() {
        let res: Vec<_> = CSI::parse(
            &[
                CsiParam::P(b'<'),
                CsiParam::Integer(0),
                CsiParam::P(b';'),
                CsiParam::Integer(12),
                CsiParam::P(b';'),
                CsiParam::Integer(300),
            ],
            false,
            'M',
        )
        .collect();
        assert_eq!(encode(&res), "\x1b[<0;12;300M");
        assert_eq!(
            res,
            vec![CSI::Mouse(MouseReport::SGR1006 {
                x: 12,
                y: 300,
                button: MouseButton::Button1Press,
                modifiers: Modifiers::NONE,
            })]
        );
    }

    #[test]
    fn soft_reset() {
        let res: Vec<_> = CSI::parse(&[CsiParam::P(b'!')], false, 'p').collect();
        assert_eq!(encode(&res), "\x1b[!p");
        assert_eq!(res, vec![CSI::Device(Box::new(Device::SoftReset))],);
    }

    #[test]
    fn device_attr() {
        let res: Vec<_> = CSI::parse(
            &[
                CsiParam::P(b'?'),
                CsiParam::Integer(63),
                CsiParam::P(b';'),
                CsiParam::Integer(1),
                CsiParam::P(b';'),
                CsiParam::Integer(2),
                CsiParam::P(b';'),
                CsiParam::Integer(4),
                CsiParam::P(b';'),
                CsiParam::Integer(6),
                CsiParam::P(b';'),
                CsiParam::Integer(9),
                CsiParam::P(b';'),
                CsiParam::Integer(15),
                CsiParam::P(b';'),
                CsiParam::Integer(22),
            ],
            false,
            'c',
        )
        .collect();

        assert_eq!(
            res,
            vec![CSI::Device(Box::new(Device::DeviceAttributes(
                DeviceAttributes::Vt320(DeviceAttributeFlags::new(vec![
                    DeviceAttribute::Code(DeviceAttributeCodes::Columns132),
                    DeviceAttribute::Code(DeviceAttributeCodes::Printer),
                    DeviceAttribute::Code(DeviceAttributeCodes::SixelGraphics),
                    DeviceAttribute::Code(DeviceAttributeCodes::SelectiveErase),
                    DeviceAttribute::Code(DeviceAttributeCodes::NationalReplacementCharsets),
                    DeviceAttribute::Code(DeviceAttributeCodes::TechnicalCharacters),
                    DeviceAttribute::Code(DeviceAttributeCodes::AnsiColor),
                ])),
            )))]
        );
        assert_eq!(encode(&res), "\x1b[?63;1;2;4;6;9;15;22c");
    }
}

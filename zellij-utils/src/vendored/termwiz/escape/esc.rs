use num_derive::*;
use num_traits::{FromPrimitive, ToPrimitive};
use std::fmt::{Display, Error as FmtError, Formatter, Write as FmtWrite};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Esc {
    Unspecified {
        intermediate: Option<u8>,
        /// The final character in the Escape sequence; this typically
        /// defines how to interpret the other parameters.
        control: u8,
    },
    Code(EscCode),
}

macro_rules! esc {
    ($low:expr) => {
        ($low as isize)
    };
    ($high:expr, $low:expr) => {
        ((($high as isize) << 8) | ($low as isize))
    };
}

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive, Copy)]
pub enum EscCode {
    /// RIS - Full Reset
    FullReset = esc!('c'),
    /// IND - Index.  Note that for Vt52 and Windows 10 ANSI consoles,
    /// this is interpreted as CursorUp
    Index = esc!('D'),
    /// NEL - Next Line
    NextLine = esc!('E'),
    /// Move the cursor to the bottom left corner of the screen
    CursorPositionLowerLeft = esc!('F'),
    /// HTS - Horizontal Tab Set
    HorizontalTabSet = esc!('H'),
    /// RI - Reverse Index – Performs the reverse operation of \n, moves cursor up one line,
    /// maintains horizontal position, scrolls buffer if necessary
    ReverseIndex = esc!('M'),
    /// SS2 Single shift of G2 character set affects next character only
    SingleShiftG2 = esc!('N'),
    /// SS3 Single shift of G3 character set affects next character only
    SingleShiftG3 = esc!('O'),
    /// SPA - Start of Guarded Area
    StartOfGuardedArea = esc!('V'),
    /// EPA - End of Guarded Area
    EndOfGuardedArea = esc!('W'),
    /// SOS - Start of String
    StartOfString = esc!('X'),
    /// DECID - Return Terminal ID (obsolete form of CSI c - aka DA)
    ReturnTerminalId = esc!('Z'),
    /// ST - String Terminator
    StringTerminator = esc!('\\'),
    /// PM - Privacy Message
    PrivacyMessage = esc!('^'),
    /// APC - Application Program Command
    ApplicationProgramCommand = esc!('_'),
    /// Used by tmux for setting the window title
    TmuxTitle = esc!('k'),

    /// DECBI - Back Index
    DecBackIndex = esc!('6'),
    /// DECSC - Save cursor position
    DecSaveCursorPosition = esc!('7'),
    /// DECRC - Restore saved cursor position
    DecRestoreCursorPosition = esc!('8'),
    /// DECPAM - Application Keypad
    DecApplicationKeyPad = esc!('='),
    /// DECPNM - Normal Keypad
    DecNormalKeyPad = esc!('>'),

    /// Designate G0 Character Set – DEC Line Drawing
    DecLineDrawingG0 = esc!('(', '0'),
    /// Designate G0 Character Set - UK
    UkCharacterSetG0 = esc!('(', 'A'),
    /// Designate G0 Character Set – US ASCII
    AsciiCharacterSetG0 = esc!('(', 'B'),

    /// Designate G1 Character Set – DEC Line Drawing
    DecLineDrawingG1 = esc!(')', '0'),
    /// Designate G1 Character Set - UK
    UkCharacterSetG1 = esc!(')', 'A'),
    /// Designate G1 Character Set – US ASCII
    AsciiCharacterSetG1 = esc!(')', 'B'),

    /// https://vt100.net/docs/vt510-rm/DECALN.html
    DecScreenAlignmentDisplay = esc!('#', '8'),

    /// DECDHL - DEC double-height line, top half
    DecDoubleHeightTopHalfLine = esc!('#', '3'),
    /// DECDHL - DEC double-height line, bottom half
    DecDoubleHeightBottomHalfLine = esc!('#', '4'),
    /// DECSWL - DEC single-width line
    DecSingleWidthLine = esc!('#', '5'),
    /// DECDWL - DEC double-width line
    DecDoubleWidthLine = esc!('#', '6'),

    /// These are typically sent by the terminal when keys are pressed
    ApplicationModeArrowUpPress = esc!('O', 'A'),
    ApplicationModeArrowDownPress = esc!('O', 'B'),
    ApplicationModeArrowRightPress = esc!('O', 'C'),
    ApplicationModeArrowLeftPress = esc!('O', 'D'),
    ApplicationModeHomePress = esc!('O', 'H'),
    ApplicationModeEndPress = esc!('O', 'F'),
    F1Press = esc!('O', 'P'),
    F2Press = esc!('O', 'Q'),
    F3Press = esc!('O', 'R'),
    F4Press = esc!('O', 'S'),
}

impl Esc {
    pub fn parse(intermediate: Option<u8>, control: u8) -> Self {
        Self::internal_parse(intermediate, control).unwrap_or_else(|_| Esc::Unspecified {
            intermediate,
            control,
        })
    }

    fn internal_parse(intermediate: Option<u8>, control: u8) -> Result<Self, ()> {
        let packed = match intermediate {
            Some(high) => ((u16::from(high)) << 8) | u16::from(control),
            None => u16::from(control),
        };

        let code = FromPrimitive::from_u16(packed).ok_or(())?;

        Ok(Esc::Code(code))
    }
}

impl Display for Esc {
    // TODO: data size optimization opportunity: if we could somehow know that we
    // had a run of CSI instances being encoded in sequence, we could
    // potentially collapse them together.  This is a few bytes difference in
    // practice so it may not be worthwhile with modern networks.
    fn fmt(&self, f: &mut Formatter) -> Result<(), FmtError> {
        f.write_char(0x1b as char)?;
        use self::Esc::*;
        match self {
            Code(code) => {
                let packed = code
                    .to_u16()
                    .expect("num-derive failed to implement ToPrimitive");
                if packed > u16::from(u8::max_value()) {
                    write!(
                        f,
                        "{}{}",
                        (packed >> 8) as u8 as char,
                        (packed & 0xff) as u8 as char
                    )?;
                } else {
                    f.write_char((packed & 0xff) as u8 as char)?;
                }
            },
            Unspecified {
                intermediate,
                control,
            } => {
                if let Some(i) = intermediate {
                    write!(f, "{}{}", *i as char, *control as char)?;
                } else {
                    f.write_char(*control as char)?;
                }
            },
        };
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn encode(osc: &Esc) -> String {
        format!("{}", osc)
    }

    fn parse(esc: &str) -> Esc {
        let result = if esc.len() == 1 {
            Esc::parse(None, esc.as_bytes()[0])
        } else {
            Esc::parse(Some(esc.as_bytes()[0]), esc.as_bytes()[1])
        };

        assert_eq!(encode(&result), format!("\x1b{}", esc));

        result
    }

    #[test]
    fn test() {
        assert_eq!(parse("(0"), Esc::Code(EscCode::DecLineDrawingG0));
        assert_eq!(parse("(B"), Esc::Code(EscCode::AsciiCharacterSetG0));
        assert_eq!(parse(")0"), Esc::Code(EscCode::DecLineDrawingG1));
        assert_eq!(parse(")B"), Esc::Code(EscCode::AsciiCharacterSetG1));
        assert_eq!(parse("#3"), Esc::Code(EscCode::DecDoubleHeightTopHalfLine));
        assert_eq!(
            parse("#4"),
            Esc::Code(EscCode::DecDoubleHeightBottomHalfLine)
        );
        assert_eq!(parse("#5"), Esc::Code(EscCode::DecSingleWidthLine));
        assert_eq!(parse("#6"), Esc::Code(EscCode::DecDoubleWidthLine));
    }
}

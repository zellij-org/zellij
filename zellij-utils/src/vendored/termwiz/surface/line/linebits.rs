use bitflags::bitflags;
#[cfg(feature = "use_serde")]
use serde::{Deserialize, Serialize};

bitflags! {
    #[cfg_attr(feature="use_serde", derive(Serialize, Deserialize))]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub(crate) struct LineBits : u16 {
        const NONE = 0;
        /// The line contains 1+ cells with explicit hyperlinks set
        const HAS_HYPERLINK = 1<<1;
        /// true if we have scanned for implicit hyperlinks
        const SCANNED_IMPLICIT_HYPERLINKS = 1<<2;
        /// true if we found implicit hyperlinks in the last scan
        const HAS_IMPLICIT_HYPERLINKS = 1<<3;

        /// true if this line should be displayed with
        /// in double-width
        const DOUBLE_WIDTH = 1<<4;

        /// true if this line should be displayed
        /// as double-height top-half
        const DOUBLE_HEIGHT_TOP = 1<<5;

        /// true if this line should be displayed
        /// as double-height bottom-half
        const DOUBLE_HEIGHT_BOTTOM = 1<<6;

        const DOUBLE_WIDTH_HEIGHT_MASK =
            Self::DOUBLE_WIDTH.bits() |
            Self::DOUBLE_HEIGHT_TOP.bits() |
            Self::DOUBLE_HEIGHT_BOTTOM.bits();

        /// true if the line should have the bidi algorithm
        /// applied as part of presentation.
        /// This corresponds to the "implicit" bidi modes
        /// described in
        /// <https://terminal-wg.pages.freedesktop.org/bidi/recommendation/basic-modes.html>
        const BIDI_ENABLED = 1<<0;

        /// true if the line base direction is RTL.
        /// When BIDI_ENABLED is also true, this is passed to the bidi algorithm.
        /// When rendering, the line will be rendered from RTL.
        const RTL = 1<<7;

        /// true if the direction for the line should be auto-detected
        /// when BIDI_ENABLED is also true.
        /// If false, the direction is taken from the RTL bit only.
        /// Otherwise, the auto-detect direction is used, falling back
        /// to the direction specified by the RTL bit.
        const AUTO_DETECT_DIRECTION = 1<<8;
    }
}

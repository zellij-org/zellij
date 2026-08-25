//! Unicode-placeholder (U=1) placements for the kitty graphics protocol.
//!
//! Applications place images by printing U+10EEEE cells. The image id
//! lives in the cell's foreground color (24-bit for truecolor SGR,
//! 8-bit for a 256-color index), an optional placement id in the
//! underline color, and up to three combining diacritics per cell
//! encode the image row, image column and the most significant byte of
//! the image id. Missing diacritics are inferred from the cell to the
//! left.
//!
//! The grid drops zero-width codepoints from cells (see
//! grid::add_character), so the diacritics cannot be kept as text.
//! Instead they are folded, at print time, into an interned
//! (row, column, id-msb) entry, and the placeholder cell's `char` is
//! replaced with a Plane-16 PUA codepoint carrying the entry's index.
//! The colors stay on the cell's styles, so a placeholder cell moves
//! through scrollback, reflow and clears like any other styled
//! character. A render-time viewport scan (grid::placeholder_kitty_chunks)
//! resolves the visible cells back into image chunks.

use std::collections::HashMap;

/// The kitty graphics protocol's Unicode placeholder codepoint.
pub const PLACEHOLDER_CHAR: char = '\u{10EEEE}';

/// Base of the encoded range. Plane-16 PUA-B is effectively unused by
/// real-world text (unlike Plane-15, which hosts icon fonts), which
/// keeps the range check below from ever matching application output.
pub const ENCODED_BASE: u32 = 0x100000;

/// Capacity cap keeps every encoded codepoint below U+10EEEE itself
/// (and well clear of the U+10FFFE/U+10FFFF noncharacters).
pub const MAX_ENCODED_ENTRIES: usize = 0xEEE0;

/// True for any codepoint inside the encoded range, whether or not it
/// is currently assigned. Used by the output serializer, which has no
/// access to the per-pane interner, to substitute a space so the host
/// terminal doesn't render an undefined glyph under the image.
pub fn is_in_encoded_range(character: char) -> bool {
    let value = character as u32;
    value >= ENCODED_BASE && value < ENCODED_BASE + MAX_ENCODED_ENTRIES as u32
}

/// What one placeholder cell encodes: the image row/column it shows,
/// and the most significant byte of the image id (the low 24 bits ride
/// in the cell's foreground color).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PlaceholderCell {
    pub image_row: u16,
    pub image_column: u16,
    pub image_id_msb: u8,
}

/// Interns (row, column, id-msb) triples so each placeholder cell can
/// carry a single `char` indexing into it. Entries are deduplicated and
/// live for the pane's lifetime; a 20x10 image visible in full costs
/// 200 entries, so the cap is effectively unreachable outside of
/// adversarial input.
#[derive(Debug, Clone, Default)]
pub struct PlaceholderInterner {
    entries: Vec<PlaceholderCell>,
    indices: HashMap<PlaceholderCell, u32>,
}

impl PlaceholderInterner {
    pub fn encode(&mut self, cell: PlaceholderCell) -> Option<char> {
        if let Some(index) = self.indices.get(&cell) {
            return char::from_u32(ENCODED_BASE + index);
        }
        if self.entries.len() >= MAX_ENCODED_ENTRIES {
            return None;
        }
        let index = self.entries.len() as u32;
        self.entries.push(cell);
        self.indices.insert(cell, index);
        char::from_u32(ENCODED_BASE + index)
    }
    pub fn decode(&self, character: char) -> Option<PlaceholderCell> {
        let index = (character as u32).checked_sub(ENCODED_BASE)? as usize;
        self.entries.get(index).copied()
    }
}

/// The 298-entry row/column diacritic table from kitty's
/// rowcolumn-diacritics.txt: codepoint KITTY_DIACRITICS[i] encodes
/// index i. Ascending, so lookups binary-search.
pub const KITTY_DIACRITICS: [u32; 298] = [
    0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F, 0x0346, 0x034A, 0x034B, 0x034C,
    0x0350, 0x0351, 0x0352, 0x0357, 0x035B, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367, 0x0368, 0x0369,
    0x036A, 0x036B, 0x036C, 0x036D, 0x036E, 0x036F, 0x0483, 0x0484, 0x0485, 0x0486, 0x0487, 0x0592,
    0x0593, 0x0594, 0x0595, 0x0597, 0x0598, 0x0599, 0x059C, 0x059D, 0x059E, 0x059F, 0x05A0, 0x05A1,
    0x05A8, 0x05A9, 0x05AB, 0x05AC, 0x05AF, 0x05C4, 0x0610, 0x0611, 0x0612, 0x0613, 0x0614, 0x0615,
    0x0616, 0x0617, 0x0657, 0x0658, 0x0659, 0x065A, 0x065B, 0x065D, 0x065E, 0x06D6, 0x06D7, 0x06D8,
    0x06D9, 0x06DA, 0x06DB, 0x06DC, 0x06DF, 0x06E0, 0x06E1, 0x06E2, 0x06E4, 0x06E7, 0x06E8, 0x06EB,
    0x06EC, 0x0730, 0x0732, 0x0733, 0x0735, 0x0736, 0x073A, 0x073D, 0x073F, 0x0740, 0x0741, 0x0743,
    0x0745, 0x0747, 0x0749, 0x074A, 0x07EB, 0x07EC, 0x07ED, 0x07EE, 0x07EF, 0x07F0, 0x07F1, 0x07F3,
    0x0816, 0x0817, 0x0818, 0x0819, 0x081B, 0x081C, 0x081D, 0x081E, 0x081F, 0x0820, 0x0821, 0x0822,
    0x0823, 0x0825, 0x0826, 0x0827, 0x0829, 0x082A, 0x082B, 0x082C, 0x082D, 0x0951, 0x0953, 0x0954,
    0x0F82, 0x0F83, 0x0F86, 0x0F87, 0x135D, 0x135E, 0x135F, 0x17DD, 0x193A, 0x1A17, 0x1A75, 0x1A76,
    0x1A77, 0x1A78, 0x1A79, 0x1A7A, 0x1A7B, 0x1A7C, 0x1B6B, 0x1B6D, 0x1B6E, 0x1B6F, 0x1B70, 0x1B71,
    0x1B72, 0x1B73, 0x1CD0, 0x1CD1, 0x1CD2, 0x1CDA, 0x1CDB, 0x1CE0, 0x1DC0, 0x1DC1, 0x1DC3, 0x1DC4,
    0x1DC5, 0x1DC6, 0x1DC7, 0x1DC8, 0x1DC9, 0x1DCB, 0x1DCC, 0x1DD1, 0x1DD2, 0x1DD3, 0x1DD4, 0x1DD5,
    0x1DD6, 0x1DD7, 0x1DD8, 0x1DD9, 0x1DDA, 0x1DDB, 0x1DDC, 0x1DDD, 0x1DDE, 0x1DDF, 0x1DE0, 0x1DE1,
    0x1DE2, 0x1DE3, 0x1DE4, 0x1DE5, 0x1DE6, 0x1DFE, 0x20D0, 0x20D1, 0x20D4, 0x20D5, 0x20D6, 0x20D7,
    0x20DB, 0x20DC, 0x20E1, 0x20E7, 0x20E9, 0x20F0, 0x2CEF, 0x2CF0, 0x2CF1, 0x2DE0, 0x2DE1, 0x2DE2,
    0x2DE3, 0x2DE4, 0x2DE5, 0x2DE6, 0x2DE7, 0x2DE8, 0x2DE9, 0x2DEA, 0x2DEB, 0x2DEC, 0x2DED, 0x2DEE,
    0x2DEF, 0x2DF0, 0x2DF1, 0x2DF2, 0x2DF3, 0x2DF4, 0x2DF5, 0x2DF6, 0x2DF7, 0x2DF8, 0x2DF9, 0x2DFA,
    0x2DFB, 0x2DFC, 0x2DFD, 0x2DFE, 0x2DFF, 0xA66F, 0xA67C, 0xA67D, 0xA6F0, 0xA6F1, 0xA8E0, 0xA8E1,
    0xA8E2, 0xA8E3, 0xA8E4, 0xA8E5, 0xA8E6, 0xA8E7, 0xA8E8, 0xA8E9, 0xA8EA, 0xA8EB, 0xA8EC, 0xA8ED,
    0xA8EE, 0xA8EF, 0xA8F0, 0xA8F1, 0xAAB0, 0xAAB2, 0xAAB3, 0xAAB7, 0xAAB8, 0xAABE, 0xAABF, 0xAAC1,
    0xFB1E, 0xFE20, 0xFE21, 0xFE22, 0xFE23, 0xFE24, 0xFE25, 0xFE26, 0x10A0F, 0x10A38, 0x1D185,
    0x1D186, 0x1D187, 0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC, 0x1D1AD, 0x1D242, 0x1D243,
    0x1D244,
];

/// The row/column/id-msb value a kitty diacritic encodes, if any.
pub fn diacritic_index(character: char) -> Option<u32> {
    KITTY_DIACRITICS
        .binary_search(&(character as u32))
        .ok()
        .map(|index| index as u32)
}

#[cfg(test)]
#[path = "./unit/placeholders_tests.rs"]
mod placeholders_tests;

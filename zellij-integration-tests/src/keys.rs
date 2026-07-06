const ESC_BYTE: u8 = 0x1b;

pub const fn ctrl(character: char) -> [u8; 1] {
    [(character as u8) & 0x1f]
}

pub const fn alt(character: char) -> [u8; 2] {
    [ESC_BYTE, character as u8]
}

pub const fn key(character: char) -> [u8; 1] {
    [character as u8]
}

pub const ENTER: [u8; 1] = [b'\r'];
pub const ESC: [u8; 1] = [ESC_BYTE];
pub const SPACE: [u8; 1] = [b' '];
pub const TAB: [u8; 1] = [b'\t'];

pub const BRACKETED_PASTE_START: [u8; 6] = *b"\x1b[200~";
pub const BRACKETED_PASTE_END: [u8; 6] = *b"\x1b[201~";

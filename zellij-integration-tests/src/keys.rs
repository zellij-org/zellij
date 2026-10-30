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
pub const ALT_ENTER: [u8; 2] = [ESC_BYTE, b'\r'];
pub const ALT_ENTER_KITTY: [u8; 7] = *b"\x1b[13;3u";
pub const ESC: [u8; 1] = [ESC_BYTE];
pub const ESC_KEY: [u8; 5] = *b"\x1b[27u";
pub const SPACE: [u8; 1] = [b' '];
pub const TAB: [u8; 1] = [b'\t'];
pub const F8: [u8; 5] = *b"\x1b[19~";
pub const F9: [u8; 5] = *b"\x1b[20~";
pub const UP: [u8; 3] = *b"\x1b[A";
pub const DOWN: [u8; 3] = *b"\x1b[B";
pub const RIGHT: [u8; 3] = *b"\x1b[C";

pub const BRACKETED_PASTE_START: [u8; 6] = *b"\x1b[200~";
pub const BRACKETED_PASTE_END: [u8; 6] = *b"\x1b[201~";
pub const F1: [u8; 3] = *b"\x1bOP";
pub const SHIFT_UP: [u8; 6] = *b"\x1b[1;2A";
pub const SHIFT_DOWN: [u8; 6] = *b"\x1b[1;2B";
pub const SHIFT_HOME: [u8; 6] = *b"\x1b[1;2H";
pub const SHIFT_END: [u8; 6] = *b"\x1b[1;2F";
pub const PAGE_UP: [u8; 4] = *b"\x1b[5~";
pub const PAGE_DOWN: [u8; 4] = *b"\x1b[6~";
pub const DELETE: [u8; 4] = *b"\x1b[3~";

pub fn sgr_click(column: usize, row: usize) -> Vec<u8> {
    format!(
        "\u{1b}[<0;{};{}M\u{1b}[<0;{};{}m",
        column + 1,
        row + 1,
        column + 1,
        row + 1
    )
    .into_bytes()
}

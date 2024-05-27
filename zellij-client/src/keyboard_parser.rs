// for more info, please see: https://sw.kovidgoyal.net/kitty/keyboard-protocol
use zellij_utils::data::KeyWithModifier;

#[derive(Debug)]
enum KittyKeysParsingState {
    Ground,
    ReceivedEscapeCharacter,
    ParsingNumber,
    ParsingModifiers,
    DoneParsingWithU,
    DoneParsingWithTilde,
}

#[derive(Debug)]
pub struct KittyKeyboardParser {
    state: KittyKeysParsingState,
    number_bytes: Vec<u8>,
    modifier_bytes: Vec<u8>,
}

impl KittyKeyboardParser {
    pub fn new() -> Self {
        KittyKeyboardParser {
            state: KittyKeysParsingState::Ground,
            number_bytes: vec![],
            modifier_bytes: vec![],
        }
    }
    pub fn parse(&mut self, buffer: &[u8]) -> Option<KeyWithModifier> {
        for byte in buffer {
            if !self.advance(*byte) {
                return None;
            }
        }
        match self.state {
            KittyKeysParsingState::DoneParsingWithU => {
                // CSI number ; modifiers u
                KeyWithModifier::from_bytes_with_u(&self.number_bytes, &self.modifier_bytes)
            },
            KittyKeysParsingState::DoneParsingWithTilde => {
                // CSI number ; modifiers ~
                KeyWithModifier::from_bytes_with_tilde(&self.number_bytes, &self.modifier_bytes)
            },
            KittyKeysParsingState::ParsingModifiers => {
                // CSI 1; modifiers [ABCDEFHPQS]
                match self.modifier_bytes.pop() {
                    Some(last_modifier) => KeyWithModifier::from_bytes_with_no_ending_byte(
                        &[last_modifier],
                        &self.modifier_bytes,
                    ),
                    None => None,
                }
            },
            KittyKeysParsingState::ParsingNumber => {
                KeyWithModifier::from_bytes_with_no_ending_byte(
                    &self.number_bytes,
                    &self.modifier_bytes,
                )
            },
            _ => None,
        }
    }
    pub fn advance(&mut self, byte: u8) -> bool {
        // returns false if we failed parsing
        match (&self.state, byte) {
            (KittyKeysParsingState::Ground, 0x1b | 0x5b) => {
                self.state = KittyKeysParsingState::ReceivedEscapeCharacter;
            },
            (KittyKeysParsingState::ReceivedEscapeCharacter, 91) => {
                self.state = KittyKeysParsingState::ParsingNumber;
            },
            (KittyKeysParsingState::ParsingNumber, 59) => {
                // semicolon
                if &self.number_bytes == &[49] {
                    self.number_bytes.clear();
                }
                self.state = KittyKeysParsingState::ParsingModifiers;
            },
            (
                KittyKeysParsingState::ParsingNumber | KittyKeysParsingState::ParsingModifiers,
                117,
            ) => {
                // u
                self.state = KittyKeysParsingState::DoneParsingWithU;
            },
            (
                KittyKeysParsingState::ParsingNumber | KittyKeysParsingState::ParsingModifiers,
                126,
            ) => {
                // ~
                self.state = KittyKeysParsingState::DoneParsingWithTilde;
            },
            (KittyKeysParsingState::ParsingNumber, _) => {
                self.number_bytes.push(byte);
            },
            (KittyKeysParsingState::ParsingModifiers, _) => {
                self.modifier_bytes.push(byte);
            },
            (_, _) => {
                return false;
            },
        }
        true
    }
}

#[test]
pub fn can_parse_bare_keys() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('a'))),
        "Can parse a bare 'a' keypress"
    );
    let key = "\u{1b}[49u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('1'))),
        "Can parse a bare '1' keypress"
    );
    let key = "\u{1b}[27u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Esc)),
        "Can parse a bare 'ESC' keypress"
    );
    let key = "\u{1b}[13u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Enter)),
        "Can parse a bare 'ENTER' keypress"
    );
    let key = "\u{1b}[9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Tab)),
        "Can parse a bare 'Tab' keypress"
    );
    let key = "\u{1b}[127u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Backspace)),
        "Can parse a bare 'Backspace' keypress"
    );
    let key = "\u{1b}[57358u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::CapsLock)),
        "Can parse a bare 'CapsLock' keypress"
    );
    let key = "\u{1b}[57359u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::ScrollLock)),
        "Can parse a bare 'ScrollLock' keypress"
    );
    let key = "\u{1b}[57360u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::NumLock)),
        "Can parse a bare 'NumLock' keypress"
    );
    let key = "\u{1b}[57361u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PrintScreen)),
        "Can parse a bare 'PrintScreen' keypress"
    );
    let key = "\u{1b}[57362u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Pause)),
        "Can parse a bare 'Pause' keypress"
    );
    let key = "\u{1b}[57363u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Menu)),
        "Can parse a bare 'Menu' keypress"
    );

    let key = "\u{1b}[2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Insert)),
        "Can parse a bare 'Insert' keypress"
    );
    let key = "\u{1b}[3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Delete)),
        "Can parse a bare 'Delete' keypress"
    );
    let key = "\u{1b}[5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageUp)),
        "Can parse a bare 'PageUp' keypress"
    );
    let key = "\u{1b}[6~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageDown)),
        "Can parse a bare 'PageDown' keypress"
    );
    let key = "\u{1b}[7~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home)),
        "Can parse a bare 'Home' keypress"
    );
    let key = "\u{1b}[8~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End)),
        "Can parse a bare 'End' keypress"
    );
    let key = "\u{1b}[11~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1))),
        "Can parse a bare 'F1' keypress"
    );
    let key = "\u{1b}[12~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2))),
        "Can parse a bare 'F2' keypress"
    );
    let key = "\u{1b}[13~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(3))),
        "Can parse a bare 'F3' keypress"
    );
    let key = "\u{1b}[14~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4))),
        "Can parse a bare 'F4' keypress"
    );
    let key = "\u{1b}[15~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(5))),
        "Can parse a bare 'F5' keypress"
    );
    let key = "\u{1b}[17~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(6))),
        "Can parse a bare 'F6' keypress"
    );
    let key = "\u{1b}[18~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(7))),
        "Can parse a bare 'F7' keypress"
    );
    let key = "\u{1b}[19~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(8))),
        "Can parse a bare 'F8' keypress"
    );
    let key = "\u{1b}[20~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(9))),
        "Can parse a bare 'F9' keypress"
    );
    let key = "\u{1b}[21~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(10))),
        "Can parse a bare 'F10' keypress"
    );
    let key = "\u{1b}[23~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(11))),
        "Can parse a bare 'F11' keypress"
    );
    let key = "\u{1b}[24~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(12))),
        "Can parse a bare 'F12' keypress"
    );
    let key = "\u{1b}[D";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Left)),
        "Can parse a bare 'Left' keypress"
    );
    let key = "\u{1b}[C";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Right)),
        "Can parse a bare 'Right' keypress"
    );
    let key = "\u{1b}[A";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Up)),
        "Can parse a bare 'Up' keypress"
    );
    let key = "\u{1b}[B";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Down)),
        "Can parse a bare 'Down' keypress"
    );
    let key = "\u{1b}[H";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home)),
        "Can parse a bare 'Home' keypress"
    );
    let key = "\u{1b}[F";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End)),
        "Can parse a bare 'End' keypress"
    );
    let key = "\u{1b}[P";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1))),
        "Can parse a bare 'F1 (alternate)' keypress"
    );
    let key = "\u{1b}[Q";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2))),
        "Can parse a bare 'F2 (alternate)' keypress"
    );
    let key = "\u{1b}[S";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4))),
        "Can parse a bare 'F4 (alternate)' keypress"
    );
}

#[test]
pub fn can_parse_keys_with_shift_modifier() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('a')).with_shift_modifier()),
        "Can parse a bare 'a' keypress with shift"
    );
    let key = "\u{1b}[49;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('1')).with_shift_modifier()),
        "Can parse a bare '1' keypress with shift"
    );
    let key = "\u{1b}[27;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Esc).with_shift_modifier()),
        "Can parse a bare 'ESC' keypress with shift"
    );
    let key = "\u{1b}[13;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Enter).with_shift_modifier()),
        "Can parse a bare 'ENTER' keypress with shift"
    );
    let key = "\u{1b}[9;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Tab).with_shift_modifier()),
        "Can parse a bare 'Tab' keypress with shift"
    );
    let key = "\u{1b}[127;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Backspace).with_shift_modifier()),
        "Can parse a bare 'Backspace' keypress with shift"
    );
    let key = "\u{1b}[57358;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::CapsLock).with_shift_modifier()),
        "Can parse a bare 'CapsLock' keypress with shift"
    );
    let key = "\u{1b}[57359;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::ScrollLock).with_shift_modifier()),
        "Can parse a bare 'ScrollLock' keypress with shift"
    );
    let key = "\u{1b}[57360;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::NumLock).with_shift_modifier()),
        "Can parse a bare 'NumLock' keypress with shift"
    );
    let key = "\u{1b}[57361;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PrintScreen).with_shift_modifier()),
        "Can parse a bare 'PrintScreen' keypress with shift"
    );
    let key = "\u{1b}[57362;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Pause).with_shift_modifier()),
        "Can parse a bare 'Pause' keypress with shift"
    );
    let key = "\u{1b}[57363;2u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Menu).with_shift_modifier()),
        "Can parse a bare 'Menu' keypress with shift"
    );

    let key = "\u{1b}[2;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Insert).with_shift_modifier()),
        "Can parse a bare 'Insert' keypress with shift"
    );
    let key = "\u{1b}[3;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Delete).with_shift_modifier()),
        "Can parse a bare 'Delete' keypress with shift"
    );
    let key = "\u{1b}[5;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageUp).with_shift_modifier()),
        "Can parse a bare 'PageUp' keypress with shift"
    );
    let key = "\u{1b}[6;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageDown).with_shift_modifier()),
        "Can parse a bare 'PageDown' keypress with shift"
    );
    let key = "\u{1b}[7;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_shift_modifier()),
        "Can parse a bare 'Home' keypress with shift"
    );
    let key = "\u{1b}[8;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_shift_modifier()),
        "Can parse a bare 'End' keypress with shift"
    );
    let key = "\u{1b}[11;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_shift_modifier()),
        "Can parse a bare 'F1' keypress with shift"
    );
    let key = "\u{1b}[12;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_shift_modifier()),
        "Can parse a bare 'F2' keypress with shift"
    );
    let key = "\u{1b}[13;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(3)).with_shift_modifier()),
        "Can parse a bare 'F3' keypress with shift"
    );
    let key = "\u{1b}[14;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_shift_modifier()),
        "Can parse a bare 'F4' keypress with shift"
    );
    let key = "\u{1b}[15;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(5)).with_shift_modifier()),
        "Can parse a bare 'F5' keypress with shift"
    );
    let key = "\u{1b}[17;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(6)).with_shift_modifier()),
        "Can parse a bare 'F6' keypress with shift"
    );
    let key = "\u{1b}[18;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(7)).with_shift_modifier()),
        "Can parse a bare 'F7' keypress with shift"
    );
    let key = "\u{1b}[19;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(8)).with_shift_modifier()),
        "Can parse a bare 'F8' keypress with shift"
    );
    let key = "\u{1b}[20;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(9)).with_shift_modifier()),
        "Can parse a bare 'F9' keypress with shift"
    );
    let key = "\u{1b}[21;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(10)).with_shift_modifier()),
        "Can parse a bare 'F10' keypress with shift"
    );
    let key = "\u{1b}[23;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(11)).with_shift_modifier()),
        "Can parse a bare 'F11' keypress with shift"
    );
    let key = "\u{1b}[24;2~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(12)).with_shift_modifier()),
        "Can parse a bare 'F12' keypress with shift"
    );
    let key = "\u{1b}[1;2D";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Left).with_shift_modifier()),
        "Can parse a bare 'Left' keypress with shift"
    );
    let key = "\u{1b}[1;2C";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Right).with_shift_modifier()),
        "Can parse a bare 'Right' keypress with shift"
    );
    let key = "\u{1b}[1;2A";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Up).with_shift_modifier()),
        "Can parse a bare 'Up' keypress with shift"
    );
    let key = "\u{1b}[1;2B";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Down).with_shift_modifier()),
        "Can parse a bare 'Down' keypress with shift"
    );
    let key = "\u{1b}[1;2H";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_shift_modifier()),
        "Can parse a bare 'Home' keypress with shift"
    );
    let key = "\u{1b}[1;2F";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_shift_modifier()),
        "Can parse a bare 'End' keypress with shift"
    );
    let key = "\u{1b}[1;2P";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_shift_modifier()),
        "Can parse a bare 'F1 (alternate)' keypress with shift"
    );
    let key = "\u{1b}[1;2Q";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_shift_modifier()),
        "Can parse a bare 'F2 (alternate)' keypress with shift"
    );
    let key = "\u{1b}[1;2S";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_shift_modifier()),
        "Can parse a bare 'F4 (alternate)' keypress with shift"
    );
}

#[test]
pub fn can_parse_keys_with_alt_modifier() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('a')).with_alt_modifier()),
        "Can parse a bare 'a' keypress with alt"
    );
    let key = "\u{1b}[49;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('1')).with_alt_modifier()),
        "Can parse a bare '1' keypress with alt"
    );
    let key = "\u{1b}[27;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Esc).with_alt_modifier()),
        "Can parse a bare 'ESC' keypress with alt"
    );
    let key = "\u{1b}[13;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Enter).with_alt_modifier()),
        "Can parse a bare 'ENTER' keypress with alt"
    );
    let key = "\u{1b}[9;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Tab).with_alt_modifier()),
        "Can parse a bare 'Tab' keypress with alt"
    );
    let key = "\u{1b}[127;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Backspace).with_alt_modifier()),
        "Can parse a bare 'Backspace' keypress with alt"
    );
    let key = "\u{1b}[57358;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::CapsLock).with_alt_modifier()),
        "Can parse a bare 'CapsLock' keypress with alt"
    );
    let key = "\u{1b}[57359;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::ScrollLock).with_alt_modifier()),
        "Can parse a bare 'ScrollLock' keypress with alt"
    );
    let key = "\u{1b}[57360;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::NumLock).with_alt_modifier()),
        "Can parse a bare 'NumLock' keypress with alt"
    );
    let key = "\u{1b}[57361;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PrintScreen).with_alt_modifier()),
        "Can parse a bare 'PrintScreen' keypress with alt"
    );
    let key = "\u{1b}[57362;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Pause).with_alt_modifier()),
        "Can parse a bare 'Pause' keypress with alt"
    );
    let key = "\u{1b}[57363;3u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Menu).with_alt_modifier()),
        "Can parse a bare 'Menu' keypress with alt"
    );

    let key = "\u{1b}[2;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Insert).with_alt_modifier()),
        "Can parse a bare 'Insert' keypress with alt"
    );
    let key = "\u{1b}[3;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Delete).with_alt_modifier()),
        "Can parse a bare 'Delete' keypress with alt"
    );
    let key = "\u{1b}[5;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageUp).with_alt_modifier()),
        "Can parse a bare 'PageUp' keypress with alt"
    );
    let key = "\u{1b}[6;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageDown).with_alt_modifier()),
        "Can parse a bare 'PageDown' keypress with alt"
    );
    let key = "\u{1b}[7;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_alt_modifier()),
        "Can parse a bare 'Home' keypress with alt"
    );
    let key = "\u{1b}[8;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_alt_modifier()),
        "Can parse a bare 'End' keypress with alt"
    );
    let key = "\u{1b}[11;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_alt_modifier()),
        "Can parse a bare 'F1' keypress with alt"
    );
    let key = "\u{1b}[12;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_alt_modifier()),
        "Can parse a bare 'F2' keypress with alt"
    );
    let key = "\u{1b}[13;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(3)).with_alt_modifier()),
        "Can parse a bare 'F3' keypress with alt"
    );
    let key = "\u{1b}[14;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_alt_modifier()),
        "Can parse a bare 'F4' keypress with alt"
    );
    let key = "\u{1b}[15;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(5)).with_alt_modifier()),
        "Can parse a bare 'F5' keypress with alt"
    );
    let key = "\u{1b}[17;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(6)).with_alt_modifier()),
        "Can parse a bare 'F6' keypress with alt"
    );
    let key = "\u{1b}[18;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(7)).with_alt_modifier()),
        "Can parse a bare 'F7' keypress with alt"
    );
    let key = "\u{1b}[19;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(8)).with_alt_modifier()),
        "Can parse a bare 'F8' keypress with alt"
    );
    let key = "\u{1b}[20;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(9)).with_alt_modifier()),
        "Can parse a bare 'F9' keypress with alt"
    );
    let key = "\u{1b}[21;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(10)).with_alt_modifier()),
        "Can parse a bare 'F10' keypress with alt"
    );
    let key = "\u{1b}[23;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(11)).with_alt_modifier()),
        "Can parse a bare 'F11' keypress with alt"
    );
    let key = "\u{1b}[24;3~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(12)).with_alt_modifier()),
        "Can parse a bare 'F12' keypress with alt"
    );
    let key = "\u{1b}[1;3D";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Left).with_alt_modifier()),
        "Can parse a bare 'Left' keypress with alt"
    );
    let key = "\u{1b}[1;3C";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Right).with_alt_modifier()),
        "Can parse a bare 'Right' keypress with alt"
    );
    let key = "\u{1b}[1;3A";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Up).with_alt_modifier()),
        "Can parse a bare 'Up' keypress with alt"
    );
    let key = "\u{1b}[1;3B";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Down).with_alt_modifier()),
        "Can parse a bare 'Down' keypress with alt"
    );
    let key = "\u{1b}[1;3H";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_alt_modifier()),
        "Can parse a bare 'Home' keypress with alt"
    );
    let key = "\u{1b}[1;3F";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_alt_modifier()),
        "Can parse a bare 'End' keypress with alt"
    );
    let key = "\u{1b}[1;3P";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_alt_modifier()),
        "Can parse a bare 'F1 (alternate)' keypress with alt"
    );
    let key = "\u{1b}[1;3Q";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_alt_modifier()),
        "Can parse a bare 'F2 (alternate)' keypress with alt"
    );
    let key = "\u{1b}[1;3S";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_alt_modifier()),
        "Can parse a bare 'F4 (alternate)' keypress with alt"
    );
}

#[test]
pub fn can_parse_keys_with_ctrl_modifier() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('a')).with_ctrl_modifier()),
        "Can parse a bare 'a' keypress with ctrl"
    );
    let key = "\u{1b}[49;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('1')).with_ctrl_modifier()),
        "Can parse a bare '1' keypress with ctrl"
    );
    let key = "\u{1b}[27;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Esc).with_ctrl_modifier()),
        "Can parse a bare 'ESC' keypress with ctrl"
    );
    let key = "\u{1b}[13;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Enter).with_ctrl_modifier()),
        "Can parse a bare 'ENTER' keypress with ctrl"
    );
    let key = "\u{1b}[9;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Tab).with_ctrl_modifier()),
        "Can parse a bare 'Tab' keypress with ctrl"
    );
    let key = "\u{1b}[127;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Backspace).with_ctrl_modifier()),
        "Can parse a bare 'Backspace' keypress with ctrl"
    );
    let key = "\u{1b}[57358;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::CapsLock).with_ctrl_modifier()),
        "Can parse a bare 'CapsLock' keypress with ctrl"
    );
    let key = "\u{1b}[57359;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::ScrollLock).with_ctrl_modifier()),
        "Can parse a bare 'ScrollLock' keypress with ctrl"
    );
    let key = "\u{1b}[57360;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::NumLock).with_ctrl_modifier()),
        "Can parse a bare 'NumLock' keypress with ctrl"
    );
    let key = "\u{1b}[57361;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PrintScreen).with_ctrl_modifier()),
        "Can parse a bare 'PrintScreen' keypress with ctrl"
    );
    let key = "\u{1b}[57362;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Pause).with_ctrl_modifier()),
        "Can parse a bare 'Pause' keypress with ctrl"
    );
    let key = "\u{1b}[57363;5u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Menu).with_ctrl_modifier()),
        "Can parse a bare 'Menu' keypress with ctrl"
    );

    let key = "\u{1b}[2;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Insert).with_ctrl_modifier()),
        "Can parse a bare 'Insert' keypress with ctrl"
    );
    let key = "\u{1b}[3;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Delete).with_ctrl_modifier()),
        "Can parse a bare 'Delete' keypress with ctrl"
    );
    let key = "\u{1b}[5;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageUp).with_ctrl_modifier()),
        "Can parse a bare 'PageUp' keypress with ctrl"
    );
    let key = "\u{1b}[6;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageDown).with_ctrl_modifier()),
        "Can parse a bare 'PageDown' keypress with ctrl"
    );
    let key = "\u{1b}[7;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_ctrl_modifier()),
        "Can parse a bare 'Home' keypress with ctrl"
    );
    let key = "\u{1b}[8;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_ctrl_modifier()),
        "Can parse a bare 'End' keypress with ctrl"
    );
    let key = "\u{1b}[11;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_ctrl_modifier()),
        "Can parse a bare 'F1' keypress with ctrl"
    );
    let key = "\u{1b}[12;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_ctrl_modifier()),
        "Can parse a bare 'F2' keypress with ctrl"
    );
    let key = "\u{1b}[13;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(3)).with_ctrl_modifier()),
        "Can parse a bare 'F3' keypress with ctrl"
    );
    let key = "\u{1b}[14;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_ctrl_modifier()),
        "Can parse a bare 'F4' keypress with ctrl"
    );
    let key = "\u{1b}[15;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(5)).with_ctrl_modifier()),
        "Can parse a bare 'F5' keypress with ctrl"
    );
    let key = "\u{1b}[17;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(6)).with_ctrl_modifier()),
        "Can parse a bare 'F6' keypress with ctrl"
    );
    let key = "\u{1b}[18;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(7)).with_ctrl_modifier()),
        "Can parse a bare 'F7' keypress with ctrl"
    );
    let key = "\u{1b}[19;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(8)).with_ctrl_modifier()),
        "Can parse a bare 'F8' keypress with ctrl"
    );
    let key = "\u{1b}[20;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(9)).with_ctrl_modifier()),
        "Can parse a bare 'F9' keypress with ctrl"
    );
    let key = "\u{1b}[21;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(10)).with_ctrl_modifier()),
        "Can parse a bare 'F10' keypress with ctrl"
    );
    let key = "\u{1b}[23;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(11)).with_ctrl_modifier()),
        "Can parse a bare 'F11' keypress with ctrl"
    );
    let key = "\u{1b}[24;5~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(12)).with_ctrl_modifier()),
        "Can parse a bare 'F12' keypress with ctrl"
    );
    let key = "\u{1b}[1;5D";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Left).with_ctrl_modifier()),
        "Can parse a bare 'Left' keypress with ctrl"
    );
    let key = "\u{1b}[1;5C";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Right).with_ctrl_modifier()),
        "Can parse a bare 'Right' keypress with ctrl"
    );
    let key = "\u{1b}[1;5A";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Up).with_ctrl_modifier()),
        "Can parse a bare 'Up' keypress with ctrl"
    );
    let key = "\u{1b}[1;5B";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Down).with_ctrl_modifier()),
        "Can parse a bare 'Down' keypress with ctrl"
    );
    let key = "\u{1b}[1;5H";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_ctrl_modifier()),
        "Can parse a bare 'Home' keypress with ctrl"
    );
    let key = "\u{1b}[1;5F";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_ctrl_modifier()),
        "Can parse a bare 'End' keypress with ctrl"
    );
    let key = "\u{1b}[1;5P";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_ctrl_modifier()),
        "Can parse a bare 'F1 (ctrlernate)' keypress with ctrl"
    );
    let key = "\u{1b}[1;5Q";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_ctrl_modifier()),
        "Can parse a bare 'F2 (ctrlernate)' keypress with ctrl"
    );
    let key = "\u{1b}[1;5S";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_ctrl_modifier()),
        "Can parse a bare 'F4 (ctrlernate)' keypress with ctrl"
    );
}

#[test]
pub fn can_parse_keys_with_super_modifier() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('a')).with_super_modifier()),
        "Can parse a bare 'a' keypress with super"
    );
    let key = "\u{1b}[49;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('1')).with_super_modifier()),
        "Can parse a bare '1' keypress with super"
    );
    let key = "\u{1b}[27;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Esc).with_super_modifier()),
        "Can parse a bare 'ESC' keypress with super"
    );
    let key = "\u{1b}[13;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Enter).with_super_modifier()),
        "Can parse a bare 'ENTER' keypress with super"
    );
    let key = "\u{1b}[9;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Tab).with_super_modifier()),
        "Can parse a bare 'Tab' keypress with super"
    );
    let key = "\u{1b}[127;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Backspace).with_super_modifier()),
        "Can parse a bare 'Backspace' keypress with super"
    );
    let key = "\u{1b}[57358;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::CapsLock).with_super_modifier()),
        "Can parse a bare 'CapsLock' keypress with super"
    );
    let key = "\u{1b}[57359;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::ScrollLock).with_super_modifier()),
        "Can parse a bare 'ScrollLock' keypress with super"
    );
    let key = "\u{1b}[57360;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::NumLock).with_super_modifier()),
        "Can parse a bare 'NumLock' keypress with super"
    );
    let key = "\u{1b}[57361;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PrintScreen).with_super_modifier()),
        "Can parse a bare 'PrintScreen' keypress with super"
    );
    let key = "\u{1b}[57362;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Pause).with_super_modifier()),
        "Can parse a bare 'Pause' keypress with super"
    );
    let key = "\u{1b}[57363;9u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Menu).with_super_modifier()),
        "Can parse a bare 'Menu' keypress with super"
    );

    let key = "\u{1b}[2;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Insert).with_super_modifier()),
        "Can parse a bare 'Insert' keypress with super"
    );
    let key = "\u{1b}[3;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Delete).with_super_modifier()),
        "Can parse a bare 'Delete' keypress with super"
    );
    let key = "\u{1b}[5;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageUp).with_super_modifier()),
        "Can parse a bare 'PageUp' keypress with super"
    );
    let key = "\u{1b}[6;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageDown).with_super_modifier()),
        "Can parse a bare 'PageDown' keypress with super"
    );
    let key = "\u{1b}[7;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_super_modifier()),
        "Can parse a bare 'Home' keypress with super"
    );
    let key = "\u{1b}[8;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_super_modifier()),
        "Can parse a bare 'End' keypress with super"
    );
    let key = "\u{1b}[11;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_super_modifier()),
        "Can parse a bare 'F1' keypress with super"
    );
    let key = "\u{1b}[12;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_super_modifier()),
        "Can parse a bare 'F2' keypress with super"
    );
    let key = "\u{1b}[13;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(3)).with_super_modifier()),
        "Can parse a bare 'F3' keypress with super"
    );
    let key = "\u{1b}[14;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_super_modifier()),
        "Can parse a bare 'F4' keypress with super"
    );
    let key = "\u{1b}[15;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(5)).with_super_modifier()),
        "Can parse a bare 'F5' keypress with super"
    );
    let key = "\u{1b}[17;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(6)).with_super_modifier()),
        "Can parse a bare 'F6' keypress with super"
    );
    let key = "\u{1b}[18;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(7)).with_super_modifier()),
        "Can parse a bare 'F7' keypress with super"
    );
    let key = "\u{1b}[19;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(8)).with_super_modifier()),
        "Can parse a bare 'F8' keypress with super"
    );
    let key = "\u{1b}[20;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(9)).with_super_modifier()),
        "Can parse a bare 'F9' keypress with super"
    );
    let key = "\u{1b}[21;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(10)).with_super_modifier()),
        "Can parse a bare 'F10' keypress with super"
    );
    let key = "\u{1b}[23;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(11)).with_super_modifier()),
        "Can parse a bare 'F11' keypress with super"
    );
    let key = "\u{1b}[24;9~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(12)).with_super_modifier()),
        "Can parse a bare 'F12' keypress with super"
    );
    let key = "\u{1b}[1;9D";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Left).with_super_modifier()),
        "Can parse a bare 'Left' keypress with super"
    );
    let key = "\u{1b}[1;9C";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Right).with_super_modifier()),
        "Can parse a bare 'Right' keypress with super"
    );
    let key = "\u{1b}[1;9A";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Up).with_super_modifier()),
        "Can parse a bare 'Up' keypress with super"
    );
    let key = "\u{1b}[1;9B";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Down).with_super_modifier()),
        "Can parse a bare 'Down' keypress with super"
    );
    let key = "\u{1b}[1;9H";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_super_modifier()),
        "Can parse a bare 'Home' keypress with super"
    );
    let key = "\u{1b}[1;9F";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_super_modifier()),
        "Can parse a bare 'End' keypress with super"
    );
    let key = "\u{1b}[1;9P";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_super_modifier()),
        "Can parse a bare 'F1 (alternate)' keypress with super"
    );
    let key = "\u{1b}[1;9Q";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_super_modifier()),
        "Can parse a bare 'F2 (alternate)' keypress with super"
    );
    let key = "\u{1b}[1;9S";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_super_modifier()),
        "Can parse a bare 'F4 (alternate)' keypress with super"
    );
}

#[test]
pub fn can_parse_keys_with_multiple_modifiers() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Char('a'))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'a' keypress with all modifiers"
    );
    let key = "\u{1b}[49;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Char('1'))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare '1' keypress with all modifiers"
    );
    let key = "\u{1b}[27;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Esc)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'ESC' keypress with all modifiers"
    );
    let key = "\u{1b}[13;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Enter)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'ENTER' keypress with all modifiers"
    );
    let key = "\u{1b}[9;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Tab)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Tab' keypress with all modifiers"
    );
    let key = "\u{1b}[127;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Backspace)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Backspace' keypress with all modifiers"
    );
    let key = "\u{1b}[57358;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::CapsLock)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'CapsLock' keypress with all modifiers"
    );
    let key = "\u{1b}[57359;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::ScrollLock)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'ScrollLock' keypress with all modifiers"
    );
    let key = "\u{1b}[57360;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::NumLock)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'NumLock' keypress with all modifiers"
    );
    let key = "\u{1b}[57361;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::PrintScreen)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'PrintScreen' keypress with all modifiers"
    );
    let key = "\u{1b}[57362;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Pause)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Pause' keypress with all modifiers"
    );
    let key = "\u{1b}[57363;16u";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Menu)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Menu' keypress with all modifiers"
    );

    let key = "\u{1b}[2;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Insert)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Insert' keypress with all modifiers"
    );
    let key = "\u{1b}[3;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Delete)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Delete' keypress with all modifiers"
    );
    let key = "\u{1b}[5;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::PageUp)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'PageUp' keypress with all modifiers"
    );
    let key = "\u{1b}[6;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::PageDown)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'PageDown' keypress with all modifiers"
    );
    let key = "\u{1b}[7;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Home)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Home' keypress with all modifiers"
    );
    let key = "\u{1b}[8;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::End)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'End' keypress with all modifiers"
    );
    let key = "\u{1b}[11;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(1))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F1' keypress with all modifiers"
    );
    let key = "\u{1b}[12;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(2))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F2' keypress with all modifiers"
    );
    let key = "\u{1b}[13;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(3))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F3' keypress with all modifiers"
    );
    let key = "\u{1b}[14;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(4))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F4' keypress with all modifiers"
    );
    let key = "\u{1b}[15;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(5))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F5' keypress with all modifiers"
    );
    let key = "\u{1b}[17;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(6))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F6' keypress with all modifiers"
    );
    let key = "\u{1b}[18;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(7))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F7' keypress with all modifiers"
    );
    let key = "\u{1b}[19;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(8))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F8' keypress with all modifiers"
    );
    let key = "\u{1b}[20;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(9))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F9' keypress with all modifiers"
    );
    let key = "\u{1b}[21;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(10))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F10' keypress with all modifiers"
    );
    let key = "\u{1b}[23;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(11))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F11' keypress with all modifiers"
    );
    let key = "\u{1b}[24;16~";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(12))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F12' keypress with all modifiers"
    );
    let key = "\u{1b}[1;16D";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Left)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Left' keypress with all modifiers"
    );
    let key = "\u{1b}[1;16C";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Right)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Right' keypress with all modifiers"
    );
    let key = "\u{1b}[1;16A";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Up)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Up' keypress with all modifiers"
    );
    let key = "\u{1b}[1;16B";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Down)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Down' keypress with all modifiers"
    );
    let key = "\u{1b}[1;16H";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Home)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'Home' keypress with all modifiers"
    );
    let key = "\u{1b}[1;16F";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::End)
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'End' keypress with all modifiers"
    );
    let key = "\u{1b}[1;16P";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(1))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F1 (superernate)' keypress with all modifiers"
    );
    let key = "\u{1b}[1;16Q";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(2))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F2 (superernate)' keypress with all modifiers"
    );
    let key = "\u{1b}[1;16S";
    assert_eq!(
        KittyKeyboardParser::new().parse(&key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(4))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F4 (superernate)' keypress with all modifiers"
    );
}

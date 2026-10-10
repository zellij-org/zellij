// for more info, please see: https://sw.kovidgoyal.net/kitty/keyboard-protocol
use zellij_utils::data::KeyWithModifier;

#[derive(Debug)]
enum KittyKeysParsingState {
    Ground,
    ReceivedEscapeCharacter,
    ParsingNumber,
    ParsingModifiers,
    /// Third CSI-u parameter (after the second `;`), present when the
    /// terminal has `REPORT_ASSOCIATED_TEXT` enabled. Holds the produced
    /// character codepoint(s) as decimal digits, optionally separated by
    /// `:` for multi-codepoint text.
    ParsingAssociatedText,
    DoneParsingWithU,
    DoneParsingWithTilde,
}

/// Three-way outcome of `KittyKeyboardParser::feed()`. Lets a long-lived
/// parser distinguish a finished sequence (consume + reset) from a
/// valid prefix (keep state, wait for the next chunk) from an unrelated
/// byte stream (reset and let a fallback parser handle it).
#[derive(Debug)]
pub enum KittyParseOutcome {
    /// Complete sequence parsed; parser resets to Ground.
    Complete(KeyWithModifier),
    /// Bytes are a valid prefix; parser keeps state. Caller should let
    /// termwiz also see them this round and call `feed()` again on the
    /// next chunk.
    Incomplete,
    /// Bytes are not a Kitty sequence; parser resets to Ground.
    NoMatch,
}

#[derive(Debug)]
pub struct KittyKeyboardParser {
    state: KittyKeysParsingState,
    number_bytes: Vec<u8>,
    modifier_bytes: Vec<u8>,
    associated_text_bytes: Vec<u8>,
}

/// CSI final-byte range (0x40..=0x7E), minus `u` and `~` which trigger
/// the explicit `DoneParsingWith{U,Tilde}` states inside the parser.
/// A trailing letter in this range while still in
/// `ParsingNumber`/`ParsingModifiers` indicates a complete
/// letter-terminated sequence (e.g. `\x1b[A`, `\x1b[1;2A`).
fn is_csi_final_letter(b: u8) -> bool {
    (0x40..=0x7E).contains(&b) && b != b'u' && b != b'~'
}

impl KittyKeyboardParser {
    pub fn new() -> Self {
        KittyKeyboardParser {
            state: KittyKeysParsingState::Ground,
            number_bytes: vec![],
            modifier_bytes: vec![],
            associated_text_bytes: vec![],
        }
    }

    fn reset(&mut self) {
        self.state = KittyKeysParsingState::Ground;
        self.number_bytes.clear();
        self.modifier_bytes.clear();
        self.associated_text_bytes.clear();
    }

    /// Stateful, cross-chunk-aware entry point. Drives the same
    /// state machine as `parse()` but:
    /// * resets to Ground after producing `Complete`/`NoMatch`, so the
    ///   parser can be reused for the next sequence;
    /// * preserves state on `Incomplete`, so a sequence split across
    ///   chunks still resolves on a follow-up call.
    ///
    /// The existing `parse()` wrapper is retained for the unit tests in
    /// this file, which construct `::new()` per assertion.
    pub fn feed(&mut self, bytes: &[u8]) -> KittyParseOutcome {
        for byte in bytes {
            if !self.advance(*byte) {
                self.reset();
                return KittyParseOutcome::NoMatch;
            }
        }
        match self.state {
            KittyKeysParsingState::DoneParsingWithU => {
                let result = KeyWithModifier::from_bytes_with_u(
                    &self.number_bytes,
                    &self.modifier_bytes,
                    &self.associated_text_bytes,
                );
                self.reset();
                match result {
                    Some(k) => KittyParseOutcome::Complete(k),
                    None => KittyParseOutcome::NoMatch,
                }
            },
            KittyKeysParsingState::DoneParsingWithTilde => {
                let result = KeyWithModifier::from_bytes_with_tilde(
                    &self.number_bytes,
                    &self.modifier_bytes,
                    &self.associated_text_bytes,
                );
                self.reset();
                match result {
                    Some(k) => KittyParseOutcome::Complete(k),
                    None => KittyParseOutcome::NoMatch,
                }
            },
            KittyKeysParsingState::ParsingNumber => {
                // ParsingNumber holds either a digit run waiting for a
                // terminator (Incomplete) or a single letter that is
                // itself the terminator — `\x1b[A` etc. (Complete).
                match self.number_bytes.last().copied() {
                    Some(last) if is_csi_final_letter(last) => {
                        let result = KeyWithModifier::from_bytes_with_no_ending_byte(
                            &self.number_bytes,
                            &self.modifier_bytes,
                        );
                        self.reset();
                        match result {
                            Some(k) => KittyParseOutcome::Complete(k),
                            None => KittyParseOutcome::NoMatch,
                        }
                    },
                    _ => KittyParseOutcome::Incomplete,
                }
            },
            KittyKeysParsingState::ParsingModifiers => {
                // ParsingModifiers holds either modifier digits waiting
                // for the terminator letter (Incomplete) or modifier
                // digits + a trailing letter terminator
                // — `\x1b[1;2A` etc. (Complete).
                match self.modifier_bytes.last().copied() {
                    Some(last) if is_csi_final_letter(last) => {
                        let last_modifier = self.modifier_bytes.pop().unwrap();
                        let result = KeyWithModifier::from_bytes_with_no_ending_byte(
                            &[last_modifier],
                            &self.modifier_bytes,
                        );
                        self.reset();
                        match result {
                            Some(k) => KittyParseOutcome::Complete(k),
                            None => KittyParseOutcome::NoMatch,
                        }
                    },
                    _ => KittyParseOutcome::Incomplete,
                }
            },
            // Associated text never appears in letter-terminated sequences
            // (those are pure functional keys with no produced character),
            // so we only wait for a `u` or `~` terminator here.
            KittyKeysParsingState::ParsingAssociatedText => KittyParseOutcome::Incomplete,
            KittyKeysParsingState::ReceivedEscapeCharacter => KittyParseOutcome::Incomplete,
            KittyKeysParsingState::Ground => KittyParseOutcome::NoMatch,
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
            (KittyKeysParsingState::ParsingModifiers, 59) => {
                // second semicolon: transition to the associated-text param.
                self.state = KittyKeysParsingState::ParsingAssociatedText;
            },
            (
                KittyKeysParsingState::ParsingNumber
                | KittyKeysParsingState::ParsingModifiers
                | KittyKeysParsingState::ParsingAssociatedText,
                117,
            ) => {
                // u
                self.state = KittyKeysParsingState::DoneParsingWithU;
            },
            (
                KittyKeysParsingState::ParsingNumber
                | KittyKeysParsingState::ParsingModifiers
                | KittyKeysParsingState::ParsingAssociatedText,
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
            (KittyKeysParsingState::ParsingAssociatedText, _) => {
                self.associated_text_bytes.push(byte);
            },
            (_, _) => {
                return false;
            },
        }
        true
    }
}

/// Test helper. Drives the production `feed()` entry point on a single
/// chunk and projects its three-way outcome onto an `Option` so the
/// existing assertion shape (`Some(KeyWithModifier { … })`) stays
/// readable. The full-byte tests in this file expect the input to be a
/// single complete sequence; `Incomplete` and `NoMatch` both flatten to
/// `None`.
#[cfg(test)]
fn parse_for_test(bytes: &[u8]) -> Option<KeyWithModifier> {
    match KittyKeyboardParser::new().feed(bytes) {
        KittyParseOutcome::Complete(k) => Some(k),
        KittyParseOutcome::Incomplete | KittyParseOutcome::NoMatch => None,
    }
}

#[test]
pub fn can_parse_bare_keys() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('a'))),
        "Can parse a bare 'a' keypress"
    );
    let key = "\u{1b}[49u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('1'))),
        "Can parse a bare '1' keypress"
    );
    let key = "\u{1b}[27u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Esc)),
        "Can parse a bare 'ESC' keypress"
    );
    let key = "\u{1b}[13u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Enter)),
        "Can parse a bare 'ENTER' keypress"
    );
    let key = "\u{1b}[9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Tab)),
        "Can parse a bare 'Tab' keypress"
    );
    let key = "\u{1b}[127u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Backspace)),
        "Can parse a bare 'Backspace' keypress"
    );
    let key = "\u{1b}[57358u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::CapsLock)),
        "Can parse a bare 'CapsLock' keypress"
    );
    let key = "\u{1b}[57359u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::ScrollLock)),
        "Can parse a bare 'ScrollLock' keypress"
    );
    let key = "\u{1b}[57360u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::NumLock)),
        "Can parse a bare 'NumLock' keypress"
    );
    let key = "\u{1b}[57361u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PrintScreen)),
        "Can parse a bare 'PrintScreen' keypress"
    );
    let key = "\u{1b}[57362u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Pause)),
        "Can parse a bare 'Pause' keypress"
    );
    let key = "\u{1b}[57363u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Menu)),
        "Can parse a bare 'Menu' keypress"
    );

    let key = "\u{1b}[2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Insert)),
        "Can parse a bare 'Insert' keypress"
    );
    let key = "\u{1b}[3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Delete)),
        "Can parse a bare 'Delete' keypress"
    );
    let key = "\u{1b}[5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageUp)),
        "Can parse a bare 'PageUp' keypress"
    );
    let key = "\u{1b}[6~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageDown)),
        "Can parse a bare 'PageDown' keypress"
    );
    let key = "\u{1b}[7~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home)),
        "Can parse a bare 'Home' keypress"
    );
    let key = "\u{1b}[8~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End)),
        "Can parse a bare 'End' keypress"
    );
    let key = "\u{1b}[11~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1))),
        "Can parse a bare 'F1' keypress"
    );
    let key = "\u{1b}[12~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2))),
        "Can parse a bare 'F2' keypress"
    );
    let key = "\u{1b}[13~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(3))),
        "Can parse a bare 'F3' keypress"
    );
    let key = "\u{1b}[14~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4))),
        "Can parse a bare 'F4' keypress"
    );
    let key = "\u{1b}[15~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(5))),
        "Can parse a bare 'F5' keypress"
    );
    let key = "\u{1b}[17~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(6))),
        "Can parse a bare 'F6' keypress"
    );
    let key = "\u{1b}[18~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(7))),
        "Can parse a bare 'F7' keypress"
    );
    let key = "\u{1b}[19~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(8))),
        "Can parse a bare 'F8' keypress"
    );
    let key = "\u{1b}[20~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(9))),
        "Can parse a bare 'F9' keypress"
    );
    let key = "\u{1b}[21~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(10))),
        "Can parse a bare 'F10' keypress"
    );
    let key = "\u{1b}[23~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(11))),
        "Can parse a bare 'F11' keypress"
    );
    let key = "\u{1b}[24~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(12))),
        "Can parse a bare 'F12' keypress"
    );
    let key = "\u{1b}[D";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Left)),
        "Can parse a bare 'Left' keypress"
    );
    let key = "\u{1b}[C";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Right)),
        "Can parse a bare 'Right' keypress"
    );
    let key = "\u{1b}[A";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Up)),
        "Can parse a bare 'Up' keypress"
    );
    let key = "\u{1b}[B";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Down)),
        "Can parse a bare 'Down' keypress"
    );
    let key = "\u{1b}[H";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home)),
        "Can parse a bare 'Home' keypress"
    );
    let key = "\u{1b}[F";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End)),
        "Can parse a bare 'End' keypress"
    );
    let key = "\u{1b}[P";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1))),
        "Can parse a bare 'F1 (alternate)' keypress"
    );
    let key = "\u{1b}[Q";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2))),
        "Can parse a bare 'F2 (alternate)' keypress"
    );
    let key = "\u{1b}[S";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4))),
        "Can parse a bare 'F4 (alternate)' keypress"
    );
    let key = "\u{1b}[1087u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('п'))),
        "Can parse a bare 'п' keypress"
    );
    let key = "\u{1b}[1255u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('ӧ'))),
        "Can parse a bare 'ӧ' keypress"
    );
    let key = "\u{1b}[1098u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('ъ'))),
        "Can parse a bare 'ъ' keypress"
    );
}

#[test]
pub fn can_parse_keys_with_shift_modifier() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('a')).with_shift_modifier()),
        "Can parse a bare 'a' keypress with shift"
    );
    let key = "\u{1b}[49;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('1')).with_shift_modifier()),
        "Can parse a bare '1' keypress with shift"
    );
    let key = "\u{1b}[27;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Esc).with_shift_modifier()),
        "Can parse a bare 'ESC' keypress with shift"
    );
    let key = "\u{1b}[13;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Enter).with_shift_modifier()),
        "Can parse a bare 'ENTER' keypress with shift"
    );
    let key = "\u{1b}[9;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Tab).with_shift_modifier()),
        "Can parse a bare 'Tab' keypress with shift"
    );
    let key = "\u{1b}[127;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Backspace).with_shift_modifier()),
        "Can parse a bare 'Backspace' keypress with shift"
    );
    let key = "\u{1b}[57358;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::CapsLock).with_shift_modifier()),
        "Can parse a bare 'CapsLock' keypress with shift"
    );
    let key = "\u{1b}[57359;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::ScrollLock).with_shift_modifier()),
        "Can parse a bare 'ScrollLock' keypress with shift"
    );
    let key = "\u{1b}[57360;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::NumLock).with_shift_modifier()),
        "Can parse a bare 'NumLock' keypress with shift"
    );
    let key = "\u{1b}[57361;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PrintScreen).with_shift_modifier()),
        "Can parse a bare 'PrintScreen' keypress with shift"
    );
    let key = "\u{1b}[57362;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Pause).with_shift_modifier()),
        "Can parse a bare 'Pause' keypress with shift"
    );
    let key = "\u{1b}[57363;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Menu).with_shift_modifier()),
        "Can parse a bare 'Menu' keypress with shift"
    );

    let key = "\u{1b}[2;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Insert).with_shift_modifier()),
        "Can parse a bare 'Insert' keypress with shift"
    );
    let key = "\u{1b}[3;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Delete).with_shift_modifier()),
        "Can parse a bare 'Delete' keypress with shift"
    );
    let key = "\u{1b}[5;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageUp).with_shift_modifier()),
        "Can parse a bare 'PageUp' keypress with shift"
    );
    let key = "\u{1b}[6;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageDown).with_shift_modifier()),
        "Can parse a bare 'PageDown' keypress with shift"
    );
    let key = "\u{1b}[7;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_shift_modifier()),
        "Can parse a bare 'Home' keypress with shift"
    );
    let key = "\u{1b}[8;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_shift_modifier()),
        "Can parse a bare 'End' keypress with shift"
    );
    let key = "\u{1b}[11;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_shift_modifier()),
        "Can parse a bare 'F1' keypress with shift"
    );
    let key = "\u{1b}[12;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_shift_modifier()),
        "Can parse a bare 'F2' keypress with shift"
    );
    let key = "\u{1b}[13;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(3)).with_shift_modifier()),
        "Can parse a bare 'F3' keypress with shift"
    );
    let key = "\u{1b}[14;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_shift_modifier()),
        "Can parse a bare 'F4' keypress with shift"
    );
    let key = "\u{1b}[15;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(5)).with_shift_modifier()),
        "Can parse a bare 'F5' keypress with shift"
    );
    let key = "\u{1b}[17;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(6)).with_shift_modifier()),
        "Can parse a bare 'F6' keypress with shift"
    );
    let key = "\u{1b}[18;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(7)).with_shift_modifier()),
        "Can parse a bare 'F7' keypress with shift"
    );
    let key = "\u{1b}[19;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(8)).with_shift_modifier()),
        "Can parse a bare 'F8' keypress with shift"
    );
    let key = "\u{1b}[20;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(9)).with_shift_modifier()),
        "Can parse a bare 'F9' keypress with shift"
    );
    let key = "\u{1b}[21;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(10)).with_shift_modifier()),
        "Can parse a bare 'F10' keypress with shift"
    );
    let key = "\u{1b}[23;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(11)).with_shift_modifier()),
        "Can parse a bare 'F11' keypress with shift"
    );
    let key = "\u{1b}[24;2~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(12)).with_shift_modifier()),
        "Can parse a bare 'F12' keypress with shift"
    );
    let key = "\u{1b}[1;2D";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Left).with_shift_modifier()),
        "Can parse a bare 'Left' keypress with shift"
    );
    let key = "\u{1b}[1;2C";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Right).with_shift_modifier()),
        "Can parse a bare 'Right' keypress with shift"
    );
    let key = "\u{1b}[1;2A";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Up).with_shift_modifier()),
        "Can parse a bare 'Up' keypress with shift"
    );
    let key = "\u{1b}[1;2B";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Down).with_shift_modifier()),
        "Can parse a bare 'Down' keypress with shift"
    );
    let key = "\u{1b}[1;2H";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_shift_modifier()),
        "Can parse a bare 'Home' keypress with shift"
    );
    let key = "\u{1b}[1;2F";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_shift_modifier()),
        "Can parse a bare 'End' keypress with shift"
    );
    let key = "\u{1b}[1;2P";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_shift_modifier()),
        "Can parse a bare 'F1 (alternate)' keypress with shift"
    );
    let key = "\u{1b}[1;2Q";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_shift_modifier()),
        "Can parse a bare 'F2 (alternate)' keypress with shift"
    );
    let key = "\u{1b}[1;2S";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_shift_modifier()),
        "Can parse a bare 'F4 (alternate)' keypress with shift"
    );
    let key = "\u{1b}[1087;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('п')).with_shift_modifier()),
        "Can parse a bare 'п' keypress with shift"
    );
    let key = "\u{1b}[1255;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('ӧ')).with_shift_modifier()),
        "Can parse a bare 'ӧ' keypress with shift"
    );
    let key = "\u{1b}[1098;2u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('ъ')).with_shift_modifier()),
        "Can parse a bare 'ъ' keypress with shift"
    );
}

#[test]
pub fn can_parse_keys_with_alt_modifier() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('a')).with_alt_modifier()),
        "Can parse a bare 'a' keypress with alt"
    );
    let key = "\u{1b}[49;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('1')).with_alt_modifier()),
        "Can parse a bare '1' keypress with alt"
    );
    let key = "\u{1b}[27;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Esc).with_alt_modifier()),
        "Can parse a bare 'ESC' keypress with alt"
    );
    let key = "\u{1b}[13;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Enter).with_alt_modifier()),
        "Can parse a bare 'ENTER' keypress with alt"
    );
    let key = "\u{1b}[9;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Tab).with_alt_modifier()),
        "Can parse a bare 'Tab' keypress with alt"
    );
    let key = "\u{1b}[127;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Backspace).with_alt_modifier()),
        "Can parse a bare 'Backspace' keypress with alt"
    );
    let key = "\u{1b}[57358;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::CapsLock).with_alt_modifier()),
        "Can parse a bare 'CapsLock' keypress with alt"
    );
    let key = "\u{1b}[57359;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::ScrollLock).with_alt_modifier()),
        "Can parse a bare 'ScrollLock' keypress with alt"
    );
    let key = "\u{1b}[57360;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::NumLock).with_alt_modifier()),
        "Can parse a bare 'NumLock' keypress with alt"
    );
    let key = "\u{1b}[57361;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PrintScreen).with_alt_modifier()),
        "Can parse a bare 'PrintScreen' keypress with alt"
    );
    let key = "\u{1b}[57362;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Pause).with_alt_modifier()),
        "Can parse a bare 'Pause' keypress with alt"
    );
    let key = "\u{1b}[57363;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Menu).with_alt_modifier()),
        "Can parse a bare 'Menu' keypress with alt"
    );

    let key = "\u{1b}[2;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Insert).with_alt_modifier()),
        "Can parse a bare 'Insert' keypress with alt"
    );
    let key = "\u{1b}[3;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Delete).with_alt_modifier()),
        "Can parse a bare 'Delete' keypress with alt"
    );
    let key = "\u{1b}[5;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageUp).with_alt_modifier()),
        "Can parse a bare 'PageUp' keypress with alt"
    );
    let key = "\u{1b}[6;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageDown).with_alt_modifier()),
        "Can parse a bare 'PageDown' keypress with alt"
    );
    let key = "\u{1b}[7;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_alt_modifier()),
        "Can parse a bare 'Home' keypress with alt"
    );
    let key = "\u{1b}[8;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_alt_modifier()),
        "Can parse a bare 'End' keypress with alt"
    );
    let key = "\u{1b}[11;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_alt_modifier()),
        "Can parse a bare 'F1' keypress with alt"
    );
    let key = "\u{1b}[12;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_alt_modifier()),
        "Can parse a bare 'F2' keypress with alt"
    );
    let key = "\u{1b}[13;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(3)).with_alt_modifier()),
        "Can parse a bare 'F3' keypress with alt"
    );
    let key = "\u{1b}[14;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_alt_modifier()),
        "Can parse a bare 'F4' keypress with alt"
    );
    let key = "\u{1b}[15;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(5)).with_alt_modifier()),
        "Can parse a bare 'F5' keypress with alt"
    );
    let key = "\u{1b}[17;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(6)).with_alt_modifier()),
        "Can parse a bare 'F6' keypress with alt"
    );
    let key = "\u{1b}[18;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(7)).with_alt_modifier()),
        "Can parse a bare 'F7' keypress with alt"
    );
    let key = "\u{1b}[19;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(8)).with_alt_modifier()),
        "Can parse a bare 'F8' keypress with alt"
    );
    let key = "\u{1b}[20;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(9)).with_alt_modifier()),
        "Can parse a bare 'F9' keypress with alt"
    );
    let key = "\u{1b}[21;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(10)).with_alt_modifier()),
        "Can parse a bare 'F10' keypress with alt"
    );
    let key = "\u{1b}[23;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(11)).with_alt_modifier()),
        "Can parse a bare 'F11' keypress with alt"
    );
    let key = "\u{1b}[24;3~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(12)).with_alt_modifier()),
        "Can parse a bare 'F12' keypress with alt"
    );
    let key = "\u{1b}[1;3D";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Left).with_alt_modifier()),
        "Can parse a bare 'Left' keypress with alt"
    );
    let key = "\u{1b}[1;3C";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Right).with_alt_modifier()),
        "Can parse a bare 'Right' keypress with alt"
    );
    let key = "\u{1b}[1;3A";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Up).with_alt_modifier()),
        "Can parse a bare 'Up' keypress with alt"
    );
    let key = "\u{1b}[1;3B";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Down).with_alt_modifier()),
        "Can parse a bare 'Down' keypress with alt"
    );
    let key = "\u{1b}[1;3H";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_alt_modifier()),
        "Can parse a bare 'Home' keypress with alt"
    );
    let key = "\u{1b}[1;3F";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_alt_modifier()),
        "Can parse a bare 'End' keypress with alt"
    );
    let key = "\u{1b}[1;3P";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_alt_modifier()),
        "Can parse a bare 'F1 (alternate)' keypress with alt"
    );
    let key = "\u{1b}[1;3Q";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_alt_modifier()),
        "Can parse a bare 'F2 (alternate)' keypress with alt"
    );
    let key = "\u{1b}[1;3S";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_alt_modifier()),
        "Can parse a bare 'F4 (alternate)' keypress with alt"
    );
    let key = "\u{1b}[1087;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('п')).with_alt_modifier()),
        "Can parse a bare 'п' keypress with alt"
    );
    let key = "\u{1b}[1255;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('ӧ')).with_alt_modifier()),
        "Can parse a bare 'ӧ' keypress with alt"
    );
    let key = "\u{1b}[1098;3u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('ъ')).with_alt_modifier()),
        "Can parse a bare 'ъ' keypress with alt"
    );
}

#[test]
pub fn can_parse_keys_with_ctrl_modifier() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('a')).with_ctrl_modifier()),
        "Can parse a bare 'a' keypress with ctrl"
    );
    let key = "\u{1b}[49;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('1')).with_ctrl_modifier()),
        "Can parse a bare '1' keypress with ctrl"
    );
    let key = "\u{1b}[27;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Esc).with_ctrl_modifier()),
        "Can parse a bare 'ESC' keypress with ctrl"
    );
    let key = "\u{1b}[13;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Enter).with_ctrl_modifier()),
        "Can parse a bare 'ENTER' keypress with ctrl"
    );
    let key = "\u{1b}[9;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Tab).with_ctrl_modifier()),
        "Can parse a bare 'Tab' keypress with ctrl"
    );
    let key = "\u{1b}[127;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Backspace).with_ctrl_modifier()),
        "Can parse a bare 'Backspace' keypress with ctrl"
    );
    let key = "\u{1b}[57358;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::CapsLock).with_ctrl_modifier()),
        "Can parse a bare 'CapsLock' keypress with ctrl"
    );
    let key = "\u{1b}[57359;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::ScrollLock).with_ctrl_modifier()),
        "Can parse a bare 'ScrollLock' keypress with ctrl"
    );
    let key = "\u{1b}[57360;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::NumLock).with_ctrl_modifier()),
        "Can parse a bare 'NumLock' keypress with ctrl"
    );
    let key = "\u{1b}[57361;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PrintScreen).with_ctrl_modifier()),
        "Can parse a bare 'PrintScreen' keypress with ctrl"
    );
    let key = "\u{1b}[57362;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Pause).with_ctrl_modifier()),
        "Can parse a bare 'Pause' keypress with ctrl"
    );
    let key = "\u{1b}[57363;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Menu).with_ctrl_modifier()),
        "Can parse a bare 'Menu' keypress with ctrl"
    );

    let key = "\u{1b}[2;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Insert).with_ctrl_modifier()),
        "Can parse a bare 'Insert' keypress with ctrl"
    );
    let key = "\u{1b}[3;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Delete).with_ctrl_modifier()),
        "Can parse a bare 'Delete' keypress with ctrl"
    );
    let key = "\u{1b}[5;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageUp).with_ctrl_modifier()),
        "Can parse a bare 'PageUp' keypress with ctrl"
    );
    let key = "\u{1b}[6;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageDown).with_ctrl_modifier()),
        "Can parse a bare 'PageDown' keypress with ctrl"
    );
    let key = "\u{1b}[7;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_ctrl_modifier()),
        "Can parse a bare 'Home' keypress with ctrl"
    );
    let key = "\u{1b}[8;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_ctrl_modifier()),
        "Can parse a bare 'End' keypress with ctrl"
    );
    let key = "\u{1b}[11;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_ctrl_modifier()),
        "Can parse a bare 'F1' keypress with ctrl"
    );
    let key = "\u{1b}[12;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_ctrl_modifier()),
        "Can parse a bare 'F2' keypress with ctrl"
    );
    let key = "\u{1b}[13;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(3)).with_ctrl_modifier()),
        "Can parse a bare 'F3' keypress with ctrl"
    );
    let key = "\u{1b}[14;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_ctrl_modifier()),
        "Can parse a bare 'F4' keypress with ctrl"
    );
    let key = "\u{1b}[15;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(5)).with_ctrl_modifier()),
        "Can parse a bare 'F5' keypress with ctrl"
    );
    let key = "\u{1b}[17;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(6)).with_ctrl_modifier()),
        "Can parse a bare 'F6' keypress with ctrl"
    );
    let key = "\u{1b}[18;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(7)).with_ctrl_modifier()),
        "Can parse a bare 'F7' keypress with ctrl"
    );
    let key = "\u{1b}[19;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(8)).with_ctrl_modifier()),
        "Can parse a bare 'F8' keypress with ctrl"
    );
    let key = "\u{1b}[20;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(9)).with_ctrl_modifier()),
        "Can parse a bare 'F9' keypress with ctrl"
    );
    let key = "\u{1b}[21;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(10)).with_ctrl_modifier()),
        "Can parse a bare 'F10' keypress with ctrl"
    );
    let key = "\u{1b}[23;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(11)).with_ctrl_modifier()),
        "Can parse a bare 'F11' keypress with ctrl"
    );
    let key = "\u{1b}[24;5~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(12)).with_ctrl_modifier()),
        "Can parse a bare 'F12' keypress with ctrl"
    );
    let key = "\u{1b}[1;5D";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Left).with_ctrl_modifier()),
        "Can parse a bare 'Left' keypress with ctrl"
    );
    let key = "\u{1b}[1;5C";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Right).with_ctrl_modifier()),
        "Can parse a bare 'Right' keypress with ctrl"
    );
    let key = "\u{1b}[1;5A";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Up).with_ctrl_modifier()),
        "Can parse a bare 'Up' keypress with ctrl"
    );
    let key = "\u{1b}[1;5B";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Down).with_ctrl_modifier()),
        "Can parse a bare 'Down' keypress with ctrl"
    );
    let key = "\u{1b}[1;5H";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_ctrl_modifier()),
        "Can parse a bare 'Home' keypress with ctrl"
    );
    let key = "\u{1b}[1;5F";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_ctrl_modifier()),
        "Can parse a bare 'End' keypress with ctrl"
    );
    let key = "\u{1b}[1;5P";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_ctrl_modifier()),
        "Can parse a bare 'F1 (ctrlernate)' keypress with ctrl"
    );
    let key = "\u{1b}[1;5Q";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_ctrl_modifier()),
        "Can parse a bare 'F2 (ctrlernate)' keypress with ctrl"
    );
    let key = "\u{1b}[1;5S";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_ctrl_modifier()),
        "Can parse a bare 'F4 (ctrlernate)' keypress with ctrl"
    );
    let key = "\u{1b}[1087;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('п')).with_ctrl_modifier()),
        "Can parse a bare 'п' keypress with ctrl"
    );
    let key = "\u{1b}[1255;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('ӧ')).with_ctrl_modifier()),
        "Can parse a bare 'ӧ' keypress with ctrl"
    );
    let key = "\u{1b}[1098;5u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('ъ')).with_ctrl_modifier()),
        "Can parse a bare 'ъ' keypress with ctrl"
    );
}

#[test]
pub fn can_parse_keys_with_super_modifier() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('a')).with_super_modifier()),
        "Can parse a bare 'a' keypress with super"
    );
    let key = "\u{1b}[49;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('1')).with_super_modifier()),
        "Can parse a bare '1' keypress with super"
    );
    let key = "\u{1b}[27;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Esc).with_super_modifier()),
        "Can parse a bare 'ESC' keypress with super"
    );
    let key = "\u{1b}[13;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Enter).with_super_modifier()),
        "Can parse a bare 'ENTER' keypress with super"
    );
    let key = "\u{1b}[9;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Tab).with_super_modifier()),
        "Can parse a bare 'Tab' keypress with super"
    );
    let key = "\u{1b}[127;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Backspace).with_super_modifier()),
        "Can parse a bare 'Backspace' keypress with super"
    );
    let key = "\u{1b}[57358;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::CapsLock).with_super_modifier()),
        "Can parse a bare 'CapsLock' keypress with super"
    );
    let key = "\u{1b}[57359;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::ScrollLock).with_super_modifier()),
        "Can parse a bare 'ScrollLock' keypress with super"
    );
    let key = "\u{1b}[57360;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::NumLock).with_super_modifier()),
        "Can parse a bare 'NumLock' keypress with super"
    );
    let key = "\u{1b}[57361;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PrintScreen).with_super_modifier()),
        "Can parse a bare 'PrintScreen' keypress with super"
    );
    let key = "\u{1b}[57362;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Pause).with_super_modifier()),
        "Can parse a bare 'Pause' keypress with super"
    );
    let key = "\u{1b}[57363;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Menu).with_super_modifier()),
        "Can parse a bare 'Menu' keypress with super"
    );

    let key = "\u{1b}[2;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Insert).with_super_modifier()),
        "Can parse a bare 'Insert' keypress with super"
    );
    let key = "\u{1b}[3;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Delete).with_super_modifier()),
        "Can parse a bare 'Delete' keypress with super"
    );
    let key = "\u{1b}[5;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageUp).with_super_modifier()),
        "Can parse a bare 'PageUp' keypress with super"
    );
    let key = "\u{1b}[6;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::PageDown).with_super_modifier()),
        "Can parse a bare 'PageDown' keypress with super"
    );
    let key = "\u{1b}[7;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_super_modifier()),
        "Can parse a bare 'Home' keypress with super"
    );
    let key = "\u{1b}[8;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_super_modifier()),
        "Can parse a bare 'End' keypress with super"
    );
    let key = "\u{1b}[11;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_super_modifier()),
        "Can parse a bare 'F1' keypress with super"
    );
    let key = "\u{1b}[12;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_super_modifier()),
        "Can parse a bare 'F2' keypress with super"
    );
    let key = "\u{1b}[13;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(3)).with_super_modifier()),
        "Can parse a bare 'F3' keypress with super"
    );
    let key = "\u{1b}[14;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_super_modifier()),
        "Can parse a bare 'F4' keypress with super"
    );
    let key = "\u{1b}[15;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(5)).with_super_modifier()),
        "Can parse a bare 'F5' keypress with super"
    );
    let key = "\u{1b}[17;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(6)).with_super_modifier()),
        "Can parse a bare 'F6' keypress with super"
    );
    let key = "\u{1b}[18;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(7)).with_super_modifier()),
        "Can parse a bare 'F7' keypress with super"
    );
    let key = "\u{1b}[19;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(8)).with_super_modifier()),
        "Can parse a bare 'F8' keypress with super"
    );
    let key = "\u{1b}[20;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(9)).with_super_modifier()),
        "Can parse a bare 'F9' keypress with super"
    );
    let key = "\u{1b}[21;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(10)).with_super_modifier()),
        "Can parse a bare 'F10' keypress with super"
    );
    let key = "\u{1b}[23;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(11)).with_super_modifier()),
        "Can parse a bare 'F11' keypress with super"
    );
    let key = "\u{1b}[24;9~";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(12)).with_super_modifier()),
        "Can parse a bare 'F12' keypress with super"
    );
    let key = "\u{1b}[1;9D";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Left).with_super_modifier()),
        "Can parse a bare 'Left' keypress with super"
    );
    let key = "\u{1b}[1;9C";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Right).with_super_modifier()),
        "Can parse a bare 'Right' keypress with super"
    );
    let key = "\u{1b}[1;9A";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Up).with_super_modifier()),
        "Can parse a bare 'Up' keypress with super"
    );
    let key = "\u{1b}[1;9B";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Down).with_super_modifier()),
        "Can parse a bare 'Down' keypress with super"
    );
    let key = "\u{1b}[1;9H";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Home).with_super_modifier()),
        "Can parse a bare 'Home' keypress with super"
    );
    let key = "\u{1b}[1;9F";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::End).with_super_modifier()),
        "Can parse a bare 'End' keypress with super"
    );
    let key = "\u{1b}[1;9P";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(1)).with_super_modifier()),
        "Can parse a bare 'F1 (alternate)' keypress with super"
    );
    let key = "\u{1b}[1;9Q";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(2)).with_super_modifier()),
        "Can parse a bare 'F2 (alternate)' keypress with super"
    );
    let key = "\u{1b}[1;9S";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::F(4)).with_super_modifier()),
        "Can parse a bare 'F4 (alternate)' keypress with super"
    );
    let key = "\u{1b}[1087;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('п')).with_super_modifier()),
        "Can parse a bare 'п' keypress with super"
    );
    let key = "\u{1b}[1255;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('ӧ')).with_super_modifier()),
        "Can parse a bare 'ӧ' keypress with super"
    );
    let key = "\u{1b}[1098;9u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(KeyWithModifier::new(BareKey::Char('ъ')).with_super_modifier()),
        "Can parse a bare 'ъ' keypress with super"
    );
}

#[test]
pub fn can_parse_keys_with_multiple_modifiers() {
    use zellij_utils::data::BareKey;
    let key = "\u{1b}[97;16u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
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
        parse_for_test(key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::F(4))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'F4 (superernate)' keypress with all modifiers"
    );
    let key = "\u{1b}[1087;16u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Char('п'))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'п' keypress with all modifiers"
    );
    let key = "\u{1b}[1255;16u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Char('ӧ'))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'ӧ' keypress with all modifiers"
    );
    let key = "\u{1b}[1098;16u";
    assert_eq!(
        parse_for_test(key.as_bytes()),
        Some(
            KeyWithModifier::new(BareKey::Char('ъ'))
                .with_super_modifier()
                .with_ctrl_modifier()
                .with_alt_modifier()
                .with_shift_modifier()
        ),
        "Can parse a bare 'ъ' keypress with all modifiers"
    );
}

// =====================================================================
// Cross-chunk fragmentation tests for the long-lived feed() entry
// point. Under SSH or any kernel-boundary-fragmented stdin read, a
// single Kitty CSI sequence routinely arrives split across multiple
// chunks; feed() must keep state across calls so the sequence still
// resolves on a follow-up chunk instead of degrading to legacy CSI
// form (and losing modifier metadata).
// =====================================================================

#[test]
fn fragmented_kitty_csi_u_emits_one_event() {
    use zellij_utils::data::BareKey;
    let mut p = KittyKeyboardParser::new();
    let r1 = p.feed(b"\x1b[97;");
    assert!(matches!(r1, KittyParseOutcome::Incomplete));
    match p.feed(b"2u") {
        KittyParseOutcome::Complete(k) => {
            assert_eq!(
                k,
                KeyWithModifier::new(BareKey::Char('a')).with_shift_modifier()
            );
        },
        other => panic!("expected Complete, got {:?}", other),
    }
}

#[test]
fn fragmented_kitty_byte_by_byte() {
    use zellij_utils::data::BareKey;
    let full = b"\x1b[97;5u"; // ctrl+a
    let mut p = KittyKeyboardParser::new();
    for &b in &full[..full.len() - 1] {
        assert!(
            matches!(p.feed(&[b]), KittyParseOutcome::Incomplete),
            "byte 0x{:02x} should be Incomplete",
            b
        );
    }
    match p.feed(&[full[full.len() - 1]]) {
        KittyParseOutcome::Complete(k) => {
            assert_eq!(
                k,
                KeyWithModifier::new(BareKey::Char('a')).with_ctrl_modifier()
            );
        },
        other => panic!("expected Complete, got {:?}", other),
    }
}

#[test]
fn non_kitty_bytes_yield_nomatch_and_reset() {
    // Plain printable bytes don't form a Kitty sequence — must return
    // NoMatch (not Incomplete) so the caller falls through to termwiz
    // immediately rather than buffering forever.
    let mut p = KittyKeyboardParser::new();
    assert!(matches!(p.feed(b"hello"), KittyParseOutcome::NoMatch));
}

// ---------------------------------------------------------------------------
// REPORT_ASSOCIATED_TEXT (3-parameter CSI-u) coverage.
//
// When the terminal has flag 16 enabled, modifier + key combinations come
// across as `\x1b[<keycode>;<modifier>;<text-codepoints>u`. The parser must
// (a) accept the third parameter without contaminating modifier_bytes, and
// (b) when the associated text differs from the keycode char, use the
// produced character as the bare key with no modifiers — this is how AltGr
// glyphs (Windows Terminal 1.25) and similar OS-keymap-resolved keystrokes
// reach zellij correctly.
// ---------------------------------------------------------------------------

#[test]
fn three_param_alt_f_preserves_alt_when_text_equals_keycode() {
    // WT 1.25, WezTerm, Alacritty all send real Alt+f as `\x1b[102;3;102u`.
    // Text == keycode → keep Alt so the Alt+f binding still fires.
    use zellij_utils::data::BareKey;
    let result = parse_for_test(b"\x1b[102;3;102u").expect("parse failed");
    assert_eq!(
        result,
        KeyWithModifier::new(BareKey::Char('f')).with_alt_modifier(),
    );
}

#[test]
fn three_param_altgr_glyph_emits_produced_char_no_modifiers() {
    // WT 1.25 on Belgian/Hungarian AZERTY sends AltGr+6 (= '|') as
    // `\x1b[45;3;124u`: keycode 45 ('-'), modifier 3 (Alt), text 124 ('|').
    // Text != keycode → emit the produced char with no modifiers so the
    // Alt+'-' binding doesn't false-fire and bash receives '|'.
    use zellij_utils::data::BareKey;
    let result = parse_for_test(b"\x1b[45;3;124u").expect("parse failed");
    assert_eq!(result, KeyWithModifier::new(BareKey::Char('|')));
}

#[test]
fn three_param_altgr_backslash_emits_backslash() {
    // AltGr+8 → '\' on AZERTY: `\x1b[95;3;92u`.
    use zellij_utils::data::BareKey;
    let result = parse_for_test(b"\x1b[95;3;92u").expect("parse failed");
    assert_eq!(result, KeyWithModifier::new(BareKey::Char('\\')));
}

#[test]
fn three_param_with_numlock_bit_preserves_alt() {
    // WezTerm with NumLock on: `\x1b[102;131;102u`. Modifier byte 131 has
    // bit 128 (NumLock) set, but the existing modifier-from-bytes path
    // already silently drops lock bits, so Alt is preserved.
    use zellij_utils::data::BareKey;
    let result = parse_for_test(b"\x1b[102;131;102u").expect("parse failed");
    assert_eq!(
        result,
        KeyWithModifier::new(BareKey::Char('f')).with_alt_modifier(),
    );
}

#[test]
fn three_param_shift_f_emits_uppercase() {
    // Shift+F: `\x1b[102;2;70u`. Text 'F' != keycode 'f' → emit 'F' with
    // no modifiers. (zellij has no Shift+letter bindings; behaviour at the
    // action layer is identical to legacy.)
    use zellij_utils::data::BareKey;
    let result = parse_for_test(b"\x1b[102;2;70u").expect("parse failed");
    assert_eq!(result, KeyWithModifier::new(BareKey::Char('F')));
}

#[test]
fn three_param_with_multi_codepoint_text_uses_first() {
    // Multi-codepoint associated text (rare: combining marks, some IME
    // outputs) is colon-separated per spec. We act on the first codepoint
    // only — enough for binding lookup, and the rest is acceptable to drop.
    use zellij_utils::data::BareKey;
    let result = parse_for_test(b"\x1b[97;3;65:768u").expect("parse failed");
    assert_eq!(result, KeyWithModifier::new(BareKey::Char('A')));
}

#[test]
fn three_param_with_control_codepoint_falls_through() {
    // If the associated text decodes to a control character, fall through
    // to the modifier-preserving path so existing legacy bindings still work.
    use zellij_utils::data::BareKey;
    let result = parse_for_test(b"\x1b[102;5;3u").expect("parse failed");
    assert_eq!(
        result,
        KeyWithModifier::new(BareKey::Char('f')).with_ctrl_modifier(),
    );
}

#[test]
fn two_param_form_still_parses_when_text_param_absent() {
    // WT 1.25 sends Ctrl+f without associated text: `\x1b[102;5u`. Our
    // changes must not break the 2-param path.
    use zellij_utils::data::BareKey;
    let result = parse_for_test(b"\x1b[102;5u").expect("parse failed");
    assert_eq!(
        result,
        KeyWithModifier::new(BareKey::Char('f')).with_ctrl_modifier(),
    );
}

#[test]
fn three_param_fragmented_across_chunks() {
    // Same handshake as `fragmented_kitty_csi_u_emits_one_event` but with
    // a 3-param sequence split across feed() calls.
    use zellij_utils::data::BareKey;
    let mut p = KittyKeyboardParser::new();
    assert!(matches!(
        p.feed(b"\x1b[45;3"),
        KittyParseOutcome::Incomplete
    ));
    assert!(matches!(p.feed(b";124"), KittyParseOutcome::Incomplete));
    let outcome = p.feed(b"u");
    match outcome {
        KittyParseOutcome::Complete(k) => {
            assert_eq!(k, KeyWithModifier::new(BareKey::Char('|')));
        },
        other => panic!("expected Complete, got {:?}", other),
    }
}

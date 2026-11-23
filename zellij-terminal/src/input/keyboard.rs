use std::borrow::Cow;
use std::collections::BTreeSet;

use winit::event::{KeyEvent, Modifiers};
use winit::keyboard::{
    Key, KeyCode, ModifiersKeyState, ModifiersState, NamedKey, NativeKeyCode, PhysicalKey, SmolStr,
};
use zellij_utils::data::{BareKey, KeyModifier, KeyWithModifier};
use zellij_utils::input::actions::Action;
use zellij_utils::input::terminal::OptionAsAlt;
use zellij_utils::ipc::ClientToServerMsg;

use crate::platform::Platform;

#[derive(Debug, Clone, PartialEq)]
pub struct Press {
    pub logical: Key,
    pub unmodified: Option<Key>,
    pub physical: PhysicalKey,
    pub modifiers: ModifiersState,
    // Whether a held Alt is reported even on a character the key does not type on its own;
    // see `keeps_alt`.
    pub keeps_alt: bool,
    pub text: Option<String>,
}

impl Press {
    pub fn of(event: &KeyEvent, modifiers: ModifiersState, keeps_alt: bool) -> Self {
        Self {
            logical: event.logical_key.clone(),
            unmodified: unmodified_key(event),
            physical: event.physical_key,
            modifiers,
            keeps_alt,
            text: event.text.as_ref().map(|text| text.to_string()),
        }
    }

    pub fn new(logical: Key, modifiers: ModifiersState) -> Self {
        Self {
            logical,
            unmodified: None,
            physical: PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
            modifiers,
            keeps_alt: false,
            text: None,
        }
    }

    // With Ctrl, Alt or Super (Cmd) held, a letter from a layout that does not write in Latin
    // (Cyrillic, Greek, Hebrew...) is reported as the US character of the same physical key,
    // as other terminals do: Ctrl+С on a Russian layout is Ctrl+C, Cmd+М is Cmd+V, and the
    // bindings written with Latin letters match. A Latin layout keeps its own letters, so
    // AZERTY's Ctrl+A is not the Q key.
    //
    // Windows treats Ctrl+Alt as AltGr, and a key that AltGr types nothing on arrives
    // unidentified. The chord is then on the key's own letter: Ctrl+Alt+L is Ctrl+Alt+l.
    fn reported_key(&self) -> Cow<'_, Key> {
        let chording = ModifiersState::CONTROL | ModifiersState::ALT | ModifiersState::SUPER;
        if !self.modifiers.intersects(chording) {
            return Cow::Borrowed(&self.logical);
        }
        let reported = match &self.logical {
            Key::Character(typed) => self.typed_stand_in(typed),
            Key::Unidentified(_) if self.produced_text().is_none() => self
                .unmodified_letter()
                .map(|letter| self.latin_stand_in(letter).unwrap_or(letter)),
            _ => None,
        };
        match reported {
            Some(character) => Cow::Owned(Key::Character(SmolStr::new(
                character.encode_utf8(&mut [0; 4]),
            ))),
            None => Cow::Borrowed(&self.logical),
        }
    }

    fn typed_stand_in(&self, typed: &str) -> Option<char> {
        // Without the unmodified key there is no telling whether AltGr or Option chose the
        // letter, and a letter they chose is typed as it is.
        let unmodified = self.unmodified.as_ref()?;
        if alt_shifted_the_character(&self.logical, Some(unmodified)) {
            return None;
        }
        self.latin_stand_in(sole_char(typed)?)
    }

    // The key's own character, capitalised when Shift is held. The layout's shifted symbol is
    // unknown, so Shift on a symbol key does not reach the pane.
    fn unmodified_letter(&self) -> Option<char> {
        let Some(Key::Character(plain)) = &self.unmodified else {
            return None;
        };
        let plain = sole_char(plain)?;
        if !self.modifiers.shift_key() {
            return Some(plain);
        }
        let mut capital = plain.to_uppercase();
        match (capital.next(), capital.next()) {
            (Some(capital), None) => Some(capital),
            _ => Some(plain),
        }
    }

    fn latin_stand_in(&self, character: char) -> Option<char> {
        if !character.is_alphabetic() || writes_in_latin(character) {
            return None;
        }
        let PhysicalKey::Code(code) = self.physical else {
            return None;
        };
        let shifted = if character.is_uppercase() {
            true
        } else if character.is_lowercase() {
            false
        } else {
            self.modifiers.shift_key()
        };
        us_character(code, shifted)
    }

    fn is_shortcut(&self) -> bool {
        self.modifiers.super_key() || (self.modifiers.control_key() && !self.modifiers.alt_key())
    }

    fn produced_text(&self) -> Option<String> {
        if self.is_shortcut() {
            return None;
        }
        let printable: String = self
            .text
            .as_deref()?
            .chars()
            .filter(|character| !character.is_control())
            .collect();
        (!printable.is_empty()).then_some(printable)
    }

    fn spelled(&self) -> Option<String> {
        match &self.logical {
            Key::Character(text) => Some(text.to_string()),
            Key::Named(NamedKey::Space) => Some(" ".to_owned()),
            _ => None,
        }
    }

    fn is_named_key(&self) -> bool {
        matches!(&self.logical, Key::Named(named) if *named != NamedKey::Space)
    }

    fn effective_modifiers(&self) -> ModifiersState {
        let mut modifiers = self.modifiers;
        if !self.keeps_alt && alt_shifted_the_character(&self.logical, self.unmodified.as_ref()) {
            modifiers.remove(ModifiersState::ALT);
        }
        modifiers
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadKeyOutcome {
    Wait,
    Type(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Key,
    Type(String),
    TypeThenKey(String),
    Nothing,
}

pub fn dead_key_press(press: &Press) -> DeadKeyOutcome {
    match press.produced_text() {
        Some(text) => DeadKeyOutcome::Type(text),
        None => DeadKeyOutcome::Wait,
    }
}

pub fn resolve_pending_accent(press: &Press) -> Resolution {
    if matches!(press.logical, Key::Unidentified(_)) {
        return match press.produced_text() {
            Some(text) => Resolution::Type(text),
            None => Resolution::Nothing,
        };
    }
    let Some(text) = press.produced_text() else {
        return Resolution::Key;
    };
    if press.spelled().as_deref() == Some(text.as_str()) {
        return Resolution::Key;
    }
    if press.is_named_key() {
        Resolution::TypeThenKey(text)
    } else {
        Resolution::Type(text)
    }
}

pub fn unidentified_text(press: &Press) -> Option<String> {
    match press.logical {
        Key::Unidentified(_) => press.produced_text(),
        _ => None,
    }
}

pub fn leaves_the_accent_pending(press: &Press) -> bool {
    matches!(
        press.logical,
        Key::Named(
            NamedKey::Shift
                | NamedKey::Control
                | NamedKey::Alt
                | NamedKey::AltGraph
                | NamedKey::Super
                | NamedKey::Meta
                | NamedKey::Hyper
                | NamedKey::Fn
                | NamedKey::FnLock
                | NamedKey::Symbol
                | NamedKey::SymbolLock
                | NamedKey::CapsLock
                | NamedKey::NumLock
                | NamedKey::ScrollLock
        )
    )
}

pub fn leftover_accent(press: &Press) -> Option<String> {
    let Key::Character(character) = &press.logical else {
        return None;
    };
    let text = press.produced_text()?;
    (text.len() > character.len() && text.ends_with(character.as_str())).then_some(text)
}

pub fn typed_text_message(text: String) -> Option<ClientToServerMsg> {
    key_message(&Press::new(
        Key::Character(text.as_str().into()),
        ModifiersState::empty(),
    ))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AltKeys {
    pub left: bool,
    pub right: bool,
}

impl AltKeys {
    pub fn of(modifiers: &Modifiers) -> Self {
        Self {
            left: modifiers.lalt_state() == ModifiersKeyState::Pressed,
            right: modifiers.ralt_state() == ModifiersKeyState::Pressed,
        }
    }
}

// Whether a held Alt is the Alt key even when the character is not the key's own, as with
// Shift on a symbol (Alt+Shift+= is Alt++) or AltGr on Linux.
// - Linux: AltGr is a modifier of its own rather than Alt, so Alt is always the Alt key:
//   Alt+AltGr+) on German Dvorak is Alt+].
// - Windows: AltGr arrives as Ctrl+Alt, and Alt alone picks no character, so Alt is the Alt
//   key unless Ctrl is held too.
// - macOS: Option picks characters unless it is set to act as Alt.
pub fn keeps_alt(
    platform: Platform,
    side: OptionAsAlt,
    held: AltKeys,
    modifiers: ModifiersState,
) -> bool {
    match platform {
        Platform::Linux => true,
        Platform::Windows => !modifiers.control_key(),
        Platform::MacOs => option_acts_as_alt(platform, side, held),
    }
}

fn option_acts_as_alt(platform: Platform, side: OptionAsAlt, held: AltKeys) -> bool {
    platform == Platform::MacOs
        && match side {
            OptionAsAlt::None => false,
            OptionAsAlt::Left => held.left,
            OptionAsAlt::Right => held.right,
            OptionAsAlt::Both => held.left || held.right,
        }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typed {
    Key(KeyWithModifier),
    Text(String),
}

#[cfg(any(target_os = "windows", target_os = "macos", freedesktop))]
fn unmodified_key(event: &KeyEvent) -> Option<Key> {
    use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
    Some(event.key_without_modifiers())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", freedesktop)))]
fn unmodified_key(_event: &KeyEvent) -> Option<Key> {
    None
}

pub fn alt_shifted_the_character(logical: &Key, unmodified: Option<&Key>) -> bool {
    let (Key::Character(typed), Some(Key::Character(plain))) = (logical, unmodified) else {
        return false;
    };
    typed.as_str() != plain.as_str() && typed.as_str() != plain.to_uppercase()
}

pub fn claims(keys: &[KeyWithModifier], press: &Press) -> bool {
    chord(press).is_some_and(|chord| keys.contains(&chord))
}

pub fn chord(press: &Press) -> Option<KeyWithModifier> {
    let bare_key = match &*press.reported_key() {
        Key::Character(text) => BareKey::Char(sole_char(text)?.to_ascii_lowercase()),
        Key::Named(NamedKey::Space) => BareKey::Char(' '),
        Key::Named(named) => named_key(*named)?,
        Key::Dead(_) | Key::Unidentified(_) => return None,
    };
    let modifiers = press.effective_modifiers();
    let mut key_modifiers = BTreeSet::new();
    if modifiers.control_key() {
        key_modifiers.insert(KeyModifier::Ctrl);
    }
    if modifiers.alt_key() {
        key_modifiers.insert(KeyModifier::Alt);
    }
    if modifiers.super_key() {
        key_modifiers.insert(KeyModifier::Super);
    }
    if modifiers.shift_key() {
        key_modifiers.insert(KeyModifier::Shift);
    }
    Some(KeyWithModifier::new_with_modifiers(bare_key, key_modifiers))
}

pub fn key_message(press: &Press) -> Option<ClientToServerMsg> {
    typed(press).map(|typed| match typed {
        Typed::Key(key) => key_press_message(key),
        Typed::Text(text) => text_message(text),
    })
}

pub fn key_press_message(key: KeyWithModifier) -> ClientToServerMsg {
    match key.serialize_kitty() {
        Some(kitty) => ClientToServerMsg::Key {
            key,
            raw_bytes: kitty.into_bytes(),
            is_kitty_keyboard_protocol: true,
        },
        None => {
            let raw_bytes = key
                .serialize_non_kitty()
                .map(String::into_bytes)
                .unwrap_or_default();
            ClientToServerMsg::Key {
                key,
                raw_bytes,
                is_kitty_keyboard_protocol: false,
            }
        },
    }
}

pub fn text_message(chars: String) -> ClientToServerMsg {
    ClientToServerMsg::Action {
        action: Action::WriteChars { chars },
        terminal_id: None,
        client_id: None,
        is_cli_client: false,
    }
}

pub fn typed(press: &Press) -> Option<Typed> {
    let modifiers = press.effective_modifiers();
    match &*press.reported_key() {
        Key::Character(text) => match sole_char(text) {
            Some(character) => Some(Typed::Key(with_modifiers(
                BareKey::Char(character),
                modifiers,
            ))),
            None if !text.is_empty() => Some(Typed::Text(text.to_string())),
            None => None,
        },
        Key::Named(NamedKey::Space) => {
            Some(Typed::Key(with_modifiers(BareKey::Char(' '), modifiers)))
        },
        Key::Named(named) => {
            named_key(*named).map(|bare| Typed::Key(with_modifiers(bare, modifiers)))
        },
        Key::Dead(_) | Key::Unidentified(_) => None,
    }
}

fn sole_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let character = chars.next()?;
    chars.next().is_none().then_some(character)
}

fn writes_in_latin(character: char) -> bool {
    matches!(
        character as u32,
        0x0000..=0x02AF | 0x1E00..=0x1EFF | 0x2C60..=0x2C7F | 0xA720..=0xA7FF | 0xAB30..=0xAB6F
    )
}

fn us_character(code: KeyCode, shifted: bool) -> Option<char> {
    let (plain, with_shift) = match code {
        KeyCode::Backquote => ('`', '~'),
        KeyCode::Digit1 => ('1', '!'),
        KeyCode::Digit2 => ('2', '@'),
        KeyCode::Digit3 => ('3', '#'),
        KeyCode::Digit4 => ('4', '$'),
        KeyCode::Digit5 => ('5', '%'),
        KeyCode::Digit6 => ('6', '^'),
        KeyCode::Digit7 => ('7', '&'),
        KeyCode::Digit8 => ('8', '*'),
        KeyCode::Digit9 => ('9', '('),
        KeyCode::Digit0 => ('0', ')'),
        KeyCode::Minus => ('-', '_'),
        KeyCode::Equal => ('=', '+'),
        KeyCode::BracketLeft => ('[', '{'),
        KeyCode::BracketRight => (']', '}'),
        KeyCode::Backslash => ('\\', '|'),
        KeyCode::Semicolon => (';', ':'),
        KeyCode::Quote => ('\'', '"'),
        KeyCode::Comma => (',', '<'),
        KeyCode::Period => ('.', '>'),
        KeyCode::Slash => ('/', '?'),
        KeyCode::KeyA => ('a', 'A'),
        KeyCode::KeyB => ('b', 'B'),
        KeyCode::KeyC => ('c', 'C'),
        KeyCode::KeyD => ('d', 'D'),
        KeyCode::KeyE => ('e', 'E'),
        KeyCode::KeyF => ('f', 'F'),
        KeyCode::KeyG => ('g', 'G'),
        KeyCode::KeyH => ('h', 'H'),
        KeyCode::KeyI => ('i', 'I'),
        KeyCode::KeyJ => ('j', 'J'),
        KeyCode::KeyK => ('k', 'K'),
        KeyCode::KeyL => ('l', 'L'),
        KeyCode::KeyM => ('m', 'M'),
        KeyCode::KeyN => ('n', 'N'),
        KeyCode::KeyO => ('o', 'O'),
        KeyCode::KeyP => ('p', 'P'),
        KeyCode::KeyQ => ('q', 'Q'),
        KeyCode::KeyR => ('r', 'R'),
        KeyCode::KeyS => ('s', 'S'),
        KeyCode::KeyT => ('t', 'T'),
        KeyCode::KeyU => ('u', 'U'),
        KeyCode::KeyV => ('v', 'V'),
        KeyCode::KeyW => ('w', 'W'),
        KeyCode::KeyX => ('x', 'X'),
        KeyCode::KeyY => ('y', 'Y'),
        KeyCode::KeyZ => ('z', 'Z'),
        _ => return None,
    };
    Some(if shifted { with_shift } else { plain })
}

fn with_modifiers(bare_key: BareKey, modifiers: ModifiersState) -> KeyWithModifier {
    let mut key_modifiers = BTreeSet::new();
    if modifiers.control_key() {
        key_modifiers.insert(KeyModifier::Ctrl);
    }
    if modifiers.alt_key() {
        key_modifiers.insert(KeyModifier::Alt);
    }
    if modifiers.super_key() {
        key_modifiers.insert(KeyModifier::Super);
    }
    if modifiers.shift_key() && !matches!(bare_key, BareKey::Char(_)) {
        key_modifiers.insert(KeyModifier::Shift);
    }
    KeyWithModifier::new_with_modifiers(bare_key, key_modifiers)
}

fn named_key(named: NamedKey) -> Option<BareKey> {
    let bare = match named {
        NamedKey::ArrowLeft => BareKey::Left,
        NamedKey::ArrowRight => BareKey::Right,
        NamedKey::ArrowUp => BareKey::Up,
        NamedKey::ArrowDown => BareKey::Down,
        NamedKey::Home => BareKey::Home,
        NamedKey::End => BareKey::End,
        NamedKey::PageUp => BareKey::PageUp,
        NamedKey::PageDown => BareKey::PageDown,
        NamedKey::Backspace => BareKey::Backspace,
        NamedKey::Delete => BareKey::Delete,
        NamedKey::Insert => BareKey::Insert,
        NamedKey::Enter => BareKey::Enter,
        NamedKey::Tab => BareKey::Tab,
        NamedKey::Escape => BareKey::Esc,
        NamedKey::CapsLock => BareKey::CapsLock,
        NamedKey::ScrollLock => BareKey::ScrollLock,
        NamedKey::NumLock => BareKey::NumLock,
        NamedKey::PrintScreen => BareKey::PrintScreen,
        NamedKey::Pause => BareKey::Pause,
        NamedKey::ContextMenu => BareKey::Menu,
        NamedKey::F1 => BareKey::F(1),
        NamedKey::F2 => BareKey::F(2),
        NamedKey::F3 => BareKey::F(3),
        NamedKey::F4 => BareKey::F(4),
        NamedKey::F5 => BareKey::F(5),
        NamedKey::F6 => BareKey::F(6),
        NamedKey::F7 => BareKey::F(7),
        NamedKey::F8 => BareKey::F(8),
        NamedKey::F9 => BareKey::F(9),
        NamedKey::F10 => BareKey::F(10),
        NamedKey::F11 => BareKey::F(11),
        NamedKey::F12 => BareKey::F(12),
        _ => return None,
    };
    Some(bare)
}

#[cfg(test)]
pub(crate) mod test_helpers;

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use super::*;
    use winit::keyboard::{NativeKey, SmolStr};

    use crate::platform::Platform;
    use crate::session::connection::{send, test_attach_at as attach_at, Capabilities, Geometry};
    use crate::testing::test_server::FakeServer;

    fn character(text: &str) -> Key {
        Key::Character(SmolStr::new(text))
    }

    fn none() -> ModifiersState {
        ModifiersState::empty()
    }

    fn press(logical: Key, modifiers: ModifiersState) -> Press {
        Press::new(logical, modifiers)
    }

    fn layered(logical: &str, unmodified: &str, modifiers: ModifiersState) -> Press {
        Press {
            unmodified: Some(character(unmodified)),
            ..press(character(logical), modifiers)
        }
    }

    fn on_key(logical: &str, unmodified: &str, code: KeyCode, modifiers: ModifiersState) -> Press {
        Press {
            physical: PhysicalKey::Code(code),
            ..layered(logical, unmodified, modifiers)
        }
    }

    // What Windows reports for a key that Ctrl+Alt, being AltGr, types nothing on.
    fn unmapped(unmodified: &str, code: KeyCode, modifiers: ModifiersState) -> Press {
        Press {
            logical: unidentified(),
            ..on_key(unmodified, unmodified, code, modifiers)
        }
    }

    // Escapes, because Cyrillic "с" and Latin "c" look the same.
    const RU_ES: &str = "\u{441}"; // с, on the C key
    const RU_ES_CAPITAL: &str = "\u{421}"; // С
    const RU_PE: &str = "\u{43f}"; // п, on the G key
    const RU_TE: &str = "\u{442}"; // т, on the N key
    const RU_EM: &str = "\u{43c}"; // м, on the V key
    const RU_EM_CAPITAL: &str = "\u{41c}"; // М
    const RU_HA: &str = "\u{445}"; // х, on the [ key
    const RU_DE: &str = "\u{434}"; // д, on the L key
    const UK_GHE: &str = "\u{433}"; // г, on the U key
    const UK_GHE_UPTURN: &str = "\u{491}"; // ґ, AltGr+г on the Ukrainian layout
    const HE_SHIN: &str = "\u{5e9}"; // ש, on the A key

    fn converted(logical: &str, unmodified: &str, modifiers: ModifiersState) -> Press {
        Press {
            keeps_alt: true,
            ..layered(logical, unmodified, modifiers)
        }
    }

    fn sent(press: &Press) -> Vec<u8> {
        match key_message(press) {
            Some(ClientToServerMsg::Key { raw_bytes, .. }) => raw_bytes,
            other => panic!("expected a key message, got {:?}", other),
        }
    }

    fn bytes(logical: &Key, modifiers: ModifiersState) -> Vec<u8> {
        match key_message(&press(logical.clone(), modifiers)) {
            Some(ClientToServerMsg::Key { raw_bytes, .. }) => raw_bytes,
            other => panic!("expected a key message, got {:?}", other),
        }
    }

    fn translated(logical: &Key, modifiers: ModifiersState) -> KeyWithModifier {
        translate(&press(logical.clone(), modifiers)).expect("expected a translated key")
    }

    #[test]
    fn control_on_a_non_latin_letter_is_the_us_key_in_the_same_place() {
        let pressed = on_key(RU_ES, RU_ES, KeyCode::KeyC, ModifiersState::CONTROL);
        assert_eq!(
            translate(&pressed),
            Some(KeyWithModifier::new(BareKey::Char('c')).with_ctrl_modifier())
        );
        assert_eq!(sent(&pressed), b"\x1b[99;5u");
        assert_eq!(
            translate(&pressed)
                .and_then(|key| key.serialize_non_kitty())
                .as_deref(),
            Some("\u{3}"),
            "a pane without the kitty protocol must get ETX rather than the letter"
        );
    }

    #[test]
    fn a_non_latin_layout_reaches_the_latin_bindings() {
        assert_eq!(
            translate(&on_key(
                RU_PE,
                RU_PE,
                KeyCode::KeyG,
                ModifiersState::CONTROL
            )),
            Some(KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier()),
            "Ctrl+g locks the interface whatever the layout"
        );
        assert_eq!(
            translate(&on_key(RU_TE, RU_TE, KeyCode::KeyN, ModifiersState::ALT)),
            Some(KeyWithModifier::new(BareKey::Char('n')).with_alt_modifier()),
            "Alt+n opens a pane whatever the layout"
        );
        let both = ModifiersState::CONTROL | ModifiersState::SHIFT;
        let paste = vec![KeyWithModifier::new(BareKey::Char('v'))
            .with_ctrl_modifier()
            .with_shift_modifier()];
        assert!(claims(
            &paste,
            &on_key(RU_EM_CAPITAL, RU_EM, KeyCode::KeyV, both)
        ));
    }

    #[test]
    fn command_on_a_non_latin_letter_copies_and_pastes() {
        let cmd = |key| KeyWithModifier::new(BareKey::Char(key)).with_super_modifier();
        let command = ModifiersState::SUPER;
        assert!(
            claims(&[cmd('v')], &on_key(RU_EM, RU_EM, KeyCode::KeyV, command)),
            "Cmd+V pastes on a Russian layout"
        );
        assert!(
            claims(&[cmd('c')], &on_key(RU_ES, RU_ES, KeyCode::KeyC, command)),
            "Cmd+C copies on a Russian layout"
        );
        let with_shift = command | ModifiersState::SHIFT;
        assert_eq!(
            translate(&on_key(RU_EM_CAPITAL, RU_EM, KeyCode::KeyV, with_shift)),
            translate(&layered("V", "v", with_shift)),
            "the same key on a US layout"
        );
    }

    #[test]
    fn shift_on_a_non_latin_letter_is_shift_on_the_us_key() {
        let both = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert_eq!(
            translate(&on_key(RU_ES_CAPITAL, RU_ES, KeyCode::KeyC, both)),
            translate(&layered("C", "c", both)),
            "the same key on a US layout"
        );
        assert_eq!(
            translate(&on_key(
                HE_SHIN,
                HE_SHIN,
                KeyCode::KeyA,
                ModifiersState::CONTROL
            )),
            Some(KeyWithModifier::new(BareKey::Char('a')).with_ctrl_modifier()),
            "a script without case takes its level from Shift"
        );
        assert_eq!(
            translate(&on_key(HE_SHIN, HE_SHIN, KeyCode::KeyA, both)),
            translate(&layered("A", "a", both))
        );
    }

    #[test]
    fn a_non_latin_letter_off_the_letter_keys_is_the_us_symbol() {
        let pressed = on_key(RU_HA, RU_HA, KeyCode::BracketLeft, ModifiersState::CONTROL);
        assert_eq!(
            translate(&pressed)
                .and_then(|key| key.serialize_non_kitty())
                .as_deref(),
            Some("\u{1b}"),
            "Ctrl+[ is Escape"
        );
    }

    #[test]
    fn a_latin_layout_keeps_its_own_letters_under_control() {
        assert_eq!(
            translate(&on_key("a", "a", KeyCode::KeyQ, ModifiersState::CONTROL)),
            Some(KeyWithModifier::new(BareKey::Char('a')).with_ctrl_modifier()),
            "AZERTY's A key sits where QWERTY has Q"
        );
        assert_eq!(
            translate(&on_key("é", "é", KeyCode::Digit2, ModifiersState::CONTROL)),
            Some(KeyWithModifier::new(BareKey::Char('é')).with_ctrl_modifier()),
            "an accented Latin letter is not a stand-in for the 2 key"
        );
    }

    #[test]
    fn a_non_latin_letter_without_control_alt_or_super_is_typed_as_itself() {
        assert_eq!(
            sent(&on_key(RU_ES, RU_ES, KeyCode::KeyC, none())),
            RU_ES.as_bytes()
        );
        assert_eq!(
            sent(&on_key(
                RU_ES_CAPITAL,
                RU_ES,
                KeyCode::KeyC,
                ModifiersState::SHIFT
            )),
            RU_ES_CAPITAL.as_bytes()
        );
    }

    #[test]
    fn a_letter_the_modifiers_chose_is_not_replaced() {
        let altgr = ModifiersState::CONTROL | ModifiersState::ALT;
        let pressed = on_key(UK_GHE_UPTURN, UK_GHE, KeyCode::KeyU, altgr);
        assert_eq!(
            translate(&pressed).map(|key| key.bare_key),
            Some(BareKey::Char('\u{491}')),
            "AltGr, which Windows reports as Ctrl+Alt, typed another letter"
        );
        let unknown = Press {
            unmodified: None,
            ..on_key(RU_ES, RU_ES, KeyCode::KeyC, ModifiersState::CONTROL)
        };
        assert_eq!(
            translate(&unknown).map(|key| key.bare_key),
            Some(BareKey::Char('\u{441}')),
            "without the unmodified key there is no telling whether AltGr chose the letter"
        );
    }

    #[test]
    fn ctrl_alt_on_a_key_altgr_types_nothing_on_is_a_chord() {
        let ctrl_alt = ModifiersState::CONTROL | ModifiersState::ALT;
        let pressed = unmapped("l", KeyCode::KeyL, ctrl_alt);
        let chord = KeyWithModifier::new(BareKey::Char('l'))
            .with_ctrl_modifier()
            .with_alt_modifier();
        assert_eq!(translate(&pressed), Some(chord.clone()));
        assert_eq!(sent(&pressed), b"\x1b[108;7u");
        assert_eq!(
            translate(&pressed)
                .and_then(|key| key.serialize_non_kitty())
                .as_deref(),
            Some("\u{1b}\u{c}"),
            "a pane without the kitty protocol gets Escape then Ctrl+L"
        );
        assert!(claims(&[chord], &pressed));
    }

    #[test]
    fn ctrl_alt_on_a_non_latin_layout_is_a_chord_on_the_us_key() {
        let ctrl_alt = ModifiersState::CONTROL | ModifiersState::ALT;
        assert_eq!(
            translate(&unmapped(RU_DE, KeyCode::KeyL, ctrl_alt)),
            translate(&unmapped("l", KeyCode::KeyL, ctrl_alt))
        );
    }

    #[test]
    fn shift_on_a_ctrl_alt_chord_capitalises_its_letter() {
        let all = ModifiersState::CONTROL | ModifiersState::ALT | ModifiersState::SHIFT;
        let expected = translate(&layered("L", "l", all));
        assert_eq!(translate(&unmapped("l", KeyCode::KeyL, all)), expected);
        assert_eq!(translate(&unmapped(RU_DE, KeyCode::KeyL, all)), expected);
    }

    #[test]
    fn a_character_ctrl_alt_types_as_altgr_is_still_typed() {
        let ctrl_alt = ModifiersState::CONTROL | ModifiersState::ALT;
        assert_eq!(
            translate(&on_key("€", "e", KeyCode::KeyE, ctrl_alt)).map(|key| key.bare_key),
            Some(BareKey::Char('€')),
            "AZERTY's Ctrl+Alt+E is AltGr+E"
        );
    }

    #[test]
    fn an_unidentified_key_is_a_chord_only_under_ctrl_alt_or_super_and_without_text() {
        let ctrl_alt = ModifiersState::CONTROL | ModifiersState::ALT;
        assert_eq!(translate(&unmapped("l", KeyCode::KeyL, none())), None);
        assert_eq!(
            translate(&unmapped("l", KeyCode::KeyL, ModifiersState::SHIFT)),
            None
        );
        let picked = unmapped("l", KeyCode::KeyL, ctrl_alt).with_text("′");
        assert_eq!(
            unidentified_text(&picked),
            Some("′".to_owned()),
            "text that arrived with the key is typed"
        );
        assert_eq!(translate(&picked), None);
        let unknown = Press {
            unmodified: None,
            ..unmapped("l", KeyCode::KeyL, ctrl_alt)
        };
        assert_eq!(
            translate(&unknown),
            None,
            "without the key's own letter there is no chord to report"
        );
    }

    #[test]
    fn a_plain_character_carries_no_modifiers_and_its_own_byte() {
        let key = translated(&character("a"), none());
        assert_eq!(key.bare_key, BareKey::Char('a'));
        assert!(key.key_modifiers.is_empty());
        assert_eq!(bytes(&character("a"), none()), b"a");
    }

    #[test]
    fn shift_is_absorbed_into_the_character_rather_than_reported() {
        let key = translated(&character("A"), ModifiersState::SHIFT);
        assert_eq!(key.bare_key, BareKey::Char('A'));
        assert!(key.key_modifiers.is_empty());
        assert_eq!(bytes(&character("A"), ModifiersState::SHIFT), b"A");
    }

    #[test]
    fn an_absorbed_shift_still_matches_the_keybinding_form() {
        assert_eq!(
            translated(&character("A"), ModifiersState::SHIFT),
            KeyWithModifier::new(BareKey::Char('a')).with_shift_modifier()
        );
    }

    #[test]
    fn control_survives_and_encodes_as_a_kitty_sequence() {
        let key = translated(&character("c"), ModifiersState::CONTROL);
        assert_eq!(key.bare_key, BareKey::Char('c'));
        assert!(key.key_modifiers.contains(&KeyModifier::Ctrl));
        assert_eq!(
            bytes(&character("c"), ModifiersState::CONTROL),
            b"\x1b[99;5u"
        );
    }

    #[test]
    fn control_with_shift_is_reported_with_its_shift() {
        assert_eq!(
            bytes(
                &character("C"),
                ModifiersState::CONTROL | ModifiersState::SHIFT
            ),
            b"\x1b[99;6u"
        );
    }

    #[test]
    fn alt_encodes_as_a_kitty_sequence() {
        assert_eq!(bytes(&character("a"), ModifiersState::ALT), b"\x1b[97;3u");
    }

    #[test]
    fn super_survives_translation() {
        let key = translated(&character("p"), ModifiersState::SUPER);
        assert!(key.key_modifiers.contains(&KeyModifier::Super));
    }

    #[test]
    fn the_named_keys_map_onto_bare_keys() {
        let cases = [
            (NamedKey::ArrowLeft, BareKey::Left),
            (NamedKey::ArrowRight, BareKey::Right),
            (NamedKey::ArrowUp, BareKey::Up),
            (NamedKey::ArrowDown, BareKey::Down),
            (NamedKey::Home, BareKey::Home),
            (NamedKey::End, BareKey::End),
            (NamedKey::PageUp, BareKey::PageUp),
            (NamedKey::PageDown, BareKey::PageDown),
            (NamedKey::Backspace, BareKey::Backspace),
            (NamedKey::Delete, BareKey::Delete),
            (NamedKey::Insert, BareKey::Insert),
            (NamedKey::Enter, BareKey::Enter),
            (NamedKey::Tab, BareKey::Tab),
            (NamedKey::Escape, BareKey::Esc),
            (NamedKey::Space, BareKey::Char(' ')),
            (NamedKey::F1, BareKey::F(1)),
            (NamedKey::F12, BareKey::F(12)),
        ];
        for (named, expected) in cases {
            assert_eq!(translated(&Key::Named(named), none()).bare_key, expected);
        }
    }

    #[test]
    fn the_named_keys_encode_the_sequences_applications_expect() {
        let cases: [(NamedKey, &[u8]); 8] = [
            (NamedKey::Enter, b"\r"),
            (NamedKey::Tab, b"\t"),
            (NamedKey::Escape, b"\x1b[27u"),
            (NamedKey::Backspace, b"\x7f"),
            (NamedKey::ArrowUp, b"\x1b[A"),
            (NamedKey::ArrowDown, b"\x1b[B"),
            (NamedKey::ArrowRight, b"\x1b[C"),
            (NamedKey::ArrowLeft, b"\x1b[D"),
        ];
        for (named, expected) in cases {
            assert_eq!(bytes(&Key::Named(named), none()), expected, "{:?}", named);
        }
    }

    #[test]
    fn shift_is_retained_on_named_keys() {
        let key = translated(&Key::Named(NamedKey::Tab), ModifiersState::SHIFT);
        assert_eq!(key.bare_key, BareKey::Tab);
        assert!(key.key_modifiers.contains(&KeyModifier::Shift));
        assert_eq!(
            bytes(&Key::Named(NamedKey::Tab), ModifiersState::SHIFT),
            b"\x1b[9;2u"
        );
    }

    #[test]
    fn bare_modifier_presses_produce_nothing() {
        for named in [
            NamedKey::Shift,
            NamedKey::Control,
            NamedKey::Alt,
            NamedKey::Super,
            NamedKey::Meta,
            NamedKey::AltGraph,
            NamedKey::Hyper,
            NamedKey::Fn,
        ] {
            assert!(
                translate(&press(Key::Named(named), none())).is_none(),
                "{:?}",
                named
            );
        }
    }

    #[test]
    fn unmapped_and_composing_keys_produce_nothing() {
        assert!(typed(&press(Key::Dead(Some('`')), none())).is_none());
        assert!(typed(&press(Key::Named(NamedKey::BrightnessUp), none())).is_none());
        assert!(typed(&press(Key::Unidentified(NativeKey::Unidentified), none())).is_none());
    }

    fn unidentified() -> Key {
        Key::Unidentified(NativeKey::Unidentified)
    }

    #[test]
    fn a_dead_key_without_text_waits_for_the_next_key() {
        assert_eq!(
            dead_key_press(&press(Key::Dead(Some('^')), none())),
            DeadKeyOutcome::Wait
        );
    }

    #[test]
    fn a_dead_key_pressed_twice_types_what_the_system_produced() {
        assert_eq!(
            dead_key_press(&press(Key::Dead(Some('^')), none()).with_text("^^")),
            DeadKeyOutcome::Type("^^".to_owned()),
            "windows reports the second press as a dead key carrying both accents"
        );
        assert_eq!(
            dead_key_press(&press(Key::Dead(Some('^')), none()).with_text("^")),
            DeadKeyOutcome::Type("^".to_owned())
        );
    }

    #[test]
    fn a_dead_key_with_a_shortcut_modifier_still_waits() {
        assert_eq!(
            dead_key_press(&press(Key::Dead(Some('^')), ModifiersState::CONTROL).with_text("^")),
            DeadKeyOutcome::Wait
        );
    }

    #[test]
    fn space_after_a_dead_key_types_the_accent_rather_than_a_space() {
        let pressed = press(Key::Named(NamedKey::Space), none()).with_text("^");
        assert_eq!(
            resolve_pending_accent(&pressed),
            Resolution::Type("^".to_owned())
        );
    }

    #[test]
    fn a_letter_the_accent_cannot_combine_with_types_both() {
        let pressed = press(character("x"), none()).with_text("^x");
        assert_eq!(
            resolve_pending_accent(&pressed),
            Resolution::Type("^x".to_owned()),
            "windows reports the plain letter as the key and both characters as the text"
        );
    }

    #[test]
    fn a_combined_letter_is_delivered_as_its_own_key() {
        let pressed = press(character("ê"), none()).with_text("ê");
        assert_eq!(resolve_pending_accent(&pressed), Resolution::Key);
        assert_eq!(
            resolve_pending_accent(&press(character("ê"), none())),
            Resolution::Key
        );
    }

    #[test]
    fn a_named_key_after_a_dead_key_types_the_accent_then_acts() {
        let pressed = press(Key::Named(NamedKey::Enter), none()).with_text("^\r");
        assert_eq!(
            resolve_pending_accent(&pressed),
            Resolution::TypeThenKey("^".to_owned()),
            "the control character in the text belongs to the key and must not be typed twice"
        );
        assert_eq!(
            resolve_pending_accent(&press(Key::Named(NamedKey::Enter), none())),
            Resolution::Key
        );
    }

    #[test]
    fn a_shortcut_after_a_dead_key_is_delivered_as_a_shortcut() {
        let pressed = press(character("c"), ModifiersState::CONTROL).with_text("^c");
        assert_eq!(resolve_pending_accent(&pressed), Resolution::Key);
        let pressed = press(character("c"), ModifiersState::SUPER).with_text("^c");
        assert_eq!(resolve_pending_accent(&pressed), Resolution::Key);
    }

    #[test]
    fn altgr_reported_as_control_and_alt_still_types() {
        let pressed = press(
            character("x"),
            ModifiersState::CONTROL | ModifiersState::ALT,
        )
        .with_text("^x");
        assert_eq!(
            resolve_pending_accent(&pressed),
            Resolution::Type("^x".to_owned())
        );
    }

    #[test]
    fn a_cancelled_sequence_with_nothing_produced_sends_nothing() {
        assert_eq!(
            resolve_pending_accent(&press(unidentified(), none())),
            Resolution::Nothing,
            "linux cancels an accent that cannot combine and reports no text"
        );
    }

    #[test]
    fn an_unidentified_key_with_text_is_typed() {
        assert_eq!(
            unidentified_text(&press(unidentified(), none()).with_text("′")),
            Some("′".to_owned()),
            "a character picker or the emoji panel delivers its character this way"
        );
        assert_eq!(unidentified_text(&press(unidentified(), none())), None);
        assert_eq!(
            unidentified_text(&press(character("a"), none()).with_text("a")),
            None
        );
        assert_eq!(
            unidentified_text(&press(unidentified(), none()).with_text("\u{7}")),
            None,
            "a lone control character is not text"
        );
    }

    #[test]
    fn modifier_keys_leave_a_pending_accent_alone() {
        for named in [
            NamedKey::Shift,
            NamedKey::Control,
            NamedKey::Alt,
            NamedKey::AltGraph,
            NamedKey::Super,
            NamedKey::CapsLock,
        ] {
            assert!(
                leaves_the_accent_pending(&press(Key::Named(named), none())),
                "{:?}",
                named
            );
        }
        for key in [
            character("p"),
            Key::Named(NamedKey::Space),
            Key::Named(NamedKey::Enter),
            Key::Named(NamedKey::ArrowLeft),
            Key::Dead(Some('^')),
            unidentified(),
        ] {
            assert!(
                !leaves_the_accent_pending(&press(key.clone(), none())),
                "{:?}",
                key
            );
        }
    }

    #[test]
    fn an_accent_the_system_still_held_is_typed_with_its_character() {
        assert_eq!(
            leftover_accent(&press(character("/"), ModifiersState::SHIFT).with_text("^/")),
            Some("^/".to_owned()),
            "windows keeps a dead key through shift and reports it with the next character"
        );
        assert_eq!(
            leftover_accent(&press(character("\\"), none()).with_text("^\\")),
            Some("^\\".to_owned())
        );
        assert_eq!(
            leftover_accent(
                &press(
                    character("\\"),
                    ModifiersState::CONTROL | ModifiersState::ALT
                )
                .with_text("^\\")
            ),
            Some("^\\".to_owned()),
            "altgr reported as control and alt"
        );
    }

    #[test]
    fn a_character_that_spells_its_text_carries_no_accent() {
        for pressed in [
            press(character("p"), none()).with_text("p"),
            press(character("ê"), none()).with_text("ê"),
            press(character("ab"), none()).with_text("ab"),
            press(character("p"), none()),
            press(character("c"), ModifiersState::CONTROL).with_text("^c"),
            press(Key::Named(NamedKey::Enter), none()).with_text("^\r"),
            press(unidentified(), none()).with_text("^p"),
        ] {
            assert_eq!(leftover_accent(&pressed), None, "{:?}", pressed);
        }
    }

    #[test]
    fn typed_text_is_a_key_for_one_character_and_text_for_more() {
        assert_eq!(
            typed_text_message("^".to_owned()),
            key_message(&press(character("^"), none()))
        );
        assert_eq!(
            typed_text_message("^x".to_owned()),
            Some(text_message("^x".to_owned()))
        );
    }

    #[test]
    fn a_multi_character_key_is_delivered_as_its_text() {
        assert_eq!(
            typed(&press(character("ab"), none())),
            Some(Typed::Text("ab".to_owned())),
            "a compose result must reach the pane rather than being dropped"
        );
        assert_eq!(
            typed(&press(character("é\u{301}"), none())),
            Some(Typed::Text("é\u{301}".to_owned()))
        );
        assert_eq!(
            translate(&press(character("ab"), none())),
            None,
            "a multi-character press has no single key to carry it"
        );
    }

    #[test]
    fn multi_character_text_is_typed_rather_than_pasted() {
        assert_eq!(
            key_message(&press(character("ab"), none())),
            Some(text_message("ab".to_owned())),
            "a key the layout spelled with more than one character is typed text"
        );
        assert_ne!(
            key_message(&press(character("ab"), none())),
            Some(crate::input::clipboard::paste_message("ab".to_owned())),
            "the paste path would wrap it in bracketed-paste markers"
        );
    }

    #[test]
    fn typed_text_carries_no_bracketed_paste_markers() {
        match text_message("你好".to_owned()) {
            ClientToServerMsg::Action {
                action: Action::WriteChars { chars },
                ..
            } => {
                assert_eq!(chars, "你好");
                assert!(!chars.contains('\u{1b}'));
            },
            other => panic!("expected typed text, got {:?}", other),
        }
    }

    #[test]
    fn an_empty_character_press_produces_nothing() {
        assert_eq!(typed(&press(character(""), none())), None);
    }

    #[test]
    fn a_third_level_character_arrives_as_itself_rather_than_as_an_alt_chord() {
        let pressed = layered("@", "q", ModifiersState::ALT);
        assert_eq!(
            translate(&pressed),
            Some(KeyWithModifier::new(BareKey::Char('@'))),
            "the layout spent the modifier on the character; reporting Alt would escape-prefix it"
        );
        assert_eq!(
            key_message(&pressed),
            key_message(&press(character("@"), none())),
            "an AltGr-typed @ must be indistinguishable from a plain one"
        );
    }

    #[test]
    fn a_third_level_character_does_not_claim_an_alt_chord_either() {
        let keys = vec![KeyWithModifier::new(BareKey::Char('q')).with_alt_modifier()];
        assert!(!claims(&keys, &layered("@", "q", ModifiersState::ALT)));
        assert!(claims(&keys, &layered("q", "q", ModifiersState::ALT)));
    }

    // A press with Alt kept or dropped as `platform` does.
    fn on(platform: Platform, logical: &str, unmodified: &str, modifiers: ModifiersState) -> Press {
        Press {
            keeps_alt: keeps_alt(platform, OptionAsAlt::None, AltKeys::default(), modifiers),
            ..layered(logical, unmodified, modifiers)
        }
    }

    #[test]
    fn alt_with_altgr_on_linux_is_alt_on_the_altgr_character() {
        // German Dvorak types ] with AltGr on the ) key, and Linux does not report AltGr as Alt.
        let alt_bracket = KeyWithModifier::new(BareKey::Char(']')).with_alt_modifier();
        let pressed = on(Platform::Linux, "]", ")", ModifiersState::ALT);
        assert_eq!(translate(&pressed), Some(alt_bracket.clone()));
        assert_eq!(sent(&pressed), b"\x1b[93;3u");
        assert_eq!(
            translate(&pressed)
                .and_then(|key| key.serialize_non_kitty())
                .as_deref(),
            Some("\u{1b}]"),
            "a pane without the kitty protocol gets Escape then ]"
        );
        assert!(
            claims(&[alt_bracket], &pressed),
            "Alt+] reaches the bindings"
        );
        assert_eq!(
            key_message(&on(Platform::Linux, "]", ")", none())),
            key_message(&press(character("]"), none())),
            "AltGr alone still types ]"
        );
    }

    #[test]
    fn alt_with_shift_on_a_symbol_is_alt_on_the_shifted_symbol() {
        let both = ModifiersState::ALT | ModifiersState::SHIFT;
        for platform in [Platform::Linux, Platform::Windows] {
            let pressed = on(platform, "+", "=", both);
            assert_eq!(
                translate(&pressed),
                Some(KeyWithModifier::new(BareKey::Char('+')).with_alt_modifier()),
                "{:?}: Alt+Shift+= on a US layout is Alt++, a default binding",
                platform
            );
            assert_eq!(sent(&pressed), b"\x1b[43;3u");
        }
    }

    #[test]
    fn altgr_on_windows_still_types_its_character() {
        let ctrl_alt = ModifiersState::CONTROL | ModifiersState::ALT;
        let key = translate(&on(Platform::Windows, "€", "e", ctrl_alt)).expect("expected a key");
        assert_eq!(key.bare_key, BareKey::Char('€'));
        assert!(
            !key.key_modifiers.contains(&KeyModifier::Alt),
            "AltGr+E is not Alt+€"
        );
    }

    #[test]
    fn each_platform_keeps_alt_where_alt_picks_no_character() {
        let sides = [
            OptionAsAlt::None,
            OptionAsAlt::Left,
            OptionAsAlt::Right,
            OptionAsAlt::Both,
        ];
        let held = [
            AltKeys::default(),
            AltKeys {
                left: true,
                right: false,
            },
            AltKeys {
                left: false,
                right: true,
            },
            AltKeys {
                left: true,
                right: true,
            },
        ];
        let alt = ModifiersState::ALT;
        let alt_shift = ModifiersState::ALT | ModifiersState::SHIFT;
        let ctrl_alt = ModifiersState::CONTROL | ModifiersState::ALT;
        for side in sides {
            for held in held {
                for modifiers in [alt, alt_shift, ctrl_alt] {
                    assert!(keeps_alt(Platform::Linux, side, held, modifiers));
                    assert_eq!(
                        keeps_alt(Platform::MacOs, side, held, modifiers),
                        option_acts_as_alt(Platform::MacOs, side, held),
                        "{:?} with {:?}",
                        side,
                        held
                    );
                }
                assert!(keeps_alt(Platform::Windows, side, held, alt));
                assert!(keeps_alt(Platform::Windows, side, held, alt_shift));
                assert!(
                    !keeps_alt(Platform::Windows, side, held, ctrl_alt),
                    "AltGr arrives as Ctrl+Alt on Windows"
                );
            }
        }
    }

    #[test]
    fn an_alt_that_the_layout_did_not_spend_is_still_reported() {
        for (logical, unmodified, modifiers) in [
            ("a", "a", ModifiersState::ALT),
            ("A", "a", ModifiersState::ALT | ModifiersState::SHIFT),
        ] {
            let pressed = layered(logical, unmodified, modifiers);
            assert!(
                translate(&pressed)
                    .expect("expected a key")
                    .key_modifiers
                    .contains(&KeyModifier::Alt),
                "{} lost its Alt",
                logical
            );
        }
        assert_eq!(bytes(&character("a"), ModifiersState::ALT), b"\x1b[97;3u");
    }

    #[test]
    fn an_option_key_converted_to_alt_is_an_alt_chord() {
        let pressed = converted("h", "h", ModifiersState::ALT);
        assert_eq!(
            translate(&pressed),
            Some(KeyWithModifier::new(BareKey::Char('h')).with_alt_modifier())
        );
        assert_eq!(sent(&pressed), b"\x1b[104;3u");
    }

    #[test]
    fn a_converted_option_keeps_its_alt_on_a_shifted_symbol() {
        let both = ModifiersState::ALT | ModifiersState::SHIFT;
        let pressed = converted("+", "=", both);
        assert_eq!(
            translate(&pressed),
            Some(KeyWithModifier::new(BareKey::Char('+')).with_alt_modifier()),
            "the conversion leaves Shift in the character; that is not Option being spent"
        );
        assert_eq!(sent(&pressed), b"\x1b[43;3u");
        let keys = vec![KeyWithModifier::new(BareKey::Char('+'))
            .with_alt_modifier()
            .with_shift_modifier()];
        assert!(claims(&keys, &pressed));
    }

    #[test]
    fn an_option_key_left_typing_characters_is_not_an_alt_chord() {
        let both = ModifiersState::ALT | ModifiersState::SHIFT;
        for (logical, unmodified, modifiers) in [("˙", "h", ModifiersState::ALT), ("±", "=", both)]
        {
            let pressed = layered(logical, unmodified, modifiers);
            assert_eq!(
                key_message(&pressed),
                key_message(&press(character(logical), none())),
                "{} must arrive as plain text",
                logical
            );
        }
    }

    #[test]
    fn only_the_configured_option_key_acts_as_alt_and_only_on_macos() {
        let left = AltKeys {
            left: true,
            right: false,
        };
        let right = AltKeys {
            left: false,
            right: true,
        };
        let both = AltKeys {
            left: true,
            right: true,
        };
        let cases = [
            (OptionAsAlt::Left, left, true),
            (OptionAsAlt::Left, right, false),
            (OptionAsAlt::Left, both, true),
            (OptionAsAlt::Right, left, false),
            (OptionAsAlt::Right, right, true),
            (OptionAsAlt::Right, both, true),
            (OptionAsAlt::Both, left, true),
            (OptionAsAlt::Both, right, true),
            (OptionAsAlt::Both, both, true),
            (OptionAsAlt::Both, AltKeys::default(), false),
            (OptionAsAlt::None, left, false),
            (OptionAsAlt::None, right, false),
            (OptionAsAlt::None, both, false),
        ];
        for (side, held, expected) in cases {
            assert_eq!(
                option_acts_as_alt(Platform::MacOs, side, held),
                expected,
                "{:?} with {:?}",
                side,
                held
            );
            for platform in [Platform::Linux, Platform::Windows] {
                assert!(
                    !option_acts_as_alt(platform, side, held),
                    "{:?} has no Option key to convert",
                    platform
                );
            }
        }
    }

    #[test]
    fn a_shift_level_is_never_mistaken_for_an_alt_level() {
        assert!(!alt_shifted_the_character(
            &character("A"),
            Some(&character("a"))
        ));
        assert!(!alt_shifted_the_character(
            &character("a"),
            Some(&character("a"))
        ));
        assert!(alt_shifted_the_character(
            &character("œ"),
            Some(&character("q"))
        ));
        assert!(alt_shifted_the_character(
            &character("Å"),
            Some(&character("a"))
        ));
    }

    #[test]
    fn a_platform_that_reports_no_unmodified_key_behaves_exactly_as_before() {
        let pressed = Press {
            logical: character("@"),
            unmodified: None,
            physical: PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
            modifiers: ModifiersState::ALT,
            keeps_alt: false,
            text: None,
        };
        assert_eq!(
            translate(&pressed),
            Some(KeyWithModifier::new(BareKey::Char('@')).with_alt_modifier())
        );
    }

    #[test]
    fn a_capslocked_letter_needs_no_lock_state_to_arrive_uppercase() {
        assert_eq!(bytes(&character("A"), none()), b"A");
        assert_eq!(
            translated(&character("A"), none()).bare_key,
            BareKey::Char('A'),
            "the layout has already applied the lock; the window must not second-guess it"
        );
    }

    #[test]
    fn a_numlocked_keypad_key_is_the_character_the_layout_produced() {
        assert_eq!(bytes(&character("1"), none()), b"1");
        assert_eq!(
            bytes(&Key::Named(NamedKey::End), none()),
            b"\x1b[F",
            "with the lock off the same physical key is a named key and must stay one"
        );
    }

    fn paste_keys() -> Vec<KeyWithModifier> {
        crate::config::options::default_paste_keys()
    }

    #[test]
    fn the_paste_bindings_are_recognised() {
        let both = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert!(claims(&paste_keys(), &press(character("V"), both)));
        assert!(claims(&paste_keys(), &press(character("v"), both)));
        assert!(claims(
            &paste_keys(),
            &press(Key::Named(NamedKey::Insert), ModifiersState::SHIFT)
        ));
    }

    #[test]
    fn a_configured_chord_is_claimed_and_an_unconfigured_one_is_not() {
        let keys = vec![KeyWithModifier::new(BareKey::F(5)).with_alt_modifier()];
        assert!(claims(
            &keys,
            &press(Key::Named(NamedKey::F5), ModifiersState::ALT)
        ));
        assert!(!claims(&keys, &press(Key::Named(NamedKey::F5), none())));
        assert!(!claims(
            &paste_keys(),
            &press(Key::Named(NamedKey::F5), ModifiersState::ALT)
        ));
    }

    #[test]
    fn claiming_no_chords_at_all_claims_nothing() {
        let both = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert!(!claims(&[], &press(character("V"), both)));
        assert!(!claims(
            &[],
            &press(Key::Named(NamedKey::Insert), ModifiersState::SHIFT)
        ));
    }

    #[test]
    fn a_chord_states_shift_and_lowercases_its_character() {
        let both = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert_eq!(
            chord(&press(character("V"), both)),
            Some(
                KeyWithModifier::new(BareKey::Char('v'))
                    .with_ctrl_modifier()
                    .with_shift_modifier()
            )
        );
        assert_eq!(
            chord(&press(character("v"), both)),
            chord(&press(character("V"), both)),
            "the same chord must be produced whether or not the layout shifted the character"
        );
    }

    #[test]
    fn a_kitty_pane_can_tell_ctrl_shift_v_from_ctrl_v() {
        let ctrl = ModifiersState::CONTROL;
        let both = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert_eq!(bytes(&character("v"), ctrl), b"\x1b[118;5u");
        assert_eq!(bytes(&character("V"), both), b"\x1b[118;6u");
    }

    #[test]
    fn a_legacy_pane_cannot_tell_ctrl_shift_v_from_ctrl_v() {
        for key in [
            KeyWithModifier::new(BareKey::Char('v')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('v'))
                .with_ctrl_modifier()
                .with_shift_modifier(),
            KeyWithModifier::new(BareKey::Char('V')).with_ctrl_modifier(),
        ] {
            assert_eq!(
                key.serialize_non_kitty().as_deref(),
                Some("\u{16}"),
                "{key}"
            );
        }
    }

    #[test]
    fn shift_insert_is_distinguishable_from_a_bare_insert() {
        assert_eq!(
            bytes(&Key::Named(NamedKey::Insert), ModifiersState::SHIFT),
            b"\x1b[2;2~"
        );
        assert_eq!(bytes(&Key::Named(NamedKey::Insert), none()), b"\x1b[2~");
    }

    #[test]
    fn the_paste_bindings_do_not_swallow_neighbouring_keys() {
        let keys = paste_keys();
        assert!(!claims(
            &keys,
            &press(character("v"), ModifiersState::CONTROL)
        ));
        assert!(!claims(
            &keys,
            &press(character("v"), ModifiersState::SHIFT)
        ));
        assert!(!claims(&keys, &press(character("v"), none())));
        assert!(!claims(
            &keys,
            &press(
                character("c"),
                ModifiersState::CONTROL | ModifiersState::SHIFT
            )
        ));
        assert!(!claims(&keys, &press(Key::Named(NamedKey::Insert), none())));
        assert!(!claims(
            &keys,
            &press(
                Key::Named(NamedKey::Insert),
                ModifiersState::CONTROL | ModifiersState::SHIFT
            )
        ));
    }

    #[test]
    fn typed_keys_survive_the_protobuf_round_trip_to_the_server() {
        let sent: Vec<ClientToServerMsg> = [
            key_message(&press(character("c"), ModifiersState::CONTROL)),
            key_message(&press(character("A"), ModifiersState::SHIFT)),
            key_message(&press(Key::Named(NamedKey::ArrowLeft), ModifiersState::ALT)),
            key_message(&press(Key::Named(NamedKey::F5), none())),
        ]
        .into_iter()
        .map(|msg| msg.expect("expected a key message"))
        .collect();

        let expected = 8 + sent.len();
        let server = FakeServer::spawn(move |side| side.expect(expected));
        let geometry = Geometry {
            rows: 40,
            cols: 120,
            cell_width: 10,
            cell_height: 20,
        };
        let connection = attach_at(
            &server.path,
            "window-test",
            geometry,
            Capabilities::default(),
        )
        .unwrap();
        for msg in &sent {
            send(&connection.sender, msg.clone()).unwrap();
        }
        drop(connection);

        assert_eq!(&server.finish()[8..], &sent[..]);
    }

    #[test]
    fn every_message_declares_the_kitty_encoding() {
        match key_message(&press(character("a"), none())).unwrap() {
            ClientToServerMsg::Key {
                is_kitty_keyboard_protocol,
                ..
            } => assert!(is_kitty_keyboard_protocol),
            other => panic!("expected a key message, got {:?}", other),
        }
    }
}

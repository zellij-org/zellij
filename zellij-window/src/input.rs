use std::collections::BTreeSet;

use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use zellij_utils::data::{BareKey, KeyModifier, KeyWithModifier};
use zellij_utils::input::actions::Action;
use zellij_utils::ipc::ClientToServerMsg;

#[derive(Debug, Clone, PartialEq)]
pub struct Press {
    pub logical: Key,
    pub unmodified: Option<Key>,
    pub modifiers: ModifiersState,
}

impl Press {
    pub fn of(event: &KeyEvent, modifiers: ModifiersState) -> Self {
        Self {
            logical: event.logical_key.clone(),
            unmodified: unmodified_key(event),
            modifiers,
        }
    }

    pub fn new(logical: Key, modifiers: ModifiersState) -> Self {
        Self {
            logical,
            unmodified: None,
            modifiers,
        }
    }

    fn effective_modifiers(&self) -> ModifiersState {
        let mut modifiers = self.modifiers;
        if alt_shifted_the_character(&self.logical, self.unmodified.as_ref()) {
            modifiers.remove(ModifiersState::ALT);
        }
        modifiers
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typed {
    Key(KeyWithModifier),
    Text(String),
}

#[cfg(any(
    target_os = "windows",
    target_os = "macos",
    all(unix, not(target_os = "macos"), not(target_os = "android"))
))]
fn unmodified_key(event: &KeyEvent) -> Option<Key> {
    use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
    Some(event.key_without_modifiers())
}

#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    all(unix, not(target_os = "macos"), not(target_os = "android"))
)))]
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
    let bare_key = match &press.logical {
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
        Typed::Key(key) => {
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
        Typed::Text(text) => text_message(text),
    })
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
    match &press.logical {
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

#[cfg(test)]
pub fn translate(press: &Press) -> Option<KeyWithModifier> {
    match typed(press) {
        Some(Typed::Key(key)) => Some(key),
        _ => None,
    }
}

fn sole_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let character = chars.next()?;
    chars.next().is_none().then_some(character)
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
mod tests {
    use super::*;
    use winit::keyboard::{NativeKey, SmolStr};

    use crate::connection::{send, test_attach_at as attach_at, Capabilities, Geometry};
    use crate::test_server::FakeServer;

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
            logical: character(logical),
            unmodified: Some(character(unmodified)),
            modifiers,
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
    fn control_survives_and_encodes_as_a_control_byte() {
        let key = translated(&character("c"), ModifiersState::CONTROL);
        assert_eq!(key.bare_key, BareKey::Char('c'));
        assert!(key.key_modifiers.contains(&KeyModifier::Ctrl));
        assert_eq!(bytes(&character("c"), ModifiersState::CONTROL), vec![3]);
    }

    #[test]
    fn control_with_shift_reaches_the_same_control_byte() {
        assert_eq!(
            bytes(
                &character("C"),
                ModifiersState::CONTROL | ModifiersState::SHIFT
            ),
            vec![3]
        );
    }

    #[test]
    fn alt_encodes_as_an_escape_prefix() {
        assert_eq!(bytes(&character("a"), ModifiersState::ALT), b"\x1ba");
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
            (NamedKey::Escape, b"\x1b"),
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
            b"\x1b[Z"
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
            Some(crate::clipboard::paste_message("ab".to_owned())),
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
        assert_eq!(bytes(&character("a"), ModifiersState::ALT), b"\x1ba");
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
            modifiers: ModifiersState::ALT,
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
        crate::options::default_paste_keys()
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
    fn a_pane_cannot_tell_ctrl_shift_v_from_ctrl_v() {
        let ctrl = ModifiersState::CONTROL;
        let both = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert_eq!(bytes(&character("v"), ctrl), vec![0x16]);
        assert_eq!(bytes(&character("V"), both), vec![0x16]);
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
            assert_eq!(key.serialize_kitty().as_deref(), Some("\u{16}"), "{key}");
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
    fn every_message_declares_the_non_kitty_encoding() {
        match key_message(&press(character("a"), none())).unwrap() {
            ClientToServerMsg::Key {
                is_kitty_keyboard_protocol,
                ..
            } => assert!(!is_kitty_keyboard_protocol),
            other => panic!("expected a key message, got {:?}", other),
        }
    }
}

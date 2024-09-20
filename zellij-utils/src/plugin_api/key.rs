pub use super::generated_api::api::key::{
    key::{
        KeyModifier as ProtobufKeyModifier, MainKey as ProtobufMainKey,
        NamedKey as ProtobufNamedKey,
    },
    Key as ProtobufKey,
};
use crate::data::{BareKey, KeyModifier, KeyWithModifier};

use std::collections::BTreeSet;
use std::convert::TryFrom;

impl TryFrom<ProtobufMainKey> for BareKey {
    type Error = &'static str;
    fn try_from(protobuf_main_key: ProtobufMainKey) -> Result<Self, &'static str> {
        match protobuf_main_key {
            ProtobufMainKey::Char(character) => Ok(BareKey::Char(char_index_to_char(character))),
            ProtobufMainKey::Key(key_index) => {
                let key = ProtobufNamedKey::from_i32(key_index).ok_or("invalid_key")?;
                Ok(named_key_to_bare_key(key))
            },
        }
    }
}

impl TryFrom<BareKey> for ProtobufMainKey {
    type Error = &'static str;
    fn try_from(bare_key: BareKey) -> Result<Self, &'static str> {
        match bare_key {
            BareKey::PageDown => Ok(ProtobufMainKey::Key(ProtobufNamedKey::PageDown as i32)),
            BareKey::PageUp => Ok(ProtobufMainKey::Key(ProtobufNamedKey::PageUp as i32)),
            BareKey::Left => Ok(ProtobufMainKey::Key(ProtobufNamedKey::LeftArrow as i32)),
            BareKey::Down => Ok(ProtobufMainKey::Key(ProtobufNamedKey::DownArrow as i32)),
            BareKey::Up => Ok(ProtobufMainKey::Key(ProtobufNamedKey::UpArrow as i32)),
            BareKey::Right => Ok(ProtobufMainKey::Key(ProtobufNamedKey::RightArrow as i32)),
            BareKey::Home => Ok(ProtobufMainKey::Key(ProtobufNamedKey::Home as i32)),
            BareKey::End => Ok(ProtobufMainKey::Key(ProtobufNamedKey::End as i32)),
            BareKey::Backspace => Ok(ProtobufMainKey::Key(ProtobufNamedKey::Backspace as i32)),
            BareKey::Delete => Ok(ProtobufMainKey::Key(ProtobufNamedKey::Delete as i32)),
            BareKey::Insert => Ok(ProtobufMainKey::Key(ProtobufNamedKey::Insert as i32)),
            BareKey::F(f_index) => fn_index_to_main_key(f_index),
            BareKey::Char(character) => Ok(ProtobufMainKey::Char(character as i32)),
            BareKey::Tab => Ok(ProtobufMainKey::Key(ProtobufNamedKey::Tab as i32)),
            BareKey::Esc => Ok(ProtobufMainKey::Key(ProtobufNamedKey::Esc as i32)),
            BareKey::Enter => Ok(ProtobufMainKey::Key(ProtobufNamedKey::Enter as i32)),
            BareKey::CapsLock => Ok(ProtobufMainKey::Key(ProtobufNamedKey::CapsLock as i32)),
            BareKey::ScrollLock => Ok(ProtobufMainKey::Key(ProtobufNamedKey::ScrollLock as i32)),
            BareKey::NumLock => Ok(ProtobufMainKey::Key(ProtobufNamedKey::NumLock as i32)),
            BareKey::PrintScreen => Ok(ProtobufMainKey::Key(ProtobufNamedKey::PrintScreen as i32)),
            BareKey::Pause => Ok(ProtobufMainKey::Key(ProtobufNamedKey::Pause as i32)),
            BareKey::Menu => Ok(ProtobufMainKey::Key(ProtobufNamedKey::Menu as i32)),
        }
    }
}

impl TryFrom<ProtobufKeyModifier> for KeyModifier {
    type Error = &'static str;
    fn try_from(protobuf_key_modifier: ProtobufKeyModifier) -> Result<Self, &'static str> {
        match protobuf_key_modifier {
            ProtobufKeyModifier::Ctrl => Ok(KeyModifier::Ctrl),
            ProtobufKeyModifier::Alt => Ok(KeyModifier::Alt),
            ProtobufKeyModifier::Shift => Ok(KeyModifier::Shift),
            ProtobufKeyModifier::Super => Ok(KeyModifier::Super),
        }
    }
}

impl TryFrom<KeyModifier> for ProtobufKeyModifier {
    type Error = &'static str;
    fn try_from(key_modifier: KeyModifier) -> Result<Self, &'static str> {
        match key_modifier {
            KeyModifier::Ctrl => Ok(ProtobufKeyModifier::Ctrl),
            KeyModifier::Alt => Ok(ProtobufKeyModifier::Alt),
            KeyModifier::Shift => Ok(ProtobufKeyModifier::Shift),
            KeyModifier::Super => Ok(ProtobufKeyModifier::Super),
            _ => Err("unsupported key modifier"), // TODO: test this so we don't crash if we have a
                                                  // Capslock or something
        }
    }
}

impl TryFrom<ProtobufKey> for KeyWithModifier {
    type Error = &'static str;
    fn try_from(protobuf_key: ProtobufKey) -> Result<Self, &'static str> {
        let bare_key = protobuf_key
            .main_key
            .ok_or("Key must have main_key")?
            .try_into()?;
        let mut key_modifiers = BTreeSet::new();
        if let Some(main_modifier) = protobuf_key.modifier {
            key_modifiers.insert(
                ProtobufKeyModifier::from_i32(main_modifier)
                    .ok_or("invalid key modifier")?
                    .try_into()?,
            );
        }
        for key_modifier in protobuf_key.additional_modifiers {
            key_modifiers.insert(
                ProtobufKeyModifier::from_i32(key_modifier)
                    .ok_or("invalid key modifier")?
                    .try_into()?,
            );
        }
        Ok(KeyWithModifier {
            bare_key,
            key_modifiers,
        })
    }
}

impl TryFrom<KeyWithModifier> for ProtobufKey {
    type Error = &'static str;
    fn try_from(key_with_modifier: KeyWithModifier) -> Result<Self, &'static str> {
        let mut modifiers: Vec<ProtobufKeyModifier> = vec![];
        for key_modifier in key_with_modifier.key_modifiers {
            modifiers.push(key_modifier.try_into()?);
        }

        Ok(ProtobufKey {
            main_key: Some(key_with_modifier.bare_key.try_into()?),
            modifier: modifiers.pop().map(|m| m as i32),
            additional_modifiers: modifiers.into_iter().map(|m| m as i32).collect(),
        })
    }
}

fn fn_index_to_main_key(index: u8) -> Result<ProtobufMainKey, &'static str> {
    match index {
        1 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F1 as i32)),
        2 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F2 as i32)),
        3 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F3 as i32)),
        4 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F4 as i32)),
        5 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F5 as i32)),
        6 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F6 as i32)),
        7 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F7 as i32)),
        8 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F8 as i32)),
        9 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F9 as i32)),
        10 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F10 as i32)),
        11 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F11 as i32)),
        12 => Ok(ProtobufMainKey::Key(ProtobufNamedKey::F12 as i32)),
        _ => Err("Invalid key"),
    }
}

fn char_index_to_char(char_index: i32) -> char {
    char_index as u8 as char
}

fn named_key_to_bare_key(named_key: ProtobufNamedKey) -> BareKey {
    match named_key {
        ProtobufNamedKey::PageDown => BareKey::PageDown,
        ProtobufNamedKey::PageUp => BareKey::PageUp,
        ProtobufNamedKey::LeftArrow => BareKey::Left,
        ProtobufNamedKey::DownArrow => BareKey::Down,
        ProtobufNamedKey::UpArrow => BareKey::Up,
        ProtobufNamedKey::RightArrow => BareKey::Right,
        ProtobufNamedKey::Home => BareKey::Home,
        ProtobufNamedKey::End => BareKey::End,
        ProtobufNamedKey::Backspace => BareKey::Backspace,
        ProtobufNamedKey::Delete => BareKey::Delete,
        ProtobufNamedKey::Insert => BareKey::Insert,
        ProtobufNamedKey::F1 => BareKey::F(1),
        ProtobufNamedKey::F2 => BareKey::F(2),
        ProtobufNamedKey::F3 => BareKey::F(3),
        ProtobufNamedKey::F4 => BareKey::F(4),
        ProtobufNamedKey::F5 => BareKey::F(5),
        ProtobufNamedKey::F6 => BareKey::F(6),
        ProtobufNamedKey::F7 => BareKey::F(7),
        ProtobufNamedKey::F8 => BareKey::F(8),
        ProtobufNamedKey::F9 => BareKey::F(9),
        ProtobufNamedKey::F10 => BareKey::F(10),
        ProtobufNamedKey::F11 => BareKey::F(11),
        ProtobufNamedKey::F12 => BareKey::F(12),
        ProtobufNamedKey::Tab => BareKey::Tab,
        ProtobufNamedKey::Esc => BareKey::Esc,
        ProtobufNamedKey::CapsLock => BareKey::CapsLock,
        ProtobufNamedKey::ScrollLock => BareKey::ScrollLock,
        ProtobufNamedKey::PrintScreen => BareKey::PrintScreen,
        ProtobufNamedKey::Pause => BareKey::Pause,
        ProtobufNamedKey::Menu => BareKey::Menu,
        ProtobufNamedKey::NumLock => BareKey::NumLock,
        ProtobufNamedKey::Enter => BareKey::Enter,
    }
}

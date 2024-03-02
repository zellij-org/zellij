pub use super::generated_api::api::key::{
    key::{KeyModifier, MainKey, NamedKey},
    Key as ProtobufKey,
};
use crate::data::{CharOrArrow, Direction, Key};

use std::convert::TryFrom;

impl TryFrom<ProtobufKey> for Key {
    type Error = &'static str;
    fn try_from(protobuf_key: ProtobufKey) -> Result<Self, &'static str> {
        let key_modifier = parse_optional_modifier(&protobuf_key);
        match key_modifier {
            Some(KeyModifier::Ctrl) => {
                if let Ok(character) = char_from_main_key(protobuf_key.main_key.clone()) {
                    Ok(Key::Ctrl(character))
                } else {
                    let index = fn_index_from_main_key(protobuf_key.main_key)?;
                    Ok(Key::CtrlF(index))
                }
            },
            Some(KeyModifier::Alt) => {
                if let Ok(char_or_arrow) = CharOrArrow::from_main_key(protobuf_key.main_key.clone())
                {
                    Ok(Key::Alt(char_or_arrow))
                } else {
                    let index = fn_index_from_main_key(protobuf_key.main_key)?;
                    Ok(Key::AltF(index))
                }
            },
            None => match protobuf_key.main_key.as_ref().ok_or("invalid key")? {
                MainKey::Char(_key_index) => {
                    let character = char_from_main_key(protobuf_key.main_key)?;
                    Ok(Key::Char(character))
                },
                MainKey::Key(key_index) => {
                    let key = NamedKey::from_i32(*key_index).ok_or("invalid_key")?;
                    Ok(named_key_to_key(key))
                },
            },
        }
    }
}

impl TryFrom<Key> for ProtobufKey {
    type Error = &'static str;
    fn try_from(key: Key) -> Result<Self, &'static str> {
        match key {
            Key::PageDown => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::PageDown as i32)),
            }),
            Key::PageUp => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::PageUp as i32)),
            }),
            Key::Left => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::LeftArrow as i32)),
            }),
            Key::Down => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::DownArrow as i32)),
            }),
            Key::Up => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::UpArrow as i32)),
            }),
            Key::Right => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::RightArrow as i32)),
            }),
            Key::Home => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::Home as i32)),
            }),
            Key::End => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::End as i32)),
            }),
            Key::Backspace => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::Backspace as i32)),
            }),
            Key::Delete => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::Delete as i32)),
            }),
            Key::Insert => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::Insert as i32)),
            }),
            Key::F(index) => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(fn_index_to_main_key(index)?),
            }),
            Key::CtrlF(index) => Ok(ProtobufKey {
                modifier: Some(KeyModifier::Ctrl as i32),
                main_key: Some(fn_index_to_main_key(index)?),
            }),
            Key::AltF(index) => Ok(ProtobufKey {
                modifier: Some(KeyModifier::Alt as i32),
                main_key: Some(fn_index_to_main_key(index)?),
            }),
            Key::Char(character) => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Char((character as u8) as i32)),
            }),
            Key::Alt(char_or_arrow) => {
                let main_key = match char_or_arrow {
                    CharOrArrow::Char(character) => MainKey::Char((character as u8) as i32),
                    CharOrArrow::Direction(Direction::Left) => {
                        MainKey::Key(NamedKey::LeftArrow as i32)
                    },
                    CharOrArrow::Direction(Direction::Right) => {
                        MainKey::Key(NamedKey::RightArrow as i32)
                    },
                    CharOrArrow::Direction(Direction::Up) => MainKey::Key(NamedKey::UpArrow as i32),
                    CharOrArrow::Direction(Direction::Down) => {
                        MainKey::Key(NamedKey::DownArrow as i32)
                    },
                };
                Ok(ProtobufKey {
                    modifier: Some(KeyModifier::Alt as i32),
                    main_key: Some(main_key),
                })
            },
            Key::Ctrl(character) => Ok(ProtobufKey {
                modifier: Some(KeyModifier::Ctrl as i32),
                main_key: Some(MainKey::Char((character as u8) as i32)),
            }),
            Key::BackTab => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::Tab as i32)),
            }),
            Key::Null => {
                Ok(ProtobufKey {
                    modifier: None,
                    main_key: None, // TODO: does this break deserialization?
                })
            },
            Key::Esc => Ok(ProtobufKey {
                modifier: None,
                main_key: Some(MainKey::Key(NamedKey::Esc as i32)),
            }),
        }
    }
}

fn fn_index_to_main_key(index: u8) -> Result<MainKey, &'static str> {
    match index {
        1 => Ok(MainKey::Key(NamedKey::F1 as i32)),
        2 => Ok(MainKey::Key(NamedKey::F2 as i32)),
        3 => Ok(MainKey::Key(NamedKey::F3 as i32)),
        4 => Ok(MainKey::Key(NamedKey::F4 as i32)),
        5 => Ok(MainKey::Key(NamedKey::F5 as i32)),
        6 => Ok(MainKey::Key(NamedKey::F6 as i32)),
        7 => Ok(MainKey::Key(NamedKey::F7 as i32)),
        8 => Ok(MainKey::Key(NamedKey::F8 as i32)),
        9 => Ok(MainKey::Key(NamedKey::F9 as i32)),
        10 => Ok(MainKey::Key(NamedKey::F10 as i32)),
        11 => Ok(MainKey::Key(NamedKey::F11 as i32)),
        12 => Ok(MainKey::Key(NamedKey::F12 as i32)),
        _ => Err("Invalid key"),
    }
}

impl CharOrArrow {
    pub fn from_main_key(
        main_key: std::option::Option<MainKey>,
    ) -> Result<CharOrArrow, &'static str> {
        match main_key {
            Some(MainKey::Char(encoded_key)) => {
                Ok(CharOrArrow::Char(char_index_to_char(encoded_key)))
            },
            Some(MainKey::Key(key_index)) => match NamedKey::from_i32(key_index) {
                Some(NamedKey::LeftArrow) => Ok(CharOrArrow::Direction(Direction::Left)),
                Some(NamedKey::RightArrow) => Ok(CharOrArrow::Direction(Direction::Right)),
                Some(NamedKey::UpArrow) => Ok(CharOrArrow::Direction(Direction::Up)),
                Some(NamedKey::DownArrow) => Ok(CharOrArrow::Direction(Direction::Down)),
                _ => Err("Unsupported key"),
            },
            _ => {
                return Err("Unsupported key");
            },
        }
    }
}

fn parse_optional_modifier(m: &ProtobufKey) -> Option<KeyModifier> {
    match m.modifier {
        Some(modifier) => KeyModifier::from_i32(modifier),
        _ => None,
    }
}

fn char_index_to_char(char_index: i32) -> char {
    char_index as u8 as char
}

fn char_from_main_key(main_key: Option<MainKey>) -> Result<char, &'static str> {
    match main_key {
        Some(MainKey::Char(encoded_key)) => {
            return Ok(char_index_to_char(encoded_key));
        },
        _ => {
            return Err("Unsupported key");
        },
    }
}

fn fn_index_from_main_key(main_key: Option<MainKey>) -> Result<u8, &'static str> {
    match main_key {
        Some(MainKey::Key(n)) if n == NamedKey::F1 as i32 => Ok(1),
        Some(MainKey::Key(n)) if n == NamedKey::F2 as i32 => Ok(2),
        Some(MainKey::Key(n)) if n == NamedKey::F3 as i32 => Ok(3),
        Some(MainKey::Key(n)) if n == NamedKey::F4 as i32 => Ok(4),
        Some(MainKey::Key(n)) if n == NamedKey::F5 as i32 => Ok(5),
        Some(MainKey::Key(n)) if n == NamedKey::F6 as i32 => Ok(6),
        Some(MainKey::Key(n)) if n == NamedKey::F7 as i32 => Ok(7),
        Some(MainKey::Key(n)) if n == NamedKey::F8 as i32 => Ok(8),
        Some(MainKey::Key(n)) if n == NamedKey::F9 as i32 => Ok(9),
        Some(MainKey::Key(n)) if n == NamedKey::F10 as i32 => Ok(10),
        Some(MainKey::Key(n)) if n == NamedKey::F11 as i32 => Ok(11),
        Some(MainKey::Key(n)) if n == NamedKey::F12 as i32 => Ok(12),
        _ => Err("Unsupported key"),
    }
}

fn named_key_to_key(named_key: NamedKey) -> Key {
    match named_key {
        NamedKey::PageDown => Key::PageDown,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::LeftArrow => Key::Left,
        NamedKey::DownArrow => Key::Down,
        NamedKey::UpArrow => Key::Up,
        NamedKey::RightArrow => Key::Right,
        NamedKey::Home => Key::Home,
        NamedKey::End => Key::End,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::Insert => Key::Insert,
        NamedKey::F1 => Key::F(1),
        NamedKey::F2 => Key::F(2),
        NamedKey::F3 => Key::F(3),
        NamedKey::F4 => Key::F(4),
        NamedKey::F5 => Key::F(5),
        NamedKey::F6 => Key::F(6),
        NamedKey::F7 => Key::F(7),
        NamedKey::F8 => Key::F(8),
        NamedKey::F9 => Key::F(9),
        NamedKey::F10 => Key::F(10),
        NamedKey::F11 => Key::F(11),
        NamedKey::F12 => Key::F(12),
        NamedKey::Tab => Key::BackTab,
        NamedKey::Esc => Key::Esc,
    }
}

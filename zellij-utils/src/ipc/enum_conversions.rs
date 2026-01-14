use crate::{
    client_server_contract::client_server_contract::{
        BareKey as ProtoBareKey, KeyModifier as ProtoKeyModifier,
    },
    data::{BareKey, KeyModifier},
    errors::prelude::*,
};

// BareKey conversions
impl From<BareKey> for ProtoBareKey {
    fn from(key: BareKey) -> Self {
        match key {
            BareKey::PageDown => ProtoBareKey::PageDown,
            BareKey::PageUp => ProtoBareKey::PageUp,
            BareKey::Left => ProtoBareKey::Left,
            BareKey::Down => ProtoBareKey::Down,
            BareKey::Up => ProtoBareKey::Up,
            BareKey::Right => ProtoBareKey::Right,
            BareKey::Home => ProtoBareKey::Home,
            BareKey::End => ProtoBareKey::End,
            BareKey::Backspace => ProtoBareKey::Backspace,
            BareKey::Delete => ProtoBareKey::Delete,
            BareKey::Insert => ProtoBareKey::Insert,
            BareKey::F(1) => ProtoBareKey::F1,
            BareKey::F(2) => ProtoBareKey::F2,
            BareKey::F(3) => ProtoBareKey::F3,
            BareKey::F(4) => ProtoBareKey::F4,
            BareKey::F(5) => ProtoBareKey::F5,
            BareKey::F(6) => ProtoBareKey::F6,
            BareKey::F(7) => ProtoBareKey::F7,
            BareKey::F(8) => ProtoBareKey::F8,
            BareKey::F(9) => ProtoBareKey::F9,
            BareKey::F(10) => ProtoBareKey::F10,
            BareKey::F(11) => ProtoBareKey::F11,
            BareKey::F(12) => ProtoBareKey::F12,
            BareKey::F(_) => ProtoBareKey::Unspecified, // Unsupported F-key
            BareKey::Char(_) => ProtoBareKey::Char,     // Character stored separately
            BareKey::Tab => ProtoBareKey::Tab,
            BareKey::Esc => ProtoBareKey::Esc,
            BareKey::Enter => ProtoBareKey::Enter,
            BareKey::CapsLock => ProtoBareKey::CapsLock,
            BareKey::ScrollLock => ProtoBareKey::ScrollLock,
            BareKey::NumLock => ProtoBareKey::NumLock,
            BareKey::PrintScreen => ProtoBareKey::PrintScreen,
            BareKey::Pause => ProtoBareKey::Pause,
            BareKey::Menu => ProtoBareKey::Menu,
        }
    }
}

impl TryFrom<ProtoBareKey> for BareKey {
    type Error = anyhow::Error;

    fn try_from(key: ProtoBareKey) -> Result<Self> {
        match key {
            ProtoBareKey::PageDown => Ok(BareKey::PageDown),
            ProtoBareKey::PageUp => Ok(BareKey::PageUp),
            ProtoBareKey::Left => Ok(BareKey::Left),
            ProtoBareKey::Down => Ok(BareKey::Down),
            ProtoBareKey::Up => Ok(BareKey::Up),
            ProtoBareKey::Right => Ok(BareKey::Right),
            ProtoBareKey::Home => Ok(BareKey::Home),
            ProtoBareKey::End => Ok(BareKey::End),
            ProtoBareKey::Backspace => Ok(BareKey::Backspace),
            ProtoBareKey::Delete => Ok(BareKey::Delete),
            ProtoBareKey::Insert => Ok(BareKey::Insert),
            ProtoBareKey::F1 => Ok(BareKey::F(1)),
            ProtoBareKey::F2 => Ok(BareKey::F(2)),
            ProtoBareKey::F3 => Ok(BareKey::F(3)),
            ProtoBareKey::F4 => Ok(BareKey::F(4)),
            ProtoBareKey::F5 => Ok(BareKey::F(5)),
            ProtoBareKey::F6 => Ok(BareKey::F(6)),
            ProtoBareKey::F7 => Ok(BareKey::F(7)),
            ProtoBareKey::F8 => Ok(BareKey::F(8)),
            ProtoBareKey::F9 => Ok(BareKey::F(9)),
            ProtoBareKey::F10 => Ok(BareKey::F(10)),
            ProtoBareKey::F11 => Ok(BareKey::F(11)),
            ProtoBareKey::F12 => Ok(BareKey::F(12)),
            ProtoBareKey::Char => Err(anyhow!("Character key needs character data")),
            ProtoBareKey::Tab => Ok(BareKey::Tab),
            ProtoBareKey::Esc => Ok(BareKey::Esc),
            ProtoBareKey::Enter => Ok(BareKey::Enter),
            ProtoBareKey::CapsLock => Ok(BareKey::CapsLock),
            ProtoBareKey::ScrollLock => Ok(BareKey::ScrollLock),
            ProtoBareKey::NumLock => Ok(BareKey::NumLock),
            ProtoBareKey::PrintScreen => Ok(BareKey::PrintScreen),
            ProtoBareKey::Pause => Ok(BareKey::Pause),
            ProtoBareKey::Menu => Ok(BareKey::Menu),
            ProtoBareKey::Unspecified => Err(anyhow!("Unspecified bare key")),
        }
    }
}

// KeyModifier conversions
impl From<KeyModifier> for ProtoKeyModifier {
    fn from(modifier: KeyModifier) -> Self {
        match modifier {
            KeyModifier::Ctrl => ProtoKeyModifier::Ctrl,
            KeyModifier::Alt => ProtoKeyModifier::Alt,
            KeyModifier::Shift => ProtoKeyModifier::Shift,
            KeyModifier::Super => ProtoKeyModifier::Super,
        }
    }
}

impl TryFrom<ProtoKeyModifier> for KeyModifier {
    type Error = anyhow::Error;

    fn try_from(modifier: ProtoKeyModifier) -> Result<Self> {
        match modifier {
            ProtoKeyModifier::Ctrl => Ok(KeyModifier::Ctrl),
            ProtoKeyModifier::Alt => Ok(KeyModifier::Alt),
            ProtoKeyModifier::Shift => Ok(KeyModifier::Shift),
            ProtoKeyModifier::Super => Ok(KeyModifier::Super),
            ProtoKeyModifier::Unspecified => Err(anyhow!("Unspecified key modifier")),
        }
    }
}

// Helper functions for converting between protobuf i32 and enum types
pub fn bare_key_to_proto_i32(key: BareKey) -> i32 {
    ProtoBareKey::from(key) as i32
}

pub fn bare_key_from_proto_i32(value: i32) -> Result<BareKey> {
    let proto_key =
        ProtoBareKey::from_i32(value).ok_or_else(|| anyhow!("Invalid BareKey value: {}", value))?;
    proto_key.try_into()
}

pub fn key_modifier_to_proto_i32(modifier: KeyModifier) -> i32 {
    ProtoKeyModifier::from(modifier) as i32
}

pub fn key_modifier_from_proto_i32(value: i32) -> Result<KeyModifier> {
    let proto_modifier = ProtoKeyModifier::from_i32(value)
        .ok_or_else(|| anyhow!("Invalid KeyModifier value: {}", value))?;
    proto_modifier.try_into()
}

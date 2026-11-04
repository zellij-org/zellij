pub const BUS_SERVICE: &str = "org.freedesktop.DBus";
pub const BUS_PATH: &str = "/org/freedesktop/DBus";
pub const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
pub const INTROSPECTABLE: &str = "org.freedesktop.DBus.Introspectable";

const MAX_MESSAGE_BYTES: usize = 1 << 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    MethodCall,
    MethodReturn,
    Error,
    Signal,
}

impl Kind {
    fn code(self) -> u8 {
        match self {
            Kind::MethodCall => 1,
            Kind::MethodReturn => 2,
            Kind::Error => 3,
            Kind::Signal => 4,
        }
    }

    fn of(code: u8) -> Option<Kind> {
        match code {
            1 => Some(Kind::MethodCall),
            2 => Some(Kind::MethodReturn),
            3 => Some(Kind::Error),
            4 => Some(Kind::Signal),
            _ => None,
        }
    }
}

const FIELD_PATH: u8 = 1;
const FIELD_INTERFACE: u8 = 2;
const FIELD_MEMBER: u8 = 3;
const FIELD_ERROR_NAME: u8 = 4;
const FIELD_REPLY_SERIAL: u8 = 5;
const FIELD_DESTINATION: u8 = 6;
const FIELD_SENDER: u8 = 7;
const FIELD_SIGNATURE: u8 = 8;

const FLAG_NO_REPLY_EXPECTED: u8 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub kind: Kind,
    pub serial: u32,
    pub no_reply: bool,
    pub reply_serial: Option<u32>,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    pub destination: Option<String>,
    pub sender: Option<String>,
    pub body: Vec<Value>,
}

impl Message {
    fn blank(kind: Kind) -> Self {
        Message {
            kind,
            serial: 0,
            no_reply: false,
            reply_serial: None,
            path: None,
            interface: None,
            member: None,
            error_name: None,
            destination: None,
            sender: None,
            body: vec![],
        }
    }

    pub fn call(
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        body: Vec<Value>,
    ) -> Self {
        Message {
            destination: Some(destination.to_owned()),
            path: Some(path.to_owned()),
            interface: Some(interface.to_owned()),
            member: Some(member.to_owned()),
            body,
            ..Message::blank(Kind::MethodCall)
        }
    }

    pub fn signal(path: &str, interface: &str, member: &str, body: Vec<Value>) -> Self {
        Message {
            path: Some(path.to_owned()),
            interface: Some(interface.to_owned()),
            member: Some(member.to_owned()),
            body,
            ..Message::blank(Kind::Signal)
        }
    }

    pub fn reply_to(call: &Message, body: Vec<Value>) -> Self {
        Message {
            reply_serial: Some(call.serial),
            destination: call.sender.clone(),
            body,
            ..Message::blank(Kind::MethodReturn)
        }
    }

    pub fn error_to(call: &Message, name: &str, text: &str) -> Self {
        Message {
            reply_serial: Some(call.serial),
            destination: call.sender.clone(),
            error_name: Some(name.to_owned()),
            body: vec![Value::str(text)],
            ..Message::blank(Kind::Error)
        }
    }

    pub fn is(&self, interface: &str, member: &str) -> bool {
        self.interface.as_deref() == Some(interface) && self.member.as_deref() == Some(member)
    }

    pub fn encode(&self, serial: u32) -> Vec<u8> {
        let (signature, body) = encode_body(&self.body);
        let mut fields = vec![];
        let mut field = |code: u8, value: Value| {
            fields.push(Value::Struct(vec![
                Value::Byte(code),
                Value::variant(value),
            ]));
        };
        if let Some(path) = &self.path {
            field(FIELD_PATH, Value::Path(path.clone()));
        }
        if let Some(interface) = &self.interface {
            field(FIELD_INTERFACE, Value::str(interface.clone()));
        }
        if let Some(member) = &self.member {
            field(FIELD_MEMBER, Value::str(member.clone()));
        }
        if let Some(error_name) = &self.error_name {
            field(FIELD_ERROR_NAME, Value::str(error_name.clone()));
        }
        if let Some(reply_serial) = self.reply_serial {
            field(FIELD_REPLY_SERIAL, Value::U32(reply_serial));
        }
        if let Some(destination) = &self.destination {
            field(FIELD_DESTINATION, Value::str(destination.clone()));
        }
        if !signature.is_empty() {
            field(FIELD_SIGNATURE, Value::Sig(signature));
        }
        let mut writer = Writer::new();
        writer.bytes.push(b'l');
        writer.bytes.push(self.kind.code());
        writer.bytes.push(if self.no_reply {
            FLAG_NO_REPLY_EXPECTED
        } else {
            0
        });
        writer.bytes.push(1);
        writer.raw_u32(body.len() as u32);
        writer.raw_u32(serial);
        writer.value(&Value::Array("(yv)".to_owned(), fields));
        writer.align(8);
        let mut bytes = writer.into_bytes();
        bytes.extend_from_slice(&body);
        bytes
    }

    pub fn needed(header: &[u8; 16]) -> Result<usize, DecodeError> {
        let big_endian = match header[0] {
            b'l' => false,
            b'B' => true,
            _ => return Err(bad("a message with an unknown byte order")),
        };
        let word = |bytes: &[u8]| {
            let word = [bytes[0], bytes[1], bytes[2], bytes[3]];
            if big_endian {
                u32::from_be_bytes(word)
            } else {
                u32::from_le_bytes(word)
            }
        };
        let body_len = word(&header[4..8]) as usize;
        let fields_len = word(&header[12..16]) as usize;
        let total = (16 + fields_len).div_ceil(8) * 8 + body_len;
        if total > MAX_MESSAGE_BYTES {
            return Err(bad("a message larger than the bus allows"));
        }
        Ok(total)
    }

    pub fn decode(bytes: &[u8]) -> Result<Message, DecodeError> {
        if bytes.len() < 16 {
            return Err(bad("a message shorter than its header"));
        }
        let big_endian = bytes[0] == b'B';
        let kind = Kind::of(bytes[1]).ok_or_else(|| bad("a message of an unknown kind"))?;
        let mut reader = Reader {
            bytes,
            at: 4,
            big_endian,
        };
        let body_len = reader.u32()? as usize;
        let serial = reader.u32()?;
        let fields = reader.value(
            &Type::Array(Box::new(Type::Struct(vec![Type::Byte, Type::Variant]))),
            0,
        )?;
        reader.align(8)?;
        let body_start = reader.at;
        let body = bytes
            .get(body_start..body_start + body_len)
            .ok_or_else(|| bad("a message body shorter than declared"))?;
        let mut message = Message::blank(kind);
        message.serial = serial;
        message.no_reply = bytes[2] & FLAG_NO_REPLY_EXPECTED != 0;
        let mut signature = String::new();
        for field in fields.items() {
            let parts = field.items();
            let (Some(Value::Byte(code)), Some(value)) = (parts.first(), parts.get(1)) else {
                continue;
            };
            let text = value.as_str().map(str::to_owned);
            match *code {
                FIELD_PATH => message.path = text,
                FIELD_INTERFACE => message.interface = text,
                FIELD_MEMBER => message.member = text,
                FIELD_ERROR_NAME => message.error_name = text,
                FIELD_REPLY_SERIAL => message.reply_serial = value.as_u32(),
                FIELD_DESTINATION => message.destination = text,
                FIELD_SENDER => message.sender = text,
                FIELD_SIGNATURE => signature = text.unwrap_or_default(),
                _ => {},
            }
        }
        message.body = decode_body(&signature, body, big_endian)?;
        Ok(message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketAddress<'a> {
    Path(&'a str),
    Abstract(&'a str),
}

pub fn bus_socket(address: &str) -> Option<SocketAddress<'_>> {
    for candidate in address.split(';') {
        let Some(rest) = candidate.strip_prefix("unix:") else {
            continue;
        };
        for pair in rest.split(',') {
            match pair.split_once('=') {
                Some(("path", value)) => return Some(SocketAddress::Path(value)),
                Some(("abstract", value)) => return Some(SocketAddress::Abstract(value)),
                _ => {},
            }
        }
    }
    None
}

pub fn auth_line(uid: u32) -> String {
    let digits = uid.to_string();
    let mut hex = String::with_capacity(digits.len() * 2);
    for byte in digits.as_bytes() {
        hex.push_str(&format!("{:02x}", byte));
    }
    format!("AUTH EXTERNAL {}\r\n", hex)
}

pub fn match_rule(interface: &str, member: Option<&str>) -> String {
    match member {
        Some(member) => format!(
            "type='signal',interface='{}',member='{}'",
            interface, member
        ),
        None => format!("type='signal',interface='{}'", interface),
    }
}

mod wire;

pub use linux::{Bus, Handler};
use wire::{bad, Reader};
pub use wire::{decode_body, encode_body, DecodeError, Type, Value, Writer};

mod linux;

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(not(feature = "test-support"), allow(dead_code))]
pub mod fake;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod session_bus_tests;

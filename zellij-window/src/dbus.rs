#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::fmt;

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
pub enum Value {
    Byte(u8),
    Bool(bool),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    Double(f64),
    Str(String),
    Path(String),
    Sig(String),
    Array(String, Vec<Value>),
    Struct(Vec<Value>),
    DictEntry(Box<Value>, Box<Value>),
    Variant(Box<Value>),
}

impl Value {
    pub fn str(text: impl Into<String>) -> Value {
        Value::Str(text.into())
    }

    pub fn variant(value: Value) -> Value {
        Value::Variant(Box::new(value))
    }

    pub fn strings<I, S>(items: I) -> Value
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Value::Array(
            "s".to_owned(),
            items.into_iter().map(|s| Value::Str(s.into())).collect(),
        )
    }

    pub fn bytes(bytes: &[u8]) -> Value {
        Value::Array(
            "y".to_owned(),
            bytes.iter().map(|b| Value::Byte(*b)).collect(),
        )
    }

    pub fn dict(entries: Vec<(&str, Value)>) -> Value {
        Value::Array(
            "{sv}".to_owned(),
            entries
                .into_iter()
                .map(|(key, value)| {
                    Value::DictEntry(Box::new(Value::str(key)), Box::new(Value::variant(value)))
                })
                .collect(),
        )
    }

    pub fn signature(&self) -> String {
        match self {
            Value::Byte(_) => "y".to_owned(),
            Value::Bool(_) => "b".to_owned(),
            Value::I16(_) => "n".to_owned(),
            Value::U16(_) => "q".to_owned(),
            Value::I32(_) => "i".to_owned(),
            Value::U32(_) => "u".to_owned(),
            Value::I64(_) => "x".to_owned(),
            Value::U64(_) => "t".to_owned(),
            Value::Double(_) => "d".to_owned(),
            Value::Str(_) => "s".to_owned(),
            Value::Path(_) => "o".to_owned(),
            Value::Sig(_) => "g".to_owned(),
            Value::Array(element, _) => format!("a{}", element),
            Value::Struct(fields) => {
                let inner: String = fields.iter().map(Value::signature).collect();
                format!("({})", inner)
            },
            Value::DictEntry(key, value) => {
                format!("{{{}{}}}", key.signature(), value.signature())
            },
            Value::Variant(_) => "v".to_owned(),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) | Value::Path(s) | Value::Sig(s) => Some(s),
            Value::Variant(inner) => inner.as_str(),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Value::U32(v) => Some(*v),
            Value::Variant(inner) => inner.as_u32(),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Value::I32(v) => Some(*v),
            Value::Variant(inner) => inner.as_i32(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            Value::Variant(inner) => inner.as_bool(),
            _ => None,
        }
    }

    pub fn items(&self) -> &[Value] {
        match self {
            Value::Array(_, items) | Value::Struct(items) => items,
            Value::Variant(inner) => inner.items(),
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Byte,
    Bool,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    Double,
    Str,
    Path,
    Sig,
    UnixFd,
    Array(Box<Type>),
    Struct(Vec<Type>),
    DictEntry(Box<Type>, Box<Type>),
    Variant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(pub String);

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DecodeError {}

fn bad(text: impl Into<String>) -> DecodeError {
    DecodeError(text.into())
}

impl Type {
    pub fn parse_all(signature: &str) -> Result<Vec<Type>, DecodeError> {
        let bytes = signature.as_bytes();
        let mut at = 0;
        let mut types = vec![];
        while at < bytes.len() {
            types.push(Type::parse_one(bytes, &mut at, 0)?);
        }
        Ok(types)
    }

    fn parse_one(bytes: &[u8], at: &mut usize, depth: usize) -> Result<Type, DecodeError> {
        if depth > 32 {
            return Err(bad("the signature nests too deeply"));
        }
        let Some(code) = bytes.get(*at) else {
            return Err(bad("the signature ended early"));
        };
        *at += 1;
        Ok(match code {
            b'y' => Type::Byte,
            b'b' => Type::Bool,
            b'n' => Type::I16,
            b'q' => Type::U16,
            b'i' => Type::I32,
            b'u' => Type::U32,
            b'x' => Type::I64,
            b't' => Type::U64,
            b'd' => Type::Double,
            b's' => Type::Str,
            b'o' => Type::Path,
            b'g' => Type::Sig,
            b'h' => Type::UnixFd,
            b'v' => Type::Variant,
            b'a' => Type::Array(Box::new(Type::parse_one(bytes, at, depth + 1)?)),
            b'(' => {
                let mut fields = vec![];
                loop {
                    match bytes.get(*at) {
                        Some(b')') => {
                            *at += 1;
                            break;
                        },
                        Some(_) => fields.push(Type::parse_one(bytes, at, depth + 1)?),
                        None => return Err(bad("an unclosed struct in the signature")),
                    }
                }
                Type::Struct(fields)
            },
            b'{' => {
                let key = Type::parse_one(bytes, at, depth + 1)?;
                let value = Type::parse_one(bytes, at, depth + 1)?;
                if bytes.get(*at) != Some(&b'}') {
                    return Err(bad("an unclosed dict entry in the signature"));
                }
                *at += 1;
                Type::DictEntry(Box::new(key), Box::new(value))
            },
            other => {
                return Err(bad(format!(
                    "an unknown type code {:?} in the signature",
                    *other as char
                )))
            },
        })
    }

    fn alignment(&self) -> usize {
        match self {
            Type::Byte | Type::Sig | Type::Variant => 1,
            Type::I16 | Type::U16 => 2,
            Type::Bool | Type::I32 | Type::U32 | Type::Str | Type::Path | Type::UnixFd => 4,
            Type::Array(_) => 4,
            Type::I64 | Type::U64 | Type::Double | Type::Struct(_) | Type::DictEntry(..) => 8,
        }
    }
}

fn alignment_of_signature(signature: &str) -> usize {
    Type::parse_all(signature)
        .ok()
        .and_then(|types| types.first().map(Type::alignment))
        .unwrap_or(1)
}

#[derive(Debug, Default)]
pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Writer::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    fn align(&mut self, to: usize) {
        while self.bytes.len() % to != 0 {
            self.bytes.push(0);
        }
    }

    fn raw_u32(&mut self, value: u32) {
        self.align(4);
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn raw_string(&mut self, value: &str) {
        self.raw_u32(value.len() as u32);
        self.bytes.extend_from_slice(value.as_bytes());
        self.bytes.push(0);
    }

    fn raw_signature(&mut self, value: &str) {
        self.bytes.push(value.len() as u8);
        self.bytes.extend_from_slice(value.as_bytes());
        self.bytes.push(0);
    }

    pub fn value(&mut self, value: &Value) {
        match value {
            Value::Byte(v) => self.bytes.push(*v),
            Value::Bool(v) => self.raw_u32(u32::from(*v)),
            Value::I16(v) => {
                self.align(2);
                self.bytes.extend_from_slice(&v.to_le_bytes());
            },
            Value::U16(v) => {
                self.align(2);
                self.bytes.extend_from_slice(&v.to_le_bytes());
            },
            Value::I32(v) => {
                self.align(4);
                self.bytes.extend_from_slice(&v.to_le_bytes());
            },
            Value::U32(v) => self.raw_u32(*v),
            Value::I64(v) => {
                self.align(8);
                self.bytes.extend_from_slice(&v.to_le_bytes());
            },
            Value::U64(v) => {
                self.align(8);
                self.bytes.extend_from_slice(&v.to_le_bytes());
            },
            Value::Double(v) => {
                self.align(8);
                self.bytes.extend_from_slice(&v.to_le_bytes());
            },
            Value::Str(v) | Value::Path(v) => self.raw_string(v),
            Value::Sig(v) => self.raw_signature(v),
            Value::Array(element, items) => {
                self.align(4);
                let length_at = self.bytes.len();
                self.bytes.extend_from_slice(&0u32.to_le_bytes());
                self.align(alignment_of_signature(element));
                let start = self.bytes.len();
                for item in items {
                    self.value(item);
                }
                let length = (self.bytes.len() - start) as u32;
                self.bytes[length_at..length_at + 4].copy_from_slice(&length.to_le_bytes());
            },
            Value::Struct(fields) => {
                self.align(8);
                for field in fields {
                    self.value(field);
                }
            },
            Value::DictEntry(key, value) => {
                self.align(8);
                self.value(key);
                self.value(value);
            },
            Value::Variant(inner) => {
                self.raw_signature(&inner.signature());
                self.value(inner);
            },
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    big_endian: bool,
}

impl<'a> Reader<'a> {
    fn align(&mut self, to: usize) -> Result<(), DecodeError> {
        let next = self.at.div_ceil(to) * to;
        if next > self.bytes.len() {
            return Err(bad("the message ended inside its padding"));
        }
        self.at = next;
        Ok(())
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .at
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| bad("the message ended early"))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        self.align(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        if self.big_endian {
            out.reverse();
        }
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.fixed::<4>()?))
    }

    fn string(&mut self) -> Result<String, DecodeError> {
        let length = self.u32()? as usize;
        let text = self.take(length)?;
        let text = std::str::from_utf8(text)
            .map_err(|_| bad("a string that is not UTF-8"))?
            .to_owned();
        self.take(1)?;
        Ok(text)
    }

    fn signature(&mut self) -> Result<String, DecodeError> {
        let length = self.take(1)?[0] as usize;
        let text = self.take(length)?;
        let text = std::str::from_utf8(text)
            .map_err(|_| bad("a signature that is not ASCII"))?
            .to_owned();
        self.take(1)?;
        Ok(text)
    }

    fn value(&mut self, kind: &Type, depth: usize) -> Result<Value, DecodeError> {
        if depth > 64 {
            return Err(bad("the message nests too deeply"));
        }
        Ok(match kind {
            Type::Byte => Value::Byte(self.take(1)?[0]),
            Type::Bool => Value::Bool(self.u32()? != 0),
            Type::I16 => Value::I16(i16::from_le_bytes(self.fixed::<2>()?)),
            Type::U16 => Value::U16(u16::from_le_bytes(self.fixed::<2>()?)),
            Type::I32 => Value::I32(i32::from_le_bytes(self.fixed::<4>()?)),
            Type::U32 | Type::UnixFd => Value::U32(self.u32()?),
            Type::I64 => Value::I64(i64::from_le_bytes(self.fixed::<8>()?)),
            Type::U64 => Value::U64(u64::from_le_bytes(self.fixed::<8>()?)),
            Type::Double => Value::Double(f64::from_le_bytes(self.fixed::<8>()?)),
            Type::Str => Value::Str(self.string()?),
            Type::Path => Value::Path(self.string()?),
            Type::Sig => Value::Sig(self.signature()?),
            Type::Array(element) => {
                let length = self.u32()? as usize;
                self.align(element.alignment())?;
                let end = self
                    .at
                    .checked_add(length)
                    .filter(|end| *end <= self.bytes.len())
                    .ok_or_else(|| bad("an array longer than its message"))?;
                let mut items = vec![];
                while self.at < end {
                    items.push(self.value(element, depth + 1)?);
                }
                Value::Array(signature_of(element), items)
            },
            Type::Struct(fields) => {
                self.align(8)?;
                let mut values = vec![];
                for field in fields {
                    values.push(self.value(field, depth + 1)?);
                }
                Value::Struct(values)
            },
            Type::DictEntry(key, value) => {
                self.align(8)?;
                let key = self.value(key, depth + 1)?;
                let value = self.value(value, depth + 1)?;
                Value::DictEntry(Box::new(key), Box::new(value))
            },
            Type::Variant => {
                let signature = self.signature()?;
                let types = Type::parse_all(&signature)?;
                if types.len() != 1 {
                    return Err(bad("a variant that does not hold exactly one value"));
                }
                Value::Variant(Box::new(self.value(&types[0], depth + 1)?))
            },
        })
    }
}

fn signature_of(kind: &Type) -> String {
    match kind {
        Type::Byte => "y".to_owned(),
        Type::Bool => "b".to_owned(),
        Type::I16 => "n".to_owned(),
        Type::U16 => "q".to_owned(),
        Type::I32 => "i".to_owned(),
        Type::U32 => "u".to_owned(),
        Type::I64 => "x".to_owned(),
        Type::U64 => "t".to_owned(),
        Type::Double => "d".to_owned(),
        Type::Str => "s".to_owned(),
        Type::Path => "o".to_owned(),
        Type::Sig => "g".to_owned(),
        Type::UnixFd => "h".to_owned(),
        Type::Variant => "v".to_owned(),
        Type::Array(element) => format!("a{}", signature_of(element)),
        Type::Struct(fields) => {
            let inner: String = fields.iter().map(signature_of).collect();
            format!("({})", inner)
        },
        Type::DictEntry(key, value) => format!("{{{}{}}}", signature_of(key), signature_of(value)),
    }
}

pub fn decode_body(
    signature: &str,
    body: &[u8],
    big_endian: bool,
) -> Result<Vec<Value>, DecodeError> {
    let types = Type::parse_all(signature)?;
    let mut reader = Reader {
        bytes: body,
        at: 0,
        big_endian,
    };
    let mut values = vec![];
    for kind in &types {
        values.push(reader.value(kind, 0)?);
    }
    Ok(values)
}

pub fn encode_body(values: &[Value]) -> (String, Vec<u8>) {
    let mut writer = Writer::new();
    let mut signature = String::new();
    for value in values {
        signature.push_str(&value.signature());
        writer.value(value);
    }
    (signature, writer.into_bytes())
}

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

#[cfg(target_os = "linux")]
pub use linux::{Bus, Handler};

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::net::{SocketAddr, UnixStream};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc::{channel, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use anyhow::{anyhow, bail, Result};

    use super::{
        auth_line, bus_socket, Kind, Message, SocketAddress, Value, BUS_PATH, BUS_SERVICE,
    };

    pub type Handler = Box<dyn FnMut(Message) + Send>;

    #[derive(Clone)]
    pub struct Bus {
        writer: Arc<Mutex<UnixStream>>,
        serial: Arc<AtomicU32>,
        pending: Arc<Mutex<HashMap<u32, Sender<Message>>>>,
    }

    impl Bus {
        pub fn session(make_handler: impl FnOnce(Bus) -> Handler) -> Result<Bus> {
            let stream = connect_session()?;
            Bus::over(stream, make_handler)
        }

        pub fn over(
            mut stream: UnixStream,
            make_handler: impl FnOnce(Bus) -> Handler,
        ) -> Result<Bus> {
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            authenticate(&mut stream)?;
            let reader = stream.try_clone()?;
            let bus = Bus {
                writer: Arc::new(Mutex::new(stream)),
                serial: Arc::new(AtomicU32::new(0)),
                pending: Arc::new(Mutex::new(HashMap::new())),
            };
            let handler = make_handler(bus.clone());
            let pending = bus.pending.clone();
            std::thread::Builder::new()
                .name("dbus_reader".to_owned())
                .spawn(move || read_loop(reader, pending, handler))?;
            bus.call(
                Message::call(BUS_SERVICE, BUS_PATH, BUS_SERVICE, "Hello", vec![]),
                Duration::from_secs(2),
            )?;
            Ok(bus)
        }

        fn next_serial(&self) -> u32 {
            self.serial
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1)
                .max(1)
        }

        pub fn send(&self, message: &Message) -> Result<u32> {
            let serial = self.next_serial();
            let bytes = message.encode(serial);
            let mut writer = self
                .writer
                .lock()
                .map_err(|_| anyhow!("the bus writer is poisoned"))?;
            writer.write_all(&bytes)?;
            Ok(serial)
        }

        pub fn call(&self, message: Message, timeout: Duration) -> Result<Message> {
            let serial = self.next_serial();
            let (sender, receiver) = channel();
            self.pending
                .lock()
                .map_err(|_| anyhow!("the bus is poisoned"))?
                .insert(serial, sender);
            let bytes = message.encode(serial);
            let written = self
                .writer
                .lock()
                .map_err(|_| anyhow!("the bus writer is poisoned"))
                .and_then(|mut writer| writer.write_all(&bytes).map_err(Into::into));
            if let Err(e) = written {
                self.forget(serial);
                return Err(e);
            }
            let answer = receiver.recv_timeout(timeout);
            self.forget(serial);
            let answer = answer.map_err(|_| {
                anyhow!(
                    "{} did not answer {}",
                    message.destination.as_deref().unwrap_or("the bus"),
                    message.member.as_deref().unwrap_or("a call")
                )
            })?;
            if answer.kind == Kind::Error {
                bail!(
                    "{}: {}",
                    answer.error_name.as_deref().unwrap_or("error"),
                    answer.body.first().and_then(Value::as_str).unwrap_or("")
                );
            }
            Ok(answer)
        }

        fn forget(&self, serial: u32) {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&serial);
            }
        }

        pub fn add_match(&self, rule: &str) -> Result<()> {
            self.call(
                Message::call(
                    BUS_SERVICE,
                    BUS_PATH,
                    BUS_SERVICE,
                    "AddMatch",
                    vec![Value::str(rule)],
                ),
                Duration::from_secs(2),
            )
            .map(|_| ())
        }

        pub fn request_name(&self, name: &str) -> Result<bool> {
            let answer = self.call(
                Message::call(
                    BUS_SERVICE,
                    BUS_PATH,
                    BUS_SERVICE,
                    "RequestName",
                    vec![Value::str(name), Value::U32(4)],
                ),
                Duration::from_secs(2),
            )?;
            Ok(matches!(
                answer.body.first().and_then(Value::as_u32),
                Some(1) | Some(4)
            ))
        }

        pub fn name_has_owner(&self, name: &str) -> bool {
            self.call(
                Message::call(
                    BUS_SERVICE,
                    BUS_PATH,
                    BUS_SERVICE,
                    "NameHasOwner",
                    vec![Value::str(name)],
                ),
                Duration::from_secs(2),
            )
            .ok()
            .and_then(|answer| answer.body.first().and_then(Value::as_bool))
            .unwrap_or(false)
        }

        pub fn shutdown(&self) {
            if let Ok(writer) = self.writer.lock() {
                let _ = writer.shutdown(std::net::Shutdown::Both);
            }
        }
    }

    fn read_loop(
        mut reader: UnixStream,
        pending: Arc<Mutex<HashMap<u32, Sender<Message>>>>,
        mut handler: Handler,
    ) {
        loop {
            let mut header = [0u8; 16];
            if reader.read_exact(&mut header).is_err() {
                break;
            }
            let total = match Message::needed(&header) {
                Ok(total) => total,
                Err(e) => {
                    report!("the session bus sent an unreadable message: {}", e);
                    break;
                },
            };
            let mut bytes = header.to_vec();
            bytes.resize(total, 0);
            if reader.read_exact(&mut bytes[16..]).is_err() {
                break;
            }
            let message = match Message::decode(&bytes) {
                Ok(message) => message,
                Err(e) => {
                    report!("skipping an unreadable bus message: {}", e);
                    continue;
                },
            };
            if matches!(message.kind, Kind::MethodReturn | Kind::Error) {
                if let Some(reply_serial) = message.reply_serial {
                    let waiting = pending
                        .lock()
                        .ok()
                        .and_then(|mut p| p.remove(&reply_serial));
                    if let Some(waiting) = waiting {
                        let _ = waiting.send(message);
                        continue;
                    }
                }
            }
            handler(message);
        }
        if let Ok(mut pending) = pending.lock() {
            pending.clear();
        }
    }

    fn connect_session() -> Result<UnixStream> {
        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
        let stream = match address.as_deref().and_then(bus_socket) {
            Some(SocketAddress::Path(path)) => UnixStream::connect(path)?,
            Some(SocketAddress::Abstract(name)) => {
                let address = SocketAddr::from_abstract_name(name.as_bytes())?;
                UnixStream::connect_addr(&address)?
            },
            None => {
                let runtime = std::env::var("XDG_RUNTIME_DIR")
                    .map_err(|_| anyhow!("no session bus address is known"))?;
                UnixStream::connect(format!("{}/bus", runtime))?
            },
        };
        Ok(stream)
    }

    fn authenticate(stream: &mut UnixStream) -> Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        let uid = std::fs::metadata("/proc/self")
            .map(|metadata| metadata.uid())
            .unwrap_or(0);
        stream.write_all(&[0])?;
        stream.write_all(auth_line(uid).as_bytes())?;
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while line.len() < 512 {
            stream.read_exact(&mut byte)?;
            if byte[0] == b'\n' {
                break;
            }
            if byte[0] != b'\r' {
                line.push(byte[0]);
            }
        }
        let reply = String::from_utf8_lossy(&line);
        if !reply.starts_with("OK") {
            bail!("the bus answered {:?} to the authentication", reply);
        }
        stream.write_all(b"BEGIN\r\n")?;
        stream.set_read_timeout(None)?;
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
pub mod fake {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::{Kind, Message, Value, BUS_SERVICE};

    #[derive(Clone)]
    pub struct FakeBus {
        writer: Arc<Mutex<UnixStream>>,
        calls: Arc<Mutex<Vec<Message>>>,
        serial: Arc<Mutex<u32>>,
    }

    impl FakeBus {
        pub fn serve(
            stream: UnixStream,
            answer: impl Fn(&Message, &FakeBus) -> Option<Vec<Value>> + Send + 'static,
        ) -> FakeBus {
            let reader = stream.try_clone().unwrap();
            let bus = FakeBus {
                writer: Arc::new(Mutex::new(stream)),
                calls: Arc::default(),
                serial: Arc::new(Mutex::new(1000)),
            };
            let served = bus.clone();
            std::thread::spawn(move || served.run(reader, answer));
            bus
        }

        pub fn signal(&self, mut message: Message) {
            message.sender = Some(BUS_SERVICE.to_owned());
            self.write(&message);
        }

        fn write(&self, message: &Message) {
            let serial = {
                let mut serial = self.serial.lock().unwrap();
                *serial += 1;
                *serial
            };
            let _ = self
                .writer
                .lock()
                .unwrap()
                .write_all(&message.encode(serial));
        }

        pub fn calls(&self) -> Vec<Message> {
            let deadline = Instant::now() + Duration::from_millis(200);
            while Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            self.calls.lock().unwrap().clone()
        }

        fn run(
            &self,
            mut reader: UnixStream,
            answer: impl Fn(&Message, &FakeBus) -> Option<Vec<Value>>,
        ) {
            let mut nul = [0u8; 1];
            if reader.read_exact(&mut nul).is_err() {
                return;
            }
            let mut line = Vec::new();
            let mut byte = [0u8; 1];
            while reader.read_exact(&mut byte).is_ok() && byte[0] != b'\n' {
                line.push(byte[0]);
            }
            let _ = self
                .writer
                .lock()
                .unwrap()
                .write_all(b"OK 0123456789abcdef\r\n");
            let mut begin = Vec::new();
            while reader.read_exact(&mut byte).is_ok() && byte[0] != b'\n' {
                begin.push(byte[0]);
            }
            loop {
                let mut header = [0u8; 16];
                if reader.read_exact(&mut header).is_err() {
                    return;
                }
                let Ok(total) = Message::needed(&header) else {
                    return;
                };
                let mut bytes = header.to_vec();
                bytes.resize(total, 0);
                if reader.read_exact(&mut bytes[16..]).is_err() {
                    return;
                }
                let Ok(mut call) = Message::decode(&bytes) else {
                    return;
                };
                call.sender = Some(":1.5".to_owned());
                if call.kind != Kind::MethodCall {
                    continue;
                }
                self.calls.lock().unwrap().push(call.clone());
                if call.is(BUS_SERVICE, "Hello") {
                    let mut acquired = Message::signal(
                        super::BUS_PATH,
                        BUS_SERVICE,
                        "NameAcquired",
                        vec![Value::str(":1.5")],
                    );
                    acquired.sender = Some(BUS_SERVICE.to_owned());
                    self.write(&acquired);
                    self.write(&Message::reply_to(&call, vec![Value::str(":1.5")]));
                    continue;
                }
                if call.is(BUS_SERVICE, "AddMatch") {
                    self.write(&Message::reply_to(&call, vec![]));
                    continue;
                }
                let reply = match answer(&call, self) {
                    Some(body) => Message::reply_to(&call, body),
                    None => Message::error_to(&call, "org.example.Unknown", "unknown"),
                };
                self.write(&reply);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(message: &Message) -> Message {
        let bytes = message.encode(42);
        let mut header = [0u8; 16];
        header.copy_from_slice(&bytes[..16]);
        assert_eq!(Message::needed(&header).unwrap(), bytes.len());
        Message::decode(&bytes).unwrap()
    }

    #[test]
    fn a_call_with_every_kind_of_value_survives_encoding() {
        let body = vec![
            Value::str("zellij"),
            Value::U32(7),
            Value::Bool(true),
            Value::I32(-3),
            Value::I64(-9_000_000_000),
            Value::U64(9_000_000_000),
            Value::Double(1.5),
            Value::Byte(9),
            Value::I16(-2),
            Value::U16(2),
            Value::Path("/StatusNotifierItem".to_owned()),
            Value::Sig("a{sv}".to_owned()),
            Value::strings(["default", "Open"]),
            Value::dict(vec![("urgency", Value::Byte(1)), ("x", Value::I32(4))]),
            Value::Array(
                "(iiay)".to_owned(),
                vec![Value::Struct(vec![
                    Value::I32(2),
                    Value::I32(1),
                    Value::bytes(&[1, 2, 3, 4, 5, 6, 7, 8]),
                ])],
            ),
            Value::Array("s".to_owned(), vec![]),
            Value::variant(Value::Struct(vec![Value::str("a"), Value::U32(1)])),
        ];
        let message = Message::call("org.example", "/a", "org.example.I", "Do", body.clone());
        let decoded = roundtrip(&message);
        assert_eq!(decoded.kind, Kind::MethodCall);
        assert_eq!(decoded.serial, 42);
        assert_eq!(decoded.destination.as_deref(), Some("org.example"));
        assert_eq!(decoded.path.as_deref(), Some("/a"));
        assert!(decoded.is("org.example.I", "Do"));
        assert_eq!(decoded.body, body);
    }

    #[test]
    fn replies_and_errors_carry_the_serial_they_answer() {
        let mut call = Message::call("a.b", "/", "a.b", "M", vec![]);
        call.serial = 5;
        call.sender = Some(":1.9".to_owned());
        let reply = roundtrip(&Message::reply_to(&call, vec![Value::U32(1)]));
        assert_eq!(reply.kind, Kind::MethodReturn);
        assert_eq!(reply.reply_serial, Some(5));
        assert_eq!(reply.destination.as_deref(), Some(":1.9"));
        let error = roundtrip(&Message::error_to(&call, "a.b.Failed", "no"));
        assert_eq!(error.kind, Kind::Error);
        assert_eq!(error.error_name.as_deref(), Some("a.b.Failed"));
        assert_eq!(error.body, vec![Value::str("no")]);
    }

    #[test]
    fn a_signal_names_its_interface_and_member() {
        let signal = roundtrip(&Message::signal(
            "/StatusNotifierItem",
            "org.kde.StatusNotifierItem",
            "NewIcon",
            vec![],
        ));
        assert_eq!(signal.kind, Kind::Signal);
        assert!(signal.is("org.kde.StatusNotifierItem", "NewIcon"));
        assert!(signal.body.is_empty());
    }

    #[test]
    fn a_big_endian_message_is_read_as_well() {
        let mut bytes = vec![b'B', 4, 0, 1];
        let body = {
            let mut body = vec![];
            body.extend_from_slice(&7u32.to_be_bytes());
            body
        };
        let fields = {
            let mut fields = vec![];
            fields.push(FIELD_SIGNATURE);
            fields.extend_from_slice(&[1, b'g', 0]);
            fields.extend_from_slice(&[1, b'u', 0]);
            fields
        };
        bytes.extend_from_slice(&(body.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&3u32.to_be_bytes());
        bytes.extend_from_slice(&(fields.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&fields);
        while bytes.len() % 8 != 0 {
            bytes.push(0);
        }
        bytes.extend_from_slice(&body);
        let message = Message::decode(&bytes).unwrap();
        assert_eq!(message.serial, 3);
        assert_eq!(message.body, vec![Value::U32(7)]);
    }

    #[test]
    fn truncated_and_hostile_messages_are_refused() {
        let bytes = Message::call("a.b", "/", "a.b", "M", vec![Value::str("x")]).encode(1);
        assert!(Message::decode(&bytes[..bytes.len() - 2]).is_err());
        assert!(decode_body("a", &[], false).is_err());
        assert!(decode_body("(i", &[], false).is_err());
        assert!(decode_body("s", &[200, 0, 0, 0, b'a'], false).is_err());
        let mut header = [0u8; 16];
        header[0] = b'l';
        header[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Message::needed(&header).is_err());
    }

    #[test]
    fn a_bus_address_is_read_for_its_socket() {
        assert_eq!(
            bus_socket("unix:path=/run/user/1000/bus"),
            Some(SocketAddress::Path("/run/user/1000/bus"))
        );
        assert_eq!(
            bus_socket("unix:abstract=/tmp/dbus-AbCdEf,guid=0123"),
            Some(SocketAddress::Abstract("/tmp/dbus-AbCdEf"))
        );
        assert_eq!(
            bus_socket("tcp:host=localhost,port=1;unix:path=/run/bus"),
            Some(SocketAddress::Path("/run/bus"))
        );
        assert_eq!(bus_socket("tcp:host=localhost,port=1"), None);
    }

    #[test]
    fn the_external_handshake_offers_the_hex_encoded_user_id() {
        assert_eq!(auth_line(1000), "AUTH EXTERNAL 31303030\r\n");
        assert_eq!(auth_line(0), "AUTH EXTERNAL 30\r\n");
    }
}

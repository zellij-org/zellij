use std::fmt;

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

pub(super) fn bad(text: impl Into<String>) -> DecodeError {
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

    pub(super) fn parse_one(
        bytes: &[u8],
        at: &mut usize,
        depth: usize,
    ) -> Result<Type, DecodeError> {
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

    pub(super) fn alignment(&self) -> usize {
        match self {
            Type::Byte | Type::Sig | Type::Variant => 1,
            Type::I16 | Type::U16 => 2,
            Type::Bool | Type::I32 | Type::U32 | Type::Str | Type::Path | Type::UnixFd => 4,
            Type::Array(_) => 4,
            Type::I64 | Type::U64 | Type::Double | Type::Struct(_) | Type::DictEntry(..) => 8,
        }
    }
}

pub(super) fn alignment_of_signature(signature: &str) -> usize {
    Type::parse_all(signature)
        .ok()
        .and_then(|types| types.first().map(Type::alignment))
        .unwrap_or(1)
}

#[derive(Debug, Default)]
pub struct Writer {
    pub(super) bytes: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Writer::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub(super) fn align(&mut self, to: usize) {
        while self.bytes.len() % to != 0 {
            self.bytes.push(0);
        }
    }

    pub(super) fn raw_u32(&mut self, value: u32) {
        self.align(4);
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn raw_string(&mut self, value: &str) {
        self.raw_u32(value.len() as u32);
        self.bytes.extend_from_slice(value.as_bytes());
        self.bytes.push(0);
    }

    pub(super) fn raw_signature(&mut self, value: &str) {
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

pub(super) struct Reader<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) at: usize,
    pub(super) big_endian: bool,
}

impl<'a> Reader<'a> {
    pub(super) fn align(&mut self, to: usize) -> Result<(), DecodeError> {
        let next = self.at.div_ceil(to) * to;
        if next > self.bytes.len() {
            return Err(bad("the message ended inside its padding"));
        }
        self.at = next;
        Ok(())
    }

    pub(super) fn take(&mut self, count: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .at
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| bad("the message ended early"))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    pub(super) fn fixed<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        self.align(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        if self.big_endian {
            out.reverse();
        }
        Ok(out)
    }

    pub(super) fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.fixed::<4>()?))
    }

    pub(super) fn string(&mut self) -> Result<String, DecodeError> {
        let length = self.u32()? as usize;
        let text = self.take(length)?;
        let text = std::str::from_utf8(text)
            .map_err(|_| bad("a string that is not UTF-8"))?
            .to_owned();
        self.take(1)?;
        Ok(text)
    }

    pub(super) fn signature(&mut self) -> Result<String, DecodeError> {
        let length = self.take(1)?[0] as usize;
        let text = self.take(length)?;
        let text = std::str::from_utf8(text)
            .map_err(|_| bad("a signature that is not ASCII"))?
            .to_owned();
        self.take(1)?;
        Ok(text)
    }

    pub(super) fn value(&mut self, kind: &Type, depth: usize) -> Result<Value, DecodeError> {
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

pub(super) fn signature_of(kind: &Type) -> String {
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

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use zellij_utils::input::window::NotificationMode;

use crate::kitty::Notification;

pub const APP_NAME: &str = "zellij";

pub fn handled(mode: NotificationMode, notification: &Notification) -> bool {
    match mode {
        NotificationMode::None => true,
        NotificationMode::Attention => false,
        NotificationMode::Desktop => deliver(notification),
    }
}

const NOTIFICATIONS_SERVICE: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";
const BUS_SERVICE: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";

const MESSAGE_TYPE_METHOD_CALL: u8 = 1;
const MESSAGE_TYPE_ERROR: u8 = 3;
const FIELD_PATH: u8 = 1;
const FIELD_INTERFACE: u8 = 2;
const FIELD_MEMBER: u8 = 3;
const FIELD_DESTINATION: u8 = 6;
const FIELD_SIGNATURE: u8 = 8;

const NOTIFY_SIGNATURE: &str = "susssasa{sv}i";
const EXPIRE_DEFAULT: i32 = -1;

#[derive(Debug, Default)]
pub struct Marshal {
    bytes: Vec<u8>,
}

impl Marshal {
    pub fn new() -> Self {
        Marshal::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    fn align(&mut self, to: usize) {
        while self.bytes.len() % to != 0 {
            self.bytes.push(0);
        }
    }

    fn byte(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.align(4);
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.align(4);
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn string(&mut self, value: &str) {
        self.u32(value.len() as u32);
        self.bytes.extend_from_slice(value.as_bytes());
        self.bytes.push(0);
    }

    fn signature(&mut self, value: &str) {
        self.byte(value.len() as u8);
        self.bytes.extend_from_slice(value.as_bytes());
        self.bytes.push(0);
    }

    fn array(&mut self, element_alignment: usize, write: impl FnOnce(&mut Marshal)) {
        self.align(4);
        let length_at = self.bytes.len();
        self.bytes.extend_from_slice(&0u32.to_le_bytes());
        self.align(element_alignment);
        let start = self.bytes.len();
        write(self);
        let length = (self.bytes.len() - start) as u32;
        self.bytes[length_at..length_at + 4].copy_from_slice(&length.to_le_bytes());
    }

    fn field(&mut self, code: u8, signature: &str, value: &str) {
        self.align(8);
        self.byte(code);
        self.signature(signature);
        match signature {
            "g" => self.signature(value),
            _ => self.string(value),
        }
    }
}

pub fn notify_body(notification: &Notification) -> Vec<u8> {
    let mut body = Marshal::new();
    body.string(APP_NAME);
    body.u32(0);
    body.string("");
    match &notification.title {
        Some(title) => {
            body.string(title);
            body.string(&notification.body);
        },
        None => {
            body.string(&notification.body);
            body.string("");
        },
    }
    body.array(4, |_| {});
    body.array(8, |_| {});
    body.i32(EXPIRE_DEFAULT);
    body.into_bytes()
}

pub fn method_call(
    serial: u32,
    destination: &str,
    path: &str,
    interface: &str,
    member: &str,
    signature: Option<&str>,
    body: &[u8],
) -> Vec<u8> {
    let mut message = Marshal::new();
    message.byte(b'l');
    message.byte(MESSAGE_TYPE_METHOD_CALL);
    message.byte(0);
    message.byte(1);
    message
        .bytes
        .extend_from_slice(&(body.len() as u32).to_le_bytes());
    message.bytes.extend_from_slice(&serial.to_le_bytes());
    message.array(8, |fields| {
        fields.field(FIELD_PATH, "o", path);
        fields.field(FIELD_INTERFACE, "s", interface);
        fields.field(FIELD_MEMBER, "s", member);
        fields.field(FIELD_DESTINATION, "s", destination);
        if let Some(signature) = signature {
            fields.field(FIELD_SIGNATURE, "g", signature);
        }
    });
    message.align(8);
    let mut bytes = message.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

pub fn notify_call(serial: u32, notification: &Notification) -> Vec<u8> {
    method_call(
        serial,
        NOTIFICATIONS_SERVICE,
        NOTIFICATIONS_PATH,
        NOTIFICATIONS_SERVICE,
        "Notify",
        Some(NOTIFY_SIGNATURE),
        &notify_body(notification),
    )
}

pub fn hello_call(serial: u32) -> Vec<u8> {
    method_call(
        serial,
        BUS_SERVICE,
        BUS_PATH,
        BUS_SERVICE,
        "Hello",
        None,
        &[],
    )
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

#[cfg(target_os = "linux")]
pub use linux::deliver;

#[cfg(not(target_os = "linux"))]
pub use elsewhere::deliver;

#[cfg(not(target_os = "linux"))]
mod elsewhere {
    use super::Notification;

    pub fn deliver(_notification: &Notification) -> bool {
        false
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::net::{SocketAddr, UnixStream};
    use std::time::Duration;

    use super::{
        auth_line, bus_socket, hello_call, notify_call, Notification, SocketAddress,
        MESSAGE_TYPE_ERROR,
    };

    const TIMEOUT: Duration = Duration::from_millis(500);
    const MAX_REPLY_BYTES: usize = 1 << 20;

    thread_local! {
        static BUS: RefCell<Option<Bus>> = const { RefCell::new(None) };
        static COMPLAINED: RefCell<bool> = const { RefCell::new(false) };
    }

    struct Bus {
        stream: UnixStream,
        serial: u32,
    }

    pub fn deliver(notification: &Notification) -> bool {
        BUS.with(|bus| {
            let mut bus = bus.borrow_mut();
            if bus.is_none() {
                *bus = Bus::connect();
            }
            let Some(open) = bus.as_mut() else {
                complain("no session bus could be reached");
                return false;
            };
            match open.notify(notification) {
                Ok(true) => true,
                Ok(false) => {
                    complain("the notification service refused the call");
                    false
                },
                Err(e) => {
                    *bus = None;
                    complain(&format!("the session bus dropped the call: {}", e));
                    false
                },
            }
        })
    }

    fn complain(reason: &str) {
        COMPLAINED.with(|complained| {
            let mut complained = complained.borrow_mut();
            if *complained {
                return;
            }
            *complained = true;
            eprintln!(
                "zellij-window: notifications stay with the window manager: {}",
                reason
            );
        });
    }

    impl Bus {
        fn connect() -> Option<Bus> {
            let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
            let stream = match address.as_deref().and_then(bus_socket) {
                Some(SocketAddress::Path(path)) => UnixStream::connect(path).ok()?,
                Some(SocketAddress::Abstract(name)) => {
                    let address = SocketAddr::from_abstract_name(name.as_bytes()).ok()?;
                    UnixStream::connect_addr(&address).ok()?
                },
                None => {
                    let runtime = std::env::var("XDG_RUNTIME_DIR").ok()?;
                    UnixStream::connect(format!("{}/bus", runtime)).ok()?
                },
            };
            stream.set_read_timeout(Some(TIMEOUT)).ok()?;
            stream.set_write_timeout(Some(TIMEOUT)).ok()?;
            let mut bus = Bus { stream, serial: 0 };
            bus.authenticate().ok()?;
            bus.hello().ok()?;
            Some(bus)
        }

        fn authenticate(&mut self) -> std::io::Result<()> {
            let uid = std::fs::metadata("/proc/self")
                .map(|metadata| metadata.uid())
                .unwrap_or(0);
            self.stream.write_all(&[0])?;
            self.stream.write_all(auth_line(uid).as_bytes())?;
            let reply = self.read_line()?;
            if !reply.starts_with("OK") {
                return Err(std::io::Error::other(format!(
                    "the bus answered {:?} to the authentication",
                    reply
                )));
            }
            self.stream.write_all(b"BEGIN\r\n")
        }

        fn read_line(&mut self) -> std::io::Result<String> {
            let mut line = Vec::new();
            let mut byte = [0u8; 1];
            while line.len() < 512 {
                self.stream.read_exact(&mut byte)?;
                if byte[0] == b'\n' {
                    break;
                }
                if byte[0] != b'\r' {
                    line.push(byte[0]);
                }
            }
            Ok(String::from_utf8_lossy(&line).into_owned())
        }

        fn next_serial(&mut self) -> u32 {
            self.serial += 1;
            self.serial
        }

        fn hello(&mut self) -> std::io::Result<()> {
            let serial = self.next_serial();
            self.stream.write_all(&hello_call(serial))?;
            self.read_reply().map(|_| ())
        }

        fn notify(&mut self, notification: &Notification) -> std::io::Result<bool> {
            let serial = self.next_serial();
            self.stream.write_all(&notify_call(serial, notification))?;
            self.read_reply()
        }

        fn read_reply(&mut self) -> std::io::Result<bool> {
            let mut header = [0u8; 16];
            self.stream.read_exact(&mut header)?;
            let big_endian = header[0] == b'B';
            let word = |bytes: &[u8]| {
                let word = [bytes[0], bytes[1], bytes[2], bytes[3]];
                match big_endian {
                    true => u32::from_be_bytes(word),
                    false => u32::from_le_bytes(word),
                }
            };
            let body_len = word(&header[4..8]) as usize;
            let fields_len = word(&header[12..16]) as usize;
            let padded_fields = fields_len.div_ceil(8) * 8;
            let remaining = padded_fields.saturating_add(body_len);
            if remaining > MAX_REPLY_BYTES {
                return Err(std::io::Error::other(
                    "the bus replied with an absurd message",
                ));
            }
            let mut rest = vec![0u8; remaining];
            self.stream.read_exact(&mut rest)?;
            Ok(header[1] != MESSAGE_TYPE_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    fn aligned(at: usize, to: usize) -> usize {
        at.div_ceil(to) * to
    }

    fn word_at(bytes: &[u8], at: usize) -> (u32, usize) {
        let at = aligned(at, 4);
        (word(bytes, at), at + 4)
    }

    fn string_at(bytes: &[u8], at: usize) -> (String, usize) {
        let (length, start) = word_at(bytes, at);
        let length = length as usize;
        let text = String::from_utf8(bytes[start..start + length].to_vec()).expect("utf8");
        (text, start + length + 1)
    }

    fn notification(title: Option<&str>, body: &str) -> Notification {
        Notification {
            title: title.map(str::to_owned),
            body: body.to_owned(),
        }
    }

    #[test]
    fn a_notify_call_carries_the_title_as_the_summary_and_the_text_as_the_body() {
        let body = notify_body(&notification(Some("Build"), "finished"));
        let (app, at) = string_at(&body, 0);
        assert_eq!(app, APP_NAME);
        let (replaces, at) = word_at(&body, at);
        assert_eq!(replaces, 0, "no notification is being replaced");
        let (icon, at) = string_at(&body, at);
        assert_eq!(icon, "");
        let (summary, at) = string_at(&body, at);
        let (text, _) = string_at(&body, at);
        assert_eq!((summary.as_str(), text.as_str()), ("Build", "finished"));
    }

    #[test]
    fn a_notification_without_a_title_becomes_the_summary_itself() {
        let body = notify_body(&notification(None, "finished"));
        let (_, at) = string_at(&body, 0);
        let (_, at) = word_at(&body, at);
        let (_, at) = string_at(&body, at);
        let (summary, at) = string_at(&body, at);
        let (text, _) = string_at(&body, at);
        assert_eq!(
            (summary.as_str(), text.as_str()),
            ("finished", ""),
            "a body with no title reads as the summary, the way every notifier shows it"
        );
    }

    #[test]
    fn a_notify_call_is_a_well_formed_method_call() {
        let call = notify_call(7, &notification(Some("Build"), "finished"));
        assert_eq!(call[0], b'l');
        assert_eq!(call[1], MESSAGE_TYPE_METHOD_CALL);
        assert_eq!(call[3], 1, "protocol version");
        assert_eq!(word(&call, 8), 7, "serial");

        let body_len = word(&call, 4) as usize;
        let fields_len = word(&call, 12) as usize;
        let body_at = (16 + fields_len).div_ceil(8) * 8;
        assert_eq!(
            call.len(),
            body_at + body_len,
            "the declared body length must account for every byte after the header"
        );
        assert_eq!(
            &call[body_at..],
            notify_body(&notification(Some("Build"), "finished")).as_slice()
        );

        let header = String::from_utf8_lossy(&call[16..body_at]).into_owned();
        for expected in [
            NOTIFICATIONS_PATH,
            NOTIFICATIONS_SERVICE,
            "Notify",
            NOTIFY_SIGNATURE,
        ] {
            assert!(
                header.contains(expected),
                "the header must name {:?}, got {:?}",
                expected,
                header
            );
        }
    }

    #[test]
    fn every_marshalled_value_sits_on_its_own_alignment() {
        let call = notify_call(1, &notification(Some("a"), "b"));
        let fields_len = word(&call, 12) as usize;
        let body_at = (16 + fields_len).div_ceil(8) * 8;
        assert_eq!(
            body_at % 8,
            0,
            "a dbus body starts on an eight-byte boundary"
        );
        let body = &call[body_at..];
        assert_eq!(body.len() % 4, 0);
    }

    #[test]
    fn a_hello_call_names_the_bus_and_carries_no_body() {
        let call = hello_call(1);
        assert_eq!(word(&call, 4), 0, "Hello takes no arguments");
        let header = String::from_utf8_lossy(&call[16..]).into_owned();
        assert!(header.contains(BUS_PATH));
        assert!(header.contains("Hello"));
        assert!(
            !header.contains("susssasa"),
            "a call with no body must not declare a signature"
        );
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
            Some(SocketAddress::Path("/run/bus")),
            "a transport this client cannot speak is skipped, not fatal"
        );
        assert_eq!(bus_socket("tcp:host=localhost,port=1"), None);
        assert_eq!(bus_socket(""), None);
    }

    #[test]
    fn the_external_handshake_offers_the_hex_encoded_user_id() {
        assert_eq!(auth_line(1000), "AUTH EXTERNAL 31303030\r\n");
        assert_eq!(auth_line(0), "AUTH EXTERNAL 30\r\n");
    }
}

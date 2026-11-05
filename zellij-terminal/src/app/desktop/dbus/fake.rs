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

pub const WATCHER_SERVICE: &str = "org.kde.StatusNotifierWatcher";
pub const NOTIFICATIONS: &str = "org.freedesktop.Notifications";

type Answer = Box<dyn Fn(&Message) -> Option<Vec<Value>> + Send + Sync>;

struct Connection {
    unique: String,
    writer: Arc<Mutex<UnixStream>>,
}

struct Shared {
    connections: Mutex<Vec<Connection>>,
    owners: Mutex<std::collections::HashMap<String, String>>,
    calls: Mutex<Vec<Message>>,
    pending: Mutex<std::collections::HashMap<u32, std::sync::mpsc::Sender<Message>>>,
    serial: Mutex<u32>,
    next_connection: Mutex<u32>,
    next_notification: Mutex<u32>,
    answer: Mutex<Option<Answer>>,
}

#[derive(Clone)]
pub struct FakeSessionBus {
    shared: Arc<Shared>,
    path: std::path::PathBuf,
}

impl FakeSessionBus {
    pub fn listen(path: impl Into<std::path::PathBuf>) -> FakeSessionBus {
        let path = path.into();
        let _ = std::fs::remove_file(&path);
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let bus = FakeSessionBus {
            shared: Arc::new(Shared {
                connections: Mutex::default(),
                owners: Mutex::default(),
                calls: Mutex::default(),
                pending: Mutex::default(),
                serial: Mutex::new(5000),
                next_connection: Mutex::new(100),
                next_notification: Mutex::new(0),
                answer: Mutex::new(None),
            }),
            path,
        };
        let accepting = bus.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else {
                    return;
                };
                let served = accepting.clone();
                std::thread::spawn(move || served.serve(stream));
            }
        });
        bus
    }

    pub fn address(&self) -> String {
        format!("unix:path={}", self.path.display())
    }

    pub fn answer_with(
        &self,
        answer: impl Fn(&Message) -> Option<Vec<Value>> + Send + Sync + 'static,
    ) {
        *self.shared.answer.lock().unwrap() = Some(Box::new(answer));
    }

    pub fn calls(&self) -> Vec<Message> {
        self.shared.calls.lock().unwrap().clone()
    }

    pub fn calls_to(&self, interface: &str, member: &str) -> Vec<Message> {
        self.calls()
            .into_iter()
            .filter(|call| call.is(interface, member))
            .collect()
    }

    pub fn connection_count(&self) -> usize {
        self.shared.connections.lock().unwrap().len()
    }

    pub fn signal(&self, mut message: Message) {
        message.sender = Some(BUS_SERVICE.to_owned());
        let writers: Vec<Arc<Mutex<UnixStream>>> = self
            .shared
            .connections
            .lock()
            .unwrap()
            .iter()
            .map(|connection| connection.writer.clone())
            .collect();
        for writer in writers {
            self.write_to(&writer, &message);
        }
    }

    pub fn call_client(
        &self,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        body: Vec<Value>,
    ) -> Result<Message, String> {
        let unique = self
            .shared
            .owners
            .lock()
            .unwrap()
            .get(destination)
            .cloned()
            .unwrap_or_else(|| destination.to_owned());
        let writer = self
            .shared
            .connections
            .lock()
            .unwrap()
            .iter()
            .find(|connection| connection.unique == unique)
            .map(|connection| connection.writer.clone())
            .ok_or_else(|| format!("nobody owns {}", destination))?;
        let mut call = Message::call(destination, path, interface, member, body);
        call.sender = Some(":1.1".to_owned());
        let serial = self.next_serial();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.shared.pending.lock().unwrap().insert(serial, sender);
        let _ = writer.lock().unwrap().write_all(&call.encode(serial));
        let reply = receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| format!("{} did not answer {}", destination, member));
        self.shared.pending.lock().unwrap().remove(&serial);
        let reply = reply?;
        if reply.kind == Kind::Error {
            return Err(format!(
                "{}: {:?}",
                reply.error_name.unwrap_or_default(),
                reply.body
            ));
        }
        Ok(reply)
    }

    fn next_serial(&self) -> u32 {
        let mut serial = self.shared.serial.lock().unwrap();
        *serial += 1;
        *serial
    }

    fn write_to(&self, writer: &Mutex<UnixStream>, message: &Message) {
        let serial = self.next_serial();
        let _ = writer.lock().unwrap().write_all(&message.encode(serial));
    }

    fn built_in(&self, call: &Message, unique: &str) -> Option<Vec<Value>> {
        let interface = call.interface.as_deref().unwrap_or("");
        let member = call.member.as_deref().unwrap_or("");
        let first = call.body.first().and_then(Value::as_str).unwrap_or("");
        match (interface, member) {
            (BUS_SERVICE, "Hello") => Some(vec![Value::str(unique)]),
            (BUS_SERVICE, "AddMatch") | (BUS_SERVICE, "RemoveMatch") => Some(vec![]),
            (BUS_SERVICE, "RequestName") => {
                self.shared
                    .owners
                    .lock()
                    .unwrap()
                    .insert(first.to_owned(), unique.to_owned());
                Some(vec![Value::U32(1)])
            },
            (BUS_SERVICE, "ReleaseName") => {
                self.shared.owners.lock().unwrap().remove(first);
                Some(vec![Value::U32(1)])
            },
            (BUS_SERVICE, "NameHasOwner") => Some(vec![Value::Bool(
                first == WATCHER_SERVICE
                    || first == NOTIFICATIONS
                    || self.shared.owners.lock().unwrap().contains_key(first),
            )]),
            (BUS_SERVICE, "GetNameOwner") => Some(vec![Value::str(":1.0")]),
            (BUS_SERVICE, "GetId") => Some(vec![Value::str("0123456789abcdef")]),
            (WATCHER_SERVICE, "RegisterStatusNotifierItem")
            | (WATCHER_SERVICE, "RegisterStatusNotifierHost") => Some(vec![]),
            (super::PROPERTIES, "Get")
                if call.path.as_deref() == Some("/StatusNotifierWatcher") =>
            {
                Some(vec![Value::variant(Value::Bool(true))])
            },
            (NOTIFICATIONS, "Notify") => {
                let replaces = call.body.get(1).and_then(Value::as_u32).unwrap_or(0);
                if replaces != 0 {
                    return Some(vec![Value::U32(replaces)]);
                }
                let mut next = self.shared.next_notification.lock().unwrap();
                *next += 1;
                Some(vec![Value::U32(*next)])
            },
            (NOTIFICATIONS, "CloseNotification") => Some(vec![]),
            (NOTIFICATIONS, "GetCapabilities") => Some(vec![Value::strings(["body", "actions"])]),
            (NOTIFICATIONS, "GetServerInformation") => Some(vec![
                Value::str("fake"),
                Value::str("zellij"),
                Value::str("1"),
                Value::str("1.2"),
            ]),
            _ => None,
        }
    }

    fn serve(&self, stream: UnixStream) {
        let Ok(mut reader) = stream.try_clone() else {
            return;
        };
        let writer = Arc::new(Mutex::new(stream));
        let unique = {
            let mut next = self.shared.next_connection.lock().unwrap();
            *next += 1;
            format!(":1.{}", *next)
        };
        let mut nul = [0u8; 1];
        if reader.read_exact(&mut nul).is_err() {
            return;
        }
        let mut byte = [0u8; 1];
        while reader.read_exact(&mut byte).is_ok() && byte[0] != b'\n' {}
        let _ = writer
            .lock()
            .unwrap()
            .write_all(b"OK 0123456789abcdef0123456789abcdef\r\n");
        while reader.read_exact(&mut byte).is_ok() && byte[0] != b'\n' {}
        self.shared.connections.lock().unwrap().push(Connection {
            unique: unique.clone(),
            writer: writer.clone(),
        });
        loop {
            let mut header = [0u8; 16];
            if reader.read_exact(&mut header).is_err() {
                break;
            }
            let Ok(total) = Message::needed(&header) else {
                break;
            };
            let mut bytes = header.to_vec();
            bytes.resize(total, 0);
            if reader.read_exact(&mut bytes[16..]).is_err() {
                break;
            }
            let Ok(mut message) = Message::decode(&bytes) else {
                break;
            };
            message.sender = Some(unique.clone());
            if matches!(message.kind, Kind::MethodReturn | Kind::Error) {
                let waiting = message
                    .reply_serial
                    .and_then(|serial| self.shared.pending.lock().unwrap().remove(&serial));
                if let Some(waiting) = waiting {
                    let _ = waiting.send(message);
                }
                continue;
            }
            if message.kind != Kind::MethodCall {
                continue;
            }
            self.shared.calls.lock().unwrap().push(message.clone());
            let custom = self
                .shared
                .answer
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|answer| answer(&message));
            let body = custom.or_else(|| self.built_in(&message, &unique));
            if message.is(BUS_SERVICE, "Hello") {
                let mut acquired = Message::signal(
                    super::BUS_PATH,
                    BUS_SERVICE,
                    "NameAcquired",
                    vec![Value::str(unique.clone())],
                );
                acquired.sender = Some(BUS_SERVICE.to_owned());
                acquired.destination = Some(unique.clone());
                self.write_to(&writer, &acquired);
            }
            if message.no_reply {
                continue;
            }
            let reply = match body {
                Some(body) => Message::reply_to(&message, body),
                None => Message::error_to(
                    &message,
                    "org.freedesktop.DBus.Error.UnknownMethod",
                    "unknown method",
                ),
            };
            self.write_to(&writer, &reply);
        }
        self.shared
            .connections
            .lock()
            .unwrap()
            .retain(|connection| connection.unique != unique);
        self.shared
            .owners
            .lock()
            .unwrap()
            .retain(|_, owner| *owner != unique);
    }
}

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

use super::{auth_line, bus_socket, Kind, Message, SocketAddress, Value, BUS_PATH, BUS_SERVICE};

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

    pub fn over(mut stream: UnixStream, make_handler: impl FnOnce(Bus) -> Handler) -> Result<Bus> {
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

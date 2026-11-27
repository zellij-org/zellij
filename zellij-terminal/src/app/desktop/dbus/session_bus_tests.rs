use super::fake::{FakeSessionBus, NOTIFICATIONS};
use super::*;
use std::os::unix::net::UnixStream;
use std::time::Duration;

#[test]
fn the_fake_session_bus_serves_every_connection_and_records_their_calls() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bus");
    let fake = FakeSessionBus::listen(&path);
    let first = Bus::over(UnixStream::connect(&path).unwrap(), |_| Box::new(|_| {})).unwrap();
    let second = Bus::over(UnixStream::connect(&path).unwrap(), |bus| {
        Box::new(move |message: Message| {
            if message.kind == Kind::MethodCall {
                let _ = bus.send(&Message::reply_to(&message, vec![Value::str("pong")]));
            }
        })
    })
    .unwrap();
    assert!(first.request_name("org.example.First").unwrap());
    assert!(second.request_name("org.example.Second").unwrap());
    let hellos = fake.calls_to(BUS_SERVICE, "Hello");
    assert_eq!(hellos.len(), 2);
    assert_ne!(hellos[0].sender, hellos[1].sender);
    assert_eq!(fake.connection_count(), 2);
    let reply = fake
        .call_client("org.example.Second", "/", "org.example.I", "Ping", vec![])
        .unwrap();
    assert_eq!(reply.body, vec![Value::str("pong")]);
    assert!(first.name_has_owner("org.kde.StatusNotifierWatcher"));
    let notified = first
        .call(
            Message::call(
                NOTIFICATIONS,
                "/org/freedesktop/Notifications",
                NOTIFICATIONS,
                "Notify",
                vec![Value::str("Zellij"), Value::U32(0)],
            ),
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(notified.body.first().and_then(Value::as_u32), Some(1));
    assert_eq!(fake.calls_to(NOTIFICATIONS, "Notify").len(), 1);
}

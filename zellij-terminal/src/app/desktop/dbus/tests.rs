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

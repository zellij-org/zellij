use crate::ipc::{
    ClientToServerMsg, IpcReceiveError, IpcReceiverWithContext, IpcSenderWithContext, IpcStream,
    ServerToClientMsg,
};
use crate::pane_size::Size;
use interprocess::local_socket::{prelude::*, ListenerOptions};

use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};

static IPC_COUNTER: AtomicU32 = AtomicU32::new(0);

struct FlushFailureStream;

impl Read for FlushFailureStream {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("read not supported"))
    }
}

impl Write for FlushFailureStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("flush failed"))
    }
}

impl IpcStream for FlushFailureStream {
    fn try_clone_stream(&self) -> std::io::Result<Box<dyn IpcStream>> {
        Err(std::io::Error::other("clone not supported"))
    }
    fn set_read_timeout(&self, _timeout: Option<std::time::Duration>) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn send_reports_flush_failure() {
    let mut sender =
        IpcSenderWithContext::<ClientToServerMsg>::from_boxed(Box::new(FlushFailureStream));

    assert!(
        sender
            .send_client_msg(ClientToServerMsg::ConnStatus)
            .is_err(),
        "a failed flush must not be reported as a successful send"
    );
}

#[test]
fn oversized_advertised_frames_are_rejected_before_allocation() {
    // The unread payload would otherwise be parsed as the next frames, so
    // the stream is treated as disconnected rather than skipped over.
    let advertised_len = (crate::ipc::MAX_FRAME_BYTES + 1) as u32;
    let mut bytes = advertised_len.to_le_bytes().to_vec();
    // Bytes that a reader attempting the allocation-and-read would consume.
    bytes.extend_from_slice(&[0u8; 64]);
    let mut frame = std::io::Cursor::new(bytes);

    assert!(matches!(
        crate::ipc::read_protobuf_message::<crate::ipc::ProtoClientToServerMsg>(&mut frame),
        Err(IpcReceiveError::Disconnected)
    ));
    // Without the limit check the reader would allocate the advertised
    // buffer and read (part of) the payload before failing with the same
    // error, so the error alone cannot show the limit was applied. The
    // reader must have stopped right after the length prefix.
    assert_eq!(
        frame.position(),
        4,
        "the oversized payload must not be read"
    );
}

#[test]
fn outgoing_frames_enforce_the_same_size_limit() {
    assert!(crate::ipc::validate_frame_length(crate::ipc::MAX_FRAME_BYTES).is_ok());
    assert!(crate::ipc::validate_frame_length(crate::ipc::MAX_FRAME_BYTES + 1).is_err());
}

// --- Cross-platform IPC helpers ---
// On Unix: use filesystem-based sockets (TempDir + PathBuf + GenericFilePath)
// On Windows: use named pipes (no guard needed, String name + GenericNamespaced)

#[cfg(unix)]
type IpcGuard = tempfile::TempDir;
#[cfg(unix)]
type IpcName = std::path::PathBuf;

#[cfg(windows)]
type IpcGuard = ();
#[cfg(windows)]
type IpcName = String;

#[cfg(unix)]
fn new_ipc() -> (IpcGuard, IpcName) {
    let dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let id = IPC_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = dir
        .path()
        .join(format!("test-{}-{}.sock", std::process::id(), id));
    (dir, path)
}

#[cfg(windows)]
fn new_ipc() -> (IpcGuard, IpcName) {
    let id = IPC_COUNTER.fetch_add(1, Ordering::SeqCst);
    let name = format!(
        "@zellij-test-{}-{}-{}",
        std::process::id(),
        id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    ((), name)
}

#[cfg(unix)]
fn bind_listener(name: &IpcName) -> interprocess::local_socket::Listener {
    use interprocess::local_socket::GenericFilePath;
    ListenerOptions::new()
        .name(name.as_path().to_fs_name::<GenericFilePath>().unwrap())
        .create_sync()
        .expect("bind failed")
}

#[cfg(windows)]
fn bind_listener(name: &IpcName) -> interprocess::local_socket::Listener {
    use interprocess::local_socket::GenericNamespaced;
    ListenerOptions::new()
        .name(name.as_str().to_ns_name::<GenericNamespaced>().unwrap())
        .create_sync()
        .expect("bind failed")
}

#[cfg(unix)]
fn connect_stream(name: &IpcName) -> interprocess::local_socket::Stream {
    use interprocess::local_socket::{GenericFilePath, Stream as LocalSocketStream};
    LocalSocketStream::connect(name.as_path().to_fs_name::<GenericFilePath>().unwrap())
        .expect("connect failed")
}

#[cfg(windows)]
fn connect_stream(name: &IpcName) -> interprocess::local_socket::Stream {
    use interprocess::local_socket::{GenericNamespaced, Stream as LocalSocketStream};
    LocalSocketStream::connect(name.as_str().to_ns_name::<GenericNamespaced>().unwrap())
        .expect("connect failed")
}

#[cfg(unix)]
fn try_connect(name: &IpcName) -> std::io::Result<interprocess::local_socket::Stream> {
    use interprocess::local_socket::{GenericFilePath, Stream as LocalSocketStream};
    LocalSocketStream::connect(name.as_path().to_fs_name::<GenericFilePath>().unwrap())
}

#[cfg(windows)]
fn try_connect(name: &IpcName) -> std::io::Result<interprocess::local_socket::Stream> {
    use interprocess::local_socket::{GenericNamespaced, Stream as LocalSocketStream};
    LocalSocketStream::connect(name.as_str().to_ns_name::<GenericNamespaced>().unwrap())
}

#[test]
fn client_to_server_message_over_socket() {
    let (_guard, name) = new_ipc();
    let listener = bind_listener(&name);

    let client = std::thread::spawn({
        let name = name.clone();
        move || {
            let stream = connect_stream(&name);
            let mut sender: IpcSenderWithContext<ClientToServerMsg> =
                IpcSenderWithContext::new(stream);
            sender
                .send_client_msg(ClientToServerMsg::ConnStatus)
                .expect("send failed");
        }
    });

    let stream = listener.incoming().next().unwrap().expect("accept failed");
    let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
        IpcReceiverWithContext::new(stream);

    let msg = receiver.recv_client_msg();
    assert!(msg.is_some(), "should receive a message");
    let (msg, _ctx) = msg.unwrap();
    assert!(
        matches!(msg, ClientToServerMsg::ConnStatus),
        "should be ConnStatus, got: {:?}",
        msg
    );

    client.join().expect("client thread panicked");
}

#[test]
fn server_to_client_message_over_socket() {
    let (_guard, name) = new_ipc();
    let listener = bind_listener(&name);

    let server = std::thread::spawn(move || {
        let stream = listener.incoming().next().unwrap().expect("accept failed");
        let mut sender: IpcSenderWithContext<ServerToClientMsg> = IpcSenderWithContext::new(stream);
        sender
            .send_server_msg(ServerToClientMsg::Connected)
            .expect("send failed");
    });

    let stream = connect_stream(&name);
    let mut receiver: IpcReceiverWithContext<ServerToClientMsg> =
        IpcReceiverWithContext::new(stream);

    let msg = receiver.recv_server_msg();
    assert!(msg.is_some(), "should receive a message");
    let (msg, _ctx) = msg.unwrap();
    assert!(
        matches!(msg, ServerToClientMsg::Connected),
        "should be Connected, got: {:?}",
        msg
    );

    server.join().expect("server thread panicked");
}

#[test]
fn bidirectional_communication_via_fd_duplication() {
    let (_guard, name) = new_ipc();
    let listener = bind_listener(&name);

    let server = std::thread::spawn(move || {
        let stream = listener.incoming().next().unwrap().expect("accept failed");
        let mut sender: IpcSenderWithContext<ServerToClientMsg> = IpcSenderWithContext::new(stream);

        let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
            sender.get_receiver().expect("clone receiver");
        sender
            .send_server_msg(ServerToClientMsg::Connected)
            .expect("send failed");

        let msg = receiver.recv_client_msg();
        assert!(msg.is_some(), "server should receive client message");
        let (msg, _) = msg.unwrap();
        assert!(
            matches!(msg, ClientToServerMsg::ConnStatus),
            "should be ConnStatus"
        );
    });

    let stream = connect_stream(&name);
    let mut sender: IpcSenderWithContext<ClientToServerMsg> = IpcSenderWithContext::new(stream);

    let mut receiver: IpcReceiverWithContext<ServerToClientMsg> =
        sender.get_receiver().expect("clone receiver");

    let msg = receiver.recv_server_msg();
    assert!(msg.is_some(), "client should receive server message");
    let (msg, _) = msg.unwrap();
    assert!(
        matches!(msg, ServerToClientMsg::Connected),
        "should be Connected"
    );

    sender
        .send_client_msg(ClientToServerMsg::ConnStatus)
        .expect("send failed");

    server.join().expect("server thread panicked");
}

#[test]
fn multiple_messages_in_sequence() {
    let (_guard, name) = new_ipc();
    let listener = bind_listener(&name);

    let client = std::thread::spawn({
        let name = name.clone();
        move || {
            let stream = connect_stream(&name);
            let mut sender: IpcSenderWithContext<ClientToServerMsg> =
                IpcSenderWithContext::new(stream);

            sender
                .send_client_msg(ClientToServerMsg::ConnStatus)
                .expect("send 1 failed");
            sender
                .send_client_msg(ClientToServerMsg::TerminalResize {
                    new_size: Size {
                        rows: 50,
                        cols: 120,
                    },
                })
                .expect("send 2 failed");
            sender
                .send_client_msg(ClientToServerMsg::KillSession)
                .expect("send 3 failed");
        }
    });

    let stream = listener.incoming().next().unwrap().expect("accept failed");
    let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
        IpcReceiverWithContext::new(stream);

    let (msg1, _) = receiver.recv_client_msg().expect("missing message 1");
    assert!(matches!(msg1, ClientToServerMsg::ConnStatus));

    let (msg2, _) = receiver.recv_client_msg().expect("missing message 2");
    match msg2 {
        ClientToServerMsg::TerminalResize { new_size } => {
            assert_eq!(new_size.rows, 50);
            assert_eq!(new_size.cols, 120);
        },
        other => panic!("expected TerminalResize, got: {:?}", other),
    }

    let (msg3, _) = receiver.recv_client_msg().expect("missing message 3");
    assert!(matches!(msg3, ClientToServerMsg::KillSession));

    client.join().expect("client thread panicked");
}

#[test]
fn receiver_returns_none_on_closed_connection() {
    let (_guard, name) = new_ipc();
    let listener = bind_listener(&name);

    let client = std::thread::spawn({
        let name = name.clone();
        move || {
            let stream = connect_stream(&name);
            let mut sender: IpcSenderWithContext<ClientToServerMsg> =
                IpcSenderWithContext::new(stream);
            sender
                .send_client_msg(ClientToServerMsg::ConnStatus)
                .expect("send failed");
        }
    });

    let stream = listener.incoming().next().unwrap().expect("accept failed");
    let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
        IpcReceiverWithContext::new(stream);

    client.join().expect("client thread panicked");

    let msg = receiver.recv_client_msg();
    assert!(msg.is_some(), "should receive the sent message");

    let msg = receiver.recv_client_msg();
    assert!(msg.is_none(), "should return None after connection closed");
}

#[test]
fn receiver_reports_disconnect_distinctly_from_undecodable_message() {
    let (_guard, name) = new_ipc();
    let listener = bind_listener(&name);

    let client = std::thread::spawn({
        let name = name.clone();
        move || {
            let stream = connect_stream(&name);
            let mut sender: IpcSenderWithContext<ClientToServerMsg> =
                IpcSenderWithContext::new(stream);
            sender
                .send_client_msg(ClientToServerMsg::ConnStatus)
                .expect("send failed");
        }
    });

    let stream = listener.incoming().next().unwrap().expect("accept failed");
    let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
        IpcReceiverWithContext::new(stream);

    client.join().expect("client thread panicked");

    assert!(
        receiver.try_recv_client_msg().is_ok(),
        "should receive the sent message"
    );

    assert!(
        matches!(
            receiver.try_recv_client_msg(),
            Err(IpcReceiveError::Disconnected)
        ),
        "a closed connection should be reported as a disconnect, not as an unreadable message"
    );
    assert!(
        matches!(
            receiver.try_recv_client_msg(),
            Err(IpcReceiveError::Disconnected)
        ),
        "a closed connection should keep reporting a disconnect"
    );
}

#[test]
fn receiver_recovers_from_an_undecodable_message() {
    let (_guard, name) = new_ipc();
    let listener = bind_listener(&name);

    let client = std::thread::spawn({
        let name = name.clone();
        move || {
            let mut stream = connect_stream(&name);
            stream
                .write_all(&0u32.to_le_bytes())
                .expect("write of empty frame failed");
            stream.flush().expect("flush failed");
            let mut sender: IpcSenderWithContext<ClientToServerMsg> =
                IpcSenderWithContext::new(stream);
            sender
                .send_client_msg(ClientToServerMsg::ConnStatus)
                .expect("send failed");
        }
    });

    let stream = listener.incoming().next().unwrap().expect("accept failed");
    let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
        IpcReceiverWithContext::new(stream);

    assert!(
        matches!(
            receiver.try_recv_client_msg(),
            Err(IpcReceiveError::Undecodable)
        ),
        "a frame carrying no known message should be reported as unreadable"
    );

    let (msg, _ctx) = match receiver.try_recv_client_msg() {
        Ok(received) => received,
        Err(e) => panic!("the connection should survive an unreadable message: {}", e),
    };
    assert!(
        matches!(msg, ClientToServerMsg::ConnStatus),
        "should be ConnStatus, got: {:?}",
        msg
    );

    client.join().expect("client thread panicked");
}

#[cfg(unix)]
#[test]
fn is_socket_identifies_bound_unix_socket() {
    use std::os::unix::fs::FileTypeExt;
    let (_guard, name) = new_ipc();
    let _listener = bind_listener(&name);

    let metadata = std::fs::metadata(&name).expect("metadata failed");
    assert!(
        metadata.file_type().is_socket(),
        "a bound LocalSocketListener path should be identified as a socket"
    );
}

#[cfg(unix)]
#[test]
fn is_socket_rejects_regular_file() {
    use std::os::unix::fs::FileTypeExt;
    let dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let file_path = dir.path().join("not_a_socket");
    std::fs::write(&file_path, b"regular file").expect("write failed");

    let metadata = std::fs::metadata(&file_path).expect("metadata failed");
    assert!(
        !metadata.file_type().is_socket(),
        "a regular file should NOT be identified as a socket"
    );
}

#[test]
fn session_probe_accepts_responding_socket() {
    let (_guard, name) = new_ipc();
    let listener = bind_listener(&name);

    let server = std::thread::spawn(move || {
        let stream = listener.incoming().next().unwrap().expect("accept failed");
        let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
            IpcReceiverWithContext::new(stream);
        let mut sender: IpcSenderWithContext<ServerToClientMsg> =
            receiver.get_sender().expect("clone sender");

        let msg = receiver.recv_client_msg();
        assert!(matches!(msg, Some((ClientToServerMsg::ConnStatus, _))));

        sender
            .send_server_msg(ServerToClientMsg::Connected)
            .expect("send failed");
    });

    let stream = connect_stream(&name);
    let mut sender: IpcSenderWithContext<ClientToServerMsg> = IpcSenderWithContext::new(stream);
    sender
        .send_client_msg(ClientToServerMsg::ConnStatus)
        .expect("send failed");
    let mut receiver: IpcReceiverWithContext<ServerToClientMsg> =
        sender.get_receiver().expect("clone receiver");

    let result = receiver.recv_server_msg();
    assert!(
        matches!(result, Some((ServerToClientMsg::Connected, _))),
        "probing a live session socket should return Connected"
    );

    server.join().expect("server thread panicked");
}

#[test]
fn session_probe_rejects_dead_socket() {
    let (_guard, name) = new_ipc();

    {
        let _listener = bind_listener(&name);
    }

    let result = try_connect(&name);
    assert!(
        result.is_err(),
        "connecting to a dead socket should fail (no listener)"
    );
}

#[cfg(unix)]
#[test]
fn socket_directory_enumeration_finds_sockets() {
    use interprocess::local_socket::GenericFilePath;
    use std::os::unix::fs::FileTypeExt;

    let dir = tempfile::TempDir::new().expect("failed to create temp dir");

    let sock_path = dir.path().join("test-session");
    let _listener = ListenerOptions::new()
        .name(sock_path.as_path().to_fs_name::<GenericFilePath>().unwrap())
        .create_sync()
        .expect("bind failed");

    let file_path = dir.path().join("not-a-session");
    std::fs::write(&file_path, b"data").expect("write failed");

    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .expect("read_dir failed")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().ok()?.is_socket() {
                entry.file_name().into_string().ok()
            } else {
                None
            }
        })
        .collect();

    assert_eq!(entries.len(), 1, "should find exactly one socket");
    assert_eq!(entries[0], "test-session");
}

use crate::ipc::{
    ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg,
};
use crate::pane_size::Size;
use interprocess::local_socket::{prelude::*, GenericFilePath, ListenerOptions, Stream as LocalSocketStream};
#[cfg(not(windows))]
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;
use tempfile::{tempdir, TempDir};

fn socket_path() -> (TempDir, PathBuf) {
    let dir = tempdir().expect("failed to create temp dir");
    let path = dir.path().join("test.sock");
    (dir, path)
}

#[test]
fn client_to_server_message_over_socket() {
    let (_dir, path) = socket_path();
    let listener = ListenerOptions::new().name(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).create_sync().expect("bind failed");

    let client = std::thread::spawn({
        let path = path.clone();
        move || {
            let stream = LocalSocketStream::connect(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).expect("connect failed");
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
    let (_dir, path) = socket_path();
    let listener = ListenerOptions::new().name(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).create_sync().expect("bind failed");

    let server = std::thread::spawn(move || {
        let stream = listener.incoming().next().unwrap().expect("accept failed");
        let mut sender: IpcSenderWithContext<ServerToClientMsg> =
            IpcSenderWithContext::new(stream);
        sender
            .send_server_msg(ServerToClientMsg::Connected)
            .expect("send failed");
    });

    let stream = LocalSocketStream::connect(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).expect("connect failed");
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
    let (_dir, path) = socket_path();
    let listener = ListenerOptions::new().name(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).create_sync().expect("bind failed");

    let server = std::thread::spawn(move || {
        let stream = listener.incoming().next().unwrap().expect("accept failed");
        let mut sender: IpcSenderWithContext<ServerToClientMsg> =
            IpcSenderWithContext::new(stream);

        // Create a receiver from the same socket via dup()
        let mut receiver: IpcReceiverWithContext<ClientToServerMsg> = sender.get_receiver();

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

    let stream = LocalSocketStream::connect(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).expect("connect failed");
    let mut sender: IpcSenderWithContext<ClientToServerMsg> = IpcSenderWithContext::new(stream);

    // Create a receiver from the same socket via dup()
    let mut receiver: IpcReceiverWithContext<ServerToClientMsg> = sender.get_receiver();

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
    let (_dir, path) = socket_path();
    let listener = ListenerOptions::new().name(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).create_sync().expect("bind failed");

    let client = std::thread::spawn({
        let path = path.clone();
        move || {
            let stream = LocalSocketStream::connect(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).expect("connect failed");
            let mut sender: IpcSenderWithContext<ClientToServerMsg> =
                IpcSenderWithContext::new(stream);

            sender
                .send_client_msg(ClientToServerMsg::ConnStatus)
                .expect("send 1 failed");
            sender
                .send_client_msg(ClientToServerMsg::TerminalResize {
                    new_size: Size { rows: 50, cols: 120 },
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
    let (_dir, path) = socket_path();
    let listener = ListenerOptions::new().name(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).create_sync().expect("bind failed");

    let client = std::thread::spawn({
        let path = path.clone();
        move || {
            let stream = LocalSocketStream::connect(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).expect("connect failed");
            let mut sender: IpcSenderWithContext<ClientToServerMsg> =
                IpcSenderWithContext::new(stream);
            sender
                .send_client_msg(ClientToServerMsg::ConnStatus)
                .expect("send failed");
            // sender drops here, closing the connection
        }
    });

    let stream = listener.incoming().next().unwrap().expect("accept failed");
    let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
        IpcReceiverWithContext::new(stream);

    client.join().expect("client thread panicked");

    let msg = receiver.recv_client_msg();
    assert!(msg.is_some(), "should receive the sent message");

    // After the sender is dropped, subsequent reads should return None
    let msg = receiver.recv_client_msg();
    assert!(msg.is_none(), "should return None after connection closed");
}

// --- Session discovery tests ---
// These test the OS-specific mechanics used by get_sessions() in sessions.rs:
// FileTypeExt::is_socket() for identifying socket files, and the assert_socket
// probing pattern (connect, send ConnStatus, expect Connected).

#[cfg(not(windows))]
#[test]
fn is_socket_identifies_bound_unix_socket() {
    let (_dir, path) = socket_path();
    let _listener = ListenerOptions::new().name(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).create_sync().expect("bind failed");

    let metadata = std::fs::metadata(&path).expect("metadata failed");
    assert!(
        metadata.file_type().is_socket(),
        "a bound LocalSocketListener path should be identified as a socket"
    );
}

#[cfg(not(windows))]
#[test]
fn is_socket_rejects_regular_file() {
    let dir = TempDir::new().expect("failed to create temp dir");
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
    // Simulates the assert_socket() pattern from sessions.rs:
    // A real Zellij server responds to ConnStatus with Connected.
    let (_dir, path) = socket_path();
    let listener = ListenerOptions::new().name(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).create_sync().expect("bind failed");

    // Spawn a fake "server" that responds to ConnStatus with Connected
    let server = std::thread::spawn(move || {
        let stream = listener.incoming().next().unwrap().expect("accept failed");
        let mut receiver: IpcReceiverWithContext<ClientToServerMsg> =
            IpcReceiverWithContext::new(stream);
        let mut sender: IpcSenderWithContext<ServerToClientMsg> = receiver.get_sender();

        let msg = receiver.recv_client_msg();
        assert!(matches!(
            msg,
            Some((ClientToServerMsg::ConnStatus, _))
        ));

        sender
            .send_server_msg(ServerToClientMsg::Connected)
            .expect("send failed");
    });

    // Client-side probing (mirrors assert_socket in sessions.rs)
    let stream = LocalSocketStream::connect(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).expect("connect failed");
    let mut sender: IpcSenderWithContext<ClientToServerMsg> = IpcSenderWithContext::new(stream);
    sender
        .send_client_msg(ClientToServerMsg::ConnStatus)
        .expect("send failed");
    let mut receiver: IpcReceiverWithContext<ServerToClientMsg> = sender.get_receiver();

    let result = receiver.recv_server_msg();
    assert!(
        matches!(result, Some((ServerToClientMsg::Connected, _))),
        "probing a live session socket should return Connected"
    );

    server.join().expect("server thread panicked");
}

#[test]
fn session_probe_rejects_dead_socket() {
    // Simulates discovering a stale socket file with no listener.
    // get_sessions() filters these out via assert_socket() which tries to connect.
    let (_dir, path) = socket_path();

    // Bind and immediately drop the listener to create a stale socket file
    {
        let _listener = ListenerOptions::new().name(path.as_path().to_fs_name::<GenericFilePath>().unwrap()).create_sync().expect("bind failed");
    }
    // Listener is dropped — the socket file may still exist but nobody is listening

    let result = LocalSocketStream::connect(path.as_path().to_fs_name::<GenericFilePath>().unwrap());
    assert!(
        result.is_err(),
        "connecting to a dead socket should fail (no listener)"
    );
}

#[cfg(not(windows))]
#[test]
fn socket_directory_enumeration_finds_sockets() {
    // Simulates the readdir + is_socket() filtering pattern from get_sessions().
    let dir = TempDir::new().expect("failed to create temp dir");

    // Create a socket
    let sock_path = dir.path().join("test-session");
    let _listener = ListenerOptions::new().name(sock_path.as_path().to_fs_name::<GenericFilePath>().unwrap()).create_sync().expect("bind failed");

    // Create a regular file (should be filtered out)
    let file_path = dir.path().join("not-a-session");
    std::fs::write(&file_path, b"data").expect("write failed");

    // Enumerate the directory, filtering for sockets (same pattern as get_sessions)
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

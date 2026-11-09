use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use interprocess::local_socket::traits::Listener;
use interprocess::local_socket::Listener as LocalListener;

use super::{assert_socket_path, observe_sessions_with, SessionProbeResult};
use crate::ipc::{
    ClientToServerMsg, ExitReason, IpcReceiverWithContext, IpcSenderWithContext, KillReply,
    ServerToClientMsg,
};

const BUDGET: Duration = Duration::from_millis(200);
/// Above this, a once-installed inactivity timeout would already have accepted
/// an 8-byte body trickled every 70ms (~560ms) or a 4-byte length prefix
/// trickled every 90ms plus another full body wait.
const DEADLINE_LIMIT: Duration = Duration::from_millis(450);

fn assert_honors_budget(elapsed: Duration) {
    assert!(
        elapsed < DEADLINE_LIMIT,
        "elapsed {elapsed:?} exceeded {DEADLINE_LIMIT:?}"
    );
}

fn assert_waited(elapsed: Duration) {
    assert!(
        elapsed >= Duration::from_millis(150),
        "returned too quickly to have been waiting on the deadline: {elapsed:?}"
    );
}

fn bind(dir: &Path, name: &str) -> (PathBuf, LocalListener) {
    let path = dir.join(name);
    let listener = crate::consts::ipc_bind(&path).expect("bind probe socket");
    (path, listener)
}

fn accept(listener: &LocalListener) -> interprocess::local_socket::Stream {
    listener.accept().expect("accept probe client")
}

fn hold_listener_until_peer_drops(listener: LocalListener) {
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    drop(release_tx);
    hold_until_peer_drops(listener, release_rx);
}

fn hold_until_peer_drops(listener: LocalListener, release: std::sync::mpsc::Receiver<()>) {
    use interprocess::local_socket::traits::Stream;
    let stream = accept(&listener);
    stream
        .set_recv_timeout(Some(Duration::from_secs(3)))
        .expect("server read timeout");
    let mut stream = stream;
    let mut buf = [0u8; 256];
    let _ = stream.read(&mut buf);
    let _ = stream.read(&mut buf);
    // Interprocess unlinks the socket when the listener drops. The assertion
    // below is about the probe, so keep the listener until that check runs.
    let _ = release.recv_timeout(Duration::from_secs(3));
    drop(listener);
}

fn trickle(mut stream: interprocess::local_socket::Stream, bytes: &[u8], interval: Duration) {
    for byte in bytes {
        thread::sleep(interval);
        if stream.write_all(&[*byte]).is_err() {
            return;
        }
    }
    let mut buf = [0u8; 1];
    use interprocess::local_socket::traits::Stream;
    let _ = stream.set_recv_timeout(Some(Duration::from_secs(2)));
    let _ = stream.read(&mut buf);
}

fn serve_server_msg(listener: LocalListener, msg: ServerToClientMsg) {
    let stream = accept(&listener);
    let mut receiver = IpcReceiverWithContext::<ClientToServerMsg>::new(stream);
    let _ = receiver.recv_client_msg();
    let mut sender: IpcSenderWithContext<ServerToClientMsg> =
        receiver.get_sender().expect("reply sender");
    sender.send_server_msg(msg).expect("send server msg");
    drop(listener);
}

#[test]
fn silent_peer_is_unknown_at_the_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let (path, listener) = bind(dir.path(), "silent");
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || hold_until_peer_drops(listener, release_rx));
    let started = Instant::now();
    let result = assert_socket_path(&path, BUDGET);
    let elapsed = started.elapsed();
    assert_eq!(result, SessionProbeResult::Unknown);
    assert_waited(elapsed);
    assert_honors_budget(elapsed);
    assert!(
        path.exists(),
        "a timed-out probe must not unlink the socket"
    );
    let _ = release_tx.send(());
    server.join().unwrap();
}

#[test]
fn trickled_length_prefix_cannot_extend_the_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let (path, listener) = bind(dir.path(), "prefix");
    // 4 length bytes, 90ms apart. An inactivity timer of 200ms accepts all
    // four (each gap is shorter than the timeout) and then waits out the body.
    let server = thread::spawn(move || {
        let stream = accept(&listener);
        trickle(stream, &8u32.to_le_bytes(), Duration::from_millis(90));
        drop(listener);
    });
    let started = Instant::now();
    let result = assert_socket_path(&path, BUDGET);
    let elapsed = started.elapsed();
    assert_eq!(result, SessionProbeResult::Unknown);
    assert_waited(elapsed);
    assert_honors_budget(elapsed);
    server.join().unwrap();
}

#[test]
fn trickled_body_after_a_small_length_cannot_extend_the_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let (path, listener) = bind(dir.path(), "body");
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        stream.write_all(&8u32.to_le_bytes()).expect("length");
        trickle(stream, &[0u8; 8], Duration::from_millis(70));
        drop(listener);
    });
    let started = Instant::now();
    let result = assert_socket_path(&path, BUDGET);
    let elapsed = started.elapsed();
    assert_eq!(result, SessionProbeResult::Unknown);
    assert_waited(elapsed);
    assert_honors_budget(elapsed);
    server.join().unwrap();
}

#[test]
fn huge_length_prefix_is_unknown_without_reading_the_payload() {
    let dir = tempfile::tempdir().unwrap();
    let (path, listener) = bind(dir.path(), "huge");
    let server = thread::spawn(move || {
        use interprocess::local_socket::traits::Stream;
        let mut stream = accept(&listener);
        let claimed = (32 * 1024 * 1024u32).to_le_bytes();
        let _ = stream.write_all(&claimed);
        stream
            .set_recv_timeout(Some(Duration::from_secs(3)))
            .expect("hold timeout");
        let mut buf = [0u8; 1];
        let _ = stream.read(&mut buf);
        drop(listener);
    });
    let started = Instant::now();
    let result = assert_socket_path(&path, Duration::from_secs(2));
    let elapsed = started.elapsed();
    assert_eq!(result, SessionProbeResult::Unknown);
    assert!(
        elapsed < Duration::from_millis(200),
        "huge length prefix took {elapsed:?}; the payload must not be allocated or read"
    );
    server.join().unwrap();
}

#[test]
fn unexpected_well_formed_response_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let (path, listener) = bind(dir.path(), "unexpected");
    let server = thread::spawn(move || {
        serve_server_msg(listener, ServerToClientMsg::UnblockInputThread);
    });
    let result = assert_socket_path(&path, Duration::from_secs(1));
    assert_eq!(result, SessionProbeResult::Unknown);
    server.join().unwrap();
}

#[test]
fn a_following_probe_against_a_healthy_peer_is_responsive() {
    let dir = tempfile::tempdir().unwrap();
    let (stale_path, stale_listener) = bind(dir.path(), "stale");
    let (live_path, live_listener) = bind(dir.path(), "live");
    let stale = thread::spawn(move || hold_listener_until_peer_drops(stale_listener));
    let live = thread::spawn(move || serve_server_msg(live_listener, ServerToClientMsg::Connected));

    let started = Instant::now();
    assert_eq!(
        assert_socket_path(&stale_path, BUDGET),
        SessionProbeResult::Unknown
    );
    assert_honors_budget(started.elapsed());
    assert_eq!(
        assert_socket_path(&live_path, Duration::from_secs(1)),
        SessionProbeResult::Responsive
    );
    stale.join().unwrap();
    live.join().unwrap();
}

#[test]
fn observe_sessions_with_returns_when_blocking_sockets_honor_the_budget() {
    let dir = tempfile::tempdir().unwrap();
    let mut servers = Vec::new();
    let mut candidates = Vec::new();
    for index in 0..4 {
        let name = format!("blocked-{index}");
        let (_path, listener) = bind(dir.path(), &name);
        servers.push(thread::spawn(move || hold_listener_until_peer_drops(listener)));
        candidates.push((name.clone(), name, ()));
    }
    let started = Instant::now();
    let observed = observe_sessions_with(candidates, BUDGET, |id, remaining| {
        assert_socket_path(&dir.path().join(id), remaining)
    });
    let elapsed = started.elapsed();
    assert!(observed.responsive.is_empty(), "{:?}", observed.responsive);
    assert_eq!(observed.unknown.len(), 4);
    assert!(
        elapsed < Duration::from_millis(700),
        "discovery did not return with the probe budget: {elapsed:?}"
    );
    for server in servers {
        server.join().unwrap();
    }
}

#[tokio::test]
async fn kill_exit_frame_is_shutdown_acknowledged() {
    let dir = tempfile::tempdir().unwrap();
    let (path, listener) = bind(dir.path(), "kill-ok");
    let server = thread::spawn(move || {
        let stream = accept(&listener);
        let mut receiver = IpcReceiverWithContext::<ClientToServerMsg>::new(stream);
        let (msg, _) = receiver.recv_client_msg().expect("KillSession");
        assert!(matches!(msg, ClientToServerMsg::KillSession));
        let mut sender: IpcSenderWithContext<ServerToClientMsg> =
            receiver.get_sender().expect("reply sender");
        sender
            .send_server_msg(ServerToClientMsg::Exit {
                exit_reason: ExitReason::Normal,
            })
            .expect("Exit");
        drop(listener);
    });
    let reply = crate::ipc::async_send_kill_and_await(&path, Duration::from_secs(2))
        .await
        .expect("kill request written");
    assert_eq!(reply, KillReply::ShutdownAcknowledged);
    server.join().unwrap();
}

#[tokio::test]
async fn kill_close_without_reply_is_not_acknowledged() {
    let dir = tempfile::tempdir().unwrap();
    let (path, listener) = bind(dir.path(), "kill-eof");
    let server = thread::spawn(move || {
        let stream = accept(&listener);
        let mut receiver = IpcReceiverWithContext::<ClientToServerMsg>::new(stream);
        let (msg, _) = receiver.recv_client_msg().expect("KillSession");
        assert!(matches!(msg, ClientToServerMsg::KillSession));
        drop(receiver);
        drop(listener);
    });
    let reply = crate::ipc::async_send_kill_and_await(&path, Duration::from_secs(2))
        .await
        .expect("kill request written");
    assert_eq!(reply, KillReply::OutcomeUnknown);
    server.join().unwrap();
}

#[tokio::test]
async fn kill_length_prefix_then_stall_is_not_acknowledged() {
    let dir = tempfile::tempdir().unwrap();
    let (path, listener) = bind(dir.path(), "kill-stall");
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        use interprocess::local_socket::traits::Stream;
        let mut stream = accept(&listener);
        stream
            .set_recv_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout");
        let mut buf = [0u8; 256];
        let _ = stream.read(&mut buf);
        stream.write_all(&8u32.to_le_bytes()).expect("prefix");
        let _ = release_rx.recv_timeout(Duration::from_secs(2));
        drop(listener);
    });
    let started = Instant::now();
    let reply = crate::ipc::async_send_kill_and_await(&path, BUDGET)
        .await
        .expect("kill request written");
    let elapsed = started.elapsed();
    assert_eq!(reply, KillReply::OutcomeUnknown);
    assert_honors_budget(elapsed);
    let _ = release_tx.send(());
    server.join().unwrap();
}

#[tokio::test]
async fn kill_non_exit_frame_is_not_acknowledged() {
    let dir = tempfile::tempdir().unwrap();
    let (path, listener) = bind(dir.path(), "kill-other");
    let server = thread::spawn(move || {
        let stream = accept(&listener);
        let mut receiver = IpcReceiverWithContext::<ClientToServerMsg>::new(stream);
        let _ = receiver.recv_client_msg().expect("KillSession");
        let mut sender: IpcSenderWithContext<ServerToClientMsg> =
            receiver.get_sender().expect("reply sender");
        sender
            .send_server_msg(ServerToClientMsg::UnblockInputThread)
            .expect("non-exit");
        drop(listener);
    });
    let reply = crate::ipc::async_send_kill_and_await(&path, Duration::from_secs(2))
        .await
        .expect("kill request written");
    assert_eq!(reply, KillReply::OutcomeUnknown);
    server.join().unwrap();
}

#[tokio::test]
async fn kill_missing_socket_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing");
    let err = crate::ipc::async_send_kill_and_await(&path, Duration::from_millis(200))
        .await
        .expect_err("missing peer must not look like a written kill");
    assert!(
        matches!(
            err.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
        ),
        "{err:?}"
    );
}

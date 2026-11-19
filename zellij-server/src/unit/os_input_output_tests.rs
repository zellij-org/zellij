use super::*;

fn make_server() -> ServerOsInputOutput {
    get_server_os_input().expect("failed to create server os input")
}

#[test]
fn get_cwd() {
    let server = make_server();

    let pid = std::process::id();
    assert!(
        server.get_cwd(pid).is_some(),
        "Get current working directory from PID {}",
        pid
    );
}

// --- Signal delivery tests ---

#[cfg(not(windows))]
#[test]
fn kill_sends_sighup_to_process() {
    let child = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("failed to spawn sleep");
    let pid = child.id();

    let server = make_server();

    server.kill(pid).expect("kill should succeed");

    // Give the signal time to be delivered
    std::thread::sleep(std::time::Duration::from_millis(100));
}

#[cfg(not(windows))]
#[test]
fn force_kill_sends_sigkill_to_process() {
    let child = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("failed to spawn sleep");
    let pid = child.id();

    let server = make_server();

    server.force_kill(pid).expect("force_kill should succeed");

    std::thread::sleep(std::time::Duration::from_millis(100));
}

#[cfg(not(windows))]
#[test]
fn send_sigint_to_process() {
    let child = Command::new("cat")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn cat");
    let pid = child.id();

    let server = make_server();

    server.send_sigint(pid).expect("send_sigint should succeed");

    std::thread::sleep(std::time::Duration::from_millis(100));
}

// zellient#22 — a client that is GONE must cost one log line, not one per buffered message.
//
// Measured on the live box 2026-09-28: 42,708 of 44,710 lines in
// /tmp/zellij-1000/zellij-log/zellij.log were
// `ERROR zellij_utils::ipc: Failed to flush ipc sender: Broken pipe (os error 32)` (250-720
// per minute), because the per-client forwarding thread below kept pulling messages out of
// its 5000-deep buffer after the client's socket had died, logging one line for each. The
// failures were immediate EPIPEs — the thread was never blocked, it just never stopped.
mod client_sender_dead_socket {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Once;
    use std::time::{Duration, Instant};

    /// How many log records mentioning the flush failure this process has emitted.
    static FLUSH_FAILURES: AtomicUsize = AtomicUsize::new(0);
    static LOGGER: Once = Once::new();

    struct FlushFailureCounter;

    impl log::Log for FlushFailureCounter {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Error
        }
        fn log(&self, record: &log::Record) {
            if record
                .args()
                .to_string()
                .contains("Failed to flush ipc sender")
            {
                FLUSH_FAILURES.fetch_add(1, Ordering::SeqCst);
            }
        }
        fn flush(&self) {}
    }

    fn install_counting_logger() {
        LOGGER.call_once(|| {
            log::set_boxed_logger(Box::new(FlushFailureCounter))
                .expect("installing the counting logger");
            log::set_max_level(log::LevelFilter::Error);
        });
    }

    /// A sender whose peer has already exited — exactly what a killed `zellij attach` leaves
    /// behind on the server side. The first write to it fails with EPIPE.
    fn sender_with_dead_peer() -> IpcSenderWithContext<ServerToClientMsg> {
        let (ours, theirs) = std::os::unix::net::UnixStream::pair()
            .expect("a unix socketpair for the dead-client fixture");
        drop(theirs); // the client is gone
        let stream = LocalSocketStream::from(
            interprocess::os::unix::uds_local_socket::Stream::from(ours),
        );
        IpcSenderWithContext::new(stream)
    }

    fn wait_for_the_first_failure(timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if FLUSH_FAILURES.load(Ordering::SeqCst) > 0 {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!(
            "the dead socket was never written to, so this test measured nothing: \
             the forwarding thread must report the failure at least once"
        );
    }

    #[test]
    fn a_dead_client_is_logged_once_not_once_per_buffered_message() {
        install_counting_logger();
        FLUSH_FAILURES.store(0, Ordering::SeqCst);

        let client_id = 42;
        let client_sender = ClientSender::new(client_id, sender_with_dead_peer());

        // 64 messages is the shape of the defect (a real queue holds up to 5000): the old
        // loop logged one line for each of them.
        const MESSAGES: usize = 64;
        for i in 0..MESSAGES {
            let _ = client_sender.send_or_buffer(ServerToClientMsg::Render {
                content: format!("msg {i}"),
            });
            std::thread::sleep(Duration::from_millis(2));
        }
        wait_for_the_first_failure(Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(200)); // let any further logging settle

        assert_eq!(
            FLUSH_FAILURES.load(Ordering::SeqCst),
            1,
            "a dead client must cost ONE log line, not one per buffered message ({MESSAGES} \
             were queued): the forwarding thread has to stop at the first send error"
        );

        // Stopping also drops the receiver, which is how the router learns to remove the
        // client: send_or_buffer must start failing instead of buffering into the void.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut refused = false;
        while Instant::now() < deadline && !refused {
            refused = client_sender
                .send_or_buffer(ServerToClientMsg::Render {
                    content: "after the failure".to_string(),
                })
                .is_err();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            refused,
            "the forwarding thread returned, so this client's buffer must be closed — \
             send_or_buffer kept accepting messages for a client that is gone"
        );
    }
}

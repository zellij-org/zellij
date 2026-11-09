use super::*;
use zellij_utils::input::command::RunCommand;

fn make_server() -> ServerOsInputOutput {
    get_server_os_input().expect("failed to create server os input")
}

// --- Cross-platform command helpers ---

#[allow(dead_code)]
#[cfg(not(windows))]
fn long_running_cmd() -> Command {
    let mut cmd = Command::new("sleep");
    cmd.arg("60");
    cmd
}

#[allow(dead_code)]
#[cfg(windows)]
fn long_running_cmd() -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("timeout");
    cmd.args(&["/T", "60"]);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    cmd
}

#[allow(dead_code)]
#[cfg(not(windows))]
fn echo_cmd(msg: &str) -> Command {
    let mut cmd = Command::new("echo");
    cmd.arg(msg);
    cmd
}

#[allow(dead_code)]
#[cfg(windows)]
fn echo_cmd(msg: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("cmd");
    cmd.args(&["/C", "echo", msg]);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    cmd
}

#[allow(dead_code)]
#[cfg(not(windows))]
fn stdin_reader_cmd() -> Command {
    let mut cmd = Command::new("cat");
    cmd.stdin(std::process::Stdio::piped());
    cmd
}

#[allow(dead_code)]
#[cfg(windows)]
fn stdin_reader_cmd() -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("findstr");
    cmd.arg("/R").arg(".*");
    cmd.stdin(std::process::Stdio::piped());
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    cmd
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
    let child = long_running_cmd()
        .spawn()
        .expect("failed to spawn long-running process");
    let pid = child.id();

    let server = make_server();

    server.kill(pid).expect("kill should succeed");

    // Give the signal time to be delivered
    std::thread::sleep(std::time::Duration::from_millis(100));
}

#[cfg(not(windows))]
#[test]
fn force_kill_sends_sigkill_to_process() {
    let child = long_running_cmd()
        .spawn()
        .expect("failed to spawn long-running process");
    let pid = child.id();

    let server = make_server();

    server.force_kill(pid).expect("force_kill should succeed");

    std::thread::sleep(std::time::Duration::from_millis(100));
}

#[cfg(not(windows))]
#[test]
fn send_sigint_to_process() {
    let child = stdin_reader_cmd()
        .spawn()
        .expect("failed to spawn stdin-reader process");
    let pid = child.id();

    let server = make_server();

    server.send_sigint(pid).expect("send_sigint should succeed");

    std::thread::sleep(std::time::Duration::from_millis(100));
}

#[test]
fn spawn_and_read_output() {
    use crate::panes::PaneId;
    use zellij_utils::input::command::TerminalAction;

    let server = make_server();
    let test_message = "hello_zellij_test";

    #[cfg(not(windows))]
    let cmd = RunCommand {
        command: PathBuf::from("echo"),
        args: vec![test_message.to_string()],
        ..Default::default()
    };
    #[cfg(windows)]
    let cmd = RunCommand {
        command: PathBuf::from("cmd"),
        args: vec![
            "/K".to_string(),
            "echo".to_string(),
            test_message.to_string(),
        ],
        ..Default::default()
    };

    let action = TerminalAction::RunCommand(cmd);
    let quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send> =
        Box::new(|_pane_id, _exit_status, _run_command| {});

    let (_terminal_id, mut reader, _child_pid) = server
        .spawn_terminal(action, quit_cb, None, &Default::default())
        .expect("spawn_terminal should succeed");

    // Read output from the spawned terminal
    let mut output = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        loop {
            if std::time::Instant::now() > deadline {
                break;
            }
            match tokio::time::timeout(
                std::time::Duration::from_millis(500),
                reader.read_chunk(4096),
            )
            .await
            {
                Ok(Ok(bytes)) if bytes.is_empty() => break,
                Ok(Ok(bytes)) => {
                    output.extend_from_slice(&bytes);
                    let s = String::from_utf8_lossy(&output);
                    if s.contains(test_message) {
                        break;
                    }
                },
                Ok(Err(_)) => break,
                Err(_) => {
                    // timeout — check if we already have enough
                    let s = String::from_utf8_lossy(&output);
                    if s.contains(test_message) {
                        break;
                    }
                },
            }
        }
    });

    let output_str = String::from_utf8_lossy(&output);
    assert!(
        output_str.contains(test_message),
        "expected output to contain '{}', got: '{}'",
        test_message,
        output_str
    );
}

// Run with this process's stdin redirected to NUL by the test below, the way a server started
// by the zellij-terminal launcher runs: a pane's shell must still read the pseudo console.
#[cfg(windows)]
#[test]
#[ignore = "run by a_pane_shell_reads_the_pseudo_console_even_when_the_server_input_is_redirected"]
fn an_interactive_shell_in_a_pane_waits_for_input() {
    use crate::panes::PaneId;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use zellij_utils::input::command::TerminalAction;

    let server = make_server();
    let exited = Arc::new(AtomicBool::new(false));
    let quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send> = Box::new({
        let exited = exited.clone();
        move |_pane_id, _exit_status, _run_command| exited.store(true, Ordering::SeqCst)
    });
    let cmd = RunCommand {
        command: PathBuf::from("cmd"),
        ..Default::default()
    };
    let (_terminal_id, _reader, child_pid) = server
        .spawn_terminal(
            TerminalAction::RunCommand(cmd),
            quit_cb,
            None,
            &Default::default(),
        )
        .expect("spawn_terminal should succeed");

    std::thread::sleep(std::time::Duration::from_secs(2));
    let still_running = !exited.load(Ordering::SeqCst);
    if let Some(child_pid) = child_pid {
        let _ = server.force_kill(child_pid);
    }
    assert!(
        still_running,
        "the shell exited at once: it read the server's own stdin instead of the pseudo console"
    );
}

#[cfg(windows)]
#[test]
fn a_pane_shell_reads_the_pseudo_console_even_when_the_server_input_is_redirected() {
    let module = module_path!()
        .split_once("::")
        .map_or(module_path!(), |(_crate, path)| path);
    let output = Command::new(std::env::current_exe().expect("the test binary has a path"))
        .args([
            "--exact",
            &format!("{}::an_interactive_shell_in_a_pane_waits_for_input", module),
            "--ignored",
            "--nocapture",
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("failed to run the test binary with its stdin on NUL");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn tcgetpgrp_returns_foreground_group() {
    use crate::panes::PaneId;
    use zellij_utils::input::command::TerminalAction;

    let server = make_server();

    // `login_tty` in the child makes the spawned command the controlling terminal's
    // foreground process group, so tcgetpgrp(master) should return its pid
    let cmd = RunCommand {
        command: PathBuf::from("sleep"),
        args: vec!["60".to_string()],
        ..Default::default()
    };
    let quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send> = Box::new(|_, _, _| {});
    let (terminal_id, _reader, child_pid) = server
        .spawn_terminal(
            TerminalAction::RunCommand(cmd),
            quit_cb,
            None,
            &Default::default(),
        )
        .expect("spawn_terminal should succeed");

    // poll (bounded to ~2s) for the child to finish setting up its controlling terminal
    let mut fpgid = None;
    for _ in 0..100 {
        fpgid = server.pty_backend.tcgetpgrp(terminal_id);
        let ready = match (fpgid, child_pid) {
            (Some(p), Some(child_pid)) => p > 0 && p as u32 == child_pid,
            (Some(p), None) => p > 0,
            _ => false,
        };
        if ready {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        matches!(fpgid, Some(p) if p > 0),
        "tcgetpgrp should return the foreground process group, got {:?}",
        fpgid
    );
    if let (Some(fpgid), Some(child_pid)) = (fpgid, child_pid) {
        assert_eq!(
            fpgid as u32, child_pid,
            "the spawned command leads its own foreground group"
        );
    }

    // An unknown terminal id has no fd and must not panic.
    assert_eq!(server.pty_backend.tcgetpgrp(u32::MAX), None);

    if let Some(child_pid) = child_pid {
        let _ = server.force_kill(child_pid);
    }
}

#[test]
fn client_buffer_refuses_messages_when_limit_is_reached() {
    let (buffer, receiver) = client_buffer(CLIENT_BUFFER_LIMIT, CLIENT_BUFFER_BYTE_LIMIT);
    let msg = || ServerToClientMsg::Exit {
        exit_reason: ExitReason::Normal,
    };
    for _ in 0..CLIENT_BUFFER_LIMIT {
        assert!(buffer.try_send(msg()).is_ok());
    }
    assert!(matches!(buffer.try_send(msg()), Err(TrySendError::Full(_))));
    assert!(matches!(buffer.try_send(msg()), Err(TrySendError::Full(_))));
    assert!(receiver.recv().is_some());
    assert!(buffer.try_send(msg()).is_ok());
    assert!(matches!(buffer.try_send(msg()), Err(TrySendError::Full(_))));
}

#[test]
fn client_buffer_refuses_payloads_over_the_byte_limit() {
    // A handful of large frames must trip the byte budget long before
    // the message-count cap — the queue retains payload bytes, not
    // just envelopes.
    let byte_limit = 1024;
    let (buffer, receiver) = client_buffer(CLIENT_BUFFER_LIMIT, byte_limit);
    let frame = |bytes: usize| ServerToClientMsg::RenderFrame {
        frame: vec![0u8; bytes],
    };
    assert!(buffer.try_send(frame(600)).is_ok());
    assert!(matches!(
        buffer.try_send(frame(600)),
        Err(TrySendError::Full(_))
    ));
    // A small payload still fits inside the remaining byte budget.
    assert!(buffer.try_send(frame(400)).is_ok());
    assert!(matches!(
        buffer.try_send(frame(100)),
        Err(TrySendError::Full(_))
    ));
    // Draining returns budget: after the receiver takes both frames,
    // another full frame is admitted.
    assert!(receiver.recv().is_some());
    assert!(receiver.recv().is_some());
    assert!(buffer.try_send(frame(1024)).is_ok());
}

#[test]
fn client_buffer_admits_an_oversized_message_into_an_empty_queue() {
    // A single message larger than the budget (e.g. a pane's initial
    // scrollback) must not disconnect a client that is keeping up.
    let byte_limit = 1024;
    let (buffer, receiver) = client_buffer(CLIENT_BUFFER_LIMIT, byte_limit);
    let frame = |bytes: usize| ServerToClientMsg::RenderFrame {
        frame: vec![0u8; bytes],
    };
    assert!(buffer.try_send(frame(4096)).is_ok());
    assert!(matches!(
        buffer.try_send(frame(1)),
        Err(TrySendError::Full(_))
    ));
    assert!(receiver.recv().is_some());
    assert!(buffer.try_send(frame(4096)).is_ok());
}

#[test]
fn client_buffer_reports_disconnect_when_receiver_is_gone() {
    let (buffer, receiver) = client_buffer(CLIENT_BUFFER_LIMIT, CLIENT_BUFFER_BYTE_LIMIT);
    drop(receiver);
    let result = buffer.try_send(ServerToClientMsg::Exit {
        exit_reason: ExitReason::Normal,
    });
    assert!(matches!(result, Err(TrySendError::Disconnected(_))));
    assert_eq!(buffer.queued.load(Ordering::Acquire), 0);
}

#[cfg(unix)]
#[test]
fn the_pane_environment_is_added_to_and_removed_from_the_child_environment() {
    use crate::os_input_output::PaneEnv;
    use crate::panes::PaneId;
    use zellij_utils::input::command::TerminalAction;

    let server = make_server();
    let cmd = RunCommand {
        command: PathBuf::from("sh"),
        args: vec![
            "-c".to_string(),
            "echo \"start:${ZELLIJ_PANE_ENV_TEST}:${HOME-unset}:end\"".to_string(),
        ],
        ..Default::default()
    };
    let mut pane_env = PaneEnv::new();
    pane_env.insert(
        "ZELLIJ_PANE_ENV_TEST".to_owned(),
        Some("from the config".to_owned()),
    );
    pane_env.insert("HOME".to_owned(), None);
    let quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send> = Box::new(|_, _, _| {});
    let (_terminal_id, mut reader, _child_pid) = server
        .spawn_terminal(TerminalAction::RunCommand(cmd), quit_cb, None, &pane_env)
        .expect("spawn_terminal should succeed");
    let expected = "start:from the config:unset:end";
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let output = rt.block_on(async {
        let mut output = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline
            && !String::from_utf8_lossy(&output).contains(expected)
        {
            match tokio::time::timeout(
                std::time::Duration::from_millis(500),
                reader.read_chunk(4096),
            )
            .await
            {
                Ok(Ok(bytes)) if bytes.is_empty() => break,
                Ok(Ok(bytes)) => output.extend_from_slice(&bytes),
                Ok(Err(_)) => break,
                Err(_) => {},
            }
        }
        String::from_utf8_lossy(&output).to_string()
    });
    assert!(output.contains(expected), "got: '{}'", output);
    assert!(std::env::var("ZELLIJ_PANE_ENV_TEST").is_err());
}

// --- Client writer ---

#[cfg(unix)]
mod client_writer {
    use super::*;
    use interprocess::os::unix::uds_local_socket::Stream as UdsStream;
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    fn local_socket(stream: UnixStream) -> LocalSocketStream {
        LocalSocketStream::UdSocket(UdsStream::from(stream))
    }

    /// A server side sender and the raw socket of the client at the other end.
    fn connected_pair() -> (IpcSenderWithContext<ServerToClientMsg>, UnixStream) {
        let (server_end, client_end) = UnixStream::pair().unwrap();
        (
            IpcSenderWithContext::new(local_socket(server_end)),
            client_end,
        )
    }

    fn big_message() -> ServerToClientMsg {
        ServerToClientMsg::RenderFrame {
            frame: vec![7u8; 64 * 1024],
        }
    }

    fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Without reading anything: once the server shut its end down, writing
    /// to it fails. Merely abandoning the queue would not show here before
    /// the client reads, since the writer is stuck holding the socket.
    fn assert_socket_was_shut_down_by_server(client_end: &mut UnixStream) {
        use std::io::{ErrorKind, Write};
        client_end.set_nonblocking(true).unwrap();
        wait_until("the server to shut the socket down", || {
            matches!(
                client_end.write(&[0u8]),
                Err(error) if matches!(
                    error.kind(),
                    ErrorKind::BrokenPipe | ErrorKind::ConnectionReset
                )
            )
        });
        client_end.set_nonblocking(false).unwrap();
    }

    /// Reads until the end of the stream, returning how many bytes arrived.
    fn read_to_end_of_stream(client_end: &mut UnixStream, timeout: Duration) -> usize {
        client_end.set_read_timeout(Some(timeout)).unwrap();
        let mut received = 0;
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            match client_end.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => received += read,
                // A reset also ends the stream.
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => break,
                Err(error) => panic!("the client never reached the end of its stream: {error}"),
            }
        }
        received
    }

    #[test]
    fn removing_a_client_that_is_not_reading_stops_its_writer_and_drops_its_queue() {
        let (sender, mut client_end) = connected_pair();
        let client_sender = ClientSender::new(1, sender);
        // Far more than the socket buffers hold, so the writer ends up
        // blocked in a write while the rest of the messages sit in its queue.
        let queued_messages = 300;
        for _ in 0..queued_messages {
            client_sender.send_or_buffer(big_message()).unwrap();
        }

        let removal = client_sender.remove(Duration::from_millis(200));

        // The client has not read a byte. The writer is blocked in a write
        // to it and can only get out because the socket is shut down.
        assert!(
            removal.join().unwrap(),
            "the writer of a removed client that is not reading never exited"
        );
        assert_socket_was_shut_down_by_server(&mut client_end);
        // The socket was shut down, so the client reaches the end of the
        // stream. What it can read is only what fitted in the socket
        // buffers, not the whole queue.
        let received = read_to_end_of_stream(&mut client_end, Duration::from_secs(5));
        assert!(
            received < queued_messages * 64 * 1024 / 2,
            "the queue of the removed client was still written to it ({received} bytes)"
        );
    }

    #[test]
    fn exit_notice_sent_just_before_removing_a_client_is_delivered() {
        let (sender, client_end) = connected_pair();
        let client_sender = ClientSender::new(1, sender);
        client_sender
            .send_or_buffer(ServerToClientMsg::Render {
                content: "last frame".to_owned(),
            })
            .unwrap();
        client_sender
            .send_or_buffer(ServerToClientMsg::Exit {
                exit_reason: ExitReason::Normal,
            })
            .unwrap();
        // The client has not read anything yet when the server removes it.
        let removal = client_sender.remove(Duration::from_secs(5));

        let mut receiver =
            IpcReceiverWithContext::<ServerToClientMsg>::new(local_socket(client_end));
        let mut received = Vec::new();
        while let Some((msg, _)) = receiver.recv_server_msg() {
            received.push(msg);
        }
        removal.join().unwrap();

        assert!(
            matches!(
                received.as_slice(),
                [
                    ServerToClientMsg::Render { .. },
                    ServerToClientMsg::Exit {
                        exit_reason: ExitReason::Normal
                    },
                    ..
                ]
            ),
            "the frame and the exit notice queued before the removal must reach the client, got {} messages",
            received.len()
        );
    }

    #[test]
    fn remove_client_stops_the_writer_of_a_client_that_is_not_reading() {
        let mut server = make_server();
        let (server_end, mut client_end) = UnixStream::pair().unwrap();
        server
            .register_client_with_reply(1, local_socket(server_end))
            .unwrap();
        let queued_messages = 300;
        for _ in 0..queued_messages {
            server.send_to_client(1, big_message()).unwrap();
        }

        server.remove_client(1).unwrap();

        // The client still does not read. Once the grace period for
        // flushing is over the queue is dropped and the socket shut down.
        std::thread::sleep(REMOVED_CLIENT_FLUSH_GRACE + Duration::from_secs(1));
        assert_socket_was_shut_down_by_server(&mut client_end);
        let received = read_to_end_of_stream(&mut client_end, Duration::from_secs(5));
        assert!(received < queued_messages * 64 * 1024 / 2);
    }
}

// --- Client byte budget accounting ---

mod payload_accounting {
    use super::*;
    use zellij_utils::attention::{AttentionItem, PaneAttention, SessionAttention};
    use zellij_utils::data::{
        ConnectToSession, PaneId as DataPaneId, SessionPreview, SessionPreviewPane,
        SessionPreviewTab,
    };
    use zellij_utils::input::terminal::TerminalSettings;
    use zellij_utils::ipc::{
        MobilePanePayload, MobileSessionPayload, MobileStatePayload, MobileTabPayload,
    };

    #[test]
    fn many_short_scrollback_lines_are_charged_for_their_string_headers() {
        let lines = 10_000;
        let msg = ServerToClientMsg::PaneRenderUpdate {
            pane_id: DataPaneId::Terminal(1),
            viewport: vec![],
            scrollback: Some(vec!["x".to_owned(); lines]),
            is_initial: true,
        };
        assert!(
            server_msg_payload_bytes(&msg) >= lines * (std::mem::size_of::<String>() + 1),
            "a Vec<String> costs its 24-byte headers, not only the text"
        );
    }

    #[test]
    fn session_preview_charges_tabs_and_pane_titles() {
        let title = "t".repeat(1024 * 1024);
        let msg = ServerToClientMsg::SessionPreview {
            preview: Some(SessionPreview {
                tabs: vec![SessionPreviewTab {
                    name: "tab".to_owned(),
                    active: true,
                    panes: vec![SessionPreviewPane {
                        id: 1,
                        title: title.clone(),
                        contents: Some("c".repeat(4096)),
                        ..Default::default()
                    }],
                }],
                error: Some("e".repeat(512)),
                ..Default::default()
            }),
        };
        assert!(server_msg_payload_bytes(&msg) >= title.len() + 4096 + 512);
    }

    #[test]
    fn mobile_state_charges_its_titles_names_and_sessions() {
        let msg = ServerToClientMsg::MobileState {
            payload: MobileStatePayload {
                session_name: "s".repeat(100),
                tabs: vec![MobileTabPayload {
                    position: 0,
                    name: "n".repeat(200),
                    active: true,
                }],
                panes: vec![MobilePanePayload {
                    tab_position: 0,
                    pane_id: 1,
                    is_plugin: false,
                    title: "p".repeat(1024 * 1024),
                    is_floating: false,
                    last_activity_secs_ago: 0,
                }],
                sessions: vec![MobileSessionPayload {
                    name: "m".repeat(300),
                    web_clients_allowed: false,
                    tab_count: 1,
                    pane_count: 1,
                    connected_clients: 0,
                    creation_secs_ago: 0,
                }],
                ..Default::default()
            },
        };
        assert!(server_msg_payload_bytes(&msg) >= 1024 * 1024 + 100 + 200 + 300);
    }

    /// Every variant that owns heap data must be charged for it. The
    /// accounting function is an exhaustive match, so a new variant cannot
    /// be added without deciding what it is charged; this makes sure that
    /// the decision is not "nothing" for one that carries heap data.
    #[test]
    fn every_message_carrying_heap_data_is_charged() {
        let big = || "z".repeat(10_000);
        let with_heap = vec![
            ServerToClientMsg::Render { content: big() },
            ServerToClientMsg::RenderFrame {
                frame: vec![0; 10_000],
            },
            ServerToClientMsg::Exit {
                exit_reason: ExitReason::Error(big()),
            },
            ServerToClientMsg::Log { lines: vec![big()] },
            ServerToClientMsg::LogError { lines: vec![big()] },
            ServerToClientMsg::SwitchSession {
                connect_to_session: ConnectToSession {
                    name: Some(big()),
                    cwd: Some(PathBuf::from(big())),
                    ..Default::default()
                },
            },
            ServerToClientMsg::UnblockCliPipeInput {
                pipe_name: big(),
                exit_code: None,
            },
            ServerToClientMsg::CliPipeOutput {
                pipe_name: big(),
                output: String::new(),
            },
            ServerToClientMsg::RenamedSession { name: big() },
            ServerToClientMsg::PaneRenderUpdate {
                pane_id: DataPaneId::Terminal(1),
                viewport: vec![big()],
                scrollback: None,
                is_initial: false,
            },
            ServerToClientMsg::ForwardQueryToHost {
                token: 1,
                query_bytes: vec![0; 10_000],
                resolve_async: false,
            },
            ServerToClientMsg::EmitNestedSessionFrame {
                payload_bytes: vec![0; 10_000],
            },
            ServerToClientMsg::MobileState {
                payload: MobileStatePayload {
                    session_name: big(),
                    ..Default::default()
                },
            },
            ServerToClientMsg::HostProgramStatus {
                status: Some(zellij_utils::attention::HostProgramStatus {
                    state: big(),
                    kind: None,
                    progress: None,
                    msg: String::new(),
                }),
            },
            ServerToClientMsg::SessionAttention {
                attention: SessionAttention {
                    items: vec![AttentionItem {
                        pane_id: DataPaneId::Terminal(1).into(),
                        tab_id: 0,
                        tab_name: big(),
                        pane_title: String::new(),
                        attention: PaneAttention::default(),
                    }],
                    ..Default::default()
                },
            },
            ServerToClientMsg::SessionPreview {
                preview: Some(SessionPreview {
                    session_name: big(),
                    ..Default::default()
                }),
            },
            ServerToClientMsg::PaneMirror {
                payload: vec![0; 10_000],
            },
        ];
        for msg in with_heap {
            assert!(
                server_msg_payload_bytes(&msg) >= 10_000,
                "{:?} carries heap data but is charged too little",
                std::mem::discriminant(&msg)
            );
        }
        // The terminal settings come from the server's own configuration:
        // they still cost at least their own size.
        assert!(
            server_msg_payload_bytes(&ServerToClientMsg::TerminalSettingsChanged {
                settings: Box::new(TerminalSettings::default()),
            }) >= std::mem::size_of::<TerminalSettings>()
        );
    }
}

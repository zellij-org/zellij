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
    let (buffer, receiver) = client_buffer(CLIENT_BUFFER_LIMIT);
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
fn client_buffer_reports_disconnect_when_receiver_is_gone() {
    let (buffer, receiver) = client_buffer(CLIENT_BUFFER_LIMIT);
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

#[cfg(unix)]
#[test]
fn command_env_overrides_the_pane_environment_but_not_zellij_pane_id() {
    use crate::os_input_output::PaneEnv;
    use crate::panes::PaneId;
    use std::collections::BTreeMap;
    use zellij_utils::input::command::TerminalAction;

    let server = make_server();
    let env: BTreeMap<String, String> = [
        ("ZELLIJ_TEST_FOO".to_owned(), "from the command".to_owned()),
        ("ZELLIJ_TEST_BAR".to_owned(), "bar baz".to_owned()),
        ("ZELLIJ_PANE_ID".to_owned(), "999999".to_owned()),
    ]
    .into();
    let cmd = RunCommand {
        command: PathBuf::from("sh"),
        args: vec![
            "-c".to_string(),
            "echo \"start:${ZELLIJ_TEST_FOO}:${ZELLIJ_TEST_BAR}:${ZELLIJ_PANE_ID}:end\""
                .to_string(),
        ],
        env,
        ..Default::default()
    };
    let mut pane_env = PaneEnv::new();
    pane_env.insert(
        "ZELLIJ_TEST_FOO".to_owned(),
        Some("from the config".to_owned()),
    );
    let quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send> = Box::new(|_, _, _| {});
    let (terminal_id, mut reader, _child_pid) = server
        .spawn_terminal(TerminalAction::RunCommand(cmd), quit_cb, None, &pane_env)
        .expect("spawn_terminal should succeed");
    let expected = format!("start:from the command:bar baz:{}:end", terminal_id);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let output = rt.block_on(async {
        let mut output = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline
            && !String::from_utf8_lossy(&output).contains(&expected)
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
    assert!(output.contains(&expected), "got: '{}'", output);
}

#[test]
fn pane_env_for_command_layers_the_command_env_on_top() {
    use crate::os_input_output::{pane_env_for_command, PaneEnv};

    let mut pane_env = PaneEnv::new();
    pane_env.insert("FROM_CONFIG".to_owned(), Some("config".to_owned()));
    pane_env.insert("OVERRIDDEN".to_owned(), Some("config".to_owned()));
    pane_env.insert("UNSET_BY_CONFIG".to_owned(), None);
    pane_env.insert("SET_AGAIN".to_owned(), None);
    let cmd = RunCommand {
        env: [
            ("OVERRIDDEN".to_owned(), "command".to_owned()),
            ("SET_AGAIN".to_owned(), "command".to_owned()),
            ("ZELLIJ_PANE_ID".to_owned(), "999".to_owned()),
        ]
        .into(),
        ..Default::default()
    };

    let mut expected = PaneEnv::new();
    expected.insert("FROM_CONFIG".to_owned(), Some("config".to_owned()));
    expected.insert("OVERRIDDEN".to_owned(), Some("command".to_owned()));
    expected.insert("UNSET_BY_CONFIG".to_owned(), None);
    expected.insert("SET_AGAIN".to_owned(), Some("command".to_owned()));
    assert_eq!(pane_env_for_command(&cmd, &pane_env), expected);
}

#[test]
fn pane_env_for_command_without_command_env_is_the_pane_env() {
    use crate::os_input_output::{pane_env_for_command, PaneEnv};

    let mut pane_env = PaneEnv::new();
    pane_env.insert("FROM_CONFIG".to_owned(), Some("config".to_owned()));
    assert_eq!(
        pane_env_for_command(&RunCommand::default(), &pane_env),
        pane_env
    );
}

#[test]
fn pane_env_for_command_matches_names_like_the_platform() {
    use crate::os_input_output::{pane_env_for_command, PaneEnv};

    let mut pane_env = PaneEnv::new();
    pane_env.insert("Path".to_owned(), Some("config".to_owned()));
    let cmd = RunCommand {
        env: [
            ("PATH".to_owned(), "command".to_owned()),
            ("zellij_pane_id".to_owned(), "999".to_owned()),
        ]
        .into(),
        ..Default::default()
    };

    let mut expected = PaneEnv::new();
    expected.insert("PATH".to_owned(), Some("command".to_owned()));
    if cfg!(not(windows)) {
        expected.insert("Path".to_owned(), Some("config".to_owned()));
        expected.insert("zellij_pane_id".to_owned(), Some("999".to_owned()));
    }
    assert_eq!(pane_env_for_command(&cmd, &pane_env), expected);
}

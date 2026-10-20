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
// by the zellij-window launcher runs: a pane's shell must still read the pseudo console.
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

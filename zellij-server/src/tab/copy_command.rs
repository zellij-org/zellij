use std::io::prelude::*;
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{bail, Context, Result};

/// Longest a helper may take to accept the copied text before it is killed.
const COPY_WRITE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct CopyCommand {
    command: String,
    args: Vec<String>,
}

impl CopyCommand {
    pub fn new(command: String) -> Self {
        let mut command_with_args = command.split(' ').map(String::from);

        Self {
            command: command_with_args.next().expect("missing command"),
            args: command_with_args.collect(),
        }
    }
    pub fn set(&self, value: String) -> Result<()> {
        self.set_with_write_timeout(value, COPY_WRITE_TIMEOUT)
    }

    /// Feed `value` to a new helper process. The calling thread owns the
    /// child: if the helper has not accepted all of its input within
    /// `write_timeout` (for example because it never reads stdin and the
    /// value exceeds the pipe capacity), the helper is killed, which also
    /// unblocks the writer, and an error is returned.
    fn set_with_write_timeout(&self, value: String, write_timeout: Duration) -> Result<()> {
        if self.command.is_empty() {
            bail!("copy_command is empty");
        }
        let mut process = Command::new(self.command.clone())
            .args(self.args.clone())
            .stdin(Stdio::piped())
            .spawn()
            .with_context(|| format!("couldn't spawn {}", self.command))?;
        let Some(mut stdin) = process.stdin.take() else {
            reap(process);
            bail!("could not get stdin");
        };
        let (done_sender, done_receiver) = std::sync::mpsc::channel();
        let writer = std::thread::Builder::new()
            .name("clipboard-writer".to_owned())
            .spawn(move || {
                // dropping `stdin` at the end closes the pipe, signalling EOF
                let _ = done_sender.send(stdin.write_all(value.as_bytes()));
            });
        if let Err(error) = writer {
            kill_and_wait(&mut process);
            return Err(error).context("couldn't start the clipboard writer thread");
        }
        match done_receiver.recv_timeout(write_timeout) {
            Ok(result) => {
                reap(process);
                result.with_context(|| format!("couldn't write to {} stdin", self.command))
            },
            Err(_) => {
                kill_and_wait(&mut process);
                bail!(
                    "{} did not read its input within {:?}; killed it",
                    self.command,
                    write_timeout
                )
            },
        }
    }
}

/// Kill a helper that stopped consuming its input. Closing its end of the
/// pipe makes the blocked writer thread fail and exit.
fn kill_and_wait(process: &mut Child) {
    if let Err(e) = process.kill() {
        log::error!("Failed to kill the clipboard helper: {}", e);
    }
    if let Err(e) = process.wait() {
        log::error!("Clipboard failure: {}", e);
    }
}

/// Wait for the copy command's process in the background, so that it does not linger as a zombie
/// once it exits.
///
/// The process is deliberately not killed if it outlives the copy operation: X11 and Wayland
/// clipboard helpers (eg. `xsel`, `xclip` or `wl-copy`) keep running for as long as they own the
/// selection, and killing them clears the clipboard. Copy commands that do exit by themselves (eg.
/// `pbcopy`) are reaped as soon as they do.
fn reap(mut process: Child) -> JoinHandle<()> {
    std::thread::spawn(move || {
        if let Err(e) = process.wait() {
            log::error!("Clipboard failure: {}", e);
        }
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use nix::errno::Errno;
    use nix::sys::wait::{waitpid, WaitPidFlag};
    use nix::unistd::Pid;
    use std::path::Path;
    use std::time::{Duration, Instant};

    // stands in for an x11/wayland clipboard helper: consumes stdin and then keeps running the way
    // `xsel` or `xclip` do while they own the selection
    fn fake_clipboard_helper(dir: &Path, received: &Path, still_alive: &Path) -> String {
        let script = dir.join("fake-clipboard-helper");
        std::fs::write(
            &script,
            format!(
                "cat > '{}'\nsleep 2\ntouch '{}'\n",
                received.display(),
                still_alive.display()
            ),
        )
        .unwrap();
        // run it through `sh` rather than making it executable, so that a concurrent fork/exec
        // elsewhere in the test binary can't make this spawn fail with ETXTBSY
        format!("sh {}", script.display())
    }

    #[test]
    fn an_empty_copy_command_is_an_error_not_a_panic() {
        let copy_command = CopyCommand::new("   ".to_owned());
        let error = copy_command.set("text".to_owned()).unwrap_err();

        assert!(
            error.to_string().contains("copy_command is empty"),
            "{error:?}"
        );
    }

    #[test]
    fn a_helper_that_closes_stdin_fails_and_is_reaped() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let script = dir.path().join("closes-stdin");
        std::fs::write(
            &script,
            format!("echo $$ > '{}'\nexec 0<&-\nexit 0\n", pid_file.display()),
        )
        .unwrap();
        let copy_command = CopyCommand::new(format!("sh {}", script.display()));
        // Larger than the pipe buffer, so the write cannot succeed before the
        // helper closes stdin.
        let selection = "x".repeat(1024 * 1024);

        let started = Instant::now();
        let result = copy_command.set(selection);

        assert!(
            result.is_err(),
            "a helper that closes stdin must fail the transfer"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "refusing stdin must not wait out the write timeout"
        );
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if nix::sys::signal::kill(Pid::from_raw(pid), None).err() == Some(Errno::ESRCH) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("a helper that closed stdin should be reaped, not left as a zombie");
    }

    #[test]
    fn a_helper_that_never_reads_its_input_is_killed_after_the_write_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let script = dir.path().join("never-reads");
        std::fs::write(
            &script,
            format!("echo $$ > '{}'\nexec sleep 30\n", pid_file.display()),
        )
        .unwrap();
        let copy_command = CopyCommand::new(format!("sh {}", script.display()));
        // larger than any pipe buffer, so the write cannot complete
        let selection = "x".repeat(4 * 1024 * 1024);

        let started = Instant::now();
        let result = copy_command.set_with_write_timeout(selection, Duration::from_secs(2));

        assert!(result.is_err());
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the writer must not block until the helper exits"
        );
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        // killed and reaped by the caller: no such process remains
        assert_eq!(
            nix::sys::signal::kill(Pid::from_raw(pid), None).err(),
            Some(Errno::ESRCH),
            "the unresponsive helper should have been killed"
        );
    }

    #[test]
    fn copy_command_is_not_killed_when_it_outlives_the_copy() {
        let dir = tempfile::tempdir().unwrap();
        let received = dir.path().join("received");
        let still_alive = dir.path().join("still-alive");
        let copy_command = fake_clipboard_helper(dir.path(), &received, &still_alive);

        CopyCommand::new(copy_command)
            .set("some copied text".to_owned())
            .unwrap();

        // poll (bounded to ~15s) for the helper to outlive the copy operation
        let deadline = Instant::now() + Duration::from_secs(15);
        while !still_alive.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }

        assert_eq!(
            std::fs::read_to_string(&received).unwrap(),
            "some copied text",
            "the copy command should be fed the copied text on its stdin"
        );
        assert!(
            still_alive.exists(),
            "the copy command should not be killed for outliving the copy operation"
        );
    }

    #[test]
    fn copy_command_is_reaped_once_it_exits() {
        let process = Command::new("sh")
            .args(["-c", "exit 0"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = Pid::from_raw(process.id() as i32);

        reap(process).join().unwrap();

        // having been waited for, the process is gone from the process table rather than left
        // behind as a zombie
        assert_eq!(
            waitpid(pid, Some(WaitPidFlag::WNOHANG)).err(),
            Some(Errno::ECHILD),
            "the copy command's process should have been reaped"
        );
    }
}

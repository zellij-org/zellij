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
        let mut command_with_args = split_command_line(&command)
            .filter(|parts| !parts.is_empty())
            .unwrap_or_else(|| command.split_ascii_whitespace().map(String::from).collect())
            .into_iter();

        Self {
            command: command_with_args.next().unwrap_or_default(),
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

/// Split a `copy_command` value into a program and its arguments.
///
/// ASCII whitespace separates words. Single or double quotes group a word that
/// contains whitespace and are removed; nothing is special inside single
/// quotes. A backslash escapes only whitespace or a quote character and is
/// literal everywhere else, so Windows paths such as
/// `C:\Windows\System32\clip.exe` and UNC paths survive intact (a POSIX
/// shell tokenizer would treat their backslashes as escapes). Returns `None`
/// for an unterminated quote, so the caller can fall back to plain
/// whitespace splitting.
fn split_command_line(command: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut characters = command.chars().peekable();
    while let Some(character) = characters.next() {
        let escapes_next = |next: Option<&char>, quote: Option<char>| match (next, quote) {
            (Some(next), None) => next.is_ascii_whitespace() || *next == '"' || *next == '\'',
            (Some('"'), Some('"')) => true,
            _ => false,
        };
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) if character == '\\' && escapes_next(characters.peek(), quote) => {
                current.extend(characters.next());
            },
            Some(_) => current.push(character),
            None if character == '\\' && escapes_next(characters.peek(), quote) => {
                current.extend(characters.next());
                in_word = true;
            },
            None if character == '"' || character == '\'' => {
                quote = Some(character);
                in_word = true;
            },
            None if character.is_ascii_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            },
            None => {
                current.push(character);
                in_word = true;
            },
        }
    }
    if quote.is_some() {
        return None;
    }
    if in_word {
        words.push(current);
    }
    Some(words)
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
    fn copy_command_preserves_quoted_and_empty_arguments() {
        let copy_command =
            CopyCommand::new("sh '/tmp/path with spaces' --flag '' trailing\\ value".to_owned());

        assert_eq!(copy_command.command, "sh");
        assert_eq!(
            copy_command.args,
            vec![
                "/tmp/path with spaces".to_owned(),
                "--flag".to_owned(),
                String::new(),
                "trailing value".to_owned(),
            ]
        );
    }

    #[test]
    fn copy_command_falls_back_for_malformed_quotes() {
        let copy_command = CopyCommand::new("helper 'unterminated".to_owned());

        assert_eq!(copy_command.command, "helper");
        assert_eq!(copy_command.args, vec!["'unterminated".to_owned()]);
    }

    #[test]
    fn copy_command_collapses_repeated_whitespace_without_creating_arguments() {
        let copy_command = CopyCommand::new("helper   --flag\t\tvalue".to_owned());

        assert_eq!(copy_command.command, "helper");
        assert_eq!(
            copy_command.args,
            vec!["--flag".to_owned(), "value".to_owned()]
        );
    }

    #[test]
    fn an_empty_copy_command_is_an_error_not_a_panic() {
        let copy_command = CopyCommand::new("   ".to_owned());

        assert!(copy_command.set("text".to_owned()).is_err());
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

#[cfg(test)]
mod split_tests {
    use super::{split_command_line, CopyCommand};

    fn split(command: &str) -> Vec<String> {
        let copy_command = CopyCommand::new(command.to_owned());
        std::iter::once(copy_command.command)
            .chain(copy_command.args)
            .collect()
    }

    #[test]
    fn only_ascii_whitespace_separates_words() {
        assert_eq!(split("copy\u{a0}tool --flag"), ["copy\u{a0}tool", "--flag"]);
        assert_eq!(split("xclip\t-sel\nclip"), ["xclip", "-sel", "clip"]);
    }

    #[test]
    fn windows_paths_keep_their_backslashes() {
        assert_eq!(
            split(r"C:\Windows\System32\clip.exe"),
            [r"C:\Windows\System32\clip.exe"]
        );
        assert_eq!(
            split(r#""C:\Program Files\Tool\copy.exe" /q"#),
            [r"C:\Program Files\Tool\copy.exe", "/q"]
        );
        assert_eq!(
            split(r#""\\server\share\clip.exe""#),
            [r"\\server\share\clip.exe"]
        );
        assert_eq!(
            split(r"\\server\share\clip.exe"),
            [r"\\server\share\clip.exe"]
        );
    }

    #[test]
    fn hash_is_an_ordinary_character() {
        assert_eq!(split("copy #tag --x"), ["copy", "#tag", "--x"]);
    }

    #[test]
    fn escapes_apply_to_whitespace_and_quotes_only() {
        assert_eq!(
            split(r#"say a\ b \"q\" "in \"quotes\"" 'single \ kept'"#),
            ["say", "a b", "\"q\"", "in \"quotes\"", r"single \ kept"]
        );
        assert_eq!(split_command_line(r#"x "open"#), None);
    }
}

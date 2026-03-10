use crate::os_input_output::{command_exists, AsyncReader};
use crate::panes::PaneId;

use nix::{
    fcntl::{fcntl, FcntlArg, OFlag},
    pty::{openpty, OpenptyResult, Winsize},
    sys::{
        signal::{kill, Signal},
        termios,
    },
    unistd,
};
use tokio::io::unix::AsyncFd;

use libc::{self, ioctl, TIOCSWINSZ};
use signal_hook;
use signal_hook::consts::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io,
    os::fd::FromRawFd,
    os::unix::{
        io::{AsRawFd, RawFd},
        process::CommandExt,
    },
    process::{Child, Command},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use zellij_utils::{errors::prelude::*, input::command::RunCommand};

pub use async_trait::async_trait;

/// An `AsyncReader` that wraps a `RawFd` using epoll via `AsyncFd`.
///
/// Construction sets O_NONBLOCK but defers `AsyncFd` registration to the first
/// `read()` call, because `AsyncFd::new()` requires a live Tokio reactor and
/// `spawn_terminal` runs on the plain PTY thread (outside the runtime).
struct RawFdAsyncReader {
    /// Holds the file before reactor registration; `None` after promotion.
    pending: Option<File>,
    /// Populated on first `read()` inside the Tokio runtime.
    async_fd: Option<AsyncFd<File>>,
}

impl RawFdAsyncReader {
    fn new(fd: RawFd) -> io::Result<Self> {
        // Set O_NONBLOCK so AsyncFd can use epoll correctly
        let flags =
            fcntl(fd, FcntlArg::F_GETFL).map_err(|e| io::Error::from_raw_os_error(e as i32))?;
        let mut oflags = OFlag::from_bits_truncate(flags);
        oflags.insert(OFlag::O_NONBLOCK);
        fcntl(fd, FcntlArg::F_SETFL(oflags)).map_err(|e| io::Error::from_raw_os_error(e as i32))?;

        let file = unsafe { File::from_raw_fd(fd) };
        Ok(Self {
            pending: Some(file),
            async_fd: None,
        })
    }

    /// Lazily register with the Tokio reactor on first use.
    fn get_async_fd(&mut self) -> io::Result<&mut AsyncFd<File>> {
        if self.async_fd.is_none() {
            let file = self
                .pending
                .take()
                .expect("RawFdAsyncReader used after init");
            self.async_fd = Some(AsyncFd::new(file)?);
        }
        Ok(self.async_fd.as_mut().unwrap())
    }
}

#[async_trait]
impl AsyncReader for RawFdAsyncReader {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        let async_fd = self.get_async_fd()?;
        loop {
            let mut guard = async_fd.readable().await?;
            match guard.try_io(|inner| {
                let fd = inner.get_ref().as_raw_fd();
                let ret =
                    unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
                if ret < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(ret as usize)
                }
            }) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }
}

fn set_terminal_size_using_fd(
    fd: RawFd,
    columns: u16,
    rows: u16,
    width_in_pixels: Option<u16>,
    height_in_pixels: Option<u16>,
) {
    // TODO: do this with the nix ioctl
    let ws_xpixel = width_in_pixels.unwrap_or(0);
    let ws_ypixel = height_in_pixels.unwrap_or(0);
    let winsize = Winsize {
        ws_col: columns,
        ws_row: rows,
        ws_xpixel,
        ws_ypixel,
    };
    // TIOCGWINSZ is an u32, but the second argument to ioctl is u64 on
    // some platforms. When checked on Linux, clippy will complain about
    // useless conversion.
    #[allow(clippy::useless_conversion)]
    unsafe {
        ioctl(fd, TIOCSWINSZ.into(), &winsize)
    };
}

/// Handle some signals for the child process. This will loop until the child
/// process exits.
fn handle_command_exit(mut child: Child) -> Result<Option<i32>> {
    let id = child.id();
    let err_context = || {
        format!(
            "failed to handle signals and command exit for child process pid {}",
            id
        )
    };

    // returns the exit status, if any
    let mut should_exit = false;
    let mut attempts = 3;
    let mut signals =
        signal_hook::iterator::Signals::new(&[SIGINT, SIGTERM]).with_context(err_context)?;
    'handle_exit: loop {
        // test whether the child process has exited
        match child.try_wait() {
            Ok(Some(status)) => {
                // if the child process has exited, break outside of the loop
                // and exit this function
                // TODO: handle errors?
                break 'handle_exit Ok(status.code());
            },
            Ok(None) => {
                thread::sleep(Duration::from_millis(10));
            },
            Err(e) => panic!("error attempting to wait: {}", e),
        }

        if !should_exit {
            for signal in signals.pending() {
                if signal == SIGINT || signal == SIGTERM {
                    should_exit = true;
                }
            }
        } else if attempts > 0 {
            // let's try nicely first...
            attempts -= 1;
            kill(
                unistd::Pid::from_raw(child.id() as i32),
                Some(Signal::SIGTERM),
            )
            .with_context(err_context)?;
            continue;
        } else {
            // when I say whoa, I mean WHOA!
            let _ = child.kill();
            break 'handle_exit Ok(None);
        }
    }
}

fn handle_openpty(
    open_pty_res: OpenptyResult,
    cmd: RunCommand,
    quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    terminal_id: u32,
) -> Result<(RawFd, RawFd)> {
    let err_context = |cmd: &RunCommand| {
        format!(
            "failed to open PTY for command '{}'",
            cmd.command.to_string_lossy().to_string()
        )
    };

    // primary side of pty and child fd
    let pid_primary = open_pty_res.master;
    let pid_secondary = open_pty_res.slave;

    if !command_exists(&cmd) {
        return Err(ZellijError::CommandNotFound {
            terminal_id,
            command: cmd.command.to_string_lossy().to_string(),
        })
        .with_context(|| err_context(&cmd));
    }

    let mut child = unsafe {
        let cmd = cmd.clone();
        let command = &mut Command::new(cmd.command);
        if let Some(current_dir) = cmd.cwd {
            if current_dir.exists() && current_dir.is_dir() {
                command.current_dir(current_dir);
            } else {
                log::error!(
                    "Failed to set CWD for new pane. '{}' does not exist or is not a folder",
                    current_dir.display()
                );
            }
        }
        command
            .args(&cmd.args)
            .env("ZELLIJ_PANE_ID", &format!("{}", terminal_id))
            .pre_exec(move || -> io::Result<()> {
                if libc::login_tty(pid_secondary) != 0 {
                    panic!("failed to set controlling terminal");
                }
                close_fds::close_open_fds(3, &[]);
                Ok(())
            })
            .spawn()
            .expect("failed to spawn")
    };

    let child_id = child.id();
    thread::spawn(move || {
        child.wait().with_context(|| err_context(&cmd)).fatal();
        let exit_status = handle_command_exit(child)
            .with_context(|| err_context(&cmd))
            .fatal();
        let _ = unistd::close(pid_secondary);
        quit_cb(PaneId::Terminal(terminal_id), exit_status, cmd);
    });

    Ok((pid_primary, child_id as RawFd))
}

/// Spawns a new terminal from the parent terminal with [`termios`](termios::Termios)
/// `orig_termios`.
fn handle_terminal(
    cmd: RunCommand,
    failover_cmd: Option<RunCommand>,
    orig_termios: Option<termios::Termios>,
    quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    terminal_id: u32,
) -> Result<(RawFd, RawFd)> {
    let err_context = || "failed to spawn child terminal".to_string();

    // Create a pipe to allow the child the communicate the shell's pid to its
    // parent.
    match openpty(None, &orig_termios) {
        Ok(open_pty_res) => handle_openpty(open_pty_res, cmd, quit_cb, terminal_id),
        Err(e) => match failover_cmd {
            Some(failover_cmd) => {
                handle_terminal(failover_cmd, None, orig_termios, quit_cb, terminal_id)
                    .with_context(err_context)
            },
            None => Err::<(i32, i32), _>(e)
                .context("failed to start pty")
                .with_context(err_context)
                .to_log(),
        },
    }
}

/// The Unix PTY backend. Manages native PTY file descriptors and signals.
#[derive(Clone)]
pub(crate) struct UnixPtyBackend {
    orig_termios: Arc<Mutex<Option<termios::Termios>>>,
    terminal_id_to_raw_fd: Arc<Mutex<BTreeMap<u32, Option<RawFd>>>>,
}

/// Default poll timeout for `write_all_to_fd` (5 seconds).
const WRITE_POLL_TIMEOUT_MS: i32 = 5_000;

/// Write all of `buf` to a raw fd, handling short writes, EINTR, and EAGAIN.
///
/// On EAGAIN, uses `poll(POLLOUT)` to wait for writability instead of busy-spinning.
/// If the fd stays unwritable for `poll_timeout_ms` consecutive milliseconds, returns
/// an error rather than silently truncating.
fn write_all_to_fd(fd: RawFd, buf: &[u8]) -> Result<usize> {
    write_all_to_fd_with_timeout(fd, buf, WRITE_POLL_TIMEOUT_MS)
}

fn write_all_to_fd_with_timeout(fd: RawFd, buf: &[u8], poll_timeout_ms: i32) -> Result<usize> {
    use nix::poll::{poll, PollFd, PollFlags};

    let mut written = 0;
    while written < buf.len() {
        match unistd::write(fd, &buf[written..]) {
            Ok(n) => written += n,
            Err(nix::errno::Errno::EINTR) => continue,
            Err(nix::errno::Errno::EAGAIN) => {
                let mut pfd = [PollFd::new(fd, PollFlags::POLLOUT)];
                match poll(&mut pfd, poll_timeout_ms) {
                    Ok(0) => {
                        // Timeout — fd never became writable
                        anyhow::bail!(
                            "timed out waiting for pty to become writable \
                             ({}/{} bytes written)",
                            written, buf.len()
                        );
                    },
                    Ok(_) => continue, // fd is writable, retry write
                    Err(nix::errno::Errno::EINTR) => continue,
                    Err(e) => return Err(e.into()),
                }
            },
            Err(e) => return Err(e.into()),
        }
    }
    Ok(written)
}

impl UnixPtyBackend {
    pub fn new() -> Result<Self, io::Error> {
        let current_termios = termios::tcgetattr(0).ok();
        if current_termios.is_none() {
            log::warn!("Starting a server without a controlling terminal, using the default termios configuration.");
        }
        Ok(Self {
            orig_termios: Arc::new(Mutex::new(current_termios)),
            terminal_id_to_raw_fd: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub fn spawn_terminal(
        &self,
        cmd: RunCommand,
        failover_cmd: Option<RunCommand>,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        terminal_id: u32,
    ) -> Result<(Box<dyn AsyncReader>, RawFd)> {
        let orig_termios = self
            .orig_termios
            .lock()
            .to_anyhow()
            .context("failed to lock orig_termios")?;
        let (pid_primary, child_fd) = handle_terminal(
            cmd,
            failover_cmd,
            orig_termios.clone(),
            quit_cb,
            terminal_id,
        )?;
        self.terminal_id_to_raw_fd
            .lock()
            .to_anyhow()?
            .insert(terminal_id, Some(pid_primary));
        let async_reader = Box::new(
            RawFdAsyncReader::new(pid_primary)
                .map_err(|e| anyhow::anyhow!("failed to create async reader: {}", e))?,
        ) as Box<dyn AsyncReader>;
        Ok((async_reader, child_fd))
    }

    pub fn set_terminal_size(
        &self,
        terminal_id: u32,
        cols: u16,
        rows: u16,
        width_in_pixels: Option<u16>,
        height_in_pixels: Option<u16>,
    ) -> Result<()> {
        let err_context = || {
            format!(
                "failed to set terminal id {} to size ({}, {})",
                terminal_id, rows, cols
            )
        };
        match self
            .terminal_id_to_raw_fd
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .get(&terminal_id)
        {
            Some(Some(fd)) => {
                if cols > 0 && rows > 0 {
                    set_terminal_size_using_fd(*fd, cols, rows, width_in_pixels, height_in_pixels);
                }
            },
            _ => {
                Err::<(), _>(anyhow!("failed to find terminal fd for id {terminal_id}"))
                    .with_context(err_context)
                    .non_fatal();
            },
        }
        Ok(())
    }

    pub fn write_to_tty_stdin(&self, terminal_id: u32, buf: &[u8]) -> Result<usize> {
        let err_context = || format!("failed to write to stdin of TTY ID {}", terminal_id);

        let fd = match self
            .terminal_id_to_raw_fd
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .get(&terminal_id)
        {
            Some(Some(fd)) => *fd,
            _ => return Err(anyhow!("could not find raw file descriptor")).with_context(err_context),
        };

        write_all_to_fd(fd, buf).with_context(err_context)
    }

    pub fn tcdrain(&self, terminal_id: u32) -> Result<()> {
        let err_context = || format!("failed to tcdrain to TTY ID {}", terminal_id);

        match self
            .terminal_id_to_raw_fd
            .lock()
            .to_anyhow()
            .with_context(err_context)?
            .get(&terminal_id)
        {
            Some(Some(fd)) => termios::tcdrain(*fd).with_context(err_context),
            _ => Err(anyhow!("could not find raw file descriptor")).with_context(err_context),
        }
    }

    pub fn kill(&self, pid: u32) -> Result<()> {
        let _ = kill(unistd::Pid::from_raw(pid as i32), Some(Signal::SIGHUP));
        Ok(())
    }

    pub fn force_kill(&self, pid: u32) -> Result<()> {
        let _ = kill(unistd::Pid::from_raw(pid as i32), Some(Signal::SIGKILL));
        Ok(())
    }

    pub fn send_sigint(&self, pid: u32) -> Result<()> {
        let _ = kill(unistd::Pid::from_raw(pid as i32), Some(Signal::SIGINT));
        Ok(())
    }

    pub fn reserve_terminal_id(&self, terminal_id: u32) {
        self.terminal_id_to_raw_fd
            .lock()
            .unwrap()
            .insert(terminal_id, None);
    }

    pub fn clear_terminal_id(&self, terminal_id: u32) {
        self.terminal_id_to_raw_fd
            .lock()
            .unwrap()
            .remove(&terminal_id);
    }

    pub fn next_terminal_id(&self) -> Option<u32> {
        self.terminal_id_to_raw_fd
            .lock()
            .unwrap()
            .keys()
            .copied()
            .collect::<BTreeSet<u32>>()
            .last()
            .map(|l| l + 1)
            .or(Some(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    use nix::sys::termios;
    use std::io::Read;

    /// Verify that `write_to_tty_stdin` writes ALL bytes even when the
    /// underlying fd returns short writes (EAGAIN / partial).
    ///
    /// The test sets the PTY master to O_NONBLOCK so that write() returns
    /// immediately with however many bytes fit in the kernel buffer (~4-16
    /// KiB on macOS). A reader thread starts after a short delay, draining
    /// the slave side. Without the write-retry loop, only the first
    /// buffer-full would be reported as written and the rest would be lost.
    #[test]
    fn write_to_tty_stdin_delivers_all_bytes() {
        let pty = openpty(None, &None).expect("openpty failed");

        // Raw mode on slave — prevent line discipline from transforming bytes
        let mut attrs = termios::tcgetattr(pty.slave).expect("tcgetattr failed");
        termios::cfmakeraw(&mut attrs);
        termios::tcsetattr(pty.slave, termios::SetArg::TCSANOW, &attrs)
            .expect("tcsetattr failed");

        // Set master to O_NONBLOCK so write() returns short / EAGAIN
        // instead of blocking, which is how the bug manifests in practice
        let flags = fcntl(pty.master, FcntlArg::F_GETFL).expect("F_GETFL");
        let mut oflags = OFlag::from_bits_truncate(flags);
        oflags.insert(OFlag::O_NONBLOCK);
        fcntl(pty.master, FcntlArg::F_SETFL(oflags)).expect("F_SETFL");

        let terminal_id = 99;
        let backend = UnixPtyBackend {
            orig_termios: Arc::new(Mutex::new(None)),
            terminal_id_to_raw_fd: Arc::new(Mutex::new(BTreeMap::from([(
                terminal_id,
                Some(pty.master),
            )]))),
        };

        // 128 KiB — well above the kernel PTY buffer
        let size = 128 * 1024;
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let expected = data.clone();

        // Reader drains the slave side concurrently. The writer still exercises
        // short writes because 128 KiB >> kernel PTY buffer (4-16 KiB on macOS).
        let slave_fd = pty.slave;
        let reader = std::thread::spawn(move || {
            let mut slave = unsafe { File::from_raw_fd(slave_fd) };
            let mut received = Vec::with_capacity(size);
            let mut buf = [0u8; 8192];
            while received.len() < size {
                match slave.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => received.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => panic!("slave read error: {e}"),
                }
            }
            received
        });

        let written = backend
            .write_to_tty_stdin(terminal_id, &data)
            .expect("write_to_tty_stdin failed");
        assert_eq!(written, size, "write_to_tty_stdin should report all bytes written");

        // Close master AFTER reader has drained all bytes — closing early can
        // race with kernel buffer flush on macOS, causing the slave to see EOF
        // before all data is delivered.
        let received = reader.join().expect("reader thread panicked");
        unsafe { libc::close(pty.master) };
        backend.terminal_id_to_raw_fd.lock().unwrap().remove(&terminal_id);
        assert_eq!(
            received.len(),
            expected.len(),
            "byte count mismatch: got {} expected {}",
            received.len(),
            expected.len()
        );
        assert!(received == expected, "content mismatch");
    }

    /// Verify that `write_all_to_fd` returns an error (not a silent partial
    /// write) when the PTY stays full and poll times out. Uses a short timeout
    /// to keep the test fast.
    #[test]
    fn write_all_to_fd_errors_on_stuck_pty() {
        let pty = openpty(None, &None).expect("openpty failed");

        let mut attrs = termios::tcgetattr(pty.slave).expect("tcgetattr failed");
        termios::cfmakeraw(&mut attrs);
        termios::tcsetattr(pty.slave, termios::SetArg::TCSANOW, &attrs)
            .expect("tcsetattr failed");

        // O_NONBLOCK so writes return EAGAIN instead of blocking
        let flags = fcntl(pty.master, FcntlArg::F_GETFL).expect("F_GETFL");
        let mut oflags = OFlag::from_bits_truncate(flags);
        oflags.insert(OFlag::O_NONBLOCK);
        fcntl(pty.master, FcntlArg::F_SETFL(oflags)).expect("F_SETFL");

        // 1 MiB — far more than the kernel PTY buffer (~4-16 KiB).
        // No reader on the slave side, so the buffer fills and stays full.
        let size = 1024 * 1024;
        let data: Vec<u8> = vec![0x42; size];

        // Use a 100ms timeout so the test finishes quickly
        let result = super::write_all_to_fd_with_timeout(pty.master, &data, 100);

        assert!(
            result.is_err(),
            "expected error on stuck PTY, got Ok({})",
            result.unwrap()
        );
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("timed out"),
            "expected timeout error, got: {err_msg}"
        );

        // Clean up fds
        unsafe {
            libc::close(pty.master);
            libc::close(pty.slave);
        }
    }
}
